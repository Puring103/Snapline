#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use snapline_app_core::{AppCore, BootstrapState};
use snapline_domain::{AssetRef, Note, NoteId, NoteSummary};
use snapline_platform::AppPaths;
use std::sync::Mutex;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

struct AppState {
    core: Mutex<AppCore>,
    startup_logging_enabled: bool,
}

#[tauri::command]
fn log_startup(state: State<'_, AppState>, message: String) {
    if state.startup_logging_enabled {
        eprintln!("{message}");
    }
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
fn set_note_pinned(
    state: State<'_, AppState>,
    id: String,
    pinned: bool,
) -> Result<Note, String> {
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

fn show_main_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

fn main() {
    let app_started = std::time::Instant::now();
    let startup_logging_enabled = std::env::var("SNAPLINE_STARTUP_LOG").ok().as_deref() == Some("1");
    if startup_logging_enabled {
        eprintln!("snapline.startup event=rust_main");
    }
    tauri::Builder::default()
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .build(),
        )
        .setup(move |app| {
            let setup_started = std::time::Instant::now();
            if startup_logging_enabled {
                eprintln!(
                    "snapline.startup event=setup_started elapsed_ms={}",
                    app_started.elapsed().as_millis()
                );
            }
            let paths = AppPaths::resolve().unwrap_or_else(|_| AppPaths::from_data_dir(std::env::temp_dir().join("Snapline")));
            let core = AppCore::open(paths).expect("open app core");
            app.manage(AppState {
                core: Mutex::new(core),
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
            if startup_logging_enabled {
                eprintln!(
                    "snapline.startup event=setup_finished elapsed_ms={} duration_ms={}",
                    app_started.elapsed().as_millis(),
                    setup_started.elapsed().as_millis()
                );
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            log_startup,
            bootstrap,
            create_note,
            get_note,
            save_note,
            set_note_title,
            set_note_pinned,
            delete_note,
            save_png_asset,
            resolve_asset_url,
            get_open_shortcut,
            set_open_shortcut
        ])
        .run(tauri::generate_context!())
        .expect("error while running Snapline");
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

        assert!(asset.markdown_path.starts_with(&format!("assets/notes/{}/", note.id)));
        assert!(dir.path().join(&asset.markdown_path).exists());
        assert!(asset.asset_url.starts_with("asset://localhost/"));
        assert_eq!(
            fs::read(dir.path().join(&asset.markdown_path)).unwrap(),
            vec![137, 80, 78, 71]
        );
    }
}
