use ocrs::{OcrEngine, OcrEngineParams};
use pdfium_render::prelude::*;
use rten::Model;
use std::sync::OnceLock;

static OCR_ENGINE: OnceLock<OcrEngine> = OnceLock::new();
static PDFIUM_LIBRARY: OnceLock<Pdfium> = OnceLock::new();

/// Returns true if PDFium can be initialized on the current platform.
/// Useful for guarding tests that depend on the native PDFium library.
#[cfg(test)]
pub(crate) fn pdfium_available() -> bool {
    // Quick check without caching: try to bind to the library
    Pdfium::bind_to_system_library()
        .or_else(|_| {
            Pdfium::bind_to_library(Pdfium::pdfium_platform_library_name_at_path("./ocr/"))
        })
        .is_ok()
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

    let current_exe = std::env::current_exe()?;
    let exe_dir = current_exe
        .parent()
        .unwrap_or_else(|| std::path::Path::new(""));

    let possible_paths = vec![
        exe_dir.join("ocr"),                    // Production or adjacent
        exe_dir.join("../../../resources/ocr"), // Dev fallback: target/debug/../../../resources/ocr
        exe_dir.join("../../../../apps/desktop/src-tauri/resources/ocr"), // Dev fallback when running examples in core
        std::path::PathBuf::from("resources/ocr"),                        // CWD relative fallback
        std::path::PathBuf::from("../../apps/desktop/src-tauri/resources/ocr"), // CWD relative fallback when in core crate
        std::path::PathBuf::from("apps/desktop/src-tauri/resources/ocr"), // CWD relative fallback when in root
    ];
    let ocr_res_dir = possible_paths
        .into_iter()
        .find(|p| p.exists())
        .unwrap_or_else(|| std::path::PathBuf::from("ocr"));

    // Attempt standard locations
    let pdfium = Pdfium::new(
        Pdfium::bind_to_system_library()
            .or_else(|_| {
                Pdfium::bind_to_library(Pdfium::pdfium_platform_library_name_at_path(
                    ocr_res_dir.to_string_lossy().as_ref(),
                ))
            })
            .or_else(|_| {
                Pdfium::bind_to_library(Pdfium::pdfium_platform_library_name_at_path("./ocr/"))
            })
            .map_err(|e| anyhow::anyhow!("Failed to bind to PDFium: {:?}", e))?,
    );

    PDFIUM_LIBRARY
        .set(pdfium)
        .map_err(|_| anyhow::anyhow!("Failed to set PDFIUM_LIBRARY"))?;
    Ok(PDFIUM_LIBRARY.get().unwrap())
}

fn init_ocr_engine() -> anyhow::Result<&'static OcrEngine> {
    if let Some(engine) = OCR_ENGINE.get() {
        return Ok(engine);
    }

    let current_exe = std::env::current_exe()?;
    let exe_dir = current_exe
        .parent()
        .unwrap_or_else(|| std::path::Path::new(""));

    let possible_paths = vec![
        exe_dir.join("ocr"),                    // Production or adjacent
        exe_dir.join("../../../resources/ocr"), // Dev fallback: target/debug/../../../resources/ocr
        exe_dir.join("../../../../apps/desktop/src-tauri/resources/ocr"), // Dev fallback when running examples in core
        std::path::PathBuf::from("resources/ocr"),                        // CWD relative fallback
        std::path::PathBuf::from("../../apps/desktop/src-tauri/resources/ocr"), // CWD relative fallback when in core crate
        std::path::PathBuf::from("apps/desktop/src-tauri/resources/ocr"), // CWD relative fallback when in root
    ];
    let ocr_res_dir = possible_paths
        .into_iter()
        .find(|p| p.exists())
        .unwrap_or_else(|| std::path::PathBuf::from("ocr"));

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
