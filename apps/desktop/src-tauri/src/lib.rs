use std::{fs, path::PathBuf, sync::Mutex};

use chrono::{DateTime, Utc};
use keyring::Entry;
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use snapline_crypto::{KeyEnvelope, MasterKey, RecoveryKey};
use snapline_desktop_core::{Item, Repository, SaveItem};
use tauri::{AppHandle, Manager, State};
use uuid::Uuid;

const DEFAULT_SERVER_URL: &str = "http://122.51.119.75/snapline";
const CREDENTIAL_SERVICE: &str = "app.snapline.desktop.refresh-token";

#[derive(Debug, Clone, Serialize)]
pub struct AuthResult {
    pub user_id: Uuid,
    pub device_id: Uuid,
    pub recovery_key: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionStatus {
    pub authenticated: bool,
    pub user_id: Option<Uuid>,
    pub device_id: Option<Uuid>,
    pub access_expires_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize, Deserialize)]
struct AuthResponse {
    user_id: Uuid,
    device_id: Uuid,
    access_token: String,
    access_expires_at: DateTime<Utc>,
    refresh_token: String,
    refresh_expires_at: DateTime<Utc>,
    wrapped_master_key: String,
    recovery_blob: String,
}

#[derive(Serialize)]
struct RegisterRequest<'a> {
    email: &'a str,
    password: &'a str,
    device_name: &'a str,
    platform: &'static str,
    wrapped_master_key: String,
    recovery_blob: String,
}

#[derive(Serialize)]
struct LoginRequest<'a> {
    email: &'a str,
    password: &'a str,
    device_name: &'a str,
    platform: &'static str,
}

#[derive(Serialize)]
struct LogoutRequest<'a> {
    refresh_token: &'a str,
}

struct Session {
    user_id: Uuid,
    device_id: Uuid,
    access_token: String,
    access_expires_at: DateTime<Utc>,
}

#[derive(Default)]
struct UnlockedState {
    master_key: Option<MasterKey>,
    repository: Option<Repository>,
    session: Option<Session>,
}

pub struct DesktopState {
    client: Client,
    server_url: String,
    data_dir: PathBuf,
    unlocked: Mutex<UnlockedState>,
}

impl DesktopState {
    fn new(data_dir: PathBuf, server_url: String) -> Result<Self, String> {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|_| "无法初始化网络客户端".to_string())?;
        Ok(Self {
            client,
            server_url: server_url.trim_end_matches('/').to_string(),
            data_dir,
            unlocked: Mutex::new(UnlockedState::default()),
        })
    }

    fn api_url(&self, path: &str) -> String {
        format!(
            "{}/api/v1/{}",
            self.server_url,
            path.trim_start_matches('/')
        )
    }

    fn unlock(&self, response: &AuthResponse, master_key: MasterKey) -> Result<(), String> {
        let user_dir = self.data_dir.join(response.user_id.to_string());
        fs::create_dir_all(&user_dir).map_err(|_| "无法创建本地数据目录".to_string())?;
        let repository = Repository::open(user_dir.join("snapline.db"))
            .map_err(|_| "无法打开本地加密记录库".to_string())?;
        credential_entry(response.user_id)?
            .set_password(&response.refresh_token)
            .map_err(|_| "无法将登录凭据写入 Windows 凭据管理器".to_string())?;
        let mut unlocked = self
            .unlocked
            .lock()
            .map_err(|_| "本地会话状态不可用".to_string())?;
        *unlocked = UnlockedState {
            master_key: Some(master_key),
            repository: Some(repository),
            session: Some(Session {
                user_id: response.user_id,
                device_id: response.device_id,
                access_token: response.access_token.clone(),
                access_expires_at: response.access_expires_at,
            }),
        };
        Ok(())
    }
}

fn credential_entry(user_id: Uuid) -> Result<Entry, String> {
    Entry::new(CREDENTIAL_SERVICE, &user_id.to_string())
        .map_err(|_| "Windows 凭据管理器不可用".to_string())
}

fn registration_material(password: &str) -> Result<(MasterKey, String, String, String), String> {
    let master_key = MasterKey::generate();
    let recovery_key = RecoveryKey::generate();
    let password_envelope = master_key
        .wrap_with_password(password)
        .map_err(|_| "无法生成密码密钥信封".to_string())?;
    let recovery_envelope = master_key
        .wrap_with_recovery(&recovery_key)
        .map_err(|_| "无法生成恢复密钥信封".to_string())?;
    Ok((
        master_key,
        serde_json::to_string(&password_envelope)
            .map_err(|_| "无法编码密码密钥信封".to_string())?,
        serde_json::to_string(&recovery_envelope)
            .map_err(|_| "无法编码恢复密钥信封".to_string())?,
        recovery_key.expose_once(),
    ))
}

fn master_key_from_response(password: &str, response: &AuthResponse) -> Result<MasterKey, String> {
    let envelope: KeyEnvelope = serde_json::from_str(&response.wrapped_master_key)
        .map_err(|_| "服务器返回了无效的密钥信封".to_string())?;
    MasterKey::unwrap_with_password(password, &envelope)
        .map_err(|_| "密码无法解锁本地数据密钥".to_string())
}

async fn parse_response(response: reqwest::Response) -> Result<AuthResponse, String> {
    let status = response.status();
    if status.is_success() {
        return response
            .json()
            .await
            .map_err(|_| "服务器返回了无效响应".to_string());
    }
    let message = match status {
        StatusCode::UNAUTHORIZED => "邮箱或密码错误",
        StatusCode::CONFLICT => "该邮箱已经注册",
        StatusCode::TOO_MANY_REQUESTS => "登录尝试过多，请稍后再试",
        StatusCode::UNPROCESSABLE_ENTITY | StatusCode::BAD_REQUEST => "提交的信息不符合要求",
        _ if status.is_server_error() => "服务器暂时不可用",
        _ => "请求失败",
    };
    Err(message.to_string())
}

#[tauri::command]
fn auth_status(state: State<'_, DesktopState>) -> Result<SessionStatus, String> {
    let unlocked = state
        .unlocked
        .lock()
        .map_err(|_| "本地会话状态不可用".to_string())?;
    Ok(SessionStatus {
        authenticated: unlocked.session.is_some() && unlocked.master_key.is_some(),
        user_id: unlocked.session.as_ref().map(|session| session.user_id),
        device_id: unlocked.session.as_ref().map(|session| session.device_id),
        access_expires_at: unlocked
            .session
            .as_ref()
            .map(|session| session.access_expires_at),
    })
}

#[tauri::command]
async fn register_account(
    state: State<'_, DesktopState>,
    email: String,
    password: String,
    device_name: String,
) -> Result<AuthResult, String> {
    let (master_key, wrapped_master_key, recovery_blob, recovery_key) =
        registration_material(&password)?;
    let response = state
        .client
        .post(state.api_url("auth/register"))
        .json(&RegisterRequest {
            email: email.trim(),
            password: &password,
            device_name: device_name.trim(),
            platform: "windows",
            wrapped_master_key,
            recovery_blob,
        })
        .send()
        .await
        .map_err(|_| "无法连接 Snapline 服务端".to_string())?;
    let response = parse_response(response).await?;
    state.unlock(&response, master_key)?;
    Ok(AuthResult {
        user_id: response.user_id,
        device_id: response.device_id,
        recovery_key: Some(recovery_key),
    })
}

#[tauri::command]
async fn login_account(
    state: State<'_, DesktopState>,
    email: String,
    password: String,
    device_name: String,
) -> Result<AuthResult, String> {
    let response = state
        .client
        .post(state.api_url("auth/login"))
        .json(&LoginRequest {
            email: email.trim(),
            password: &password,
            device_name: device_name.trim(),
            platform: "windows",
        })
        .send()
        .await
        .map_err(|_| "无法连接 Snapline 服务端".to_string())?;
    let response = parse_response(response).await?;
    let master_key = master_key_from_response(&password, &response)?;
    state.unlock(&response, master_key)?;
    Ok(AuthResult {
        user_id: response.user_id,
        device_id: response.device_id,
        recovery_key: None,
    })
}

#[tauri::command]
async fn logout_account(state: State<'_, DesktopState>) -> Result<(), String> {
    let session = {
        let unlocked = state
            .unlocked
            .lock()
            .map_err(|_| "本地会话状态不可用".to_string())?;
        unlocked
            .session
            .as_ref()
            .map(|session| (session.user_id, session.access_token.clone()))
    };
    if let Some((user_id, _)) = session
        && let Ok(entry) = credential_entry(user_id)
    {
        if let Ok(refresh_token) = entry.get_password() {
            let _ = state
                .client
                .post(state.api_url("auth/logout"))
                .json(&LogoutRequest {
                    refresh_token: &refresh_token,
                })
                .send()
                .await;
        }
        let _ = entry.delete_credential();
    }
    let mut unlocked = state
        .unlocked
        .lock()
        .map_err(|_| "本地会话状态不可用".to_string())?;
    *unlocked = UnlockedState::default();
    Ok(())
}

fn with_repository<T>(
    state: &DesktopState,
    operation: impl FnOnce(&Repository, &MasterKey) -> Result<T, String>,
) -> Result<T, String> {
    let unlocked = state
        .unlocked
        .lock()
        .map_err(|_| "本地会话状态不可用".to_string())?;
    let repository = unlocked
        .repository
        .as_ref()
        .ok_or_else(|| "请先登录并解锁".to_string())?;
    let master_key = unlocked
        .master_key
        .as_ref()
        .ok_or_else(|| "请先登录并解锁".to_string())?;
    operation(repository, master_key)
}

#[tauri::command]
fn list_items(state: State<'_, DesktopState>) -> Result<Vec<Item>, String> {
    with_repository(&state, |repository, master_key| {
        repository
            .list(master_key, true)
            .map_err(|_| "无法读取本地记录".to_string())
    })
}

#[tauri::command]
fn save_item(state: State<'_, DesktopState>, input: SaveItem) -> Result<Item, String> {
    with_repository(&state, |repository, master_key| {
        repository
            .save(master_key, input)
            .map_err(|_| "无法保存本地记录".to_string())
    })
}

#[tauri::command]
fn delete_item(state: State<'_, DesktopState>, id: Uuid) -> Result<(), String> {
    with_repository(&state, |repository, _| {
        repository
            .delete(id)
            .map_err(|_| "无法删除本地记录".to_string())
    })
}

fn server_url() -> String {
    std::env::var("SNAPLINE_SERVER_URL").unwrap_or_else(|_| DEFAULT_SERVER_URL.to_string())
}

fn app_data_dir(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map_err(|_| "无法定位应用数据目录".to_string())
}

pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let state = DesktopState::new(app_data_dir(app.handle())?, server_url())?;
            app.manage(state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            auth_status,
            register_account,
            login_account,
            logout_account,
            list_items,
            save_item,
            delete_item
        ])
        .run(tauri::generate_context!())
        .expect("failed to run Snapline desktop application");
}

#[cfg(test)]
mod tests {
    use super::*;
    use snapline_desktop_core::{ItemContent, SourceType};

    #[test]
    fn registration_material_unlocks_with_password_and_recovery_key() {
        let password = "correct horse battery staple";
        let (master, wrapped, recovery_blob, recovery_key) =
            registration_material(password).unwrap();
        let password_envelope: KeyEnvelope = serde_json::from_str(&wrapped).unwrap();
        let recovery_envelope: KeyEnvelope = serde_json::from_str(&recovery_blob).unwrap();
        let by_password = MasterKey::unwrap_with_password(password, &password_envelope).unwrap();
        let recovery = RecoveryKey::parse(&recovery_key).unwrap();
        let by_recovery = MasterKey::unwrap_with_recovery(&recovery, &recovery_envelope).unwrap();
        let record = master.encrypt(b"record", b"private content").unwrap();
        assert_eq!(
            by_password.decrypt(b"record", &record).unwrap(),
            b"private content"
        );
        assert_eq!(
            by_recovery.decrypt(b"record", &record).unwrap(),
            b"private content"
        );
    }

    #[test]
    fn api_url_preserves_reverse_proxy_prefix() {
        let directory = tempfile::tempdir().unwrap();
        let state = DesktopState::new(
            directory.path().to_path_buf(),
            "http://server.example/snapline/".into(),
        )
        .unwrap();
        assert_eq!(
            state.api_url("auth/login"),
            "http://server.example/snapline/api/v1/auth/login"
        );
    }

    #[test]
    fn locked_state_rejects_repository_access() {
        let directory = tempfile::tempdir().unwrap();
        let state =
            DesktopState::new(directory.path().to_path_buf(), "http://localhost".into()).unwrap();
        let result = with_repository(&state, |_, _| Ok(()));
        assert_eq!(result.unwrap_err(), "请先登录并解锁");
    }

    #[tokio::test]
    #[ignore = "requires SNAPLINE_LIVE_TEST=1 and the deployed myServer API"]
    async fn live_server_register_unlock_save_and_login_again() {
        assert_eq!(std::env::var("SNAPLINE_LIVE_TEST").as_deref(), Ok("1"));
        let directory = tempfile::tempdir().unwrap();
        let state =
            DesktopState::new(directory.path().to_path_buf(), DEFAULT_SERVER_URL.into()).unwrap();
        let email = format!("native-smoke-{}@example.com", Uuid::new_v4());
        let password = "temporary native smoke password";
        let (master_key, wrapped_master_key, recovery_blob, _) =
            registration_material(password).unwrap();
        let registered = parse_response(
            state
                .client
                .post(state.api_url("auth/register"))
                .json(&RegisterRequest {
                    email: &email,
                    password,
                    device_name: "Automated Windows Test",
                    platform: "windows",
                    wrapped_master_key,
                    recovery_blob,
                })
                .send()
                .await
                .unwrap(),
        )
        .await
        .unwrap();
        state.unlock(&registered, master_key).unwrap();
        assert_eq!(
            credential_entry(registered.user_id)
                .unwrap()
                .get_password()
                .unwrap(),
            registered.refresh_token
        );

        let item_id = Uuid::new_v4();
        with_repository(&state, |repository, master_key| {
            repository
                .save(
                    master_key,
                    SaveItem {
                        id: item_id,
                        content: ItemContent {
                            title: "live encrypted title".into(),
                            markdown: "# live encrypted markdown".into(),
                            source_type: SourceType::Text,
                            tags: vec!["live".into()],
                            markers: vec!["账目".into()],
                            attachment_ids: Vec::new(),
                            ai_metadata: None,
                        },
                        archived: false,
                        pinned: false,
                    },
                )
                .map(|_| ())
                .map_err(|error| error.to_string())
        })
        .unwrap();

        let logged_in = parse_response(
            state
                .client
                .post(state.api_url("auth/login"))
                .json(&LoginRequest {
                    email: &email,
                    password,
                    device_name: "Automated Windows Relogin",
                    platform: "windows",
                })
                .send()
                .await
                .unwrap(),
        )
        .await
        .unwrap();
        let login_key = master_key_from_response(password, &logged_in).unwrap();
        let repository = Repository::open(
            directory
                .path()
                .join(registered.user_id.to_string())
                .join("snapline.db"),
        )
        .unwrap();
        assert_eq!(
            repository
                .get(&login_key, item_id)
                .unwrap()
                .content
                .markdown,
            "# live encrypted markdown"
        );
        credential_entry(registered.user_id)
            .unwrap()
            .delete_credential()
            .unwrap();
        println!("LIVE_TEST_EMAIL={email}");
    }
}
