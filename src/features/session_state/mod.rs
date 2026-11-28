// Session state — remember the active tab and cursor line between sessions.
//
// Stored as the outermost block in the encrypted payload:
//
//   ...other content...
//   [LOCKNOTE_STATE]
//   active=2
//   line=42
//   agent_key=ssh-ed25519 AAAA...     (one per SSH key in ssh-agent, issue 054)
//   [/LOCKNOTE_STATE]
//
// Parsed before anything else; a missing block means "no prior session".

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SessionState {
    /// Index of the active tab at save time (0-based). Clamped on load.
    pub active_tab: usize,
    /// Zero-based line of the caret in the active tab at save time.
    pub cursor_line: u32,
    /// Public lines of the SSH keys this file loaded into ssh-agent and has
    /// not removed yet (issue 054). One `agent_key=` line each.
    pub agent_keys: Vec<String>,
}

use crate::crypto::wiping::WipingString;
use crate::features::markers::{find_marker_line, rfind_loose_marker_line, rfind_marker_line, STATE_FOOTER, STATE_HEADER};

/// One line per SSH key loaded into ssh-agent and not removed yet (issue 054).
const AGENT_KEY: &str = "agent_key=";

/// Serialize the session state. Returns an empty string when the state is
/// fully default — no point writing `active=0\nline=0` for a fresh file.
/// (`payload::compose` writes the block with `write_block`.)
#[cfg(test)]
pub fn serialize(state: &SessionState) -> String {
    let mut out = WipingString::new();
    write_block(&mut out, state);
    out.into_string()
}

/// True when the state is written at all (not fully default).
pub fn is_written(state: &SessionState) -> bool {
    !(state.active_tab == 0 && state.cursor_line == 0 && state.agent_keys.is_empty())
}

/// The session block, appended to `out` (nothing for a default state).
pub fn write_block(out: &mut WipingString, state: &SessionState) {
    if !is_written(state) {
        return;
    }
    out.push_str(STATE_HEADER);
    out.push('\n');
    out.push_str("active=");
    out.push_str(&state.active_tab.to_string());
    out.push('\n');
    out.push_str("line=");
    out.push_str(&state.cursor_line.to_string());
    out.push('\n');
    for key in &state.agent_keys {
        out.push_str(AGENT_KEY);
        out.push_str(key);
        out.push('\n');
    }
    out.push_str(STATE_FOOTER);
}

/// Where the parser sees a session block in `input`: byte positions of its
/// header and of its footer. The header is the last line that is the header
/// and nothing else, the footer the first such line after it, and the block
/// must end the text. Inside a line, or escaped in a tab or a vault value,
/// header and footer are text (issue 050).
fn block_span(input: &str) -> Option<(usize, usize)> {
    let start = rfind_marker_line(input, STATE_HEADER)?;
    let footer = find_marker_line(input, STATE_FOOTER, start + STATE_HEADER.len())?;
    // Only accept a block that is actually the tail of the payload —
    // anything non-whitespace after the footer means we matched stray text.
    if !input[footer + STATE_FOOTER.len()..].trim().is_empty() {
        return None;
    }
    Some((start, footer))
}

/// True when `parse` takes the end of `input` for a session block, or could
/// once `Settings::parse` has dropped the `\r` / `\n` opening the note text
/// (a header behind a lone `\r` there becomes a header line). For the
/// decision made when writing (`payload::compose`), not for reading.
///
/// The footer is the one `parse` would stop at: the last footer line, with
/// only whitespace after it. The header may be any place where the marker
/// stands between line bounds, `\r` alone included
/// (`markers::rfind_loose_marker_line`), as long as no other footer line
/// separates it from that footer. Every block `block_span` finds passes.
pub fn may_end_with_block(input: &str) -> bool {
    let Some(footer) = rfind_marker_line(input, STATE_FOOTER) else {
        return false;
    };
    if !input[footer + STATE_FOOTER.len()..].trim().is_empty() {
        return false;
    }
    let before = &input[..footer];
    let after_previous_footer = match rfind_marker_line(before, STATE_FOOTER) {
        Some(previous) => previous + STATE_FOOTER.len(),
        None => 0,
    };
    match rfind_loose_marker_line(before, STATE_HEADER) {
        Some(header) => header >= after_previous_footer,
        None => false,
    }
}

/// Parse a `[LOCKNOTE_STATE]` block from the end of `input`.
/// Returns `(Some(state), remaining)` when a valid block is found at the
/// tail, otherwise `(None, input)` unchanged.
pub fn parse(input: &str) -> (Option<SessionState>, String) {
    let Some((start, footer)) = block_span(input) else {
        return (None, input.to_string());
    };

    let body = &input[start + STATE_HEADER.len()..footer];
    let mut state = SessionState::default();
    for line in body.lines() {
        if let Some(rest) = line.strip_prefix("active=") {
            state.active_tab = rest.trim().parse().unwrap_or(0);
        } else if let Some(rest) = line.strip_prefix("line=") {
            state.cursor_line = rest.trim().parse().unwrap_or(0);
        } else if let Some(rest) = line.strip_prefix(AGENT_KEY) {
            let key = rest.trim();
            if !key.is_empty() {
                state.agent_keys.push(key.to_string());
            }
        }
    }

    let before = input[..start].trim_end_matches('\n').trim_end_matches('\r');
    (Some(state), before.to_string())
}

/// Append `state` as a `[LOCKNOTE_STATE]` block to `data` (what
/// `payload::compose` writes, for the tests that build a payload block by
/// block).
#[cfg(test)]
pub fn combine(data: &str, state: &SessionState) -> String {
    let block = serialize(state);
    if block.is_empty() {
        return data.to_string();
    }
    if data.is_empty() {
        return block;
    }
    format!("{}\n{}", data, block)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn may_end_with_block_covers_what_parse_takes_and_the_lone_cr_headers() {
        let block = "[LOCKNOTE_STATE]\nactive=1\nline=2\n[/LOCKNOTE_STATE]";
        let crlf = block.replace('\n', "\r\n");
        // (text, taken by parse as written, decision)
        let cases: Vec<(String, bool, bool)> = vec![
            (block.to_string(), true, true),
            (format!("a\n{}", block), true, true),
            (format!("a\r\n{}\r\n\r\n \t", crlf), true, true),
            (format!("{}\n{}", block, block), true, true),
            ("[LOCKNOTE_STATE]\n[LOCKNOTE_STATE]\n[/LOCKNOTE_STATE]".to_string(), true, true),
            // Header behind a lone `\r`: a header line once the line breaks
            // opening the text are dropped.
            (format!("\r{}", block), false, true),
            (format!("\n\r{}\r\n", block), false, true),
            (format!("a\r{}", block), false, true),
            // The reader stops at the first footer after its header.
            ("\r[LOCKNOTE_STATE]\rx\r[/LOCKNOTE_STATE]\ry\n[/LOCKNOTE_STATE]".to_string(), false, true),
            // Never a block: no footer line ending the text, or no header
            // between the previous footer line and that one.
            ("[LOCKNOTE_STATE]".to_string(), false, false),
            ("a\r\n[LOCKNOTE_STATE]\r\nactive=1".to_string(), false, false),
            ("\r[LOCKNOTE_STATE]\r\r\n".to_string(), false, false),
            ("a\n[/LOCKNOTE_STATE]".to_string(), false, false),
            (format!("{}\nafter", block), false, false),
            (format!("{}\r\r\n", block), false, false),
            (format!("{} ", block), false, false),
            (format!("{}\nx\n[/LOCKNOTE_STATE]", block), false, false),
            ("a\nvoir [LOCKNOTE_STATE]\nactive=1\n[/LOCKNOTE_STATE]".to_string(), false, false),
            ("a\n [LOCKNOTE_STATE]\n[/LOCKNOTE_STATE]".to_string(), false, false),
            ("\x00[LOCKNOTE_STATE]\n[/LOCKNOTE_STATE]".to_string(), false, false),
            ("".to_string(), false, false),
        ];
        for (text, taken, decision) in cases {
            assert_eq!(parse(&text).0.is_some(), taken, "parse {:?}", text);
            assert_eq!(may_end_with_block(&text), decision, "decision {:?}", text);
            assert!(decision || !taken, "{:?}", text);
        }
    }

    #[test]
    fn roundtrip_non_default() {
        let s = SessionState { active_tab: 2, cursor_line: 42, ..Default::default() };
        let payload = combine("note data", &s);
        let (parsed, remaining) = parse(&payload);
        assert_eq!(parsed, Some(s));
        assert_eq!(remaining, "note data");
    }

    #[test]
    fn default_state_serializes_to_empty() {
        assert_eq!(serialize(&SessionState::default()), "");
        assert_eq!(combine("data", &SessionState::default()), "data");
    }

    #[test]
    fn missing_block_leaves_input_unchanged() {
        let (parsed, remaining) = parse("just note content");
        assert!(parsed.is_none());
        assert_eq!(remaining, "just note content");
    }

    #[test]
    fn trailing_content_after_footer_prevents_match() {
        // Stray text after the block — we don't try to fish it out of the middle.
        let input = "[LOCKNOTE_STATE]\nactive=1\nline=0\n[/LOCKNOTE_STATE]\nextra";
        let (parsed, _) = parse(input);
        assert!(parsed.is_none());
    }

    #[test]
    fn malformed_values_fall_back_to_defaults() {
        let input = "prefix\n[LOCKNOTE_STATE]\nactive=not_a_number\nline=xyz\n[/LOCKNOTE_STATE]";
        let (parsed, remaining) = parse(input);
        assert_eq!(parsed, Some(SessionState::default()));
        assert_eq!(remaining, "prefix");
    }

    #[test]
    fn unknown_keys_are_ignored_for_forward_compat() {
        let input = "x\n[LOCKNOTE_STATE]\nactive=3\nfuture_key=hello\nline=7\n[/LOCKNOTE_STATE]";
        let (parsed, _) = parse(input);
        assert_eq!(parsed, Some(SessionState { active_tab: 3, cursor_line: 7, ..Default::default() }));
    }

    #[test]
    fn only_partial_fields_present() {
        let input = "[LOCKNOTE_STATE]\nactive=5\n[/LOCKNOTE_STATE]";
        let (parsed, _) = parse(input);
        assert_eq!(parsed, Some(SessionState { active_tab: 5, cursor_line: 0, ..Default::default() }));
    }

    const KEY_A: &str = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIOMqqnkVzrm0SdG6UOoqKLsabgH5C9okWi0dh2l9GKJl alice@pc";
    const KEY_B: &str = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIBrUMQdFCWh8fgfWxNNdW9wGU9h5f5Zx3HJ5L7q6wC0Q";

    #[test]
    fn agent_keys_roundtrip() {
        let s = SessionState {
            active_tab: 1,
            cursor_line: 2,
            agent_keys: vec![KEY_A.to_string(), KEY_B.to_string()],
        };
        let (parsed, remaining) = parse(&combine("note data", &s));
        assert_eq!(parsed, Some(s));
        assert_eq!(remaining, "note data");
    }

    #[test]
    fn agent_keys_alone_are_serialized() {
        // Tab 0, line 0 but a key still in the agent: the block must be
        // written, otherwise the list is lost and the key stays loaded.
        let s = SessionState { agent_keys: vec![KEY_A.to_string()], ..Default::default() };
        assert!(!serialize(&s).is_empty());
        let (parsed, _) = parse(&combine("data", &s));
        assert_eq!(parsed, Some(s));
    }

    #[test]
    fn state_without_agent_keys_parses_to_empty_list() {
        // Block written by a version before issue 054.
        let input = "x\n[LOCKNOTE_STATE]\nactive=1\nline=4\n[/LOCKNOTE_STATE]";
        let (parsed, _) = parse(input);
        assert_eq!(parsed.unwrap().agent_keys, Vec::<String>::new());
    }

    #[test]
    fn combine_empty_data() {
        let s = SessionState { active_tab: 1, cursor_line: 0, ..Default::default() };
        let out = combine("", &s);
        assert!(out.starts_with("[LOCKNOTE_STATE]"));
    }
}
