//! Example binary to generate TypeScript type definitions from Rust structs/enums
//! for config-related types.
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

fn export_type<T: TS>(out_dir: &Path, add_export: bool) {
    let decl = T::decl();
    let content = if add_export {
        format!("export {}\n", decl.trim_end())
    } else {
        format!("{}\n", decl.trim_end())
    };
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

    // Export individual types with `export` keyword (ts-rs #[ts(export)] omits it)
    export_type::<ReplacementRule>(&out_dir, true);
    export_type::<TableExtractionStrategy>(&out_dir, true);
    export_type::<PdfEmbeddedTextStrategy>(&out_dir, true);

    // CleaningConfig needs imports and re-exports for its nested types.
    // Generate the raw declaration first, then wrap it.
    let cleaning_decl = CleaningConfig::decl();
    let combined = format!(
        r#"import type {{ ReplacementRule }} from "./ReplacementRule.js";
import type {{ TableExtractionStrategy }} from "./TableExtractionStrategy.js";
import type {{ PdfEmbeddedTextStrategy }} from "./PdfEmbeddedTextStrategy.js";

export type {{ ReplacementRule, TableExtractionStrategy, PdfEmbeddedTextStrategy }};

export {cleaning_decl}
"#,
    );
    let file_path = out_dir.join("CleaningConfig.ts");
    fs::write(&file_path, &combined)
        .unwrap_or_else(|e| panic!("failed to write {}: {}", file_path.display(), e));
    eprintln!("  wrote {}", file_path.display());

    eprintln!("Done.");
}
