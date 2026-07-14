use tauri::Window;

#[cfg(target_os = "windows")]
mod win32 {
    use std::ffi::c_void;

    const DWMWA_CAPTION_COLOR: u32 = 35;

    unsafe extern "system" {
        fn DwmSetWindowAttribute(
            hwnd: *mut c_void,
            dwAttribute: u32,
            pvAttribute: *const c_void,
            cbAttribute: u32,
        ) -> i32;
    }

    pub fn set_titlebar_color(hwnd: *mut c_void, color: u32) {
        unsafe {
            let pv_attribute = &color as *const u32 as *const c_void;
            let _ = DwmSetWindowAttribute(
                hwnd,
                DWMWA_CAPTION_COLOR,
                pv_attribute,
                std::mem::size_of::<u32>() as u32,
            );
        }
    }
}

#[cfg(target_os = "windows")]
fn parse_hex_color(hex: &str) -> Option<u32> {
    let hex = hex.trim_start_matches('#');
    if hex.len() == 6 {
        let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
        let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
        let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
        // BGR format: 0x00BBGGRR
        Some(((b as u32) << 16) | ((g as u32) << 8) | (r as u32))
    } else {
        None
    }
}

#[cfg(target_os = "windows")]
pub(crate) fn set_titlebar_color_raw(hwnd: *mut std::ffi::c_void, color: u32) {
    win32::set_titlebar_color(hwnd, color);
}

pub(crate) fn set_window_titlebar_color(window: Window, color: String) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        if let Some(parsed_color) = parse_hex_color(&color) {
            if let Ok(hwnd) = window.hwnd() {
                set_titlebar_color_raw(hwnd.0, parsed_color);
                return Ok(());
            }
            return Err("Could not get window handle".to_string());
        }
        Err(format!("Invalid hex color: {}", color))
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = window;
        let _ = color;
        Ok(())
    }
}
