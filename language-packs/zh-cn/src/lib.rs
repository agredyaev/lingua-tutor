//! Mandarin Chinese (Simplified) pack — the M7 second-pack validation
//! (13-roadmap.md M7): proves the core is language-neutral against a
//! no-whitespace CJK script. Normalization removes whitespace *between*
//! CJK characters (Chinese text carries no word spaces) while keeping
//! single spaces inside Latin runs (pinyin, names).

use language_pack_api::{
    AnswerEvaluation, AnswerEvaluationRequest, AnswerVerdict, Capability, CardIntent, CardTemplate,
    CoreErrorCategory, CurriculumCatalog, LanguageErrorClassification,
    LanguageErrorClassificationRequest, LanguagePack, LanguagePackManifest, LearnerLanguageProfile,
    NormalizationMode, NormalizedText, PhraseSeed, Scenario, TutorInstructionBundle,
};

/// Builds the pack for registration.
#[must_use]
pub fn register() -> Box<dyn LanguagePack> {
    Box::new(ZhCnPack::new())
}

struct ZhCnPack {
    manifest: LanguagePackManifest,
    templates: Vec<CardTemplate>,
    catalog: CurriculumCatalog,
}

/// CJK ranges that never take surrounding spaces: unified ideographs
/// (+ext A), and CJK/fullwidth punctuation.
fn is_cjk(c: char) -> bool {
    matches!(c,
        '\u{3400}'..='\u{4DBF}' | '\u{4E00}'..='\u{9FFF}'
        | '\u{3000}'..='\u{303F}' | '\u{FF00}'..='\u{FFEF}')
}

/// Punctuation dropped in comparison mode (ASCII + common CJK marks).
fn is_comparison_punctuation(c: char) -> bool {
    c.is_ascii_punctuation()
        || matches!(
            c,
            '。' | '，'
                | '、'
                | '！'
                | '？'
                | '；'
                | '：'
                | '「'
                | '」'
                | '『'
                | '』'
                | '（'
                | '）'
                | '《'
                | '》'
                | '…'
                | '·'
                | '\u{201C}'
                | '\u{201D}'
                | '\u{2018}'
                | '\u{2019}'
        )
}

impl ZhCnPack {
    fn new() -> Self {
        Self {
            manifest: LanguagePackManifest {
                id: "zh-CN".to_owned(),
                package: "language-pack-zh-cn".to_owned(),
                version: "0.1.0".to_owned(),
                contract_version: language_pack_api::CONTRACT_VERSION,
                target_language: "zh".to_owned(),
                target_locale: "zh-CN".to_owned(),
                display_name: "Chinese (Mandarin, Simplified)".to_owned(),
                supported_source_languages: vec!["ru".to_owned(), "en".to_owned()],
                default_source_language: "ru".to_owned(),
                level_framework: "HSK".to_owned(),
                capabilities: vec![
                    Capability::ProductionCards,
                    Capability::ListeningCards,
                    Capability::ContrastCards,
                    Capability::TransferCards,
                    Capability::ErrorClassification,
                    Capability::AnswerMatching,
                ],
            },
            templates: [
                (CardIntent::Production, "zh-production"),
                (CardIntent::Listening, "zh-listening"),
                (CardIntent::Contrast, "zh-contrast"),
                (CardIntent::Transfer, "zh-transfer"),
            ]
            .into_iter()
            .map(|(intent, id)| CardTemplate {
                template_id: id.to_owned(),
                intent,
                prompt_template: format!("{id}: {{prompt}}"),
                answer_evaluation_mode: "exact".to_owned(),
            })
            .collect(),
            catalog: CurriculumCatalog {
                scenarios: vec![
                    Scenario {
                        id: "zh-greetings".to_owned(),
                        title: "问候 — greetings and introductions".to_owned(),
                        level: "HSK1".to_owned(),
                    },
                    Scenario {
                        id: "zh-restaurant".to_owned(),
                        title: "点菜 — ordering food".to_owned(),
                        level: "HSK1".to_owned(),
                    },
                ],
                seeds: vec![PhraseSeed {
                    target_text: "你好，很高兴认识你".to_owned(),
                    prompt: "Greet someone you meet for the first time".to_owned(),
                    scenario_id: "zh-greetings".to_owned(),
                }],
            },
        }
    }

    /// Canonical: trim; drop whitespace adjacent to a CJK character; collapse
    /// remaining whitespace runs (Latin/pinyin) to one space. Idempotent.
    fn canonical(input: &str) -> String {
        let tokens: Vec<&str> = input.split_whitespace().collect();
        let mut out = String::with_capacity(input.len());
        for (i, token) in tokens.iter().enumerate() {
            if i > 0 {
                let prev_last = out.chars().last();
                let next_first = token.chars().next();
                let cjk_boundary = prev_last.is_some_and(is_cjk) || next_first.is_some_and(is_cjk);
                if !cjk_boundary {
                    out.push(' ');
                }
            }
            out.push_str(token);
        }
        out
    }

    /// Comparison: canonical + drop punctuation + lowercase Latin (pinyin).
    fn comparison(input: &str) -> String {
        Self::canonical(input)
            .chars()
            .filter(|c| !is_comparison_punctuation(*c))
            .flat_map(char::to_lowercase)
            .collect::<String>()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    }

    fn char_count(text: &str) -> usize {
        text.chars().filter(|c| !c.is_whitespace()).count()
    }
}

impl LanguagePack for ZhCnPack {
    fn manifest(&self) -> &LanguagePackManifest {
        &self.manifest
    }

    fn normalize_target_text(&self, input: &str, mode: NormalizationMode) -> NormalizedText {
        NormalizedText(match mode {
            NormalizationMode::Canonical => Self::canonical(input),
            NormalizationMode::Comparison => Self::comparison(input),
        })
    }

    fn evaluate_answer(&self, request: &AnswerEvaluationRequest) -> AnswerEvaluation {
        let given = Self::comparison(&request.learner_answer);
        if given == Self::comparison(&request.expected_answer) {
            return AnswerEvaluation {
                verdict: AnswerVerdict::Correct,
                matched_answer: Some(request.expected_answer.clone()),
                feedback: None,
            };
        }
        let variant = request
            .accepted_answers
            .iter()
            .find(|a| Self::comparison(a) == given);
        match variant {
            Some(matched) => AnswerEvaluation {
                verdict: AnswerVerdict::AcceptableVariant,
                matched_answer: Some(matched.clone()),
                feedback: None,
            },
            None => AnswerEvaluation {
                verdict: AnswerVerdict::Incorrect,
                matched_answer: None,
                feedback: None,
            },
        }
    }

    fn classify_error(
        &self,
        request: &LanguageErrorClassificationRequest,
    ) -> LanguageErrorClassification {
        let original = Self::comparison(&request.original_production);
        let corrected = Self::comparison(&request.corrected_production);
        // Deterministic taxonomy: identical after normalization = writing
        // form; changed length = structure (order / measure words / 了);
        // same length = word choice or tone confusion.
        let (category, code) = if original == corrected {
            (CoreErrorCategory::Orthography, "zh.orth.form")
        } else if Self::char_count(&original) != Self::char_count(&corrected) {
            (CoreErrorCategory::Grammar, "zh.gram.structure")
        } else {
            (CoreErrorCategory::Vocabulary, "zh.voc.choice")
        };
        LanguageErrorClassification {
            core_category: category,
            pack_error_code: Some(code.to_owned()),
            explanation: None,
        }
    }

    fn card_templates(&self) -> &[CardTemplate] {
        &self.templates
    }

    fn tutor_instructions(&self, learner: &LearnerLanguageProfile) -> TutorInstructionBundle {
        TutorInstructionBundle {
            instructions: format!(
                "Teach Mandarin (simplified characters). Instruct from {}. \
                 Always give pinyin with tone marks alongside characters; \
                 speaking-first, characters second. Target level: {}.",
                learner.source_language,
                learner.estimated_level.as_deref().unwrap_or("HSK1"),
            ),
            correction_conventions: "Correct inline: characters + pinyin + one-line why; \
                 tone errors are vocabulary-critical, always flag them."
                .to_owned(),
        }
    }

    fn curriculum_catalog(&self) -> &CurriculumCatalog {
        &self.catalog
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn pack() -> Box<dyn LanguagePack> {
        register()
    }

    #[test]
    fn canonical_removes_spaces_between_hanzi_keeps_latin_spacing() {
        let out = pack()
            .normalize_target_text("你 好 ， 我 叫 Anna Petrova", NormalizationMode::Canonical);
        assert_eq!(out.0, "你好，我叫Anna Petrova");
        // Idempotent (conformance also proptests this).
        let again = pack().normalize_target_text(&out.0, NormalizationMode::Canonical);
        assert_eq!(again.0, out.0);
    }

    #[test]
    fn comparison_strips_cjk_punctuation_and_lowercases_pinyin() {
        let out = pack().normalize_target_text("你好！Nǐ Hǎo。", NormalizationMode::Comparison);
        assert_eq!(out.0, "你好nǐ hǎo");
    }

    #[test]
    fn evaluate_ignores_spacing_and_punctuation_differences() {
        let request = |answer: &str| AnswerEvaluationRequest {
            expected_answer: "你好，很高兴认识你".to_owned(),
            accepted_answers: vec!["认识你很高兴".to_owned()],
            learner_answer: answer.to_owned(),
            evaluation_mode: "exact".to_owned(),
        };
        let pack = pack();
        assert_eq!(
            pack.evaluate_answer(&request("你好 很高兴 认识你！"))
                .verdict,
            AnswerVerdict::Correct
        );
        assert_eq!(
            pack.evaluate_answer(&request("认识你很高兴。")).verdict,
            AnswerVerdict::AcceptableVariant
        );
        assert_eq!(
            pack.evaluate_answer(&request("再见")).verdict,
            AnswerVerdict::Incorrect
        );
    }

    #[test]
    fn classification_deterministic_with_pack_codes() {
        let pack = pack();
        let request = LanguageErrorClassificationRequest {
            original_production: "我有三个书".to_owned(),
            corrected_production: "我有三本书".to_owned(),
            context: None,
        };
        let first = pack.classify_error(&request);
        assert_eq!(first, pack.classify_error(&request));
        assert_eq!(first.pack_error_code.as_deref(), Some("zh.voc.choice"));
        // Structure change (dropped 了):
        let grammar = pack.classify_error(&LanguageErrorClassificationRequest {
            original_production: "我吃饭".to_owned(),
            corrected_production: "我吃饭了".to_owned(),
            context: None,
        });
        assert_eq!(grammar.core_category, CoreErrorCategory::Grammar);
        assert_eq!(
            grammar.pack_error_code.as_deref(),
            Some("zh.gram.structure")
        );
    }
}
