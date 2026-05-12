#[cfg(not(desktop))]
use tauri::AppHandle;
#[cfg(desktop)]
use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, Position, WebviewUrl, WebviewWindowBuilder,
};

#[cfg(desktop)]
pub const FOCUS_EDITOR_EVENT: &str = "snapline-focus-editor";

#[cfg(desktop)]
const NOTE_WINDOW_WIDTH: f64 = 340.0;
#[cfg(desktop)]
const NOTE_WINDOW_HEIGHT: f64 = 440.0;
#[cfg(desktop)]
const NOTE_WINDOW_MIN_WIDTH: f64 = 300.0;
#[cfg(desktop)]
const NOTE_WINDOW_MIN_HEIGHT: f64 = 260.0;

#[cfg_attr(not(desktop), allow(dead_code))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CursorPoint {
    x: i32,
    y: i32,
}

#[cfg_attr(not(desktop), allow(dead_code))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct WindowPoint {
    x: i32,
    y: i32,
}

#[cfg_attr(not(desktop), allow(dead_code))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct WindowSize {
    width: i32,
    height: i32,
}

#[cfg_attr(not(desktop), allow(dead_code))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct WorkArea {
    x: i32,
    y: i32,
    width: i32,
    height: i32,
}

#[cfg(desktop)]
use serde::Deserialize;

#[cfg(desktop)]
#[derive(Debug, Clone, Deserialize)]
pub struct WindowPosition {
    pub x: f64,
    pub y: f64,
}

#[cfg(not(desktop))]
#[derive(Debug, Clone, serde::Deserialize)]
pub struct WindowPosition {
    pub x: f64,
    pub y: f64,
}

#[cfg(desktop)]
pub fn hide_main_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.hide();
    }
}

#[cfg(not(desktop))]
pub fn hide_main_window(_app: &tauri::AppHandle) {}

#[cfg(desktop)]
pub fn show_main_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let is_visible = window.is_visible().unwrap_or(false);
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
        let _ = window.emit(FOCUS_EDITOR_EVENT, ());
        if is_visible {
            let _ = window.show();
            let _ = window.unminimize();
            let _ = window.set_focus();
        }
    }
}

#[cfg(desktop)]
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
        .decorations(false)
        .visible(false);

    builder = if let Some(position) = position {
        builder.position(position.x, position.y)
    } else {
        builder.center()
    };

    let _window = builder.build().map_err(|err| err.to_string())?;
    Ok(label.to_string())
}

#[cfg(not(desktop))]
pub fn build_note_window(
    _app: &AppHandle,
    label: &str,
    _url: &str,
    position: Option<WindowPosition>,
) -> Result<String, String> {
    if let Some(position) = position {
        let _ = (position.x, position.y);
    }
    Ok(label.to_string())
}

#[cfg_attr(not(desktop), allow(dead_code))]
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
