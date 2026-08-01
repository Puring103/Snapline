use std::{
    fs::{self, File},
    io::Cursor,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Instant,
};

use chrono::{DateTime, Utc};
use cpal::{
    SampleFormat, Stream, StreamConfig,
    traits::{DeviceTrait, HostTrait, StreamTrait},
};
use image::{DynamicImage, ImageFormat};
use keyring::Entry;
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use snapline_crypto::{KeyEnvelope, MasterKey, RecoveryKey};
use snapline_desktop_core::{Attachment, AttachmentDescriptor, Item, Repository, SaveItem};
use tauri::{
    AppHandle, Manager, State, WebviewUrl, WebviewWindow, WebviewWindowBuilder, WindowEvent,
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, ShortcutState};
use uuid::Uuid;
use xcap::Monitor;

mod agent;
mod ai;
mod media_ai;

use ai::{AiAttachment, AiConfig, OpenAiCompatibleClient, validate_config};

const DEFAULT_SERVER_URL: &str = "http://122.51.119.75/snapline";
const CREDENTIAL_SERVICE: &str = "app.snapline.desktop.refresh-token";
const AI_CREDENTIAL_SERVICE: &str = "app.snapline.desktop.ai-config";
const MAX_PASTED_ATTACHMENT_BYTES: usize = 32 * 1024 * 1024;
const MAX_IMPORTED_ATTACHMENT_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const MAX_RECORDING_SECONDS: usize = 30 * 60;
const ATTACHMENT_CHUNK_BYTES: u64 = 1024 * 1024;
const ATTACHMENT_FRAME_OVERHEAD: u64 = 20;
const MAX_PROTOCOL_RANGE_BYTES: u64 = 64 * 1024 * 1024;

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

#[derive(Debug, Clone, Serialize)]
pub struct MediaAttachment {
    pub id: Uuid,
    pub media_type: String,
    pub display_name: String,
    pub ciphertext_bytes: u64,
    pub ciphertext_sha256: String,
    pub duration_seconds: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AiConfigStatus {
    pub configured: bool,
    pub base_url: Option<String>,
    pub model: Option<String>,
    pub processing: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct AiProcessResult {
    pub completed: usize,
    pub failed: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredAiConfig {
    base_url: String,
    model: String,
    api_key: String,
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
    master_key: Option<Arc<MasterKey>>,
    repository: Option<Arc<Repository>>,
    session: Option<Session>,
}

struct RecordingSession {
    stream: Stream,
    samples: Arc<Mutex<Vec<i16>>>,
    overflowed: Arc<AtomicBool>,
    config: StreamConfig,
    started_at: Instant,
}

pub struct DesktopState {
    client: Client,
    server_url: String,
    data_dir: PathBuf,
    unlocked: Mutex<UnlockedState>,
    recording: Mutex<Option<RecordingSession>>,
    ai_processing: AtomicBool,
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
            recording: Mutex::new(None),
            ai_processing: AtomicBool::new(false),
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
            master_key: Some(Arc::new(master_key)),
            repository: Some(Arc::new(repository)),
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

fn ai_credential_entry(user_id: Uuid) -> Result<Entry, String> {
    Entry::new(AI_CREDENTIAL_SERVICE, &user_id.to_string())
        .map_err(|_| "Windows 凭据管理器不可用".to_string())
}

fn current_user_id(state: &DesktopState) -> Result<Uuid, String> {
    state
        .unlocked
        .lock()
        .map_err(|_| "本地会话状态不可用".to_string())?
        .session
        .as_ref()
        .map(|session| session.user_id)
        .ok_or_else(|| "请先登录并解锁".to_string())
}

fn stored_ai_config(state: &DesktopState) -> Result<Option<StoredAiConfig>, String> {
    let user_id = current_user_id(state)?;
    let entry = ai_credential_entry(user_id)?;
    match entry.get_password() {
        Ok(value) => serde_json::from_str(&value)
            .map(Some)
            .map_err(|_| "AI 配置已损坏，请重新配置".to_string()),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(_) => Err("无法读取 Windows 凭据管理器中的 AI 配置".to_string()),
    }
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

fn crypto_context(state: &DesktopState) -> Result<(Arc<Repository>, Arc<MasterKey>), String> {
    let unlocked = state
        .unlocked
        .lock()
        .map_err(|_| "本地会话状态不可用".to_string())?;
    Ok((
        unlocked
            .repository
            .as_ref()
            .cloned()
            .ok_or_else(|| "请先登录并解锁".to_string())?,
        unlocked
            .master_key
            .as_ref()
            .cloned()
            .ok_or_else(|| "请先登录并解锁".to_string())?,
    ))
}

#[tauri::command]
fn list_items(state: State<'_, DesktopState>) -> Result<Vec<Item>, String> {
    let configured = stored_ai_config(&state).ok().flatten().is_some();
    let mut items = with_repository(&state, |repository, master_key| {
        repository
            .list(master_key, true)
            .map_err(|_| "无法读取本地记录".to_string())
    })?;
    if !configured {
        for item in &mut items {
            if item.ai_status != "complete" {
                item.ai_status = "unconfigured".into();
            }
        }
    }
    Ok(items)
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

#[tauri::command]
fn get_ai_config(state: State<'_, DesktopState>) -> Result<AiConfigStatus, String> {
    let stored = stored_ai_config(&state)?;
    Ok(AiConfigStatus {
        configured: stored.is_some(),
        base_url: stored.as_ref().map(|config| config.base_url.clone()),
        model: stored.as_ref().map(|config| config.model.clone()),
        processing: state.ai_processing.load(Ordering::Acquire),
    })
}

#[tauri::command]
async fn set_ai_config(
    state: State<'_, DesktopState>,
    base_url: String,
    model: String,
    api_key: String,
) -> Result<AiConfigStatus, String> {
    let config =
        validate_config(&AiConfig { base_url, model }).map_err(|error| error.to_string())?;
    let api_key = if api_key.trim().is_empty() {
        stored_ai_config(&state)?
            .map(|stored| stored.api_key)
            .ok_or_else(|| "请输入 API Key".to_string())?
    } else {
        api_key
    };
    let client = OpenAiCompatibleClient::new(config.clone(), api_key.clone())
        .map_err(|error| error.to_string())?;
    client.probe().await.map_err(|error| error.to_string())?;
    let user_id = current_user_id(&state)?;
    let value = serde_json::to_string(&StoredAiConfig {
        base_url: config.base_url.clone(),
        model: config.model.clone(),
        api_key,
    })
    .map_err(|_| "无法编码 AI 配置".to_string())?;
    ai_credential_entry(user_id)?
        .set_password(&value)
        .map_err(|_| "无法将 AI Key 写入 Windows 凭据管理器".to_string())?;
    with_repository(&state, |repository, _| {
        repository
            .reset_ai_jobs()
            .map(|_| ())
            .map_err(|_| "无法重建 AI 处理队列".to_string())
    })?;
    Ok(AiConfigStatus {
        configured: true,
        base_url: Some(config.base_url),
        model: Some(config.model),
        processing: false,
    })
}

#[tauri::command]
fn clear_ai_config(state: State<'_, DesktopState>) -> Result<(), String> {
    let user_id = current_user_id(&state)?;
    match ai_credential_entry(user_id)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(_) => Err("无法删除 Windows 凭据管理器中的 AI 配置".to_string()),
    }
}

#[tauri::command]
fn rebuild_ai_metadata(state: State<'_, DesktopState>) -> Result<usize, String> {
    if stored_ai_config(&state)?.is_none() {
        return Err("尚未配置 AI 模型".to_string());
    }
    with_repository(&state, |repository, _| {
        repository
            .reset_ai_jobs()
            .map_err(|_| "无法重建 AI 处理队列".to_string())
    })
}

struct AiProcessingGuard<'a>(&'a AtomicBool);

impl Drop for AiProcessingGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

#[tauri::command]
async fn process_ai_queue(state: State<'_, DesktopState>) -> Result<AiProcessResult, String> {
    if state.ai_processing.swap(true, Ordering::AcqRel) {
        return Ok(AiProcessResult {
            completed: 0,
            failed: 0,
        });
    }
    let _guard = AiProcessingGuard(&state.ai_processing);
    let stored = stored_ai_config(&state)?.ok_or_else(|| "尚未配置 AI 模型".to_string())?;
    let client = OpenAiCompatibleClient::new(
        AiConfig {
            base_url: stored.base_url,
            model: stored.model,
        },
        stored.api_key,
    )
    .map_err(|error| error.to_string())?;
    let (repository, master_key) = crypto_context(&state)?;
    let jobs = repository
        .claim_ai_jobs(&master_key, 20)
        .map_err(|_| "无法读取 AI 处理队列".to_string())?;
    let mut result = AiProcessResult {
        completed: 0,
        failed: 0,
    };
    for item in jobs {
        let mut attachments = Vec::new();
        let mut attachment_error = None;
        for id in &item.content.attachment_ids {
            let descriptor = match repository.attachment_descriptor(*id) {
                Ok(value) => value,
                Err(_) => {
                    attachment_error = Some("附件描述缺失".to_string());
                    break;
                }
            };
            if descriptor.media_type.starts_with("video/") {
                match media_ai::ensure_ffmpeg(&state.data_dir, &state.client).await {
                    Ok(ffmpeg) => {
                        match media_ai::extract_video_inputs(&repository, &master_key, *id, &ffmpeg)
                        {
                            Ok(mut inputs) => attachments.append(&mut inputs),
                            Err(error) => attachment_error = Some(error),
                        }
                    }
                    Err(error) => attachment_error = Some(error),
                }
                if attachment_error.is_some() {
                    break;
                }
                continue;
            }
            if descriptor.media_type.starts_with("audio/")
                && repository
                    .attachment_ciphertext_bytes(*id)
                    .unwrap_or(u64::MAX)
                    > (32 * 1024 * 1024 + 1024 * 1024) as u64
            {
                match media_ai::ensure_ffmpeg(&state.data_dir, &state.client).await {
                    Ok(ffmpeg) => {
                        match media_ai::extract_audio_input(&repository, &master_key, *id, &ffmpeg)
                        {
                            Ok(input) => attachments.push(input),
                            Err(error) => attachment_error = Some(error),
                        }
                    }
                    Err(error) => attachment_error = Some(error),
                }
                if attachment_error.is_some() {
                    break;
                }
                continue;
            }
            if descriptor.media_type.starts_with("image/")
                && repository
                    .attachment_ciphertext_bytes(*id)
                    .unwrap_or(u64::MAX)
                    > (32 * 1024 * 1024 + 1024 * 1024) as u64
            {
                match media_ai::ensure_ffmpeg(&state.data_dir, &state.client).await {
                    Ok(ffmpeg) => {
                        match media_ai::extract_image_input(&repository, &master_key, *id, &ffmpeg)
                        {
                            Ok(input) => attachments.push(input),
                            Err(error) => attachment_error = Some(error),
                        }
                    }
                    Err(error) => attachment_error = Some(error),
                }
                if attachment_error.is_some() {
                    break;
                }
                continue;
            }
            if repository
                .attachment_ciphertext_bytes(*id)
                .unwrap_or(u64::MAX)
                > (32 * 1024 * 1024 + 1024 * 1024) as u64
            {
                attachment_error = Some("附件超过单模型处理上限".to_string());
                break;
            }
            let mut bytes = Vec::new();
            if repository
                .read_attachment(&master_key, *id, &mut bytes)
                .is_err()
            {
                attachment_error = Some("无法解密 AI 处理附件".to_string());
                break;
            }
            attachments.push(AiAttachment {
                media_type: descriptor.media_type,
                display_name: format!("附件-{}", descriptor.id),
                bytes,
            });
        }
        let response = match attachment_error {
            Some(error) => Err((error, 3_600)),
            None => client.metadata(&item, &attachments).await.map_err(|error| {
                let attempts = repository.ai_job_attempts(item.id).unwrap_or(1);
                let retry = error.retry_after_seconds(attempts);
                (error.to_string(), retry)
            }),
        };
        match response {
            Ok(metadata) => {
                if repository
                    .complete_ai_job(&master_key, item.id, metadata)
                    .is_ok()
                {
                    result.completed += 1;
                } else {
                    let _ = repository.fail_ai_job(item.id, "无法保存 AI 元数据", 60);
                    result.failed += 1;
                }
            }
            Err((message, retry)) => {
                let _ = repository.fail_ai_job(item.id, &message, retry);
                result.failed += 1;
            }
        }
    }
    repository
        .rebuild_search_index(&master_key)
        .map_err(|_| "无法更新本地全文索引".to_string())?;
    Ok(result)
}

#[tauri::command]
async fn ask_agent(
    state: State<'_, DesktopState>,
    question: String,
) -> Result<agent::AgentAnswer, String> {
    let stored = stored_ai_config(&state)?.ok_or_else(|| "尚未配置 AI 模型".to_string())?;
    let client = OpenAiCompatibleClient::new(
        AiConfig {
            base_url: stored.base_url,
            model: stored.model,
        },
        stored.api_key,
    )
    .map_err(|error| error.to_string())?;
    let (repository, master_key) = crypto_context(&state)?;
    agent::run_agent(&client, &repository, &master_key, &question)
        .await
        .map_err(|error| error.to_string())
}

fn media_attachment(
    attachment: Attachment,
    media_type: impl Into<String>,
    display_name: impl Into<String>,
    duration_seconds: Option<u64>,
) -> MediaAttachment {
    MediaAttachment {
        id: attachment.id,
        media_type: media_type.into(),
        display_name: display_name.into(),
        ciphertext_bytes: attachment.ciphertext_bytes,
        ciphertext_sha256: attachment.ciphertext_sha256,
        duration_seconds,
    }
}

fn described_media_attachment(
    repository: &Repository,
    attachment: Attachment,
    media_type: impl Into<String>,
    display_name: impl Into<String>,
    duration_seconds: Option<u64>,
) -> Result<MediaAttachment, String> {
    let media_type = media_type.into();
    let display_name = display_name.into();
    repository
        .save_attachment_descriptor(&AttachmentDescriptor {
            id: attachment.id,
            media_type: media_type.clone(),
        })
        .map_err(|_| "无法保存附件描述".to_string())?;
    Ok(media_attachment(
        attachment,
        media_type,
        display_name,
        duration_seconds,
    ))
}

#[tauri::command]
fn store_attachment_bytes(
    state: State<'_, DesktopState>,
    bytes: Vec<u8>,
    media_type: String,
    display_name: String,
) -> Result<MediaAttachment, String> {
    if bytes.is_empty() || bytes.len() > MAX_PASTED_ATTACHMENT_BYTES {
        return Err("粘贴附件为空或超过 32 MiB".to_string());
    }
    if !matches!(
        media_type.as_str(),
        "image/png" | "image/jpeg" | "image/webp" | "image/gif"
    ) {
        return Err("不支持该粘贴图片格式".to_string());
    }
    let (repository, master_key) = crypto_context(&state)?;
    let id = Uuid::new_v4();
    let attachment = repository
        .save_attachment(&master_key, id, bytes.as_slice())
        .map_err(|_| "无法加密保存粘贴图片".to_string())?;
    described_media_attachment(
        &repository,
        attachment,
        media_type,
        sanitize_display_name(&display_name),
        None,
    )
}

#[tauri::command]
fn capture_screenshot(
    window: WebviewWindow,
    state: State<'_, DesktopState>,
) -> Result<MediaAttachment, String> {
    let (repository, master_key) = crypto_context(&state)?;
    let hidden = window.is_visible().unwrap_or(false);
    if hidden {
        let _ = window.hide();
        std::thread::sleep(std::time::Duration::from_millis(140));
    }
    let result = capture_screenshot_to(&repository, &master_key);
    if hidden {
        let _ = window.show();
        let _ = window.set_focus();
    }
    result
}

fn capture_screenshot_to(
    repository: &Repository,
    master_key: &MasterKey,
) -> Result<MediaAttachment, String> {
    let monitors = Monitor::all().map_err(|_| "无法读取屏幕列表".to_string())?;
    let monitor = monitors
        .iter()
        .find(|monitor| monitor.is_primary().unwrap_or(false))
        .or_else(|| monitors.first())
        .ok_or_else(|| "未找到可截图的显示器".to_string())?;
    let image = monitor
        .capture_image()
        .map_err(|_| "屏幕截图失败".to_string())?;
    let mut png = Vec::new();
    DynamicImage::ImageRgba8(image)
        .write_to(&mut Cursor::new(&mut png), ImageFormat::Png)
        .map_err(|_| "无法编码截图".to_string())?;
    let id = Uuid::new_v4();
    let attachment = repository
        .save_attachment(master_key, id, png.as_slice())
        .map_err(|_| "无法加密保存截图".to_string())?;
    described_media_attachment(
        repository,
        attachment,
        "image/png",
        format!("截图-{}.png", Utc::now().format("%Y%m%d-%H%M%S")),
        None,
    )
}

#[tauri::command]
fn pick_and_import_attachment(
    app: AppHandle,
    state: State<'_, DesktopState>,
) -> Result<Option<MediaAttachment>, String> {
    let selected = app
        .dialog()
        .file()
        .add_filter(
            "图片和视频",
            &[
                "png", "jpg", "jpeg", "webp", "gif", "mp4", "mov", "webm", "mkv",
            ],
        )
        .blocking_pick_file();
    let Some(selected) = selected else {
        return Ok(None);
    };
    let path = selected
        .into_path()
        .map_err(|_| "无法读取所选文件路径".to_string())?;
    let (repository, master_key) = crypto_context(&state)?;
    import_attachment_from_path(&repository, &master_key, &path).map(Some)
}

fn import_attachment_from_path(
    repository: &Repository,
    master_key: &MasterKey,
    path: &Path,
) -> Result<MediaAttachment, String> {
    let metadata = fs::metadata(path).map_err(|_| "无法读取所选文件".to_string())?;
    if metadata.len() == 0 || metadata.len() > MAX_IMPORTED_ATTACHMENT_BYTES {
        return Err("附件为空或超过 2 GiB".to_string());
    }
    let media_type = media_type_from_path(path)?;
    let display_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .map(sanitize_display_name)
        .ok_or_else(|| "附件文件名无效".to_string())?;
    let id = Uuid::new_v4();
    let input = File::open(path).map_err(|_| "无法打开所选文件".to_string())?;
    let attachment = repository
        .save_attachment(master_key, id, input)
        .map_err(|_| "无法加密导入附件".to_string())?;
    described_media_attachment(repository, attachment, media_type, display_name, None)
}

fn media_type_from_path(path: &Path) -> Result<&'static str, String> {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    match extension.as_str() {
        "png" => Ok("image/png"),
        "jpg" | "jpeg" => Ok("image/jpeg"),
        "webp" => Ok("image/webp"),
        "gif" => Ok("image/gif"),
        "mp4" => Ok("video/mp4"),
        "mov" => Ok("video/quicktime"),
        "webm" => Ok("video/webm"),
        "mkv" => Ok("video/x-matroska"),
        _ => Err("不支持该图片或视频格式".to_string()),
    }
}

fn sanitize_display_name(value: &str) -> String {
    let sanitized = value
        .chars()
        .filter(|character| !character.is_control())
        .take(180)
        .collect::<String>();
    if sanitized.trim().is_empty() {
        "附件".to_string()
    } else {
        sanitized
    }
}

fn build_input_stream<T>(
    device: &cpal::Device,
    config: &StreamConfig,
    samples: Arc<Mutex<Vec<i16>>>,
    overflowed: Arc<AtomicBool>,
    max_samples: usize,
    convert: fn(T) -> i16,
) -> Result<Stream, String>
where
    T: cpal::SizedSample + Copy + Send + 'static,
{
    let stream_overflow = overflowed.clone();
    device
        .build_input_stream(
            config,
            move |input: &[T], _| {
                if let Ok(mut output) = samples.try_lock() {
                    let available = max_samples.saturating_sub(output.len());
                    if input.len() > available {
                        stream_overflow.store(true, Ordering::Relaxed);
                    }
                    output.extend(input.iter().take(available).copied().map(convert));
                }
            },
            move |_| overflowed.store(true, Ordering::Relaxed),
            None,
        )
        .map_err(|_| "无法打开麦克风输入流".to_string())
}

fn sample_f32_to_i16(sample: f32) -> i16 {
    (sample.clamp(-1.0, 1.0) * i16::MAX as f32) as i16
}

fn sample_u16_to_i16(sample: u16) -> i16 {
    (sample as i32 - 32_768) as i16
}

#[tauri::command]
fn start_recording(state: State<'_, DesktopState>) -> Result<(), String> {
    let _ = crypto_context(&state)?;
    let mut active = state
        .recording
        .lock()
        .map_err(|_| "录音状态不可用".to_string())?;
    if active.is_some() {
        return Ok(());
    }
    *active = Some(new_recording_session()?);
    Ok(())
}

fn new_recording_session() -> Result<RecordingSession, String> {
    let device = cpal::default_host()
        .default_input_device()
        .ok_or_else(|| "未找到可用麦克风".to_string())?;
    let supported = device
        .default_input_config()
        .map_err(|_| "无法读取麦克风配置".to_string())?;
    let config: StreamConfig = supported.clone().into();
    let samples = Arc::new(Mutex::new(Vec::new()));
    let overflowed = Arc::new(AtomicBool::new(false));
    let max_samples =
        config.sample_rate.0 as usize * config.channels as usize * MAX_RECORDING_SECONDS;
    let stream = match supported.sample_format() {
        SampleFormat::F32 => build_input_stream(
            &device,
            &config,
            samples.clone(),
            overflowed.clone(),
            max_samples,
            sample_f32_to_i16,
        )?,
        SampleFormat::I16 => build_input_stream(
            &device,
            &config,
            samples.clone(),
            overflowed.clone(),
            max_samples,
            |sample: i16| sample,
        )?,
        SampleFormat::U16 => build_input_stream(
            &device,
            &config,
            samples.clone(),
            overflowed.clone(),
            max_samples,
            sample_u16_to_i16,
        )?,
        _ => return Err("麦克风采样格式暂不受支持".to_string()),
    };
    stream.play().map_err(|_| "无法开始录音".to_string())?;
    Ok(RecordingSession {
        stream,
        samples,
        overflowed,
        config,
        started_at: Instant::now(),
    })
}

#[tauri::command]
fn stop_recording(state: State<'_, DesktopState>) -> Result<MediaAttachment, String> {
    let session = state
        .recording
        .lock()
        .map_err(|_| "录音状态不可用".to_string())?
        .take()
        .ok_or_else(|| "当前没有正在进行的录音".to_string())?;
    let (repository, master_key) = crypto_context(&state)?;
    finish_recording_session(session, &repository, &master_key)
}

fn finish_recording_session(
    session: RecordingSession,
    repository: &Repository,
    master_key: &MasterKey,
) -> Result<MediaAttachment, String> {
    let duration = session.started_at.elapsed().as_secs().max(1);
    drop(session.stream);
    if session.overflowed.load(Ordering::Relaxed) {
        return Err("录音已达到 30 分钟上限或输入流发生中断".to_string());
    }
    let samples = session
        .samples
        .lock()
        .map_err(|_| "无法读取录音数据".to_string())?;
    if samples.is_empty() {
        return Err("没有采集到音频数据".to_string());
    }
    let wav = encode_wav(&samples, &session.config)?;
    drop(samples);
    let id = Uuid::new_v4();
    let attachment = repository
        .save_attachment(master_key, id, wav.as_slice())
        .map_err(|_| "无法加密保存录音".to_string())?;
    described_media_attachment(
        repository,
        attachment,
        "audio/wav",
        format!("录音-{}.wav", Utc::now().format("%Y%m%d-%H%M%S")),
        Some(duration),
    )
}

fn encode_wav(samples: &[i16], config: &StreamConfig) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    {
        let cursor = Cursor::new(&mut bytes);
        let mut writer = hound::WavWriter::new(
            cursor,
            hound::WavSpec {
                channels: config.channels,
                sample_rate: config.sample_rate.0,
                bits_per_sample: 16,
                sample_format: hound::SampleFormat::Int,
            },
        )
        .map_err(|_| "无法初始化 WAV 编码器".to_string())?;
        for sample in samples {
            writer
                .write_sample(*sample)
                .map_err(|_| "无法编码录音".to_string())?;
        }
        writer
            .finalize()
            .map_err(|_| "无法完成 WAV 编码".to_string())?;
    }
    Ok(bytes)
}

struct RangeCollector {
    cursor: u64,
    start: u64,
    end: u64,
    bytes: Vec<u8>,
}

impl std::io::Write for RangeCollector {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        let buffer_start = self.cursor;
        let buffer_end = buffer_start.saturating_add(buffer.len() as u64);
        let overlap_start = buffer_start.max(self.start);
        let overlap_end = buffer_end.min(self.end.saturating_add(1));
        if overlap_start < overlap_end {
            let from = (overlap_start - buffer_start) as usize;
            let to = (overlap_end - buffer_start) as usize;
            self.bytes.extend_from_slice(&buffer[from..to]);
        }
        self.cursor = buffer_end;
        Ok(buffer.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn attachment_plaintext_bytes(ciphertext_bytes: u64) -> Option<u64> {
    if ciphertext_bytes == ATTACHMENT_FRAME_OVERHEAD {
        return Some(0);
    }
    if ciphertext_bytes < ATTACHMENT_FRAME_OVERHEAD * 2 {
        return None;
    }
    let framed_payload = ciphertext_bytes.checked_sub(ATTACHMENT_FRAME_OVERHEAD)?;
    let frames = framed_payload.div_ceil(ATTACHMENT_CHUNK_BYTES + ATTACHMENT_FRAME_OVERHEAD);
    let plaintext = ciphertext_bytes.checked_sub((frames + 1) * ATTACHMENT_FRAME_OVERHEAD)?;
    if frames == 0
        || plaintext <= (frames - 1) * ATTACHMENT_CHUNK_BYTES
        || plaintext > frames * ATTACHMENT_CHUNK_BYTES
    {
        return None;
    }
    Some(plaintext)
}

fn protocol_media_type(path: &str) -> &'static str {
    match Path::new(path)
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "gif" => "image/gif",
        "wav" => "audio/wav",
        "mp4" => "video/mp4",
        "mov" => "video/quicktime",
        "webm" => "video/webm",
        "mkv" => "video/x-matroska",
        _ => "application/octet-stream",
    }
}

fn parse_protocol_range(value: Option<&str>, total: u64) -> Result<(u64, u64, bool), String> {
    if total == 0 {
        return Ok((0, 0, false));
    }
    let Some(value) = value else {
        let end = (total - 1).min(MAX_PROTOCOL_RANGE_BYTES - 1);
        return Ok((0, end, end + 1 < total));
    };
    let range = value
        .strip_prefix("bytes=")
        .and_then(|value| value.split(',').next())
        .ok_or_else(|| "无效的附件范围".to_string())?;
    let (start, requested_end) = range
        .split_once('-')
        .ok_or_else(|| "无效的附件范围".to_string())?;
    let (start, end) = if start.is_empty() {
        let suffix = requested_end
            .parse::<u64>()
            .map_err(|_| "无效的附件范围".to_string())?
            .min(total);
        (total - suffix, total - 1)
    } else {
        let start = start
            .parse::<u64>()
            .map_err(|_| "无效的附件范围".to_string())?;
        let end = if requested_end.is_empty() {
            total - 1
        } else {
            requested_end
                .parse::<u64>()
                .map_err(|_| "无效的附件范围".to_string())?
                .min(total - 1)
        };
        (start, end)
    };
    if start >= total || end < start {
        return Err("附件范围超出边界".to_string());
    }
    let capped_end = end.min(start.saturating_add(MAX_PROTOCOL_RANGE_BYTES - 1));
    Ok((start, capped_end, true))
}

fn protocol_error(
    status: tauri::http::StatusCode,
    message: &str,
) -> tauri::http::Response<Vec<u8>> {
    tauri::http::Response::builder()
        .status(status)
        .header(
            tauri::http::header::CONTENT_TYPE,
            "text/plain; charset=utf-8",
        )
        .header(tauri::http::header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
        .body(message.as_bytes().to_vec())
        .expect("static attachment error response is valid")
}

fn read_attachment_range(
    repository: &Repository,
    master_key: &MasterKey,
    id: Uuid,
    total: u64,
    start: u64,
    end: u64,
) -> Result<Vec<u8>, String> {
    let mut collector = RangeCollector {
        cursor: 0,
        start,
        end,
        bytes: if total == 0 {
            Vec::new()
        } else {
            Vec::with_capacity((end - start + 1) as usize)
        },
    };
    repository
        .read_attachment(master_key, id, &mut collector)
        .map_err(|_| "attachment authentication failed".to_string())?;
    let expected = if total == 0 { 0 } else { end - start + 1 };
    if collector.cursor != total || collector.bytes.len() as u64 != expected {
        return Err("attachment is incomplete".to_string());
    }
    Ok(collector.bytes)
}

fn attachment_protocol_response(
    app: &AppHandle,
    request: tauri::http::Request<Vec<u8>>,
) -> tauri::http::Response<Vec<u8>> {
    if request.method() != tauri::http::Method::GET && request.method() != tauri::http::Method::HEAD
    {
        return protocol_error(
            tauri::http::StatusCode::METHOD_NOT_ALLOWED,
            "method not allowed",
        );
    }
    let path = request.uri().path().trim_start_matches('/');
    let Some(id_segment) = path.split('/').next() else {
        return protocol_error(
            tauri::http::StatusCode::BAD_REQUEST,
            "missing attachment id",
        );
    };
    let Ok(id) = Uuid::parse_str(id_segment) else {
        return protocol_error(
            tauri::http::StatusCode::BAD_REQUEST,
            "invalid attachment id",
        );
    };
    let state = app.state::<DesktopState>();
    let Ok((repository, master_key)) = crypto_context(&state) else {
        return protocol_error(tauri::http::StatusCode::UNAUTHORIZED, "locked");
    };
    let Ok(ciphertext_bytes) = repository.attachment_ciphertext_bytes(id) else {
        return protocol_error(tauri::http::StatusCode::NOT_FOUND, "attachment not found");
    };
    let Some(total) = attachment_plaintext_bytes(ciphertext_bytes) else {
        return protocol_error(
            tauri::http::StatusCode::UNPROCESSABLE_ENTITY,
            "invalid attachment",
        );
    };
    let range_header = request
        .headers()
        .get(tauri::http::header::RANGE)
        .and_then(|value| value.to_str().ok());
    let Ok((start, end, partial)) = parse_protocol_range(range_header, total) else {
        return tauri::http::Response::builder()
            .status(tauri::http::StatusCode::RANGE_NOT_SATISFIABLE)
            .header(
                tauri::http::header::CONTENT_RANGE,
                format!("bytes */{total}"),
            )
            .header(tauri::http::header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
            .body(Vec::new())
            .expect("static range response is valid");
    };
    let expected = if total == 0 { 0 } else { end - start + 1 };
    let bytes = if request.method() == tauri::http::Method::HEAD {
        Vec::new()
    } else {
        match read_attachment_range(&repository, &master_key, id, total, start, end) {
            Ok(bytes) => bytes,
            Err(message) => {
                return protocol_error(tauri::http::StatusCode::UNPROCESSABLE_ENTITY, &message);
            }
        }
    };
    let mut response = tauri::http::Response::builder()
        .status(if partial {
            tauri::http::StatusCode::PARTIAL_CONTENT
        } else {
            tauri::http::StatusCode::OK
        })
        .header(tauri::http::header::CONTENT_TYPE, protocol_media_type(path))
        .header(tauri::http::header::ACCEPT_RANGES, "bytes")
        .header(tauri::http::header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
        .header(tauri::http::header::CACHE_CONTROL, "no-store")
        .header(tauri::http::header::CONTENT_LENGTH, expected.to_string());
    if partial && total > 0 {
        response = response.header(
            tauri::http::header::CONTENT_RANGE,
            format!("bytes {start}-{end}/{total}"),
        );
    }
    response
        .body(bytes)
        .expect("validated attachment response is valid")
}

fn server_url() -> String {
    std::env::var("SNAPLINE_SERVER_URL").unwrap_or_else(|_| DEFAULT_SERVER_URL.to_string())
}

fn app_data_dir(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map_err(|_| "无法定位应用数据目录".to_string())
}

fn show_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

fn open_capture_window(app: &AppHandle, kind: &str) -> Result<(), String> {
    let kind = match kind {
        "text" | "screenshot" | "audio" | "image" | "video" => kind,
        _ => return Err("无效的快速记录类型".to_string()),
    };
    if let Some(window) = app.get_webview_window("capture") {
        window
            .eval(format!("location.href = '/?capture={kind}'"))
            .map_err(|_| "无法切换快速记录类型".to_string())?;
        window
            .show()
            .map_err(|_| "无法显示快速记录窗口".to_string())?;
        let _ = window.unminimize();
        window
            .set_focus()
            .map_err(|_| "无法聚焦快速记录窗口".to_string())?;
        return Ok(());
    }
    WebviewWindowBuilder::new(
        app,
        "capture",
        WebviewUrl::App(format!("index.html?capture={kind}").into()),
    )
    .title("Snapline 快速记录")
    .inner_size(920.0, 640.0)
    .min_inner_size(620.0, 440.0)
    .center()
    .build()
    .map_err(|_| "无法创建快速记录窗口".to_string())?;
    Ok(())
}

fn setup_tray(app: &AppHandle) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "show", "打开 Snapline", true, None::<&str>)?;
    let capture = MenuItem::with_id(app, "capture", "新建快速记录", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &capture, &quit])?;
    let mut tray = TrayIconBuilder::with_id("snapline")
        .menu(&menu)
        .tooltip("Snapline")
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => show_main_window(app),
            "capture" => {
                let _ = open_capture_window(app, "text");
            }
            "quit" => app.exit(0),
            _ => {}
        });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    Ok(())
}

pub fn run() {
    let shortcuts = tauri_plugin_global_shortcut::Builder::new()
        .with_handler(|app, shortcut, event| {
            if event.state != ShortcutState::Pressed {
                return;
            }
            let modifiers = Modifiers::CONTROL | Modifiers::SHIFT;
            let kind = if shortcut.matches(modifiers, Code::Space) {
                Some("text")
            } else if shortcut.matches(modifiers, Code::Digit1) {
                Some("screenshot")
            } else if shortcut.matches(modifiers, Code::Digit2) {
                Some("audio")
            } else if shortcut.matches(modifiers, Code::KeyV) {
                Some("image")
            } else {
                None
            };
            if let Some(kind) = kind {
                let _ = open_capture_window(app, kind);
            }
        })
        .build();

    tauri::Builder::default()
        .register_uri_scheme_protocol("snapline-attachment", |context, request| {
            attachment_protocol_response(context.app_handle(), request)
        })
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            show_main_window(app);
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(shortcuts)
        .setup(|app| {
            let state = DesktopState::new(app_data_dir(app.handle())?, server_url())?;
            app.manage(state);
            setup_tray(app.handle())?;
            for shortcut in [
                "ctrl+shift+space",
                "ctrl+shift+1",
                "ctrl+shift+2",
                "ctrl+shift+v",
            ] {
                if let Err(error) = app.global_shortcut().register(shortcut) {
                    eprintln!("Snapline global shortcut {shortcut} is unavailable: {error}");
                }
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() == "main"
                && let WindowEvent::CloseRequested { api, .. } = event
            {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![
            auth_status,
            register_account,
            login_account,
            logout_account,
            list_items,
            save_item,
            delete_item,
            get_ai_config,
            set_ai_config,
            clear_ai_config,
            rebuild_ai_metadata,
            process_ai_queue,
            ask_agent,
            store_attachment_bytes,
            capture_screenshot,
            pick_and_import_attachment,
            start_recording,
            stop_recording
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

    #[test]
    fn wav_encoder_produces_readable_pcm_without_disk_files() {
        let config = StreamConfig {
            channels: 1,
            sample_rate: cpal::SampleRate(16_000),
            buffer_size: cpal::BufferSize::Default,
        };
        let samples = (0..16_000)
            .map(|index| ((index % 200) as i16 - 100) * 100)
            .collect::<Vec<_>>();
        let wav = encode_wav(&samples, &config).unwrap();
        let mut reader = hound::WavReader::new(Cursor::new(wav)).unwrap();
        assert_eq!(reader.spec().channels, 1);
        assert_eq!(reader.spec().sample_rate, 16_000);
        assert_eq!(
            reader
                .samples::<i16>()
                .collect::<Result<Vec<_>, _>>()
                .unwrap(),
            samples
        );
        assert_eq!(sample_f32_to_i16(-2.0), -i16::MAX);
        assert_eq!(sample_f32_to_i16(2.0), i16::MAX);
        assert_eq!(sample_u16_to_i16(0), i16::MIN);
        assert_eq!(sample_u16_to_i16(u16::MAX), i16::MAX);
    }

    #[test]
    fn attachment_import_validation_rejects_unknown_extensions_and_control_names() {
        assert_eq!(
            media_type_from_path(Path::new("capture.MP4")).unwrap(),
            "video/mp4"
        );
        assert!(media_type_from_path(Path::new("secret.exe")).is_err());
        assert_eq!(sanitize_display_name("a\0b\n.png"), "ab.png");
        assert_eq!(sanitize_display_name("\n\t"), "附件");
    }

    #[test]
    fn video_import_streams_through_encryption_and_round_trips() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("capture.mp4");
        let plaintext = (0..ATTACHMENT_CHUNK_BYTES as usize * 2 + 701)
            .map(|index| (index % 239) as u8)
            .collect::<Vec<_>>();
        fs::write(&path, &plaintext).unwrap();
        let repository = Repository::open(directory.path().join("snapline.db")).unwrap();
        let master_key = MasterKey::generate();
        let imported = import_attachment_from_path(&repository, &master_key, &path).unwrap();
        assert_eq!(imported.media_type, "video/mp4");
        assert_eq!(imported.display_name, "capture.mp4");
        let mut restored = Vec::new();
        repository
            .read_attachment(&master_key, imported.id, &mut restored)
            .unwrap();
        assert_eq!(restored, plaintext);
    }

    #[test]
    fn attachment_protocol_lengths_and_ranges_are_bounded() {
        for plaintext in [0, 1, ATTACHMENT_CHUNK_BYTES, ATTACHMENT_CHUNK_BYTES + 1] {
            let frames = if plaintext == 0 {
                0
            } else {
                plaintext.div_ceil(ATTACHMENT_CHUNK_BYTES)
            };
            let ciphertext = plaintext + (frames + 1) * ATTACHMENT_FRAME_OVERHEAD;
            assert_eq!(attachment_plaintext_bytes(ciphertext), Some(plaintext));
        }
        assert_eq!(
            parse_protocol_range(Some("bytes=10-19"), 100).unwrap(),
            (10, 19, true)
        );
        assert_eq!(
            parse_protocol_range(Some("bytes=-10"), 100).unwrap(),
            (90, 99, true)
        );
        assert!(parse_protocol_range(Some("bytes=100-"), 100).is_err());
        assert_eq!(
            parse_protocol_range(None, MAX_PROTOCOL_RANGE_BYTES + 1).unwrap(),
            (0, MAX_PROTOCOL_RANGE_BYTES - 1, true)
        );
    }

    #[test]
    fn attachment_protocol_decrypts_only_the_requested_plaintext_range() {
        let directory = tempfile::tempdir().unwrap();
        let repository = Repository::open(directory.path().join("snapline.db")).unwrap();
        let master_key = MasterKey::generate();
        let id = Uuid::new_v4();
        let plaintext = (0..ATTACHMENT_CHUNK_BYTES as usize * 2 + 137)
            .map(|index| (index % 251) as u8)
            .collect::<Vec<_>>();
        let attachment = repository
            .save_attachment(&master_key, id, plaintext.as_slice())
            .unwrap();
        let total = attachment_plaintext_bytes(attachment.ciphertext_bytes).unwrap();
        let start = ATTACHMENT_CHUNK_BYTES - 17;
        let end = ATTACHMENT_CHUNK_BYTES + 33;
        let range = read_attachment_range(&repository, &master_key, id, total, start, end).unwrap();
        assert_eq!(range, plaintext[start as usize..=end as usize]);
    }

    #[test]
    #[ignore = "requires SNAPLINE_MEDIA_TEST=1 and an interactive desktop"]
    fn live_windows_screen_is_encrypted_and_decodable() {
        assert_eq!(std::env::var("SNAPLINE_MEDIA_TEST").as_deref(), Ok("1"));
        let directory = tempfile::tempdir().unwrap();
        let repository = Repository::open(directory.path().join("snapline.db")).unwrap();
        let master_key = MasterKey::generate();

        let screenshot = capture_screenshot_to(&repository, &master_key).unwrap();
        let mut png = Vec::new();
        repository
            .read_attachment(&master_key, screenshot.id, &mut png)
            .unwrap();
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
        assert!(image::load_from_memory_with_format(&png, ImageFormat::Png).is_ok());
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
