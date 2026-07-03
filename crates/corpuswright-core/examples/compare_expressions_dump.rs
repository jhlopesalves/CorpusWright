use corpuswright_core::analysis::{
    AnalysisDocumentSelection, AnalysisInputProvenance, AnalysisInputResolution,
    CompareExpressionsQuery, ConcordanceDocument, compare_expressions, resolve_analysis_input,
};
use corpuswright_core::cache::{DocumentTextMode, ExtractionCache, document_text_for_record};
use corpuswright_core::clean::CleaningConfig;
use corpuswright_core::scan::scan_directory;
use corpuswright_core::tokenization::TokenizationConfig;
use std::env;
use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::process;

struct Cli {
    input_dir: PathBuf,
    expressions: Vec<String>,
    case_sensitive: bool,
    min_document_tokens: u64,
    out: Option<PathBuf>,
}

fn usage(program: &str) -> String {
    format!(
        "Usage: {program} <input_directory> --expr \"I like it\" --expr \"I love it\" [--case-sensitive] [--min-document-tokens N] [--out /path/dump.csv]"
    )
}

fn parse_args() -> Result<Cli, String> {
    let args = env::args().collect::<Vec<_>>();
    if args.len() < 2 {
        return Err(usage(&args[0]));
    }
    let mut index = 2usize;
    let input_dir = PathBuf::from(&args[1]);
    let mut expressions = Vec::new();
    let mut case_sensitive = false;
    let mut min_document_tokens = 0u64;
    let mut out = None;

    while index < args.len() {
        match args[index].as_str() {
            "--expr" => {
                index += 1;
                let expression = args
                    .get(index)
                    .ok_or_else(|| "--expr requires a value.".to_string())?;
                expressions.push(expression.clone());
            }
            "--case-sensitive" => {
                case_sensitive = true;
            }
            "--min-document-tokens" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| "--min-document-tokens requires a value.".to_string())?;
                min_document_tokens = value.parse::<u64>().map_err(|_| {
                    "--min-document-tokens must be a non-negative integer.".to_string()
                })?;
            }
            "--out" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| "--out requires a path.".to_string())?;
                out = Some(PathBuf::from(value));
            }
            "--help" | "-h" => {
                return Err(usage(&args[0]));
            }
            unknown => {
                return Err(format!("Unknown argument: {unknown}\n{}", usage(&args[0])));
            }
        }
        index += 1;
    }

    if expressions.len() < 2 {
        return Err("Provide at least two --expr values.".to_string());
    }
    if expressions.len() > 6 {
        return Err("Provide no more than six --expr values.".to_string());
    }

    Ok(Cli {
        input_dir,
        expressions,
        case_sensitive,
        min_document_tokens,
        out,
    })
}

fn resolve_document_text(
    record: &corpuswright_core::scan::DocumentRecord,
    cleaning_config: &CleaningConfig,
    cache: &ExtractionCache,
) -> AnalysisInputResolution {
    resolve_analysis_input(
        record,
        cleaning_config,
        |record, cleaning_config| {
            document_text_for_record(
                record,
                cleaning_config,
                DocumentTextMode::Processed,
                Some(cache),
            )
            .map(|document| document.text)
        },
        |record, cleaning_config| {
            document_text_for_record(
                record,
                cleaning_config,
                DocumentTextMode::Original,
                Some(cache),
            )
            .map(|document| document.text)
        },
    )
}

fn text_for_compare_resolution(resolution: AnalysisInputResolution) -> Option<String> {
    match resolution.provenance {
        AnalysisInputProvenance::Processed | AnalysisInputProvenance::SourceTextFallback => {
            resolution.text
        }
        AnalysisInputProvenance::EmptyOrFailed => None,
    }
    .filter(|value| !value.trim().is_empty())
}

fn concordance_document_from_record(
    record: &corpuswright_core::scan::DocumentRecord,
    document_index: usize,
    cleaning_config: &CleaningConfig,
    cache: &ExtractionCache,
) -> Option<ConcordanceDocument> {
    let resolution = resolve_document_text(record, cleaning_config, cache);
    let text = text_for_compare_resolution(resolution)?;
    let source_path = record.source_path.to_string_lossy().into_owned();
    let relative_path = record.relative_path.to_string_lossy().into_owned();
    Some(ConcordanceDocument {
        document_index,
        source_path: source_path.clone(),
        display_name: if relative_path.is_empty() {
            source_path
        } else {
            relative_path
        },
        text,
    })
}

fn csv_cell(value: &str) -> String {
    if value.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

fn format_optional_rate(rate: Option<f64>) -> String {
    rate.map(|value| value.to_string()).unwrap_or_default()
}

fn write_dump(
    mut writer: impl Write,
    corpus_path: &Path,
    result: &corpuswright_core::analysis::CompareExpressionsResult,
) -> io::Result<()> {
    writeln!(writer, "# tool: compare_expressions_dump")?;
    writeln!(writer, "# corpus_path: {}", corpus_path.display())?;
    for (index, expression) in result.expressions_queried.iter().enumerate() {
        writeln!(writer, "# expression_{index}: {expression}")?;
    }
    writeln!(writer, "# case_sensitive: {}", result.case_sensitive)?;
    writeln!(
        writer,
        "# min_document_tokens: {}",
        result.min_document_tokens
    )?;
    writeln!(writer, "# normalisation: counts per 10,000 document tokens")?;
    writeln!(
        writer,
        "# cleaning_config: default; may differ from project-specific app settings"
    )?;
    writeln!(
        writer,
        "# selected_documents_total: {}",
        result.selected_documents_total
    )?;
    writeln!(
        writer,
        "# selected_tokens_total: {}",
        result.selected_tokens_total
    )?;
    for (index, summary) in result.expressions.iter().enumerate() {
        writeln!(
            writer,
            "# expression_summary_{index}: expression={}, total_hits={}, documents_with_hits={}, rate_per_10k={}, rate_per_million={}, dp={}, dispersion_label={}, top_document_share={}",
            summary.expression,
            summary.total_hits,
            summary.documents_with_hits,
            summary.rate_per_10k,
            summary.rate_per_million,
            summary.dp,
            summary.dispersion_label,
            summary.top_document_share
        )?;
    }

    let mut header = vec![
        "document_id".to_string(),
        "display_name".to_string(),
        "token_count".to_string(),
        "included_in_rate".to_string(),
    ];
    for index in 0..result.expressions_queried.len() {
        header.push(format!("expr{index}_count"));
        header.push(format!("expr{index}_per_10k"));
    }
    writeln!(writer, "{}", header.join(","))?;

    for row in &result.documents {
        let mut cells = vec![
            row.document_id.to_string(),
            csv_cell(&row.display_name),
            row.token_count.to_string(),
            row.included_in_rate.to_string(),
        ];
        for cell in &row.cells {
            cells.push(cell.count.to_string());
            cells.push(format_optional_rate(cell.rate_per_10k));
        }
        writeln!(writer, "{}", cells.join(","))?;
    }

    Ok(())
}

fn run() -> Result<(), String> {
    let cli = parse_args()?;
    let report = scan_directory(&cli.input_dir).map_err(|error| format!("{error:?}"))?;
    if report.files.is_empty() {
        return Err("No supported files found.".to_string());
    }

    let cleaning_config = CleaningConfig::default();
    let cache = ExtractionCache::new();
    let mut documents = Vec::new();
    for record in &report.files {
        let document_index = documents.len();
        if let Some(document) =
            concordance_document_from_record(record, document_index, &cleaning_config, &cache)
        {
            documents.push(document);
        }
    }
    if documents.is_empty() {
        return Err("No documents produced analysable text.".to_string());
    }

    let query = CompareExpressionsQuery {
        selection: AnalysisDocumentSelection::default(),
        expressions: cli.expressions,
        case_sensitive: cli.case_sensitive,
        min_document_tokens: cli.min_document_tokens,
        offset: 0,
        limit: documents.len().max(1),
    };
    let result = compare_expressions(&documents, &query, &TokenizationConfig::default())
        .map_err(|error| error.to_string())?;
    if result.documents.len() != result.total_documents {
        return Err(format!(
            "Compare returned {} of {} rows; this dump requires a single unpaginated result.",
            result.documents.len(),
            result.total_documents
        ));
    }

    match cli.out {
        Some(path) => {
            let file = File::create(&path)
                .map_err(|error| format!("Failed to create {}: {error}", path.display()))?;
            write_dump(BufWriter::new(file), &report.summary.root, &result)
                .map_err(|error| error.to_string())
        }
        None => {
            let stdout = io::stdout();
            write_dump(stdout.lock(), &report.summary.root, &result)
                .map_err(|error| error.to_string())
        }
    }
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        process::exit(1);
    }
}
