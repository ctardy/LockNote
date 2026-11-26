// Unit tests of the editor module (moved out of editor.rs unchanged).

    use super::*;

    // ── save_precondition ──

    #[test]
    fn save_refused_when_password_empty() {
        assert!(save_precondition("").is_err());
    }

    #[test]
    fn save_allowed_with_password() {
        assert_eq!(save_precondition("abcd"), Ok(()));
    }

    // ── link_text_range (EN_LINK) ──

    #[test]
    fn link_after_accent_is_exact() {
        assert_eq!(link_text_range("é http://x.com", 2, 14), Ok("http://x.com"));
    }

    #[test]
    fn link_after_two_accents() {
        assert_eq!(link_text_range("éé http://a.b", 3, 13), Ok("http://a.b"));
    }

    #[test]
    fn link_after_emoji() {
        // 😀 is 2 UTF-16 code units
        assert_eq!(link_text_range("😀 http://a.b", 3, 13), Ok("http://a.b"));
    }

    #[test]
    fn link_mid_surrogate_is_error() {
        assert_eq!(
            link_text_range("😀x", 1, 3),
            Err(Utf16RangeError::MidSurrogate { pos: 1 })
        );
    }

    #[test]
    fn link_after_crlf() {
        assert_eq!(link_text_range("a\r\nhttp://x.y", 2, 12), Ok("http://x.y"));
    }

    #[test]
    fn link_after_lone_cr() {
        assert_eq!(link_text_range("a\rhttp://x.y", 2, 12), Ok("http://x.y"));
    }

    #[test]
    fn link_out_of_bounds() {
        assert_eq!(
            link_text_range("abc", 0, 10),
            Err(Utf16RangeError::OutOfBounds { max: 10, len_u16: 3 })
        );
    }

    #[test]
    fn link_empty_or_negative() {
        assert_eq!(link_text_range("abcdefgh", 5, 5), Err(Utf16RangeError::Empty));
        assert_eq!(link_text_range("abcdefgh", -1, 3), Err(Utf16RangeError::Negative));
    }

    #[test]
    fn link_ascii_unchanged() {
        assert_eq!(link_text_range("see http://x.y ok", 4, 14), Ok("http://x.y"));
    }

    // ── utf16_range_to_byte_range ──

    #[test]
    fn utf16_range_ascii() {
        let text = "hello world";
        // UTF-16 and byte positions are identical for ASCII
        assert_eq!(utf16_range_to_byte_range(text, 0, 5), (0, 5));
        assert_eq!(utf16_range_to_byte_range(text, 6, 11), (6, 11));
    }

    #[test]
    fn utf16_range_multibyte() {
        // 'é' is 2 bytes in UTF-8 but 1 code unit in UTF-16
        let text = "café";
        // byte layout: c(1) a(1) f(1) é(2) = 5 bytes
        // utf16:       c(1) a(1) f(1) é(1) = 4 units
        assert_eq!(utf16_range_to_byte_range(text, 0, 4), (0, 5));
        assert_eq!(utf16_range_to_byte_range(text, 3, 4), (3, 5));
    }

    #[test]
    fn utf16_range_emoji() {
        // '😀' is 4 bytes in UTF-8, 2 code units in UTF-16 (surrogate pair)
        let text = "a😀b";
        // bytes: a(1) 😀(4) b(1) = 6 bytes
        // utf16: a(1) 😀(2) b(1) = 4 units
        assert_eq!(utf16_range_to_byte_range(text, 0, 1), (0, 1));   // "a"
        assert_eq!(utf16_range_to_byte_range(text, 1, 3), (1, 5));   // "😀"
        assert_eq!(utf16_range_to_byte_range(text, 3, 4), (5, 6));   // "b"
        assert_eq!(utf16_range_to_byte_range(text, 0, 4), (0, 6));   // "a😀b"
    }

    #[test]
    fn utf16_range_empty_selection() {
        let text = "hello";
        assert_eq!(utf16_range_to_byte_range(text, 3, 3), (3, 3));
    }

    #[test]
    fn utf16_range_end_of_string() {
        let text = "abc";
        assert_eq!(utf16_range_to_byte_range(text, 0, 3), (0, 3));
    }

    // ── build_utf16_offset_map ──

    #[test]
    fn offset_map_ascii() {
        let text = "abc";
        let map = build_utf16_offset_map(text);
        assert_eq!(map.len(), 4); // 3 bytes + 1
        assert_eq!(map, vec![0, 1, 2, 3]);
    }

    #[test]
    fn offset_map_multibyte() {
        let text = "café";
        // bytes: c(0) a(1) f(2) é(3..4) -> 5 bytes
        let map = build_utf16_offset_map(text);
        assert_eq!(map.len(), 6); // 5 bytes + 1
        assert_eq!(map[0], 0); // 'c'
        assert_eq!(map[1], 1); // 'a'
        assert_eq!(map[2], 2); // 'f'
        assert_eq!(map[3], 3); // 'é' start
        assert_eq!(map[5], 4); // end of string
    }

    #[test]
    fn offset_map_emoji() {
        let text = "a😀b";
        // bytes: a(0) 😀(1..4) b(5) -> 6 bytes
        let map = build_utf16_offset_map(text);
        assert_eq!(map[0], 0); // 'a'
        assert_eq!(map[1], 1); // '😀' start -> utf16 pos 1
        assert_eq!(map[5], 3); // 'b' -> utf16 pos 3 (after surrogate pair)
        assert_eq!(map[6], 4); // end of string
    }

    // ── find_all_case_insensitive ──

    #[test]
    fn find_case_insensitive_basic() {
        let text = "Hello World hello";
        let matches = find_all_case_insensitive(text, "hello");
        assert_eq!(matches, vec![(0, 5), (12, 17)]);
    }

    #[test]
    fn find_case_insensitive_no_match() {
        let text = "Hello World";
        let matches = find_all_case_insensitive(text, "xyz");
        assert!(matches.is_empty());
    }

    #[test]
    fn find_case_insensitive_empty_needle() {
        let text = "Hello";
        let matches = find_all_case_insensitive(text, "");
        assert!(matches.is_empty());
    }

    #[test]
    fn find_case_insensitive_accented() {
        let text = "Café café CAFÉ";
        let matches = find_all_case_insensitive(text, "café");
        assert_eq!(matches.len(), 3);
        // All 3 occurrences found
        assert_eq!(&text[matches[0].0..matches[0].1], "Café");
        assert_eq!(&text[matches[1].0..matches[1].1], "café");
        assert_eq!(&text[matches[2].0..matches[2].1], "CAFÉ");
    }

    #[test]
    fn find_case_insensitive_overlapping() {
        let text = "aaa";
        let matches = find_all_case_insensitive(text, "aa");
        // "aa" at position 0 and "aa" at position 1
        assert_eq!(matches, vec![(0, 2), (1, 3)]);
    }

    #[test]
    fn find_case_insensitive_emoji_context() {
        let text = "a😀Hello😀hello";
        let matches = find_all_case_insensitive(text, "hello");
        assert_eq!(matches.len(), 2);
        assert_eq!(&text[matches[0].0..matches[0].1], "Hello");
        assert_eq!(&text[matches[1].0..matches[1].1], "hello");
    }

    #[test]
    fn find_case_insensitive_single_char() {
        let text = "AaBb";
        let matches = find_all_case_insensitive(text, "a");
        assert_eq!(matches, vec![(0, 1), (1, 2)]);
    }

    #[test]
    fn find_case_insensitive_full_string() {
        let text = "Hello";
        let matches = find_all_case_insensitive(text, "HELLO");
        assert_eq!(matches, vec![(0, 5)]);
    }

    // ── detect_and_decode ──

    #[test]
    fn decode_utf8_plain() {
        let bytes = "Bonjour café".as_bytes();
        let (text, enc) = detect_and_decode(bytes);
        assert_eq!(text, "Bonjour café");
        assert_eq!(enc, "UTF-8");
    }

    #[test]
    fn decode_utf8_bom() {
        let mut bytes = vec![0xEF, 0xBB, 0xBF];
        bytes.extend_from_slice("Hello".as_bytes());
        let (text, enc) = detect_and_decode(&bytes);
        assert_eq!(text, "Hello");
        assert_eq!(enc, "UTF-8 BOM");
    }

    #[test]
    fn decode_utf16_le_bom() {
        // "AB" in UTF-16 LE with BOM
        let bytes: Vec<u8> = vec![0xFF, 0xFE, 0x41, 0x00, 0x42, 0x00];
        let (text, enc) = detect_and_decode(&bytes);
        assert_eq!(text, "AB");
        assert_eq!(enc, "UTF-16 LE");
    }

    #[test]
    fn decode_utf16_be_bom() {
        // "AB" in UTF-16 BE with BOM
        let bytes: Vec<u8> = vec![0xFE, 0xFF, 0x00, 0x41, 0x00, 0x42];
        let (text, enc) = detect_and_decode(&bytes);
        assert_eq!(text, "AB");
        assert_eq!(enc, "UTF-16 BE");
    }

    #[test]
    fn decode_windows_1252_fallback() {
        // "café" in Windows-1252: c=0x63, a=0x61, f=0x66, é=0xE9
        let bytes: Vec<u8> = vec![0x63, 0x61, 0x66, 0xE9];
        let (text, enc) = detect_and_decode(&bytes);
        assert_eq!(text, "café");
        assert_eq!(enc, "Windows-1252");
    }

    #[test]
    fn decode_windows_1252_special_chars() {
        // Windows-1252 has special chars in 0x80-0x9F range that Latin-1 doesn't
        // 0x93 = left double quotation mark "
        // 0x94 = right double quotation mark "
        let bytes: Vec<u8> = vec![0x93, 0x48, 0x69, 0x94]; // "Hi"
        let (text, enc) = detect_and_decode(&bytes);
        assert_eq!(text, "\u{201C}Hi\u{201D}");
        assert_eq!(enc, "Windows-1252");
    }

    #[test]
    fn decode_empty() {
        let bytes: Vec<u8> = vec![];
        let (text, enc) = detect_and_decode(&bytes);
        assert_eq!(text, "");
        assert_eq!(enc, "UTF-8");
    }

    // ── is_probably_text_file ──────────────────────────────────────────────
    //
    // Heuristic used by drag-and-drop to decide whether to offer "Insert as
    // text" or "Attach as binary". A wrong answer is not a crash but a bad UX:
    // attaching .txt as binary, or mangling PNG bytes as text. These tests
    // cover each branch: extension fast-path, BOM, nul-byte, UTF-8 sniff,
    // incomplete multi-byte tail, and the not-openable fallback.

    fn write_tmp(suffix: &str, bytes: &[u8]) -> std::path::PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        use std::time::SystemTime;
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let ts = SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let p = std::env::temp_dir()
            .join(format!("locknote_sniff_{}_{}{}", ts, n, suffix));
        std::fs::write(&p, bytes).unwrap();
        p
    }

    #[test]
    fn sniff_binary_extension_shortcircuits() {
        // Content is pure ASCII but the extension is blacklisted — must be
        // classified as binary without even opening the file.
        let p = write_tmp(".png", b"hello world, definitely text inside");
        assert!(!is_probably_text_file(&p));
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn sniff_extension_case_insensitive() {
        let p = write_tmp(".ZIP", b"text content");
        assert!(!is_probably_text_file(&p));
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn sniff_plain_utf8_is_text() {
        let p = write_tmp(".txt", "Bonjour café 日本語".as_bytes());
        assert!(is_probably_text_file(&p));
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn sniff_no_extension_utf8_is_text() {
        // No extension at all (README, Makefile...) must still sniff as text.
        let p = write_tmp("", b"plain ascii content without extension");
        assert!(is_probably_text_file(&p));
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn sniff_empty_file_is_text() {
        let p = write_tmp(".unknown", b"");
        assert!(is_probably_text_file(&p));
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn sniff_utf8_bom_is_text() {
        let mut bytes = vec![0xEF, 0xBB, 0xBF];
        bytes.extend_from_slice(b"hello");
        let p = write_tmp(".unknown", &bytes);
        assert!(is_probably_text_file(&p));
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn sniff_utf16_le_bom_is_text() {
        // UTF-16 LE: 0xFF 0xFE is the BOM. After the BOM there are nul bytes,
        // but BOM detection runs BEFORE the nul-byte check so this must still
        // be classified as text.
        let bytes = vec![0xFF, 0xFE, 0x41, 0x00, 0x42, 0x00];
        let p = write_tmp(".unknown", &bytes);
        assert!(is_probably_text_file(&p));
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn sniff_utf16_be_bom_is_text() {
        let bytes = vec![0xFE, 0xFF, 0x00, 0x41, 0x00, 0x42];
        let p = write_tmp(".unknown", &bytes);
        assert!(is_probably_text_file(&p));
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn sniff_embedded_nul_byte_is_binary() {
        // No BOM, contains a nul byte → binary.
        let bytes = b"hello\0world".to_vec();
        let p = write_tmp(".unknown", &bytes);
        assert!(!is_probably_text_file(&p));
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn sniff_invalid_utf8_is_binary() {
        // No BOM, no nul byte, but bytes that never form valid UTF-8:
        // 0xC0 and 0xC1 are forbidden lead bytes, 0xF5/0xF6 are out of range.
        // UTF-8 sniff returns Err with a definite error_len → classified as binary.
        // (Avoid the 0xFF 0xFE prefix which IS a UTF-16 LE BOM.)
        let bad: Vec<u8> = vec![0xC0, 0xC1, 0xF5, 0xF6, 0x41, 0x42];
        let p = write_tmp(".unknown", &bad);
        assert!(!is_probably_text_file(&p));
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn sniff_utf8_with_split_multibyte_tail_is_text() {
        // Valid UTF-8 prefix, then a single incomplete multi-byte head byte at
        // the very end (simulates an 8 KiB boundary that split a character).
        // The function must accept this via the error_len().is_none() branch.
        let mut bytes = b"valid text content ".to_vec();
        bytes.push(0xC3); // start of a 2-byte UTF-8 sequence, missing its continuation
        let p = write_tmp(".unknown", &bytes);
        assert!(
            is_probably_text_file(&p),
            "truncated trailing multi-byte sequence must still count as text"
        );
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn sniff_nonexistent_path_is_binary() {
        // File.open fails → fall through to `return false` (safer default:
        // if we can't read it, don't try to paste it as text).
        let p = std::env::temp_dir().join("locknote_sniff_does_not_exist_xyz.txt");
        assert!(!p.exists());
        assert!(!is_probably_text_file(&p));
    }

    // ── highlight_term ──

    #[test]
    fn highlight_rejects_too_long() {
        // find_all_case_insensitive is O(n·m): a 1 MB base64 line selected by
        // double-click must not be searched on every timer tick.
        assert_eq!(highlight_term(&"a".repeat(300)), None);
        let max = "b".repeat(256);
        assert_eq!(highlight_term(&max), Some(max.as_str()));
    }

    #[test]
    fn highlight_rules_unchanged() {
        assert_eq!(highlight_term("a"), None);
        assert_eq!(highlight_term("a\nb"), None);
        assert_eq!(highlight_term(" ab"), None);
        assert_eq!(highlight_term("ab "), None);
        assert_eq!(highlight_term("ab"), Some("ab"));
        assert_eq!(highlight_term("é"), Some("é")); // 2 bytes, as before
    }

    // ── double-click target (issue: double-click not 100 %) ────────────────

    #[test]
    fn utf16_range_caret_at_line_start_crlf() {
        assert_eq!(utf16_range_to_byte_range("ab\r\ncd", 3, 3), (4, 4));
        assert_eq!(utf16_range_to_byte_range("ab\r\ncd", 2, 3), (2, 4));
    }

    #[test]
    fn dblclick_target_caret_line_start_crlf() {
        assert_eq!(dblclick_target("ab\r\ncd ef", 3, 3), Some((3, 5)));
    }

    #[test]
    fn dblclick_target_crlf_word() {
        assert_eq!(dblclick_target("ab\r\ncd ef", 3, 6), Some((3, 5)));
    }

    #[test]
    fn dblclick_target_emoji() {
        assert_eq!(dblclick_target("a\u{1F600}bc d", 3, 5), Some((0, 5)));
    }

    #[test]
    fn dblclick_target_already_whole_word_is_none() {
        assert_eq!(dblclick_target("foo bar", 0, 3), None);
    }

    #[test]
    fn utf16_len_counts_astral() {
        assert_eq!(utf16_len("\u{1F600}a"), 3);
        assert_eq!(utf16_len("ab\r\ncd"), 5);
        assert_eq!(utf16_len(""), 0);
    }

    #[test]
    fn richedit_notification_layouts_match_richedit_h() {
        use std::mem::{offset_of, size_of};
        assert_eq!(offset_of!(MsgFilter, msg), 24);
        assert_eq!(offset_of!(MsgFilter, wparam), 28);
        assert_eq!(offset_of!(MsgFilter, lparam), 36);
        assert_eq!(size_of::<MsgFilter>(), 44);
        assert_eq!(offset_of!(EnLink, msg), 24);
        assert_eq!(offset_of!(EnLink, wparam), 28);
        assert_eq!(offset_of!(EnLink, lparam), 36);
        assert_eq!(offset_of!(EnLink, chrg_min), 44);
        assert_eq!(offset_of!(EnLink, chrg_max), 48);
        assert_eq!(size_of::<EnLink>(), 52);
    }

    #[test]
    fn menu_label_escapes_ampersand() {
        assert_eq!(super::tab_ops::menu_label("R&D"), "R&&D");
        assert_eq!(super::tab_ops::menu_label("plain"), "plain");
    }

    // ── expand_word_selection ──────────────────────────────────────────────

    fn expand_str<'a>(text: &'a str, start: usize, end: usize) -> Option<&'a str> {
        expand_word_selection(text, start, end).map(|(s, e)| &text[s..e])
    }

    #[test]
    fn expand_fullword_from_any_segment() {
        // RichEdit splits "CT6A61EN" at every letter↔digit transition — each
        // segment must widen back to the full run regardless of which one
        // the double-click landed on.
        let text = "CT6A61EN";
        assert_eq!(expand_str(text, 0, 2), Some("CT6A61EN"));
        assert_eq!(expand_str(text, 2, 3), Some("CT6A61EN"));
        assert_eq!(expand_str(text, 3, 4), Some("CT6A61EN"));
        assert_eq!(expand_str(text, 4, 6), Some("CT6A61EN"));
        assert_eq!(expand_str(text, 6, 8), Some("CT6A61EN"));
    }

    #[test]
    fn trailing_space_in_initial_selection_does_not_bridge_to_next_word() {
        // Reproduces the bug where RichEdit's word-select included the trailing
        // space, causing the expansion to jump onto the next word.
        let text = "CT6A61EN dddd";
        assert_eq!(expand_str(text, 0, 9), Some("CT6A61EN")); // "CT6A61EN "
        assert_eq!(expand_str(text, 9, 13), Some("dddd"));
    }

    #[test]
    fn expansion_stays_within_clicked_word_for_plain_text() {
        let text = "hello world";
        assert_eq!(expand_str(text, 0, 5), Some("hello"));
        assert_eq!(expand_str(text, 6, 11), Some("world"));
    }

    // Spec change 2026-09-30 (issue 045): a word is delimited by whitespace
    // only; inner punctuation is part of it.
    #[test]
    fn expansion_spans_inner_punctuation() {
        let text = "foo.bar-baz_qux";
        assert_eq!(expand_str(text, 0, 3), Some("foo.bar-baz_qux"));
        assert_eq!(expand_str(text, 4, 7), Some("foo.bar-baz_qux"));
        assert_eq!(expand_str(text, 12, 15), Some("foo.bar-baz_qux"));
    }

    #[test]
    fn expand_email_in_sentence() {
        assert_eq!(expand_str("mail user@x.com fin", 7, 11), Some("user@x.com"));
    }

    // Spec change 2026-09-30 (issue 045, revised after user test): a word
    // is exactly the text between whitespace, nothing is stripped — the
    // editor mostly holds secrets, and any stripping rule truncates some
    // password (`Pass@word1!`, `abc)`, `"xyz"`).
    #[test]
    fn password_with_trailing_punctuation_is_whole() {
        assert_eq!(expand_str("mdp Secr3t@mail2000! ok", 4, 9), Some("Secr3t@mail2000!"));
        assert_eq!(expand_str("Secr3t@mail2000!", 15, 16), Some("Secr3t@mail2000!"));
    }

    #[test]
    fn nothing_is_stripped() {
        for w in ["((foo)),", "‘x’", "“mot”", "'quoted'", "\"dq\"", "«x»", "<user@x.com>",
                  "user@x.com.", r"C:\x\y.txt.", r"C:\dir\", ".bashrc", "fin.", "oui,", "quoi?",
                  "non!", "etc…", "abc)", "http://a/a_(b)", "e.g.", "...", "->", "-v", "#tag",
                  "$HOME", "%PATH%", "l'école", "aujourd’hui"] {
            // Double-click on the first character (a char boundary, as
            // produced by utf16_range_to_byte_range in the editor).
            let first = w.chars().next().unwrap().len_utf8();
            assert_eq!(expand_str(w, 0, first), Some(w), "{}", w);
        }
    }

    #[test]
    fn whitespace_classes() {
        assert_eq!(expand_str("a\tb", 0, 1), Some("a"));
        assert_eq!(expand_str("a\tb", 2, 3), Some("b"));
        assert_eq!(expand_str("a\u{00A0}b", 0, 1), Some("a"));
        assert_eq!(expand_str("a\u{200B}b", 0, 1), Some("a\u{200B}b"));
    }
    #[test]
    fn expansion_includes_unicode_alphabetics() {
        // 'é' is is_alphanumeric → must be absorbed into the run.
        let text = "café123";
        assert_eq!(expand_str(text, 0, 3), Some("café123"));
    }

    #[test]
    fn expansion_stops_at_newline() {
        let text = "line1\nline2";
        assert_eq!(expand_str(text, 0, 5), Some("line1"));
        assert_eq!(expand_str(text, 6, 11), Some("line2"));
    }

    #[test]
    fn boundary_selections_at_start_and_end_of_text() {
        let text = "abc123";
        assert_eq!(expand_str(text, 0, 3), Some("abc123"));
        assert_eq!(expand_str(text, 3, 6), Some("abc123"));
    }

    #[test]
    fn empty_caret_inside_alphanumeric_run_expands_to_full_word() {
        // Double-click can land on a character boundary and produce an
        // empty selection; we should still recover the surrounding word.
        let text = "hello";
        assert_eq!(expand_str(text, 3, 3), Some("hello"));
        assert_eq!(expand_str(text, 0, 0), Some("hello"));
        assert_eq!(expand_str(text, 5, 5), Some("hello"));
    }

    #[test]
    fn empty_caret_between_spaces_returns_none() {
        // Caret sitting in whitespace — nothing to expand in either direction.
        let text = "foo   bar";
        assert!(expand_word_selection(text, 4, 4).is_none()); // between spaces
        // Between dots is inside a whitespace-delimited word now.
        assert_eq!(expand_str("foo..bar", 4, 4), Some("foo..bar"));
    }

    #[test]
    fn invalid_range_returns_none() {
        assert!(expand_word_selection("hello", 5, 3).is_none());
        assert!(expand_word_selection("abc", 2, 10).is_none());
    }

    #[test]
    fn whitespace_only_selection_returns_none() {
        // Double-click on a run of spaces between two words.
        let text = "hello   world";
        assert!(expand_word_selection(text, 5, 8).is_none());
    }

    #[test]
    fn both_leading_and_trailing_whitespace_are_trimmed_before_expansion() {
        // RichEdit occasionally returns a selection padded on both sides.
        let text = "hello world foo";
        assert_eq!(expand_str(text, 5, 12), Some("world")); // " world "
    }

    // ── status bar and gutter layout (content.rs) ──

    #[test]
    fn gutter_width_grows_with_digits_and_has_a_floor() {
        assert_eq!(gutter_width(0), 40);
        assert_eq!(gutter_width(9), 40);
        assert_eq!(gutter_width(99), 40);
        assert_eq!(gutter_width(999), 10 * 3 + 14 + 4);
        assert_eq!(gutter_width(10_000), 10 * 5 + 14 + 4);
    }

    #[test]
    fn gutter_numbers_end_14px_before_the_gutter_edge() {
        assert_eq!(gutter_number_right(40), 26);
        assert_eq!(gutter_number_right(gutter_width(12345)), gutter_width(12345) - 14);
    }

    #[test]
    fn save_indicator_text_follows_the_modified_flag() {
        assert_eq!(save_indicator_text(false), "status.saved");
        assert_eq!(save_indicator_text(true), "status.modified");
    }

    #[test]
    fn status_layout_flows_left_items_from_the_margin() {
        let l = status_layout(1000, &[60, 80, 40], 50, 70, None, None);
        assert_eq!(l.left, vec![12, 12 + 60 + 20, 12 + 60 + 20 + 80 + 20]);
        assert_eq!(l.info_x, None);
        assert_eq!(l.path, None);
    }

    #[test]
    fn status_layout_right_group_is_ordered_and_ends_at_the_right_margin() {
        let l = status_layout(1000, &[60], 50, 70, Some(45), None);
        let info_x = l.info_x.expect("info present");
        // Right to left: info, Encrypted (text then padlock), Saved (text then dot).
        assert_eq!(info_x + 45, 1000 - 12);
        assert!(l.encrypted_text_x + 70 < info_x);
        assert!(l.lock_x + STATUS_LOCK_WIDTH < l.encrypted_text_x);
        assert!(l.saved_text_x + 50 < l.lock_x);
        assert!(l.dot_x + STATUS_DOT_SIZE < l.saved_text_x);
    }

    #[test]
    fn status_layout_without_info_ends_with_the_encrypted_text() {
        let l = status_layout(800, &[], 50, 70, None, None);
        assert_eq!(l.encrypted_text_x + 70, 800 - 12);
    }

    // Issue 057: the path of the LockNote file, between the counters and the
    // save indicator.

    #[test]
    fn status_path_starts_after_the_counters_and_keeps_its_width_when_it_fits() {
        let l = status_layout(1200, &[60, 80, 40], 50, 70, Some(45), Some(300));
        let p = l.path.expect("room enough");
        assert_eq!(p.x, 12 + 60 + 20 + 80 + 20 + 40 + 20);
        assert_eq!(p.w, 300);
        assert!(p.x + p.w + 20 <= l.dot_x);
    }

    #[test]
    fn status_path_without_counters_starts_at_the_margin() {
        let l = status_layout(1200, &[], 50, 70, None, Some(100));
        assert_eq!(l.path, Some(PathSlot { x: 12, w: 100 }));
    }

    #[test]
    fn status_path_is_narrowed_to_the_room_left() {
        let l = status_layout(700, &[60, 80, 40], 50, 70, Some(45), Some(2000));
        let p = l.path.expect("some room");
        assert_eq!(p.x + p.w + 20, l.dot_x, "ends one gap before the save dot");
        assert!(p.w >= STATUS_PATH_MIN_WIDTH && p.w < 2000);
    }

    #[test]
    fn status_path_is_hidden_below_the_minimum_width() {
        // Room for the path = dot_x - 20 - start; find widths around 60 px.
        let base = status_layout(1000, &[60], 50, 70, None, None);
        let start = 12 + 60 + 20;
        let room = base.dot_x - 20 - start;
        let narrower = 1000 - (room - (STATUS_PATH_MIN_WIDTH - 1));
        assert_eq!(status_layout(narrower, &[60], 50, 70, None, Some(500)).path, None);
        let just_enough = 1000 - (room - STATUS_PATH_MIN_WIDTH);
        assert_eq!(
            status_layout(just_enough, &[60], 50, 70, None, Some(500)).path,
            Some(PathSlot { x: start, w: STATUS_PATH_MIN_WIDTH })
        );
    }

    #[test]
    fn status_path_moves_nothing_else() {
        for w in [300, 700, 1200] {
            let without = status_layout(w, &[60, 80, 40], 50, 70, Some(45), None);
            let with = status_layout(w, &[60, 80, 40], 50, 70, Some(45), Some(400));
            assert_eq!(StatusLayout { path: None, ..with }, without, "width {}", w);
        }
    }

    #[test]
    fn status_path_slot_hit_test() {
        let p = PathSlot { x: 100, w: 50 };
        assert!(!p.contains(99));
        assert!(p.contains(100));
        assert!(p.contains(149));
        assert!(!p.contains(150));
    }

    // ── plain_key_action (handle_shortcut without Ctrl/Shift) ──

    /// Issue #24: the Tabs menu shows F2 for "Rename Tab...", so F2 must
    /// reach the rename action like the menu item.
    #[test]
    fn f2_is_routed_to_tab_rename() {
        assert_eq!(plain_key_action(0x71), Some(PlainKey::RenameTab));
    }

    #[test]
    fn plain_keys_keep_their_actions() {
        assert_eq!(plain_key_action(0x74), Some(PlainKey::InsertTimestamp));
        assert_eq!(plain_key_action(0x1B), Some(PlainKey::Escape));
        // Plain letters go to the editor.
        assert_eq!(plain_key_action(0x41), None);
    }

    // ── count_text / attach_limit_message (issue #25) ──

    /// Pass 2: the tab colour submenu showed the stored names ("blue").
    #[test]
    fn tab_color_labels_are_translated() {
        use crate::features::i18n::{set_language, Language};
        let _g = crate::features::i18n::test_support::lang_guard();
        for lang in [Language::English, Language::French, Language::Spanish, Language::German] {
            set_language(lang);
            for name in crate::theme::tab_color_names() {
                let label = tab_color_label(name);
                assert!(!label.starts_with('[') && label != name, "{:?} {} -> {}", lang, name, label);
            }
        }
        set_language(Language::French);
        let blue = tab_color_label("blue");
        set_language(Language::English);
        assert_eq!(blue, "Bleu");
    }

    #[test]
    fn count_text_uses_the_singular_key_for_one_only() {
        let _g = crate::features::i18n::test_support::lang_guard();
        crate::features::i18n::set_language(crate::features::i18n::Language::French);
        assert_eq!(count_text(1, "status.word", "status.words"), "1 mot");
        assert_eq!(count_text(3, "status.word", "status.words"), "3 mots");
        // French: 0 takes the singular ("0 mot", pass 2).
        let fr_zero = count_text(0, "status.word", "status.words");
        crate::features::i18n::set_language(crate::features::i18n::Language::English);
        assert_eq!(count_text(0, "status.line", "status.lines"), "0 lines");
        assert_eq!(fr_zero, "0 mot");
    }

    /// Pass 4, M1: the export writes a text read after the last question,
    /// only if no lock came in between, and every plaintext buffer is wiped.
    #[test]
    fn the_export_reads_the_text_after_the_last_question_and_wipes_it() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src").join("ui").join("editor");
        let src = std::fs::read_to_string(dir.join("file_ops.rs")).unwrap();
        let start = src.find("pub(super) fn on_export(&self)").unwrap();
        let body = &src[start..start + src[start..].find("\n    }\n").unwrap()];
        let last_question = body.rfind("self.confirm(").expect("confirm");
        let last_read = body.rfind("get_content()").expect("get_content");
        assert!(last_read > last_question, "the text written is read before the question");
        assert!(body.contains("still_valid(epoch)"), "no lock epoch check before writing");
        assert!(!body.contains("let content = self.get_content();"), "plain String of the note");
        assert!(body.contains("let _wiped = Zeroizing::new(bytes);"), "the bytes of the lossy check are not wiped");
        assert!(body.contains("Zeroizing::new(encoding.encode(&content).0)"), "the bytes written are not wiped");
    }

    /// Pass 4: a secret copied in the 2FA setup arms the clipboard guard:
    /// the setup hands it back and on_security arms it.
    #[test]
    fn the_2fa_setup_copy_arms_the_clipboard_guard() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src").join("ui");
        let sec = std::fs::read_to_string(root.join("dialogs").join("security_dialog.rs")).unwrap();
        assert!(sec.contains("TotpSetupDialog::show(\"LockNote 2FA\", None, &mut setup_copied)"));
        assert!(sec.contains("*copied_out = copied.borrow_mut().take();"));
        let prefs = std::fs::read_to_string(root.join("editor").join("prefs_ops.rs")).unwrap();
        let at = prefs.find("SecurityDialog::show(&current, &mut copied)").expect("copied passed");
        assert!(prefs[at..].find("self.arm_clipboard_guard(copied);").map_or(false, |i| i < 400));
    }

    /// Pass 6: under the curtain the window behaves as locked for display
    /// and input. One predicate, used by every input entry point and by the
    /// layout.
    #[test]
    fn the_curtain_blocks_input_and_layout_like_the_lock() {
        use super::modal_guard::Curtain;
        assert!(!input_blocked_by(false, Curtain::None));
        assert!(input_blocked_by(true, Curtain::None));
        assert!(input_blocked_by(false, Curtain::Up));
        assert_eq!(layout_mode(true), LayoutMode::OverlayOnly);
        assert_eq!(layout_mode(false), LayoutMode::Full);
    }

    /// Pass 6: every input entry point and the layout ask `input_blocked()`.
    /// Fails if one of them goes back to `is_locked` alone.
    #[test]
    fn every_input_entry_point_uses_the_blocked_predicate() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src").join("ui").join("editor");
        let body_of = |file: &str, fun: &str| -> String {
            let src = std::fs::read_to_string(dir.join(file)).unwrap();
            let start = src.find(fun).unwrap_or_else(|| panic!("{} in {}", fun, file));
            let end = src[start..].find("\n    }\n").map_or(src.len(), |e| start + e);
            src[start..end].to_string()
        };
        assert!(body_of("edit_ops.rs", "pub(super) fn handle_shortcut").contains("if self.input_blocked()"));
        assert!(body_of("layout.rs", "pub(super) fn layout_controls").contains("layout_mode(self.input_blocked())"));
        assert!(body_of("panel_grip.rs", "fn note_is_locked").contains("self.input_blocked()"));
        let ev = std::fs::read_to_string(dir.join("events.rs")).unwrap();
        // Menu handler, drop handler, editor WM_KEYDOWN and WM_CHAR.
        assert!(ev.matches("f.input_blocked()").count() >= 2, "menu and drop");
        assert!(ev.matches("f_ed.input_blocked()").count() >= 2, "editor keys and chars");
        assert!(!ev.contains("if *f_ed.is_locked.borrow()"), "editor key handler on is_locked alone");
    }

    /// Pass 6: the "save failed" curtain lifts only with the password held
    /// in memory (compared in constant time), never with an empty one.
    #[test]
    fn the_save_failed_curtain_lifts_only_with_the_password() {
        assert!(curtain_password_ok("s3cret", "s3cret"));
        assert!(!curtain_password_ok("s3cre", "s3cret"));
        assert!(!curtain_password_ok("", ""));
        assert!(!curtain_password_ok("x", ""));
    }

    /// Pass 7/8: the activation focuses the password field of the curtain.
    #[test]
    fn activation_focuses_the_curtain_password_field() {
        use super::modal_guard::Curtain;
        assert!(focus_password_field(true, Curtain::None));
        assert!(focus_password_field(false, Curtain::Up));
        assert!(!focus_password_field(false, Curtain::None));
        let ev = std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src").join("ui").join("editor").join("events.rs")).unwrap();
        assert_eq!(ev.matches("focus_password_field(*f2.is_locked.borrow(), f2.curtain.get())").count(), 2, "WM_ACTIVATE and WM_SETFOCUS");
    }

    /// Pass 7, finding 4: wrong passwords on the "save failed" curtain get
    /// the lock screen's attempt counter and backoff (it was a free
    /// password oracle), and never end the program (unsaved changes behind).
    #[test]
    fn wrong_curtain_passwords_are_slowed_down_like_the_lock_screen() {
        let delays: Vec<u32> = (1..=8).map(curtain_wrong_password_delay).collect();
        assert_eq!(delays, [0, 1, 2, 4, 8, 8, 8, 8]);
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src").join("ui").join("editor");
        let lock = std::fs::read_to_string(dir.join("lock_ops.rs")).unwrap();
        let start = lock.find("pub(super) fn lift_curtain(").unwrap();
        let body = &lock[start..start + lock[start..].find("\n    }\n").unwrap()];
        assert!(body.contains("self.lock_attempts.borrow_mut()"), "no attempt counter");
        assert!(body.contains("self.lock_backoff_timer.start()"), "no backoff");
        assert!(!body.contains("stop_thread_dispatch"), "must not end the program");
    }

    /// Pass 7, finding 4: the comparison does not stop early on a length
    /// mismatch.
    #[test]
    fn the_constant_time_comparison_has_no_early_return() {
        use crate::ui::dialogs::confirm_password::constant_time_eq;
        assert!(constant_time_eq(b"abc", b"abc"));
        assert!(!constant_time_eq(b"abc", b"abcd"));
        assert!(!constant_time_eq(b"abcd", b"abc"));
        assert!(!constant_time_eq(b"", b"a"));
        assert!(constant_time_eq(b"", b""));
        let src = std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src").join("ui").join("dialogs").join("confirm_password.rs")).unwrap();
        let start = src.find("pub(crate) fn constant_time_eq(").unwrap();
        let body = &src[start..start + src[start..].find("\n}\n").unwrap()];
        assert!(!body.contains("return false") && !body.contains("return true"), "early return in constant_time_eq");
    }

    /// Pass 6 (m6): the WM_MOUSEMOVE Windows sends after a dialog closes,
    /// cursor unchanged, is not user activity.
    #[test]
    fn a_mouse_move_without_moving_does_not_count_as_activity() {
        assert!(mouse_really_moved(None, 0x0010_0020));
        assert!(!mouse_really_moved(Some(0x0010_0020), 0x0010_0020));
        assert!(mouse_really_moved(Some(0x0010_0020), 0x0010_0021));
    }

    /// Pass 6: the wiring of the lock and dialog decisions. Each assert
    /// fails if the call is removed or moved back (reasoned per line).
    #[test]
    fn the_lock_and_dialog_decisions_are_wired() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src").join("ui").join("editor");
        let read = |f: &str| std::fs::read_to_string(dir.join(f)).unwrap();
        let body = |src: &str, fun: &str| -> String {
            let start = src.find(fun).unwrap_or_else(|| panic!("{}", fun));
            let end = src[start..].find("\n    }\n").map_or(src.len(), |e| start + e);
            src[start..end].to_string()
        };
        let lock = read("lock_ops.rs");
        // Pass 8: lock_flow::try_lock saves before lock_now, the only path
        // to do_lock (the wipe); the persistence guard is taken only in the
        // save and lock_now steps.
        let flow = read("lock_flow.rs");
        // lock_flow's functions are top-level: their body ends at a "}" in
        // column 0.
        let top = |src: &str, fun: &str| -> String {
            let start = src.find(fun).unwrap_or_else(|| panic!("{}", fun));
            let end = src[start..].find("\n}\n").map_or(src.len(), |e| start + e);
            src[start..end].to_string()
        };
        let try_lock = top(&flow, "fn try_lock<");
        let save = try_lock.find("s.save()").expect("try_lock saves");
        let lock_now = try_lock.find("s.lock_now()").expect("try_lock locks");
        assert!(save < lock_now, "lock before the save");
        assert!(try_lock.contains("Err(reason) =>"), "a failed save keeps the curtain");
        let do_lock = body(&lock, "pub(super) fn do_lock(");
        assert!(do_lock.contains("self.wipe_plaintext_stores()"));
        assert!(do_lock.contains("WARN lock: the shortened ssh-agent list was not saved"));
        // Pass 9: the curtain becoming the lock screen keeps its field.
        assert!(do_lock.contains("self.show_lock_overlay(curtain_was_up)"));
        assert!(body(&lock, "pub(super) fn show_lock_overlay(").contains("lock_screen_field(curtain_was_up,"));
        let guard = read("modal_guard.rs");
        // Pass 9: the unlock window is asked to close by the lock and named
        // on the curtain.
        assert!(top(&guard, "fn cancel_open_dialogs(").contains(".chain(crate::ui::dialogs::unlock::open_window())"));
        assert!(top(&guard, "pub(super) fn first_dialog_title(").contains("crate::ui::dialogs::unlock::open_window()"));
        assert!(body(&guard, "pub(super) fn run_pending_requests(").contains("lock_flow::tick("));
        assert!(body(&guard, "pub(super) fn request_lock(").contains("lock_flow::request("));
        let req = top(&flow, "pub(super) fn request<");
        let curtain = req.find("s.show_curtain()").expect("curtain at once");
        assert!(req.find("s.bump_epoch()").unwrap() < curtain, "epoch moved before the curtain");
        assert!(req.contains("s.cancel_dialogs()"));
        // Only two acquire() of the persistence guard in the lock path: the
        // save and lock_now steps.
        assert_eq!(guard.matches("persistence_lock::acquire()").count(), 2, "acquire() outside save / lock_now");
        assert!(body(&guard, "    fn save(&mut self)").contains("persistence_lock::acquire()"));
        assert!(body(&guard, "    fn lock_now(&mut self)").contains("persistence_lock::acquire()"));
        assert!(!flow.contains("persistence_lock::acquire()"));
        assert!(!do_lock.contains("persistence_lock::acquire()"));
        let unlock = body(&lock, "pub(super) fn unlock_from_disk(");
        let m = unlock.find("let _modal = self.modal();").expect("unlock window under a modal guard");
        assert!(m < unlock.find("show_with_parent(").unwrap());
        assert!(unlock.contains("self.refresh_totp_list();"), "TOTP panel after unlock_from_disk");
        let restore = body(&lock, "pub(super) fn restore_after_unlock(");
        assert!(restore.contains("        self.refresh_totp_list();"), "TOTP refresh not gated");
        assert!(!restore.contains("if self.totp_panel.is_visible() {\n            self.refresh_totp_list();"));
        let prefs = read("prefs_ops.rs");
        for fun in ["pub(super) fn on_security(", "pub(super) fn on_preferences("] {
            let b = body(&prefs, fun);
            assert!(b.contains("zeroize_settings_secrets(&mut current)"), "{} wipes its settings copy", fun);
            assert!(b.contains("still_valid(epoch)"), "{} re-checks the epoch", fun);
        }
        assert!(body(&lock, "pub(super) fn on_unlock_overlay(").contains("self.lift_curtain()"));
        let lift = body(&lock, "pub(super) fn lift_curtain(");
        assert!(lift.find("lock_backoff_remaining.get() > 0").unwrap() < lift.find("curtain_password_ok(").unwrap(), "backoff checked in the lift itself");
        assert!(lift.contains("lock_flow::lift("));
        let close = read("close_flow.rs");
        let on_close = body(&close, "pub(super) fn on_close(");
        assert!(on_close.contains("close_request(curtain, self.close_box_open.get(), self.modal_open())"), "quit refused under the curtain, close box kept");
        // Pass 9: the close flow's own boxes are shown through close_box, so
        // the close path waits for them instead of cancelling them.
        assert!(body(&close, "    fn ask(&mut self) -> crate::ui::dialogs::close_confirm::CloseConfirmResult {").contains("self.form.close_box(||"), "close prompt");
        assert!(body(&close, "    fn close_without_saving(&mut self, reason: &str) -> bool {").contains("self.form.close_box(||"), "save-failed question");
        assert!(body(&guard, "    fn offer_close_without_saving(&self) {").contains("self.close_box(||"), "stuck-close offer");
        assert!(guard.contains("fn own_close_box_open(&self) -> bool {
        self.form.close_box_open.get()"));
        // Answers returning after a curtain are harmless (one shared check).
        assert!(close.matches("self.form.still_valid(epoch)").count() >= 2, "close prompt and save-failed question");
        assert!(body(&guard, "    fn offer_close_without_saving(&self) {").contains("self.still_valid(epoch)"));
        assert!(body(&guard, "pub(super) fn still_valid(").contains("answer_still_valid("));
        assert!(top(&flow, "pub(super) fn answer_still_valid<").contains("s.curtain_up(), s.locked()) && !s.exiting()"));
    }

    /// Pass 9: no auto-save under the curtain (its failure box would cover
    /// the password field); the lock's own retry is the only save there.
    /// The result of the auto-save is handled, not discarded.
    #[test]
    fn auto_save_waits_under_the_curtain_and_handles_its_result() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src").join("ui").join("editor");
        let read = |f: &str| std::fs::read_to_string(dir.join(f)).unwrap();
        let events = read("events.rs");
        let start = events.find("if handle == f.autosave_timer.handle {").expect("auto-save tick");
        let tick = &events[start..start + 400];
        assert!(tick.contains("f.autosave_gate.tick(f.modal_open(), f.input_blocked())"), "{}", tick);
        let rt = read("runtime_ops.rs");
        let start = rt.find("pub(super) fn on_autosave(").unwrap();
        let end = start + rt[start..].find("
    }
").unwrap();
        let autosave = &rt[start..end];
        assert!(!autosave.contains("let _ = self.on_save();"), "auto-save result discarded");
        assert!(autosave.contains("WARN autosave: save failed"), "auto-save failure logged");
    }

    /// Pass 9: while a window keeps the lock waiting, the curtain says so
    /// (naming it), even after a failed save: that save is retried once the
    /// window is gone.
    #[test]
    fn the_curtain_names_the_window_it_waits_for_before_a_failed_save() {
        assert_eq!(curtain_subtitle(true, true), CurtainSubtitle::WaitingFor);
        assert_eq!(curtain_subtitle(true, false), CurtainSubtitle::WaitingFor);
        assert_eq!(curtain_subtitle(false, true), CurtainSubtitle::SaveFailed);
        assert_eq!(curtain_subtitle(false, false), CurtainSubtitle::Password);
    }

    /// The reason of a failed save at lock, shown once the curtain is lifted
    /// (the decision itself is tested in lock_flow).
    #[test]
    fn the_failed_save_reason_text_names_the_error() {
        let _g = crate::features::i18n::test_support::lang_guard();
        let t = lock_save_failed_text("disk full");
        assert!(t.contains("disk full") && !t.contains("{error}"), "{}", t);
    }

    /// Pass 5: the copy of the settings given to a dialog loses its TOTP
    /// secrets afterwards.
    #[test]
    fn settings_secrets_are_wiped_from_a_copy() {
        let mut s = crate::settings::Settings::default();
        s.totp_2fa_secret = "JBSWY3DPEHPK3PXP".into();
        s.totp_entries.push(crate::settings::TotpEntry { name: "x".into(), secret: "GEZDGNBVGY3TQOJQ".into() });
        zeroize_settings_secrets(&mut s);
        assert!(s.totp_2fa_secret.is_empty() && s.totp_entries.is_empty());
    }

    /// Pass 5: the dialogs that change the note re-check the lock epoch.
    #[test]
    fn note_changing_dialogs_recheck_the_lock_epoch() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src").join("ui").join("editor");
        for (file, fun) in [
            ("stego_ops.rs", "fn on_stego_hide"),
            ("stego_ops.rs", "fn on_stego_extract"),
            ("attach_ops.rs", "fn on_totp_add"),
            ("prefs_ops.rs", "fn on_security"),
            ("attach_ops.rs", "fn on_attach_open"),
            ("file_ops.rs", "fn on_export"),
            // Pass 9: the remaining dialog-result call sites.
            ("backup_ops.rs", "pub(super) fn on_backups("),
            ("prefs_ops.rs", "pub(super) fn on_temp_files("),
            ("print_ops.rs", "pub(super) fn on_print("),
        ] {
            let src = std::fs::read_to_string(dir.join(file)).unwrap();
            let start = src.find(fun).unwrap_or_else(|| panic!("{}", fun));
            let body = &src[start..start + src[start..].find("\n    }\n").unwrap()];
            assert!(body.contains("self.lock_epoch()") && body.contains("still_valid(epoch)"), "{} {}", file, fun);
        }
        let ev = std::fs::read_to_string(dir.join("events.rs")).unwrap();
        assert!(ev.contains("let choice = DropChoiceDialog::show(") && ev.contains("f.still_valid(epoch)"));
    }

    /// Pass 9: the lock fake decides "an answer after the curtain" with the
    /// production check that `still_valid` runs, not a copy of its own.
    #[test]
    fn the_fake_and_still_valid_share_one_check() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src").join("ui").join("editor");
        let read = |f: &str| std::fs::read_to_string(dir.join(f)).unwrap();
        let guard = read("modal_guard.rs");
        let start = guard.find("pub(super) fn still_valid(").unwrap();
        let still_valid = &guard[start..start + guard[start..].find("
    }
").unwrap()];
        assert!(still_valid.contains("lock_flow::answer_still_valid(&FormLockSteps { form: self }, epoch)"), "{}", still_valid);
        let flow = read("lock_flow.rs");
        let start = flow.find("fn answer_returns(").unwrap();
        let fake = &flow[start..start + flow[start..].find("
        }
").unwrap()];
        assert!(fake.contains("answer_still_valid(self, epoch_at_open)") && !fake.contains("answer_applies("), "{}", fake);
    }

    /// Pass 5: nothing is saved while locked (the stores are wiped, a
    /// deferred password wipe could leave a password to save them with).
    #[test]
    fn no_save_while_locked_or_exiting() {
        assert_eq!(save_allowed(false, false), Ok(()));
        assert_eq!(save_allowed(true, false), Err("locked"));
        assert_eq!(save_allowed(false, true), Err("exiting"));
    }

    /// Pass 4: a wipe deferred at lock is retried while locked, never after
    /// the unlock restored the notes.
    #[test]
    fn a_deferred_wipe_runs_only_while_locked_or_exiting() {
        assert!(wipe_retry_due(true, true, false));
        assert!(wipe_retry_due(true, false, true));
        assert!(!wipe_retry_due(true, false, false), "unlocked: the notes are back");
        assert!(!wipe_retry_due(false, true, false));
    }

    /// Pass 4, B2: a removal with an index read before a question does not
    /// panic once the list was emptied (lock) meanwhile.
    #[test]
    fn a_stale_index_removes_nothing_and_does_not_panic() {
        let mut v = vec![1, 2, 3];
        assert!(remove_checked(&mut v, 1, "x"));
        assert_eq!(v, [1, 3]);
        v.clear();
        assert!(!remove_checked(&mut v, 1, "x"));
    }

    /// Pass 3, B1: a wipe under a held borrow must not panic (abort in
    /// release); it is deferred and done once the borrow is gone.
    #[test]
    fn a_wipe_under_a_held_borrow_is_deferred_not_a_panic() {
        let cell = RefCell::new(String::from("secret"));
        let pending = Cell::new(false);
        {
            let _held = cell.borrow(); // a dialog holding it
            assert!(!wipe_or_defer(&cell, &pending, "password"));
            assert!(pending.get());
        }
        assert!(wipe_or_defer(&cell, &pending, "password"));
        assert!(!pending.get());
        assert!(cell.borrow().is_empty());
    }

    /// Pass 3: while locked, only File > Quit and Help > About stay enabled.
    /// Every menu field of the form is greyed by `set_menus_for_lock`,
    /// directly or through its parent menu.
    #[test]
    fn every_menu_but_quit_and_about_is_greyed_while_locked() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src").join("ui").join("editor");
        let fields = std::fs::read_to_string(dir.join("mod.rs")).unwrap();
        let lock = std::fs::read_to_string(dir.join("lock_ops.rs")).unwrap();
        let start = lock.find("fn set_menus_for_lock").expect("set_menus_for_lock");
        let body = &lock[start..start + lock[start..].find("\n    }\n").unwrap()];
        let parents = ["menu_edit", "menu_view", "menu_tab", "menu_tools", "menu_file_settings"];
        let mut missing = Vec::new();
        for line in fields.lines() {
            let l = line.trim();
            let Some(name) = l.strip_prefix("menu_").and_then(|r| r.split(':').next()) else { continue };
            if !(l.contains("nwg::MenuItem") || l.contains("nwg::Menu,")) {
                continue;
            }
            let name = format!("menu_{}", name);
            let kept = ["menu_file", "menu_help", "menu_file_quit", "menu_help_about"].contains(&name.as_str());
            let under_parent = parents.iter().any(|p| name.starts_with(&format!("{}_", p)));
            if !kept && !under_parent && !body.contains(&format!("self.{}.", name)) {
                missing.push(name);
            }
        }
        assert!(missing.is_empty(), "not greyed while locked: {:?}", missing);
    }

    /// Pass 5: every information box of the editor goes through `inform`
    /// (counted as a modal, owned by the main window). Pass 6: every
    /// `modal_*_message(` (error, warning, fatal...) too.
    #[test]
    fn every_main_window_message_goes_through_inform() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src").join("ui").join("editor");
        let needle = format!("modal_{}_message(", "info");
        let others: Vec<String> = ["error", "warning", "fatal"].iter().map(|k| format!("modal_{}_message(", k)).collect();
        let mut found = Vec::new();
        for e in std::fs::read_dir(&dir).unwrap() {
            let p = e.unwrap().path();
            let name = p.file_name().unwrap().to_string_lossy().to_string();
            if !name.ends_with(".rs") || name == "tests.rs" {
                continue;
            }
            let text = std::fs::read_to_string(&p).unwrap();
            for (i, line) in text.lines().enumerate() {
                let allowed = name == "modal_guard.rs" && line.trim() == format!("nwg::{}&self.window, title, content);", needle);
                if (line.contains(&needle) && !allowed) || others.iter().any(|o| line.contains(o.as_str())) {
                    found.push(format!("{}:{}: {}", name, i + 1, line.trim()));
                }
            }
        }
        assert!(found.is_empty(), "messages outside inform():\n{}", found.join("\n"));
    }

    /// Pass 4: every Yes/No question of the main window goes through
    /// `confirm` (lock epoch, locked and exiting re-checked after the box).
    /// Pass 5: no other way to show a box either (raw MessageBoxW, a task
    /// dialog, an alias of nwg's functions). The questions asked inside the
    /// dialogs of `src/ui/dialogs` (backups, SSH keys, temporary files...)
    /// only decide for that dialog, and "No" (what a lock answers) is
    /// harmless there; what they return to the note goes through the
    /// callers' epoch check.
    #[test]
    fn no_other_box_api_is_used_by_the_editor() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src").join("ui").join("editor");
        let banned = [
            format!("MessageBox{}", "W"),
            format!("MessageBox{}", "A"),
            format!("TaskDia{}", "log"),
            format!("modal_message {}", "as"),
            format!("message {}", "as "),
            format!("modal_info_message {}", "as"),
        ];
        let mut found = Vec::new();
        for e in std::fs::read_dir(&dir).unwrap() {
            let p = e.unwrap().path();
            let name = p.file_name().unwrap().to_string_lossy().to_string();
            if !name.ends_with(".rs") || name == "tests.rs" {
                continue;
            }
            let text = std::fs::read_to_string(&p).unwrap();
            for (i, line) in text.lines().enumerate() {
                if banned.iter().any(|b| line.contains(b.as_str())) {
                    found.push(format!("{}:{}: {}", name, i + 1, line.trim()));
                }
            }
        }
        assert!(found.is_empty(), "other box APIs:\n{}", found.join("\n"));
    }

    #[test]
    fn every_main_window_question_goes_through_confirm() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src").join("ui").join("editor");
        let mut found = Vec::new();
        for e in std::fs::read_dir(&dir).unwrap() {
            let p = e.unwrap().path();
            let name = p.file_name().unwrap().to_string_lossy().to_string();
            if !name.ends_with(".rs") || name == "tests.rs" {
                continue;
            }
            let text = std::fs::read_to_string(&p).unwrap();
            for (i, line) in text.lines().enumerate() {
                let direct = line.contains(&format!("nwg::{}(", "message")) || line.contains(&format!("{}_message(", "modal"));
                let in_confirm = name == "modal_guard.rs" && line.trim() == "nwg::modal_message(&self.window, params)";
                if direct && !in_confirm {
                    found.push(format!("{}:{}: {}", name, i + 1, line.trim()));
                }
            }
        }
        assert!(found.is_empty(), "questions outside confirm():\n{}", found.join("\n"));
    }

    /// Pass 4: no dialog may be shown while a `Ref` of a store the wipe
    /// touches (password, attachments, tabs, settings) is alive: WM_ENDSESSION
    /// dispatched in its nested loop wipes them. A `let x = self.store.borrow()`
    /// is followed, brace by brace, to the end of its scope or its `drop(x)`;
    /// a dialog call before that is reported.
    #[test]
    fn no_dialog_holds_a_borrow_of_a_wiped_store() {
        let stores = ["password", "attachments", "tab_bar", "settings"];
        let dialogs = ["::show(", "::show_", "show_save_dialog(", "self.confirm(", ".run(Some(", "modal_info_message(", "_dialog("];
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src").join("ui").join("editor");
        let mut found = Vec::new();
        for e in std::fs::read_dir(&dir).unwrap() {
            let p = e.unwrap().path();
            let file = p.file_name().unwrap().to_string_lossy().to_string();
            if !file.ends_with(".rs") || file == "tests.rs" {
                continue;
            }
            let text = std::fs::read_to_string(&p).unwrap();
            for store in stores {
                for kind in [".borrow()", ".borrow_mut()"] {
                    let needle = format!("= self.{}{}", store, kind);
                    let mut from = 0;
                    while let Some(pos) = text[from..].find(&needle) {
                        let at = from + pos;
                        from = at + needle.len();
                        // `let NAME = self.store.borrow()` (a temporary used
                        // in one expression ends at its `;`).
                        let line_start = text[..at].rfind('\n').map_or(0, |i| i + 1);
                        let head = text[line_start..at].trim();
                        let Some(name) = head.strip_prefix("let ").map(|n| n.trim_start_matches("mut ").trim()) else { continue };
                        // Chained (`.borrow().clone()`...): no Ref is kept.
                        let after = &text[at + needle.len()..];
                        if !after.trim_start().starts_with(';') {
                            continue;
                        }
                        let mut depth = 0i32;
                        let rest = after;
                        let mut i = 0;
                        let bytes = rest.as_bytes();
                        while i < bytes.len() {
                            match bytes[i] {
                                b'{' => depth += 1,
                                b'}' => {
                                    depth -= 1;
                                    if depth < 0 {
                                        break;
                                    }
                                }
                                _ => {}
                            }
                            let Some(tail) = rest.get(i..) else {
                                i += 1;
                                continue;
                            };
                            if tail.starts_with(&format!("drop({})", name)) {
                                break;
                            }
                            if dialogs.iter().any(|d| tail.starts_with(d)) {
                                let line = text[..at].matches('\n').count() + 1;
                                found.push(format!("{}:{}: {} held across a dialog", file, line, name));
                                break;
                            }
                            i += 1;
                        }
                    }
                }
                // Inline: `show(&self.store.borrow()` and the like.
                for (i, line) in text.lines().enumerate() {
                    let inline = format!("self.{}.borrow", store);
                    if line.contains(&inline) && dialogs.iter().any(|d| line.contains(d)) {
                        found.push(format!("{}:{}: {}", file, i + 1, line.trim()));
                    }
                }
            }
        }
        assert!(found.is_empty(), "borrows held across a dialog:\n{}", found.join("\n"));
    }

    /// Pass 3, B1: no dialog may be shown while a borrow of the password is
    /// held: its nested loop can run WM_ENDSESSION or the lock, which wipe
    /// the password.
    #[test]
    fn no_dialog_holds_a_borrow_of_the_password() {
        let src = super::all_source();
        // Built here so this test does not find itself.
        let needle = format!("show(&self.password{}", ".borrow()");
        for (i, line) in src.lines().enumerate() {
            assert!(!line.contains(&needle), "line {}: {}", i + 1, line.trim());
        }
    }

    /// Pass 2: the settings read back at unlock (or from a backup) may differ
    /// from the menu: the check marks follow them.
    #[test]
    fn view_menu_checks_follow_the_settings() {
        let mut s = crate::settings::Settings::default();
        s.word_wrap = false;
        s.show_line_numbers = false;
        assert_eq!(view_menu_checks(&s), (false, false));
        s.word_wrap = true;
        assert_eq!(view_menu_checks(&s), (true, false));
        s.show_line_numbers = true;
        s.word_wrap = false;
        assert_eq!(view_menu_checks(&s), (false, true));
    }

    #[test]
    fn singular_rule_per_language() {
        use crate::features::i18n::Language::*;
        assert!(is_singular(French, 0) && is_singular(French, 1) && !is_singular(French, 2));
        for lang in [English, Spanish, German] {
            assert!(!is_singular(lang, 0) && is_singular(lang, 1) && !is_singular(lang, 2), "{:?}", lang);
        }
    }

    #[test]
    fn attach_limit_message_is_translated() {
        use crate::features::attachments::AttachmentError;
        let _g = crate::features::i18n::test_support::lang_guard();
        crate::features::i18n::set_language(crate::features::i18n::Language::German);
        let m = attach_limit_message(&AttachmentError::FileTooLarge { size: 2048, max: 1024 });
        crate::features::i18n::set_language(crate::features::i18n::Language::English);
        assert!(m.starts_with("Datei zu gro"), "{}", m);
        assert!(!m.contains('{'), "{}", m);
    }

    // ── line-number gutter shown or hidden (issue #26) ──

    #[test]
    fn hidden_line_numbers_take_no_width() {
        assert_eq!(shown_gutter_width(false, 1), 0);
        assert_eq!(shown_gutter_width(false, 12345), 0);
        assert_eq!(shown_gutter_width(true, 12345), gutter_width(12345));
    }

    #[test]
    fn the_editor_takes_the_width_of_a_hidden_gutter() {
        let shown = editor_columns(1000, 300, shown_gutter_width(true, 10) as u32);
        assert_eq!(shown, (40, 660));
        let hidden = editor_columns(1000, 300, shown_gutter_width(false, 10) as u32);
        assert_eq!(hidden, (0, 700));
        // Without a side panel the editor spans the whole client width.
        assert_eq!(editor_columns(800, 0, 0), (0, 800));
    }
