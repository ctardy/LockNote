// Case conversion of Edit > Convert Case (UPPERCASE, lowercase, Title Case).
//
// Pure text transforms, Unicode-aware: the result can be longer or shorter
// than the input ("ß" -> "SS"). The editor (`ui::editor::case_ops`) picks
// the text, calls `convert` and puts the result back as one undo step.
// Nothing here is stored.

/// One command of the Edit > Convert Case submenu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaseOp {
    Upper,
    Lower,
    Title,
}

/// `text` converted by `op`.
pub fn convert(op: CaseOp, text: &str) -> String {
    match op {
        CaseOp::Upper => text.to_uppercase(),
        // str::to_lowercase handles the final sigma ("ΟΔΟΣ" -> "οδος").
        CaseOp::Lower => text.to_lowercase(),
        CaseOp::Title => title_case(text),
    }
}

/// Title Case: in every whitespace-delimited word (the double-click word,
/// `char::is_whitespace`), the first letter takes its title form and the
/// rest is lowercased. Whitespace is kept as is.
fn title_case(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut word_start: Option<usize> = None;
    for (i, ch) in text.char_indices() {
        if ch.is_whitespace() {
            if let Some(s) = word_start.take() {
                out.push_str(&title_word(&text[s..i]));
            }
            out.push(ch);
        } else if word_start.is_none() {
            word_start = Some(i);
        }
    }
    if let Some(s) = word_start {
        out.push_str(&title_word(&text[s..]));
    }
    out
}

/// One word in Title Case. Its first alphabetic character is capitalised;
/// what comes before it (punctuation, digits) has no case.
fn title_word(word: &str) -> String {
    let Some((i, first)) = word.char_indices().find(|(_, c)| c.is_alphabetic()) else {
        return word.to_lowercase();
    };
    let rest = &word[i + first.len_utf8()..];
    let mut out = word[..i].to_lowercase();
    // Rust has no titlecase mapping: the uppercase form's first character,
    // then the others lowercased ("ß" -> "SS" -> "Ss", "ﬁ" -> "FI" -> "Fi").
    let mut upper = first.to_uppercase();
    if let Some(c) = upper.next() {
        out.push(c);
    }
    out.extend(upper.flat_map(char::to_lowercase));
    out.push_str(&rest.to_lowercase());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upper_and_lower_of_ascii() {
        assert_eq!(convert(CaseOp::Upper, "Hello, World 42"), "HELLO, WORLD 42");
        assert_eq!(convert(CaseOp::Lower, "Hello, World 42"), "hello, world 42");
        assert_eq!(convert(CaseOp::Upper, ""), "");
        assert_eq!(convert(CaseOp::Title, ""), "");
    }

    #[test]
    fn upper_and_lower_of_accented_letters() {
        assert_eq!(convert(CaseOp::Upper, "élève à Noël"), "ÉLÈVE À NOËL");
        assert_eq!(convert(CaseOp::Lower, "ÉLÈVE À NOËL"), "élève à noël");
    }

    #[test]
    fn conversion_can_change_the_length() {
        // "ß" has no single uppercase letter: it becomes "SS".
        assert_eq!(convert(CaseOp::Upper, "straße"), "STRASSE");
        // "İ" (dotted capital I) lowercases to "i" + U+0307: 2 bytes -> 3.
        assert_eq!(convert(CaseOp::Lower, "İ"), "i\u{0307}");
        // The ligature "ﬁ" uppercases to two letters.
        assert_eq!(convert(CaseOp::Upper, "ﬁne"), "FINE");
    }

    #[test]
    fn lower_uses_the_final_sigma() {
        assert_eq!(convert(CaseOp::Lower, "ΟΔΟΣ"), "οδος");
    }

    #[test]
    fn surrogate_pairs_are_kept_or_converted_whole() {
        // An emoji (2 UTF-16 units) has no case: unchanged, not split.
        assert_eq!(convert(CaseOp::Upper, "a😀b"), "A😀B");
        assert_eq!(convert(CaseOp::Title, "😀abc"), "😀Abc");
        // Deseret letters live outside the BMP and have case.
        assert_eq!(convert(CaseOp::Upper, "\u{10428}"), "\u{10400}");
        assert_eq!(convert(CaseOp::Lower, "\u{10400}x"), "\u{10428}x");
    }

    #[test]
    fn title_capitalises_the_first_letter_of_each_word() {
        assert_eq!(convert(CaseOp::Title, "hello WORLD"), "Hello World");
        assert_eq!(convert(CaseOp::Title, "  two  spaces\r\nnext\tline"), "  Two  Spaces\r\nNext\tLine");
        assert_eq!(convert(CaseOp::Title, "éCOLE"), "École");
    }

    #[test]
    fn title_words_are_whitespace_delimited_like_the_double_click() {
        // Punctuation and digits stay inside the word: only its first
        // letter is capitalised.
        assert_eq!(convert(CaseOp::Title, "don't stop"), "Don't Stop");
        assert_eq!(convert(CaseOp::Title, "(hello) user@MAIL.com"), "(Hello) User@mail.com");
        assert_eq!(convert(CaseOp::Title, "jean-PIERRE 42abc"), "Jean-pierre 42Abc");
    }

    #[test]
    fn title_of_a_letter_without_single_capital() {
        // The first letter takes the title form: "ß" -> "Ss", not "SS".
        assert_eq!(convert(CaseOp::Title, "ßa"), "Ssa");
        assert_eq!(convert(CaseOp::Title, "ﬁne"), "Fine");
    }

    #[test]
    fn conversion_is_idempotent() {
        for s in ["straße Noël", "ΟΔΟΣ", "a😀b don't", "İx"] {
            for op in [CaseOp::Upper, CaseOp::Lower, CaseOp::Title] {
                let once = convert(op, s);
                assert_eq!(convert(op, &once), once, "{op:?} of {s:?}");
            }
        }
    }
}
