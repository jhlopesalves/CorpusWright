//! Deterministic promotion of repeated-artefact candidates into removal rules.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::preparation::cleaning::{
    RemovalMatcher, RemovalRule, RemovalRuleSource, RemovalScope, is_valid_anywhere_literal_text,
};
use crate::preparation::repeated_artifacts::{RepeatedArtifactCandidate, RepeatedArtifactKind};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum RepeatedArtifactMatcherKind {
    Literal,
    NormalizedLine,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export)]
pub struct RepeatedArtifactPromotion {
    pub candidate_id: String,
    pub rule_text: String,
    pub matcher_kind: RepeatedArtifactMatcherKind,
    pub scope: RemovalScope,
    pub rule_id: String,
    pub can_promote: bool,
    pub rule: Option<RemovalRule>,
}

pub fn candidate_scope(candidate: &RepeatedArtifactCandidate) -> RemovalScope {
    if candidate.kind == RepeatedArtifactKind::InlineArtifact {
        return RemovalScope::Anywhere;
    }

    if candidate.position_summary_is_page_based && candidate.position_summary.middle_count == 0 {
        let top = candidate.position_summary.top_count;
        let bottom = candidate.position_summary.bottom_count;
        if top > 0 && bottom == 0 {
            return RemovalScope::PageTop;
        }
        if top == 0 && bottom > 0 {
            return RemovalScope::PageBottom;
        }
        if top > 0 && bottom > 0 {
            return RemovalScope::PageTopOrBottom;
        }
    }

    RemovalScope::WholeLine
}

pub fn candidate_rule_text(candidate: &RepeatedArtifactCandidate) -> String {
    match candidate.kind {
        RepeatedArtifactKind::NormalizedLine => candidate.normalized_key.trim().to_string(),
        _ => candidate.display_text.clone(),
    }
}

pub fn candidate_matcher_kind(
    candidate: &RepeatedArtifactCandidate,
) -> RepeatedArtifactMatcherKind {
    match candidate.kind {
        RepeatedArtifactKind::NormalizedLine => RepeatedArtifactMatcherKind::NormalizedLine,
        _ => RepeatedArtifactMatcherKind::Literal,
    }
}

pub fn rule_id(candidate: &RepeatedArtifactCandidate, scope: RemovalScope) -> String {
    let scope = scope_name(scope);
    match candidate.kind {
        RepeatedArtifactKind::NormalizedLine => format!(
            "repeated-artifact:normalized-line:{scope}:{}",
            candidate.candidate_id
        ),
        _ => format!("repeated-artifact:{scope}:{}", candidate.candidate_id),
    }
}

pub fn can_promote(candidate: &RepeatedArtifactCandidate) -> bool {
    !candidate_rule_text(candidate).trim().is_empty()
}

pub fn promote_candidate(
    candidate: &RepeatedArtifactCandidate,
    label: Option<&str>,
    scope_override: Option<RemovalScope>,
) -> Option<RemovalRule> {
    let rule_text = candidate_rule_text(candidate);
    if rule_text.trim().is_empty() {
        return None;
    }

    let scope = scope_override.unwrap_or_else(|| candidate_scope(candidate));
    let matcher = match candidate.kind {
        RepeatedArtifactKind::NormalizedLine => RemovalMatcher::NormalizedLine {
            normalized_key: rule_text.trim().to_string(),
        },
        _ => RemovalMatcher::Literal {
            text: candidate.display_text.clone(),
        },
    };

    if scope == RemovalScope::Anywhere {
        match &matcher {
            RemovalMatcher::NormalizedLine { .. } => return None,
            RemovalMatcher::Literal { text } if !is_valid_anywhere_literal_text(text) => {
                return None;
            }
            RemovalMatcher::Literal { .. } => {}
        }
    }

    let label = label
        .map(str::to_string)
        .unwrap_or_else(|| default_rule_label(&matcher, scope));

    Some(RemovalRule {
        id: rule_id(candidate, scope),
        label,
        source: RemovalRuleSource::PromotedRepeatedArtifact,
        matcher,
        scope,
        enabled: true,
    })
}

pub fn promotion_for_candidate(candidate: &RepeatedArtifactCandidate) -> RepeatedArtifactPromotion {
    let scope = candidate_scope(candidate);
    RepeatedArtifactPromotion {
        candidate_id: candidate.candidate_id.clone(),
        rule_text: candidate_rule_text(candidate),
        matcher_kind: candidate_matcher_kind(candidate),
        scope,
        rule_id: rule_id(candidate, scope),
        can_promote: can_promote(candidate),
        rule: promote_candidate(candidate, None, Some(scope)),
    }
}

fn scope_name(scope: RemovalScope) -> &'static str {
    match scope {
        RemovalScope::Anywhere => "anywhere",
        RemovalScope::WholeLine => "whole_line",
        RemovalScope::PageTop => "page_top",
        RemovalScope::PageBottom => "page_bottom",
        RemovalScope::PageTopOrBottom => "page_top_or_bottom",
    }
}

fn default_rule_label(matcher: &RemovalMatcher, scope: RemovalScope) -> String {
    match matcher {
        RemovalMatcher::Literal { text } => {
            let scope_label = match scope {
                RemovalScope::WholeLine => "Whole line",
                RemovalScope::PageTop => "Page top",
                RemovalScope::PageBottom => "Page bottom",
                RemovalScope::PageTopOrBottom => "Page top/bottom",
                RemovalScope::Anywhere => "Inline literal",
            };
            format!("{scope_label}: {text}")
        }
        RemovalMatcher::NormalizedLine { normalized_key } => {
            let scope_label = match scope {
                RemovalScope::WholeLine => "Whole-line",
                RemovalScope::PageTop => "Page top",
                RemovalScope::PageBottom => "Page bottom",
                RemovalScope::PageTopOrBottom => "Page top/bottom",
                RemovalScope::Anywhere => "Anywhere",
            };
            format!("{scope_label} family: {normalized_key}")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::preparation::repeated_artifacts::{
        ArtifactRiskLabel, CandidateContentClass, PositionSummary,
    };
    use crate::text::profile::{CandidateTextProfile, CandidateTextSignalLabel};
    use serde::Deserialize;

    #[derive(Deserialize)]
    struct PromotionFixture {
        candidate_id: String,
        kind: RepeatedArtifactKind,
        display_text: String,
        normalized_key: String,
        top: usize,
        middle: usize,
        bottom: usize,
        page_based: bool,
        can_promote: bool,
        expected_rule: Option<RemovalRule>,
    }

    fn candidate(
        candidate_id: &str,
        kind: RepeatedArtifactKind,
        display_text: &str,
        normalized_key: &str,
        position_summary: PositionSummary,
        page_based: bool,
        risk_label: ArtifactRiskLabel,
    ) -> RepeatedArtifactCandidate {
        RepeatedArtifactCandidate {
            candidate_id: candidate_id.to_string(),
            kind,
            display_text: display_text.to_string(),
            normalized_key: normalized_key.to_string(),
            occurrence_count: 3,
            file_count: 1,
            example_count: 0,
            position_summary,
            position_summary_is_page_based: page_based,
            risk_label,
            content_class: CandidateContentClass::TextDominant,
            text_profile: CandidateTextProfile::default(),
            text_signal_label: CandidateTextSignalLabel::default(),
            text_signal_reasons: Vec::new(),
            raw_variant_count: 1,
            raw_variant_count_is_capped: false,
            raw_variants: Vec::new(),
            examples: Vec::new(),
        }
    }

    fn positions(top: usize, middle: usize, bottom: usize) -> PositionSummary {
        PositionSummary {
            top_count: top,
            middle_count: middle,
            bottom_count: bottom,
            unknown_count: 0,
        }
    }

    #[test]
    fn scope_selection_matches_the_gui_contract() {
        let exact = |id, summary, page_based| {
            candidate(
                id,
                RepeatedArtifactKind::ExactLine,
                "Header",
                "header",
                summary,
                page_based,
                ArtifactRiskLabel::StrongHeaderFooterCandidate,
            )
        };

        assert_eq!(
            candidate_scope(&exact("top", positions(3, 0, 0), true)),
            RemovalScope::PageTop
        );
        assert_eq!(
            candidate_scope(&exact("bottom", positions(0, 0, 3), true)),
            RemovalScope::PageBottom
        );
        assert_eq!(
            candidate_scope(&exact("both", positions(2, 0, 1), true)),
            RemovalScope::PageTopOrBottom
        );
        assert_eq!(
            candidate_scope(&exact("middle", positions(2, 1, 0), true)),
            RemovalScope::WholeLine
        );
        assert_eq!(
            candidate_scope(&exact("flat", positions(3, 0, 0), false)),
            RemovalScope::WholeLine
        );

        let inline = candidate(
            "inline_1",
            RepeatedArtifactKind::InlineArtifact,
            "<br />",
            "",
            positions(0, 3, 0),
            false,
            ArtifactRiskLabel::Ambiguous,
        );
        assert_eq!(candidate_scope(&inline), RemovalScope::Anywhere);
    }

    #[test]
    fn shared_typescript_characterisation_fixture_matches_rust_promotion() {
        let fixtures: Vec<PromotionFixture> = serde_json::from_str(include_str!(
            "../../../tests/fixtures/repeated_artifact_promotion.json"
        ))
        .unwrap();

        for fixture in fixtures {
            let candidate = candidate(
                &fixture.candidate_id,
                fixture.kind,
                &fixture.display_text,
                &fixture.normalized_key,
                positions(fixture.top, fixture.middle, fixture.bottom),
                fixture.page_based,
                ArtifactRiskLabel::StrongHeaderFooterCandidate,
            );
            assert_eq!(
                can_promote(&candidate),
                fixture.can_promote,
                "acceptance mismatch for {}",
                fixture.candidate_id
            );
            assert_eq!(
                promote_candidate(&candidate, None, None),
                fixture.expected_rule,
                "rule mismatch for {}",
                fixture.candidate_id
            );
        }
    }

    #[test]
    fn promotion_serialization_matches_the_former_typescript_output() {
        let normalized = candidate(
            "norm_2222222222222222",
            RepeatedArtifactKind::NormalizedLine,
            "Page 12",
            "  page #  ",
            positions(0, 3, 0),
            false,
            ArtifactRiskLabel::PossibleBoilerplate,
        );
        let rule = promote_candidate(&normalized, None, None).unwrap();
        assert_eq!(
            serde_json::to_string(&rule).unwrap(),
            r#"{"id":"repeated-artifact:normalized-line:whole_line:norm_2222222222222222","label":"Whole-line family: page #","source":"promoted_repeated_artifact","matcher":{"kind":"normalized_line","normalized_key":"page #"},"scope":"whole_line","enabled":true}"#
        );

        let inline = candidate(
            "inline_1111111111111111",
            RepeatedArtifactKind::InlineArtifact,
            "<br />",
            "",
            positions(0, 3, 0),
            false,
            ArtifactRiskLabel::Ambiguous,
        );
        let rule = promote_candidate(&inline, None, None).unwrap();
        assert_eq!(
            serde_json::to_string(&rule).unwrap(),
            r#"{"id":"repeated-artifact:anywhere:inline_1111111111111111","label":"Inline literal: <br />","source":"promoted_repeated_artifact","matcher":{"kind":"literal","text":"<br />"},"scope":"anywhere","enabled":true}"#
        );
    }

    #[test]
    fn anywhere_validation_preserves_the_acceptance_predicate() {
        for (id, text) in [
            ("empty", ""),
            ("whitespace", "   "),
            ("lf", "line one\nline two"),
            ("cr", "line one\rline two"),
        ] {
            let candidate = candidate(
                id,
                RepeatedArtifactKind::InlineArtifact,
                text,
                "",
                positions(0, 1, 0),
                false,
                ArtifactRiskLabel::Ambiguous,
            );
            assert_eq!(can_promote(&candidate), !text.trim().is_empty());
            assert!(promote_candidate(&candidate, None, None).is_none());
        }
    }

    #[test]
    fn multi_line_blocks_keep_the_existing_literal_whole_line_shape() {
        let candidate = candidate(
            "block_1",
            RepeatedArtifactKind::TwoLineBlock,
            "Header one\nHeader two",
            "header one\nheader two",
            positions(3, 0, 0),
            true,
            ArtifactRiskLabel::StrongHeaderFooterCandidate,
        );
        let rule = promote_candidate(&candidate, None, None).unwrap();
        assert_eq!(rule.scope, RemovalScope::PageTop);
        assert_eq!(
            rule.matcher,
            RemovalMatcher::Literal {
                text: "Header one\nHeader two".to_string()
            }
        );
    }
}
