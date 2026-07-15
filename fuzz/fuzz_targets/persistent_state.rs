#![no_main]

use corpuswright_core::analysis::concordance::index::fuzz_surface_index_cache;
use corpuswright_core::annotation::cache::fuzz_annotation_cache_envelope;
use corpuswright_core::preparation::cleaning::{CleaningConfig, RemovalRule};
use corpuswright_core::preparation::repeated_artifacts::RepeatedArtifactScanConfig;
use corpuswright_core::project::sources::ProjectSourceManifest;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|bytes: &[u8]| {
    let _ = serde_json::from_slice::<ProjectSourceManifest>(bytes);
    let _ = serde_json::from_slice::<CleaningConfig>(bytes);
    let _ = serde_json::from_slice::<Vec<RemovalRule>>(bytes);
    let _ = serde_json::from_slice::<RepeatedArtifactScanConfig>(bytes);
    fuzz_annotation_cache_envelope(bytes);
    fuzz_surface_index_cache(bytes);
});
