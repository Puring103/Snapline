use axum::{
    Json,
    extract::{Query, State},
    http::HeaderMap,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use snapline_domain::{
    DEFAULT_SYNC_PAGE_SIZE, EncryptedEnvelope, MAX_CIPHERTEXT_BYTES, MAX_SYNC_PAGE_SIZE,
    SyncChange, SyncOperation,
};
use sqlx::{FromRow, Postgres, Transaction};
use uuid::Uuid;

use crate::{auth::authenticated, error::ApiError, state::AppState};

const MAX_CHANGES_PER_PUSH: usize = 100;
const MAX_ENCRYPTION_FIELD_BYTES: usize = 16_384;

#[derive(Debug, Deserialize)]
pub struct PushRequest {
    pub idempotency_key: Uuid,
    pub changes: Vec<EncryptedEnvelope>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AcceptedChange {
    pub object_id: Uuid,
    pub version: i64,
    pub cursor: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PushResponse {
    pub accepted: Vec<AcceptedChange>,
}

#[derive(Debug, Deserialize)]
pub struct PullQuery {
    #[serde(default)]
    pub cursor: i64,
    pub limit: Option<u32>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PullResponse {
    pub changes: Vec<SyncChange>,
    pub next_cursor: i64,
    pub has_more: bool,
}

#[derive(Debug, Deserialize)]
pub struct AckRequest {
    pub cursor: i64,
}

#[derive(Debug, FromRow)]
struct ChangeRow {
    cursor: i64,
    object_id: Uuid,
    object_type: String,
    version: i64,
    operation: String,
    ciphertext: String,
    nonce: String,
    wrapped_key: String,
    device_id: Uuid,
    client_updated_at: DateTime<Utc>,
    server_created_at: DateTime<Utc>,
}

pub async fn push(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<PushRequest>,
) -> Result<Json<PushResponse>, ApiError> {
    let claims = authenticated(&state, &headers).await?;
    if request.changes.is_empty() || request.changes.len() > MAX_CHANGES_PER_PUSH {
        return Err(ApiError::Validation);
    }
    for change in &request.changes {
        validate_change(change, claims.device_id)?;
    }

    let mut tx = state.pool.begin().await.map_err(internal)?;
    if let Some(existing) = sqlx::query_scalar::<_, serde_json::Value>(
        "SELECT response FROM sync_push_requests WHERE user_id = $1 AND idempotency_key = $2",
    )
    .bind(claims.sub)
    .bind(request.idempotency_key)
    .fetch_optional(&mut *tx)
    .await
    .map_err(internal)?
    {
        let response = serde_json::from_value(existing).map_err(|_| ApiError::Internal)?;
        tx.rollback().await.map_err(internal)?;
        return Ok(Json(response));
    }

    let mut accepted = Vec::with_capacity(request.changes.len());
    for change in &request.changes {
        accepted.push(apply_change(&mut tx, claims.sub, change).await?);
    }
    let response = PushResponse { accepted };
    sqlx::query(
        "INSERT INTO sync_push_requests (user_id, idempotency_key, response) VALUES ($1, $2, $3)",
    )
    .bind(claims.sub)
    .bind(request.idempotency_key)
    .bind(serde_json::to_value(&response).map_err(|_| ApiError::Internal)?)
    .execute(&mut *tx)
    .await
    .map_err(internal)?;
    tx.commit().await.map_err(internal)?;
    Ok(Json(response))
}

pub async fn pull(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<PullQuery>,
) -> Result<Json<PullResponse>, ApiError> {
    let claims = authenticated(&state, &headers).await?;
    if query.cursor < 0 {
        return Err(ApiError::Validation);
    }
    let limit = query.limit.unwrap_or(DEFAULT_SYNC_PAGE_SIZE);
    if limit == 0 || limit > MAX_SYNC_PAGE_SIZE {
        return Err(ApiError::Validation);
    }
    let rows = sqlx::query_as::<_, ChangeRow>(
        "SELECT cursor, object_id, object_type, version, operation, ciphertext, nonce, wrapped_key, \
                device_id, client_updated_at, server_created_at \
         FROM sync_changes WHERE user_id = $1 AND cursor > $2 ORDER BY cursor ASC LIMIT $3",
    )
    .bind(claims.sub)
    .bind(query.cursor)
    .bind(i64::from(limit) + 1)
    .fetch_all(&state.pool)
    .await
    .map_err(internal)?;
    let has_more = rows.len() > limit as usize;
    let changes: Vec<_> = rows
        .into_iter()
        .take(limit as usize)
        .map(row_to_change)
        .collect::<Result<_, _>>()?;
    let next_cursor = changes
        .last()
        .map(|change| change.cursor)
        .unwrap_or(query.cursor);
    Ok(Json(PullResponse {
        changes,
        next_cursor,
        has_more,
    }))
}

pub async fn ack(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<AckRequest>,
) -> Result<(), ApiError> {
    let claims = authenticated(&state, &headers).await?;
    if request.cursor < 0 {
        return Err(ApiError::Validation);
    }
    sqlx::query(
        "INSERT INTO sync_acks (user_id, device_id, cursor) VALUES ($1, $2, $3) \
         ON CONFLICT (user_id, device_id) DO UPDATE SET \
             cursor = GREATEST(sync_acks.cursor, EXCLUDED.cursor), updated_at = now()",
    )
    .bind(claims.sub)
    .bind(claims.device_id)
    .bind(request.cursor)
    .execute(&state.pool)
    .await
    .map_err(internal)?;
    Ok(())
}

async fn apply_change(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    change: &EncryptedEnvelope,
) -> Result<AcceptedChange, ApiError> {
    let current_version = sqlx::query_scalar::<_, i64>(
        "SELECT version FROM sync_objects WHERE user_id = $1 AND object_id = $2 FOR UPDATE",
    )
    .bind(user_id)
    .bind(change.object_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(internal)?
    .unwrap_or(0);
    if current_version != change.base_version {
        return Err(ApiError::Conflict);
    }
    let version = current_version + 1;
    let operation = operation_name(&change.operation);
    sqlx::query(
        "INSERT INTO sync_objects \
         (user_id, object_id, object_type, version, operation, ciphertext, nonce, wrapped_key, device_id, client_updated_at) \
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10) \
         ON CONFLICT (user_id, object_id) DO UPDATE SET \
           object_type=EXCLUDED.object_type, version=EXCLUDED.version, operation=EXCLUDED.operation, \
           ciphertext=EXCLUDED.ciphertext, nonce=EXCLUDED.nonce, wrapped_key=EXCLUDED.wrapped_key, \
           device_id=EXCLUDED.device_id, client_updated_at=EXCLUDED.client_updated_at, server_updated_at=now()",
    )
    .bind(user_id)
    .bind(change.object_id)
    .bind(&change.object_type)
    .bind(version)
    .bind(operation)
    .bind(&change.ciphertext)
    .bind(&change.nonce)
    .bind(&change.wrapped_key)
    .bind(change.device_id)
    .bind(change.client_updated_at)
    .execute(&mut **tx)
    .await
    .map_err(internal)?;
    let cursor = sqlx::query_scalar::<_, i64>(
        "INSERT INTO sync_changes \
         (user_id, object_id, object_type, version, operation, ciphertext, nonce, wrapped_key, device_id, client_updated_at) \
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10) RETURNING cursor",
    )
    .bind(user_id)
    .bind(change.object_id)
    .bind(&change.object_type)
    .bind(version)
    .bind(operation)
    .bind(&change.ciphertext)
    .bind(&change.nonce)
    .bind(&change.wrapped_key)
    .bind(change.device_id)
    .bind(change.client_updated_at)
    .fetch_one(&mut **tx)
    .await
    .map_err(internal)?;
    Ok(AcceptedChange {
        object_id: change.object_id,
        version,
        cursor,
    })
}

fn validate_change(change: &EncryptedEnvelope, authenticated_device: Uuid) -> Result<(), ApiError> {
    let object_type_valid = !change.object_type.is_empty()
        && change.object_type.len() <= 64
        && change
            .object_type
            .chars()
            .all(|c| c.is_ascii_lowercase() || c == '_');
    if change.device_id != authenticated_device
        || change.base_version < 0
        || !object_type_valid
        || change.ciphertext.is_empty()
        || change.ciphertext.len() > MAX_CIPHERTEXT_BYTES
        || change.nonce.is_empty()
        || change.nonce.len() > MAX_ENCRYPTION_FIELD_BYTES
        || change.wrapped_key.is_empty()
        || change.wrapped_key.len() > MAX_ENCRYPTION_FIELD_BYTES
    {
        return Err(ApiError::Validation);
    }
    Ok(())
}

fn row_to_change(row: ChangeRow) -> Result<SyncChange, ApiError> {
    let operation = match row.operation.as_str() {
        "upsert" => SyncOperation::Upsert,
        "delete" => SyncOperation::Delete,
        _ => return Err(ApiError::Internal),
    };
    Ok(SyncChange {
        cursor: row.cursor,
        version: row.version,
        envelope: EncryptedEnvelope {
            object_id: row.object_id,
            object_type: row.object_type,
            device_id: row.device_id,
            base_version: row.version - 1,
            operation,
            ciphertext: row.ciphertext,
            nonce: row.nonce,
            wrapped_key: row.wrapped_key,
            client_updated_at: row.client_updated_at,
        },
        server_created_at: row.server_created_at,
    })
}

fn operation_name(operation: &SyncOperation) -> &'static str {
    match operation {
        SyncOperation::Upsert => "upsert",
        SyncOperation::Delete => "delete",
    }
}

fn internal(error: impl std::fmt::Display) -> ApiError {
    tracing::error!(error = %error, "sync database operation failed");
    ApiError::Internal
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_change_from_another_device() {
        let change = EncryptedEnvelope {
            object_id: Uuid::new_v4(),
            object_type: "item".into(),
            device_id: Uuid::new_v4(),
            base_version: 0,
            operation: SyncOperation::Upsert,
            ciphertext: "cipher".into(),
            nonce: "nonce".into(),
            wrapped_key: "key".into(),
            client_updated_at: Utc::now(),
        };
        assert!(validate_change(&change, Uuid::new_v4()).is_err());
    }
}
