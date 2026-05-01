use crate::{
    assets::{AssetStore, LocalFsAssetStore},
    auth,
    config::Config,
    sync_service,
};
use axum::{
    extract::{Json, Multipart, Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use snapline_domain::AssetUploadPayload;
use snapline_sync_client::protocol::{PullResponse, PushRequest, PushResponse, SnapshotResponse};
use sqlx::{PgPool, Row};
use std::sync::Arc;
use uuid::Uuid;

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub config: Config,
    pub asset_store: LocalFsAssetStore,
}

#[derive(Debug, Deserialize)]
pub struct RegisterRequest {
    pub email: String,
    pub password: String,
    pub device_id: String,
    pub device_name: String,
}

#[derive(Debug, Serialize)]
pub struct AuthResponse {
    pub account_id: String,
    pub access_token: String,
}

#[derive(Debug, Deserialize)]
pub struct PullQuery {
    pub cursor: i64,
}

pub async fn register(
    State(state): State<Arc<AppState>>,
    Json(request): Json<RegisterRequest>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    if !state.config.allow_registration {
        return Err((
            StatusCode::FORBIDDEN,
            "registration is disabled".to_string(),
        ));
    }
    create_account_and_device(state, request).await
}

pub async fn login(
    State(state): State<Arc<AppState>>,
    Json(request): Json<RegisterRequest>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let row = sqlx::query(
        "SELECT id, password_hash FROM accounts WHERE email = $1 AND disabled_at IS NULL",
    )
    .bind(&request.email)
    .fetch_optional(&state.pool)
    .await
    .map_err(internal_error)?
    .ok_or((StatusCode::UNAUTHORIZED, "invalid credentials".to_string()))?;
    let account_id: String = row.get("id");
    let password_hash: String = row.get("password_hash");
    if !auth::verify_password(&request.password, &password_hash).map_err(internal_error)? {
        return Err((StatusCode::UNAUTHORIZED, "invalid credentials".to_string()));
    }
    sqlx::query(
        "INSERT INTO devices (id, account_id, name) VALUES ($1, $2, $3)
         ON CONFLICT(id) DO UPDATE SET last_seen_at = now(), name = excluded.name",
    )
    .bind(&request.device_id)
    .bind(&account_id)
    .bind(&request.device_name)
    .execute(&state.pool)
    .await
    .map_err(internal_error)?;
    let token = auth::issue_token(&account_id, &state.config.jwt_secret).map_err(internal_error)?;
    Ok(Json(AuthResponse {
        account_id,
        access_token: token,
    }))
}

pub async fn push(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(request): Json<PushRequest>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let account_id = auth_account_id(&headers, &state)?;
    let mut tx = state.pool.begin().await.map_err(internal_error)?;
    let mut results = Vec::new();
    for change in request.changes {
        results.push(
            sync_service::apply_push_change(&mut tx, &account_id, &request.device_id, change)
                .await
                .map_err(internal_error)?,
        );
    }
    tx.commit().await.map_err(internal_error)?;
    Ok(Json(PushResponse { results }))
}

pub async fn pull(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(query): Query<PullQuery>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let account_id = auth_account_id(&headers, &state)?;
    let changes = sync_service::pull_changes(&state.pool, &account_id, query.cursor)
        .await
        .map_err(internal_error)?;
    let cursor = changes
        .last()
        .map(|change| change.cursor)
        .unwrap_or(query.cursor);
    Ok(Json(PullResponse { cursor, changes }))
}

pub async fn snapshot(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let account_id = auth_account_id(&headers, &state)?;
    let (cursor, notes, assets) = sync_service::snapshot(&state.pool, &account_id)
        .await
        .map_err(internal_error)?;
    Ok(Json(SnapshotResponse {
        cursor,
        notes,
        assets,
    }))
}

pub async fn upload_asset(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    mut multipart: Multipart,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let account_id = auth_account_id(&headers, &state)?;
    let mut payload: Option<AssetUploadPayload> = None;
    let mut file_bytes: Option<bytes::Bytes> = None;

    while let Some(field) = multipart.next_field().await.map_err(internal_error)? {
        match field.name() {
            Some("metadata") => {
                let text = field.text().await.map_err(internal_error)?;
                payload = Some(serde_json::from_str(&text).map_err(internal_error)?);
            }
            Some("file") => {
                file_bytes = Some(field.bytes().await.map_err(internal_error)?);
            }
            _ => {}
        }
    }

    let payload = payload.ok_or((StatusCode::BAD_REQUEST, "missing metadata".to_string()))?;
    let file_bytes = file_bytes.ok_or((StatusCode::BAD_REQUEST, "missing file".to_string()))?;
    validate_asset_upload(&payload.sha256, payload.byte_size, &file_bytes)?;
    let storage_key = format!(
        "accounts/{}/notes/{}/{}.png",
        account_id, payload.note_id, payload.asset_id
    );
    let row = sqlx::query_scalar::<_, String>(
        "INSERT INTO assets (id, account_id, note_id, content_type, byte_size, sha256, storage_key)
         VALUES ($1, $2, $3, $4, $5, $6, $7)
         ON CONFLICT(account_id, id) DO UPDATE SET
           note_id = assets.note_id
         WHERE assets.sha256 = excluded.sha256 AND assets.deleted_at IS NULL
         RETURNING storage_key",
    )
    .bind(payload.asset_id.to_string())
    .bind(&account_id)
    .bind(payload.note_id.to_string())
    .bind(payload.content_type)
    .bind(payload.byte_size)
    .bind(payload.sha256)
    .bind(storage_key)
    .fetch_optional(&state.pool)
    .await
    .map_err(internal_error)?;
    let Some(storage_key) = row else {
        return Err((
            StatusCode::CONFLICT,
            "asset id already exists with different sha256".to_string(),
        ));
    };
    state
        .asset_store
        .put(&storage_key, file_bytes)
        .await
        .map_err(internal_error)?;
    Ok(StatusCode::NO_CONTENT)
}

fn validate_asset_upload(
    expected_sha256: &str,
    expected_byte_size: i64,
    bytes: &Bytes,
) -> Result<(), (StatusCode, String)> {
    if bytes.len() as i64 != expected_byte_size {
        return Err((
            StatusCode::BAD_REQUEST,
            "asset byte size mismatch".to_string(),
        ));
    }
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let actual_sha256 = format!("{:x}", hasher.finalize());
    if actual_sha256 != expected_sha256 {
        return Err((StatusCode::BAD_REQUEST, "asset sha256 mismatch".to_string()));
    }
    Ok(())
}

pub async fn download_asset(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(asset_id): Path<String>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let account_id = auth_account_id(&headers, &state)?;
    let row = sqlx::query(
        "SELECT content_type, storage_key FROM assets
         WHERE account_id = $1 AND id = $2 AND deleted_at IS NULL",
    )
    .bind(account_id)
    .bind(asset_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(internal_error)?
    .ok_or((StatusCode::NOT_FOUND, "asset not found".to_string()))?;
    let content_type: String = row.get("content_type");
    let storage_key: String = row.get("storage_key");
    let bytes = state
        .asset_store
        .get(&storage_key)
        .await
        .map_err(internal_error)?;
    Ok(([(axum::http::header::CONTENT_TYPE, content_type)], bytes))
}

async fn create_account_and_device(
    state: Arc<AppState>,
    request: RegisterRequest,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let account_id = create_account(&state.pool, &request.email, &request.password)
        .await
        .map_err(internal_error)?;
    sqlx::query("INSERT INTO devices (id, account_id, name) VALUES ($1, $2, $3)")
        .bind(&request.device_id)
        .bind(&account_id)
        .bind(&request.device_name)
        .execute(&state.pool)
        .await
        .map_err(internal_error)?;
    let token = auth::issue_token(&account_id, &state.config.jwt_secret).map_err(internal_error)?;
    Ok(Json(AuthResponse {
        account_id,
        access_token: token,
    }))
}

pub async fn bootstrap_first_account(pool: &PgPool, config: &Config) -> anyhow::Result<()> {
    let count: i64 = sqlx::query("SELECT COUNT(*) AS count FROM accounts")
        .fetch_one(pool)
        .await?
        .get("count");
    if count > 0 {
        return Ok(());
    }
    let (Some(email), Some(password)) = (
        config.bootstrap_admin_email.as_deref(),
        config.bootstrap_admin_password.as_deref(),
    ) else {
        return Ok(());
    };
    create_account(pool, email, password).await?;
    Ok(())
}

async fn create_account(pool: &PgPool, email: &str, password: &str) -> anyhow::Result<String> {
    let account_id = Uuid::new_v4().to_string();
    let password_hash = auth::hash_password(password)?;
    sqlx::query("INSERT INTO accounts (id, email, password_hash) VALUES ($1, $2, $3)")
        .bind(&account_id)
        .bind(email)
        .bind(&password_hash)
        .execute(pool)
        .await?;
    Ok(account_id)
}

fn auth_account_id(headers: &HeaderMap, state: &AppState) -> Result<String, (StatusCode, String)> {
    let header = headers
        .get("authorization")
        .and_then(|value| value.to_str().ok())
        .ok_or((
            StatusCode::UNAUTHORIZED,
            "missing authorization".to_string(),
        ))?;
    let token = header.strip_prefix("Bearer ").ok_or((
        StatusCode::UNAUTHORIZED,
        "invalid authorization".to_string(),
    ))?;
    let claims = auth::verify_token(token, &state.config.jwt_secret)
        .map_err(|_| (StatusCode::UNAUTHORIZED, "invalid token".to_string()))?;
    Ok(claims.sub)
}

fn internal_error(err: impl std::fmt::Display) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, err.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_asset_upload_when_sha256_does_not_match_bytes() {
        let bytes = Bytes::from_static(b"actual");
        let err = validate_asset_upload("wrong", bytes.len() as i64, &bytes).unwrap_err();
        assert_eq!(err.0, StatusCode::BAD_REQUEST);
        assert_eq!(err.1, "asset sha256 mismatch");
    }

    #[test]
    fn accepts_asset_upload_when_size_and_sha256_match() {
        let bytes = Bytes::from_static(b"actual");
        let mut hasher = Sha256::new();
        hasher.update(&bytes);
        let sha256 = format!("{:x}", hasher.finalize());

        validate_asset_upload(&sha256, bytes.len() as i64, &bytes).unwrap();
    }
}
