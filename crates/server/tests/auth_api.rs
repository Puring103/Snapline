use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode, header},
};
use serde_json::{Value, json};
use snapline_server::{app, auth::AuthResponse, config::Config};
use sqlx::{PgPool, postgres::PgPoolOptions};
use std::path::PathBuf;
use tower::ServiceExt;

async fn test_app() -> (Router, PgPool) {
    let url = std::env::var("TEST_DATABASE_URL")
        .expect("TEST_DATABASE_URL must point to disposable PostgreSQL");
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&url)
        .await
        .unwrap();
    sqlx::migrate!().run(&pool).await.unwrap();
    sqlx::query("TRUNCATE sessions, devices, users CASCADE")
        .execute(&pool)
        .await
        .unwrap();
    let config = Config {
        bind: "127.0.0.1:0".parse().unwrap(),
        database_url: url,
        jwt_secret: "integration-test-secret-at-least-32-characters".into(),
        access_token_ttl_seconds: 300,
        refresh_token_ttl_seconds: 3600,
        object_dir: PathBuf::from("target/test-objects-auth"),
        attachment_quota_bytes: 10 * 1024 * 1024 * 1024,
        upload_ttl_seconds: 86_400,
        upload_cleanup_interval_seconds: 3_600,
    };
    (app(pool.clone(), &config), pool)
}

async fn json_request(
    app: Router,
    method: &str,
    uri: &str,
    body: Value,
    bearer: Option<&str>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/json");
    if let Some(token) = bearer {
        builder = builder.header(header::AUTHORIZATION, format!("Bearer {token}"));
    }
    let response = app
        .oneshot(builder.body(Body::from(body.to_string())).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    let value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap()
    };
    (status, value)
}

#[tokio::test]
async fn complete_auth_and_device_lifecycle() {
    let (app, pool) = test_app().await;
    let registration = json!({
        "email": "Owner@Example.com",
        "password": "correct horse battery staple",
        "device_name": "Primary desktop",
        "platform": "windows",
        "wrapped_master_key": "opaque-wrapped-key",
        "recovery_blob": "opaque-recovery-blob"
    });
    let (status, body) = json_request(
        app.clone(),
        "POST",
        "/api/v1/auth/register",
        registration.clone(),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let primary: AuthResponse = serde_json::from_value(body).unwrap();

    let (status, _) = json_request(
        app.clone(),
        "POST",
        "/api/v1/auth/register",
        registration,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);

    let login = json!({
        "email": "owner@example.com",
        "password": "correct horse battery staple",
        "device_name": "Second desktop",
        "platform": "windows"
    });
    let (status, body) = json_request(app.clone(), "POST", "/api/v1/auth/login", login, None).await;
    assert_eq!(status, StatusCode::OK);
    let second: AuthResponse = serde_json::from_value(body).unwrap();
    assert_eq!(second.wrapped_master_key, "opaque-wrapped-key");

    let (status, devices) = json_request(
        app.clone(),
        "GET",
        "/api/v1/devices",
        Value::Null,
        Some(&primary.access_token),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(devices.as_array().unwrap().len(), 2);

    let refresh_body = json!({"refresh_token": second.refresh_token});
    let (status, body) = json_request(
        app.clone(),
        "POST",
        "/api/v1/auth/refresh",
        refresh_body.clone(),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let rotated: AuthResponse = serde_json::from_value(body).unwrap();
    let (status, _) = json_request(
        app.clone(),
        "POST",
        "/api/v1/auth/refresh",
        refresh_body,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let (status, _) = json_request(
        app.clone(),
        "DELETE",
        &format!("/api/v1/devices/{}", second.device_id),
        Value::Null,
        Some(&primary.access_token),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = json_request(
        app,
        "GET",
        "/api/v1/devices",
        Value::Null,
        Some(&rotated.access_token),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let plaintext_secret_count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM sessions WHERE refresh_token_hash = $1")
            .bind(rotated.refresh_token)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(plaintext_secret_count, 0);
}

#[tokio::test]
async fn readiness_depends_on_postgres() {
    let (app, pool) = test_app().await;
    let (status, _) = json_request(app.clone(), "GET", "/health/ready", Value::Null, None).await;
    assert_eq!(status, StatusCode::OK);
    pool.close().await;
    let (status, body) = json_request(app, "GET", "/health/ready", Value::Null, None).await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(body["code"], "internal_error");
}
