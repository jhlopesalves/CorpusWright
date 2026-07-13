use crate::*;

/// Checks whether the given path has a `.json` extension (case-insensitive).
pub(crate) fn is_json_path(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.eq_ignore_ascii_case("json"))
        .unwrap_or(false)
}

pub(crate) const MAX_CONFIG_SIZE: u64 = 1_048_576; // 1 MB

/// Reads a config JSON file from an arbitrary user-chosen path.
///
/// Validates that:
/// - The path has a `.json` extension (case-insensitive).
/// - The file exists and is a regular file.
/// - The file does not exceed the maximum config size.
/// - The content is valid JSON.
///
/// # Security note
///
/// Users still choose the path and an injected webview caller could still read
/// writable `.json` paths within process permissions. This is defence-in-depth,
/// not a complete filesystem sandbox.
#[tauri::command(async)]
pub(crate) fn read_config_file_command(path: String) -> Result<String, String> {
    let path_buf = PathBuf::from(&path);
    if !is_json_path(&path_buf) {
        return Err("Only .json config files are supported.".to_string());
    }
    let metadata = std::fs::metadata(&path).map_err(|e| format!("Cannot access file: {}", e))?;
    if !metadata.is_file() {
        return Err("Path is not a regular file.".to_string());
    }
    if metadata.len() > MAX_CONFIG_SIZE {
        return Err(format!(
            "File exceeds {} MB size limit.",
            MAX_CONFIG_SIZE / 1_048_576
        ));
    }
    let content = std::fs::read_to_string(&path).map_err(|e| format!("Cannot read file: {}", e))?;
    let _: serde_json::Value =
        serde_json::from_str(&content).map_err(|e| format!("File is not valid JSON: {}", e))?;
    Ok(content)
}

/// Writes a config JSON string to an arbitrary user-chosen path.
///
/// Validates that:
/// - The path has a `.json` extension (case-insensitive).
/// - The content does not exceed the maximum config size.
/// - The content is valid JSON.
///
/// # Security note
///
/// Users still choose the path and an injected webview caller could still write
/// to writable `.json` paths within process permissions. This is defence-in-depth,
/// not a complete filesystem sandbox.
#[tauri::command(async)]
pub(crate) fn save_config_file_command(path: String, content: String) -> Result<(), String> {
    let path_buf = PathBuf::from(&path);
    if !is_json_path(&path_buf) {
        return Err("Only .json config files are supported.".to_string());
    }
    if content.len() as u64 > MAX_CONFIG_SIZE {
        return Err(format!(
            "Config exceeds {} MB size limit.",
            MAX_CONFIG_SIZE / 1_048_576
        ));
    }
    let _: serde_json::Value =
        serde_json::from_str(&content).map_err(|e| format!("Content is not valid JSON: {}", e))?;
    std::fs::write(&path, &content).map_err(|e| format!("Cannot write file: {}", e))
}

#[tauri::command(async)]
pub(crate) fn save_svg_file_command(path: String, content: String) -> Result<(), String> {
    let path_buf = PathBuf::from(&path);
    let is_svg = path_buf
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("svg"));
    if !is_svg {
        return Err("Only .svg files are supported.".to_string());
    }
    if !content.trim_start().starts_with("<svg") {
        return Err("SVG content must have an <svg> root element.".to_string());
    }
    std::fs::write(&path_buf, content).map_err(|error| format!("Cannot write SVG file: {error}"))
}
