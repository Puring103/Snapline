#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use snapline_app_core::{AppCore, BootstrapState};
use snapline_domain::{AssetRef, Note, NoteId, NoteSummary};
use snapline_platform::AppPaths;
use std::sync::Mutex;
use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, Position, RunEvent, State, WindowEvent,
};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

const AUTOSTART_BACKGROUND_ARG: &str = "--background";
const FOCUS_EDITOR_EVENT: &str = "snapline-focus-editor";
const CURSOR_OFFSET: i32 = 12;

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
            resolve_asset_url,
            get_open_shortcut,
            set_open_shortcut
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
