use serde::Deserialize;
use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, Position, WebviewUrl, WebviewWindowBuilder,
};

pub const FOCUS_EDITOR_EVENT: &str = "snapline-focus-editor";

const NOTE_WINDOW_WIDTH: f64 = 340.0;
const NOTE_WINDOW_HEIGHT: f64 = 440.0;
const NOTE_WINDOW_MIN_WIDTH: f64 = 300.0;
const NOTE_WINDOW_MIN_HEIGHT: f64 = 260.0;

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

#[derive(Debug, Clone, Deserialize)]
pub struct WindowPosition {
    pub x: f64,
    pub y: f64,
}

pub fn hide_main_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.hide();
    }
}

pub fn show_main_window(app: &tauri::AppHandle) {
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

pub fn build_note_window(
    app: &AppHandle,
    label: &str,
    url: &str,
    position: Option<WindowPosition>,
) -> Result<String, String> {
    let mut builder = WebviewWindowBuilder::new(app, label, WebviewUrl::App(url.into()))
        .title("Snapline Note")
        .inner_size(NOTE_WINDOW_WIDTH, NOTE_WINDOW_HEIGHT)
        .min_inner_size(NOTE_WINDOW_MIN_WIDTH, NOTE_WINDOW_MIN_HEIGHT)
        .resizable(true)
        .decorations(false);

    builder = if let Some(position) = position {
        builder.position(position.x, position.y)
    } else {
        builder.center()
    };

    let window = builder.build().map_err(|err| err.to_string())?;
    reveal_window(&window, None)?;
    Ok(label.to_string())
}

pub fn reveal_window(
    window: &tauri::WebviewWindow,
    position: Option<&WindowPosition>,
) -> Result<(), String> {
    if let Some(position) = position {
        window
            .set_position(Position::Logical(tauri::LogicalPosition {
                x: position.x,
                y: position.y,
            }))
            .map_err(|err| err.to_string())?;
    }
    window.show().map_err(|err| err.to_string())?;
    window.unminimize().map_err(|err| err.to_string())?;
    window.set_focus().map_err(|err| err.to_string())?;
    let _ = window.emit(FOCUS_EDITOR_EVENT, ());
    Ok(())
}

pub fn close_other_note_windows(app: &AppHandle, keep_label: &str) {
    for (label, window) in app.webview_windows() {
        if label != keep_label && label != "main" && label != "list" && label.starts_with("note-") {
            let _ = window.close();
        }
    }
}

fn position_near_cursor(cursor: CursorPoint, size: WindowSize, work_area: WorkArea) -> WindowPoint {
    let max_x = work_area.x + (work_area.width - size.width).max(0);
    let max_y = work_area.y + (work_area.height - size.height).max(0);
    WindowPoint {
        x: cursor.x.clamp(work_area.x, max_x),
        y: cursor.y.clamp(work_area.y, max_y),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
            WindowPoint { x: 240, y: 180 }
        );
        assert_eq!(
            position_near_cursor(CursorPoint { x: 990, y: 740 }, size, monitor),
            WindowPoint { x: 640, y: 270 }
        );
    }
}
