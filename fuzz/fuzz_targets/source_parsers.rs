#![no_main]

use corpuswright_core::preparation::cleaning::CleaningConfig;
use corpuswright_core::sources::{decode::decode_source_bytes, docx::extract_docx, html::extract_html};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|bytes: &[u8]| {
    let decoded = decode_source_bytes(bytes);
    let _ = extract_html(&decoded.text);
    let _ = extract_docx(bytes, &CleaningConfig::default());
});
