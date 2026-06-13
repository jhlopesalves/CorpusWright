#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExtractionQuality {
    Good,
    Suspicious,
    Poor,
    Empty,
}

#[derive(Debug, Clone)]
pub struct QualityMetrics {
    pub total_characters: usize,
    pub alphabetic_ratio: f64,
    pub whitespace_ratio: f64,
    pub symbol_ratio: f64,
    pub replacement_ratio: f64,
    pub word_like_count: usize,
    pub word_like_ratio: f64,
    pub average_token_length: f64,
}

pub fn evaluate(text: &str) -> (ExtractionQuality, QualityMetrics) {
    if text.is_empty() {
        return (
            ExtractionQuality::Empty,
            QualityMetrics {
                total_characters: 0,
                alphabetic_ratio: 0.0,
                whitespace_ratio: 0.0,
                symbol_ratio: 0.0,
                replacement_ratio: 0.0,
                word_like_count: 0,
                word_like_ratio: 0.0,
                average_token_length: 0.0,
            },
        );
    }

    let total_characters = text.chars().count();
    if total_characters == 0 {
        return (
            ExtractionQuality::Empty,
            QualityMetrics {
                total_characters: 0,
                alphabetic_ratio: 0.0,
                whitespace_ratio: 0.0,
                symbol_ratio: 0.0,
                replacement_ratio: 0.0,
                word_like_count: 0,
                word_like_ratio: 0.0,
                average_token_length: 0.0,
            },
        );
    }

    let mut alpha_count = 0;
    let mut space_count = 0;
    let mut symbol_count = 0;
    let mut replacement_count = 0;

    for c in text.chars() {
        if c.is_alphabetic() {
            alpha_count += 1;
        } else if c.is_whitespace() {
            space_count += 1;
        } else if c == '\u{FFFD}' {
            replacement_count += 1;
            symbol_count += 1; // Count replacement as a symbol too
        } else if c.is_ascii_punctuation() || c.is_ascii_graphic() {
            if !c.is_alphanumeric() {
                symbol_count += 1;
            }
        } else if !c.is_alphanumeric() && !c.is_control() {
            symbol_count += 1;
        }
    }

    let total_f = total_characters as f64;
    let alphabetic_ratio = alpha_count as f64 / total_f;
    let whitespace_ratio = space_count as f64 / total_f;
    let symbol_ratio = symbol_count as f64 / total_f;
    let replacement_ratio = replacement_count as f64 / total_f;

    let tokens: Vec<&str> = text.split_whitespace().collect();
    let total_tokens = tokens.len();

    let mut word_like_count = 0;
    let mut total_token_len = 0;

    for token in &tokens {
        let len = token.chars().count();
        total_token_len += len;

        let alphas = token.chars().filter(|c| c.is_alphabetic()).count();
        if len > 0 && alphas as f64 / len as f64 >= 0.5 {
            word_like_count += 1;
        }
    }

    let word_like_ratio = if total_tokens > 0 {
        word_like_count as f64 / total_tokens as f64
    } else {
        0.0
    };

    let average_token_length = if total_tokens > 0 {
        total_token_len as f64 / total_tokens as f64
    } else {
        0.0
    };

    let metrics = QualityMetrics {
        total_characters,
        alphabetic_ratio,
        whitespace_ratio,
        symbol_ratio,
        replacement_ratio,
        word_like_count,
        word_like_ratio,
        average_token_length,
    };

    let quality = if total_characters < 20 {
        if symbol_ratio > 0.5 || replacement_ratio > 0.1 {
            ExtractionQuality::Poor
        } else if word_like_ratio < 0.5 {
            ExtractionQuality::Suspicious
        } else {
            ExtractionQuality::Good
        }
    } else if replacement_ratio > 0.05
        || symbol_ratio > 0.3
        || word_like_ratio < 0.4
        || alphabetic_ratio < 0.3
        || average_token_length > 40.0
    {
        ExtractionQuality::Poor
    } else if symbol_ratio > 0.15
        || word_like_ratio < 0.6
        || alphabetic_ratio < 0.5
        || average_token_length > 20.0
    {
        ExtractionQuality::Suspicious
    } else {
        ExtractionQuality::Good
    };

    (quality, metrics)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normal_prose() {
        let text = "This is a normal scholarly paragraph. It has some punctuation, like commas, and periods! But overall it looks like perfectly fine English text.";
        let (quality, _metrics) = evaluate(text);
        assert_eq!(quality, ExtractionQuality::Good);
    }

    #[test]
    fn test_empty_string() {
        let text = "";
        let (quality, _metrics) = evaluate(text);
        assert_eq!(quality, ExtractionQuality::Empty);
    }

    #[test]
    fn test_symbol_heavy_garbage() {
        let text = "!\" #$%\n&\"’\"\n\"! () \" *\n(+ ,- .( /01( #";
        let (quality, _metrics) = evaluate(text);
        assert!(quality == ExtractionQuality::Poor || quality == ExtractionQuality::Suspicious);
    }

    #[test]
    fn test_replacement_characters() {
        let text = "This \u{FFFD} text \u{FFFD} has \u{FFFD} a \u{FFFD} lot \u{FFFD} of \u{FFFD} missing \u{FFFD} glyphs.";
        let (quality, _metrics) = evaluate(text);
        assert_eq!(quality, ExtractionQuality::Poor);
    }

    #[test]
    fn test_mixed_usable_text() {
        let text = "Figure 1. (!@#$%^&) Some good text interspersed with garbage. (A) *&^% (B)";
        let (quality, _metrics) = evaluate(text);
        assert!(quality == ExtractionQuality::Suspicious || quality == ExtractionQuality::Good);
    }
}
