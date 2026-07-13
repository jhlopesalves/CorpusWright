use crate::*;

#[tauri::command]
pub(crate) fn set_window_titlebar_color(window: Window, color: String) -> Result<(), String> {
    platform::window::set_window_titlebar_color(window, color)
}
