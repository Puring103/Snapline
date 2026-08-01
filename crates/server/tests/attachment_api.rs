use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode, header},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use snapline_server::{app, auth::AuthResponse, config::Config};
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
        app,
        "GET",
        &format!("/api/v1/attachments/{id}/content"),
        Body::empty(),
        Some(&stranger.access_token),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}
