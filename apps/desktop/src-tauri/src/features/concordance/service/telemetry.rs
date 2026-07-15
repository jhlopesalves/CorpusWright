//! Stable formatting primitives shared by Concordance route evidence.

use std::time::Duration;

pub(super) fn duration_ms(duration: Duration) -> u64 {
    duration.as_millis().try_into().unwrap_or(u64::MAX)
}

pub(super) fn log_string(value: &str, max_chars: usize) -> String {
    let compact = value.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut truncated = String::new();
    for (index, character) in compact.chars().enumerate() {
        if index >= max_chars {
            truncated.push_str("...");
            break;
        }
        truncated.push(if character.is_control() {
            ' '
        } else {
            character
        });
    }
    if truncated.is_empty() {
        truncated.push_str("<empty>");
    }
    serde_json::to_string(&truncated).unwrap_or_else(|_| "\"<unavailable>\"".to_string())
}
