//! Spanish (Spain) normalization + matching policy (06-language-packs.md §7,
//! WBS 7.1). Uses character operations; no regex or ICU dependency.
//! (09-frontend.md §9 size rule; 16-rust-guidelines.md §5).

/// Canonical stored form: trimmed, single-spaced, case/diacritics/¿¡ kept.
#[must_use]
pub fn canonical(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut pending_space = false;
    for ch in input.trim().chars() {
        if ch.is_whitespace() {
            pending_space = true;
        } else {
            if pending_space && !out.is_empty() {
                out.push(' ');
            }
            pending_space = false;
            out.push(ch);
        }
    }
    out
}

/// Comparison form for answer matching: canonical + lowercase, ¿¡ and
/// commas/sentence punctuation dropped (prosody, not meaning),
/// `de el`→`del`, `a el`→`al`. Diacritics are KEPT — tú/tu, sí/si differ
/// in meaning; a missing tilde is feedback material, not a silent pass.
#[must_use]
pub fn comparison(input: &str) -> String {
    let lowered: String = canonical(input)
        .to_lowercase()
        .chars()
        .filter(|c| {
            !matches!(
                c,
                '¿' | '¡' | '.' | '!' | '?' | '…' | ',' | ';' | ':' | '"' | '«' | '»'
            )
        })
        .collect();
    contract(canonical(&lowered).as_str())
}

/// Mandatory Spanish contractions the learner often expands.
fn contract(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut words = text.split(' ').peekable();
    while let Some(word) = words.next() {
        let contracted = match (word, words.peek()) {
            ("de", Some(&"el")) => {
                words.next();
                "del"
            }
            ("a", Some(&"el")) => {
                words.next();
                "al"
            }
            _ => word,
        };
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(contracted);
    }
    out
}

/// Accent-stripped variant, used only to *detect* accent-only mistakes for
/// feedback — never to accept them.
#[must_use]
pub fn strip_accents(input: &str) -> String {
    input
        .chars()
        .map(|ch| match ch {
            'á' => 'a',
            'é' => 'e',
            'í' => 'i',
            'ó' => 'o',
            'ú' | 'ü' => 'u',
            'Á' => 'A',
            'É' => 'E',
            'Í' => 'I',
            'Ó' => 'O',
            'Ú' | 'Ü' => 'U',
            other => other,
        })
        .collect()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn canonical_keeps_marks_and_case() {
        assert_eq!(canonical("  ¿Qué   tal? "), "¿Qué tal?");
    }

    #[test]
    fn comparison_drops_marks_case_and_contracts() {
        assert_eq!(comparison("¿Vamos A EL centro?"), "vamos al centro");
        assert_eq!(
            comparison("La casa de el profesor."),
            "la casa del profesor"
        );
    }

    #[test]
    fn comparison_keeps_diacritics() {
        assert_ne!(comparison("tú"), comparison("tu"));
        assert_eq!(comparison("¿Qué tal?"), "qué tal");
    }

    #[test]
    fn accent_strip_detects_accent_only_difference() {
        assert_eq!(
            strip_accents(&comparison("que tal")),
            strip_accents(&comparison("qué tal"))
        );
    }
}
