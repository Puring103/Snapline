use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode, header},
};
use chrono::Utc;
use serde_json::{Value, json};
use snapline_server::{
    app,
    auth::AuthResponse,
    config::Config,
    sync::{PullResponse, PushResponse},
};
use sqlx::postgres::PgPoolOptions;
use std::path::PathBuf;
use tower::ServiceExt;
use uuid::Uuid;

async fn request(
    app: Router,
    method: &str,
    uri: &str,
    body: Value,
    token: Option<&str>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/json");
    if let Some(token) = token {
        builder = builder.header(header::AUTHORIZATION, format!("Bearer {token}"));
    }
    let response = app
        .oneshot(builder.body(Body::from(body.to_string())).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 8 * 1024 * 1024)
        .await
        .unwrap();
    (
        status,
        if bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&bytes).unwrap()
        },
    )
}

async fn register(app: Router, email: &str) -> AuthResponse {
    let (_, body) = request(
        app,
        "POST",
        "/api/v1/auth/register",
        json!({
            "email": email, "password": "correct horse battery staple", "device_name": "Desktop",
            "platform": "windows", "wrapped_master_key": "wrapped", "recovery_blob": "recovery"
        }),
        None,
    )
    .await;
    serde_json::from_value(body).unwrap()
}

#[tokio::test]
async fn encrypted_sync_is_idempotent_isolated_and_conflict_aware() {
    let url = std::env::var("TEST_DATABASE_URL")
        .expect("TEST_DATABASE_URL must point to disposable PostgreSQL");
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&url)
        .await
        .unwrap();
    sqlx::migrate!().run(&pool).await.unwrap();
    let config = Config {
        bind: "127.0.0.1:0".parse().unwrap(),
        database_url: url,
        jwt_secret: "integration-test-secret-at-least-32-characters".into(),
        access_token_ttl_seconds: 300,
        refresh_token_ttl_seconds: 3600,
        object_dir: PathBuf::from("target/test-objects-sync"),
    };
    let app = app(pool, &config);
    let owner = register(app.clone(), &format!("sync-{}@example.com", Uuid::new_v4())).await;
    let stranger = register(
        app.clone(),
        &format!("other-{}@example.com", Uuid::new_v4()),
    )
    .await;
    let object_id = Uuid::new_v4();
    let idempotency_key = Uuid::new_v4();
    let push = json!({"idempotency_key": idempotency_key, "changes": [{
        "object_id": object_id, "object_type": "item", "device_id": owner.device_id,
        "base_version": 0, "operation": "upsert", "ciphertext": "opaque-ciphertext",
        "nonce": "opaque-nonce", "wrapped_key": "opaque-key", "client_updated_at": Utc::now()
    }]});
    let (status, body) = request(
        app.clone(),
        "POST",
        "/api/v1/sync/push",
        push.clone(),
        Some(&owner.access_token),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let first: PushResponse = serde_json::from_value(body).unwrap();
    assert_eq!(first.accepted[0].version, 1);
    let (status, body) = request(
        app.clone(),
        "POST",
        "/api/v1/sync/push",
        push,
        Some(&owner.access_token),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(serde_json::from_value::<PushResponse>(body).unwrap(), first);

    let conflict = json!({"idempotency_key": Uuid::new_v4(), "changes": [{
        "object_id": object_id, "object_type": "item", "device_id": owner.device_id,
        "base_version": 0, "operation": "upsert", "ciphertext": "stale",
        "nonce": "nonce", "wrapped_key": "key", "client_updated_at": Utc::now()
    }]});
    let (status, _) = request(
        app.clone(),
        "POST",
        "/api/v1/sync/push",
        conflict,
        Some(&owner.access_token),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);

    let (status, body) = request(
        app.clone(),
        "GET",
        "/api/v1/sync/pull?cursor=0&limit=100",
        Value::Null,
        Some(&owner.access_token),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let pulled: PullResponse = serde_json::from_value(body).unwrap();
    assert_eq!(pulled.changes.len(), 1);
    assert_eq!(pulled.changes[0].envelope.ciphertext, "opaque-ciphertext");

    let (_, body) = request(
        app.clone(),
        "GET",
        "/api/v1/sync/pull?cursor=0",
        Value::Null,
        Some(&stranger.access_token),
    )
    .await;
    let stranger_pull: PullResponse = serde_json::from_value(body).unwrap();
    assert!(stranger_pull.changes.is_empty());

    let (status, _) = request(
        app,
        "POST",
        "/api/v1/sync/ack",
        json!({"cursor": pulled.next_cursor}),
        Some(&owner.access_token),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
}
