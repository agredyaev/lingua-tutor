//! Deterministic Spanish error classification (WBS 7.2): bounded,
//! first-match-wins rules mapping to stable `es.*` taxonomy codes
//! (assets/error-taxonomy/taxonomy.json).

use crate::normalize::{comparison, strip_accents};
use language_pack_api::{CoreErrorCategory, LanguageErrorClassification};

const SER_FORMS: [&str; 6] = ["soy", "eres", "es", "somos", "sois", "son"];
const ESTAR_FORMS: [&str; 6] = ["estoy", "estás", "está", "estamos", "estáis", "están"];
const ARTICLES: [&str; 8] = ["el", "la", "los", "las", "un", "una", "unos", "unas"];

/// Classifies a correction pair. First matching rule wins; codes are stable
/// across pack patch versions (06-language-packs.md §2).
#[must_use]
pub fn classify(original: &str, corrected: &str) -> LanguageErrorClassification {
    let orig = comparison(original);
    let corr = comparison(corrected);

    let (category, code) = if orig == corr {
        (CoreErrorCategory::Orthography, "es.orthography.general")
    } else if strip_accents(&orig) == strip_accents(&corr) {
        (CoreErrorCategory::Orthography, "es.orthography.accent")
    } else if copula_swapped(&orig, &corr) {
        (CoreErrorCategory::Grammar, "es.verb.ser_estar")
    } else if word_swapped(&orig, &corr, "por", "para") {
        (CoreErrorCategory::Grammar, "es.prep.por_para")
    } else if article_only_difference(&orig, &corr) {
        (CoreErrorCategory::Grammar, "es.agreement.gender")
    } else if reordered(&orig, &corr) {
        (CoreErrorCategory::WordOrder, "es.word_order")
    } else if same_length_word_swap(&orig, &corr) {
        (CoreErrorCategory::Vocabulary, "es.vocab.choice")
    } else {
        (CoreErrorCategory::Grammar, "es.grammar.general")
    };

    LanguageErrorClassification {
        core_category: category,
        pack_error_code: Some(code.to_owned()),
        explanation: None,
    }
}

fn words(text: &str) -> Vec<&str> {
    text.split(' ').filter(|w| !w.is_empty()).collect()
}

/// One side uses a ser form where the other uses an estar form.
fn copula_swapped(orig: &str, corr: &str) -> bool {
    let orig_words = words(orig);
    let corr_words = words(corr);
    let has = |list: &[&str], forms: &[&str]| list.iter().any(|w| forms.contains(w));
    (has(&orig_words, &SER_FORMS) && has(&corr_words, &ESTAR_FORMS))
        || (has(&orig_words, &ESTAR_FORMS) && has(&corr_words, &SER_FORMS))
}

fn word_swapped(orig: &str, corr: &str, a: &str, b: &str) -> bool {
    let orig_words = words(orig);
    let corr_words = words(corr);
    (orig_words.contains(&a) && corr_words.contains(&b) && !corr_words.contains(&a))
        || (orig_words.contains(&b) && corr_words.contains(&a) && !corr_words.contains(&b))
}

/// Same sentence except article choices (gender/number agreement).
fn article_only_difference(orig: &str, corr: &str) -> bool {
    let orig_words = words(orig);
    let corr_words = words(corr);
    if orig_words.len() != corr_words.len() {
        return false;
    }
    let mut any_article_diff = false;
    for (a, b) in orig_words.iter().zip(&corr_words) {
        if a == b {
            continue;
        }
        if ARTICLES.contains(a) && ARTICLES.contains(b) {
            any_article_diff = true;
        } else {
            return false;
        }
    }
    any_article_diff
}

/// Same words, different order.
fn reordered(orig: &str, corr: &str) -> bool {
    let mut orig_words = words(orig);
    let mut corr_words = words(corr);
    if orig_words == corr_words {
        return false;
    }
    orig_words.sort_unstable();
    corr_words.sort_unstable();
    orig_words == corr_words
}

/// Same word count, at least one word replaced (lexical choice).
fn same_length_word_swap(orig: &str, corr: &str) -> bool {
    let orig_words = words(orig);
    let corr_words = words(corr);
    orig_words.len() == corr_words.len()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn code(original: &str, corrected: &str) -> String {
        classify(original, corrected).pack_error_code.unwrap()
    }

    #[test]
    fn rules_map_to_stable_codes() {
        assert_eq!(code("que tal", "qué tal"), "es.orthography.accent");
        assert_eq!(code("Qué tal", "¿qué tal?"), "es.orthography.general");
        assert_eq!(code("estoy médico", "soy médico"), "es.verb.ser_estar");
        assert_eq!(code("es para eso", "es por eso"), "es.prep.por_para");
        assert_eq!(code("el casa", "la casa"), "es.agreement.gender");
        assert_eq!(code("blanca casa la", "la casa blanca"), "es.word_order");
        assert_eq!(
            code("quiero la factura", "quiero la cuenta"),
            "es.vocab.choice"
        );
        assert_eq!(
            code("yo tengo hambre mucho grande", "tengo mucha hambre"),
            "es.grammar.general"
        );
    }

    #[test]
    fn classification_is_deterministic() {
        let first = classify("estoy médico", "soy médico");
        let second = classify("estoy médico", "soy médico");
        assert_eq!(first, second);
    }
}
