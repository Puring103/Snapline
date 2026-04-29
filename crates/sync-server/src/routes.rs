use crate::{auth, config::Config, sync_service};
use axum::{
    extract::{Json, Query, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use serde::{Deserialize, Serialize};
use snapline_sync_client::protocol::{PullResponse, PushRequest, PushResponse, SnapshotResponse};
use sqlx::{PgPool, Row};
use std::sync::Arc;
use uuid::Uuid;

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub config: Config,
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
        return Err((StatusCode::FORBIDDEN, "registration is disabled".to_string()));
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
    let (cursor, notes) = sync_service::snapshot(&state.pool, &account_id)
        .await
        .map_err(internal_error)?;
    Ok(Json(SnapshotResponse {
        cursor,
        notes,
        assets: Vec::new(),
    }))
}

async fn create_account_and_device(
    state: Arc<AppState>,
    request: RegisterRequest,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let account_id = Uuid::new_v4().to_string();
    let password_hash = auth::hash_password(&request.password).map_err(internal_error)?;
    let mut tx = state.pool.begin().await.map_err(internal_error)?;
    sqlx::query("INSERT INTO accounts (id, email, password_hash) VALUES ($1, $2, $3)")
        .bind(&account_id)
        .bind(&request.email)
        .bind(&password_hash)
        .execute(&mut *tx)
        .await
        .map_err(internal_error)?;
    sqlx::query("INSERT INTO devices (id, account_id, name) VALUES ($1, $2, $3)")
        .bind(&request.device_id)
        .bind(&account_id)
        .bind(&request.device_name)
        .execute(&mut *tx)
        .await
        .map_err(internal_error)?;
    tx.commit().await.map_err(internal_error)?;
    let token = auth::issue_token(&account_id, &state.config.jwt_secret).map_err(internal_error)?;
    Ok(Json(AuthResponse {
        account_id,
        access_token: token,
    }))
}

fn auth_account_id(headers: &HeaderMap, state: &AppState) -> Result<String, (StatusCode, String)> {
    let header = headers
        .get("authorization")
        .and_then(|value| value.to_str().ok())
        .ok_or((StatusCode::UNAUTHORIZED, "missing authorization".to_string()))?;
    let token = header
        .strip_prefix("Bearer ")
        .ok_or((StatusCode::UNAUTHORIZED, "invalid authorization".to_string()))?;
    let claims = auth::verify_token(token, &state.config.jwt_secret)
        .map_err(|_| (StatusCode::UNAUTHORIZED, "invalid token".to_string()))?;
    Ok(claims.sub)
}

fn internal_error(err: impl std::fmt::Display) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, err.to_string())
}
