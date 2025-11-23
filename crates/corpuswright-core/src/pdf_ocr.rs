use crate::clean::PdfOcrQuality;
use ocrs::{OcrEngine, OcrEngineParams};
use pdfium_render::prelude::*;
use rten::Model;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex, OnceLock, RwLock};

static OCR_ENGINE: OnceLock<OcrEngine> = OnceLock::new();
static OCR_ENGINE_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));
static PDFIUM_LIBRARY: OnceLock<Pdfium> = OnceLock::new();
static OCR_RESOURCE_DIR: LazyLock<RwLock<Option<PathBuf>>> = LazyLock::new(|| RwLock::new(None));
const PREVIEW_MAX_DIMENSION: i32 = 900;
const PREVIEW_MAX_PIXELS: u64 = 300_000;

#[derive(Debug, Clone)]
pub struct OcrExtraction {
    pub text: String,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct OcrPageExtraction {
    pub page_index: usize,
    pub text: String,
    pub warnings: Vec<String>,
    pub render_clamped: bool,
    pub error: Option<String>,
}

#[derive(Clone, Copy)]
struct OcrRenderPreset {
    dpi: f64,
    max_dimension: i32,
    max_pixels: u64,
}

fn render_preset(quality: PdfOcrQuality, preview_mode: bool) -> OcrRenderPreset {
    let mut preset = match quality {
        PdfOcrQuality::Fast => OcrRenderPreset {
            dpi: 150.0,
            max_dimension: 2_400,
            max_pixels: 6_000_000,
        },
        PdfOcrQuality::Balanced => OcrRenderPreset {
            dpi: 200.0,
            max_dimension: 3_200,
            max_pixels: 10_000_000,
        },
        PdfOcrQuality::HighQuality => OcrRenderPreset {
            dpi: 300.0,
            max_dimension: 4_096,
            max_pixels: 14_000_000,
        },
    };

    if preview_mode {
        preset.max_dimension = preset.max_dimension.min(PREVIEW_MAX_DIMENSION);
        preset.max_pixels = preset.max_pixels.min(PREVIEW_MAX_PIXELS);
    }

    preset
}

fn render_config_for_page(
    page: &PdfPage<'_>,
    quality: PdfOcrQuality,
    page_number: usize,
    preview_mode: bool,
) -> (PdfRenderConfig, Option<String>) {
    let preset = render_preset(quality, preview_mode);
    let page_width_points = f64::from(page.width().value).max(1.0);
    let page_height_points = f64::from(page.height().value).max(1.0);
    let desired_width = (page_width_points / 72.0 * preset.dpi).round().max(1.0);
    let desired_height = (page_height_points / 72.0 * preset.dpi).round().max(1.0);

    let dimension_scale = (f64::from(preset.max_dimension) / desired_width)
        .min(f64::from(preset.max_dimension) / desired_height)
        .min(1.0);
    let desired_pixels = desired_width * desired_height;
    let pixel_scale = if desired_pixels > preset.max_pixels as f64 {
        (preset.max_pixels as f64 / desired_pixels).sqrt()
    } else {
        1.0
    };
    let scale = dimension_scale.min(pixel_scale).min(1.0);

    let target_width = (desired_width * scale)
        .round()
        .max(1.0)
        .min(f64::from(preset.max_dimension)) as i32;
    let target_height = (desired_height * scale)
        .round()
        .max(1.0)
        .min(f64::from(preset.max_dimension)) as i32;

    let warning = if scale < 0.999 {
        Some(format!(
            "OCR render size was clamped on page {} from {}x{} to about {}x{} pixels to limit memory use.",
            page_number, desired_width as u64, desired_height as u64, target_width, target_height
        ))
    } else {
        None
    };

    let render_config = PdfRenderConfig::new()
        .set_target_size(target_width, target_height)
        .set_maximum_width(preset.max_dimension)
        .set_maximum_height(preset.max_dimension)
        .set_clear_color(PdfColor::WHITE);

    (render_config, warning)
}

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

pub fn first_existing_ocr_model_dir() -> Option<PathBuf> {
    ocr_resource_candidates().into_iter().find(|path| {
        path.join("text-detection.rten").is_file() && path.join("text-recognition.rten").is_file()
    })
}

pub fn ocr_model_resources_available() -> bool {
    first_existing_ocr_model_dir().is_some()
}

pub fn ocr_model_identity() -> Option<String> {
    first_existing_ocr_model_dir().map(|path| path.display().to_string())
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

pub fn extract_text_via_ocr(
    bytes: &[u8],
    max_chars: Option<usize>,
    max_pages: Option<usize>,
    quality: PdfOcrQuality,
) -> anyhow::Result<OcrExtraction> {
    let pdfium = init_pdfium()?;
    let engine = init_ocr_engine()?;
    let preview_mode = max_chars.is_some() || max_pages.is_some();

    let document = {
        let _lock = crate::pdf::PDFIUM_LOCK.lock().unwrap();
        pdfium.load_pdf_from_byte_slice(bytes, None)?
    };
    let mut all_text = String::new();
    let mut warnings = Vec::new();

    let page_count = {
        let _lock = crate::pdf::PDFIUM_LOCK.lock().unwrap();
        document.pages().len() as usize
    };

    let pages_to_process = max_pages
        .map(|limit| limit.min(page_count))
        .unwrap_or(page_count);

    for page_index in 0..pages_to_process {
        if let Some(limit) = max_chars
            && all_text.chars().count() >= limit
        {
            break;
        }

        let image = {
            let _lock = crate::pdf::PDFIUM_LOCK.lock().unwrap();
            let page = document.pages().get(page_index as i32)?;
            let (render_config, render_warning) =
                render_config_for_page(&page, quality, page_index + 1, preview_mode);
            if let Some(warning) = render_warning {
                warnings.push(warning);
            }

            let bitmap = page.render_with_config(&render_config)?;
            bitmap.as_image()?
        };

        let rgb_image = image.into_rgb8();
        let (width, height) = rgb_image.dimensions();

        let page_text = {
            // The recogniser is shared process-wide and can be called from Rayon workers.
            let _ocr_lock = OCR_ENGINE_LOCK.lock().unwrap();
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
            page_text
        };

        let trimmed = page_text.trim();
        if !trimmed.is_empty() {
            if !all_text.is_empty() {
                all_text.push('\n');
                all_text.push('\n');
            }
            all_text.push_str(trimmed);
        }
    }

    Ok(OcrExtraction {
        text: all_text,
        warnings,
    })
}

pub fn extract_page_range_via_ocr(
    bytes: &[u8],
    start_page_index: usize,
    requested_page_count: usize,
    quality: PdfOcrQuality,
) -> anyhow::Result<(usize, Vec<OcrPageExtraction>)> {
    let pdfium = init_pdfium()?;
    let engine = init_ocr_engine()?;

    let document = {
        let _lock = crate::pdf::PDFIUM_LOCK.lock().unwrap();
        pdfium.load_pdf_from_byte_slice(bytes, None)?
    };

    let page_count = {
        let _lock = crate::pdf::PDFIUM_LOCK.lock().unwrap();
        document.pages().len() as usize
    };

    let end_page_index = start_page_index
        .saturating_add(requested_page_count)
        .min(page_count);
    let mut pages = Vec::with_capacity(end_page_index.saturating_sub(start_page_index));

    for page_index in start_page_index..end_page_index {
        let page_number = page_index + 1;
        let mut warnings = Vec::new();
        let mut render_clamped = false;

        let image = {
            let _lock = crate::pdf::PDFIUM_LOCK.lock().unwrap();
            match document.pages().get(page_index as i32) {
                Ok(page) => {
                    let (render_config, render_warning) =
                        render_config_for_page(&page, quality, page_number, false);
                    if let Some(warning) = render_warning {
                        render_clamped = true;
                        warnings.push(warning);
                    }

                    match page.render_with_config(&render_config) {
                        Ok(bitmap) => bitmap.as_image().map_err(|error| error.to_string()),
                        Err(error) => Err(error.to_string()),
                    }
                }
                Err(error) => Err(error.to_string()),
            }
        };

        let image = match image {
            Ok(image) => image,
            Err(error) => {
                pages.push(OcrPageExtraction {
                    page_index,
                    text: String::new(),
                    warnings,
                    render_clamped,
                    error: Some(error),
                });
                continue;
            }
        };

        let rgb_image = image.into_rgb8();
        let (width, height) = rgb_image.dimensions();

        let page_text = {
            let _ocr_lock = OCR_ENGINE_LOCK.lock().unwrap();
            let recognition = || -> anyhow::Result<String> {
                let image_source =
                    ocrs::ImageSource::from_bytes(rgb_image.as_raw(), (width, height))?;
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
                Ok(page_text)
            };

            match recognition() {
                Ok(text) => text,
                Err(error) => {
                    pages.push(OcrPageExtraction {
                        page_index,
                        text: String::new(),
                        warnings,
                        render_clamped,
                        error: Some(error.to_string()),
                    });
                    continue;
                }
            }
        };

        let text = page_text.trim().to_string();
        if text.is_empty() {
            warnings.push(format!("OCR produced no text for page {page_number}."));
        }

        pages.push(OcrPageExtraction {
            page_index,
            text,
            warnings,
            render_clamped,
            error: None,
        });
    }

    Ok((page_count, pages))
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
