//! Example binary to generate TypeScript type definitions from Rust structs/enums
//! for frontend-facing IPC/domain types.
//!
//! Usage:
//!   cargo run -p corpusaid-core --example export_ts
//!
//! This writes .ts files to apps/desktop/src/generated/.

use std::fs;
use std::path::{Path, PathBuf};
use ts_rs::TS;

use corpusaid_core::clean::{
    CleaningConfig, PdfEmbeddedTextStrategy, ReplacementRule, TableExtractionStrategy,
};
use corpusaid_core::export::{ExportReport, ExportWarning, ExportWarningKind};
use corpusaid_core::manifest::{ExportManifest, ManifestFileRecord};
use corpusaid_core::preview::{CombinedPreview, FilePreview, PreviewWarning, PreviewWarningKind};
use corpusaid_core::repeated_artifacts::{
    ArtifactRiskLabel, CandidateContentClass, PositionSummary, RepeatedArtifactCandidate,
    RepeatedArtifactExample, RepeatedArtifactKind, RepeatedArtifactScanConfig,
    RepeatedArtifactScanDiagnostics, RepeatedArtifactScanReport,
};
use corpusaid_core::scan::{
    CorpusSummary, DocumentRecord, DocumentType, DocumentTypeCounts, ScanReport,
};
use corpusaid_core::search::{SearchHit, SearchResult};

struct TypeImport {
    name: &'static str,
    file: &'static str,
}

fn export_type<T: TS>(out_dir: &Path, imports: &[TypeImport], re_exports: &[&str]) {
    let mut content = String::new();
    for import in imports {
        content.push_str(&format!(
            "import type {{ {} }} from \"./{}.js\";\n",
            import.name, import.file
        ));
    }
    if !imports.is_empty() {
        content.push('\n');
    }
    if !re_exports.is_empty() {
        content.push_str(&format!("export type {{ {} }};\n\n", re_exports.join(", ")));
    }
    content.push_str(&format!("export {}\n", T::decl().trim_end()));

    let file_path = out_dir.join(format!("{}.ts", T::name()));
    fs::write(&file_path, &content)
        .unwrap_or_else(|e| panic!("failed to write {}: {}", file_path.display(), e));
    eprintln!("  wrote {}", file_path.display());
}

fn main() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let out_dir = manifest_dir
        .join("../../apps/desktop/src/generated/")
        .canonicalize()
        .expect("failed to resolve output directory");

    eprintln!("Exporting TypeScript bindings to {}", out_dir.display());

    export_type::<ReplacementRule>(&out_dir, &[], &[]);
    export_type::<TableExtractionStrategy>(&out_dir, &[], &[]);
    export_type::<PdfEmbeddedTextStrategy>(&out_dir, &[], &[]);
    export_type::<CleaningConfig>(
        &out_dir,
        &[
            TypeImport {
                name: "ReplacementRule",
                file: "ReplacementRule",
            },
            TypeImport {
                name: "TableExtractionStrategy",
                file: "TableExtractionStrategy",
            },
            TypeImport {
                name: "PdfEmbeddedTextStrategy",
                file: "PdfEmbeddedTextStrategy",
            },
        ],
        &[
            "ReplacementRule",
            "TableExtractionStrategy",
            "PdfEmbeddedTextStrategy",
        ],
    );

    export_type::<DocumentType>(&out_dir, &[], &[]);
    export_type::<DocumentRecord>(
        &out_dir,
        &[TypeImport {
            name: "DocumentType",
            file: "DocumentType",
        }],
        &[],
    );
    export_type::<DocumentTypeCounts>(&out_dir, &[], &[]);
    export_type::<CorpusSummary>(
        &out_dir,
        &[TypeImport {
            name: "DocumentTypeCounts",
            file: "DocumentTypeCounts",
        }],
        &[],
    );
    export_type::<ScanReport>(
        &out_dir,
        &[
            TypeImport {
                name: "DocumentRecord",
                file: "DocumentRecord",
            },
            TypeImport {
                name: "CorpusSummary",
                file: "CorpusSummary",
            },
        ],
        &[],
    );

    export_type::<PreviewWarningKind>(&out_dir, &[], &[]);
    export_type::<PreviewWarning>(
        &out_dir,
        &[TypeImport {
            name: "PreviewWarningKind",
            file: "PreviewWarningKind",
        }],
        &[],
    );
    export_type::<FilePreview>(
        &out_dir,
        &[
            TypeImport {
                name: "DocumentType",
                file: "DocumentType",
            },
            TypeImport {
                name: "PreviewWarning",
                file: "PreviewWarning",
            },
        ],
        &[],
    );
    export_type::<CombinedPreview>(
        &out_dir,
        &[
            TypeImport {
                name: "FilePreview",
                file: "FilePreview",
            },
            TypeImport {
                name: "PreviewWarning",
                file: "PreviewWarning",
            },
        ],
        &[],
    );

    export_type::<SearchHit>(&out_dir, &[], &[]);
    export_type::<SearchResult>(
        &out_dir,
        &[TypeImport {
            name: "SearchHit",
            file: "SearchHit",
        }],
        &[],
    );

    export_type::<ExportWarningKind>(&out_dir, &[], &[]);
    export_type::<ExportWarning>(
        &out_dir,
        &[TypeImport {
            name: "ExportWarningKind",
            file: "ExportWarningKind",
        }],
        &[],
    );
    export_type::<ManifestFileRecord>(
        &out_dir,
        &[TypeImport {
            name: "DocumentType",
            file: "DocumentType",
        }],
        &[],
    );
    export_type::<ExportManifest>(
        &out_dir,
        &[
            TypeImport {
                name: "CleaningConfig",
                file: "CleaningConfig",
            },
            TypeImport {
                name: "ManifestFileRecord",
                file: "ManifestFileRecord",
            },
        ],
        &[],
    );
    export_type::<ExportReport>(
        &out_dir,
        &[
            TypeImport {
                name: "ManifestFileRecord",
                file: "ManifestFileRecord",
            },
            TypeImport {
                name: "ExportWarning",
                file: "ExportWarning",
            },
            TypeImport {
                name: "ExportManifest",
                file: "ExportManifest",
            },
        ],
        &[],
    );

    export_type::<RepeatedArtifactScanConfig>(&out_dir, &[], &[]);
    export_type::<RepeatedArtifactKind>(&out_dir, &[], &[]);
    export_type::<ArtifactRiskLabel>(&out_dir, &[], &[]);
    export_type::<CandidateContentClass>(&out_dir, &[], &[]);
    export_type::<PositionSummary>(&out_dir, &[], &[]);
    export_type::<RepeatedArtifactExample>(&out_dir, &[], &[]);
    export_type::<RepeatedArtifactCandidate>(
        &out_dir,
        &[
            TypeImport {
                name: "RepeatedArtifactKind",
                file: "RepeatedArtifactKind",
            },
            TypeImport {
                name: "PositionSummary",
                file: "PositionSummary",
            },
            TypeImport {
                name: "ArtifactRiskLabel",
                file: "ArtifactRiskLabel",
            },
            TypeImport {
                name: "CandidateContentClass",
                file: "CandidateContentClass",
            },
            TypeImport {
                name: "RepeatedArtifactExample",
                file: "RepeatedArtifactExample",
            },
        ],
        &[],
    );
    export_type::<RepeatedArtifactScanDiagnostics>(&out_dir, &[], &[]);
    export_type::<RepeatedArtifactScanReport>(
        &out_dir,
        &[
            TypeImport {
                name: "RepeatedArtifactCandidate",
                file: "RepeatedArtifactCandidate",
            },
            TypeImport {
                name: "RepeatedArtifactScanDiagnostics",
                file: "RepeatedArtifactScanDiagnostics",
            },
        ],
        &[],
    );

    eprintln!("Done.");
}
