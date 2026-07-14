use corpuswright_core::analysis::{
    AnalysisBuildOptions, AnalysisDocumentSelection, AnalysisInputProvenance,
    AnalysisInputResolution, AnalysisInputStatus, CorpusOverview, CorpusOverviewContext,
    FrequencyGroupMode, FrequencyMatchMode, FrequencyQuery, FrequencySortKey, FrequencyTokenClass,
    OverviewDistributionMetric, OverviewDistributionQuery, OverviewDistributionScale,
    OverviewHistogramRule, OverviewQqReference, SortDirection, build_analysis_corpus,
    cleaning_config_signature, query_frequency_list, query_overview_distribution,
    query_term_distribution,
};
use corpuswright_core::documentation_corpus::{
    CandidateEvaluation, CandidateScoringInput, GutenbergCatalogRecord, GutenbergCleanResult,
    RetrievalPolicy, SeedTechnicalAudit, clean_gutenberg_text, corpus_checksum,
    detect_duplicate_content, is_publishable_status, manifest_paths_are_strictly_ordered,
    parse_gutenberg_catalog, score_candidate, sha256_hex, technical_audit,
};
use corpuswright_core::scan::{DocumentRecord, load_files};
use corpuswright_core::tokenization::{TokenizationConfig, TokenizationManifest, tokenize};
use corpuswright_core::{CleaningConfig, scan_directory};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::sync::atomic::AtomicBool;
use std::thread;
use std::time::{Duration, Instant};

const CORPUS_ID: &str = "corpuswright-documentation-books-v1";
const CORPUS_TITLE: &str = "CorpusWright Documentation Books Corpus";
const CATALOGUE_URL: &str = "https://www.gutenberg.org/cache/epub/feeds/pg_catalog.csv";
const MIRROR_BASE: &str = "https://gutenberg.pglaf.org/cache/epub";
const USER_AGENT: &str = "CorpusWright/0.1 (+https://github.com/jhlopesalves/CorpusWright)";
const SNAPSHOT_RETRIEVED_AT: &str = "2026-07-14T12:50:45Z";
const BASELINE_COMMIT: &str = "439205bd005937bf01d5298d8ff4499b4cea0d18";
const BRAZIL_COPYRIGHT_SOURCE: &str = "https://www.gov.br/cultura/pt-br/assuntos/direitos-autorais/perguntas-frequentes/perguntas-frequentes";
const SELECTION_LIMIT: usize = 12;
const EDITORIAL_PROFILE: &str = "Canonical adult literary writing, principally nineteenth- and early-twentieth-century fiction, drama, essays, and limited poetry, with Gothic, realist, decadent, modernist, philosophical, social, or political seriousness and enough linguistic substance for corpus diagnostics.";

type BoxError = Box<dyn std::error::Error + Send + Sync + 'static>;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SeedMetadataFile {
    schema_version: u32,
    corpus_id: String,
    policy: String,
    seeds: Vec<SeedMetadata>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SeedMetadata {
    #[serde(alias = "path")]
    source_path: String,
    title: String,
    author: String,
    translator: Option<String>,
    original_publication_year: Option<u16>,
    edition_publication_year: Option<u16>,
    probable_source: String,
    us_project_gutenberg_ebook_id: Option<u64>,
    redistribution_status: String,
    planned_disposition: String,
    qualification_warnings: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CandidatePlan {
    schema_version: u32,
    selection_limit: usize,
    profile: String,
    candidates: Vec<CandidatePlanEntry>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CandidatePlanEntry {
    ebook_id: u64,
    canonical_filename: String,
    original_publication_year: Option<u16>,
    translator: Option<String>,
    profile_compatibility: u8,
    analytical_contribution: u8,
    genre_balance: u8,
    profile_rationale: String,
    balance_contribution: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SelectionPlanRow {
    source_kind: String,
    source_id: String,
    canonical_filename: String,
    original_publication_year: Option<u16>,
    translator: Option<String>,
    profile_compatibility: Option<u8>,
    analytical_contribution: Option<u8>,
    genre_balance: Option<u8>,
    profile_rationale: String,
    balance_contribution: String,
}

#[derive(Debug, Clone)]
struct RetrievedEbook {
    url: String,
    raw: Vec<u8>,
    raw_sha256: String,
}

#[derive(Debug, Clone)]
struct PreparedEbook {
    retrieval: RetrievedEbook,
    cleaned: GutenbergCleanResult,
    cleaned_sha256: String,
    tokens: u64,
    types: usize,
}

#[derive(Debug, Clone)]
struct EvaluatedCandidate {
    plan: CandidatePlanEntry,
    catalog: GutenbergCatalogRecord,
    prepared: Option<PreparedEbook>,
    evaluation: CandidateEvaluation,
    author: String,
    selected: bool,
    selection_reason: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DocumentManifest {
    path: String,
    filename: String,
    title: String,
    author: String,
    translator: Option<String>,
    original_publication_year: Option<u16>,
    edition_publication_year: Option<u16>,
    source: String,
    source_id: String,
    source_landing_page: String,
    public_domain_basis: String,
    redistribution_status: String,
    raw_sha256: String,
    cleaned_sha256: String,
    bytes: u64,
    tokens: u64,
    types: usize,
    qualification_status: String,
    qualification_warnings: Vec<String>,
}

#[derive(Debug, Clone)]
struct AcceptedDocument {
    manifest: DocumentManifest,
    text: String,
    cleaning_report: Value,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AnalysisPayload<'a> {
    schema_version: u32,
    corpus_id: &'a str,
    corpus_checksum: &'a str,
    overview: &'a CorpusOverview,
    document_distribution: Value,
    mattr_distribution: Value,
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("documentation corpus pipeline failed: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), BoxError> {
    let repository_root = repository_root()?;
    let corpus_root = repository_root.join("examples/corpora/documentation-books");
    let raw_cache = env::var_os("CORPUSWRIGHT_DOCUMENTATION_RAW_CACHE")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp/corpuswright-documentation-books-raw"));
    let dossier_root = env::var_os("CORPUSWRIGHT_DOCUMENTATION_DOSSIER")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp/corpuswright-documentation-corpus-dossier"));
    let catalogue_path = raw_cache.join("catalog/pg_catalog.csv");
    ensure_catalogue(&catalogue_path)?;

    let seed_metadata: SeedMetadataFile = read_json(&corpus_root.join("seed-manifest.json"))?;
    let candidate_plan = read_candidate_plan(&corpus_root.join("selection.csv"))?;
    validate_plans(&seed_metadata, &candidate_plan)?;
    let catalog_bytes = fs::read(&catalogue_path)?;
    let catalog_sha256 = sha256_hex(&catalog_bytes);
    let catalog = parse_gutenberg_catalog(catalog_bytes.as_slice())?;
    let by_id = catalog
        .into_iter()
        .map(|record| (record.ebook_id, record))
        .collect::<HashMap<_, _>>();

    let seed_audits = audit_seeds(&repository_root, &corpus_root, &raw_cache, &seed_metadata)?;
    let requested_ids = requested_ebook_ids(&seed_metadata, &candidate_plan);
    let retrieval_policy = RetrievalPolicy::default().validate()?;
    let retrieved = retrieve_ebooks(&raw_cache, &requested_ids, retrieval_policy)?;
    let prepared = prepare_retrieved(retrieved);
    let accepted_seed_authors = seed_metadata
        .seeds
        .iter()
        .filter(|seed| is_publishable_status(&seed.redistribution_status))
        .map(|seed| seed.author.to_lowercase())
        .collect::<HashSet<_>>();
    let seed_titles = seed_metadata
        .seeds
        .iter()
        .map(|seed| normalized_work_key(&seed.title))
        .collect::<HashSet<_>>();
    let mut candidates = evaluate_candidates(
        &candidate_plan,
        &by_id,
        &prepared,
        &accepted_seed_authors,
        &seed_titles,
    )?;
    select_candidates(&mut candidates, candidate_plan.selection_limit);

    let mut accepted = accepted_seed_documents(&raw_cache, &seed_metadata, &by_id, &prepared)?;
    accepted.extend(accepted_candidate_documents(&candidates)?);
    accepted.sort_by(|left, right| left.manifest.path.cmp(&right.manifest.path));
    let accepted_seed_count = seed_metadata
        .seeds
        .iter()
        .filter(|seed| is_publishable_status(&seed.redistribution_status))
        .count();
    validate_accepted(
        &accepted,
        accepted_seed_count,
        candidate_plan.selection_limit,
    )?;

    write_cleaned_texts(&corpus_root, &accepted)?;
    let analysis = build_real_analysis(&corpus_root.join("texts"), "accepted-corpus")?;
    attach_analysis_counts(&mut accepted, analysis.overview())?;
    let corpus_identity = corpus_checksum(
        accepted.iter().map(|document| {
            (
                document.manifest.path.as_str(),
                document.manifest.cleaned_sha256.as_str(),
            )
        }),
        &serde_json::to_vec(&TokenizationManifest::default())?,
    );
    let seed_analysis = build_seed_analysis(&raw_cache, &seed_metadata)?;

    write_committed_outputs(
        &repository_root,
        &corpus_root,
        &seed_metadata,
        &seed_audits,
        &candidate_plan,
        &candidates,
        &requested_ids,
        &by_id,
        &prepared,
        &accepted,
        &catalog_sha256,
        &corpus_identity,
        analysis.overview(),
    )?;
    write_dossier(
        &repository_root,
        &dossier_root,
        &seed_metadata,
        &seed_audits,
        &candidate_plan,
        &candidates,
        &accepted,
        &catalog_sha256,
        &corpus_identity,
        &analysis,
        &seed_analysis,
    )?;
    remove_direct_seed_sources(&repository_root, &seed_metadata)?;
    println!(
        "documentation corpus ready: {} documents, {} tokens, checksum {}",
        analysis.overview().document_count,
        analysis.overview().total_tokens,
        corpus_identity
    );
    println!("dossier payload: {}", dossier_root.display());
    Ok(())
}

fn repository_root() -> Result<PathBuf, BoxError> {
    let output = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()?;
    if !output.status.success() {
        return Err("not inside the CorpusWright Git repository".into());
    }
    Ok(PathBuf::from(
        String::from_utf8(output.stdout)?.trim().to_string(),
    ))
}

fn ensure_catalogue(path: &Path) -> Result<(), BoxError> {
    if path.metadata().is_ok_and(|metadata| metadata.len() > 0) {
        println!(
            "using cached Project Gutenberg catalogue: {}",
            path.display()
        );
        return Ok(());
    }
    let parent = path.parent().ok_or("catalogue path has no parent")?;
    fs::create_dir_all(parent)?;
    let headers = parent.join("pg_catalog.headers");
    let part = parent.join("pg_catalog.csv.part");
    run_curl(CATALOGUE_URL, &part, Some(&headers))?;
    ensure_nonempty(&part)?;
    fs::rename(part, path)?;
    println!("downloaded official catalogue once: {}", path.display());
    Ok(())
}

fn run_curl(url: &str, destination: &Path, headers: Option<&Path>) -> Result<(), BoxError> {
    let mut command = Command::new("curl");
    command.args([
        "--fail",
        "--location",
        "--retry",
        "3",
        "--silent",
        "--show-error",
        "--user-agent",
        USER_AGENT,
    ]);
    if let Some(headers) = headers {
        command.arg("--dump-header").arg(headers);
    }
    let status = command.arg("--output").arg(destination).arg(url).status()?;
    if !status.success() {
        return Err(format!("curl failed for {url} with {status}").into());
    }
    Ok(())
}

fn ensure_nonempty(path: &Path) -> Result<(), BoxError> {
    if path.metadata()?.len() == 0 {
        return Err(format!("retrieval returned an empty file: {}", path.display()).into());
    }
    Ok(())
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, BoxError> {
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}

fn read_candidate_plan(path: &Path) -> Result<CandidatePlan, BoxError> {
    let mut reader = csv::Reader::from_path(path)?;
    let mut candidates = Vec::new();
    for row in reader.deserialize::<SelectionPlanRow>() {
        let row = row?;
        if row.source_kind != "candidate" {
            continue;
        }
        let ebook_id = row.source_id.parse::<u64>()?;
        let required_score = |value: Option<u8>, component: &str| -> Result<u8, BoxError> {
            value.ok_or_else(|| {
                format!("selection row for ebook {ebook_id} lacks {component}").into()
            })
        };
        if row.canonical_filename.is_empty()
            || row.profile_rationale.is_empty()
            || row.balance_contribution.is_empty()
        {
            return Err(format!(
                "selection row for ebook {ebook_id} lacks durable editorial metadata"
            )
            .into());
        }
        candidates.push(CandidatePlanEntry {
            ebook_id,
            canonical_filename: row.canonical_filename,
            original_publication_year: row.original_publication_year,
            translator: row.translator.filter(|value| !value.is_empty()),
            profile_compatibility: required_score(
                row.profile_compatibility,
                "profile compatibility",
            )?,
            analytical_contribution: required_score(
                row.analytical_contribution,
                "analytical contribution",
            )?,
            genre_balance: required_score(row.genre_balance, "genre balance")?,
            profile_rationale: row.profile_rationale,
            balance_contribution: row.balance_contribution,
        });
    }
    Ok(CandidatePlan {
        schema_version: 1,
        selection_limit: SELECTION_LIMIT,
        profile: EDITORIAL_PROFILE.into(),
        candidates,
    })
}

fn validate_plans(seeds: &SeedMetadataFile, candidates: &CandidatePlan) -> Result<(), BoxError> {
    if seeds.schema_version != 1 || candidates.schema_version != 1 {
        return Err("unsupported documentation corpus plan schema".into());
    }
    if seeds.corpus_id != CORPUS_ID {
        return Err("seed plan corpus ID does not match the stable corpus ID".into());
    }
    if seeds.seeds.len() != 9 {
        return Err(format!("expected nine seed records, found {}", seeds.seeds.len()).into());
    }
    if candidates.selection_limit < 8 || candidates.selection_limit > 12 {
        return Err("candidate selection limit must remain within 8–12".into());
    }
    if candidates.candidates.len() < candidates.selection_limit * 2 - 1 {
        return Err(
            "candidate shortlist must contain approximately twice the selected count".into(),
        );
    }
    let mut ids = HashSet::new();
    for candidate in &candidates.candidates {
        if !ids.insert(candidate.ebook_id) {
            return Err(format!("duplicate candidate ebook ID {}", candidate.ebook_id).into());
        }
    }
    Ok(())
}

fn audit_seeds(
    repository_root: &Path,
    corpus_root: &Path,
    raw_cache: &Path,
    metadata: &SeedMetadataFile,
) -> Result<Vec<(SeedMetadata, SeedTechnicalAudit, u64, usize)>, BoxError> {
    let mut results = Vec::new();
    for seed in &metadata.seeds {
        let path = seed_input_path(repository_root, corpus_root, raw_cache, seed)?;
        let raw = fs::read(&path).map_err(|error| {
            io::Error::new(
                error.kind(),
                format!("failed to read seed {}: {error}", path.display()),
            )
        })?;
        let audit = technical_audit(&raw);
        if !audit.utf8_valid {
            return Err(format!("seed is not valid UTF-8: {}", seed.source_path).into());
        }
        cache_seed_bytes(raw_cache, seed, &raw)?;
        let (tokens, types) = canonical_counts(std::str::from_utf8(&raw)?);
        results.push((seed.clone(), audit, tokens, types));
    }
    Ok(results)
}

fn seed_cache_path(raw_cache: &Path, seed: &SeedMetadata) -> Result<PathBuf, BoxError> {
    let filename = Path::new(&seed.source_path)
        .file_name()
        .ok_or_else(|| format!("seed source has no filename: {}", seed.source_path))?;
    Ok(raw_cache.join("seeds").join(filename))
}

fn seed_input_path(
    repository_root: &Path,
    corpus_root: &Path,
    raw_cache: &Path,
    seed: &SeedMetadata,
) -> Result<PathBuf, BoxError> {
    let cached = seed_cache_path(raw_cache, seed)?;
    if cached.is_file() {
        return Ok(cached);
    }
    let direct = repository_root.join(&seed.source_path);
    if direct.is_file() {
        return Ok(direct);
    }
    let filename = Path::new(&seed.source_path)
        .file_name()
        .ok_or_else(|| format!("seed source has no filename: {}", seed.source_path))?;
    let in_texts = corpus_root.join("texts").join(filename);
    if in_texts.is_file() {
        return Ok(in_texts);
    }
    Err(format!("seed source is missing: {}", seed.source_path).into())
}

fn cache_seed_bytes(raw_cache: &Path, seed: &SeedMetadata, raw: &[u8]) -> Result<(), BoxError> {
    let path = seed_cache_path(raw_cache, seed)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    if path.is_file() && fs::read(&path)? != raw {
        return Err(format!("cached seed bytes changed: {}", seed.source_path).into());
    }
    fs::write(path, raw)?;
    Ok(())
}

fn remove_direct_seed_sources(
    repository_root: &Path,
    metadata: &SeedMetadataFile,
) -> Result<(), BoxError> {
    for seed in &metadata.seeds {
        let path = repository_root.join(&seed.source_path);
        if path.is_file() {
            fs::remove_file(path)?;
        }
    }
    Ok(())
}

fn requested_ebook_ids(seeds: &SeedMetadataFile, candidates: &CandidatePlan) -> BTreeSet<u64> {
    seeds
        .seeds
        .iter()
        .filter_map(|seed| seed.us_project_gutenberg_ebook_id)
        .chain(
            candidates
                .candidates
                .iter()
                .map(|candidate| candidate.ebook_id),
        )
        .collect()
}

fn retrieve_ebooks(
    raw_cache: &Path,
    ids: &BTreeSet<u64>,
    policy: RetrievalPolicy,
) -> Result<HashMap<u64, RetrievedEbook>, BoxError> {
    let ebook_cache = raw_cache.join("ebooks");
    fs::create_dir_all(&ebook_cache)?;
    let mut last_request = None::<Instant>;
    let mut retrieved = HashMap::new();
    for &ebook_id in ids {
        let url = format!("{MIRROR_BASE}/{ebook_id}/pg{ebook_id}.txt");
        let cache_path = ebook_cache.join(format!("pg{ebook_id}.txt"));
        let downloaded_this_run = !cache_path
            .metadata()
            .is_ok_and(|metadata| metadata.len() > 0);
        if downloaded_this_run {
            if let Some(previous) = last_request {
                let interval = Duration::from_millis(policy.minimum_request_interval_ms);
                if previous.elapsed() < interval {
                    thread::sleep(interval - previous.elapsed());
                }
            }
            last_request = Some(Instant::now());
            let part = ebook_cache.join(format!("pg{ebook_id}.txt.part"));
            let _ = fs::remove_file(&part);
            run_curl(&url, &part, None)?;
            ensure_nonempty(&part)?;
            fs::rename(&part, &cache_path)?;
        }
        let raw = fs::read(&cache_path)?;
        let raw_sha256 = sha256_hex(&raw);
        fs::write(
            ebook_cache.join(format!("pg{ebook_id}.sha256")),
            format!("{raw_sha256}  pg{ebook_id}.txt\n"),
        )?;
        println!(
            "{} Project Gutenberg ebook {} ({})",
            if downloaded_this_run {
                "retrieved"
            } else {
                "cached"
            },
            ebook_id,
            raw.len()
        );
        retrieved.insert(
            ebook_id,
            RetrievedEbook {
                url,
                raw,
                raw_sha256,
            },
        );
    }
    Ok(retrieved)
}

fn prepare_retrieved(
    retrieved: HashMap<u64, RetrievedEbook>,
) -> HashMap<u64, Result<PreparedEbook, String>> {
    retrieved
        .into_iter()
        .map(|(ebook_id, retrieval)| {
            let result = clean_gutenberg_text(&retrieval.raw)
                .map_err(|error| format!("{error:?}"))
                .map(|cleaned| {
                    let cleaned_sha256 = sha256_hex(cleaned.text.as_bytes());
                    let (tokens, types) = canonical_counts(&cleaned.text);
                    PreparedEbook {
                        retrieval,
                        cleaned,
                        cleaned_sha256,
                        tokens,
                        types,
                    }
                });
            (ebook_id, result)
        })
        .collect()
}

fn canonical_counts(text: &str) -> (u64, usize) {
    let config = TokenizationConfig::default();
    let tokens = tokenize(text, 0, &config)
        .into_iter()
        .filter(|token| token.counts_as_word())
        .collect::<Vec<_>>();
    let types = tokens
        .iter()
        .map(|token| token.aggregation_key(&config))
        .collect::<HashSet<_>>()
        .len();
    (tokens.len() as u64, types)
}

fn evaluate_candidates(
    plan: &CandidatePlan,
    catalog: &HashMap<u64, GutenbergCatalogRecord>,
    prepared: &HashMap<u64, Result<PreparedEbook, String>>,
    seed_authors: &HashSet<String>,
    seed_titles: &HashSet<String>,
) -> Result<Vec<EvaluatedCandidate>, BoxError> {
    let mut evaluated = Vec::new();
    for candidate in &plan.candidates {
        let catalog = catalog
            .get(&candidate.ebook_id)
            .ok_or_else(|| {
                format!(
                    "ebook {} is absent from the pinned catalogue",
                    candidate.ebook_id
                )
            })?
            .clone();
        let prepared_value = prepared.get(&candidate.ebook_id);
        let clean = prepared_value
            .and_then(|value| value.as_ref().ok())
            .cloned();
        let author = canonical_author(&candidate.canonical_filename)?;
        let input = CandidateScoringInput {
            estimated_tokens: clean.as_ref().map_or(0, |ebook| ebook.tokens),
            profile_compatibility: candidate.profile_compatibility,
            analytical_contribution: candidate.analytical_contribution,
            genre_balance: candidate.genre_balance,
            author_is_new: !seed_authors.contains(&author.to_lowercase()),
            work_is_duplicate: seed_titles.contains(&normalized_work_key(&catalog.title)),
            plain_text_available: clean.is_some(),
        };
        evaluated.push(EvaluatedCandidate {
            evaluation: score_candidate(&catalog, &input),
            plan: candidate.clone(),
            catalog,
            prepared: clean,
            author,
            selected: false,
            selection_reason: prepared_value
                .and_then(|value| value.as_ref().err())
                .map_or_else(String::new, |error| format!("Cleaning failed: {error}")),
        });
    }
    evaluated.sort_by(|left, right| {
        right
            .evaluation
            .score
            .total
            .cmp(&left.evaluation.score.total)
            .then_with(|| left.plan.ebook_id.cmp(&right.plan.ebook_id))
    });
    Ok(evaluated)
}

fn select_candidates(candidates: &mut [EvaluatedCandidate], limit: usize) {
    let mut selected_authors = HashSet::<String>::new();
    let mut selected = 0usize;
    for candidate in candidates {
        if candidate.prepared.is_none() {
            if candidate.selection_reason.is_empty() {
                candidate.selection_reason =
                    "Plain-text retrieval or deterministic cleaning failed.".into();
            }
            continue;
        }
        let author_key = candidate.author.to_lowercase();
        if selected >= limit {
            candidate.selection_reason =
                "Below the deterministic selection limit after scoring.".into();
            continue;
        }
        if !selected_authors.insert(author_key) {
            candidate.selection_reason =
                "Rejected to preserve one newly added work per author.".into();
            continue;
        }
        candidate.selected = true;
        candidate.selection_reason = format!(
            "Selected at score {} for profile, author diversity, length, analytical contribution, genre balance, and provenance.",
            candidate.evaluation.score.total
        );
        selected += 1;
    }
}

fn canonical_author(filename: &str) -> Result<String, BoxError> {
    let stem = filename.strip_suffix(".txt").unwrap_or(filename);
    stem.rsplit_once(" - ")
        .map(|(_, author)| author.to_string())
        .ok_or_else(|| format!("canonical filename lacks author separator: {filename}").into())
}

fn normalized_work_key(title: &str) -> String {
    title
        .chars()
        .filter(|character| character.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn accepted_seed_documents(
    raw_cache: &Path,
    seeds: &SeedMetadataFile,
    catalog: &HashMap<u64, GutenbergCatalogRecord>,
    prepared: &HashMap<u64, Result<PreparedEbook, String>>,
) -> Result<Vec<AcceptedDocument>, BoxError> {
    let mut accepted = Vec::new();
    for seed in &seeds.seeds {
        if !is_publishable_status(&seed.redistribution_status) {
            continue;
        }
        if seed.probable_source == "Project Gutenberg Australia" {
            let raw = fs::read(seed_cache_path(raw_cache, seed)?)?;
            accepted.push(accepted_australia_seed_document(seed, &raw)?);
            continue;
        }
        let ebook_id = seed
            .us_project_gutenberg_ebook_id
            .ok_or("accepted seed lacks a United States Project Gutenberg ID")?;
        let prepared = prepared
            .get(&ebook_id)
            .ok_or_else(|| format!("seed refresh {ebook_id} was not retrieved"))?
            .as_ref()
            .map_err(|error| format!("seed refresh {ebook_id} could not be cleaned: {error}"))?
            .clone();
        let catalog = catalog
            .get(&ebook_id)
            .ok_or_else(|| format!("seed refresh {ebook_id} is absent from the catalogue"))?;
        let filename = format!("{} - {}.txt", seed.title, seed.author);
        accepted.push(accepted_document(
            filename,
            seed.title.clone(),
            seed.author.clone(),
            seed.translator.clone(),
            seed.original_publication_year,
            seed.edition_publication_year,
            "ACCEPTED WITH NOTE",
            vec![format!(
                "The immutable seed lacked a validated start marker; the canonical copy is regenerated from current U.S. Project Gutenberg ebook {ebook_id}."
            )],
            catalog,
            prepared,
            "projectGutenbergSeedRefresh",
        )?);
    }
    Ok(accepted)
}

fn accepted_australia_seed_document(
    seed: &SeedMetadata,
    raw: &[u8],
) -> Result<AcceptedDocument, BoxError> {
    let raw_sha256 = sha256_hex(raw);
    let decoded = std::str::from_utf8(raw)?;
    let normalized = decoded.replace("\r\n", "\n").replace('\r', "\n");
    let marker = "Project Gutenberg Australia";
    let marker_lines = normalized
        .match_indices(marker)
        .filter(|(index, _)| {
            let line_start = normalized[..*index]
                .rfind('\n')
                .map_or(0, |value| value + 1);
            let line_end = normalized[*index..]
                .find('\n')
                .map_or(normalized.len(), |value| *index + value);
            normalized[line_start..line_end].trim() == marker
        })
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    if marker_lines.len() != 1 {
        return Err(format!(
            "{} has {} exact Project Gutenberg Australia footer markers; expected one",
            seed.title,
            marker_lines.len()
        )
        .into());
    }
    let marker_index = marker_lines[0];
    if marker_index < normalized.len() * 9 / 10 {
        return Err(format!("{} footer marker occurs before the final 10%", seed.title).into());
    }
    let cleaned = format!("{}\n", normalized[..marker_index].trim_end());
    if cleaned.contains(marker) {
        return Err(format!(
            "{} retains Project Gutenberg Australia boilerplate",
            seed.title
        )
        .into());
    }
    let cleaned_sha256 = sha256_hex(cleaned.as_bytes());
    let (tokens, types) = canonical_counts(&cleaned);
    let filename = if seed.title == "Nineteen Eighty-Four" {
        "1984 - George Orwell.txt".to_string()
    } else {
        format!("{} - {}.txt", seed.title, seed.author)
    };
    let path = format!("examples/corpora/documentation-books/texts/{filename}");
    let warning = "Public-domain qualification is jurisdiction-specific to Brazil; no United States public-domain claim is made.";
    Ok(AcceptedDocument {
        manifest: DocumentManifest {
            path,
            filename,
            title: seed.title.clone(),
            author: seed.author.clone(),
            translator: seed.translator.clone(),
            original_publication_year: seed.original_publication_year,
            edition_publication_year: seed.edition_publication_year,
            source: "Project Gutenberg Australia".into(),
            source_id: format!("local-seed:{raw_sha256}"),
            source_landing_page: "https://gutenberg.net.au/".into(),
            public_domain_basis: format!(
                "George Orwell died in 1950. Brazil Law 9,610/1998 article 41 provides a life-plus-70-year economic-rights term measured from 1 January after death; the maintainer states this corpus is distributed in Brazil. Official guidance: {BRAZIL_COPYRIGHT_SOURCE}"
            ),
            redistribution_status: seed.redistribution_status.clone(),
            raw_sha256: raw_sha256.clone(),
            cleaned_sha256: cleaned_sha256.clone(),
            bytes: cleaned.len() as u64,
            tokens,
            types,
            qualification_status: seed.redistribution_status.clone(),
            qualification_warnings: vec![warning.into()],
        },
        text: cleaned.clone(),
        cleaning_report: json!({
            "sourceKind": "projectGutenbergAustraliaSeed",
            "sourcePathOrEbookId": seed.source_path,
            "rawSha256": raw_sha256,
            "rawBytes": raw.len(),
            "startMarker": "body begins at byte 0; no leading boilerplate was present",
            "endMarker": marker,
            "removedPrefixBytes": 0,
            "removedSuffixBytes": normalized.len() - cleaned.len(),
            "cleanedSha256": cleaned_sha256,
            "cleanedBytes": cleaned.len(),
            "cleanedTokenCount": tokens,
            "warnings": [warning],
        }),
    })
}

fn accepted_candidate_documents(
    candidates: &[EvaluatedCandidate],
) -> Result<Vec<AcceptedDocument>, BoxError> {
    candidates
        .iter()
        .filter(|candidate| candidate.selected)
        .map(|candidate| {
            accepted_document(
                candidate.plan.canonical_filename.clone(),
                canonical_title(&candidate.plan.canonical_filename)?,
                candidate.author.clone(),
                candidate.plan.translator.clone(),
                candidate.plan.original_publication_year,
                None,
                "ACCEPTED",
                Vec::new(),
                &candidate.catalog,
                candidate
                    .prepared
                    .clone()
                    .ok_or("selected candidate has no prepared text")?,
                "projectGutenbergCandidate",
            )
        })
        .collect()
}

fn canonical_title(filename: &str) -> Result<String, BoxError> {
    let stem = filename.strip_suffix(".txt").unwrap_or(filename);
    stem.rsplit_once(" - ")
        .map(|(title, _)| title.to_string())
        .ok_or_else(|| format!("canonical filename lacks title separator: {filename}").into())
}

#[allow(clippy::too_many_arguments)]
fn accepted_document(
    filename: String,
    title: String,
    author: String,
    translator: Option<String>,
    original_publication_year: Option<u16>,
    edition_publication_year: Option<u16>,
    status: &str,
    warnings: Vec<String>,
    catalog: &GutenbergCatalogRecord,
    prepared: PreparedEbook,
    source_kind: &str,
) -> Result<AcceptedDocument, BoxError> {
    let ebook_id = catalog.ebook_id;
    let relative_path = format!("examples/corpora/documentation-books/texts/{filename}");
    let cleaning_report = gutenberg_cleaning_report(ebook_id, &prepared, source_kind);
    Ok(AcceptedDocument {
        manifest: DocumentManifest {
            path: relative_path,
            filename,
            title,
            author,
            translator,
            original_publication_year,
            edition_publication_year,
            source: "Project Gutenberg".into(),
            source_id: ebook_id.to_string(),
            source_landing_page: format!("https://www.gutenberg.org/ebooks/{ebook_id}"),
            public_domain_basis: format!(
                "Present in the pinned current United States Project Gutenberg catalogue as ebook {ebook_id}; accepted under the repository safety policy."
            ),
            redistribution_status: status.into(),
            raw_sha256: prepared.retrieval.raw_sha256,
            cleaned_sha256: prepared.cleaned_sha256,
            bytes: prepared.cleaned.text.len() as u64,
            tokens: prepared.tokens,
            types: prepared.types,
            qualification_status: status.into(),
            qualification_warnings: warnings,
        },
        text: prepared.cleaned.text,
        cleaning_report,
    })
}

fn gutenberg_cleaning_report(ebook_id: u64, prepared: &PreparedEbook, source_kind: &str) -> Value {
    json!({
        "sourceKind": source_kind,
        "sourcePathOrEbookId": ebook_id,
        "rawSha256": prepared.retrieval.raw_sha256,
        "rawBytes": prepared.retrieval.raw.len(),
        "startMarker": prepared.cleaned.report.start_marker.text,
        "endMarker": prepared.cleaned.report.end_marker.text,
        "removedPrefixBytes": prepared.cleaned.report.removed_prefix_bytes,
        "removedSuffixBytes": prepared.cleaned.report.removed_suffix_bytes,
        "cleanedSha256": prepared.cleaned_sha256,
        "cleanedBytes": prepared.cleaned.text.len(),
        "cleanedTokenCount": prepared.tokens,
        "warnings": prepared.cleaned.report.warnings,
    })
}

fn project_gutenberg_retrieval_json(
    catalog: &GutenbergCatalogRecord,
    prepared: &PreparedEbook,
) -> Value {
    let ebook_id = catalog.ebook_id;
    json!({
        "ebookId": ebook_id,
        "catalogueTitle": catalog.title,
        "catalogueAuthors": catalog.authors,
        "catalogueIssued": catalog.issued,
        "catalogueLanguage": catalog.language,
        "catalogueSubjects": catalog.subjects,
        "catalogueBookshelves": catalog.bookshelves,
        "landingPage": format!("https://www.gutenberg.org/ebooks/{ebook_id}"),
        "retrievalEndpoint": prepared.retrieval.url,
        "rawSha256": prepared.retrieval.raw_sha256,
        "rawBytes": prepared.retrieval.raw.len(),
        "cleanedSha256": prepared.cleaned_sha256,
        "cleanedBytes": prepared.cleaned.text.len(),
    })
}

fn validate_accepted(
    accepted: &[AcceptedDocument],
    accepted_seed_count: usize,
    selected_count: usize,
) -> Result<(), BoxError> {
    if accepted.len() != accepted_seed_count + selected_count {
        return Err(format!(
            "expected {} accepted documents ({} seeds plus candidates), found {}",
            accepted_seed_count + selected_count,
            accepted_seed_count,
            accepted.len()
        )
        .into());
    }
    let filenames = accepted
        .iter()
        .map(|document| document.manifest.filename.as_str())
        .collect::<HashSet<_>>();
    if filenames.len() != accepted.len() {
        return Err("canonical filename collision detected".into());
    }
    if !manifest_paths_are_strictly_ordered(
        accepted
            .iter()
            .map(|document| document.manifest.path.as_str()),
    ) {
        return Err("accepted manifest paths are not strictly ordered".into());
    }
    let duplicates = detect_duplicate_content(
        accepted
            .iter()
            .map(|document| (document.manifest.path.as_str(), document.text.as_bytes())),
    );
    if !duplicates.is_empty() {
        return Err(format!("duplicate accepted content detected: {duplicates:?}").into());
    }
    let author_counts = accepted
        .iter()
        .fold(HashMap::new(), |mut counts, document| {
            *counts
                .entry(document.manifest.author.to_lowercase())
                .or_insert(0usize) += 1;
            counts
        });
    if author_counts.values().any(|count| *count > 3) {
        return Err("accepted corpus exceeds the three-document author cap".into());
    }
    Ok(())
}

fn write_cleaned_texts(corpus_root: &Path, accepted: &[AcceptedDocument]) -> Result<(), BoxError> {
    let texts = corpus_root.join("texts");
    if texts.exists() {
        fs::remove_dir_all(&texts)?;
    }
    fs::create_dir_all(&texts)?;
    for document in accepted {
        fs::write(texts.join(&document.manifest.filename), &document.text)?;
    }
    Ok(())
}

fn build_real_analysis(
    texts: &Path,
    build_name: &str,
) -> Result<corpuswright_core::analysis::AnalysisCorpus, BoxError> {
    let report = scan_directory(texts).map_err(|error| format!("scan failed: {error:?}"))?;
    build_analysis(&report.files, build_name)
}

fn build_seed_analysis(
    raw_cache: &Path,
    seeds: &SeedMetadataFile,
) -> Result<corpuswright_core::analysis::AnalysisCorpus, BoxError> {
    let paths = seeds
        .seeds
        .iter()
        .map(|seed| seed_cache_path(raw_cache, seed))
        .collect::<Result<Vec<_>, _>>()?;
    let report = load_files(paths).map_err(|error| format!("seed scan failed: {error:?}"))?;
    build_analysis(&report.files, "original-nine-seeds")
}

fn build_analysis(
    records: &[DocumentRecord],
    build_name: &str,
) -> Result<corpuswright_core::analysis::AnalysisCorpus, BoxError> {
    let cleaning = CleaningConfig::default();
    let language_overrides = (0..records.len())
        .map(|index| (index, "en".to_string()))
        .collect();
    Ok(build_analysis_corpus(
        records,
        AnalysisBuildOptions {
            built_at: SNAPSHOT_RETRIEVED_AT.into(),
            build_signature: build_name.into(),
            processing_config_signature: cleaning_config_signature(&cleaning),
            overview_context: CorpusOverviewContext {
                sources_in_project: Some(records.len()),
                sources_with_warnings: Some(0),
                failed_sources: 0,
                config_version: Some("documentation-corpus-v1".into()),
                metadata_attached: true,
            },
            udpipe_model_dir: None,
            force_compute_hashes: false,
            language_overrides,
            tokenization: TokenizationConfig::default(),
            annotation_cache_root: None,
        },
        &AtomicBool::new(false),
        |record| {
            let text =
                fs::read_to_string(&record.source_path).map_err(|error| error.to_string())?;
            Ok(AnalysisInputResolution {
                text: Some(text),
                provenance: AnalysisInputProvenance::Processed,
                source_id: record.source_path.to_string_lossy().into_owned(),
                source_name: record.relative_path.to_string_lossy().into_owned(),
                status: AnalysisInputStatus::Ready,
            })
        },
        |_| {},
    )?)
}

fn attach_analysis_counts(
    accepted: &mut [AcceptedDocument],
    overview: &CorpusOverview,
) -> Result<(), BoxError> {
    let counts = overview
        .documents
        .iter()
        .map(|document| (document.name.as_str(), document.tokens))
        .collect::<HashMap<_, _>>();
    for document in accepted {
        let count = counts
            .get(document.manifest.filename.as_str())
            .copied()
            .ok_or_else(|| format!("Overview omitted {}", document.manifest.filename))?;
        if count != document.manifest.tokens {
            return Err(format!(
                "token-count mismatch for {}: direct {} vs Overview {}",
                document.manifest.filename, document.manifest.tokens, count
            )
            .into());
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn write_committed_outputs(
    _repository_root: &Path,
    corpus_root: &Path,
    seed_metadata: &SeedMetadataFile,
    seed_audits: &[(SeedMetadata, SeedTechnicalAudit, u64, usize)],
    candidate_plan: &CandidatePlan,
    candidates: &[EvaluatedCandidate],
    requested_ids: &BTreeSet<u64>,
    catalog: &HashMap<u64, GutenbergCatalogRecord>,
    prepared: &HashMap<u64, Result<PreparedEbook, String>>,
    accepted: &[AcceptedDocument],
    catalog_sha256: &str,
    corpus_identity: &str,
    overview: &CorpusOverview,
) -> Result<(), BoxError> {
    let seed_manifest = seed_manifest_json(seed_metadata, seed_audits, catalog_sha256);
    write_json(&corpus_root.join("seed-manifest.json"), &seed_manifest)?;
    let retrievals = requested_ids
        .iter()
        .map(|ebook_id| {
            let catalog = catalog
                .get(ebook_id)
                .ok_or_else(|| format!("retrieved ebook {ebook_id} is absent from catalogue"))?;
            let prepared = prepared
                .get(ebook_id)
                .ok_or_else(|| format!("retrieved ebook {ebook_id} was not prepared"))?
                .as_ref()
                .map_err(|error| format!("retrieved ebook {ebook_id} failed cleaning: {error}"))?;
            Ok(project_gutenberg_retrieval_json(catalog, prepared))
        })
        .collect::<Result<Vec<_>, BoxError>>()?;
    let jurisdiction_specific_sources = accepted
        .iter()
        .filter(|document| document.manifest.source == "Project Gutenberg Australia")
        .map(|document| {
            json!({
                "sourceSystem": document.manifest.source,
                "sourceId": document.manifest.source_id,
                "title": document.manifest.title,
                "author": document.manifest.author,
                "landingPage": document.manifest.source_landing_page,
                "rawSha256": document.manifest.raw_sha256,
                "cleanedSha256": document.manifest.cleaned_sha256,
                "publicDomainJurisdiction": "Brazil",
                "publicDomainGuidance": BRAZIL_COPYRIGHT_SOURCE,
            })
        })
        .collect::<Vec<_>>();
    write_json(
        &corpus_root.join("project-gutenberg-manifest.json"),
        &json!({
            "schemaVersion": 1,
            "catalogue": catalogue_identity(catalog_sha256),
            "approvedRetrievalEndpoint": MIRROR_BASE,
            "userAgent": USER_AGENT,
            "minimumRequestIntervalMs": 2000,
            "ebooks": retrievals,
            "jurisdictionSpecificSources": jurisdiction_specific_sources,
        }),
    )?;
    let mut cleaning_reports = requested_ids
        .iter()
        .map(|ebook_id| {
            let prepared = prepared
                .get(ebook_id)
                .ok_or_else(|| format!("retrieved ebook {ebook_id} was not prepared"))?
                .as_ref()
                .map_err(|error| format!("retrieved ebook {ebook_id} failed cleaning: {error}"))?;
            Ok(gutenberg_cleaning_report(
                *ebook_id,
                prepared,
                "projectGutenbergRetrieved",
            ))
        })
        .collect::<Result<Vec<_>, BoxError>>()?;
    cleaning_reports.extend(
        accepted
            .iter()
            .filter(|document| document.manifest.source == "Project Gutenberg Australia")
            .map(|document| document.cleaning_report.clone()),
    );
    write_json(
        &corpus_root.join("cleaning-report.json"),
        &json!({ "schemaVersion": 1, "files": cleaning_reports }),
    )?;
    let documents = accepted
        .iter()
        .map(|document| document.manifest.clone())
        .collect::<Vec<_>>();
    let total_bytes = documents.iter().map(|document| document.bytes).sum::<u64>();
    write_json(
        &corpus_root.join("corpus.json"),
        &json!({
            "schemaVersion": 1,
            "corpusId": CORPUS_ID,
            "title": CORPUS_TITLE,
            "purpose": [
                "real analytical figures for the Concepts page",
                "reproducible documentation corpus",
                "manually loadable CorpusWright demonstration corpus",
                "naturalistic acceptance corpus"
            ],
            "language": "en",
            "documentUnit": "one accepted literary work or coherent single-author collection per UTF-8 text file",
            "tokenisation": TokenizationManifest::default(),
            "caseHandling": "surface tokens are retained; aggregation keys use NFC plus Unicode lowercase",
            "sourceDirectory": "examples/corpora/documentation-books/texts",
            "documentCount": overview.document_count,
            "totalBytes": total_bytes,
            "totalTokens": overview.total_tokens,
            "totalTypes": overview.total_types,
            "corpusChecksum": corpus_identity,
            "selectionPolicy": candidate_plan.profile,
            "limitations": [
                "Deliberately curated for transparent pedagogy; not representative of English.",
                "Not balanced by genre, period, author population, or inferential sampling design.",
                "Project Gutenberg editions can differ from print editions and are pinned here by raw and cleaned SHA-256."
            ],
            "documents": documents,
        }),
    )?;
    let checksum_map = accepted
        .iter()
        .map(|document| {
            (
                document.manifest.path.clone(),
                json!({
                    "sha256": document.manifest.cleaned_sha256,
                    "bytes": document.manifest.bytes,
                }),
            )
        })
        .collect::<BTreeMap<_, _>>();
    write_json(
        &corpus_root.join("checksums.json"),
        &json!({
            "schemaVersion": 1,
            "algorithm": "SHA-256",
            "corpusChecksum": corpus_identity,
            "analysisConfiguration": TokenizationManifest::default(),
            "documents": checksum_map,
        }),
    )?;
    fs::write(
        corpus_root.join("selection.csv"),
        selection_csv(seed_metadata, candidates, accepted)?,
    )?;
    fs::write(
        corpus_root.join("README.md"),
        corpus_readme(candidate_plan, accepted, overview, corpus_identity),
    )?;
    Ok(())
}

fn seed_manifest_json(
    seed_metadata: &SeedMetadataFile,
    audits: &[(SeedMetadata, SeedTechnicalAudit, u64, usize)],
    catalog_sha256: &str,
) -> Value {
    let records = audits
        .iter()
        .map(|(metadata, audit, tokens, types)| {
            let cache_filename = Path::new(&metadata.source_path)
                .file_name()
                .map(|value| value.to_string_lossy().into_owned())
                .unwrap_or_else(|| metadata.source_path.clone());
            json!({
                "path": metadata.source_path,
                "preservedOriginal": {
                    "status": "BYTE-IDENTICAL EXTERNAL CACHE COPY",
                    "cacheRelativePath": format!("seeds/{cache_filename}"),
                    "sha256": audit.sha256,
                },
                "title": metadata.title,
                "author": metadata.author,
                "translator": metadata.translator,
                "originalPublicationYear": metadata.original_publication_year,
                "editionPublicationYear": metadata.edition_publication_year,
                "probableSource": metadata.probable_source,
                "usProjectGutenbergEbookId": metadata.us_project_gutenberg_ebook_id,
                "bytes": audit.bytes,
                "sha256": audit.sha256,
                "utf8Valid": audit.utf8_valid,
                "bom": audit.bom,
                "nulBytes": audit.nul_bytes,
                "crlfCount": audit.crlf_count,
                "loneCrCount": audit.lone_cr_count,
                "replacementCharacterCount": audit.replacement_character_count,
                "pageNumberLineCount": audit.page_number_line_count,
                "repeatedLongLineGroups": audit.repeated_long_line_groups,
                "startMarkerCount": audit.start_marker_count,
                "endMarkerCount": audit.end_marker_count,
                "canonicalRawTokenCount": tokens,
                "canonicalRawTypeCount": types,
                "redistributionStatus": metadata.redistribution_status,
                "plannedDisposition": metadata.planned_disposition,
                "qualificationWarnings": metadata.qualification_warnings,
            })
        })
        .collect::<Vec<_>>();
    json!({
        "schemaVersion": 1,
        "corpusId": CORPUS_ID,
        "policy": seed_metadata.policy,
        "catalogueEvidence": catalogue_identity(catalog_sha256),
        "seeds": records,
    })
}

fn catalogue_identity(sha256: &str) -> Value {
    json!({
        "url": CATALOGUE_URL,
        "lastModified": "2026-07-12T21:34:24Z",
        "retrievedAt": SNAPSHOT_RETRIEVED_AT,
        "sha256": sha256,
    })
}

fn write_json(path: &Path, value: &impl Serialize) -> Result<(), BoxError> {
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    fs::write(path, bytes)?;
    Ok(())
}

fn csv_escape(value: impl AsRef<str>) -> String {
    let value = value.as_ref();
    if value.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

fn selection_csv(
    seeds: &SeedMetadataFile,
    candidates: &[EvaluatedCandidate],
    accepted: &[AcceptedDocument],
) -> Result<String, BoxError> {
    let mut output = String::from(
        "sourceKind,sourceId,title,author,tokens,status,reason,canonicalPath,canonicalFilename,originalPublicationYear,translator,profileCompatibility,analyticalContribution,genreBalance,profileRationale,balanceContribution\n",
    );
    let accepted_by_work = accepted
        .iter()
        .map(|document| {
            (
                (
                    document.manifest.title.as_str(),
                    document.manifest.author.as_str(),
                ),
                &document.manifest,
            )
        })
        .collect::<HashMap<_, _>>();
    for seed in &seeds.seeds {
        let source_id = seed
            .us_project_gutenberg_ebook_id
            .map_or_else(|| "local-seed".into(), |id| id.to_string());
        let accepted_doc = accepted_by_work.get(&(seed.title.as_str(), seed.author.as_str()));
        let row = [
            "seed".to_string(),
            source_id,
            seed.title.clone(),
            seed.author.clone(),
            accepted_doc.map_or_else(String::new, |document| document.tokens.to_string()),
            seed.redistribution_status.clone(),
            seed.planned_disposition.clone(),
            accepted_doc.map_or_else(String::new, |document| document.path.clone()),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
        ];
        output.push_str(&row.map(csv_escape).join(","));
        output.push('\n');
    }
    for candidate in candidates {
        let prepared_tokens = candidate
            .prepared
            .as_ref()
            .map_or_else(String::new, |ebook| ebook.tokens.to_string());
        let row = [
            "candidate".to_string(),
            candidate.plan.ebook_id.to_string(),
            candidate.catalog.title.clone(),
            candidate.author.clone(),
            prepared_tokens,
            if candidate.selected {
                "ACCEPTED".into()
            } else {
                "REJECTED BY SELECTION POLICY".into()
            },
            candidate.selection_reason.clone(),
            if candidate.selected {
                format!(
                    "examples/corpora/documentation-books/texts/{}",
                    candidate.plan.canonical_filename
                )
            } else {
                String::new()
            },
            candidate.plan.canonical_filename.clone(),
            candidate
                .plan
                .original_publication_year
                .map_or_else(String::new, |year| year.to_string()),
            candidate.plan.translator.clone().unwrap_or_default(),
            candidate.plan.profile_compatibility.to_string(),
            candidate.plan.analytical_contribution.to_string(),
            candidate.plan.genre_balance.to_string(),
            candidate.plan.profile_rationale.clone(),
            candidate.plan.balance_contribution.clone(),
        ];
        output.push_str(&row.map(csv_escape).join(","));
        output.push('\n');
    }
    Ok(output)
}

fn corpus_readme(
    candidate_plan: &CandidatePlan,
    accepted: &[AcceptedDocument],
    overview: &CorpusOverview,
    corpus_identity: &str,
) -> String {
    format!(
        "# {CORPUS_TITLE}\n\n\
Stable identifier: `{CORPUS_ID}`\n\n\
This is a deliberately curated English literary corpus for transparent CorpusWright documentation examples, a manually loadable demonstration, and naturalistic acceptance testing. It is not representative of English, genre-balanced, historically balanced, author-population balanced, or suitable for inferential comparison.\n\n\
## Editorial profile\n\n\
{}\n\n\
The retained seed profile covers psychologically, socially, politically, and aesthetically serious adult literature. `1984` and `Animal Farm` are included under Brazil's life-plus-70-year public-domain rule, based on George Orwell's death in 1950 and the maintainer's statement that this corpus is distributed in Brazil; no United States public-domain claim is made for those files. `Lady Windermere's Fan` is excluded to cap Oscar Wilde at three documents while retaining drama through Shaw's `Pygmalion`.\n\n\
## Reproduction\n\n\
From the repository root, run:\n\n\
```bash\n\
cargo run -p corpuswright-core --example documentation_corpus\n\
node apps/desktop/scripts/capture-documentation-corpus-dossier.mjs\n\
```\n\n\
The Rust command caches the official Project Gutenberg catalogue and shortlisted ebook downloads under `/tmp/corpuswright-documentation-books-raw/`. It uses the approved high-speed Project Gutenberg mirror, a descriptive User-Agent, a minimum two-second interval between ebook requests, raw SHA-256 checksums, strict validated start/end markers, LF line endings, and exactly one final newline. Missing or ambiguous markers are refused. The second command renders the temporary dossier with CorpusWright's actual frontend figure geometry, layout, theme tokens, renderer, and standalone SVG export path.\n\n\
Raw cache files and dossier images are intentionally not committed. Public source references use permanent landing pages; exact retrieved bytes are pinned by SHA-256 in the manifests.\n\n\
## Corpus identity\n\n\
- Repository baseline audited for this generation: `{}`\n\
- Documents: {}\n\
- CorpusWright tokens: {}\n\
- CorpusWright types: {}\n\
- Corpus checksum: `{}`\n\
- Canonical tokenizer: `corpuswright-uax29` version 1, default configuration\n\
- Document unit: one accepted book or coherent single-author collection per file\n\n\
## Files\n\n\
- `corpus.json`: corpus and document metadata\n\
- `checksums.json`: ordered cleaned-file checksums and corpus identity\n\
- `selection.csv`: seed dispositions, complete shortlist, editorial score inputs, selections, and rejections\n\
- `cleaning-report.json`: marker evidence and removed byte counts\n\
- `seed-manifest.json`: immutable original-seed audit\n\
- `project-gutenberg-manifest.json`: catalogue, retrieval, and raw-byte evidence\n\
- `texts/`: accepted cleaned UTF-8 documents only\n\n\
Accepted cleaned bytes total {}. Byte-identical seed originals are retained in the external raw cache; the corpus directory contains no source subdirectory and no direct-child text files.\n",
        candidate_plan.profile,
        BASELINE_COMMIT,
        overview.document_count,
        overview.total_tokens,
        overview.total_types,
        corpus_identity,
        accepted
            .iter()
            .map(|document| document.manifest.bytes)
            .sum::<u64>(),
    )
}

fn git_output<const N: usize>(root: &Path, args: [&str; N]) -> Result<String, BoxError> {
    let output = Command::new("git").args(args).current_dir(root).output()?;
    if !output.status.success() {
        return Err(format!("git command failed: {:?}", args).into());
    }
    Ok(String::from_utf8(output.stdout)?.trim().to_string())
}

#[allow(clippy::too_many_arguments)]
fn write_dossier(
    repository_root: &Path,
    dossier_root: &Path,
    seed_metadata: &SeedMetadataFile,
    seed_audits: &[(SeedMetadata, SeedTechnicalAudit, u64, usize)],
    candidate_plan: &CandidatePlan,
    candidates: &[EvaluatedCandidate],
    accepted: &[AcceptedDocument],
    catalog_sha256: &str,
    corpus_identity: &str,
    analysis: &corpuswright_core::analysis::AnalysisCorpus,
    seed_analysis: &corpuswright_core::analysis::AnalysisCorpus,
) -> Result<(), BoxError> {
    if dossier_root.exists() {
        fs::remove_dir_all(dossier_root)?;
    }
    for directory in [
        dossier_root.to_path_buf(),
        dossier_root.join("figures/light"),
        dossier_root.join("figures/dark"),
        dossier_root.join("figures/png"),
        dossier_root.join("logs"),
    ] {
        fs::create_dir_all(directory)?;
    }
    let document_distribution = query_overview_distribution(
        analysis,
        &OverviewDistributionQuery {
            metric: OverviewDistributionMetric::DocumentLength,
            histogram_rule: OverviewHistogramRule::Auto,
            manual_bin_count: 12,
            x_scale: OverviewDistributionScale::Log,
            qq_reference: OverviewQqReference::Lognormal,
            maximum_returned_point_count: 500,
        },
    )
    .map_err(|error| format!("document-length distribution failed: {error:?}"))?;
    let mattr_distribution = query_overview_distribution(
        analysis,
        &OverviewDistributionQuery {
            metric: OverviewDistributionMetric::Mattr,
            histogram_rule: OverviewHistogramRule::Auto,
            manual_bin_count: 12,
            x_scale: OverviewDistributionScale::Linear,
            qq_reference: OverviewQqReference::Normal,
            maximum_returned_point_count: 500,
        },
    )
    .map_err(|error| format!("MATTR distribution failed: {error:?}"))?;
    write_json(
        &dossier_root.join("dossier.json"),
        &AnalysisPayload {
            schema_version: 1,
            corpus_id: CORPUS_ID,
            corpus_checksum: corpus_identity,
            overview: analysis.overview(),
            document_distribution: serde_json::to_value(document_distribution)?,
            mattr_distribution: serde_json::to_value(mattr_distribution)?,
        },
    )?;
    let examples = candidate_examples(analysis)?;
    write_json(&dossier_root.join("candidate-examples.json"), &examples)?;
    fs::write(
        dossier_root.join("seed-audit.csv"),
        seed_audit_csv(seed_audits),
    )?;
    fs::write(
        dossier_root.join("candidate-shortlist.csv"),
        candidate_shortlist_csv(candidates),
    )?;
    fs::write(
        dossier_root.join("corpus-balance.csv"),
        balance_csv(accepted, analysis.overview()),
    )?;
    fs::write(
        dossier_root.join("selection-report.md"),
        selection_report(candidate_plan, candidates),
    )?;
    fs::write(
        dossier_root.join("concept-figure-map.md"),
        concept_figure_map(),
    )?;
    fs::write(dossier_root.join("toy-requirements.md"), toy_requirements())?;
    fs::write(
        dossier_root.join("dossier.md"),
        dossier_markdown(
            repository_root,
            seed_metadata,
            candidates,
            accepted,
            catalog_sha256,
            corpus_identity,
            analysis.overview(),
            seed_analysis.overview(),
        ),
    )?;
    fs::write(dossier_root.join("index.html"), dossier_index_html())?;
    Ok(())
}

fn candidate_examples(
    corpus: &corpuswright_core::analysis::AnalysisCorpus,
) -> Result<Value, BoxError> {
    let base = |hide_stopwords, token_class| FrequencyQuery {
        selection: AnalysisDocumentSelection::default(),
        query: String::new(),
        match_mode: FrequencyMatchMode::Contains,
        minimum_frequency: 20,
        token_class,
        hide_stopwords,
        group_by: FrequencyGroupMode::Surface,
        case_fold: true,
        language: Some("en".into()),
        pos_filter: None,
        sort_key: FrequencySortKey::Frequency,
        sort_direction: SortDirection::Desc,
        offset: 0,
        limit: 500,
    };
    let all = query_frequency_list(corpus, &base(false, FrequencyTokenClass::Alphabetic));
    let content = query_frequency_list(corpus, &base(true, FrequencyTokenClass::Alphabetic));
    let pair = best_dispersion_pair(&all.items);
    let content_pair = best_dispersion_pair(&content.items);
    let widespread = content
        .items
        .iter()
        .filter(|item| item.document_frequency * 5 >= corpus.overview().document_count * 4)
        .min_by(|left, right| left.dispersion.total_cmp(&right.dispersion));
    let concentrated = content
        .items
        .iter()
        .filter(|item| item.document_frequency <= 3)
        .max_by(|left, right| {
            left.dispersion
                .total_cmp(&right.dispersion)
                .then_with(|| left.frequency.cmp(&right.frequency))
        });
    let mut distributions = BTreeMap::<String, Value>::new();
    for token in [
        pair.as_ref().map(|(left, _)| left.token.as_str()),
        pair.as_ref().map(|(_, right)| right.token.as_str()),
        content_pair.as_ref().map(|(left, _)| left.token.as_str()),
        content_pair.as_ref().map(|(_, right)| right.token.as_str()),
        widespread.map(|item| item.token.as_str()),
        concentrated.map(|item| item.token.as_str()),
    ]
    .into_iter()
    .flatten()
    {
        distributions.entry(token.to_string()).or_insert_with(|| {
            serde_json::to_value(query_term_distribution(
                corpus,
                token,
                &corpuswright_core::analysis::TermDistributionQuery {
                    selection: AnalysisDocumentSelection::default(),
                    group_by: FrequencyGroupMode::Surface,
                    case_fold: true,
                    language: Some("en".into()),
                    offset: 0,
                    limit: corpus.overview().document_count,
                },
            ))
            .expect("term distribution must serialize")
        });
    }
    let growth = &corpus.overview().vocabulary_growth.points;
    let total = corpus.overview().total_tokens;
    let growth_milestones = [10u64, 25, 50, 75, 100]
        .into_iter()
        .map(|percent| {
            let target = total.saturating_mul(percent) / 100;
            let point = growth
                .iter()
                .min_by_key(|point| point.tokens.abs_diff(target));
            json!({ "percent": percent, "targetTokens": target, "observed": point })
        })
        .collect::<Vec<_>>();
    let quartiles = length_quartiles(corpus.overview());
    Ok(json!({
        "similarFrequencyDifferentDispersion": pair,
        "contentWordPair": content_pair,
        "widespreadTerm": widespread,
        "concentratedTerm": concentrated,
        "termDistributions": distributions,
        "documentLength": {
            "shortest": corpus.overview().documents.iter().min_by_key(|document| document.tokens),
            "longest": corpus.overview().documents.iter().max_by_key(|document| document.tokens),
            "quartiles": {
                "q1": quartiles.0,
                "median": quartiles.1,
                "q3": quartiles.2
            }
        },
        "vocabularyGrowthMilestones": growth_milestones,
        "rankFrequency": corpus.overview().rank_frequency,
        "lexicalSpectrum": corpus.overview().lexical_spectrum,
        "coverage": corpus.overview().frequency_concentration,
        "dispersionPlane": corpus.overview().dispersion.plane,
    }))
}

fn best_dispersion_pair(
    items: &[corpuswright_core::analysis::FrequencyTermItem],
) -> Option<(
    corpuswright_core::analysis::FrequencyTermItem,
    corpuswright_core::analysis::FrequencyTermItem,
)> {
    let mut best = None::<(f64, usize, &_, &_)>;
    for (index, left) in items.iter().enumerate() {
        for right in &items[index + 1..] {
            let larger = left.frequency.max(right.frequency);
            let smaller = left.frequency.min(right.frequency);
            if larger.saturating_sub(smaller) > (larger / 10).max(3) {
                continue;
            }
            let dispersion_gap = (left.dispersion - right.dispersion).abs();
            let document_gap = left.document_frequency.abs_diff(right.document_frequency);
            if dispersion_gap < 0.15 || document_gap < 3 {
                continue;
            }
            let key = (dispersion_gap, document_gap);
            if best
                .as_ref()
                .is_none_or(|current| key > (current.0, current.1))
            {
                best = Some((dispersion_gap, document_gap, left, right));
            }
        }
    }
    best.map(|(_, _, left, right)| (left.clone(), right.clone()))
}

fn length_quartiles(overview: &CorpusOverview) -> (f64, f64, f64) {
    let mut values = overview
        .documents
        .iter()
        .map(|document| document.tokens as f64)
        .collect::<Vec<_>>();
    values.sort_by(f64::total_cmp);
    let percentile = |fraction: f64| {
        if values.is_empty() {
            return 0.0;
        }
        let position = (values.len() - 1) as f64 * fraction;
        let lower = position.floor() as usize;
        let upper = position.ceil() as usize;
        if lower == upper {
            values[lower]
        } else {
            let weight = position - lower as f64;
            values[lower] * (1.0 - weight) + values[upper] * weight
        }
    };
    (percentile(0.25), percentile(0.5), percentile(0.75))
}

fn seed_audit_csv(audits: &[(SeedMetadata, SeedTechnicalAudit, u64, usize)]) -> String {
    let mut output = String::from(
        "path,title,author,source,translator,tokens,types,bytes,sha256,utf8,bom,lineEndings,nulBytes,pageNumberLines,repeatedLongLineGroups,startMarkers,endMarkers,status,disposition,warnings\n",
    );
    for (metadata, audit, tokens, types) in audits {
        let line_endings = if audit.crlf_count > 0 { "CRLF" } else { "LF" };
        let row = [
            metadata.source_path.clone(),
            metadata.title.clone(),
            metadata.author.clone(),
            metadata.probable_source.clone(),
            metadata.translator.clone().unwrap_or_default(),
            tokens.to_string(),
            types.to_string(),
            audit.bytes.to_string(),
            audit.sha256.clone(),
            audit.utf8_valid.to_string(),
            audit.bom.to_string(),
            line_endings.into(),
            audit.nul_bytes.to_string(),
            audit.page_number_line_count.to_string(),
            audit.repeated_long_line_groups.to_string(),
            audit.start_marker_count.to_string(),
            audit.end_marker_count.to_string(),
            metadata.redistribution_status.clone(),
            metadata.planned_disposition.clone(),
            metadata.qualification_warnings.join(" | "),
        ];
        output.push_str(&row.map(csv_escape).join(","));
        output.push('\n');
    }
    output
}

fn candidate_shortlist_csv(candidates: &[EvaluatedCandidate]) -> String {
    let mut output = String::from(
        "rank,ebookId,title,author,tokens,bytes,score,status,rationale,balanceContribution,reason\n",
    );
    for (index, candidate) in candidates.iter().enumerate() {
        let row = [
            (index + 1).to_string(),
            candidate.plan.ebook_id.to_string(),
            candidate.catalog.title.clone(),
            candidate.author.clone(),
            candidate
                .prepared
                .as_ref()
                .map_or_else(String::new, |ebook| ebook.tokens.to_string()),
            candidate
                .prepared
                .as_ref()
                .map_or_else(String::new, |ebook| ebook.cleaned.text.len().to_string()),
            candidate.evaluation.score.total.to_string(),
            if candidate.selected {
                "ACCEPTED"
            } else {
                "REJECTED"
            }
            .into(),
            candidate.plan.profile_rationale.clone(),
            candidate.plan.balance_contribution.clone(),
            candidate.selection_reason.clone(),
        ];
        output.push_str(&row.map(csv_escape).join(","));
        output.push('\n');
    }
    output
}

fn balance_csv(accepted: &[AcceptedDocument], overview: &CorpusOverview) -> String {
    let total = overview.total_tokens.max(1) as f64;
    let author_totals = accepted
        .iter()
        .fold(BTreeMap::new(), |mut totals, document| {
            *totals
                .entry(document.manifest.author.clone())
                .or_insert(0u64) += document.manifest.tokens;
            totals
        });
    let mut output = String::from("kind,name,author,tokens,tokenProportion,documentCount\n");
    for document in accepted {
        output.push_str(&format!(
            "document,{},{},{},{:.9},1\n",
            csv_escape(&document.manifest.filename),
            csv_escape(&document.manifest.author),
            document.manifest.tokens,
            document.manifest.tokens as f64 / total,
        ));
    }
    for (author, tokens) in author_totals {
        let count = accepted
            .iter()
            .filter(|document| document.manifest.author == author)
            .count();
        output.push_str(&format!(
            "author,{0},{0},{1},{2:.9},{3}\n",
            csv_escape(author),
            tokens,
            tokens as f64 / total,
            count,
        ));
    }
    output
}

fn selection_report(plan: &CandidatePlan, candidates: &[EvaluatedCandidate]) -> String {
    let selected = candidates
        .iter()
        .filter(|candidate| candidate.selected)
        .map(|candidate| {
            format!(
                "- *{}* — {} (ebook {}, score {}): {}",
                candidate.catalog.title,
                candidate.author,
                candidate.plan.ebook_id,
                candidate.evaluation.score.total,
                candidate.plan.profile_rationale
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let rejected = candidates
        .iter()
        .filter(|candidate| !candidate.selected)
        .take(8)
        .map(|candidate| {
            format!(
                "- *{}* — {} (ebook {}, score {}): {}",
                candidate.catalog.title,
                candidate.author,
                candidate.plan.ebook_id,
                candidate.evaluation.score.total,
                candidate.selection_reason
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "# Candidate selection report\n\n## Editorial profile\n\n{}\n\n## Selected works\n\n{}\n\n## Highest-ranking rejections\n\n{}\n\nThe temporary dossier's `candidate-shortlist.csv` and the durable corpus `selection.csv` record the complete shortlist and score components. Popularity is not a scoring component.\n",
        plan.profile, selected, rejected
    )
}

fn concept_figure_map() -> &'static str {
    "# Concepts-page evidence map\n\n| Topic | Evidence | Planned witness |\n|---|---|---|\n| Preparation and cleaning | FULL CORPUS RESULT | Cleaning report and provenance flow |\n| Tokens, types and lemmas | CONTROLLED ILLUSTRATION | Boundary and aggregation examples must isolate one rule |\n| Raw and normalised frequency | CONTROLLED ILLUSTRATION | Equal raw counts in unequal corpus sizes |\n| Document-length distribution | FULL CORPUS RESULT | Histogram, ECDF and Q–Q |\n| Lexical diversity | FULL CORPUS RESULT | MATTR distribution; selected low/high documents |\n| Hapax and dislegomena | FULL CORPUS RESULT | Lexical spectrum |\n| Rank–frequency and Zipf | FULL CORPUS RESULT | Rank curve with fitted interval |\n| Rank residuals | FULL CORPUS RESULT | Residual view |\n| Cumulative coverage | FULL CORPUS RESULT | Top-N cumulative coverage |\n| Vocabulary growth | FULL CORPUS RESULT | Observed growth checkpoints |\n| Heaps | FULL CORPUS RESULT | Observed curve and fitted model |\n| Entropy | TEXT OR FORMULA ONLY | Report observed entropy/effective vocabulary beside formula |\n| Corpus balance | FULL CORPUS RESULT | Document and author share table |\n| Dispersion | SELECTED CORPUS EXAMPLE | Reproducible similar-frequency term pair plus full profile |\n| Effective documents | CONTROLLED ILLUSTRATION | Even versus concentrated frequency at fixed total |\n| Reading the Overview | FULL CORPUS RESULT | Real Overview payload and coordinated witnesses |\n"
}

fn toy_requirements() -> &'static str {
    "# Controlled-illustration requirements for the next pass\n\nThe real corpus should remain primary. Controlled examples are still preferable for:\n\n- token, type and lemma distinctions where every non-target feature must remain fixed;\n- identical raw counts in corpora of different sizes;\n- identical frequency distributed perfectly evenly versus concentrated in one document;\n- balanced versus deliberately dominated corpus composition;\n- even versus concentrated probability mass for entropy;\n- effective-document intuition at a fixed total frequency.\n\nNo Tier 3 figure is implemented in Pass A.\n"
}

#[allow(clippy::too_many_arguments)]
fn dossier_markdown(
    repository_root: &Path,
    _seed_metadata: &SeedMetadataFile,
    candidates: &[EvaluatedCandidate],
    accepted: &[AcceptedDocument],
    catalog_sha256: &str,
    corpus_identity: &str,
    overview: &CorpusOverview,
    seed: &CorpusOverview,
) -> String {
    let quartiles = length_quartiles(overview);
    let author_totals = accepted
        .iter()
        .fold(BTreeMap::new(), |mut totals, document| {
            *totals
                .entry(document.manifest.author.clone())
                .or_insert(0u64) += document.manifest.tokens;
            totals
        });
    let largest_author = author_totals
        .iter()
        .max_by_key(|(_, tokens)| *tokens)
        .map(|(author, tokens)| (author.as_str(), *tokens))
        .unwrap_or(("none", 0));
    let mut shares = accepted
        .iter()
        .map(|document| document.manifest.tokens)
        .collect::<Vec<_>>();
    shares.sort_unstable_by(|left, right| right.cmp(left));
    let total = overview.total_tokens.max(1) as f64;
    let largest_share = shares.first().copied().unwrap_or(0) as f64 / total;
    let two_largest_share = shares.iter().take(2).sum::<u64>() as f64 / total;
    let seed_total = seed.total_tokens.max(1) as f64;
    let mut seed_lengths = seed
        .documents
        .iter()
        .map(|document| document.tokens)
        .collect::<Vec<_>>();
    seed_lengths.sort_unstable_by(|left, right| right.cmp(left));
    let commit =
        git_output(repository_root, ["rev-parse", "HEAD"]).unwrap_or_else(|_| "unknown".into());
    format!(
        "# {CORPUS_TITLE}: qualification dossier\n\n## Provenance and selection\n\nCatalogue `{}` (SHA-256 `{}`) was cached once. The complete {}-work shortlist was scored reproducibly; {} new works were selected. Eighteen accepted texts come from recorded U.S. Project Gutenberg ebook IDs, permanent landing pages, and pinned raw checksums. The two Project Gutenberg Australia Orwell seeds are included under Brazil's life-plus-70-year rule and carry an explicit jurisdiction warning; no United States public-domain claim is made for them.\n\n## Final corpus\n\n- Repository commit: `{}`\n- Accepted documents: {}\n- Authors: {}\n- Tokens: {}\n- Types: {}\n- Document quartiles: {:.1} / {:.1} / {:.1}\n- Largest-document share: {:.2}%\n- Two-largest-document share: {:.2}%\n- Largest-author share: {} at {:.2}%\n- Corpus checksum: `{}`\n\n## Original-nine comparison\n\nThe raw nine-file seed set contains {} CorpusWright tokens. Its largest-document share is {:.2}% and its two-largest share is {:.2}%. The expanded qualified corpus changes those figures to {:.2}% and {:.2}% while reducing the Oscar Wilde cluster to three accepted documents and retaining both Orwell works. The raw-seed comparison includes retained licence material in seven inputs because their start markers had already been removed; that limitation is explicit rather than silently guessed away.\n\n## Suitability judgement\n\n**QUALIFIED**\n\nQualified for the documented Brazilian distribution context. Accepted files are valid UTF-8, deterministically cleaned from unambiguous markers, duplicate-free, processable by the real CorpusWright Overview, and documented by catalogue, retrieval, raw, and cleaned identities. The corpus retains the serious literary seed identity while adding authors, modernist and realist modes, essays, drama, hybrid prose/poetry, and useful length variation. It remains a curated pedagogical corpus, not a representative sample.\n\n## Figure dossier\n\nThe Rust payload is `dossier.json`. After the frontend capture command runs, `figures/light/`, `figures/dark/`, and `figures/png/` contain actual CorpusWright witnesses for document histogram/ECDF/Q–Q, MATTR, rank curve/residuals, lexical spectrum, cumulative coverage, vocabulary growth/Heaps, dispersion profile, and dispersion plane. See the generated figure manifest for inspection notes and Concepts-page roles.\n\n## Next pass\n\nUse the evidence classifications in `concept-figure-map.md`: integrate full-corpus figures first, then reproducibly selected corpus examples, and create narrowly controlled Tier 3 illustrations only for concepts listed in `toy-requirements.md`. Do not treat this temporary review page as the website design.\n",
        CATALOGUE_URL,
        catalog_sha256,
        candidates.len(),
        candidates
            .iter()
            .filter(|candidate| candidate.selected)
            .count(),
        commit,
        overview.document_count,
        author_totals.len(),
        overview.total_tokens,
        overview.total_types,
        quartiles.0,
        quartiles.1,
        quartiles.2,
        largest_share * 100.0,
        two_largest_share * 100.0,
        largest_author.0,
        largest_author.1 as f64 / total * 100.0,
        corpus_identity,
        seed.total_tokens,
        seed_lengths.first().copied().unwrap_or(0) as f64 / seed_total * 100.0,
        seed_lengths.iter().take(2).sum::<u64>() as f64 / seed_total * 100.0,
        largest_share * 100.0,
        two_largest_share * 100.0,
    )
}

fn dossier_index_html() -> &'static str {
    "<!doctype html>\n<html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>CorpusWright documentation corpus dossier</title><style>body{font:16px/1.55 system-ui;margin:auto;max-width:1100px;padding:2rem;color:#172033;background:#f5f7fb}h1,h2{line-height:1.2}nav{display:flex;gap:1rem;flex-wrap:wrap}a{color:#164ea3}.grid{display:grid;grid-template-columns:repeat(auto-fit,minmax(300px,1fr));gap:1rem}.grid img{width:100%;background:white;border:1px solid #ccd4e0}.note{padding:1rem;background:white;border-left:4px solid #476fd8}</style></head><body><h1>CorpusWright Documentation Books Corpus</h1><p class=\"note\">Temporary Pass A review dossier. This is not the future Concepts-page design.</p><nav><a href=\"dossier.md\">Dossier</a><a href=\"selection-report.md\">Selection</a><a href=\"concept-figure-map.md\">Evidence map</a><a href=\"toy-requirements.md\">Toy requirements</a><a href=\"candidate-examples.json\">Examples</a></nav><h2>Real CorpusWright figure witnesses</h2><div id=\"figures\" class=\"grid\"></div><script>fetch('figures/manifest.json').then(r=>r.json()).then(m=>{const root=document.querySelector('#figures');for(const f of m.figures){const a=document.createElement('a');a.href=f.svgPath;const img=document.createElement('img');img.src=f.pngPath;img.alt=f.title+' — '+f.theme;a.append(img);root.append(a)}}).catch(()=>document.querySelector('#figures').textContent='Run the frontend capture command to populate figures.');</script></body></html>\n"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn documentation_texts_generate_a_real_overview() {
        let root = env::temp_dir().join(format!(
            "corpuswright-documentation-overview-test-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("First - Author A.txt"), "One two two.\n").unwrap();
        fs::write(root.join("Second - Author B.txt"), "Three four five.\n").unwrap();

        let analysis = build_real_analysis(&root, "documentation-overview-test").unwrap();
        assert_eq!(analysis.overview().document_count, 2);
        assert_eq!(analysis.overview().total_tokens, 6);
        assert_eq!(analysis.overview().total_types, 5);

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn australia_footer_cleaning_preserves_body_and_canonicalises_the_end() {
        let seed = SeedMetadata {
            source_path: "seed.txt".into(),
            title: "Animal Farm".into(),
            author: "George Orwell".into(),
            translator: None,
            original_publication_year: Some(1945),
            edition_publication_year: None,
            probable_source: "Project Gutenberg Australia".into(),
            us_project_gutenberg_ebook_id: None,
            redistribution_status: "ACCEPTED WITH NOTE".into(),
            planned_disposition: "test".into(),
            qualification_warnings: Vec::new(),
        };
        let body = "CHAPTER I\r\n\r\nBody text and a footnote.[1]\r\n".repeat(20);
        let raw = format!("{body}\r\nTHE END\r\n\r\nProject Gutenberg Australia\r\n");
        let document = accepted_australia_seed_document(&seed, raw.as_bytes()).unwrap();

        assert!(
            document
                .text
                .contains("CHAPTER I\n\nBody text and a footnote.[1]")
        );
        assert!(document.text.ends_with("THE END\n"));
        assert!(!document.text.contains("Project Gutenberg Australia"));
        assert_eq!(
            document.manifest.redistribution_status,
            "ACCEPTED WITH NOTE"
        );
    }
}
