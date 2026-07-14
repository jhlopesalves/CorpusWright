//! Repository orchestration for CorpusWright.
//!
//! Product logic remains in `corpuswright-core`; this binary owns validation, generated local
//! fixtures, dossier capture, security orchestration, and packaging commands that must never enter
//! release binaries.

use anyhow::{Context, Result, anyhow, bail};
use corpuswright_core::analysis::{
    AnalysisBuildOptions, CorpusOverviewContext, OverviewDistributionMetric,
    OverviewDistributionQuery, OverviewDistributionScale, OverviewHistogramRule,
    OverviewQqReference, build_analysis_corpus, cleaning_config_signature,
    query_overview_distribution, resolve_analysis_input,
};
use corpuswright_core::preparation::cleaning::CleaningConfig;
use corpuswright_core::sources::cache::{
    DocumentTextMode, ExtractionCache, document_text_for_record,
};
use corpuswright_core::support::hash::sha256_hex;
use corpuswright_core::text::tokenisation::TokenizationConfig;
use corpuswright_core::{CancellationToken, ExportOptions, export_corpus, scan_directory};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

fn main() {
    if let Err(error) = dispatch(env::args().skip(1).collect()) {
        eprintln!("xtask failed: {error:#}");
        std::process::exit(1);
    }
}

fn dispatch(args: Vec<String>) -> Result<()> {
    let Some(command) = args.first().map(String::as_str) else {
        print_help();
        return Ok(());
    };
    match command {
        "check" => check(&args[1..]),
        "repository-policy" => repository_policy(),
        "generate-fixtures" => generate_fixtures(&args[1..]),
        "documentation-dossier" => documentation_dossier(&args[1..]),
        "security" => security(),
        "browser-test" => run_named("frontend browser tests", npm(&["run", "test:browser"])),
        "package-linux" => package_linux(&args[1..]),
        "release-preflight" => release_preflight(),
        "help" | "--help" | "-h" => {
            print_help();
            Ok(())
        }
        unknown => bail!("unknown xtask command `{unknown}`; run `cargo xtask help`"),
    }
}

fn print_help() {
    println!(
        "CorpusWright repository tasks:\n\
         cargo xtask check [--fresh-node] [--scope core|desktop|frontend]\n\
         cargo xtask repository-policy\n\
         cargo xtask generate-fixtures --output <path>\n\
         cargo xtask documentation-dossier --corpus-dir <path> --output <path> [--include-source-text] [--skip-figures]\n\
         cargo xtask security\n\
         cargo xtask browser-test\n\
         cargo xtask package-linux [--install-smoke]\n\
         cargo xtask release-preflight"
    );
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CheckScope {
    All,
    Core,
    Desktop,
    Frontend,
}

fn check(args: &[String]) -> Result<()> {
    let fresh_node = args.iter().any(|arg| arg == "--fresh-node");
    let scope = option_value(args, "--scope")?
        .map(|scope| match scope.as_str() {
            "core" => Ok(CheckScope::Core),
            "desktop" => Ok(CheckScope::Desktop),
            "frontend" => Ok(CheckScope::Frontend),
            other => Err(anyhow!("unknown check scope `{other}`")),
        })
        .transpose()?
        .unwrap_or(CheckScope::All);
    reject_unknown_flags(args, &["--fresh-node", "--scope"])?;

    run_named("Rust formatting", cargo(&["fmt", "--all", "--check"]))?;
    if matches!(scope, CheckScope::All | CheckScope::Core) {
        run_named(
            "core Clippy",
            cargo(&[
                "clippy",
                "-p",
                "corpuswright-core",
                "--all-targets",
                "--",
                "-D",
                "warnings",
            ]),
        )?;
        run_named(
            "test-support Clippy",
            cargo(&[
                "clippy",
                "-p",
                "corpuswright-test-support",
                "--all-targets",
                "--",
                "-D",
                "warnings",
            ]),
        )?;
        run_named(
            "core tests",
            cargo(&["test", "-p", "corpuswright-core", "--", "--test-threads=1"]),
        )?;
        run_named(
            "test-support tests",
            cargo(&[
                "test",
                "-p",
                "corpuswright-test-support",
                "--",
                "--test-threads=1",
            ]),
        )?;
        run_named(
            "generated TypeScript bindings",
            cargo(&["run", "-p", "corpuswright-core", "--example", "export_ts"]),
        )?;
        run_named(
            "generated binding drift",
            git(&["diff", "--exit-code", "--", "apps/desktop/src/generated/"]),
        )?;
    }
    if matches!(scope, CheckScope::All | CheckScope::Desktop) {
        run_named(
            "desktop check",
            cargo(&["check", "-p", "corpuswright-desktop", "--all-targets"]),
        )?;
        run_named(
            "desktop Clippy",
            cargo(&[
                "clippy",
                "-p",
                "corpuswright-desktop",
                "--all-targets",
                "--",
                "-D",
                "warnings",
            ]),
        )?;
        run_named(
            "desktop tests",
            cargo(&[
                "test",
                "-p",
                "corpuswright-desktop",
                "--",
                "--test-threads=1",
            ]),
        )?;
    }
    if matches!(scope, CheckScope::All | CheckScope::Frontend) {
        if fresh_node {
            run_named("fresh frontend dependencies", npm(&["ci"]))?;
        } else if !repository_root().join("apps/desktop/node_modules").is_dir() {
            bail!("frontend dependencies are missing; run `cargo xtask check --fresh-node`");
        }
        run_named("frontend check", npm(&["run", "check"]))?;
    }
    if scope == CheckScope::All {
        release_preflight()?;
        run_named("working-tree whitespace", git(&["diff", "--check"]))?;
    }
    println!("CorpusWright check passed for scope {scope:?}.");
    Ok(())
}

fn repository_policy() -> Result<()> {
    run_named(
        "repository data policy",
        node(&["scripts/check-repository-data.cjs"]),
    )?;
    run_named(
        "architecture policy",
        node(&["scripts/check-architecture.cjs"]),
    )
}

fn generate_fixtures(args: &[String]) -> Result<()> {
    let output = required_path(args, "--output")?;
    reject_unknown_flags(args, &["--output"])?;
    let paths = corpuswright_test_support::generate_corpus_at(&output)
        .with_context(|| format!("generate fixtures under {}", output.display()))?;
    println!(
        "Generated {} deterministic synthetic documents under {}.",
        paths.len(),
        output.display()
    );
    Ok(())
}

fn documentation_dossier(args: &[String]) -> Result<()> {
    let corpus_dir = required_path(args, "--corpus-dir")?;
    let output = required_path(args, "--output")?;
    let include_source_text = args.iter().any(|arg| arg == "--include-source-text");
    let skip_figures = args.iter().any(|arg| arg == "--skip-figures");
    reject_unknown_flags(
        args,
        &[
            "--corpus-dir",
            "--output",
            "--include-source-text",
            "--skip-figures",
        ],
    )?;
    if !corpus_dir.is_dir() {
        bail!("corpus directory does not exist: {}", corpus_dir.display());
    }
    fs::create_dir_all(&output)
        .with_context(|| format!("create dossier output {}", output.display()))?;

    let scan = scan_directory(&corpus_dir).map_err(|error| anyhow!("{error:?}"))?;
    if scan.files.is_empty() {
        bail!(
            "no CorpusWright-supported documents found in {}",
            corpus_dir.display()
        );
    }
    let cleaning = CleaningConfig::default();
    let cache = ExtractionCache::new();
    let cancellation = CancellationToken::new();
    let config_signature = cleaning_config_signature(&cleaning);
    let corpus_checksum = checksum_records(&scan.files)?;
    let analysis = build_analysis_corpus(
        &scan.files,
        AnalysisBuildOptions {
            built_at: "deterministic-local-dossier".to_string(),
            build_signature: corpus_checksum.clone(),
            processing_config_signature: config_signature.clone(),
            overview_context: CorpusOverviewContext {
                sources_in_project: Some(scan.files.len()),
                sources_with_warnings: None,
                failed_sources: 0,
                config_version: None,
                metadata_attached: false,
            },
            udpipe_model_dir: None,
            force_compute_hashes: true,
            language_overrides: HashMap::new(),
            tokenization: TokenizationConfig::default(),
            annotation_cache_root: None,
        },
        &cancellation,
        |record| {
            Ok(resolve_analysis_input(
                record,
                &cleaning,
                |record, config| {
                    document_text_for_record(
                        record,
                        config,
                        DocumentTextMode::Processed,
                        Some(&cache),
                    )
                    .map(|document| document.text)
                },
                |record, config| {
                    document_text_for_record(
                        record,
                        config,
                        DocumentTextMode::Original,
                        Some(&cache),
                    )
                    .map(|document| document.text)
                },
            ))
        },
        |_| {},
    )
    .map_err(|error| anyhow!(error.to_string()))?;

    let overview = analysis.overview().clone();
    let document_distribution = query_overview_distribution(
        &analysis,
        &OverviewDistributionQuery {
            metric: OverviewDistributionMetric::DocumentLength,
            histogram_rule: OverviewHistogramRule::Auto,
            manual_bin_count: 12,
            x_scale: OverviewDistributionScale::Log,
            qq_reference: OverviewQqReference::Lognormal,
            maximum_returned_point_count: 500,
        },
    )
    .map_err(|error| anyhow!(error))?;
    let mattr_distribution = query_overview_distribution(
        &analysis,
        &OverviewDistributionQuery {
            metric: OverviewDistributionMetric::Mattr,
            histogram_rule: OverviewHistogramRule::Auto,
            manual_bin_count: 12,
            x_scale: OverviewDistributionScale::Linear,
            qq_reference: OverviewQqReference::Normal,
            maximum_returned_point_count: 500,
        },
    )
    .map_err(|error| anyhow!(error))?;

    let checksums = scan
        .files
        .iter()
        .map(|record| {
            let bytes = fs::read(&record.source_path)?;
            Ok((
                record.relative_path.to_string_lossy().into_owned(),
                Value::String(sha256_hex(&bytes)),
            ))
        })
        .collect::<io::Result<serde_json::Map<String, Value>>>()?;
    let source_text = include_source_text.then(|| {
        scan.files
            .iter()
            .map(|record| {
                let text = document_text_for_record(
                    record,
                    &cleaning,
                    DocumentTextMode::Original,
                    Some(&cache),
                )
                .map(|document| document.text)?;
                Ok((record.relative_path.to_string_lossy().into_owned(), text))
            })
            .collect::<Result<HashMap<_, _>, String>>()
    });
    let payload = json!({
        "schemaVersion": 1,
        "corpusId": format!("local-{corpus_checksum}"),
        "corpusChecksum": corpus_checksum,
        "configuration": {
            "cleaning": cleaning,
            "cleaningSignature": config_signature,
            "tokenization": TokenizationConfig::default(),
            "sourceTextIncluded": include_source_text,
        },
        "checksums": checksums,
        "overview": overview,
        "documentDistribution": document_distribution,
        "mattrDistribution": mattr_distribution,
        "sourceText": source_text.transpose().map_err(|error| anyhow!(error))?,
    });
    fs::write(
        output.join("dossier.json"),
        format!("{}\n", serde_json::to_string_pretty(&payload)?),
    )?;
    let working_export = output.join(".working-export");
    let export_report = export_corpus(
        &scan.files,
        &working_export,
        &cleaning,
        &ExportOptions {
            app_name: "CorpusWright local dossier".to_string(),
            app_version: None,
            overwrite: true,
        },
        None,
        Some(&cache),
    )
    .map_err(|error| anyhow!("{error:?}"))?;
    fs::write(
        output.join("export-evidence.json"),
        format!("{}\n", serde_json::to_string_pretty(&export_report)?),
    )?;
    fs::remove_dir_all(&working_export).with_context(|| {
        format!(
            "remove temporary content-bearing export {}",
            working_export.display()
        )
    })?;

    if !skip_figures {
        let mut command = node(&["apps/desktop/scripts/capture-local-corpus-dossier.mjs"]);
        command.env("CORPUSWRIGHT_LOCAL_DOSSIER", &output);
        run_named("local dossier figure capture", command)?;
    }
    println!("Wrote deterministic local dossier to {}.", output.display());
    Ok(())
}

fn checksum_records(records: &[corpuswright_core::corpus::DocumentRecord]) -> Result<String> {
    let mut bytes = Vec::new();
    for record in records {
        bytes.extend_from_slice(record.relative_path.to_string_lossy().as_bytes());
        bytes.push(0);
        bytes.extend_from_slice(&fs::read(&record.source_path)?);
        bytes.push(0xff);
    }
    Ok(sha256_hex(&bytes))
}

fn security() -> Result<()> {
    if command_available("cargo-deny", &["--version"]) {
        run_named("Cargo dependency policy", cargo(&["deny", "check"]))?;
    } else {
        bail!("cargo-deny is required; install it with `cargo install cargo-deny --locked`");
    }
    run_named(
        "npm dependency audit",
        npm(&["audit", "--audit-level=high"]),
    )?;
    run_named(
        "parser hardening tests",
        cargo(&["test", "-p", "corpuswright-core", "sources::docx::tests"]),
    )?;
    repository_policy()?;
    Ok(())
}

fn package_linux(args: &[String]) -> Result<()> {
    reject_unknown_flags(args, &["--install-smoke"])?;
    if !cfg!(target_os = "linux") {
        bail!("Linux Debian packaging requires a Linux host");
    }
    println!(
        "Ubuntu prerequisites: build-essential curl file jq libayatana-appindicator3-dev \
         libgtk-3-dev librsvg2-dev libssl-dev libwebkit2gtk-4.1-dev libxdo-dev patchelf xvfb"
    );
    run_named(
        "verified Linux PDFium",
        node(&[
            "scripts/fetch-pdfium.cjs",
            "linux-x64",
            "--output-dir",
            "apps/desktop/src-tauri/resources/ocr",
        ]),
    )?;
    let install_smoke = args.iter().any(|arg| arg == "--install-smoke");
    let package_args = if install_smoke {
        vec![
            "run",
            "tauri",
            "build",
            "--",
            "--bundles",
            "deb",
            "--features",
            "native-smoke",
        ]
    } else {
        vec!["run", "tauri", "build", "--", "--bundles", "deb"]
    };
    run_named("Linux Debian package", npm(&package_args))?;
    let packages = find_deb_packages(&repository_root().join("target/release/bundle/deb"))?;
    for package in &packages {
        println!("Built Debian package: {}", package.display());
        run_named(
            "Debian package contents",
            command("dpkg-deb", &["--contents", path_arg(package)?]),
        )?;
    }
    if install_smoke {
        print!(
            "Install, exercise, and uninstall the dedicated native-smoke package with sudo? [y/N] "
        );
        io::stdout().flush()?;
        let mut answer = String::new();
        io::stdin().read_line(&mut answer)?;
        if matches!(answer.trim().to_ascii_lowercase().as_str(), "y" | "yes") {
            if !command_available("xvfb-run", &["--help"]) {
                bail!("installed smoke requires xvfb-run (Ubuntu package: xvfb)");
            }
            let package = packages
                .first()
                .ok_or_else(|| anyhow!("no Debian package was produced"))?;
            let package_name = Command::new("dpkg-deb")
                .args(["--field", path_arg(package)?, "Package"])
                .output()
                .context("read Debian package name")?;
            if !package_name.status.success() {
                bail!("dpkg-deb could not read the package name");
            }
            let package_name = String::from_utf8(package_name.stdout)?.trim().to_string();
            run_named(
                "install Debian package",
                command("sudo", &["apt-get", "install", "-y", path_arg(package)?]),
            )?;
            let smoke_root = PathBuf::from(format!(
                "/tmp/corpuswright-native-smoke-{}",
                std::process::id()
            ));
            let corpus_dir = smoke_root.join("corpus");
            let export_dir = smoke_root.join("export");
            let result_path = smoke_root.join("result.json");
            let smoke_result = (|| -> Result<()> {
                fs::create_dir_all(&smoke_root)?;
                corpuswright_test_support::generate_corpus_at(&corpus_dir)?;
                let mut smoke = command(
                    "timeout",
                    &["180s", "xvfb-run", "-a", "/usr/bin/corpuswright-desktop"],
                );
                smoke
                    .env("CORPUSWRIGHT_NATIVE_SMOKE_CORPUS", &corpus_dir)
                    .env("CORPUSWRIGHT_NATIVE_SMOKE_EXPORT", &export_dir)
                    .env("CORPUSWRIGHT_NATIVE_SMOKE_RESULT", &result_path);
                run_named("installed native smoke", smoke)?;
                let evidence: Value = serde_json::from_slice(&fs::read(&result_path)?)?;
                if evidence.get("status").and_then(Value::as_str) != Some("passed") {
                    bail!("installed native smoke failed: {evidence}");
                }
                println!("Installed resource and workflow evidence: {evidence}");
                Ok(())
            })();
            let uninstall_result = run_named(
                "uninstall Debian package",
                command("sudo", &["apt-get", "remove", "-y", &package_name]),
            );
            let _ = fs::remove_dir_all(&smoke_root);
            smoke_result?;
            uninstall_result?;
            if Path::new("/usr/bin/corpuswright-desktop").exists() {
                bail!("Debian package uninstall left /usr/bin/corpuswright-desktop behind");
            }
        } else {
            println!("Installation skipped; no sudo command was run.");
        }
    }
    Ok(())
}

fn find_deb_packages(directory: &Path) -> Result<Vec<PathBuf>> {
    let mut packages = fs::read_dir(directory)
        .with_context(|| format!("read package directory {}", directory.display()))?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.extension().is_some_and(|extension| extension == "deb"))
        .collect::<Vec<_>>();
    packages.sort();
    if packages.is_empty() {
        bail!("no .deb package found under {}", directory.display());
    }
    Ok(packages)
}

fn release_preflight() -> Result<()> {
    run_named(
        "release metadata consistency",
        node(&["scripts/check-release-consistency.cjs"]),
    )?;
    run_named(
        "repository reference regression tests",
        node(&["--test", "scripts/check-repository-references.test.cjs"]),
    )?;
    run_named(
        "repository references",
        node(&["scripts/check-repository-references.cjs"]),
    )?;
    run_named("PDFium provenance", node(&["scripts/check-pdfium.cjs"]))?;
    repository_policy()
}

fn required_path(args: &[String], flag: &str) -> Result<PathBuf> {
    option_value(args, flag)?
        .map(PathBuf::from)
        .ok_or_else(|| anyhow!("{flag} <path> is required"))
}

fn option_value(args: &[String], flag: &str) -> Result<Option<String>> {
    let mut found = None;
    let mut index = 0;
    while index < args.len() {
        if args[index] == flag {
            let value = args
                .get(index + 1)
                .filter(|value| !value.starts_with("--"))
                .ok_or_else(|| anyhow!("{flag} requires a value"))?;
            if found.replace(value.clone()).is_some() {
                bail!("{flag} may be supplied only once");
            }
            index += 2;
        } else {
            index += 1;
        }
    }
    Ok(found)
}

fn reject_unknown_flags(args: &[String], flags: &[&str]) -> Result<()> {
    let value_flags = flags
        .iter()
        .filter(|flag| matches!(**flag, "--scope" | "--output" | "--corpus-dir"))
        .copied()
        .collect::<Vec<_>>();
    let mut index = 0;
    while index < args.len() {
        let arg = &args[index];
        if !flags.contains(&arg.as_str()) {
            bail!("unexpected argument `{arg}`");
        }
        index += if value_flags.contains(&arg.as_str()) {
            2
        } else {
            1
        };
    }
    Ok(())
}

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("xtask crate must live under crates/")
        .to_path_buf()
}

fn run_named(label: &str, mut command: Command) -> Result<()> {
    println!("==> {label}");
    command.current_dir(repository_root());
    let status = command.status().with_context(|| format!("start {label}"))?;
    if !status.success() {
        bail!("{label} failed with {status}");
    }
    Ok(())
}

fn cargo(args: &[&str]) -> Command {
    command("cargo", args)
}

fn git(args: &[&str]) -> Command {
    command("git", args)
}

fn node(args: &[&str]) -> Command {
    command("node", args)
}

fn npm(args: &[&str]) -> Command {
    let mut command = Command::new("npm");
    command.arg("--prefix").arg("apps/desktop").args(args);
    command
}

fn command(program: &str, args: &[&str]) -> Command {
    let mut command = Command::new(program);
    command.args(args);
    command
}

fn command_available(program: &str, args: &[&str]) -> bool {
    Command::new(program)
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

fn path_arg(path: &Path) -> Result<&str> {
    path.to_str()
        .ok_or_else(|| anyhow!("path is not valid UTF-8: {}", path.display()))
}
