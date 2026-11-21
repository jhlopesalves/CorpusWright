//! Capture adapter for the pinned public walkthrough. The desktop UI owns presentation
//! and interaction; this process delegates extraction, analysis and export to the core.
use corpuswright_core::analysis::concordance::index::SurfaceIndexCancellationToken;
use corpuswright_core::analysis::{
    AnalysisBuildOptions, AnalysisCorpus, CorpusOverviewContext, OverviewDistributionQuery,
    build_analysis_corpus, cleaning_config_signature, query_frequency_list,
    query_overview_distribution, resolve_analysis_input,
};
use corpuswright_core::corpus::{DocumentRecord, ScanReport, word_count::count_words_for_record};
use corpuswright_core::preparation::repeated_artifacts::{
    RepeatedArtifactScanConfig, scan_repeated_artifacts_report_with_cancel_and_cache,
};
use corpuswright_core::sources::{
    cache::{DocumentTextMode, ExtractionCache, document_text_for_record},
    pdf::ocr::{init_pdfium, set_ocr_resource_dir},
};
use corpuswright_core::{
    AnnotatedFrequencyQuery, AnnotationCacheAvailability, AnnotationCacheState, CancellationToken,
    CleaningConfig, ConcordanceCollocationQuery, ConcordanceDocument, ConcordanceQuery,
    ConcordanceResult, ConcordanceSortKey, ExportOptions, FrequencyMatchMode, FrequencyQuery,
    FrequencyTokenClass, PreviewOptions, SortDirection, TokenizationConfig, export_corpus,
    preview_files, preview_processed_files, query_annotated_frequency,
    query_concordance_collocations_documents, scan_directory, search_concordance_documents,
};
use serde_json::{Value, json};
use std::error::Error;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

struct CaptureCore {
    root: PathBuf,
    export_root: PathBuf,
    scan: ScanReport,
    cache: ExtractionCache,
    cleaning: CleaningConfig,
    analysis: Option<AnalysisCorpus>,
    concordance_documents: Option<Vec<ConcordanceDocument>>,
}

impl CaptureCore {
    fn new(root: PathBuf, export_root: PathBuf) -> Result<Self> {
        let scan = scan_directory(&root).map_err(|error| format!("{error:?}"))?;
        Ok(Self {
            root,
            export_root,
            scan,
            cache: ExtractionCache::new(),
            cleaning: CleaningConfig::default(),
            analysis: None,
            concordance_documents: None,
        })
    }

    fn build(&self, cleaning: &CleaningConfig, build_signature: &str) -> Result<AnalysisCorpus> {
        Ok(build_analysis_corpus(
            &self.scan.files,
            AnalysisBuildOptions {
                built_at: "2026-11-21T12:00:00Z".into(),
                build_signature: build_signature.into(),
                processing_config_signature: cleaning_config_signature(cleaning),
                overview_context: CorpusOverviewContext {
                    sources_in_project: Some(self.scan.files.len()),
                    ..Default::default()
                },
                udpipe_model_dir: None,
                force_compute_hashes: true,
                language_overrides: Default::default(),
                tokenization: TokenizationConfig::default(),
                annotation_cache_root: None,
            },
            &CancellationToken::new(),
            |record| {
                Ok(resolve_analysis_input(
                    record,
                    cleaning,
                    |record, config| {
                        self.document_text(record, config, DocumentTextMode::Processed)
                    },
                    |record, config| self.document_text(record, config, DocumentTextMode::Original),
                ))
            },
            |_| {},
        )?)
    }

    fn document_text(
        &self,
        record: &DocumentRecord,
        cleaning: &CleaningConfig,
        mode: DocumentTextMode,
    ) -> std::result::Result<String, String> {
        document_text_for_record(record, cleaning, mode, Some(&self.cache))
            .map(|document| document.text)
    }

    fn documents(
        &self,
        cleaning: &CleaningConfig,
        mode: DocumentTextMode,
    ) -> Result<Vec<ConcordanceDocument>> {
        self.scan
            .files
            .iter()
            .enumerate()
            .map(|(index, record)| {
                Ok(ConcordanceDocument {
                    document_index: index,
                    source_path: record.source_path.to_string_lossy().into_owned(),
                    display_name: record.relative_path.to_string_lossy().into_owned(),
                    text: self.document_text(record, cleaning, mode)?,
                })
            })
            .collect()
    }

    fn ranked(corpus: &AnalysisCorpus, token: &str, count: u64) -> Option<usize> {
        if count == 0 {
            return None;
        }
        // A frequency floor cannot change a surviving token's rank in this sort.
        let mut query = FrequencyQuery {
            selection: Default::default(),
            query: String::new(),
            match_mode: FrequencyMatchMode::Exact,
            minimum_frequency: count,
            token_class: FrequencyTokenClass::All,
            hide_stopwords: false,
            group_by: Default::default(),
            case_fold: true,
            language: None,
            pos_filter: None,
            sort_key: Default::default(),
            sort_direction: SortDirection::Desc,
            offset: 0,
            limit: 500,
        };
        loop {
            let page = query_frequency_list(corpus, &query);
            if let Some(index) = page.items.iter().position(|row| row.token == token) {
                return Some(query.offset + index + 1);
            }
            query.offset += page.items.len();
            if page.items.is_empty() || query.offset >= page.total_terms {
                return None;
            }
        }
    }

    fn audit(&self) -> Result<Value> {
        let analysis = self
            .analysis
            .as_ref()
            .ok_or("Build surface analysis before auditing")?;
        let original_config = CleaningConfig::default();
        let original_analysis = self.build(&original_config, "uncleaned-audit")?;
        let original = self.documents(&original_config, DocumentTextMode::Original)?;
        let prepared = self.documents(&self.cleaning, DocumentTextMode::Processed)?;
        let original_gutenberg = concordance(&original, "gutenberg")?;
        let prepared_gutenberg = concordance(&prepared, "gutenberg")?;
        let probability = concordance(&prepared, "probability")?;
        let phrase = concordance(&prepared, "in all probability")?;
        let outside_treatises = |result: &ConcordanceResult| {
            result
                .distribution
                .as_ref()
                .unwrap()
                .items
                .iter()
                .filter(|item| {
                    !matches!(
                        item.display_name.as_str(),
                        "15114-pdf.pdf" | "57359-0.txt" | "58881-0.txt" | "74864-0.txt"
                    )
                })
                .map(|item| item.count)
                .sum::<u64>()
        };
        Ok(json!({
            "facts": {
                "sources": self.scan.files.len(),
                "formats": 3,
                "originalTokens": original_gutenberg.selected_tokens_total,
                "preparedTokens": analysis.overview().totals.tokens,
                "preparedTypes": analysis.overview().totals.types,
                "tokensRemoved": original_gutenberg.selected_tokens_total - analysis.overview().totals.tokens,
                "originalGutenbergHits": original_gutenberg.total_hits,
                "originalGutenbergRange": original_gutenberg.documents_with_hits,
                "originalGutenbergDp": original_gutenberg.dp,
                "originalGutenbergRank": Self::ranked(&original_analysis, "gutenberg", original_gutenberg.total_hits as u64),
                "preparedGutenbergHits": prepared_gutenberg.total_hits,
                "preparedGutenbergRange": prepared_gutenberg.documents_with_hits,
                "preparedGutenbergDp": prepared_gutenberg.dp,
                "preparedGutenbergRank": Self::ranked(analysis, "gutenberg", prepared_gutenberg.total_hits as u64),
                "probabilityHits": probability.total_hits,
                "probabilityRange": probability.documents_with_hits,
                "probabilityDp": probability.dp,
                "probabilityRank": Self::ranked(analysis, "probability", probability.total_hits as u64),
                "phraseHits": phrase.total_hits,
                "probabilityOutsideTreatises": outside_treatises(&probability),
                "phraseOutsideTreatises": outside_treatises(&phrase),
                "rules": self.cleaning.removal_rules.len(),
            },
            "probabilityByDocument": distribution(&probability),
            "phraseByDocument": distribution(&phrase),
            "cleaningSignature": cleaning_config_signature(&self.cleaning),
        }))
    }

    fn dispatch(&mut self, command: &str, args: &Value) -> Result<Value> {
        let nested = args.get("request").unwrap_or(args);
        let config: CleaningConfig = args
            .get("cleaningConfig")
            .or_else(|| nested.get("cleaningConfig"))
            .map(|value| serde_json::from_value(value.clone()))
            .transpose()?
            .unwrap_or_else(|| self.cleaning.clone());
        let query = &args["query"];
        match command {
            "bootstrap" => Ok(serde_json::to_value(&self.scan)?),
            "compute_word_count_window_command" => {
                let offset = args["offset"].as_u64().unwrap_or(0) as usize;
                let limit = args["limit"].as_u64().unwrap_or(1000) as usize;
                let outcomes = self
                    .scan
                    .files
                    .iter()
                    .skip(offset)
                    .take(limit)
                    .map(|record| count_words_for_record(record, &config, Some(&self.cache)))
                    .collect::<Vec<_>>();
                Ok(json!({
                    "totalWords": outcomes.iter().map(|item| item.count).sum::<usize>(),
                    "skippedOcrMode": outcomes.iter().any(|item| item.skipped_ocr_mode),
                    "processedCount": outcomes.len(),
                    "totalCount": self.scan.files.len(),
                }))
            }
            "preview_selected" => {
                let paths = args["paths"].as_array().ok_or("Missing selection")?;
                let files = self
                    .scan
                    .files
                    .iter()
                    .filter(|record| {
                        let key = format!("Walkthrough corpus/{}", record.relative_path.display());
                        paths.iter().any(|path| path.as_str() == Some(&key))
                    })
                    .cloned()
                    .collect::<Vec<_>>();
                let options = PreviewOptions::default();
                let preview = if args["processed"].as_bool() == Some(true) {
                    preview_processed_files(&files, &options, &config, Some(&self.cache))
                } else {
                    preview_files(&files, &options, Some(&self.cache))
                }
                .map_err(|error| format!("{error:?}"))?;
                Ok(serde_json::to_value(preview.files)?)
            }
            "scan_repeated_artifacts_command" => Ok(serde_json::to_value(
                scan_repeated_artifacts_report_with_cancel_and_cache(
                    &self.scan.files,
                    &serde_json::from_value::<RepeatedArtifactScanConfig>(args["config"].clone())?,
                    &config,
                    Some(&self.cache),
                    &CancellationToken::new(),
                )?,
            )?),
            "build_analysis_corpus_command" => {
                let analysis = self.build(
                    &config,
                    nested["buildSignature"]
                        .as_str()
                        .unwrap_or("public-walkthrough"),
                )?;
                self.concordance_documents =
                    Some(self.documents(&config, DocumentTextMode::Processed)?);
                let overview = serde_json::to_value(analysis.overview())?;
                self.analysis = Some(analysis);
                self.cleaning = config;
                Ok(overview)
            }
            "audit" => self.audit(),
            "query_frequency_list_command" => Ok(serde_json::to_value(query_frequency_list(
                self.analysis.as_ref().ok_or("Missing analysis")?,
                &serde_json::from_value::<FrequencyQuery>(query.clone())?,
            ))?),
            "query_annotated_frequency_command" => {
                Ok(serde_json::to_value(query_annotated_frequency(
                    self.analysis.as_ref().ok_or("Missing analysis")?,
                    &serde_json::from_value::<AnnotatedFrequencyQuery>(query.clone())?,
                    AnnotationCacheAvailability {
                        state: AnnotationCacheState::Missing,
                        detail: None,
                    },
                ))?)
            }
            "query_overview_distribution_command" => {
                Ok(serde_json::to_value(query_overview_distribution(
                    self.analysis.as_ref().ok_or("Missing analysis")?,
                    &serde_json::from_value::<OverviewDistributionQuery>(query.clone())?,
                )?)?)
            }
            "search_concordance_command" => {
                Ok(serde_json::to_value(search_concordance_documents(
                    self.concordance_documents
                        .as_deref()
                        .ok_or("Missing concordance")?,
                    &serde_json::from_value::<ConcordanceQuery>(query.clone())?,
                    &TokenizationConfig::default(),
                )?)?)
            }
            "query_concordance_collocations_command" => Ok(serde_json::to_value(
                query_concordance_collocations_documents(
                    self.concordance_documents
                        .as_deref()
                        .ok_or("Missing concordance")?,
                    &serde_json::from_value::<ConcordanceCollocationQuery>(query.clone())?,
                    &TokenizationConfig::default(),
                    &SurfaceIndexCancellationToken::new(),
                )?,
            )?),
            "export_corpus_command" => {
                if config != self.cleaning {
                    return Err("Export and analysis must share cleaning settings".into());
                }
                let output = self.export_root.join(
                    args["outputDir"]
                        .as_str()
                        .ok_or("Missing export destination")?
                        .rsplit('/')
                        .next()
                        .ok_or("Missing export destination")?,
                );
                let report = export_corpus(
                    &self.scan.files,
                    &output,
                    &config,
                    &ExportOptions::default(),
                    None,
                    Some(&self.cache),
                )
                .map_err(|error| format!("{error:?}"))?;
                let manifest: Value =
                    serde_json::from_reader(std::fs::File::open(&report.manifest_path)?)?;
                let example = manifest["files"]
                    .as_array()
                    .and_then(|files| files.iter().find(|file| file["relative_path"] == "97.txt"))
                    .ok_or("Missing Flatland in export manifest")?;
                // Copy actual fields from the written file. Host paths and clock-specific values
                // stay in the scratch export; this excerpt can be made public.
                Ok(json!({
                    "files_exported": report.files_exported,
                    "exportExcerpt": {
                        "files_exported": manifest["files_exported"],
                        "warnings_count": manifest["warnings_count"],
                        "cleaning_config": {
                            "removal_rules": manifest["cleaning_config"]["removal_rules"]
                                .as_array()
                                .ok_or("Missing export removal rules")?
                                .iter()
                                .map(|rule| json!({ "enabled": rule["enabled"], "scope": rule["scope"] }))
                                .collect::<Vec<_>>(),
                        },
                        "files": [{
                            "relative_path": example["relative_path"],
                            "source_hash_sha256": example["source_hash_sha256"],
                            "processed_hash_sha256": example["processed_hash_sha256"],
                        }],
                    },
                }))
            }
            _ => Err(format!("Unsupported walkthrough command: {command}").into()),
        }
    }
}

fn concordance(documents: &[ConcordanceDocument], token: &str) -> Result<ConcordanceResult> {
    Ok(search_concordance_documents(
        documents,
        &ConcordanceQuery {
            selection: Default::default(),
            query: token.into(),
            context_filter: None,
            context_filter_direction: Default::default(),
            case_sensitive: false,
            sort_keys: vec![ConcordanceSortKey::FilePosition],
            window_tokens: 6,
            offset: 0,
            limit: 1,
            include_rows: false,
            distribution_offset: 0,
            distribution_limit: documents.len(),
            include_distribution: true,
        },
        &TokenizationConfig::default(),
    )?)
}

fn distribution(result: &ConcordanceResult) -> Value {
    json!(
        result
            .distribution
            .as_ref()
            .unwrap()
            .items
            .iter()
            .map(|item| { (&item.display_name, item.count) })
            .collect::<std::collections::BTreeMap<_, _>>()
    )
}

fn hide_scratch_paths(value: &mut Value, root: &Path) {
    match value {
        Value::String(text) => {
            *text = text.replace(root.to_string_lossy().as_ref(), "Walkthrough corpus")
        }
        Value::Object(values) => values
            .values_mut()
            .for_each(|item| hide_scratch_paths(item, root)),
        Value::Array(values) => values
            .iter_mut()
            .for_each(|item| hide_scratch_paths(item, root)),
        _ => {}
    }
}

fn main() -> Result<()> {
    let mut args = std::env::args_os().skip(1).map(PathBuf::from);
    let root = args.next().ok_or("Missing corpus")?;
    set_ocr_resource_dir(args.next().ok_or("Missing PDFium resources")?)?;
    let _ = init_pdfium()?;
    let mut core = CaptureCore::new(root, args.next().ok_or("Missing export root")?)?;
    let mut output = io::stdout().lock();
    for line in io::stdin().lock().lines() {
        let request: Value = serde_json::from_str(&line?)?;
        let result = core.dispatch(
            request["command"].as_str().ok_or("Missing command")?,
            &request["args"],
        );
        let mut reply = match result {
            Ok(value) => json!({"id": request["id"], "value": value}),
            Err(error) => json!({"id": request["id"], "error": error.to_string()}),
        };
        hide_scratch_paths(&mut reply, &core.root);
        writeln!(output, "{reply}")?;
        output.flush()?;
    }
    Ok(())
}
