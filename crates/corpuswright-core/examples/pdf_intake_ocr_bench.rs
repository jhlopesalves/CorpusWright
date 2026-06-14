use corpuswright_core::clean::{
    CleaningConfig, PdfEmbeddedTextStrategy, PdfOcrQuality, PdfTextSource,
};
use corpuswright_core::pdf::{PdfExtractionOptions, extract_pdf_page_range, pdf_page_count};
use corpuswright_core::scan::load_files;
use corpuswright_core::{DocumentType, pdf_ocr};
use rayon::prelude::*;
use std::env;
use std::path::PathBuf;
use std::time::{Duration, Instant};

const DEFAULT_OCR_WORKERS: usize = 3;
const MAX_INTERNAL_OCR_WORKERS: usize = 4;
const DEFAULT_START_PAGE: usize = 25;
const DEFAULT_END_PAGE: usize = 40;
const DEFAULT_PHRASE: &str = "frequency is not that important";

#[derive(Debug)]
struct Cli {
    pdf_path: PathBuf,
    start_page: usize,
    end_page: usize,
    phrase: String,
}

#[derive(Debug)]
struct WorkerConfig {
    worker_count: usize,
    available_parallelism: usize,
    env_value: Option<String>,
}

#[derive(Debug)]
struct BenchPageTiming {
    page_index: usize,
    page_total_ms: u64,
    chars_extracted: usize,
    warnings: Vec<String>,
    render_clamped: bool,
    phrase_found: bool,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = parse_cli()?;

    if cli.start_page == 0 {
        return Err("start page must be 1 or greater".into());
    }
    if cli.end_page < cli.start_page {
        return Err("end page must be greater than or equal to start page".into());
    }
    if !cli.pdf_path.is_file() {
        return Err(format!("PDF not found: {}", cli.pdf_path.display()).into());
    }

    let report = load_files(vec![cli.pdf_path.clone()]).map_err(|error| format!("{error:?}"))?;
    let record = report
        .files
        .first()
        .ok_or("load_files returned no document record")?;
    if record.document_type != DocumentType::Pdf {
        return Err("benchmark input must be a PDF".into());
    }

    let bytes = std::fs::read(&record.source_path)?;
    let total_pages = pdf_page_count(&bytes).map_err(|error| error.to_string())?;
    if cli.end_page > total_pages {
        return Err(format!(
            "page range {}-{} exceeds PDF page count {}",
            cli.start_page, cli.end_page, total_pages
        )
        .into());
    }

    let cleaning_config = CleaningConfig {
        pdf_text_source: PdfTextSource::ForceOcr,
        pdf_ocr_quality: PdfOcrQuality::HighQuality,
        pdf_embedded_text_strategy: PdfEmbeddedTextStrategy::PdfiumFlat,
        ..CleaningConfig::default()
    };
    let options = PdfExtractionOptions::raw_from_cleaning_config(&cleaning_config);
    let worker_config = configured_ocr_worker_count();
    let build_mode = if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    };

    println!("PDF intake OCR materialisation benchmark");
    println!("build_mode={build_mode}");
    println!(
        "materialisation_path=desktop force-OCR materialisation loop shape; per page uses extract_pdf_page_range(bytes, page_index, 1, PdfExtractionOptions::raw_from_cleaning_config(force OCR high quality), None)"
    );
    println!("pdf_path={}", record.source_path.display());
    println!("file_name={}", record.relative_path.display());
    println!("pdf_page_count={total_pages}");
    println!("ocr_quality={:?}", cleaning_config.pdf_ocr_quality);
    println!("pdf_text_source={:?}", cleaning_config.pdf_text_source);
    println!(
        "embedded_text_strategy={:?}",
        cleaning_config.pdf_embedded_text_strategy
    );
    println!(
        "workers={} available_parallelism={} env_CORPUSWRIGHT_OCR_WORKERS={}",
        worker_config.worker_count,
        worker_config.available_parallelism,
        worker_config.env_value.as_deref().unwrap_or("<unset>")
    );
    println!(
        "page_range={}..={} one_based_inclusive",
        cli.start_page, cli.end_page
    );
    println!("cache_policy=cold process; no persistent cache; no in-process cache read");
    println!("ocr_model_dir={}", ocr_model_dir_label());
    println!("model_initialisation=lazy, included in first page and total timing");
    println!("pdfium_rendering=serialised by PDFIUM_LOCK during each page render");
    println!("ocr_recognition=serialised by shared OCR_ENGINE_LOCK");
    println!("ocr_engines=one process-wide shared OCR engine");
    println!("models_loaded=once per process on first OCR extraction");

    let pool_started_at = Instant::now();
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(worker_config.worker_count)
        .build()?;
    let pool_build_ms = duration_ms(pool_started_at.elapsed());

    let start_index = cli.start_page - 1;
    let end_index = cli.end_page;
    let phrase_lower = cli.phrase.to_lowercase();

    let started_at = Instant::now();
    let mut timings = Vec::with_capacity(end_index - start_index);
    let mut next_page_index = start_index;
    while next_page_index < end_index {
        let batch_end = next_page_index
            .saturating_add(worker_config.worker_count)
            .min(end_index);
        let mut batch_results = pool.install(|| {
            (next_page_index..batch_end)
                .into_par_iter()
                .map(|page_index| {
                    extract_materialized_ocr_page(&bytes, page_index, options, &phrase_lower)
                })
                .collect::<Result<Vec<_>, _>>()
        })?;
        batch_results.sort_by_key(|page| page.page_index);
        timings.extend(batch_results);
        next_page_index = batch_end;
    }
    let total_elapsed = started_at.elapsed();

    print_summary(
        &timings,
        total_elapsed,
        pool_build_ms,
        cli.start_page,
        cli.end_page,
        &cli.phrase,
    );

    Ok(())
}

fn parse_cli() -> Result<Cli, Box<dyn std::error::Error>> {
    let mut pdf_path = None;
    let mut start_page = DEFAULT_START_PAGE;
    let mut end_page = DEFAULT_END_PAGE;
    let mut phrase = DEFAULT_PHRASE.to_string();
    let mut args = env::args().skip(1);

    while let Some(arg) = args.next() {
        if arg == "--help" || arg == "-h" {
            print_usage();
            std::process::exit(0);
        } else if arg == "--start-page" {
            start_page = parse_page_arg("--start-page", args.next())?;
        } else if let Some(value) = arg.strip_prefix("--start-page=") {
            start_page = value.parse()?;
        } else if arg == "--end-page" {
            end_page = parse_page_arg("--end-page", args.next())?;
        } else if let Some(value) = arg.strip_prefix("--end-page=") {
            end_page = value.parse()?;
        } else if arg == "--phrase" {
            phrase = args.next().ok_or("--phrase requires a value")?;
        } else if let Some(value) = arg.strip_prefix("--phrase=") {
            phrase = value.to_string();
        } else if arg.starts_with('-') {
            return Err(format!("unknown option: {arg}").into());
        } else if pdf_path.is_none() {
            pdf_path = Some(PathBuf::from(arg));
        } else {
            return Err(format!("unexpected argument: {arg}").into());
        }
    }

    let pdf_path = pdf_path.ok_or("PDF path is required")?;
    Ok(Cli {
        pdf_path,
        start_page,
        end_page,
        phrase,
    })
}

fn parse_page_arg(
    name: &'static str,
    value: Option<String>,
) -> Result<usize, Box<dyn std::error::Error>> {
    value
        .ok_or_else(|| format!("{name} requires a value"))?
        .parse()
        .map_err(Into::into)
}

fn print_usage() {
    println!("Usage: pdf_intake_ocr_bench <pdf> [--start-page N] [--end-page N] [--phrase TEXT]");
    println!("Set CORPUSWRIGHT_OCR_WORKERS=1..4 to match the desktop app worker override.");
}

fn configured_ocr_worker_count() -> WorkerConfig {
    let available_parallelism = std::thread::available_parallelism()
        .map(|count| count.get())
        .unwrap_or(DEFAULT_OCR_WORKERS)
        .max(1);
    let default_workers = DEFAULT_OCR_WORKERS.min(available_parallelism).max(1);
    let env_value = env::var("CORPUSWRIGHT_OCR_WORKERS").ok();
    let worker_count = env_value
        .as_deref()
        .and_then(|value| value.trim().parse::<usize>().ok())
        .filter(|value| *value > 0)
        .map(|value| {
            value
                .min(MAX_INTERNAL_OCR_WORKERS)
                .min(available_parallelism)
                .max(1)
        })
        .unwrap_or(default_workers);

    WorkerConfig {
        worker_count,
        available_parallelism,
        env_value,
    }
}

fn extract_materialized_ocr_page(
    bytes: &[u8],
    page_index: usize,
    options: PdfExtractionOptions,
    phrase_lower: &str,
) -> Result<BenchPageTiming, String> {
    let page_started_at = Instant::now();
    let result = extract_pdf_page_range(bytes, page_index, 1, options, None)
        .map_err(|error| format!("PDF materialisation failed: {error}"))?;
    let mut warnings = result.warnings;
    let mut render_clamped = false;
    let mut text = String::new();

    match result.pages.into_iter().next() {
        Some(page) => {
            render_clamped = page.render_clamped;
            warnings.extend(page.warnings);
            if let Some(page_error) = page.error {
                warnings.push(format!(
                    "Page {} extraction failed during materialisation: {}",
                    page.page_number, page_error
                ));
            } else {
                text = page.text.trim().to_string();
            }
        }
        None => {
            warnings.push(format!(
                "Page {} extraction returned no page result.",
                page_index + 1
            ));
        }
    }

    Ok(BenchPageTiming {
        page_index,
        page_total_ms: duration_ms(page_started_at.elapsed()),
        chars_extracted: text.chars().count(),
        warnings,
        render_clamped,
        phrase_found: text.to_lowercase().contains(phrase_lower),
    })
}

fn print_summary(
    timings: &[BenchPageTiming],
    total_elapsed: Duration,
    pool_build_ms: u64,
    start_page: usize,
    end_page: usize,
    phrase: &str,
) {
    let pages = timings.len();
    let total_ms = duration_ms(total_elapsed);
    let pages_per_minute = pages as f64 / (total_elapsed.as_secs_f64() / 60.0).max(0.001);
    let total_page_ms: u64 = timings.iter().map(|timing| timing.page_total_ms).sum();
    let average_page_ms = if pages == 0 {
        0.0
    } else {
        total_page_ms as f64 / pages as f64
    };
    let total_chars: usize = timings.iter().map(|timing| timing.chars_extracted).sum();
    let average_chars = if pages == 0 {
        0.0
    } else {
        total_chars as f64 / pages as f64
    };
    let warning_count: usize = timings.iter().map(|timing| timing.warnings.len()).sum();
    let clamped_pages = timings
        .iter()
        .filter(|timing| timing.render_clamped)
        .count();
    let phrase_page = timings
        .iter()
        .find(|timing| timing.page_index + 1 == 32)
        .map(|timing| timing.phrase_found);
    let mut slowest_pages: Vec<&BenchPageTiming> = timings.iter().collect();
    slowest_pages.sort_by_key(|timing| std::cmp::Reverse(timing.page_total_ms));
    let slowest_pages = slowest_pages
        .into_iter()
        .take(5)
        .map(|timing| {
            format!(
                "p{}={}ms/{}chars",
                timing.page_index + 1,
                timing.page_total_ms,
                timing.chars_extracted
            )
        })
        .collect::<Vec<_>>()
        .join(", ");

    println!("pool_build_ms={pool_build_ms}");
    println!("pages_processed={pages}");
    println!("total_ms={total_ms}");
    println!("pages_per_minute={pages_per_minute:.2}");
    println!("average_page_ms={average_page_ms:.1}");
    println!("average_chars_per_page={average_chars:.1}");
    println!("warnings={warning_count}");
    println!("render_clamped_pages={clamped_pages}");
    println!("slowest_pages=[{slowest_pages}]");
    println!(
        "page_32_phrase=\"{phrase}\" found={}",
        phrase_page_label(phrase_page)
    );
    println!(
        "requested_page_range={}..={} one_based_inclusive",
        start_page, end_page
    );
}

fn phrase_page_label(value: Option<bool>) -> &'static str {
    match value {
        Some(true) => "true",
        Some(false) => "false",
        None => "not_in_range",
    }
}

fn duration_ms(duration: Duration) -> u64 {
    duration.as_millis().min(u128::from(u64::MAX)) as u64
}

fn ocr_model_dir_label() -> String {
    pdf_ocr::first_existing_ocr_model_dir()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| "<missing>".to_string())
}
