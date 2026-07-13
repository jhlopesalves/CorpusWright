//! Generates TypeScript type definitions for frontend-facing IPC and domain types.
//!
//! Output is written under `apps/desktop/src/generated/`.

use std::fs;
use std::path::{Path, PathBuf};
use ts_rs::TS;

use corpuswright_core::analysis::{
    AnalysisDocumentSelection, AnalysisInputSource, AnnotatedFrequencyMode,
    AnnotatedFrequencyQuery, AnnotatedFrequencyResult, AnnotatedTermDistributionQuery,
    AnnotatedTermSelector, AnnotationCacheAvailability, AnnotationCacheBuildError,
    AnnotationCacheBuildPhase, AnnotationCacheBuildProgress, AnnotationCacheBuildReport,
    AnnotationCacheState, CompareDocumentCell, CompareDocumentRow, CompareExpressionSummary,
    CompareExpressionsQuery, CompareExpressionsResult, ConcordanceCollocatePositionCounts,
    ConcordanceCollocateRow, ConcordanceCollocationDirection, ConcordanceCollocationMeasure,
    ConcordanceCollocationQuery, ConcordanceCollocationResult, ConcordanceCollocationSortKey,
    ConcordanceContextFilterDirection, ConcordanceDistributionItem, ConcordanceDistributionPage,
    ConcordanceExpansionQuery, ConcordanceExpansionResult, ConcordanceQuery, ConcordanceResult,
    ConcordanceRow, ConcordanceSortKey, CorpusOverview, CorpusOverviewBalance, CorpusOverviewBuild,
    CorpusOverviewDispersion, CorpusOverviewDispersionPlane, CorpusOverviewDocument,
    CorpusOverviewExtractionQuality, CorpusOverviewFinding, CorpusOverviewFindingKind,
    CorpusOverviewFindingSeverity, CorpusOverviewFrequencyConcentration,
    CorpusOverviewLexicalSpectrum, CorpusOverviewMattrDistribution, CorpusOverviewPopulation,
    CorpusOverviewRankFrequency, CorpusOverviewReading, CorpusOverviewTotals,
    CorpusOverviewVocabularyGrowth, CoverageTargetCrossing, CumulativeCoveragePoint,
    DispersionProfileBucket, DispersionRosterEntry, DispersionWitness, DispersionWitnessKind,
    FrequencyGroupMode, FrequencyMatchMode, FrequencyNotice, FrequencyNoticeKind, FrequencyPage,
    FrequencyQuery, FrequencySortKey, FrequencySpectrumPoint, FrequencyTermItem,
    FrequencyTokenClass, LengthHistogramBin, LexFrequencyRow, MattrWitness,
    OverviewDistributionBin, OverviewDistributionEvidencePage, OverviewDistributionEvidenceQuery,
    OverviewDistributionEvidenceRow, OverviewDistributionMetric, OverviewDistributionQuery,
    OverviewDistributionResult, OverviewDistributionScale, OverviewDistributionWitness,
    OverviewEcdfPoint, OverviewHistogramRule, OverviewQqPoint, OverviewQqReference,
    PosFrequencyRow, RankFrequencyPoint, SortDirection, TermDistributionItem, TermDistributionPage,
    UniversalType, VocabularyGrowthPoint,
};
use corpuswright_core::clean::{
    CleaningConfig, CleaningCount, CleaningOperation, CleaningReport, PdfEmbeddedTextStrategy,
    PdfOcrQuality, PdfTextSource, ReasonCount, RemovalMatcher, RemovalRule, RemovalRuleSource,
    RemovalScope, RemovalSummary, RemovedTextExample, ReplacementRule, SkippedRuleNotice,
    TableExtractionStrategy,
};
use corpuswright_core::decode::{DecodeStrategy, SourceEncoding};
use corpuswright_core::export::{ExportReport, ExportWarning, ExportWarningKind};
use corpuswright_core::language::{
    DetectedLanguage, DocumentLanguage, LanguageResolution, ProjectLanguage,
};
use corpuswright_core::manifest::{ExportManifest, ManifestFileRecord};
use corpuswright_core::pdf::{PdfPageExtractionMethod, PdfPageRangePage, PdfPageRangeResult};
use corpuswright_core::pdf_audit::{PdfAuditQuality, PdfAuditResult, PdfAuditSuggestedProfile};
use corpuswright_core::preview::{
    CombinedPreview, FilePreview, PreviewWarning, PreviewWarningKind,
};
use corpuswright_core::project::{ActiveProject, CreateCorpusProjectRequest};
use corpuswright_core::project_sources::{
    ProjectSource, ProjectSourceExtractionRoute, ProjectSourceImportPolicy,
    ProjectSourceImportReview, ProjectSourceKind, ProjectSourceKindCounts, ProjectSourceManifest,
    ProjectSourcePage, ProjectSourcePdfProfile, ProjectSourcePreviewItem,
    ProjectSourcePreviewViewport, ProjectSourceSelectionDescriptor, ProjectSourceSelectionViewport,
    ProjectSourceStatus, ProjectSourceStatusCounts, ProjectSourceSummary, ProjectSourceUpdate,
};
use corpuswright_core::repeated_artifact_promotion::{
    RepeatedArtifactMatcherKind, RepeatedArtifactPromotion,
};
use corpuswright_core::repeated_artifacts::{
    ArtifactRiskLabel, CandidateContentClass, CandidateTextProfile, CandidateTextSignalLabel,
    PositionSummary, RepeatedArtifactCandidate, RepeatedArtifactExample, RepeatedArtifactKind,
    RepeatedArtifactMemoryStage, RepeatedArtifactScanConfig, RepeatedArtifactScanDiagnostics,
    RepeatedArtifactScanError, RepeatedArtifactScanPhase, RepeatedArtifactScanProgress,
    RepeatedArtifactScanReport,
};
use corpuswright_core::scan::{
    CorpusSummary, DocumentRecord, DocumentType, DocumentTypeCounts, ScanReport,
};
use corpuswright_core::search::{SearchHit, SearchResult};
use corpuswright_core::tokenization::{
    TokenizationConfig, TokenizationManifest, UnicodeNormalizationForm,
};

struct TypeImport {
    name: &'static str,
    file: &'static str,
}

fn trim_trailing_whitespace(input: &str) -> String {
    input
        .lines()
        .map(str::trim_end)
        .collect::<Vec<_>>()
        .join("\n")
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
    content.push_str(&format!(
        "export {}\n",
        trim_trailing_whitespace(T::decl().trim_end())
    ));

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
    export_type::<RemovalRuleSource>(&out_dir, &[], &[]);
    export_type::<RemovalScope>(&out_dir, &[], &[]);
    export_type::<RemovalMatcher>(&out_dir, &[], &[]);
    export_type::<RemovalRule>(
        &out_dir,
        &[
            TypeImport {
                name: "RemovalRuleSource",
                file: "RemovalRuleSource",
            },
            TypeImport {
                name: "RemovalMatcher",
                file: "RemovalMatcher",
            },
            TypeImport {
                name: "RemovalScope",
                file: "RemovalScope",
            },
        ],
        &[],
    );
    export_type::<TableExtractionStrategy>(&out_dir, &[], &[]);
    export_type::<PdfEmbeddedTextStrategy>(&out_dir, &[], &[]);
    export_type::<PdfTextSource>(&out_dir, &[], &[]);
    export_type::<PdfOcrQuality>(&out_dir, &[], &[]);
    export_type::<PdfPageExtractionMethod>(&out_dir, &[], &[]);
    export_type::<PdfPageRangePage>(
        &out_dir,
        &[TypeImport {
            name: "PdfPageExtractionMethod",
            file: "PdfPageExtractionMethod",
        }],
        &[],
    );
    export_type::<PdfPageRangeResult>(
        &out_dir,
        &[TypeImport {
            name: "PdfPageRangePage",
            file: "PdfPageRangePage",
        }],
        &[],
    );
    export_type::<PdfAuditQuality>(&out_dir, &[], &[]);
    export_type::<PdfAuditSuggestedProfile>(&out_dir, &[], &[]);
    export_type::<PdfAuditResult>(
        &out_dir,
        &[
            TypeImport {
                name: "PdfAuditQuality",
                file: "PdfAuditQuality",
            },
            TypeImport {
                name: "PdfAuditSuggestedProfile",
                file: "PdfAuditSuggestedProfile",
            },
        ],
        &[],
    );
    export_type::<CleaningConfig>(
        &out_dir,
        &[
            TypeImport {
                name: "ReplacementRule",
                file: "ReplacementRule",
            },
            TypeImport {
                name: "RemovalRule",
                file: "RemovalRule",
            },
            TypeImport {
                name: "TableExtractionStrategy",
                file: "TableExtractionStrategy",
            },
            TypeImport {
                name: "PdfEmbeddedTextStrategy",
                file: "PdfEmbeddedTextStrategy",
            },
            TypeImport {
                name: "PdfTextSource",
                file: "PdfTextSource",
            },
            TypeImport {
                name: "PdfOcrQuality",
                file: "PdfOcrQuality",
            },
        ],
        &[
            "ReplacementRule",
            "RemovalRule",
            "TableExtractionStrategy",
            "PdfEmbeddedTextStrategy",
            "PdfTextSource",
            "PdfOcrQuality",
        ],
    );

    export_type::<DocumentType>(&out_dir, &[], &[]);
    export_type::<DocumentRecord>(
        &out_dir,
        &[
            TypeImport {
                name: "DocumentType",
                file: "DocumentType",
            },
            TypeImport {
                name: "ProjectSourceExtractionRoute",
                file: "ProjectSourceExtractionRoute",
            },
            TypeImport {
                name: "ProjectSourcePdfProfile",
                file: "ProjectSourcePdfProfile",
            },
            TypeImport {
                name: "PdfOcrQuality",
                file: "PdfOcrQuality",
            },
        ],
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

    export_type::<DetectedLanguage>(&out_dir, &[], &[]);
    export_type::<ProjectLanguage>(&out_dir, &[], &[]);
    export_type::<LanguageResolution>(&out_dir, &[], &[]);
    export_type::<DocumentLanguage>(
        &out_dir,
        &[
            TypeImport {
                name: "DetectedLanguage",
                file: "DetectedLanguage",
            },
            TypeImport {
                name: "LanguageResolution",
                file: "LanguageResolution",
            },
        ],
        &[],
    );
    export_type::<CreateCorpusProjectRequest>(
        &out_dir,
        &[TypeImport {
            name: "ProjectLanguage",
            file: "ProjectLanguage",
        }],
        &[],
    );
    export_type::<ActiveProject>(
        &out_dir,
        &[TypeImport {
            name: "ProjectLanguage",
            file: "ProjectLanguage",
        }],
        &[],
    );

    export_type::<ProjectSourceKind>(&out_dir, &[], &[]);
    export_type::<ProjectSourceStatus>(&out_dir, &[], &[]);
    export_type::<ProjectSourceExtractionRoute>(&out_dir, &[], &[]);
    export_type::<ProjectSourceImportPolicy>(&out_dir, &[], &[]);
    export_type::<ProjectSourcePdfProfile>(&out_dir, &[], &[]);
    export_type::<ProjectSource>(
        &out_dir,
        &[
            TypeImport {
                name: "ProjectSourceKind",
                file: "ProjectSourceKind",
            },
            TypeImport {
                name: "ProjectSourceStatus",
                file: "ProjectSourceStatus",
            },
            TypeImport {
                name: "ProjectSourceExtractionRoute",
                file: "ProjectSourceExtractionRoute",
            },
            TypeImport {
                name: "PdfOcrQuality",
                file: "PdfOcrQuality",
            },
            TypeImport {
                name: "ProjectSourcePdfProfile",
                file: "ProjectSourcePdfProfile",
            },
        ],
        &[],
    );
    export_type::<ProjectSourceManifest>(
        &out_dir,
        &[TypeImport {
            name: "ProjectSource",
            file: "ProjectSource",
        }],
        &[],
    );
    export_type::<ProjectSourceStatusCounts>(&out_dir, &[], &[]);
    export_type::<ProjectSourceKindCounts>(&out_dir, &[], &[]);
    export_type::<ProjectSourceSummary>(
        &out_dir,
        &[
            TypeImport {
                name: "ProjectSourceStatusCounts",
                file: "ProjectSourceStatusCounts",
            },
            TypeImport {
                name: "ProjectSourceKindCounts",
                file: "ProjectSourceKindCounts",
            },
        ],
        &[],
    );
    export_type::<ProjectSourcePage>(
        &out_dir,
        &[
            TypeImport {
                name: "ProjectSource",
                file: "ProjectSource",
            },
            TypeImport {
                name: "ProjectSourceSummary",
                file: "ProjectSourceSummary",
            },
        ],
        &[],
    );
    export_type::<ProjectSourceSelectionDescriptor>(&out_dir, &[], &[]);
    export_type::<ProjectSourceSelectionViewport>(
        &out_dir,
        &[TypeImport {
            name: "ProjectSource",
            file: "ProjectSource",
        }],
        &[],
    );
    export_type::<ProjectSourcePreviewItem>(
        &out_dir,
        &[
            TypeImport {
                name: "ProjectSource",
                file: "ProjectSource",
            },
            TypeImport {
                name: "FilePreview",
                file: "FilePreview",
            },
        ],
        &[],
    );
    export_type::<ProjectSourcePreviewViewport>(
        &out_dir,
        &[TypeImport {
            name: "ProjectSourcePreviewItem",
            file: "ProjectSourcePreviewItem",
        }],
        &[],
    );
    export_type::<ProjectSourceUpdate>(
        &out_dir,
        &[TypeImport {
            name: "ProjectSourceManifest",
            file: "ProjectSourceManifest",
        }],
        &[],
    );
    export_type::<ProjectSourceImportReview>(
        &out_dir,
        &[TypeImport {
            name: "ProjectSource",
            file: "ProjectSource",
        }],
        &[],
    );

    export_type::<CleaningOperation>(&out_dir, &[], &[]);
    export_type::<RemovedTextExample>(&out_dir, &[], &[]);
    export_type::<RemovalSummary>(
        &out_dir,
        &[
            TypeImport {
                name: "CleaningOperation",
                file: "CleaningOperation",
            },
            TypeImport {
                name: "RemovalRuleSource",
                file: "RemovalRuleSource",
            },
            TypeImport {
                name: "RemovedTextExample",
                file: "RemovedTextExample",
            },
        ],
        &[],
    );
    export_type::<SkippedRuleNotice>(
        &out_dir,
        &[TypeImport {
            name: "RemovalRuleSource",
            file: "RemovalRuleSource",
        }],
        &[],
    );
    export_type::<CleaningCount>(
        &out_dir,
        &[TypeImport {
            name: "CleaningOperation",
            file: "CleaningOperation",
        }],
        &[],
    );
    export_type::<ReasonCount>(&out_dir, &[], &[]);
    export_type::<CleaningReport>(
        &out_dir,
        &[
            TypeImport {
                name: "RemovalSummary",
                file: "RemovalSummary",
            },
            TypeImport {
                name: "SkippedRuleNotice",
                file: "SkippedRuleNotice",
            },
            TypeImport {
                name: "CleaningCount",
                file: "CleaningCount",
            },
            TypeImport {
                name: "ReasonCount",
                file: "ReasonCount",
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
            TypeImport {
                name: "CleaningReport",
                file: "CleaningReport",
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
            TypeImport {
                name: "CleaningReport",
                file: "CleaningReport",
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
    export_type::<SourceEncoding>(&out_dir, &[], &[]);
    export_type::<DecodeStrategy>(&out_dir, &[], &[]);
    export_type::<ManifestFileRecord>(
        &out_dir,
        &[
            TypeImport {
                name: "DocumentType",
                file: "DocumentType",
            },
            TypeImport {
                name: "SourceEncoding",
                file: "SourceEncoding",
            },
            TypeImport {
                name: "DecodeStrategy",
                file: "DecodeStrategy",
            },
        ],
        &[],
    );
    export_type::<UnicodeNormalizationForm>(&out_dir, &[], &[]);
    export_type::<TokenizationConfig>(
        &out_dir,
        &[TypeImport {
            name: "UnicodeNormalizationForm",
            file: "UnicodeNormalizationForm",
        }],
        &[],
    );
    export_type::<TokenizationManifest>(
        &out_dir,
        &[TypeImport {
            name: "TokenizationConfig",
            file: "TokenizationConfig",
        }],
        &[],
    );
    export_type::<AnalysisDocumentSelection>(&out_dir, &[], &[]);
    export_type::<OverviewDistributionMetric>(&out_dir, &[], &[]);
    export_type::<OverviewHistogramRule>(&out_dir, &[], &[]);
    export_type::<OverviewDistributionScale>(&out_dir, &[], &[]);
    export_type::<OverviewQqReference>(&out_dir, &[], &[]);
    export_type::<OverviewDistributionQuery>(
        &out_dir,
        &[
            TypeImport {
                name: "OverviewDistributionMetric",
                file: "OverviewDistributionMetric",
            },
            TypeImport {
                name: "OverviewHistogramRule",
                file: "OverviewHistogramRule",
            },
            TypeImport {
                name: "OverviewDistributionScale",
                file: "OverviewDistributionScale",
            },
            TypeImport {
                name: "OverviewQqReference",
                file: "OverviewQqReference",
            },
        ],
        &[],
    );
    export_type::<OverviewDistributionBin>(&out_dir, &[], &[]);
    export_type::<OverviewEcdfPoint>(&out_dir, &[], &[]);
    export_type::<OverviewQqPoint>(&out_dir, &[], &[]);
    export_type::<OverviewDistributionWitness>(&out_dir, &[], &[]);
    export_type::<OverviewDistributionResult>(
        &out_dir,
        &[
            TypeImport {
                name: "OverviewDistributionMetric",
                file: "OverviewDistributionMetric",
            },
            TypeImport {
                name: "OverviewDistributionScale",
                file: "OverviewDistributionScale",
            },
            TypeImport {
                name: "OverviewHistogramRule",
                file: "OverviewHistogramRule",
            },
            TypeImport {
                name: "OverviewDistributionBin",
                file: "OverviewDistributionBin",
            },
            TypeImport {
                name: "OverviewEcdfPoint",
                file: "OverviewEcdfPoint",
            },
            TypeImport {
                name: "OverviewQqReference",
                file: "OverviewQqReference",
            },
            TypeImport {
                name: "OverviewQqPoint",
                file: "OverviewQqPoint",
            },
            TypeImport {
                name: "OverviewDistributionWitness",
                file: "OverviewDistributionWitness",
            },
        ],
        &[],
    );
    export_type::<OverviewDistributionEvidenceQuery>(
        &out_dir,
        &[TypeImport {
            name: "OverviewDistributionMetric",
            file: "OverviewDistributionMetric",
        }],
        &[],
    );
    export_type::<OverviewDistributionEvidenceRow>(&out_dir, &[], &[]);
    export_type::<OverviewDistributionEvidencePage>(
        &out_dir,
        &[TypeImport {
            name: "OverviewDistributionEvidenceRow",
            file: "OverviewDistributionEvidenceRow",
        }],
        &[],
    );
    export_type::<AnalysisInputSource>(&out_dir, &[], &[]);
    export_type::<CorpusOverviewTotals>(&out_dir, &[], &[]);
    export_type::<CorpusOverviewPopulation>(&out_dir, &[], &[]);
    export_type::<CorpusOverviewDocument>(&out_dir, &[], &[]);
    export_type::<LengthHistogramBin>(&out_dir, &[], &[]);
    export_type::<CorpusOverviewBalance>(
        &out_dir,
        &[TypeImport {
            name: "LengthHistogramBin",
            file: "LengthHistogramBin",
        }],
        &[],
    );
    export_type::<CorpusOverviewExtractionQuality>(&out_dir, &[], &[]);
    export_type::<FrequencySpectrumPoint>(&out_dir, &[], &[]);
    export_type::<MattrWitness>(&out_dir, &[], &[]);
    export_type::<CorpusOverviewMattrDistribution>(
        &out_dir,
        &[TypeImport {
            name: "MattrWitness",
            file: "MattrWitness",
        }],
        &[],
    );
    export_type::<CorpusOverviewLexicalSpectrum>(
        &out_dir,
        &[
            TypeImport {
                name: "FrequencySpectrumPoint",
                file: "FrequencySpectrumPoint",
            },
            TypeImport {
                name: "CorpusOverviewMattrDistribution",
                file: "CorpusOverviewMattrDistribution",
            },
        ],
        &[],
    );
    export_type::<CumulativeCoveragePoint>(&out_dir, &[], &[]);
    export_type::<CoverageTargetCrossing>(&out_dir, &[], &[]);
    export_type::<CorpusOverviewFrequencyConcentration>(
        &out_dir,
        &[
            TypeImport {
                name: "CumulativeCoveragePoint",
                file: "CumulativeCoveragePoint",
            },
            TypeImport {
                name: "CoverageTargetCrossing",
                file: "CoverageTargetCrossing",
            },
        ],
        &[],
    );
    export_type::<RankFrequencyPoint>(&out_dir, &[], &[]);
    export_type::<CorpusOverviewRankFrequency>(
        &out_dir,
        &[TypeImport {
            name: "RankFrequencyPoint",
            file: "RankFrequencyPoint",
        }],
        &[],
    );
    export_type::<VocabularyGrowthPoint>(&out_dir, &[], &[]);
    export_type::<CorpusOverviewVocabularyGrowth>(
        &out_dir,
        &[TypeImport {
            name: "VocabularyGrowthPoint",
            file: "VocabularyGrowthPoint",
        }],
        &[],
    );
    export_type::<DispersionProfileBucket>(&out_dir, &[], &[]);
    export_type::<DispersionWitnessKind>(&out_dir, &[], &[]);
    export_type::<DispersionWitness>(
        &out_dir,
        &[TypeImport {
            name: "DispersionWitnessKind",
            file: "DispersionWitnessKind",
        }],
        &[],
    );
    export_type::<CorpusOverviewDispersionPlane>(
        &out_dir,
        &[TypeImport {
            name: "DispersionWitness",
            file: "DispersionWitness",
        }],
        &[],
    );
    export_type::<DispersionRosterEntry>(&out_dir, &[], &[]);
    export_type::<UniversalType>(&out_dir, &[], &[]);
    export_type::<CorpusOverviewDispersion>(
        &out_dir,
        &[
            TypeImport {
                name: "DispersionProfileBucket",
                file: "DispersionProfileBucket",
            },
            TypeImport {
                name: "CorpusOverviewDispersionPlane",
                file: "CorpusOverviewDispersionPlane",
            },
            TypeImport {
                name: "DispersionRosterEntry",
                file: "DispersionRosterEntry",
            },
            TypeImport {
                name: "UniversalType",
                file: "UniversalType",
            },
        ],
        &[],
    );
    export_type::<CorpusOverviewFindingKind>(&out_dir, &[], &[]);
    export_type::<CorpusOverviewFindingSeverity>(&out_dir, &[], &[]);
    export_type::<CorpusOverviewFinding>(
        &out_dir,
        &[
            TypeImport {
                name: "CorpusOverviewFindingKind",
                file: "CorpusOverviewFindingKind",
            },
            TypeImport {
                name: "CorpusOverviewFindingSeverity",
                file: "CorpusOverviewFindingSeverity",
            },
        ],
        &[],
    );
    export_type::<CorpusOverviewReading>(
        &out_dir,
        &[TypeImport {
            name: "CorpusOverviewFinding",
            file: "CorpusOverviewFinding",
        }],
        &[],
    );
    export_type::<CorpusOverviewBuild>(&out_dir, &[], &[]);
    let overview_imports = [
        TypeImport {
            name: "AnalysisInputSource",
            file: "AnalysisInputSource",
        },
        TypeImport {
            name: "CorpusOverviewTotals",
            file: "CorpusOverviewTotals",
        },
        TypeImport {
            name: "CorpusOverviewPopulation",
            file: "CorpusOverviewPopulation",
        },
        TypeImport {
            name: "CorpusOverviewDocument",
            file: "CorpusOverviewDocument",
        },
        TypeImport {
            name: "CorpusOverviewBalance",
            file: "CorpusOverviewBalance",
        },
        TypeImport {
            name: "LengthHistogramBin",
            file: "LengthHistogramBin",
        },
        TypeImport {
            name: "CorpusOverviewExtractionQuality",
            file: "CorpusOverviewExtractionQuality",
        },
        TypeImport {
            name: "CorpusOverviewLexicalSpectrum",
            file: "CorpusOverviewLexicalSpectrum",
        },
        TypeImport {
            name: "MattrWitness",
            file: "MattrWitness",
        },
        TypeImport {
            name: "CorpusOverviewMattrDistribution",
            file: "CorpusOverviewMattrDistribution",
        },
        TypeImport {
            name: "CorpusOverviewFrequencyConcentration",
            file: "CorpusOverviewFrequencyConcentration",
        },
        TypeImport {
            name: "CumulativeCoveragePoint",
            file: "CumulativeCoveragePoint",
        },
        TypeImport {
            name: "CoverageTargetCrossing",
            file: "CoverageTargetCrossing",
        },
        TypeImport {
            name: "FrequencySpectrumPoint",
            file: "FrequencySpectrumPoint",
        },
        TypeImport {
            name: "RankFrequencyPoint",
            file: "RankFrequencyPoint",
        },
        TypeImport {
            name: "CorpusOverviewRankFrequency",
            file: "CorpusOverviewRankFrequency",
        },
        TypeImport {
            name: "VocabularyGrowthPoint",
            file: "VocabularyGrowthPoint",
        },
        TypeImport {
            name: "CorpusOverviewVocabularyGrowth",
            file: "CorpusOverviewVocabularyGrowth",
        },
        TypeImport {
            name: "CorpusOverviewDispersion",
            file: "CorpusOverviewDispersion",
        },
        TypeImport {
            name: "DispersionProfileBucket",
            file: "DispersionProfileBucket",
        },
        TypeImport {
            name: "DispersionWitnessKind",
            file: "DispersionWitnessKind",
        },
        TypeImport {
            name: "DispersionWitness",
            file: "DispersionWitness",
        },
        TypeImport {
            name: "CorpusOverviewDispersionPlane",
            file: "CorpusOverviewDispersionPlane",
        },
        TypeImport {
            name: "DispersionRosterEntry",
            file: "DispersionRosterEntry",
        },
        TypeImport {
            name: "UniversalType",
            file: "UniversalType",
        },
        TypeImport {
            name: "CorpusOverviewFindingKind",
            file: "CorpusOverviewFindingKind",
        },
        TypeImport {
            name: "CorpusOverviewFindingSeverity",
            file: "CorpusOverviewFindingSeverity",
        },
        TypeImport {
            name: "CorpusOverviewFinding",
            file: "CorpusOverviewFinding",
        },
        TypeImport {
            name: "CorpusOverviewReading",
            file: "CorpusOverviewReading",
        },
        TypeImport {
            name: "CorpusOverviewBuild",
            file: "CorpusOverviewBuild",
        },
    ];
    let overview_re_exports = overview_imports
        .iter()
        .map(|overview_import| overview_import.name)
        .collect::<Vec<_>>();
    export_type::<CorpusOverview>(&out_dir, &overview_imports, &overview_re_exports);
    export_type::<FrequencyMatchMode>(&out_dir, &[], &[]);
    export_type::<FrequencyTokenClass>(&out_dir, &[], &[]);
    export_type::<FrequencySortKey>(&out_dir, &[], &[]);
    export_type::<SortDirection>(&out_dir, &[], &[]);
    export_type::<FrequencyGroupMode>(&out_dir, &[], &[]);
    export_type::<FrequencyNoticeKind>(&out_dir, &[], &[]);
    export_type::<FrequencyNotice>(
        &out_dir,
        &[TypeImport {
            name: "FrequencyNoticeKind",
            file: "FrequencyNoticeKind",
        }],
        &[],
    );
    export_type::<FrequencyQuery>(
        &out_dir,
        &[
            TypeImport {
                name: "AnalysisDocumentSelection",
                file: "AnalysisDocumentSelection",
            },
            TypeImport {
                name: "FrequencyMatchMode",
                file: "FrequencyMatchMode",
            },
            TypeImport {
                name: "FrequencyTokenClass",
                file: "FrequencyTokenClass",
            },
            TypeImport {
                name: "FrequencyGroupMode",
                file: "FrequencyGroupMode",
            },
            TypeImport {
                name: "FrequencySortKey",
                file: "FrequencySortKey",
            },
            TypeImport {
                name: "SortDirection",
                file: "SortDirection",
            },
        ],
        &[],
    );
    export_type::<FrequencyTermItem>(&out_dir, &[], &[]);
    export_type::<FrequencyPage>(
        &out_dir,
        &[
            TypeImport {
                name: "FrequencyTermItem",
                file: "FrequencyTermItem",
            },
            TypeImport {
                name: "FrequencyNotice",
                file: "FrequencyNotice",
            },
        ],
        &[],
    );
    export_type::<AnnotatedFrequencyMode>(&out_dir, &[], &[]);
    export_type::<AnnotationCacheState>(&out_dir, &[], &[]);
    export_type::<AnnotationCacheAvailability>(
        &out_dir,
        &[TypeImport {
            name: "AnnotationCacheState",
            file: "AnnotationCacheState",
        }],
        &[],
    );
    export_type::<AnnotatedFrequencyQuery>(
        &out_dir,
        &[
            TypeImport {
                name: "AnalysisDocumentSelection",
                file: "AnalysisDocumentSelection",
            },
            TypeImport {
                name: "AnnotatedFrequencyMode",
                file: "AnnotatedFrequencyMode",
            },
            TypeImport {
                name: "FrequencyMatchMode",
                file: "FrequencyMatchMode",
            },
            TypeImport {
                name: "FrequencySortKey",
                file: "FrequencySortKey",
            },
            TypeImport {
                name: "SortDirection",
                file: "SortDirection",
            },
        ],
        &[],
    );
    export_type::<AnnotatedTermSelector>(&out_dir, &[], &[]);
    export_type::<AnnotatedTermDistributionQuery>(
        &out_dir,
        &[
            TypeImport {
                name: "AnalysisDocumentSelection",
                file: "AnalysisDocumentSelection",
            },
            TypeImport {
                name: "AnnotatedTermSelector",
                file: "AnnotatedTermSelector",
            },
        ],
        &[],
    );
    export_type::<PosFrequencyRow>(&out_dir, &[], &[]);
    export_type::<LexFrequencyRow>(&out_dir, &[], &[]);
    export_type::<AnnotatedFrequencyResult>(
        &out_dir,
        &[
            TypeImport {
                name: "AnnotatedFrequencyMode",
                file: "AnnotatedFrequencyMode",
            },
            TypeImport {
                name: "AnnotationCacheAvailability",
                file: "AnnotationCacheAvailability",
            },
            TypeImport {
                name: "PosFrequencyRow",
                file: "PosFrequencyRow",
            },
            TypeImport {
                name: "LexFrequencyRow",
                file: "LexFrequencyRow",
            },
        ],
        &[],
    );
    export_type::<TermDistributionItem>(&out_dir, &[], &[]);
    export_type::<TermDistributionPage>(
        &out_dir,
        &[TypeImport {
            name: "TermDistributionItem",
            file: "TermDistributionItem",
        }],
        &[],
    );
    export_type::<corpuswright_core::analysis::ConcordancePhase>(&out_dir, &[], &[]);
    export_type::<corpuswright_core::analysis::ConcordanceProgress>(
        &out_dir,
        &[TypeImport {
            name: "ConcordancePhase",
            file: "ConcordancePhase",
        }],
        &[],
    );
    export_type::<ConcordanceSortKey>(&out_dir, &[], &[]);
    export_type::<ConcordanceContextFilterDirection>(&out_dir, &[], &[]);
    export_type::<ConcordanceQuery>(
        &out_dir,
        &[
            TypeImport {
                name: "AnalysisDocumentSelection",
                file: "AnalysisDocumentSelection",
            },
            TypeImport {
                name: "ConcordanceContextFilterDirection",
                file: "ConcordanceContextFilterDirection",
            },
            TypeImport {
                name: "ConcordanceSortKey",
                file: "ConcordanceSortKey",
            },
        ],
        &[],
    );
    export_type::<ConcordanceRow>(&out_dir, &[], &[]);
    export_type::<ConcordanceExpansionQuery>(&out_dir, &[], &[]);
    export_type::<ConcordanceExpansionResult>(&out_dir, &[], &[]);
    export_type::<ConcordanceDistributionItem>(&out_dir, &[], &[]);
    export_type::<ConcordanceDistributionPage>(
        &out_dir,
        &[TypeImport {
            name: "ConcordanceDistributionItem",
            file: "ConcordanceDistributionItem",
        }],
        &[],
    );
    export_type::<ConcordanceResult>(
        &out_dir,
        &[
            TypeImport {
                name: "ConcordanceSortKey",
                file: "ConcordanceSortKey",
            },
            TypeImport {
                name: "ConcordanceRow",
                file: "ConcordanceRow",
            },
            TypeImport {
                name: "ConcordanceDistributionPage",
                file: "ConcordanceDistributionPage",
            },
        ],
        &[],
    );
    export_type::<CompareDocumentCell>(&out_dir, &[], &[]);
    export_type::<CompareDocumentRow>(
        &out_dir,
        &[TypeImport {
            name: "CompareDocumentCell",
            file: "CompareDocumentCell",
        }],
        &[],
    );
    export_type::<CompareExpressionSummary>(&out_dir, &[], &[]);
    export_type::<CompareExpressionsQuery>(
        &out_dir,
        &[TypeImport {
            name: "AnalysisDocumentSelection",
            file: "AnalysisDocumentSelection",
        }],
        &[],
    );
    export_type::<CompareExpressionsResult>(
        &out_dir,
        &[
            TypeImport {
                name: "CompareExpressionSummary",
                file: "CompareExpressionSummary",
            },
            TypeImport {
                name: "CompareDocumentRow",
                file: "CompareDocumentRow",
            },
        ],
        &[],
    );
    export_type::<ConcordanceCollocationDirection>(&out_dir, &[], &[]);
    export_type::<ConcordanceCollocationMeasure>(&out_dir, &[], &[]);
    export_type::<ConcordanceCollocationSortKey>(&out_dir, &[], &[]);
    export_type::<ConcordanceCollocationQuery>(
        &out_dir,
        &[
            TypeImport {
                name: "AnalysisDocumentSelection",
                file: "AnalysisDocumentSelection",
            },
            TypeImport {
                name: "ConcordanceCollocationDirection",
                file: "ConcordanceCollocationDirection",
            },
            TypeImport {
                name: "ConcordanceCollocationMeasure",
                file: "ConcordanceCollocationMeasure",
            },
            TypeImport {
                name: "ConcordanceCollocationSortKey",
                file: "ConcordanceCollocationSortKey",
            },
            TypeImport {
                name: "SortDirection",
                file: "SortDirection",
            },
        ],
        &[],
    );
    export_type::<ConcordanceCollocatePositionCounts>(&out_dir, &[], &[]);
    export_type::<ConcordanceCollocateRow>(
        &out_dir,
        &[TypeImport {
            name: "ConcordanceCollocatePositionCounts",
            file: "ConcordanceCollocatePositionCounts",
        }],
        &[],
    );
    export_type::<ConcordanceCollocationResult>(
        &out_dir,
        &[
            TypeImport {
                name: "ConcordanceCollocationDirection",
                file: "ConcordanceCollocationDirection",
            },
            TypeImport {
                name: "ConcordanceCollocationMeasure",
                file: "ConcordanceCollocationMeasure",
            },
            TypeImport {
                name: "ConcordanceCollocationSortKey",
                file: "ConcordanceCollocationSortKey",
            },
            TypeImport {
                name: "SortDirection",
                file: "SortDirection",
            },
            TypeImport {
                name: "ConcordanceCollocateRow",
                file: "ConcordanceCollocateRow",
            },
        ],
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
            TypeImport {
                name: "TokenizationManifest",
                file: "TokenizationManifest",
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
    export_type::<RepeatedArtifactScanPhase>(&out_dir, &[], &[]);
    export_type::<RepeatedArtifactScanProgress>(
        &out_dir,
        &[TypeImport {
            name: "RepeatedArtifactScanPhase",
            file: "RepeatedArtifactScanPhase",
        }],
        &[],
    );
    export_type::<RepeatedArtifactMemoryStage>(&out_dir, &[], &[]);
    export_type::<RepeatedArtifactScanError>(
        &out_dir,
        &[TypeImport {
            name: "RepeatedArtifactMemoryStage",
            file: "RepeatedArtifactMemoryStage",
        }],
        &[],
    );
    export_type::<RepeatedArtifactKind>(&out_dir, &[], &[]);
    export_type::<ArtifactRiskLabel>(&out_dir, &[], &[]);
    export_type::<CandidateContentClass>(&out_dir, &[], &[]);
    export_type::<CandidateTextProfile>(&out_dir, &[], &[]);
    export_type::<CandidateTextSignalLabel>(&out_dir, &[], &[]);
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
                name: "CandidateTextProfile",
                file: "CandidateTextProfile",
            },
            TypeImport {
                name: "CandidateTextSignalLabel",
                file: "CandidateTextSignalLabel",
            },
            TypeImport {
                name: "RepeatedArtifactExample",
                file: "RepeatedArtifactExample",
            },
        ],
        &[],
    );
    export_type::<RepeatedArtifactMatcherKind>(&out_dir, &[], &[]);
    export_type::<RepeatedArtifactPromotion>(
        &out_dir,
        &[
            TypeImport {
                name: "RepeatedArtifactMatcherKind",
                file: "RepeatedArtifactMatcherKind",
            },
            TypeImport {
                name: "RemovalScope",
                file: "RemovalScope",
            },
            TypeImport {
                name: "RemovalRule",
                file: "RemovalRule",
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
            TypeImport {
                name: "RepeatedArtifactPromotion",
                file: "RepeatedArtifactPromotion",
            },
        ],
        &[],
    );

    export_type::<AnnotationCacheBuildPhase>(&out_dir, &[], &[]);
    export_type::<AnnotationCacheBuildProgress>(
        &out_dir,
        &[TypeImport {
            name: "AnnotationCacheBuildPhase",
            file: "AnnotationCacheBuildPhase",
        }],
        &[],
    );
    export_type::<AnnotationCacheBuildReport>(&out_dir, &[], &[]);
    export_type::<AnnotationCacheBuildError>(&out_dir, &[], &[]);

    eprintln!("Done.");
}
