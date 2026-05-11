mod windows;

use serde::{Deserialize, Serialize};
use snapline_app_core::{AppCore, BootstrapState, SyncAccountState};
use snapline_domain::{AssetRef, MarkdownImageMapping, Note, NoteId, NoteSummary};
use snapline_platform::AppPaths;
use snapline_sync_client::{
    processor::{self, FullSyncContext, FullSyncReport},
    protocol::LoginRequest,
    HttpSyncApi, SyncApi,
};
#[cfg(desktop)]
use std::borrow::Cow;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager, State};
#[cfg(desktop)]
use tauri::{RunEvent, WindowEvent};
#[cfg(desktop)]
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};
#[cfg(desktop)]
use windows::show_main_window;
use windows::{
    build_note_window, close_other_note_windows, hide_main_window, reveal_window, WindowPosition,
};

const AUTOSTART_BACKGROUND_ARG: &str = "--background";
const BACKGROUND_SYNC_INTERVAL_SECS: u64 = 60;

struct AppState {
    core: Mutex<AppCore>,
    launched_in_background: bool,
    startup_logging_enabled: bool,
}

#[derive(Debug, Clone, Serialize)]
struct LoginSyncResult {
    account: SyncAccountState,
    anonymous_note_count: usize,
}

#[derive(Debug, Clone, Serialize)]
struct DraftPartsDto {
    title: String,
    body_md: String,
}

#[derive(Debug, Clone, Serialize)]
struct HydratedMarkdownDto {
    markdown: String,
    mappings: Vec<MarkdownImageMapping>,
}

#[derive(Debug, Clone, Deserialize)]
struct SaveDraftRequest {
    id: Option<String>,
    title: String,
    body_md: String,
    pinned: bool,
}

#[derive(Debug, Clone, Serialize)]
struct SaveDraftResult {
    note: Option<Note>,
    skipped: bool,
}

#[derive(Debug, Clone, Serialize)]
struct SyncReport {
    uploaded_assets: usize,
    pushed: usize,
    pulled: usize,
    conflicts: usize,
    failed: usize,
    has_conflicts: bool,
    detail: String,
}

impl SyncReport {
    fn from_full(report: FullSyncReport) -> Self {
        Self::new(
            report.uploaded_assets,
            report.pushed,
            report.pulled,
            report.conflicts,
            report.failed,
        )
    }

    fn new(
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

#[tauri::command]
fn log_startup(state: State<'_, AppState>, message: String) {
    if state.startup_logging_enabled {
        eprintln!("{message}");
    }
}

#[tauri::command]
fn launched_in_background(state: State<'_, AppState>) -> bool {
    state.launched_in_background
}

#[tauri::command]
fn bootstrap(state: State<'_, AppState>) -> Result<BootstrapState, String> {
    let started = std::time::Instant::now();
    let result = state
        .core
        .lock()
        .map_err(|_| "app state lock poisoned".to_string())?
        .bootstrap()
        .map_err(|err| err.to_string());
    eprintln!("snapline.bootstrap_ms={}", started.elapsed().as_millis());
    result
}

#[tauri::command]
fn create_note(state: State<'_, AppState>) -> Result<Note, String> {
    state
        .core
        .lock()
        .map_err(|_| "app state lock poisoned".to_string())?
        .create_note()
        .map_err(|err| err.to_string())
}

#[tauri::command]
fn get_note(state: State<'_, AppState>, id: String) -> Result<Note, String> {
    let id = parse_note_id(&id)?;
    state
        .core
        .lock()
        .map_err(|_| "app state lock poisoned".to_string())?
        .get_note(&id)
        .map_err(|err| err.to_string())
}

#[tauri::command]
fn derive_title_from_markdown(
    state: State<'_, AppState>,
    markdown: String,
) -> Result<String, String> {
    Ok(state
        .core
        .lock()
        .map_err(|_| "app state lock poisoned".to_string())?
        .derive_title_from_markdown(&markdown))
}

#[tauri::command]
fn compose_draft_markdown(
    state: State<'_, AppState>,
    title: String,
    body_md: String,
) -> Result<String, String> {
    Ok(state
        .core
        .lock()
        .map_err(|_| "app state lock poisoned".to_string())?
        .compose_draft_markdown(&title, &body_md))
}

#[tauri::command]
fn split_draft_markdown(
    state: State<'_, AppState>,
    markdown: String,
) -> Result<DraftPartsDto, String> {
    let parts = state
        .core
        .lock()
        .map_err(|_| "app state lock poisoned".to_string())?
        .split_draft_markdown(&markdown);
    Ok(DraftPartsDto {
        title: parts.title,
        body_md: parts.body_md,
    })
}

#[tauri::command]
fn split_stored_note_markdown(
    state: State<'_, AppState>,
    stored_title: String,
    markdown: String,
) -> Result<DraftPartsDto, String> {
    let parts = state
        .core
        .lock()
        .map_err(|_| "app state lock poisoned".to_string())?
        .split_stored_note_markdown(&stored_title, &markdown);
    Ok(DraftPartsDto {
        title: parts.title,
        body_md: parts.body_md,
    })
}

#[tauri::command]
fn prepare_draft_for_save(
    state: State<'_, AppState>,
    title: String,
    body_md: String,
) -> Result<DraftPartsDto, String> {
    let parts = state
        .core
        .lock()
        .map_err(|_| "app state lock poisoned".to_string())?
        .prepare_draft_for_save(&title, &body_md);
    Ok(DraftPartsDto {
        title: parts.title,
        body_md: parts.body_md,
    })
}

#[tauri::command]
fn normalize_markdown(state: State<'_, AppState>, markdown: String) -> Result<String, String> {
    Ok(state
        .core
        .lock()
        .map_err(|_| "app state lock poisoned".to_string())?
        .normalize_markdown(&markdown))
}

#[tauri::command]
fn asset_url_from_markdown_path(
    state: State<'_, AppState>,
    markdown_path: String,
) -> Result<String, String> {
    Ok(state
        .core
        .lock()
        .map_err(|_| "app state lock poisoned".to_string())?
        .asset_url_from_markdown_path(&markdown_path))
}

#[tauri::command]
fn markdown_path_from_asset_url(
    state: State<'_, AppState>,
    asset_url: String,
) -> Result<String, String> {
    Ok(state
        .core
        .lock()
        .map_err(|_| "app state lock poisoned".to_string())?
        .markdown_path_from_asset_url(&asset_url))
}

#[tauri::command]
fn hydrate_markdown_assets(
    state: State<'_, AppState>,
    markdown: String,
) -> Result<HydratedMarkdownDto, String> {
    let hydrated = state
        .core
        .lock()
        .map_err(|_| "app state lock poisoned".to_string())?
        .hydrate_markdown_assets(&markdown);
    Ok(HydratedMarkdownDto {
        markdown: hydrated.markdown,
        mappings: hydrated.mappings,
    })
}

#[tauri::command]
fn restore_markdown_asset_sources(
    state: State<'_, AppState>,
    markdown: String,
    mappings: Vec<MarkdownImageMapping>,
) -> Result<String, String> {
    Ok(state
        .core
        .lock()
        .map_err(|_| "app state lock poisoned".to_string())?
        .restore_markdown_asset_sources(&markdown, &mappings))
}

#[tauri::command]
fn get_note_summary(state: State<'_, AppState>, id: String) -> Result<NoteSummary, String> {
    let id = parse_note_id(&id)?;
    state
        .core
        .lock()
        .map_err(|_| "app state lock poisoned".to_string())?
        .get_note_summary(&id)
        .map_err(|err| err.to_string())
}

#[tauri::command]
fn search_notes(state: State<'_, AppState>, query: String) -> Result<Vec<NoteSummary>, String> {
    state
        .core
        .lock()
        .map_err(|_| "app state lock poisoned".to_string())?
        .search_notes(&query)
        .map_err(|err| err.to_string())
}

#[tauri::command]
fn save_note(
    state: State<'_, AppState>,
    id: String,
    title: String,
    content_md: String,
    pinned: bool,
) -> Result<Note, String> {
    let id = parse_note_id(&id)?;
    let started = std::time::Instant::now();
    let result = state
        .core
        .lock()
        .map_err(|_| "app state lock poisoned".to_string())?
        .save_note(&id, &title, &content_md, pinned)
        .map_err(|err| err.to_string());
    eprintln!("snapline.save_note_ms={}", started.elapsed().as_millis());
    result
}

#[tauri::command]
fn save_draft_session(
    state: State<'_, AppState>,
    request: SaveDraftRequest,
) -> Result<SaveDraftResult, String> {
    let core = state
        .core
        .lock()
        .map_err(|_| "app state lock poisoned".to_string())?;

    if !core.has_meaningful_draft_content(&request.title, &request.body_md) {
        return Ok(SaveDraftResult {
            note: None,
            skipped: true,
        });
    }

    let prepared = core.prepare_draft_for_save(&request.title, &request.body_md);
    let note_id = match request.id {
        Some(id) => parse_note_id(&id)?,
        None => core.create_note().map_err(|err| err.to_string())?.id,
    };
    let note = core
        .save_note(&note_id, &prepared.title, &prepared.body_md, request.pinned)
        .map_err(|err| err.to_string())?;

    Ok(SaveDraftResult {
        note: Some(note),
        skipped: false,
    })
}

#[tauri::command]
fn set_note_title(state: State<'_, AppState>, id: String, title: String) -> Result<Note, String> {
    let id = parse_note_id(&id)?;
    state
        .core
        .lock()
        .map_err(|_| "app state lock poisoned".to_string())?
        .set_note_title(&id, &title)
        .map_err(|err| err.to_string())
}

#[tauri::command]
fn set_note_pinned(state: State<'_, AppState>, id: String, pinned: bool) -> Result<Note, String> {
    let id = parse_note_id(&id)?;
    let note = state
        .core
        .lock()
        .map_err(|_| "app state lock poisoned".to_string())?
        .set_note_pinned(&id, pinned)
        .map_err(|err| err.to_string())?;
    Ok(note)
}

#[tauri::command]
fn delete_note(state: State<'_, AppState>, id: String) -> Result<Vec<NoteSummary>, String> {
    let id = parse_note_id(&id)?;
    state
        .core
        .lock()
        .map_err(|_| "app state lock poisoned".to_string())?
        .delete_note(&id)
        .map_err(|err| err.to_string())
}

#[tauri::command]
fn save_png_asset(
    state: State<'_, AppState>,
    note_id: String,
    bytes: Vec<u8>,
) -> Result<AssetRef, String> {
    let note_id = parse_note_id(&note_id)?;
    state
        .core
        .lock()
        .map_err(|_| "app state lock poisoned".to_string())?
        .save_png_asset(&note_id, &bytes)
        .map_err(|err| err.to_string())
}

#[tauri::command]
fn resolve_asset_url(state: State<'_, AppState>, markdown_path: String) -> Result<String, String> {
    let resolved = state
        .core
        .lock()
        .map_err(|_| "app state lock poisoned".to_string())?
        .resolve_asset_url(&markdown_path);
    Ok(resolved)
}

#[tauri::command]
async fn open_note_window(
    app: AppHandle,
    note_id: Option<String>,
    position: Option<WindowPosition>,
) -> Result<String, String> {
    if let Some(id) = note_id {
        let _ = parse_note_id(&id)?;
        let label = format!("note-{id}");
        if let Some(window) = app.get_webview_window(&label) {
            reveal_window(&window, position.as_ref())?;
            close_other_note_windows(&app, &label);
            return Ok(label);
        }

        let label = build_note_window(&app, &label, &format!("/?mode=note&noteId={id}"), position)?;
        close_other_note_windows(&app, &label);
        Ok(label)
    } else {
        let label = format!("note-{}", uuid::Uuid::new_v4().simple());
        let label = build_note_window(&app, &label, "/?mode=note&newDraft=1", position)?;
        close_other_note_windows(&app, &label);
        Ok(label)
    }
}

#[tauri::command]
fn read_asset_bytes(state: State<'_, AppState>, markdown_path: String) -> Result<Vec<u8>, String> {
    if !is_allowed_markdown_asset_path(&markdown_path) {
        return Err("unsupported asset path".to_string());
    }

    let path = state
        .core
        .lock()
        .map_err(|_| "app state lock poisoned".to_string())?
        .resolve_asset_path(&markdown_path);

    std::fs::read(path).map_err(|err| err.to_string())
}
#[tauri::command]
fn read_local_image_file(path: String) -> Result<Vec<u8>, String> {
    if !is_allowed_local_image_path(&path) {
        return Err("unsupported image path".to_string());
    }

    std::fs::read(path).map_err(|err| err.to_string())
}

#[cfg(desktop)]
#[tauri::command]
fn read_clipboard_image_png() -> Result<Option<Vec<u8>>, String> {
    let mut clipboard = arboard::Clipboard::new().map_err(|err| err.to_string())?;
    match clipboard.get_image() {
        Ok(image) => encode_clipboard_image_as_png(image).map(Some),
        Err(arboard::Error::ContentNotAvailable) => Ok(None),
        Err(err) => Err(err.to_string()),
    }
}

#[cfg(not(desktop))]
#[tauri::command]
fn read_clipboard_image_png() -> Result<Option<Vec<u8>>, String> {
    Ok(None)
}

fn is_allowed_markdown_asset_path(markdown_path: &str) -> bool {
    markdown_path.starts_with("assets/")
        && !markdown_path.contains('\\')
        && !markdown_path
            .split('/')
            .any(|segment| segment.is_empty() || segment == "." || segment == "..")
}
fn is_allowed_local_image_path(path: &str) -> bool {
    let path = std::path::Path::new(path);
    path.is_absolute()
        && path.is_file()
        && path
            .extension()
            .and_then(|extension| extension.to_str())
            .map(|extension| {
                matches!(
                    extension.to_ascii_lowercase().as_str(),
                    "png" | "jpg" | "jpeg" | "webp" | "gif" | "bmp"
                )
            })
            .unwrap_or(false)
}

#[cfg(desktop)]
fn encode_clipboard_image_as_png(image: arboard::ImageData<'_>) -> Result<Vec<u8>, String> {
    let rgba = clipboard_image_rgba_bytes(image.bytes);
    let mut output = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut output, image.width as u32, image.height as u32);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().map_err(|err| err.to_string())?;
        writer
            .write_image_data(&rgba)
            .map_err(|err| err.to_string())?;
    }
    Ok(output)
}

#[cfg(desktop)]
fn clipboard_image_rgba_bytes(bytes: Cow<'_, [u8]>) -> Vec<u8> {
    bytes.into_owned()
}

#[cfg(all(test, desktop))]
fn test_image_data(width: usize, height: usize, bytes: Vec<u8>) -> arboard::ImageData<'static> {
    arboard::ImageData {
        width,
        height,
        bytes: Cow::Owned(bytes),
    }
}

#[tauri::command]
fn export_note_as_markdown(state: State<'_, AppState>, id: String) -> Result<String, String> {
    let note_id = parse_note_id(&id)?;
    let note = state
        .core
        .lock()
        .map_err(|_| "app state lock poisoned".to_string())?
        .get_note(&note_id)
        .map_err(|err| err.to_string())?;

    let filename = if note.title.trim().is_empty() {
        "Untitled.md".to_string()
    } else {
        let safe: String = note
            .title
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || matches!(c, ' ' | '-' | '_' | '.') {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        format!("{}.md", safe.trim())
    };

    let downloads = dirs::download_dir()
        .or_else(|| dirs::home_dir().map(|h| h.join("Downloads")))
        .ok_or_else(|| "could not find downloads directory".to_string())?;

    std::fs::create_dir_all(&downloads).map_err(|err| err.to_string())?;
    let dest = downloads.join(&filename);
    std::fs::write(&dest, note.content_md.as_bytes()).map_err(|err| err.to_string())?;
    Ok(dest.to_string_lossy().into_owned())
}

#[tauri::command]
fn open_external_url(url: String) -> Result<String, String> {
    if !is_allowed_external_url(&url) {
        return Err("unsupported external URL".to_string());
    }

    open_url_with_system(&url)?;
    Ok(url)
}

fn is_allowed_external_url(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    !url.chars()
        .any(|character| character == '\r' || character == '\n')
        && (lower.starts_with("http://")
            || lower.starts_with("https://")
            || lower.starts_with("mailto:"))
}

#[cfg(target_os = "windows")]
fn open_url_with_system(url: &str) -> Result<(), String> {
    std::process::Command::new("explorer.exe")
        .arg(url)
        .spawn()
        .map_err(|err| err.to_string())?;
    Ok(())
}

#[cfg(target_os = "macos")]
fn open_url_with_system(url: &str) -> Result<(), String> {
    std::process::Command::new("open")
        .arg(url)
        .spawn()
        .map_err(|err| err.to_string())?;
    Ok(())
}

#[cfg(all(unix, not(target_os = "macos")))]
fn open_url_with_system(url: &str) -> Result<(), String> {
    std::process::Command::new("xdg-open")
        .arg(url)
        .spawn()
        .map_err(|err| err.to_string())?;
    Ok(())
}

#[tauri::command]
fn get_open_shortcut(state: State<'_, AppState>) -> Result<String, String> {
    state
        .core
        .lock()
        .map_err(|_| "app state lock poisoned".to_string())?
        .get_open_shortcut()
        .map_err(|err| err.to_string())
}

#[tauri::command]
fn set_open_shortcut(
    app: AppHandle,
    state: State<'_, AppState>,
    shortcut: String,
) -> Result<String, String> {
    state
        .core
        .lock()
        .map_err(|_| "app state lock poisoned".to_string())?
        .set_open_shortcut(&shortcut)
        .map_err(|err| err.to_string())?;
    register_open_shortcut(&app, &shortcut)?;
    Ok(shortcut)
}

#[tauri::command]
fn get_sync_account_state(state: State<'_, AppState>) -> Result<SyncAccountState, String> {
    state
        .core
        .lock()
        .map_err(|_| "app state lock poisoned".to_string())?
        .sync_account_state()
        .map_err(|err| err.to_string())
}

#[tauri::command]
async fn register_sync(
    state: State<'_, AppState>,
    server_base_url: String,
    email: String,
    password: String,
) -> Result<LoginSyncResult, String> {
    let (device_id, kek_salt, encrypted_dek) = {
        let mut core = state
            .core
            .lock()
            .map_err(|_| "app state lock poisoned".to_string())?;
        let device_id = core.sync_state().map_err(|err| err.to_string())?.device_id;
        let (kek_salt, encrypted_dek) = core
            .generate_e2ee_material(&password)
            .map_err(|err| err.to_string())?;
        (device_id, kek_salt, encrypted_dek)
    };
    let api = HttpSyncApi::new(&server_base_url);
    let response = api
        .register(LoginRequest {
            email: email.clone(),
            password: password.clone(),
            device_id,
            device_name: "Snapline Desktop".to_string(),
            kek_salt: Some(kek_salt),
            encrypted_dek: Some(encrypted_dek),
        })
        .await
        .map_err(|err| err.to_string())?;
    let (account_state, anonymous_note_count) = {
        let mut core = state
            .core
            .lock()
            .map_err(|_| "app state lock poisoned".to_string())?;
        let account_state = core
            .save_sync_login(
                &server_base_url,
                &response.account_id,
                &response.access_token,
                None, // DEK already in memory from generate_e2ee_material
                response.kek_salt.as_deref(),
                response.encrypted_dek.as_deref(),
            )
            .map_err(|err| err.to_string())?;
        let anonymous_note_count = core.anonymous_note_count().map_err(|err| err.to_string())?;
        (account_state, anonymous_note_count)
    };
    import_snapshot_and_assets(&state, &api, &response.access_token).await?;
    Ok(LoginSyncResult {
        account: account_state,
        anonymous_note_count,
    })
}

#[tauri::command]
async fn login_sync(
    state: State<'_, AppState>,
    server_base_url: String,
    email: String,
    password: String,
) -> Result<LoginSyncResult, String> {
    let device_id = state
        .core
        .lock()
        .map_err(|_| "app state lock poisoned".to_string())?
        .sync_state()
        .map_err(|err| err.to_string())?
        .device_id;
    let api = HttpSyncApi::new(&server_base_url);
    let response = api
        .login(LoginRequest {
            email: email.clone(),
            password: password.clone(),
            device_id,
            device_name: "Snapline Desktop".to_string(),
            kek_salt: None,
            encrypted_dek: None,
        })
        .await
        .map_err(|err| err.to_string())?;
    let (account_state, anonymous_note_count) = {
        let mut core = state
            .core
            .lock()
            .map_err(|_| "app state lock poisoned".to_string())?;
        let account_state = core
            .save_sync_login(
                &server_base_url,
                &response.account_id,
                &response.access_token,
                Some(&password),
                response.kek_salt.as_deref(),
                response.encrypted_dek.as_deref(),
            )
            .map_err(|err| err.to_string())?;
        let anonymous_note_count = core.anonymous_note_count().map_err(|err| err.to_string())?;
        (account_state, anonymous_note_count)
    };
    import_snapshot_and_assets(&state, &api, &response.access_token).await?;
    Ok(LoginSyncResult {
        account: account_state,
        anonymous_note_count,
    })
}

#[tauri::command]
fn anonymous_note_count(state: State<'_, AppState>) -> Result<usize, String> {
    state
        .core
        .lock()
        .map_err(|_| "app state lock poisoned".to_string())?
        .anonymous_note_count()
        .map_err(|err| err.to_string())
}

#[tauri::command]
fn import_anonymous_notes(state: State<'_, AppState>) -> Result<Vec<NoteSummary>, String> {
    state
        .core
        .lock()
        .map_err(|_| "app state lock poisoned".to_string())?
        .import_anonymous_notes_to_current_account()
        .map_err(|err| err.to_string())
}

#[tauri::command]
async fn sync_now(state: State<'_, AppState>) -> Result<SyncReport, String> {
    run_sync_once(state.inner()).await
}

async fn run_sync_once(state: &AppState) -> Result<SyncReport, String> {
    let (base_url, token, device_id, data_dir, db_path, dek) = {
        let core = state
            .core
            .lock()
            .map_err(|_| "app state lock poisoned".to_string())?;
        let (base_url, token, device_id) = core
            .sync_credentials()
            .map_err(|err| err.to_string())?
            .ok_or_else(|| "not logged in".to_string())?;
        (
            base_url,
            token,
            device_id,
            core.data_dir().to_path_buf(),
            core.db_path().to_path_buf(),
            core.dek().copied(),
        )
    };
    let api = HttpSyncApi::new(base_url);
    let report = processor::run_full_sync_from_path(
        &db_path,
        &api,
        FullSyncContext {
            token: &token,
            device_id: &device_id,
            data_dir: &data_dir,
            dek: dek.as_ref(),
        },
    )
    .await
    .map_err(|err| err.to_string())?;
    Ok(SyncReport::from_full(report))
}

async fn import_snapshot_and_assets(
    state: &AppState,
    api: &HttpSyncApi,
    token: &str,
) -> Result<(), String> {
    let snapshot = api.snapshot(token).await.map_err(|err| err.to_string())?;
    let missing_assets = {
        let core = state
            .core
            .lock()
            .map_err(|_| "app state lock poisoned".to_string())?;
        let notes = match core.dek() {
            Some(key) => snapshot
                .notes
                .iter()
                .map(|note| {
                    Ok(Note {
                        title: snapline_domain::crypto::decrypt_field_legacy_plaintext(
                            key,
                            &note.title,
                        )
                        .map_err(|err| err.to_string())?,
                        content_md: snapline_domain::crypto::decrypt_field_legacy_plaintext(
                            key,
                            &note.content_md,
                        )
                        .map_err(|err| err.to_string())?,
                        ..note.clone()
                    })
                })
                .collect::<Result<Vec<_>, String>>()?,
            None => snapshot.notes.clone(),
        };
        core.import_snapshot(&notes, snapshot.cursor)
            .map_err(|err| err.to_string())?;
        core.missing_asset_metadata(&snapshot.assets)
    };
    for asset in missing_assets {
        let downloaded = api
            .download_asset(token, &asset.id.to_string())
            .await
            .map_err(|err| err.to_string())?;
        let core = state
            .core
            .lock()
            .map_err(|_| "app state lock poisoned".to_string())?;
        let bytes = match core.dek() {
            Some(key) => snapline_domain::crypto::decrypt_bytes(key, &downloaded.bytes)
                .map_err(|err| err.to_string())?,
            None => downloaded.bytes,
        };
        core.save_remote_asset(&asset, &bytes)
            .map_err(|err| err.to_string())?;
    }
    Ok(())
}

async fn run_background_sync_loop(app: AppHandle) {
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(
        BACKGROUND_SYNC_INTERVAL_SECS,
    ));
    loop {
        interval.tick().await;
        let state = app.state::<AppState>();
        match run_sync_once(state.inner()).await {
            Ok(report) => {
                let _ = app.emit("sync-status", report);
            }
            Err(err) if err == "not logged in" => {}
            Err(err) => {
                eprintln!("snapline.background_sync_error={err}");
                let _ = app.emit("sync-error", err);
            }
        }
    }
}

fn parse_note_id(value: &str) -> Result<NoteId, String> {
    uuid::Uuid::parse_str(value)
        .map(NoteId)
        .map_err(|err| format!("invalid note id: {err}"))
}

#[cfg(desktop)]
fn register_open_shortcut(app: &AppHandle, shortcut: &str) -> Result<(), String> {
    app.global_shortcut()
        .unregister_all()
        .map_err(|err| err.to_string())?;
    app.global_shortcut()
        .on_shortcut(shortcut, |app, _shortcut, event| {
            if event.state == ShortcutState::Pressed {
                show_main_window(app);
            }
        })
        .map_err(|err| err.to_string())
}

#[cfg(not(desktop))]
fn register_open_shortcut(_app: &AppHandle, shortcut: &str) -> Result<(), String> {
    if shortcut.trim().is_empty() {
        return Err("shortcut cannot be empty".to_string());
    }
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app_started = std::time::Instant::now();
    let startup_logging_enabled =
        std::env::var("SNAPLINE_STARTUP_LOG").ok().as_deref() == Some("1");
    let should_launch_in_background = std::env::args().any(|arg| arg == AUTOSTART_BACKGROUND_ARG);
    if startup_logging_enabled {
        eprintln!("snapline.startup event=rust_main");
    }
    let builder = tauri::Builder::default();
    #[cfg(desktop)]
    let builder = builder
        .plugin(
            tauri_plugin_autostart::Builder::new()
                .args([AUTOSTART_BACKGROUND_ARG])
                .app_name("Snapline")
                .build(),
        )
        .plugin(tauri_plugin_global_shortcut::Builder::new().build());

    builder
        .setup(move |app| {
            let setup_started = std::time::Instant::now();
            if startup_logging_enabled {
                eprintln!(
                    "snapline.startup event=setup_started elapsed_ms={}",
                    app_started.elapsed().as_millis()
                );
            }
            let paths = AppPaths::resolve()
                .unwrap_or_else(|_| AppPaths::from_data_dir(std::env::temp_dir().join("Snapline")));
            let core = AppCore::open(paths).expect("open app core");
            app.manage(AppState {
                core: Mutex::new(core),
                launched_in_background: should_launch_in_background,
                startup_logging_enabled,
            });
            if startup_logging_enabled {
                eprintln!(
                    "snapline.startup event=app_core_opened elapsed_ms={} duration_ms={}",
                    app_started.elapsed().as_millis(),
                    setup_started.elapsed().as_millis()
                );
            }
            let app_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let shortcut_started = std::time::Instant::now();
                let shortcut = app_handle
                    .state::<AppState>()
                    .core
                    .lock()
                    .map_err(|_| "app state lock poisoned".to_string())
                    .and_then(|core| core.get_open_shortcut().map_err(|err| err.to_string()))
                    .unwrap_or_else(|_| "Ctrl+Shift+Space".to_string());
                if let Err(err) = register_open_shortcut(&app_handle, &shortcut) {
                    eprintln!("snapline.register_open_shortcut_error={err}");
                }
                if startup_logging_enabled {
                    eprintln!(
                        "snapline.startup event=shortcut_registered duration_ms={}",
                        shortcut_started.elapsed().as_millis()
                    );
                }
            });
            let sync_app_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                run_background_sync_loop(sync_app_handle).await;
            });
            if startup_logging_enabled {
                eprintln!(
                    "snapline.startup event=setup_finished elapsed_ms={} duration_ms={}",
                    app_started.elapsed().as_millis(),
                    setup_started.elapsed().as_millis()
                );
            }
            if should_launch_in_background {
                hide_main_window(app.handle());
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            log_startup,
            launched_in_background,
            bootstrap,
            derive_title_from_markdown,
            compose_draft_markdown,
            split_draft_markdown,
            split_stored_note_markdown,
            prepare_draft_for_save,
            normalize_markdown,
            hydrate_markdown_assets,
            restore_markdown_asset_sources,
            create_note,
            get_note,
            get_note_summary,
            search_notes,
            save_note,
            save_draft_session,
            set_note_title,
            set_note_pinned,
            delete_note,
            save_png_asset,
            read_asset_bytes,
            read_local_image_file,
            read_clipboard_image_png,
            resolve_asset_url,
            open_note_window,
            asset_url_from_markdown_path,
            markdown_path_from_asset_url,
            export_note_as_markdown,
            open_external_url,
            get_open_shortcut,
            set_open_shortcut,
            get_sync_account_state,
            register_sync,
            login_sync,
            anonymous_note_count,
            import_anonymous_notes,
            sync_now
        ])
        .build(tauri::generate_context!())
        .expect("error while building Snapline")
        .run(move |app, event| handle_run_event(app, event, should_launch_in_background));
}

#[cfg(desktop)]
fn handle_run_event(app: &AppHandle, event: RunEvent, should_launch_in_background: bool) {
    match event {
        RunEvent::Ready if should_launch_in_background => hide_main_window(app),
        RunEvent::WindowEvent {
            label,
            event: WindowEvent::CloseRequested { api, .. },
            ..
        } => {
            if label == "main" {
                api.prevent_close();
                hide_main_window(app);
            }
        }
        _ => {}
    }
}

#[cfg(not(desktop))]
fn handle_run_event(_app: &AppHandle, _event: tauri::RunEvent, _should_launch_in_background: bool) {
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn bootstrap_starts_with_blank_draft_note() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(dir.path());
        let core = AppCore::open(paths).unwrap();
        let state = core.bootstrap().unwrap();

        assert!(state.notes.is_empty());
        assert_eq!(state.current.title, "Untitled");
        assert!(!state.current.pinned);
    }

    #[test]
    fn saves_png_asset_under_note_directory() {
        let dir = tempfile::tempdir().unwrap();
        let core = AppCore::open(AppPaths::from_data_dir(dir.path())).unwrap();
        let note = core.create_note().unwrap();

        let asset = core.save_png_asset(&note.id, &[137, 80, 78, 71]).unwrap();

        assert!(asset
            .markdown_path
            .starts_with(&format!("assets/notes/{}/", note.id)));
        assert!(dir.path().join(&asset.markdown_path).exists());
        assert!(asset.asset_url.starts_with("asset://localhost/"));
        assert_eq!(
            fs::read(dir.path().join(&asset.markdown_path)).unwrap(),
            vec![137, 80, 78, 71]
        );
    }

    #[test]
    fn asset_reader_only_accepts_internal_asset_paths() {
        assert!(is_allowed_markdown_asset_path(
            "assets/notes/note-id/image-id.png"
        ));
        assert!(!is_allowed_markdown_asset_path("../snapline.db"));
        assert!(!is_allowed_markdown_asset_path("assets/../snapline.db"));
        assert!(!is_allowed_markdown_asset_path("C:/Users/wtl/image.png"));
        assert!(!is_allowed_markdown_asset_path(
            "assets\\notes\\note-id\\image-id.png"
        ));
    }

    #[test]
    fn local_image_reader_only_accepts_image_files() {
        let dir = tempfile::tempdir().unwrap();
        let image_path = dir.path().join("Screenshot.png");
        let nested_image_path = dir.path().join("nested").join("photo.jpeg");
        let text_path = dir.path().join("notes.txt");
        fs::create_dir_all(nested_image_path.parent().unwrap()).unwrap();
        fs::write(&image_path, [137, 80, 78, 71]).unwrap();
        fs::write(&nested_image_path, [255, 216, 255, 224]).unwrap();
        fs::write(&text_path, b"not an image").unwrap();

        assert!(is_allowed_local_image_path(image_path.to_str().unwrap()));
        assert!(is_allowed_local_image_path(
            nested_image_path.to_str().unwrap()
        ));
        assert!(!is_allowed_local_image_path(text_path.to_str().unwrap()));
        assert!(!is_allowed_local_image_path("../relative.png"));
        assert!(!is_allowed_local_image_path(""));
    }

    #[test]
    fn encodes_clipboard_image_as_png() {
        let png =
            encode_clipboard_image_as_png(test_image_data(1, 1, vec![255, 0, 0, 255])).unwrap();

        assert_eq!(&png[0..8], &[137, 80, 78, 71, 13, 10, 26, 10]);
    }
}
