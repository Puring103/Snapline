use std::path::{Path, PathBuf};

use axum::{
    Json,
    body::{Body, Bytes},
    extract::{Path as AxumPath, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use snapline_domain::{ATTACHMENT_PART_BYTES, MAX_ATTACHMENT_BYTES};
use sqlx::FromRow;
use tokio::{
    fs::{self, File},
    io::{AsyncReadExt, AsyncWriteExt},
};
use tokio_util::io::ReaderStream;
use uuid::Uuid;

use crate::{auth::authenticated, error::ApiError, state::AppState};

#[derive(Debug, Deserialize)]
pub struct CreateAttachmentRequest {
    pub id: Uuid,
    pub total_size: u64,
    pub part_size: u64,
    pub total_parts: u32,
    pub ciphertext_sha256: String,
    pub encrypted_metadata: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AttachmentResponse {
    pub id: Uuid,
    pub status: String,
    pub uploaded_parts: Vec<u32>,
}

#[derive(Debug, FromRow)]
struct AttachmentRow {
    total_size: i64,
    part_size: i64,
    total_parts: i32,
    ciphertext_sha256: String,
    status: String,
}

#[derive(Debug, FromRow)]
struct PartRow {
    part_number: i32,
    size: i64,
}

pub async fn create(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<CreateAttachmentRequest>,
) -> Result<Json<AttachmentResponse>, ApiError> {
    let claims = authenticated(&state, &headers).await?;
    validate_create(&request)?;
    sqlx::query(
        "INSERT INTO attachments \
         (id,user_id,device_id,total_size,part_size,total_parts,ciphertext_sha256,encrypted_metadata) \
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT (user_id,id) DO NOTHING",
    )
    .bind(request.id)
    .bind(claims.sub)
    .bind(claims.device_id)
    .bind(request.total_size as i64)
    .bind(request.part_size as i64)
    .bind(request.total_parts as i32)
    .bind(&request.ciphertext_sha256)
    .bind(&request.encrypted_metadata)
    .execute(&state.pool)
    .await
    .map_err(internal)?;
    status_response(&state, claims.sub, request.id)
        .await
        .map(Json)
}

pub async fn status(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(id): AxumPath<Uuid>,
) -> Result<Json<AttachmentResponse>, ApiError> {
    let claims = authenticated(&state, &headers).await?;
    status_response(&state, claims.sub, id).await.map(Json)
}

pub async fn upload_part(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath((id, part_number)): AxumPath<(Uuid, u32)>,
    body: Bytes,
) -> Result<StatusCode, ApiError> {
    let claims = authenticated(&state, &headers).await?;
    let attachment = get_attachment(&state, claims.sub, id).await?;
    if attachment.status != "uploading"
        || part_number >= attachment.total_parts as u32
        || body.is_empty()
        || body.len() as i64 > attachment.part_size
        || (part_number + 1 < attachment.total_parts as u32
            && body.len() as i64 != attachment.part_size)
    {
        return Err(ApiError::Validation);
    }
    let directory = parts_dir(&state.object_dir, claims.sub, id);
    fs::create_dir_all(&directory).await.map_err(internal)?;
    let final_path = directory.join(format!("{part_number}.part"));
    let temporary_path = directory.join(format!("{part_number}.tmp"));
    fs::write(&temporary_path, &body).await.map_err(internal)?;
    fs::rename(&temporary_path, &final_path)
        .await
        .map_err(internal)?;
    sqlx::query(
        "INSERT INTO attachment_parts (user_id,attachment_id,part_number,size,ciphertext_sha256) \
         VALUES ($1,$2,$3,$4,$5) ON CONFLICT (user_id,attachment_id,part_number) DO UPDATE SET \
         size=EXCLUDED.size,ciphertext_sha256=EXCLUDED.ciphertext_sha256,created_at=now()",
    )
    .bind(claims.sub)
    .bind(id)
    .bind(part_number as i32)
    .bind(body.len() as i64)
    .bind(checksum_bytes(&body))
    .execute(&state.pool)
    .await
    .map_err(internal)?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn complete(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(id): AxumPath<Uuid>,
) -> Result<Json<AttachmentResponse>, ApiError> {
    let claims = authenticated(&state, &headers).await?;
    let attachment = get_attachment(&state, claims.sub, id).await?;
    if attachment.status == "complete" {
        return status_response(&state, claims.sub, id).await.map(Json);
    }
    let parts = sqlx::query_as::<_, PartRow>(
        "SELECT part_number,size FROM attachment_parts \
         WHERE user_id=$1 AND attachment_id=$2 ORDER BY part_number",
    )
    .bind(claims.sub)
    .bind(id)
    .fetch_all(&state.pool)
    .await
    .map_err(internal)?;
    if parts.len() != attachment.total_parts as usize
        || parts.iter().map(|part| part.size).sum::<i64>() != attachment.total_size
        || parts
            .iter()
            .enumerate()
            .any(|(index, part)| part.part_number != index as i32)
    {
        return Err(ApiError::Validation);
    }

    let user_dir = state.object_dir.join(claims.sub.to_string());
    fs::create_dir_all(&user_dir).await.map_err(internal)?;
    let temporary_path = user_dir.join(format!("{id}.tmp"));
    let final_path = object_path(&state.object_dir, claims.sub, id);
    let mut output = File::create(&temporary_path).await.map_err(internal)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; 64 * 1024];
    for part in parts {
        let path =
            parts_dir(&state.object_dir, claims.sub, id).join(format!("{}.part", part.part_number));
        let mut input = File::open(path).await.map_err(internal)?;
        loop {
            let read = input.read(&mut buffer).await.map_err(internal)?;
            if read == 0 {
                break;
            }
            hasher.update(&buffer[..read]);
            output.write_all(&buffer[..read]).await.map_err(internal)?;
        }
    }
    output.flush().await.map_err(internal)?;
    drop(output);
    let actual = URL_SAFE_NO_PAD.encode(hasher.finalize());
    if actual != attachment.ciphertext_sha256 {
        let _ = fs::remove_file(&temporary_path).await;
        return Err(ApiError::Validation);
    }
    fs::rename(&temporary_path, &final_path)
        .await
        .map_err(internal)?;
    sqlx::query(
        "UPDATE attachments SET status='complete',completed_at=now() WHERE user_id=$1 AND id=$2",
    )
    .bind(claims.sub)
    .bind(id)
    .execute(&state.pool)
    .await
    .map_err(internal)?;
    let _ = fs::remove_dir_all(parts_dir(&state.object_dir, claims.sub, id)).await;
    status_response(&state, claims.sub, id).await.map(Json)
}

pub async fn download(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(id): AxumPath<Uuid>,
) -> Result<Response, ApiError> {
    let claims = authenticated(&state, &headers).await?;
    let attachment = get_attachment(&state, claims.sub, id).await?;
    if attachment.status != "complete" {
        return Err(ApiError::NotFound);
    }
    let file = File::open(object_path(&state.object_dir, claims.sub, id))
        .await
        .map_err(internal)?;
    let body = Body::from_stream(ReaderStream::new(file));
    Ok((
        [
            (header::CONTENT_TYPE, "application/octet-stream"),
            (
                header::CONTENT_DISPOSITION,
                "attachment; filename=encrypted.bin",
            ),
        ],
        body,
    )
        .into_response())
}

async fn get_attachment(
    state: &AppState,
    user_id: Uuid,
    id: Uuid,
) -> Result<AttachmentRow, ApiError> {
    sqlx::query_as::<_, AttachmentRow>(
        "SELECT total_size,part_size,total_parts,ciphertext_sha256,status \
         FROM attachments WHERE user_id=$1 AND id=$2",
    )
    .bind(user_id)
    .bind(id)
    .fetch_optional(&state.pool)
    .await
    .map_err(internal)?
    .ok_or(ApiError::NotFound)
}

async fn status_response(
    state: &AppState,
    user_id: Uuid,
    id: Uuid,
) -> Result<AttachmentResponse, ApiError> {
    let attachment = get_attachment(state, user_id, id).await?;
    let uploaded_parts = sqlx::query_scalar::<_, i32>(
        "SELECT part_number FROM attachment_parts \
         WHERE user_id=$1 AND attachment_id=$2 ORDER BY part_number",
    )
    .bind(user_id)
    .bind(id)
    .fetch_all(&state.pool)
    .await
    .map_err(internal)?
    .into_iter()
    .map(|value| value as u32)
    .collect();
    Ok(AttachmentResponse {
        id,
        status: attachment.status,
        uploaded_parts,
    })
}

fn validate_create(request: &CreateAttachmentRequest) -> Result<(), ApiError> {
    let expected_parts = request.total_size.div_ceil(request.part_size.max(1));
    if request.total_size == 0
        || request.total_size > MAX_ATTACHMENT_BYTES
        || request.part_size == 0
        || request.part_size > ATTACHMENT_PART_BYTES
        || request.total_parts == 0
        || u64::from(request.total_parts) != expected_parts
        || request.ciphertext_sha256.len() != 43
        || request.encrypted_metadata.is_empty()
        || request.encrypted_metadata.len() > 64 * 1024
    {
        return Err(ApiError::Validation);
    }
    Ok(())
}

fn parts_dir(root: &Path, user_id: Uuid, id: Uuid) -> PathBuf {
    root.join(user_id.to_string()).join(format!("{id}.parts"))
}

fn object_path(root: &Path, user_id: Uuid, id: Uuid) -> PathBuf {
    root.join(user_id.to_string()).join(format!("{id}.blob"))
}

fn checksum_bytes(bytes: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(bytes))
}

fn internal(error: impl std::fmt::Display) -> ApiError {
    tracing::error!(error = %error, "attachment operation failed");
    ApiError::Internal
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_part_count_from_size() {
        let request = CreateAttachmentRequest {
            id: Uuid::new_v4(),
            total_size: 9,
            part_size: 4,
            total_parts: 3,
            ciphertext_sha256: "a".repeat(43),
            encrypted_metadata: "cipher".into(),
        };
        assert!(validate_create(&request).is_ok());
    }
}
