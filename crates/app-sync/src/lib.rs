use anyhow::{anyhow, Result};
use serde::Serialize;
use snapline_app_core::{AppCore, SyncAccountState};
use snapline_domain::{
    crypto::{decrypt_bytes, decrypt_field_legacy_plaintext},
    AssetMetadata, Note,
};
use snapline_sync_client::{
    processor::{self, FullSyncContext, FullSyncReport},
    protocol::{AssetDownload, LoginRequest, LoginResponse, SnapshotResponse},
    HttpSyncApi, SyncApi,
};
use std::path::PathBuf;

const DESKTOP_DEVICE_NAME: &str = "Snapline Desktop";

#[derive(Debug, Clone, Serialize)]
pub struct LoginSyncResult {
    pub account: SyncAccountState,
    pub anonymous_note_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct SyncReport {
    pub uploaded_assets: usize,
    pub pushed: usize,
    pub pulled: usize,
    pub conflicts: usize,
    pub failed: usize,
    pub has_conflicts: bool,
    pub detail: String,
}

pub struct FullSyncInvocation {
    pub base_url: String,
    pub token: String,
    pub device_id: String,
    pub data_dir: PathBuf,
    pub db_path: PathBuf,
    pub dek: Option<[u8; 32]>,
}

impl SyncReport {
    pub fn from_full(report: FullSyncReport) -> Self {
        Self::new(
            report.uploaded_assets,
            report.pushed,
            report.pulled,
            report.conflicts,
            report.failed,
        )
    }

    pub fn new(
        uploaded_assets: usize,
        pushed: usize,
        pulled: usize,
        conflicts: usize,
        failed: usize,
    ) -> Self {
        let detail = format!(
            "uploaded_assets={}, pushed={}, pulled={}, conflicts={}, failed={}",
            uploaded_assets, pushed, pulled, conflicts, failed
        );
        Self {
            uploaded_assets,
            pushed,
            pulled,
            conflicts,
            failed,
            has_conflicts: conflicts > 0,
            detail,
        }
    }
}

pub fn prepare_register_request(
    core: &mut AppCore,
    email: &str,
    password: &str,
) -> Result<LoginRequest> {
    let device_id = core.sync_state()?.device_id;
    let (kek_salt, encrypted_dek) = core.generate_e2ee_material(password)?;
    Ok(LoginRequest {
        email: email.to_string(),
        password: password.to_string(),
        device_id,
        device_name: DESKTOP_DEVICE_NAME.to_string(),
        kek_salt: Some(kek_salt),
        encrypted_dek: Some(encrypted_dek),
    })
}

pub fn prepare_login_request(core: &AppCore, email: &str, password: &str) -> Result<LoginRequest> {
    let device_id = core.sync_state()?.device_id;
    Ok(LoginRequest {
        email: email.to_string(),
        password: password.to_string(),
        device_id,
        device_name: DESKTOP_DEVICE_NAME.to_string(),
        kek_salt: None,
        encrypted_dek: None,
    })
}

pub async fn register_with_server(
    server_base_url: &str,
    request: LoginRequest,
) -> Result<LoginResponse> {
    HttpSyncApi::new(server_base_url).register(request).await
}

pub async fn login_with_server(
    server_base_url: &str,
    request: LoginRequest,
) -> Result<LoginResponse> {
    HttpSyncApi::new(server_base_url).login(request).await
}

pub fn save_register_response(
    core: &mut AppCore,
    server_base_url: &str,
    response: &LoginResponse,
) -> Result<LoginSyncResult> {
    let account = core.save_sync_login(
        server_base_url,
        &response.account_id,
        &response.access_token,
        None,
        response.kek_salt.as_deref(),
        response.encrypted_dek.as_deref(),
    )?;
    login_sync_result(core, account)
}

pub fn save_login_response(
    core: &mut AppCore,
    server_base_url: &str,
    password: &str,
    response: &LoginResponse,
) -> Result<LoginSyncResult> {
    let account = core.save_sync_login(
        server_base_url,
        &response.account_id,
        &response.access_token,
        Some(password),
        response.kek_salt.as_deref(),
        response.encrypted_dek.as_deref(),
    )?;
    login_sync_result(core, account)
}

pub fn prepare_full_sync(core: &AppCore) -> Result<FullSyncInvocation> {
    let (base_url, token, device_id) = core
        .sync_credentials()?
        .ok_or_else(|| anyhow!("not logged in"))?;
    Ok(FullSyncInvocation {
        base_url,
        token,
        device_id,
        data_dir: core.data_dir().to_path_buf(),
        db_path: core.db_path().to_path_buf(),
        dek: core.dek().copied(),
    })
}

pub async fn run_prepared_full_sync(invocation: FullSyncInvocation) -> Result<SyncReport> {
    let api = HttpSyncApi::new(invocation.base_url);
    let report = processor::run_full_sync_from_path(
        &invocation.db_path,
        &api,
        FullSyncContext {
            token: &invocation.token,
            device_id: &invocation.device_id,
            data_dir: &invocation.data_dir,
            dek: invocation.dek.as_ref(),
        },
    )
    .await?;
    Ok(SyncReport::from_full(report))
}

pub async fn fetch_snapshot(server_base_url: &str, token: &str) -> Result<SnapshotResponse> {
    HttpSyncApi::new(server_base_url).snapshot(token).await
}

pub fn import_snapshot_and_find_missing_assets(
    core: &AppCore,
    snapshot: &SnapshotResponse,
) -> Result<Vec<AssetMetadata>> {
    let notes = snapshot_notes_plaintext(core, &snapshot.notes)?;
    core.import_snapshot(&notes, snapshot.cursor)?;
    Ok(core.missing_asset_metadata(&snapshot.assets))
}

pub async fn download_asset(
    server_base_url: &str,
    token: &str,
    asset: &AssetMetadata,
) -> Result<AssetDownload> {
    HttpSyncApi::new(server_base_url)
        .download_asset(token, &asset.id.to_string())
        .await
}

pub fn save_remote_asset(
    core: &AppCore,
    asset: &AssetMetadata,
    downloaded: &AssetDownload,
) -> Result<()> {
    let bytes = match core.dek() {
        Some(key) => decrypt_bytes(key, &downloaded.bytes)?,
        None => downloaded.bytes.clone(),
    };
    core.save_remote_asset(asset, &bytes)
}

fn login_sync_result(core: &AppCore, account: SyncAccountState) -> Result<LoginSyncResult> {
    Ok(LoginSyncResult {
        account,
        anonymous_note_count: core.anonymous_note_count()?,
    })
}

fn snapshot_notes_plaintext(core: &AppCore, notes: &[Note]) -> Result<Vec<Note>> {
    match core.dek() {
        Some(key) => notes
            .iter()
            .map(|note| {
                Ok(Note {
                    title: decrypt_field_legacy_plaintext(key, &note.title)?,
                    content_md: decrypt_field_legacy_plaintext(key, &note.content_md)?,
                    ..note.clone()
                })
            })
            .collect(),
        None => Ok(notes.to_vec()),
    }
}
