use corpuswright_core::pdf_ocr::init_pdfium;
use lopdf::Document;
use pdfium_render::prelude::*;
use std::fs;
use std::path::Path;
use std::time::Instant;

#[derive(Clone, Debug)]
struct CharInfo {
    c: String,
    bottom: f32,
    left: f32,
    top: f32,
    right: f32,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let possible_paths = vec![
        Path::new(".local-corpora/pdf-benchmarks").to_path_buf(),
        Path::new("../../.local-corpora/pdf-benchmarks").to_path_buf(),
        Path::new("../.local-corpora/pdf-benchmarks").to_path_buf(),
    ];
    let benchmark_dir = match possible_paths.into_iter().find(|p| p.exists()) {
        Some(p) => p,
        None => {
            println!(
                "Benchmark directory .local-corpora/pdf-benchmarks not found in any of the search locations."
            );
            return Ok(());
        }
    };

    let paths: Vec<_> = fs::read_dir(benchmark_dir)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|ext| ext == "pdf"))
        .collect();

    if paths.is_empty() {
        println!("No PDF files found in .local-corpora/pdf-benchmarks.");
        return Ok(());
    }

    println!("Found {} PDF files to benchmark.", paths.len());
    println!();
    println!(
        "| File | Engine | Pages | Time (ms) | Char Count | U+FFFD Count | Reading Order Programmatic Check |"
    );
    println!("| --- | --- | --- | --- | --- | --- | --- |");

    // PDFium is initialised once because native access is serialised.
    let pdfium = match init_pdfium() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Failed to initialize PDFium runtime: {}", e);
            return Ok(());
        }
    };

    let artifact_dir = Path::new(
        "C:/Users/jhonm/.gemini/antigravity-ide/brain/04c58a98-8503-4862-b308-7375810882a5",
    );

    for path in &paths {
        let filename = path.file_name().unwrap().to_string_lossy();
        let bytes = fs::read(path)?;
        let is_target_file = filename.contains("advanced_r") || filename.contains("modern_stats_r");

        let mut lopdf_text = String::new();
        {
            let start = Instant::now();
            let mut char_count = 0;
            let mut fffd_count = 0;
            let mut page_count = 0;
            let mut doc_text = String::new();

            if let Ok(doc) = Document::load_mem(&bytes) {
                let pages = doc.get_pages();
                page_count = pages.len().min(10);
                for (page_number, _) in pages.iter().take(10) {
                    if let Ok(text) = doc.extract_text(&[*page_number]) {
                        char_count += text.chars().count();
                        fffd_count += text.chars().filter(|&c| c == '\u{FFFD}').count();
                        lopdf_text.push_str(&text);
                        lopdf_text.push_str("\n--- PAGE SPLIT ---\n");
                        doc_text.push_str(&text);
                        doc_text.push_str("\n--- PAGE SPLIT ---\n");
                    }
                }
            }
            let duration = start.elapsed().as_millis();
            let qual_note = evaluate_reading_order_concrete(&filename, &doc_text);
            println!(
                "| {} | lopdf (baseline) | {}/10 | {} | {} | {} | {} |",
                filename, page_count, duration, char_count, fffd_count, qual_note
            );
            if is_target_file {
                let out_name = if filename.contains("advanced_r") {
                    "advanced_r_lopdf.txt"
                } else {
                    "modern_stats_r_lopdf.txt"
                };
                fs::write(artifact_dir.join(out_name), &lopdf_text)?;
            }
        }

        let mut flat_text = String::new();
        {
            let start = Instant::now();
            let mut char_count = 0;
            let mut fffd_count = 0;
            let mut page_count = 0;
            let mut doc_text = String::new();

            if let Ok(doc) = pdfium.load_pdf_from_byte_slice(&bytes, None) {
                page_count = doc.pages().len().min(10);
                for page in doc.pages().iter().take(10) {
                    if let Ok(text_page) = page.text() {
                        let text = text_page.all();
                        char_count += text.chars().count();
                        fffd_count += text.chars().filter(|&c| c == '\u{FFFD}').count();
                        flat_text.push_str(&text);
                        flat_text.push_str("\n--- PAGE SPLIT ---\n");
                        doc_text.push_str(&text);
                        doc_text.push_str("\n--- PAGE SPLIT ---\n");
                    }
                }
            }
            let duration = start.elapsed().as_millis();
            let qual_note = evaluate_reading_order_concrete(&filename, &doc_text);
            println!(
                "| {} | pdfium (flat naive) | {}/10 | {} | {} | {} | {} |",
                filename, page_count, duration, char_count, fffd_count, qual_note
            );
            if is_target_file {
                let out_name = if filename.contains("advanced_r") {
                    "advanced_r_pdfium_flat.txt"
                } else {
                    "modern_stats_r_pdfium_flat.txt"
                };
                fs::write(artifact_dir.join(out_name), &flat_text)?;
            }
        }

        let mut layout_text = String::new();
        {
            let start = Instant::now();
            let mut char_count = 0;
            let mut fffd_count = 0;
            let mut page_count = 0;
            let mut doc_text = String::new();

            if let Ok(doc) = pdfium.load_pdf_from_byte_slice(&bytes, None) {
                page_count = doc.pages().len().min(10);
                for page in doc.pages().iter().take(10) {
                    if let Ok(text) = extract_page_layout_aware(&page) {
                        char_count += text.chars().count();
                        fffd_count += text.chars().filter(|&c| c == '\u{FFFD}').count();
                        layout_text.push_str(&text);
                        layout_text.push_str("\n--- PAGE SPLIT ---\n");
                        doc_text.push_str(&text);
                        doc_text.push_str("\n--- PAGE SPLIT ---\n");
                    }
                }
            }
            let duration = start.elapsed().as_millis();
            let qual_note = evaluate_reading_order_concrete(&filename, &doc_text);
            println!(
                "| {} | pdfium (layout-aware) | {}/10 | {} | {} | {} | {} |",
                filename, page_count, duration, char_count, fffd_count, qual_note
            );
            if is_target_file {
                let out_name = if filename.contains("advanced_r") {
                    "advanced_r_pdfium_layout.txt"
                } else {
                    "modern_stats_r_pdfium_layout.txt"
                };
                fs::write(artifact_dir.join(out_name), &layout_text)?;
            }
        }
    }

    Ok(())
}

fn evaluate_reading_order_concrete(filename: &str, text: &str) -> String {
    if text.is_empty() {
        return "Empty extract".to_string();
    }

    if filename.contains("modern_stats_r") {
        // The copyright probe expects trademarks before the edition notice.
        let idx_trademarks = text.find("trademarks");
        let idx_edition = text.find("Second edition published 2025");
        let copyright_ok = match (idx_trademarks, idx_edition) {
            (Some(t), Some(e)) => t < e,
            _ => false,
        };

        // The TOC probe expects the roman page number between heading and entry.
        let idx_figures = text.find("List of Figures");
        let idx_xvii = text.find("xvii");
        let idx_basics = text.find("2 The basics");
        let toc_ok = match (idx_figures, idx_xvii, idx_basics) {
            (Some(fig), Some(x), Some(b)) => fig < x && x < b,
            _ => false,
        };

        format!(
            "Copyright: {}, TOC: {}",
            if copyright_ok { "PASS" } else { "FAIL" },
            if toc_ok { "PASS" } else { "FAIL" }
        )
    } else if filename.contains("advanced_r") {
        // The Advanced R probe checks that Contents, Preface, and first entries stay ordered.
        let idx_contents = text.find("Contents");
        let idx_preface = text.find("Preface");
        let idx_intro = text
            .find("Introduction")
            .or_else(|| text.find("Intro duction"));
        let idx_why_r = text.find("Why R?");
        let idx_basics = text.find("2 Names and values");

        let toc_ok = match (idx_contents, idx_preface, idx_intro, idx_why_r, idx_basics) {
            (Some(c), Some(p), Some(i), Some(w), Some(b)) => c < p && p < i && i < w && w < b,
            _ => false,
        };

        format!("TOC Order: {}", if toc_ok { "PASS" } else { "FAIL" })
    } else {
        "N/A".to_string()
    }
}

fn extract_page_layout_aware(page: &PdfPage) -> Result<String, PdfiumError> {
    let text_page = page.text()?;
    let chars = text_page.chars();
    let mut char_infos = Vec::new();

    let mut last_valid_coords = None;

    for c in chars.iter() {
        let txt = match c.unicode_string() {
            Some(s) => s,
            None => continue,
        };
        if txt == "\n" || txt == "\r" || txt.as_bytes().first().is_some_and(|&b| b < 32) {
            continue;
        }

        let bounds = c.loose_bounds();
        let (bottom, left, top, right) = match bounds {
            Ok(b) => {
                let bottom = b.bottom().value;
                let left = b.left().value;
                let top = b.top().value;
                let right = b.right().value;

                if (right - left).abs() > 0.001 || (top - bottom).abs() > 0.001 {
                    last_valid_coords = Some((bottom, left, top, right));
                    (bottom, left, top, right)
                } else if txt == " " {
                    if let Some((b, _, t, r)) = last_valid_coords {
                        (b, r, t, r + 0.1)
                    } else {
                        continue;
                    }
                } else {
                    continue;
                }
            }
            Err(_) => {
                if txt == " " {
                    if let Some((b, _, t, r)) = last_valid_coords {
                        (b, r, t, r + 0.1)
                    } else {
                        continue;
                    }
                } else {
                    continue;
                }
            }
        };

        char_infos.push(CharInfo {
            c: txt,
            bottom,
            left,
            top,
            right,
        });
    }

    if char_infos.is_empty() {
        return Ok(String::new());
    }

    // Group into lines by Y midpoint
    char_infos.sort_by(|a, b| {
        let mid_a = (a.top + a.bottom) / 2.0;
        let mid_b = (b.top + b.bottom) / 2.0;
        mid_b
            .partial_cmp(&mid_a)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut lines: Vec<Vec<CharInfo>> = Vec::new();
    for char_info in char_infos {
        let mid = (char_info.top + char_info.bottom) / 2.0;
        let mut placed = false;
        for line in &mut lines {
            if let Some(first) = line.first() {
                let line_mid = (first.top + first.bottom) / 2.0;
                let line_h = first.top - first.bottom;
                let tolerance = (line_h * 0.4).max(4.0);
                if (mid - line_mid).abs() <= tolerance {
                    line.push(char_info.clone());
                    placed = true;
                    break;
                }
            }
        }
        if !placed {
            lines.push(vec![char_info]);
        }
    }

    // Sort lines by Y average coordinate descending (top to bottom)
    lines.sort_by(|line_a, line_b| {
        let avg_y_a =
            line_a.iter().map(|c| (c.top + c.bottom) / 2.0).sum::<f32>() / line_a.len() as f32;
        let avg_y_b =
            line_b.iter().map(|c| (c.top + c.bottom) / 2.0).sum::<f32>() / line_b.len() as f32;
        avg_y_b
            .partial_cmp(&avg_y_a)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut page_text = String::new();
    for mut line in lines {
        line.sort_by(|a, b| {
            a.left
                .partial_cmp(&b.left)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        let mut line_str = String::new();
        for (i, char_info) in line.iter().enumerate() {
            if i > 0 {
                let prev = &line[i - 1];
                let gap = char_info.left - prev.right;
                let char_w = char_info.right - char_info.left;

                // A gap greater than half a character width usually represents a space.
                if gap > char_w * 0.5 && prev.c != " " && char_info.c != " " {
                    line_str.push(' ');
                }
            }
            line_str.push_str(&char_info.c);
        }
        let trimmed = line_str.trim();
        if !trimmed.is_empty() {
            if !page_text.is_empty() {
                page_text.push('\n');
            }
            page_text.push_str(trimmed);
        }
    }
    Ok(page_text)
}
