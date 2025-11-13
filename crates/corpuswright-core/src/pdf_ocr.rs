use ocrs::{OcrEngine, OcrEngineParams};
use pdfium_render::prelude::*;
use rten::Model;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, OnceLock, RwLock};

static OCR_ENGINE: OnceLock<OcrEngine> = OnceLock::new();
static PDFIUM_LIBRARY: OnceLock<Pdfium> = OnceLock::new();
static OCR_RESOURCE_DIR: LazyLock<RwLock<Option<PathBuf>>> = LazyLock::new(|| RwLock::new(None));

/// Overrides the OCR/PDFium resource directory used by core extraction.
///
/// UI layers should call this during startup after resolving their packaged
/// resource directory. The core crate intentionally does not depend on Tauri.
pub fn set_ocr_resource_dir(path: impl Into<PathBuf>) -> anyhow::Result<()> {
    let path = path.into();
    if path.as_os_str().is_empty() {
        return Err(anyhow::anyhow!("OCR resource directory cannot be empty"));
    }

    let mut configured = OCR_RESOURCE_DIR
        .write()
        .map_err(|_| anyhow::anyhow!("OCR resource directory lock was poisoned"))?;
    *configured = Some(path);
    Ok(())
}

pub fn configured_ocr_resource_dir() -> Option<PathBuf> {
    OCR_RESOURCE_DIR.read().ok().and_then(|guard| guard.clone())
}

fn dedupe_paths(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut deduped = Vec::new();
    for path in paths {
        if !deduped.iter().any(|seen| seen == &path) {
            deduped.push(path);
        }
    }
    deduped
}

fn default_ocr_resource_candidates_from_exe(current_exe: Option<&Path>) -> Vec<PathBuf> {
    let mut paths = Vec::new();

    if let Some(current_exe) = current_exe {
        let exe_dir = current_exe.parent().unwrap_or_else(|| Path::new(""));
        paths.push(exe_dir.join("ocr"));
        paths.push(exe_dir.join("../../../resources/ocr"));
        paths.push(exe_dir.join("../../../../apps/desktop/src-tauri/resources/ocr"));
    }

    paths.push(PathBuf::from("resources/ocr"));
    paths.push(PathBuf::from("../../apps/desktop/src-tauri/resources/ocr"));
    paths.push(PathBuf::from("apps/desktop/src-tauri/resources/ocr"));

    paths
}

fn ocr_resource_candidates_with(
    configured: Option<PathBuf>,
    env_override: Option<PathBuf>,
    current_exe: Option<&Path>,
) -> Vec<PathBuf> {
    let mut paths = Vec::new();

    if let Some(path) = configured {
        paths.push(path);
    }
    if let Some(path) = env_override {
        paths.push(path);
    }

    paths.extend(default_ocr_resource_candidates_from_exe(current_exe));
    dedupe_paths(paths)
}

pub fn ocr_resource_candidates() -> Vec<PathBuf> {
    let configured = configured_ocr_resource_dir();
    let env_override = std::env::var_os("CORPUSWRIGHT_OCR_DIR").map(PathBuf::from);
    let current_exe = std::env::current_exe().ok();

    ocr_resource_candidates_with(configured, env_override, current_exe.as_deref())
}

pub fn pdfium_library_path_for_dir(dir: &Path) -> PathBuf {
    Pdfium::pdfium_platform_library_name_at_path(dir.to_string_lossy().as_ref())
}

pub fn pdfium_library_candidates() -> Vec<PathBuf> {
    ocr_resource_candidates()
        .into_iter()
        .map(|dir| pdfium_library_path_for_dir(&dir))
        .collect()
}

pub fn first_existing_pdfium_library() -> Option<PathBuf> {
    pdfium_library_candidates()
        .into_iter()
        .find(|path| path.is_file())
}

fn first_existing_ocr_resource_dir() -> Option<PathBuf> {
    ocr_resource_candidates()
        .into_iter()
        .find(|path| path.is_dir())
}

/// Returns true if PDFium can be initialized on the current platform.
/// Useful for guarding tests that depend on the native PDFium library.
#[cfg(test)]
pub(crate) fn pdfium_available() -> bool {
    first_existing_pdfium_library()
        .and_then(|path| Pdfium::bind_to_library(path).ok())
        .or_else(|| Pdfium::bind_to_system_library().ok())
        .is_some()
}

pub fn init_pdfium() -> anyhow::Result<&'static Pdfium> {
    if let Some(p) = PDFIUM_LIBRARY.get() {
        return Ok(p);
    }

    static INIT_MUTEX: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _lock = INIT_MUTEX.lock().unwrap();

    if let Some(p) = PDFIUM_LIBRARY.get() {
        return Ok(p);
    }

    let mut attempted = Vec::new();
    for library_path in pdfium_library_candidates() {
        if !library_path.is_file() {
            attempted.push(format!("{} (missing)", library_path.display()));
            continue;
        }

        match Pdfium::bind_to_library(&library_path) {
            Ok(bindings) => {
                let pdfium = Pdfium::new(bindings);
                PDFIUM_LIBRARY
                    .set(pdfium)
                    .map_err(|_| anyhow::anyhow!("Failed to set PDFIUM_LIBRARY"))?;
                return Ok(PDFIUM_LIBRARY.get().unwrap());
            }
            Err(error) => {
                attempted.push(format!("{} ({:?})", library_path.display(), error));
            }
        }
    }

    let pdfium = Pdfium::new(Pdfium::bind_to_system_library().map_err(|error| {
        anyhow::anyhow!(
            "Failed to bind to PDFium; attempted bundled libraries [{}], then system library ({:?})",
            attempted.join("; "),
            error
        )
    })?);

    PDFIUM_LIBRARY
        .set(pdfium)
        .map_err(|_| anyhow::anyhow!("Failed to set PDFIUM_LIBRARY"))?;
    Ok(PDFIUM_LIBRARY.get().unwrap())
}

fn init_ocr_engine() -> anyhow::Result<&'static OcrEngine> {
    if let Some(engine) = OCR_ENGINE.get() {
        return Ok(engine);
    }

    let ocr_res_dir = first_existing_ocr_resource_dir().unwrap_or_else(|| PathBuf::from("ocr"));

    let detection_path = ocr_res_dir.join("text-detection.rten");
    let recognition_path = ocr_res_dir.join("text-recognition.rten");

    let detection_model = Model::load_file(&detection_path).map_err(|e| {
        anyhow::anyhow!(
            "Failed to load detection model from {:?}: {}",
            detection_path,
            e
        )
    })?;
    let recognition_model = Model::load_file(&recognition_path).map_err(|e| {
        anyhow::anyhow!(
            "Failed to load recognition model from {:?}: {}",
            recognition_path,
            e
        )
    })?;

    let engine = OcrEngine::new(OcrEngineParams {
        detection_model: Some(detection_model),
        recognition_model: Some(recognition_model),
        ..Default::default()
    })
    .map_err(|e| anyhow::anyhow!("Failed to init OCR engine: {}", e))?;

    OCR_ENGINE
        .set(engine)
        .map_err(|_| anyhow::anyhow!("Failed to set OCR_ENGINE"))?;
    Ok(OCR_ENGINE.get().unwrap())
}

pub fn extract_text_via_ocr(bytes: &[u8], max_chars: Option<usize>) -> anyhow::Result<String> {
    let pdfium = init_pdfium()?;
    let engine = init_ocr_engine()?;

    let document = {
        let _lock = crate::pdf::PDFIUM_LOCK.lock().unwrap();
        pdfium.load_pdf_from_byte_slice(bytes, None)?
    };
    let mut all_text = String::new();

    let page_count = {
        let _lock = crate::pdf::PDFIUM_LOCK.lock().unwrap();
        document.pages().len() as usize
    };

    for page_index in 0..page_count {
        if let Some(limit) = max_chars
            && all_text.chars().count() >= limit
        {
            break;
        }

        let image = {
            let _lock = crate::pdf::PDFIUM_LOCK.lock().unwrap();
            let page = document.pages().get(page_index as i32)?;
            // Render to roughly 200 DPI
            let render_config = PdfRenderConfig::new()
                .set_target_width(1200)
                .set_clear_color(PdfColor::WHITE);

            let bitmap = page.render_with_config(&render_config)?;
            bitmap.as_image()?
        };

        let rgb_image = image.into_rgb8();
        let (width, height) = rgb_image.dimensions();

        let image_source = ocrs::ImageSource::from_bytes(rgb_image.as_raw(), (width, height))?;
        let ocr_input = engine.prepare_input(image_source)?;

        let word_rects = engine.detect_words(&ocr_input)?;
        let line_rects = engine.find_text_lines(&ocr_input, &word_rects);
        let line_texts = engine.recognize_text(&ocr_input, &line_rects)?;

        let mut page_text = String::new();
        for line in line_texts.iter().flatten() {
            if !page_text.is_empty() {
                page_text.push('\n');
            }
            page_text.push_str(&line.to_string());
        }

        let trimmed = page_text.trim();
        if !trimmed.is_empty() {
            if !all_text.is_empty() {
                all_text.push('\n');
                all_text.push('\n');
            }
            all_text.push_str(trimmed);
        }
    }

    Ok(all_text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn configured_resource_dir_is_first_candidate() {
        let exe = Path::new("C:/Program Files/CorpusWright/corpuswright-desktop.exe");
        let configured = PathBuf::from("C:/Users/example/AppData/Local/CorpusWright/resources/ocr");
        let env_override = PathBuf::from("D:/override/ocr");

        let candidates =
            ocr_resource_candidates_with(Some(configured.clone()), Some(env_override), Some(exe));

        assert_eq!(candidates.first(), Some(&configured));
    }
}
