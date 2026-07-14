use regex::Regex;
use serde::{Deserialize, Serialize};
use std::sync::LazyLock;

static MARKER: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?im)^\s*\*{3}\s*(START|END)\s+OF\s+(?:THIS|THE)\s+PROJECT\s+GUTENBERG\s+EBOOK\b[^\r\n]*?\*{3}\s*$",
    )
    .expect("Gutenberg marker regex must compile")
});

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum GutenbergMarkerKind {
    Start,
    End,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GutenbergMarker {
    pub kind: GutenbergMarkerKind,
    pub text: String,
    pub byte_start: usize,
    pub byte_end: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GutenbergCleanReport {
    pub start_marker: GutenbergMarker,
    pub end_marker: GutenbergMarker,
    pub removed_prefix_bytes: usize,
    pub removed_suffix_bytes: usize,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GutenbergCleanResult {
    pub text: String,
    pub report: GutenbergCleanReport,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum GutenbergCleanError {
    InvalidUtf8,
    MissingStartMarker,
    MissingEndMarker,
    AmbiguousStartMarkers { count: usize },
    AmbiguousEndMarkers { count: usize },
    InvalidMarkerOrder,
    RemainingBoilerplate { evidence: String },
}

fn normalize_line_endings(value: &str) -> String {
    value.replace("\r\n", "\n").replace('\r', "\n")
}

fn markers(text: &str) -> Vec<GutenbergMarker> {
    MARKER
        .captures_iter(text)
        .filter_map(|capture| {
            let matched = capture.get(0)?;
            let kind = match capture.get(1)?.as_str().to_ascii_uppercase().as_str() {
                "START" => GutenbergMarkerKind::Start,
                "END" => GutenbergMarkerKind::End,
                _ => return None,
            };
            Some(GutenbergMarker {
                kind,
                text: matched.as_str().trim().to_string(),
                byte_start: matched.start(),
                byte_end: matched.end(),
            })
        })
        .collect()
}

fn boundary_boilerplate(text: &str) -> Option<String> {
    let sample_bytes = 16_384;
    let leading_end = text
        .char_indices()
        .map(|(offset, _)| offset)
        .take_while(|offset| *offset <= sample_bytes)
        .last()
        .unwrap_or(text.len());
    let trailing_start = text
        .char_indices()
        .map(|(offset, _)| offset)
        .find(|offset| *offset >= text.len().saturating_sub(sample_bytes))
        .unwrap_or(0);
    let sample = format!("{}\n{}", &text[..leading_end], &text[trailing_start..]);
    let folded = sample.to_ascii_lowercase();
    [
        "project gutenberg license",
        "full project gutenberg",
        "gutenberg-tm",
        "gutenberg™",
    ]
    .into_iter()
    .find(|needle| folded.contains(needle))
    .map(str::to_string)
}

/// Removes only content outside one unambiguous pair of validated markers.
pub fn clean_gutenberg_text(raw: &[u8]) -> Result<GutenbergCleanResult, GutenbergCleanError> {
    let raw = std::str::from_utf8(raw).map_err(|_| GutenbergCleanError::InvalidUtf8)?;
    let without_bom = raw.strip_prefix('\u{feff}').unwrap_or(raw);
    let normalized = normalize_line_endings(without_bom);
    let all_markers = markers(&normalized);
    let starts = all_markers
        .iter()
        .filter(|marker| marker.kind == GutenbergMarkerKind::Start)
        .cloned()
        .collect::<Vec<_>>();
    let ends = all_markers
        .iter()
        .filter(|marker| marker.kind == GutenbergMarkerKind::End)
        .cloned()
        .collect::<Vec<_>>();
    let start = match starts.as_slice() {
        [] => return Err(GutenbergCleanError::MissingStartMarker),
        [start] => start.clone(),
        _ => {
            return Err(GutenbergCleanError::AmbiguousStartMarkers {
                count: starts.len(),
            });
        }
    };
    let end = match ends.as_slice() {
        [] => return Err(GutenbergCleanError::MissingEndMarker),
        [end] => end.clone(),
        _ => {
            return Err(GutenbergCleanError::AmbiguousEndMarkers { count: ends.len() });
        }
    };
    if start.byte_end >= end.byte_start {
        return Err(GutenbergCleanError::InvalidMarkerOrder);
    }
    let body = normalized[start.byte_end..end.byte_start].trim_end_matches('\n');
    let cleaned = format!("{body}\n");
    if let Some(evidence) = boundary_boilerplate(&cleaned) {
        return Err(GutenbergCleanError::RemainingBoilerplate { evidence });
    }
    let removed_prefix_bytes = start.byte_end;
    let removed_suffix_bytes = normalized.len().saturating_sub(end.byte_start);
    Ok(GutenbergCleanResult {
        text: cleaned,
        report: GutenbergCleanReport {
            start_marker: start,
            end_marker: end,
            removed_prefix_bytes,
            removed_suffix_bytes,
            warnings: Vec::new(),
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wrapped(start: &str, body: &str, end: &str) -> Vec<u8> {
        format!("licence\r\n{start}\r\n{body}\r\n{end}\r\ntrailer").into_bytes()
    }

    #[test]
    fn accepts_common_marker_spacing_variants_and_normalises_line_endings() {
        for start in [
            "*** START OF THE PROJECT GUTENBERG EBOOK EXAMPLE ***",
            "***START OF THE PROJECT GUTENBERG EBOOK EXAMPLE***",
            "*** START OF THIS PROJECT GUTENBERG EBOOK EXAMPLE ***",
        ] {
            let result = clean_gutenberg_text(&wrapped(
                start,
                "CHAPTER I\r\n\r\nBody text.[1]",
                "***END OF THE PROJECT GUTENBERG EBOOK EXAMPLE***",
            ))
            .unwrap();
            assert_eq!(result.text, "\nCHAPTER I\n\nBody text.[1]\n");
        }
    }

    #[test]
    fn refuses_missing_or_ambiguous_boundaries() {
        assert_eq!(
            clean_gutenberg_text(b"body\n*** END OF THE PROJECT GUTENBERG EBOOK X ***"),
            Err(GutenbergCleanError::MissingStartMarker)
        );
        assert_eq!(
            clean_gutenberg_text(b"*** START OF THE PROJECT GUTENBERG EBOOK X ***\nbody"),
            Err(GutenbergCleanError::MissingEndMarker)
        );
        let ambiguous = b"*** START OF THE PROJECT GUTENBERG EBOOK X ***\n*** START OF THE PROJECT GUTENBERG EBOOK X ***\nbody\n*** END OF THE PROJECT GUTENBERG EBOOK X ***";
        assert_eq!(
            clean_gutenberg_text(ambiguous),
            Err(GutenbergCleanError::AmbiguousStartMarkers { count: 2 })
        );
    }

    #[test]
    fn preserves_headings_paragraph_spacing_and_footnotes() {
        let result = clean_gutenberg_text(&wrapped(
            "*** START OF THE PROJECT GUTENBERG EBOOK X ***",
            "DEDICATION\r\n\r\nCHAPTER I\r\n\r\nLine one.\r\n\r\n[1] A footnote.",
            "*** END OF THE PROJECT GUTENBERG EBOOK X ***",
        ))
        .unwrap();
        assert!(result.text.contains("DEDICATION\n\nCHAPTER I"));
        assert!(result.text.contains("Line one.\n\n[1] A footnote."));
        assert!(result.text.ends_with('\n'));
        assert!(!result.text.ends_with("\n\n"));
    }

    #[test]
    fn rejects_remaining_licence_near_a_boundary() {
        let raw = wrapped(
            "*** START OF THE PROJECT GUTENBERG EBOOK X ***",
            "THE FULL PROJECT GUTENBERG LICENSE\nBody",
            "*** END OF THE PROJECT GUTENBERG EBOOK X ***",
        );
        assert!(matches!(
            clean_gutenberg_text(&raw),
            Err(GutenbergCleanError::RemainingBoilerplate { .. })
        ));
    }
}
