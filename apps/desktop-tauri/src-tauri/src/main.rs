#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use snapline_app_core::{AppCore, BootstrapState, SyncAccountState};
use snapline_domain::{AssetRef, Note, NoteId, NoteSummary, SyncPayload};
use snapline_platform::AppPaths;
use snapline_sync_client::{
    protocol::{AssetUploadRequest, LoginRequest, PushChange, PushChangeResult, PushRequest},
    HttpSyncApi, SyncApi,
};
use std::sync::Mutex;
use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, Position, RunEvent, State, WindowEvent,
};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

const AUTOSTART_BACKGROUND_ARG: &str = "--background";
const FOCUS_EDITOR_EVENT: &str = "snapline-focus-editor";
const CURSOR_OFFSET: i32 = 12;
const BACKGROUND_SYNC_INTERVAL_SECS: u64 = 60;

struct AppState {
    core: Mutex<AppCore>,
    launched_in_background: bool,
    startup_logging_enabled: bool,
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

fn is_allowed_markdown_asset_path(markdown_path: &str) -> bool {
    markdown_path.starts_with("assets/")
        && !markdown_path.contains('\\')
        && !markdown_path
            .split('/')
            .any(|segment| segment.is_empty() || segment == "." || segment == "..")
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
async fn login_sync(
    state: State<'_, AppState>,
    server_base_url: String,
    email: String,
    password: String,
) -> Result<SyncAccountState, String> {
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
            email,
            password,
            device_id,
            device_name: "Snapline Desktop".to_string(),
        })
        .await
        .map_err(|err| err.to_string())?;
    let account_state = state
        .core
        .lock()
        .map_err(|_| "app state lock poisoned".to_string())?
        .save_sync_login(
            &server_base_url,
            &response.account_id,
            &response.access_token,
        )
        .map_err(|err| err.to_string())?;
    import_snapshot_and_assets(&state, &api, &response.access_token).await?;
    Ok(account_state)
}

#[tauri::command]
async fn sync_now(state: State<'_, AppState>) -> Result<String, String> {
    run_sync_once(state.inner()).await
}

async fn run_sync_once(state: &AppState) -> Result<String, String> {
    let (base_url, token, device_id, data_dir) = {
        let core = state
            .core
            .lock()
            .map_err(|_| "app state lock poisoned".to_string())?;
        let (base_url, token, device_id) = core
            .sync_credentials()
            .map_err(|err| err.to_string())?
            .ok_or_else(|| "not logged in".to_string())?;
        (base_url, token, device_id, core.data_dir().to_path_buf())
    };
    let api = HttpSyncApi::new(base_url);
    let asset_items = {
        let core = state
            .core
            .lock()
            .map_err(|_| "app state lock poisoned".to_string())?;
        core.pending_sync_changes()
            .map_err(|err| err.to_string())?
            .into_iter()
            .filter(|item| matches!(item.payload, SyncPayload::Asset(_)))
            .collect::<Vec<_>>()
    };
    let mut uploaded_assets = 0;
    for item in asset_items {
        let SyncPayload::Asset(metadata) = item.payload else {
            continue;
        };
        let bytes =
            std::fs::read(data_dir.join(&metadata.markdown_path)).map_err(|err| err.to_string())?;
        api.upload_asset(&token, AssetUploadRequest { metadata, bytes })
            .await
            .map_err(|err| err.to_string())?;
        let core = state
            .core
            .lock()
            .map_err(|_| "app state lock poisoned".to_string())?;
        core.delete_sync_change(&item.id)
            .map_err(|err| err.to_string())?;
        uploaded_assets += 1;
    }
    let pending = {
        let core = state
            .core
            .lock()
            .map_err(|_| "app state lock poisoned".to_string())?;
        core.pending_sync_changes().map_err(|err| err.to_string())?
    };
    let push_response = api
        .push(
            &token,
            PushRequest {
                device_id: device_id.clone(),
                changes: pending
                    .iter()
                    .map(|item| PushChange {
                        queue_id: item.id.clone(),
                        note_id: item.note_id.clone(),
                        base_version: item.base_version,
                        payload: item.payload.clone(),
                    })
                    .collect(),
            },
        )
        .await
        .map_err(|err| err.to_string())?;
    let mut pushed = 0;
    let mut push_conflicts = 0;
    let mut max_cursor = 0;
    {
        let core = state
            .core
            .lock()
            .map_err(|_| "app state lock poisoned".to_string())?;
        for result in push_response.results {
            match result {
                PushChangeResult::Accepted {
                    queue_id,
                    note_id,
                    server_version,
                    cursor,
                } => {
                    core.update_note_server_version(&note_id, server_version)
                        .map_err(|err| err.to_string())?;
                    core.delete_sync_change(&queue_id)
                        .map_err(|err| err.to_string())?;
                    max_cursor = max_cursor.max(cursor);
                    pushed += 1;
                }
                PushChangeResult::Conflict {
                    queue_id,
                    note_id,
                    server_note,
                } => {
                    if let Some(rejected_note) =
                        rejected_note_from_pending(&pending, &queue_id, &note_id)
                    {
                        core.create_conflict_copy(&rejected_note)
                            .map_err(|err| err.to_string())?;
                        core.apply_remote_note(&server_note)
                            .map_err(|err| err.to_string())?;
                    }
                    core.delete_sync_change(&queue_id)
                        .map_err(|err| err.to_string())?;
                    push_conflicts += 1;
                }
            }
        }
        if max_cursor > 0 {
            core.update_sync_cursor_success(max_cursor, chrono::Utc::now())
                .map_err(|err| err.to_string())?;
        }
    }
    let cursor = {
        let core = state
            .core
            .lock()
            .map_err(|_| "app state lock poisoned".to_string())?;
        core.sync_state()
            .map_err(|err| err.to_string())?
            .server_cursor
    };
    let pull_response = api
        .pull(&token, cursor)
        .await
        .map_err(|err| err.to_string())?;
    let mut pulled = 0;
    let mut pull_conflicts = 0;
    {
        let core = state
            .core
            .lock()
            .map_err(|_| "app state lock poisoned".to_string())?;
        for change in pull_response.changes {
            if change.device_id == device_id {
                continue;
            }
            if core
                .has_pending_note_change(&change.note.id)
                .map_err(|err| err.to_string())?
            {
                let local_note = core
                    .get_note(&change.note.id)
                    .map_err(|err| err.to_string())?;
                core.create_conflict_copy(&local_note)
                    .map_err(|err| err.to_string())?;
                core.delete_sync_changes_for_note(&change.note.id)
                    .map_err(|err| err.to_string())?;
                pull_conflicts += 1;
            }
            core.apply_remote_note(&change.note)
                .map_err(|err| err.to_string())?;
            pulled += 1;
        }
        core.update_sync_cursor_success(pull_response.cursor, chrono::Utc::now())
            .map_err(|err| err.to_string())?;
    };
    import_snapshot_and_assets(state, &api, &token).await?;
    Ok(format!(
        "uploaded_assets={}, pushed={}, pulled={}, conflicts={}, failed={}",
        uploaded_assets,
        pushed,
        pulled,
        push_conflicts + pull_conflicts,
        0
    ))
}

fn rejected_note_from_pending(
    pending: &[snapline_storage::ChangeQueueItem],
    queue_id: &str,
    note_id: &NoteId,
) -> Option<Note> {
    let item = pending.iter().find(|item| item.id == queue_id)?;
    let SyncPayload::Note(payload) = &item.payload else {
        return None;
    };
    let now = chrono::Utc::now();
    Some(Note {
        id: note_id.clone(),
        title: payload.title.clone(),
        content_md: payload.content_md.clone(),
        pinned: payload.pinned,
        created_at: now,
        updated_at: now,
        deleted_at: payload.deleted_at,
        server_version: item.base_version,
        last_modified_by_device: None,
        is_conflict_copy: false,
        source_note_id: None,
    })
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
        core.import_snapshot(&snapshot.notes, snapshot.cursor)
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
        core.save_remote_asset(&asset, &downloaded.bytes)
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CursorPoint {
    x: i32,
    y: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct WindowPoint {
    x: i32,
    y: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct WindowSize {
    width: i32,
    height: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct WorkArea {
    x: i32,
    y: i32,
    width: i32,
    height: i32,
}

fn position_near_cursor(cursor: CursorPoint, size: WindowSize, work_area: WorkArea) -> WindowPoint {
    let max_x = work_area.x + (work_area.width - size.width).max(0);
    let max_y = work_area.y + (work_area.height - size.height).max(0);
    WindowPoint {
        x: (cursor.x + CURSOR_OFFSET).clamp(work_area.x, max_x),
        y: (cursor.y + CURSOR_OFFSET).clamp(work_area.y, max_y),
    }
}

fn hide_main_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.hide();
    }
}

fn show_main_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        if let Ok(cursor) = app.cursor_position() {
            if let Ok(Some(monitor)) = app.monitor_from_point(cursor.x, cursor.y) {
                let work_area = monitor.work_area();
                let size = window
                    .outer_size()
                    .map(|size| WindowSize {
                        width: size.width as i32,
                        height: size.height as i32,
                    })
                    .unwrap_or(WindowSize {
                        width: 420,
                        height: 560,
                    });
                let next_position = position_near_cursor(
                    CursorPoint {
                        x: cursor.x.round() as i32,
                        y: cursor.y.round() as i32,
                    },
                    size,
                    WorkArea {
                        x: work_area.position.x,
                        y: work_area.position.y,
                        width: work_area.size.width as i32,
                        height: work_area.size.height as i32,
                    },
                );
                let _ = window.set_position(Position::Physical(PhysicalPosition {
                    x: next_position.x,
                    y: next_position.y,
                }));
            }
        }
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
        let _ = window.emit(FOCUS_EDITOR_EVENT, ());
    }
}

fn main() {
    let app_started = std::time::Instant::now();
    let startup_logging_enabled =
        std::env::var("SNAPLINE_STARTUP_LOG").ok().as_deref() == Some("1");
    let should_launch_in_background = std::env::args().any(|arg| arg == AUTOSTART_BACKGROUND_ARG);
    if startup_logging_enabled {
        eprintln!("snapline.startup event=rust_main");
    }
    tauri::Builder::default()
        .plugin(
            tauri_plugin_autostart::Builder::new()
                .args([AUTOSTART_BACKGROUND_ARG])
                .app_name("Snapline")
                .build(),
        )
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
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
            create_note,
            get_note,
            save_note,
            set_note_title,
            set_note_pinned,
            delete_note,
            save_png_asset,
            read_asset_bytes,
            resolve_asset_url,
            open_external_url,
            get_open_shortcut,
            set_open_shortcut,
            get_sync_account_state,
            login_sync,
            sync_now
        ])
        .build(tauri::generate_context!())
        .expect("error while building Snapline")
        .run(move |app, event| match event {
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
        });
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
    fn places_opened_window_near_cursor_within_monitor_bounds() {
        let monitor = WorkArea {
            x: 100,
            y: 50,
            width: 900,
            height: 700,
        };
        let size = WindowSize {
            width: 360,
            height: 480,
        };

        assert_eq!(
            position_near_cursor(CursorPoint { x: 240, y: 180 }, size, monitor),
            WindowPoint { x: 252, y: 192 }
        );
        assert_eq!(
            position_near_cursor(CursorPoint { x: 990, y: 740 }, size, monitor),
            WindowPoint { x: 640, y: 270 }
        );
    }
}
