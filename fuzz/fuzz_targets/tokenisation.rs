#![no_main]

use corpuswright_core::text::tokenisation::{TokenizationConfig, tokenize};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|bytes: &[u8]| {
    let text = String::from_utf8_lossy(bytes);
    let _ = tokenize(&text, 0, &TokenizationConfig::default());
});
