use std::sync::Arc;

use argon2::{
    Argon2, PasswordHash, PasswordHasher, PasswordVerifier,
    password_hash::{SaltString, rand_core::OsRng},
};
use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, header},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, Duration, Utc};
use email_address::EmailAddress;
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, encode};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{FromRow, Postgres, Transaction};
use uuid::Uuid;

use crate::{error::ApiError, state::AppState};

const ISSUER: &str = "snapline";
const MIN_PASSWORD_LEN: usize = 10;
const MAX_PASSWORD_LEN: usize = 1024;

#[derive(Clone)]
pub struct TokenService {
    encoding: Arc<EncodingKey>,
    decoding: Arc<DecodingKey>,
    access_ttl_seconds: i64,
    refresh_ttl_seconds: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessClaims {
    pub sub: Uuid,
    pub device_id: Uuid,
    pub exp: usize,
    pub iat: usize,
    pub iss: String,
}

impl TokenService {
    pub fn new(secret: &[u8], access_ttl_seconds: i64, refresh_ttl_seconds: i64) -> Self {
        Self {
            encoding: Arc::new(EncodingKey::from_secret(secret)),
            decoding: Arc::new(DecodingKey::from_secret(secret)),
            access_ttl_seconds,
            refresh_ttl_seconds,
        }
    }

    fn issue_access(
        &self,
        user_id: Uuid,
        device_id: Uuid,
    ) -> Result<(String, DateTime<Utc>), ApiError> {
        let now = Utc::now();
        let expires_at = now + Duration::seconds(self.access_ttl_seconds);
        let claims = AccessClaims {
            sub: user_id,
            device_id,
            iat: now.timestamp() as usize,
            exp: expires_at.timestamp() as usize,
            iss: ISSUER.into(),
        };
        encode(&Header::default(), &claims, &self.encoding)
            .map(|token| (token, expires_at))
            .map_err(|_| ApiError::Internal)
    }

    pub fn decode_access(&self, token: &str) -> Result<AccessClaims, ApiError> {
        let mut validation = Validation::new(Algorithm::HS256);
        validation.set_issuer(&[ISSUER]);
        decode::<AccessClaims>(token, &self.decoding, &validation)
            .map(|data| data.claims)
            .map_err(|_| ApiError::Unauthorized)
    }

    fn new_refresh(&self) -> (String, String, DateTime<Utc>) {
        let mut bytes = [0_u8; 32];
        OsRng.fill_bytes(&mut bytes);
        let token = URL_SAFE_NO_PAD.encode(bytes);
        let hash = hash_refresh(&token);
        let expires_at = Utc::now() + Duration::seconds(self.refresh_ttl_seconds);
        (token, hash, expires_at)
    }
}

#[derive(Debug, Deserialize)]
pub struct RegisterRequest {
    pub email: String,
    pub password: String,
    pub device_name: String,
    pub platform: String,
    pub wrapped_master_key: String,
    pub recovery_blob: String,
}

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
    pub device_name: String,
    pub platform: String,
}

#[derive(Debug, Deserialize)]
pub struct RefreshRequest {
    pub refresh_token: String,
}

#[derive(Debug, Deserialize)]
pub struct LogoutRequest {
    pub refresh_token: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AuthResponse {
    pub user_id: Uuid,
    pub device_id: Uuid,
    pub access_token: String,
    pub access_expires_at: DateTime<Utc>,
    pub refresh_token: String,
    pub refresh_expires_at: DateTime<Utc>,
    pub wrapped_master_key: String,
    pub recovery_blob: String,
}

#[derive(Debug, Serialize, Deserialize, FromRow)]
pub struct DeviceResponse {
    pub id: Uuid,
    pub name: String,
    pub platform: String,
    pub created_at: DateTime<Utc>,
    pub last_seen_at: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,
}

#[derive(Debug, FromRow)]
struct UserRow {
    id: Uuid,
    password_hash: String,
    wrapped_master_key: String,
    recovery_blob: String,
}

#[derive(Debug, FromRow)]
struct SessionRow {
    id: Uuid,
    user_id: Uuid,
    device_id: Uuid,
    expires_at: DateTime<Utc>,
    revoked_at: Option<DateTime<Utc>>,
    device_revoked_at: Option<DateTime<Utc>>,
    wrapped_master_key: String,
    recovery_blob: String,
}

pub async fn register(
    State(state): State<AppState>,
    Json(request): Json<RegisterRequest>,
) -> Result<Json<AuthResponse>, ApiError> {
    validate_registration(&request)?;
    let email = request.email.trim().to_lowercase();
    let password_hash = hash_password(&request.password)?;
    let user_id = Uuid::new_v4();
    let device_id = Uuid::new_v4();
    let mut transaction = state.pool.begin().await.map_err(internal)?;

    let inserted = sqlx::query(
        "INSERT INTO users (id, email, password_hash, wrapped_master_key, recovery_blob) \
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(user_id)
    .bind(&email)
    .bind(password_hash)
    .bind(&request.wrapped_master_key)
    .bind(&request.recovery_blob)
    .execute(&mut *transaction)
    .await;
    if let Err(error) = inserted {
        return if is_unique_violation(&error) {
            Err(ApiError::Conflict)
        } else {
            Err(internal(error))
        };
    }

    insert_device(
        &mut transaction,
        device_id,
        user_id,
        &request.device_name,
        &request.platform,
    )
    .await?;
    let response = issue_session(
        &state.tokens,
        &mut transaction,
        user_id,
        device_id,
        request.wrapped_master_key,
        request.recovery_blob,
    )
    .await?;
    transaction.commit().await.map_err(internal)?;
    Ok(Json(response))
}

pub async fn login(
    State(state): State<AppState>,
    Json(request): Json<LoginRequest>,
) -> Result<Json<AuthResponse>, ApiError> {
    validate_email_password(&request.email, &request.password)?;
    validate_device(&request.device_name, &request.platform)?;
    let email = request.email.trim().to_lowercase();
    if !state.login_limiter.check(&email) {
        return Err(ApiError::RateLimited);
    }
    let user = sqlx::query_as::<_, UserRow>(
        "SELECT id, password_hash, wrapped_master_key, recovery_blob FROM users WHERE email = $1",
    )
    .bind(&email)
    .fetch_optional(&state.pool)
    .await
    .map_err(internal)?
    .ok_or_else(|| {
        state.login_limiter.failure(&email);
        ApiError::Unauthorized
    })?;
    if verify_password(&request.password, &user.password_hash).is_err() {
        state.login_limiter.failure(&email);
        return Err(ApiError::Unauthorized);
    }
    state.login_limiter.success(&email);

    let device_id = Uuid::new_v4();
    let mut transaction = state.pool.begin().await.map_err(internal)?;
    insert_device(
        &mut transaction,
        device_id,
        user.id,
        &request.device_name,
        &request.platform,
    )
    .await?;
    let response = issue_session(
        &state.tokens,
        &mut transaction,
        user.id,
        device_id,
        user.wrapped_master_key,
        user.recovery_blob,
    )
    .await?;
    transaction.commit().await.map_err(internal)?;
    Ok(Json(response))
}

pub async fn refresh(
    State(state): State<AppState>,
    Json(request): Json<RefreshRequest>,
) -> Result<Json<AuthResponse>, ApiError> {
    if request.refresh_token.len() > 256 || request.refresh_token.len() < 32 {
        return Err(ApiError::Unauthorized);
    }
    let hash = hash_refresh(&request.refresh_token);
    let mut transaction = state.pool.begin().await.map_err(internal)?;
    let session = sqlx::query_as::<_, SessionRow>(
        "SELECT s.id, s.user_id, s.device_id, s.expires_at, s.revoked_at, \
                d.revoked_at AS device_revoked_at, u.wrapped_master_key, u.recovery_blob \
         FROM sessions s JOIN devices d ON d.id = s.device_id JOIN users u ON u.id = s.user_id \
         WHERE s.refresh_token_hash = $1 FOR UPDATE OF s",
    )
    .bind(hash)
    .fetch_optional(&mut *transaction)
    .await
    .map_err(internal)?
    .ok_or(ApiError::Unauthorized)?;
    if session.revoked_at.is_some()
        || session.device_revoked_at.is_some()
        || session.expires_at <= Utc::now()
    {
        return Err(ApiError::Unauthorized);
    }

    let response = issue_session(
        &state.tokens,
        &mut transaction,
        session.user_id,
        session.device_id,
        session.wrapped_master_key,
        session.recovery_blob,
    )
    .await?;
    let new_session_id = session_id_for_refresh(&mut transaction, &response.refresh_token).await?;
    sqlx::query("UPDATE sessions SET revoked_at = now(), rotated_to = $1 WHERE id = $2")
        .bind(new_session_id)
        .bind(session.id)
        .execute(&mut *transaction)
        .await
        .map_err(internal)?;
    transaction.commit().await.map_err(internal)?;
    Ok(Json(response))
}

pub async fn logout(
    State(state): State<AppState>,
    Json(request): Json<LogoutRequest>,
) -> Result<(), ApiError> {
    sqlx::query("UPDATE sessions SET revoked_at = COALESCE(revoked_at, now()) WHERE refresh_token_hash = $1")
        .bind(hash_refresh(&request.refresh_token))
        .execute(&state.pool)
        .await
        .map_err(internal)?;
    Ok(())
}

pub async fn list_devices(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<DeviceResponse>>, ApiError> {
    let claims = authenticated(&state, &headers).await?;
    let devices = sqlx::query_as::<_, DeviceResponse>(
        "SELECT id, name, platform, created_at, last_seen_at, revoked_at \
         FROM devices WHERE user_id = $1 ORDER BY created_at DESC",
    )
    .bind(claims.sub)
    .fetch_all(&state.pool)
    .await
    .map_err(internal)?;
    Ok(Json(devices))
}

pub async fn revoke_device(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(device_id): Path<Uuid>,
) -> Result<(), ApiError> {
    let claims = authenticated(&state, &headers).await?;
    let mut transaction = state.pool.begin().await.map_err(internal)?;
    let affected = sqlx::query(
        "UPDATE devices SET revoked_at = COALESCE(revoked_at, now()) WHERE id = $1 AND user_id = $2",
    )
    .bind(device_id)
    .bind(claims.sub)
    .execute(&mut *transaction)
    .await
    .map_err(internal)?
    .rows_affected();
    if affected == 0 {
        return Err(ApiError::NotFound);
    }
    sqlx::query("UPDATE sessions SET revoked_at = COALESCE(revoked_at, now()) WHERE device_id = $1 AND user_id = $2")
        .bind(device_id)
        .bind(claims.sub)
        .execute(&mut *transaction)
        .await
        .map_err(internal)?;
    transaction.commit().await.map_err(internal)?;
    Ok(())
}

pub async fn authenticated(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<AccessClaims, ApiError> {
    let token = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .ok_or(ApiError::Unauthorized)?;
    let claims = state.tokens.decode_access(token)?;
    let active = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM devices WHERE id = $1 AND user_id = $2 AND revoked_at IS NULL)",
    )
    .bind(claims.device_id)
    .bind(claims.sub)
    .fetch_one(&state.pool)
    .await
    .map_err(internal)?;
    if !active {
        return Err(ApiError::Unauthorized);
    }
    Ok(claims)
}

async fn insert_device(
    transaction: &mut Transaction<'_, Postgres>,
    device_id: Uuid,
    user_id: Uuid,
    name: &str,
    platform: &str,
) -> Result<(), ApiError> {
    sqlx::query("INSERT INTO devices (id, user_id, name, platform) VALUES ($1, $2, $3, $4)")
        .bind(device_id)
        .bind(user_id)
        .bind(name.trim())
        .bind(platform.trim())
        .execute(&mut **transaction)
        .await
        .map_err(internal)?;
    Ok(())
}

async fn issue_session(
    tokens: &TokenService,
    transaction: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    device_id: Uuid,
    wrapped_master_key: String,
    recovery_blob: String,
) -> Result<AuthResponse, ApiError> {
    let (access_token, access_expires_at) = tokens.issue_access(user_id, device_id)?;
    let (refresh_token, refresh_hash, refresh_expires_at) = tokens.new_refresh();
    sqlx::query(
        "INSERT INTO sessions (id, user_id, device_id, refresh_token_hash, expires_at) \
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(Uuid::new_v4())
    .bind(user_id)
    .bind(device_id)
    .bind(refresh_hash)
    .bind(refresh_expires_at)
    .execute(&mut **transaction)
    .await
    .map_err(internal)?;
    Ok(AuthResponse {
        user_id,
        device_id,
        access_token,
        access_expires_at,
        refresh_token,
        refresh_expires_at,
        wrapped_master_key,
        recovery_blob,
    })
}

async fn session_id_for_refresh(
    transaction: &mut Transaction<'_, Postgres>,
    refresh_token: &str,
) -> Result<Uuid, ApiError> {
    sqlx::query_scalar("SELECT id FROM sessions WHERE refresh_token_hash = $1")
        .bind(hash_refresh(refresh_token))
        .fetch_one(&mut **transaction)
        .await
        .map_err(internal)
}

fn validate_registration(request: &RegisterRequest) -> Result<(), ApiError> {
    validate_email_password(&request.email, &request.password)?;
    validate_device(&request.device_name, &request.platform)?;
    if request.wrapped_master_key.is_empty()
        || request.wrapped_master_key.len() > 16_384
        || request.recovery_blob.is_empty()
        || request.recovery_blob.len() > 16_384
    {
        return Err(ApiError::Validation);
    }
    Ok(())
}

fn validate_email_password(email: &str, password: &str) -> Result<(), ApiError> {
    if email.len() > 254
        || !EmailAddress::is_valid(email.trim())
        || !(MIN_PASSWORD_LEN..=MAX_PASSWORD_LEN).contains(&password.len())
    {
        return Err(ApiError::Validation);
    }
    Ok(())
}

fn validate_device(name: &str, platform: &str) -> Result<(), ApiError> {
    if name.trim().is_empty()
        || name.len() > 100
        || platform.trim().is_empty()
        || platform.len() > 32
    {
        return Err(ApiError::Validation);
    }
    Ok(())
}

fn hash_password(password: &str) -> Result<String, ApiError> {
    Argon2::default()
        .hash_password(password.as_bytes(), &SaltString::generate(&mut OsRng))
        .map(|hash| hash.to_string())
        .map_err(|_| ApiError::Internal)
}

fn verify_password(password: &str, encoded: &str) -> Result<(), ApiError> {
    let hash = PasswordHash::new(encoded).map_err(|_| ApiError::Internal)?;
    Argon2::default()
        .verify_password(password.as_bytes(), &hash)
        .map_err(|_| ApiError::Unauthorized)
}

fn hash_refresh(token: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(token.as_bytes()))
}

fn internal(error: impl std::fmt::Display) -> ApiError {
    tracing::error!(error = %error, "database operation failed");
    ApiError::Internal
}

fn is_unique_violation(error: &sqlx::Error) -> bool {
    error
        .as_database_error()
        .and_then(|value| value.code())
        .as_deref()
        == Some("23505")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn password_hash_is_salted_and_verifiable() {
        let first = hash_password("correct horse battery staple").unwrap();
        let second = hash_password("correct horse battery staple").unwrap();
        assert_ne!(first, second);
        assert!(verify_password("correct horse battery staple", &first).is_ok());
        assert!(verify_password("wrong password", &first).is_err());
    }

    #[test]
    fn token_rejects_wrong_secret() {
        let service = TokenService::new(b"01234567890123456789012345678901", 60, 120);
        let other = TokenService::new(b"abcdefghijklmnopqrstuvwxyz123456", 60, 120);
        let (token, _) = service
            .issue_access(Uuid::new_v4(), Uuid::new_v4())
            .unwrap();
        assert!(service.decode_access(&token).is_ok());
        assert!(other.decode_access(&token).is_err());
    }

    #[test]
    fn registration_validation_rejects_empty_encryption_material() {
        let request = RegisterRequest {
            email: "person@example.com".into(),
            password: "a secure password".into(),
            device_name: "Desktop".into(),
            platform: "windows".into(),
            wrapped_master_key: String::new(),
            recovery_blob: "recovery".into(),
        };
        assert!(validate_registration(&request).is_err());
    }
}
