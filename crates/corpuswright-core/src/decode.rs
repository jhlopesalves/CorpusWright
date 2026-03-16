use serde::{Deserialize, Serialize};
use ts_rs::TS;

pub const DECODE_POLICY_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum SourceEncoding {
    #[default]
    Utf8,
    Windows1252,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum DecodeStrategy {
    #[default]
    Utf8Strict,
    Windows1252Fallback,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodedText {
    pub text: String,
    pub detected_encoding: SourceEncoding,
    pub decode_strategy: DecodeStrategy,
    pub replacement_char_count: u32,
    pub had_utf8_error: bool,
}

pub fn decode_policy_version() -> u32 {
    DECODE_POLICY_VERSION
}

pub fn decode_source_bytes(bytes: &[u8]) -> DecodedText {
    match std::str::from_utf8(bytes) {
        Ok(text) => DecodedText {
            text: text.to_string(),
            detected_encoding: SourceEncoding::Utf8,
            decode_strategy: DecodeStrategy::Utf8Strict,
            replacement_char_count: 0,
            had_utf8_error: false,
        },
        Err(_) => DecodedText {
            text: decode_windows_1252_bytes(bytes),
            detected_encoding: SourceEncoding::Windows1252,
            decode_strategy: DecodeStrategy::Windows1252Fallback,
            replacement_char_count: 0,
            had_utf8_error: true,
        },
    }
}

pub(crate) fn decode_windows_1252_bytes(bytes: &[u8]) -> String {
    let (text, _, _) = encoding_rs::WINDOWS_1252.decode(bytes);
    text.into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_utf8_is_decoded_unchanged() {
        let text = "ação coração não café très ça";

        let decoded = decode_source_bytes(text.as_bytes());

        assert_eq!(decoded.text, text);
        assert_eq!(decoded.detected_encoding, SourceEncoding::Utf8);
        assert_eq!(decoded.decode_strategy, DecodeStrategy::Utf8Strict);
        assert_eq!(decoded.replacement_char_count, 0);
        assert!(!decoded.had_utf8_error);
    }

    #[test]
    fn windows_1252_recovers_portuguese_accents() {
        let bytes = [
            b'c', b'a', b'f', 0xe9, b' ', b'a', 0xe7, 0xe3, b'o', b' ', b'J', b'o', 0xe3, b'o',
            b' ', b'c', b'o', b'r', b'a', 0xe7, 0xe3, b'o', b' ', b'n', 0xe3, b'o', b' ', b'm',
            b'a', 0xe7, 0xe3,
        ];

        let decoded = decode_source_bytes(&bytes);

        assert_eq!(decoded.text, "café ação João coração não maçã");
        assert_eq!(decoded.detected_encoding, SourceEncoding::Windows1252);
        assert_eq!(decoded.decode_strategy, DecodeStrategy::Windows1252Fallback);
        assert_eq!(decoded.replacement_char_count, 0);
        assert!(decoded.had_utf8_error);
        assert!(!decoded.text.contains('\u{fffd}'));
    }

    #[test]
    fn windows_1252_recovers_french_accents() {
        let bytes = [
            b't', b'r', 0xe8, b's', b' ', b'c', b'a', b'f', 0xe9, b' ', b'g', b'a', b'r', 0xe7,
            b'o', b'n', b' ', b'd', 0xe9, b'j', 0xe0, b' ', 0xea, b't', b'r', b'e',
        ];

        let decoded = decode_source_bytes(&bytes);

        assert_eq!(decoded.text, "très café garçon déjà être");
        assert_eq!(decoded.detected_encoding, SourceEncoding::Windows1252);
        assert_eq!(decoded.decode_strategy, DecodeStrategy::Windows1252Fallback);
        assert_eq!(decoded.replacement_char_count, 0);
        assert!(decoded.had_utf8_error);
        assert!(!decoded.text.contains('\u{fffd}'));
    }

    #[test]
    fn windows_1252_uses_smart_punctuation_semantics() {
        let bytes = [
            0x93, b'H', b'e', b'l', b'l', b'o', 0x94, b' ', 0x91, b't', b'e', b's', b't', 0x92,
            b' ', 0x80, b' ', 0x96, b' ', 0x97,
        ];

        let decoded = decode_source_bytes(&bytes);

        assert_eq!(decoded.text, "“Hello” ‘test’ € – —");
        assert_eq!(decoded.detected_encoding, SourceEncoding::Windows1252);
        assert_eq!(decoded.decode_strategy, DecodeStrategy::Windows1252Fallback);
        assert_eq!(decoded.replacement_char_count, 0);
        assert!(decoded.had_utf8_error);
        assert!(!decoded.text.contains('\u{fffd}'));
    }
}
