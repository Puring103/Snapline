use snapline_app_core::{AppCore, BootstrapState};
use snapline_domain::{AssetRef, Note, NoteId, NoteSummary};
use snapline_platform::AppPaths;
use std::sync::Mutex;
use tauri::{AppHandle, Manager, State};

struct AppState {
    core: Mutex<AppCore>,
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
fn save_note(state: State<'_, AppState>, id: String, content_md: String) -> Result<Note, String> {
    let id = parse_note_id(&id)?;
    let started = std::time::Instant::now();
    let result = state
        .core
        .lock()
        .map_err(|_| "app state lock poisoned".to_string())?
        .save_note(&id, &content_md)
        .map_err(|err| err.to_string());
    eprintln!("snapline.save_note_ms={}", started.elapsed().as_millis());
    result
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

fn parse_note_id(value: &str) -> Result<NoteId, String> {
    uuid::Uuid::parse_str(value)
        .map(NoteId)
        .map_err(|err| format!("invalid note id: {err}"))
}

fn app_data_dir(app: &AppHandle) -> std::path::PathBuf {
    app.path()
        .app_data_dir()
        .unwrap_or_else(|_| std::env::temp_dir().join("Snapline"))
}

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            let paths = AppPaths::from_data_dir(app_data_dir(&app.handle()));
            let core = AppCore::open(paths).expect("open app core");
            app.manage(AppState {
                core: Mutex::new(core),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            bootstrap,
            create_note,
            get_note,
            save_note,
            delete_note,
            save_png_asset
        ])
        .run(tauri::generate_context!())
        .expect("error while running Snapline");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn bootstrap_creates_first_note() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(dir.path());
        let core = AppCore::open(paths).unwrap();
        let state = core.bootstrap().unwrap();

        assert_eq!(state.notes.len(), 1);
        assert_eq!(state.current.title, "Untitled");
    }

    #[test]
    fn saves_png_asset_under_note_directory() {
        let dir = tempfile::tempdir().unwrap();
        let core = AppCore::open(AppPaths::from_data_dir(dir.path())).unwrap();
        let note = core.create_note().unwrap();

        let asset = core.save_png_asset(&note.id, &[137, 80, 78, 71]).unwrap();

        assert!(asset.markdown_path.starts_with(&format!("assets/notes/{}/", note.id)));
        assert!(dir.path().join(&asset.markdown_path).exists());
        assert_eq!(
            fs::read(dir.path().join(&asset.markdown_path)).unwrap(),
            vec![137, 80, 78, 71]
        );
    }
}
