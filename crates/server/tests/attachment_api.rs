use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode, header},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{Duration, Utc};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use snapline_server::{
    app, attachments::cleanup_stale_uploads, auth::AuthResponse, config::Config, state::AppState,
};
use sqlx::postgres::PgPoolOptions;
use tempfile::TempDir;
use tower::ServiceExt;
use uuid::Uuid;

async fn call(
    app: Router,
    method: &str,
    uri: &str,
    body: Body,
    token: Option<&str>,
    json_body: bool,
) -> (StatusCode, Vec<u8>) {
    let mut builder = Request::builder().method(method).uri(uri);
    if json_body {
        builder = builder.header(header::CONTENT_TYPE, "application/json");
    }
    if let Some(token) = token {
        builder = builder.header(header::AUTHORIZATION, format!("Bearer {token}"));
    }
    let response = app.oneshot(builder.body(body).unwrap()).await.unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 16 * 1024 * 1024)
        .await
        .unwrap();
    (status, bytes.to_vec())
}

async fn register(app: Router, email: &str) -> AuthResponse {
    let body = json!({
        "email":email,
        "password":"correct horse battery staple",
        "device_name":"Desktop",
        "platform":"windows",
        "wrapped_master_key":"wrapped",
        "recovery_blob":"recovery"
    })
    .to_string();
    let (_, bytes) = call(
        app,
        "POST",
        "/api/v1/auth/register",
        Body::from(body),
        None,
        true,
    )
    .await;
    serde_json::from_slice(&bytes).unwrap()
}

#[tokio::test]
async fn encrypted_attachment_supports_resume_integrity_download_and_isolation() {
    let url = std::env::var("TEST_DATABASE_URL")
        .expect("TEST_DATABASE_URL must point to disposable PostgreSQL");
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&url)
        .await
        .unwrap();
    sqlx::migrate!().run(&pool).await.unwrap();
    let objects = TempDir::new().unwrap();
    let config = Config {
        bind: "127.0.0.1:0".parse().unwrap(),
        database_url: url,
        jwt_secret: "integration-test-secret-at-least-32-characters".into(),
        access_token_ttl_seconds: 300,
        refresh_token_ttl_seconds: 3600,
        object_dir: objects.path().to_path_buf(),
        attachment_quota_bytes: 10 * 1024 * 1024 * 1024,
        upload_ttl_seconds: 86_400,
        upload_cleanup_interval_seconds: 3_600,
    };
    let app = app(pool, &config);
    let owner = register(
        app.clone(),
        &format!("attachment-{}@example.com", Uuid::new_v4()),
    )
    .await;
    let stranger = register(
        app.clone(),
        &format!("attachment-other-{}@example.com", Uuid::new_v4()),
    )
    .await;
    let encrypted = b"opaque-encrypted-attachment";
    let checksum = URL_SAFE_NO_PAD.encode(Sha256::digest(encrypted));
    let id = Uuid::new_v4();
    let create = json!({
        "id":id,
        "total_size":encrypted.len(),
        "part_size":10,
        "total_parts":3,
        "ciphertext_sha256":checksum,
        "encrypted_metadata":"opaque-metadata"
    })
    .to_string();
    let (status, _) = call(
        app.clone(),
        "POST",
        "/api/v1/attachments",
        Body::from(create),
        Some(&owner.access_token),
        true,
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    for (number, part) in encrypted.chunks(10).enumerate() {
        let (status, _) = call(
            app.clone(),
            "PUT",
            &format!("/api/v1/attachments/{id}/parts/{number}"),
            Body::from(part.to_vec()),
            Some(&owner.access_token),
            false,
        )
        .await;
        assert_eq!(status, StatusCode::NO_CONTENT);
    }
    let (status, status_body) = call(
        app.clone(),
        "GET",
        &format!("/api/v1/attachments/{id}"),
        Body::empty(),
        Some(&owner.access_token),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        serde_json::from_slice::<Value>(&status_body).unwrap()["uploaded_parts"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
    let (status, _) = call(
        app.clone(),
        "POST",
        &format!("/api/v1/attachments/{id}/complete"),
        Body::empty(),
        Some(&owner.access_token),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, downloaded) = call(
        app.clone(),
        "GET",
        &format!("/api/v1/attachments/{id}/content"),
        Body::empty(),
        Some(&owner.access_token),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(downloaded, encrypted);

    let (status, _) = call(
        app.clone(),
        "GET",
        &format!("/api/v1/attachments/{id}/content"),
        Body::empty(),
        Some(&stranger.access_token),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = call(
        app.clone(),
        "DELETE",
        &format!("/api/v1/attachments/{id}"),
        Body::empty(),
        Some(&stranger.access_token),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = call(
        app,
        "DELETE",
        &format!("/api/v1/attachments/{id}"),
        Body::empty(),
        Some(&owner.access_token),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert!(
        !objects
            .path()
            .join(owner.user_id.to_string())
            .join(format!("{id}.blob"))
            .exists()
    );
}

#[tokio::test]
async fn attachment_quota_is_enforced_and_stale_uploads_are_removed() {
    let url = std::env::var("TEST_DATABASE_URL")
        .expect("TEST_DATABASE_URL must point to disposable PostgreSQL");
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&url)
        .await
        .unwrap();
    sqlx::migrate!().run(&pool).await.unwrap();
    let objects = TempDir::new().unwrap();
    let config = Config {
        bind: "127.0.0.1:0".parse().unwrap(),
        database_url: url,
        jwt_secret: "integration-test-secret-at-least-32-characters".into(),
        access_token_ttl_seconds: 300,
        refresh_token_ttl_seconds: 3600,
        object_dir: objects.path().to_path_buf(),
        attachment_quota_bytes: 10,
        upload_ttl_seconds: 60,
        upload_cleanup_interval_seconds: 3_600,
    };
    let app = app(pool.clone(), &config);
    let owner = register(
        app.clone(),
        &format!("attachment-lifecycle-{}@example.com", Uuid::new_v4()),
    )
    .await;
    let first_id = Uuid::new_v4();
    let create = |id: Uuid, size: usize| {
        json!({
            "id": id,
            "total_size": size,
            "part_size": size,
            "total_parts": 1,
            "ciphertext_sha256": URL_SAFE_NO_PAD.encode(Sha256::digest(vec![1_u8; size])),
            "encrypted_metadata": "opaque-metadata"
        })
        .to_string()
    };
    let (status, _) = call(
        app.clone(),
        "POST",
        "/api/v1/attachments",
        Body::from(create(first_id, 8)),
        Some(&owner.access_token),
        true,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, body) = call(
        app.clone(),
        "POST",
        "/api/v1/attachments",
        Body::from(create(Uuid::new_v4(), 3)),
        Some(&owner.access_token),
        true,
    )
    .await;
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(
        serde_json::from_slice::<Value>(&body).unwrap()["code"],
        "quota_exceeded"
    );

    let (status, _) = call(
        app.clone(),
        "DELETE",
        &format!("/api/v1/attachments/{first_id}"),
        Body::empty(),
        Some(&owner.access_token),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let stale_id = Uuid::new_v4();
    let (status, _) = call(
        app.clone(),
        "POST",
        "/api/v1/attachments",
        Body::from(create(stale_id, 3)),
        Some(&owner.access_token),
        true,
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, _) = call(
        app,
        "PUT",
        &format!("/api/v1/attachments/{stale_id}/parts/0"),
        Body::from(vec![1_u8; 3]),
        Some(&owner.access_token),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    sqlx::query("UPDATE attachments SET updated_at=$1 WHERE id=$2")
        .bind(Utc::now() - Duration::minutes(2))
        .bind(stale_id)
        .execute(&pool)
        .await
        .unwrap();
    let state = AppState::new(pool.clone(), &config);
    assert_eq!(cleanup_stale_uploads(&state, Utc::now()).await.unwrap(), 1);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM attachments WHERE id=$1")
            .bind(stale_id)
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
    assert!(
        !objects
            .path()
            .join(owner.user_id.to_string())
            .join(format!("{stale_id}.parts"))
            .exists()
    );
}
