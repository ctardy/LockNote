// Payload — single place that builds and takes apart the decrypted note
// payload.
//
// `compose` (called by `EditorForm::on_save`) builds it as:
//
//   tabs → prompts::combine → vault::combine → attachments::combine
//        → session_state::combine
//
// Files written before the safety backups replaced the snapshots also hold a
// history block between the tabs and the vault (`snapshots::combine`), so the
// payload is taken apart in the reverse order, history included:
//
//   session_state → attachments → vault → snapshots → prompts → tabs
//
// The history block (old files only, never written now) and the prompts block
// (issue 058) both sit between the tabs and the vault, and no file holds both:
// a version that writes prompts no longer writes the history. Each reader
// keeps what precedes its block and drops what follows it, so the order
// between the two only matters for hostile text. History first: the text of
// an old snapshot was written before `[LOCKNOTE_PROMPTS]` was a reserved
// marker, so it may hold that line unescaped; prompt texts and names never
// hold a history header line (escaped, or behind `[PROMPT:`).
//
// Every load path (startup, unlock) goes through `parse_payload` and every
// save goes through `compose`, so the order lives in exactly one place.
//
// User text (tab, prompt, vault value) is escaped against every reserved marker and
// a block header is only recognized on a line of its own: see `features::markers`.

use crate::crypto::wiping::WipingString;
use crate::features::attachments::Attachment;
use crate::features::prompts::PromptEntry;
use crate::features::session_state::SessionState;
use crate::features::snapshots::Snapshot;
use crate::features::tabs::NoteTab;
use crate::features::vault::SecretEntry;
use zeroize::Zeroizing;

/// Every block of a decrypted payload, split apart.
pub struct ParsedPayload {
    pub session: Option<SessionState>,
    pub attachments: Vec<Attachment>,
    pub secrets: Vec<SecretEntry>,
    pub prompts: Vec<PromptEntry>,
    pub snapshots: Vec<Snapshot>,
    /// Always at least one tab (`tabs::parse` guarantees it).
    pub tabs: Vec<NoteTab>,
}

/// Name of the tab a new note starts with.
pub const DEFAULT_TAB_NAME: &str = "Note 1";

/// Note text a new note starts with in `lang` (issue #34): empty when its
/// first tab is named `DEFAULT_TAB_NAME` (English and French, as before),
/// otherwise a tabs block holding one empty tab named in `lang` ("Nota 1"),
/// so the name is stored with the note and never changes afterwards.
pub fn new_note_text(lang: crate::features::i18n::Language) -> String {
    let name = crate::features::tabs::default_name(lang, 1);
    if name == DEFAULT_TAB_NAME {
        return String::new();
    }
    crate::features::tabs::serialize(&[NoteTab::new(&name)])
}

/// True for the single tab of a note that never used tabs: one tab, default
/// name, no color.
fn is_default_single_tab(tabs: &[NoteTab]) -> bool {
    tabs.len() == 1 && tabs[0].color.is_none() && tabs[0].name == DEFAULT_TAB_NAME
}

/// True when the reading chain could take a part of `text`, saved as plain
/// text, for a block. Asked to the block modules, with the markers and the
/// search their readers use, in the order the readers run:
///
/// * `session_state::may_end_with_block`: a whole session block ending it;
/// * `loose_header_line` of the attachments, the vault, the history and the
///   prompts: their header between line bounds anywhere in it;
/// * `tabs::starts_with_block`: the tabs header starting it (the very test
///   of `tabs::parse`).
///
/// This is a cautious superset of what the readers recognize in `text` as it
/// is written, because they do not see it as it is written: `Settings::parse`
/// drops the `\r` / `\n` opening the note text, and the reader of the block
/// `compose` appends trims `\n` then `\r` from the end of the text. Both can
/// turn a header next to a lone `\r` into a header line. So a lone `\r`
/// counts as a line bound here, and only here (the readers are unchanged).
/// The superset costs nothing that was ever read back: before issue 050 the
/// headers were searched anywhere in the text, so such a text was cut.
fn plain_text_may_be_misread(text: &str) -> bool {
    crate::features::session_state::may_end_with_block(text)
        || crate::features::attachments::loose_header_line(text).is_some()
        || crate::features::vault::loose_header_line(text).is_some()
        || crate::features::snapshots::loose_header_line(text).is_some()
        || crate::features::prompts::may_end_with_block(text)
        || crate::features::tabs::starts_with_block(text)
}

/// Build the payload (what `Settings::serialize` then prefixes with the
/// settings header) from its blocks: tabs, prompts, vault, attachments,
/// session state.
///
/// A default single tab is written as its plain text, without the tabs block:
/// that is what LockNote 1.2.1 and the versions without tabs read. The text
/// is written byte for byte; it is read back byte for byte too when no block
/// follows it (a reader trims the line breaks ending the text that precedes
/// its block, and `Settings::parse` those opening the note text). The one
/// exception is a text the reading chain could take for a block
/// (`plain_text_may_be_misread`, issue 050): it goes to the tabs block like
/// any other tab, where it is escaped. Empty `tabs` writes no tab text at
/// all. Each block follows what precedes it after one line break, a block
/// with nothing before it starts the payload, an empty block is not written.
///
/// Issue #42: everything is written into one buffer that grows without
/// leaving copies (no intermediate text per block, no `format!`).
pub fn compose(
    tabs: &[NoteTab],
    prompts: &[PromptEntry],
    secrets: &[SecretEntry],
    attachments: &[Attachment],
    state: &SessionState,
) -> String {
    let mut out = WipingString::new();
    if is_default_single_tab(tabs) && !plain_text_may_be_misread(&tabs[0].content) {
        out.push_str(&tabs[0].content);
    } else {
        crate::features::tabs::write_block(&mut out, tabs);
    }
    if !prompts.is_empty() {
        start_block(&mut out);
        crate::features::prompts::write_block(&mut out, prompts);
    }
    if !secrets.is_empty() {
        start_block(&mut out);
        crate::features::vault::write_block(&mut out, secrets);
    }
    if !attachments.is_empty() {
        start_block(&mut out);
        crate::features::attachments::write_block(&mut out, attachments);
    }
    if crate::features::session_state::is_written(state) {
        start_block(&mut out);
        crate::features::session_state::write_block(&mut out, state);
    }
    out.into_string()
}

/// The line break between a block and what precedes it, when something does.
fn start_block(out: &mut WipingString) {
    if !out.is_empty() {
        out.push('\n');
    }
}

/// Split the payload (after decompression and `Settings::parse`) into its
/// blocks. Each sub-result is moved out, never cloned.
/// `categories` is the user's vault category list (issue 052).
///
/// Each reader hands back a copy of the text before its block: wrapped,
/// wiped when the function returns (issue #42).
pub fn parse_payload(note_text: &str, categories: &[String]) -> ParsedPayload {
    let (session, rest) = crate::features::session_state::parse(note_text);
    let rest = Zeroizing::new(rest);
    let (attachments, rest) = crate::features::attachments::parse(&rest);
    let rest = Zeroizing::new(rest);
    // Vault before snapshots: write order is snapshots → vault, so the
    // vault block sits after the history block.
    let (secrets, rest) = crate::features::vault::parse_with_categories(&rest, categories);
    let rest = Zeroizing::new(rest);
    // History before prompts: see the header of this module.
    let (snapshots, rest) = crate::features::snapshots::parse(&rest);
    let rest = Zeroizing::new(rest);
    let (prompts, rest) = crate::features::prompts::parse(&rest);
    let rest = Zeroizing::new(rest);
    let tabs = crate::features::tabs::parse(&rest);
    ParsedPayload { session, attachments, secrets, prompts, snapshots, tabs }
}

/// Pick the session to restore. The in-memory state (captured at lock time)
/// wins over the one read from disk: a tab switch after the last save only
/// reaches the disk on close, and a failed save at lock leaves the disk
/// stale. The tab index is clamped to `tab_count`.
pub fn resolve_session(
    in_memory: Option<SessionState>,
    from_disk: Option<SessionState>,
    tab_count: usize,
) -> SessionState {
    let mut state = in_memory.or(from_disk).unwrap_or_default();
    state.active_tab = state.active_tab.min(tab_count.saturating_sub(1));
    state
}

/// What the editor shows after a load: the tabs, which one is active and
/// where the caret goes. Invariant: `active < tabs.len()`, and the editor
/// content is always `tabs[active].content`.
pub struct EditorView {
    pub tabs: Vec<NoteTab>,
    pub active: usize,
    pub cursor_line: u32,
    /// SSH keys this file loaded into ssh-agent and not removed yet
    /// (issue 054), from the same session as `active`.
    pub agent_keys: Vec<String>,
}

/// Single place deciding the post-load view (startup and both unlock paths).
pub fn build_view(
    parsed_tabs: Vec<NoteTab>,
    in_memory: Option<SessionState>,
    from_disk: Option<SessionState>,
) -> EditorView {
    let state = resolve_session(in_memory, from_disk, parsed_tabs.len());
    EditorView {
        tabs: parsed_tabs,
        active: state.active_tab,
        cursor_line: state.cursor_line,
        agent_keys: state.agent_keys,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::prompts::PromptEntry;

    fn parse_defaults(text: &str) -> ParsedPayload {
        let cats: Vec<String> = crate::features::vault::CATEGORIES.iter().map(|c| c.to_string()).collect();
        parse_payload(text, &cats)
    }

    const S: fn(usize, u32) -> SessionState =
        |active_tab, cursor_line| SessionState { active_tab, cursor_line, ..Default::default() };

    /// Every load path must go through parse_payload: a direct call to one
    /// of the block parsers in the editor means a path that skips the
    /// session state (issue 046) or reorders the pipeline.
    #[test]
    fn no_direct_parse_calls_outside_payload() {
        let editor = crate::ui::editor::all_source();
        let forbidden = [
            "features::tabs::parse(",
            "features::vault::parse(",
            "features::prompts::parse(",
            "features::snapshots::parse(",
            "features::attachments::parse(",
            "features::session_state::parse(",
        ];
        let found: Vec<&str> = forbidden.iter().copied().filter(|p| editor.contains(p)).collect();
        assert_eq!(found, Vec::<&str>::new());
    }

    #[test]
    fn resolve_prefers_memory() {
        assert_eq!(resolve_session(Some(S(2, 5)), Some(S(0, 0)), 3), S(2, 5));
    }

    #[test]
    fn resolve_falls_back_to_disk() {
        assert_eq!(resolve_session(None, Some(S(1, 4)), 3), S(1, 4));
    }

    #[test]
    fn resolve_clamps() {
        assert_eq!(resolve_session(Some(S(7, 0)), None, 3).active_tab, 2);
        assert_eq!(resolve_session(None, Some(S(9, 1)), 2).active_tab, 1);
    }

    #[test]
    fn resolve_none() {
        assert_eq!(resolve_session(None, None, 3), SessionState::default());
    }

    #[test]
    fn build_view_invariant() {
        for active in 0..6 {
            let tabs = vec![
                NoteTab::with_content("A", "a", None),
                NoteTab::with_content("B", "b", None),
                NoteTab::with_content("C", "c", None),
            ];
            let view = build_view(tabs, Some(S(active, 0)), None);
            assert!(view.active < view.tabs.len(), "active={}", active);
        }
    }

    #[test]
    fn build_view_restores_memory_tab_over_disk() {
        let tabs = vec![NoteTab::with_content("A", "a", None), NoteTab::with_content("B", "b", None)];
        let view = build_view(tabs, Some(S(1, 3)), Some(S(0, 0)));
        assert_eq!((view.active, view.cursor_line), (1, 3));
        assert_eq!(view.tabs[view.active].content, "b");
    }

    #[test]
    fn build_view_carries_agent_keys_of_the_chosen_session() {
        let tabs = || vec![NoteTab::with_content("A", "a", None)];
        let with_keys = |keys: &[&str]| SessionState {
            agent_keys: keys.iter().map(|k| k.to_string()).collect(),
            ..Default::default()
        };
        // In memory wins: keys removed at lock time are not resurrected from a
        // stale disk copy.
        let view = build_view(tabs(), Some(with_keys(&[])), Some(with_keys(&["ssh-ed25519 AAAA old"])));
        assert!(view.agent_keys.is_empty());
        // Startup / no lock in this run: the disk list is used.
        let view = build_view(tabs(), None, Some(with_keys(&["ssh-ed25519 AAAA k"])));
        assert_eq!(view.agent_keys, vec!["ssh-ed25519 AAAA k".to_string()]);
    }

    /// A payload as the versions with snapshots wrote it: always the tabs
    /// block, then the history block between the tabs and the vault. Nothing
    /// writes that block any more; old files still hold it.
    fn legacy_payload_with_history(
        tabs: &[NoteTab],
        snapshots: &[Snapshot],
        secrets: &[SecretEntry],
        attachments: &[Attachment],
        state: &SessionState,
    ) -> String {
        let text = crate::features::tabs::serialize(tabs);
        let text = crate::features::snapshots::combine(&text, snapshots);
        let text = crate::features::vault::combine(&text, secrets);
        let text = crate::features::attachments::combine(&text, attachments);
        crate::features::session_state::combine(&text, state)
    }

    fn names(tabs: &[NoteTab]) -> Vec<(&str, &str)> {
        tabs.iter().map(|t| (t.name.as_str(), t.content.as_str())).collect()
    }

    #[test]
    fn compose_roundtrip_all_blocks() {
        let tabs = vec![
            NoteTab::with_content("A", "alpha", None),
            NoteTab::with_content("B", "beta", Some("blue")),
            NoteTab::with_content("C", "gamma", None),
        ];
        let prompts = vec![PromptEntry::new("summary", "Summarize:\n", "Writing"), PromptEntry::new("fix", "Fix it", "Code")];
        let secrets = vec![SecretEntry::new("gmail", "s3cret", "Email")];
        let atts = vec![Attachment::new("a.txt", b"hello".to_vec())];
        let state = SessionState { active_tab: 2, cursor_line: 5, ..Default::default() };

        let p = parse_defaults(&compose(&tabs, &prompts, &secrets, &atts, &state));

        assert_eq!(p.session, Some(state));
        assert_eq!(names(&p.tabs), vec![("A", "alpha"), ("B", "beta"), ("C", "gamma")]);
        assert_eq!(p.tabs[1].color.as_deref(), Some("blue"));
        assert!(p.snapshots.is_empty());
        assert_eq!(p.prompts, prompts);
        assert_eq!(p.secrets, secrets);
        assert_eq!(p.attachments, atts);
    }

    // Old file: the history block sits between the tabs and the vault.
    #[test]
    fn legacy_payload_with_history_block_is_read() {
        let tabs = vec![
            NoteTab::with_content("A", "alpha", None),
            NoteTab::with_content("B", "beta", Some("blue")),
            NoteTab::with_content("C", "gamma", None),
        ];
        let snaps = vec![Snapshot::new(1_700_000_000, "old")];
        let secrets = vec![SecretEntry::new("gmail", "s3cret", "Email")];
        let atts = vec![Attachment::new("a.txt", b"hello".to_vec())];
        let state = SessionState { active_tab: 2, cursor_line: 5, ..Default::default() };

        let p = parse_defaults(&legacy_payload_with_history(&tabs, &snaps, &secrets, &atts, &state));

        assert_eq!(p.session, Some(state));
        assert_eq!(names(&p.tabs), vec![("A", "alpha"), ("B", "beta"), ("C", "gamma")]);
        assert_eq!(p.tabs[1].color.as_deref(), Some("blue"));
        assert_eq!(p.snapshots, snaps);
        assert!(p.prompts.is_empty());
        assert_eq!(p.secrets, secrets);
        assert_eq!(p.attachments, atts);
    }

    // The exact text `on_save` wrote before `compose` existed.
    #[test]
    fn compose_default_single_tab_is_plain_text() {
        let tabs = vec![NoteTab::with_content("Note 1", "just\r\nsome notes", None)];
        let prompts = vec![PromptEntry::new("p", "text", "Cat")];
        let secrets = vec![SecretEntry::new("gmail", "s3cret", "Email")];
        let atts = vec![Attachment::new("a.txt", b"hello".to_vec())];
        let state = SessionState { active_tab: 0, cursor_line: 4, ..Default::default() };

        assert_eq!(compose(&tabs, &[], &[], &[], &SessionState::default()), "just\r\nsome notes");

        let expected = crate::features::prompts::combine("just\r\nsome notes", &prompts);
        let expected = crate::features::vault::combine(&expected, &secrets);
        let expected = crate::features::attachments::combine(&expected, &atts);
        let expected = crate::features::session_state::combine(&expected, &state);
        assert_eq!(compose(&tabs, &prompts, &secrets, &atts, &state), expected);
    }

    #[test]
    fn compose_uses_the_tabs_block_for_anything_else() {
        let renamed = vec![NoteTab::with_content("Journal", "x", None)];
        let colored = vec![NoteTab::with_content("Note 1", "x", Some("red"))];
        let two = vec![NoteTab::with_content("Note 1", "x", None), NoteTab::with_content("Note 2", "y", None)];
        for tabs in [renamed, colored, two] {
            let expected = crate::features::tabs::serialize(&tabs);
            assert!(expected.starts_with("[LOCKNOTE_TABS]\n"));
            assert_eq!(compose(&tabs, &[], &[], &[], &SessionState::default()), expected);
        }
        assert_eq!(compose(&[], &[], &[], &[], &SessionState::default()), "");
    }

    #[test]
    fn session_default_writes_nothing() {
        let tabs = vec![NoteTab::with_content("A", "x", None), NoteTab::with_content("B", "y", None)];
        let p = parse_defaults(&compose(&tabs, &[], &[], &[], &SessionState::default()));
        assert_eq!(p.session, None);
        assert_eq!(p.tabs.len(), 2);
    }

    #[test]
    fn plain_text_is_single_tab() {
        let p = parse_defaults("just some notes");
        assert_eq!(p.session, None);
        assert_eq!(p.tabs.len(), 1);
        assert_eq!(p.tabs[0].content, "just some notes");
    }

    // A single default tab is saved as plain text, so a [LOCKNOTE_STATE]
    // block that leaked into the text (issue 046) sits at the tail and is
    // consumed as the session state. Characterizes session_state::parse's
    // rfind; decision (m): leaked blocks are not cleaned automatically.
    #[test]
    fn leaked_block_alone_is_consumed_as_state() {
        let p = parse_defaults("texte\n[LOCKNOTE_STATE]\nactive=0\nline=3\n[/LOCKNOTE_STATE]");
        assert_eq!(p.session, Some(SessionState { active_tab: 0, cursor_line: 3, ..Default::default() }));
        assert_eq!(p.tabs[0].content, "texte");
    }

    #[test]
    fn leaked_block_plus_real_block() {
        let leaked = "texte\n[LOCKNOTE_STATE]\nactive=0\nline=3\n[/LOCKNOTE_STATE]";
        let payload = crate::features::session_state::combine(
            leaked,
            &SessionState { active_tab: 1, cursor_line: 2, ..Default::default() },
        );
        let p = parse_defaults(&payload);
        assert_eq!(p.session, Some(SessionState { active_tab: 1, cursor_line: 2, ..Default::default() }));
        assert!(p.tabs[0].content.contains("[LOCKNOTE_STATE]"));
    }

    // Issue 050 on a file written before the fix: the text was saved as is,
    // so a line that is exactly a block header cannot be told from a real
    // one and still cuts. Nothing writes such a payload any more (see
    // `single_tab_with_marker_line_roundtrips`).
    #[test]
    fn old_unescaped_header_line_still_cuts() {
        let p = parse_defaults("before\n[LOCKNOTE_VAULT]\nafter");
        assert_eq!(p.tabs.len(), 1);
        assert_eq!(p.tabs[0].content, "before");
    }

    // ... but inside a line it was never a header: the text of such an old
    // file, cut by the previous versions, is read whole.
    #[test]
    fn old_unescaped_marker_inside_a_line_is_not_cut() {
        for m in MARKERS {
            let text = format!("before\nvoir {} ici\nafter", m);
            let p = parse_defaults(&text);
            assert_eq!(tab_view(&p.tabs), vec![("Note 1".to_string(), text.clone(), None)], "{}", m);
            assert!(p.secrets.is_empty() && p.attachments.is_empty() && p.snapshots.is_empty() && p.prompts.is_empty());
            assert_eq!(p.session, None);
        }
    }

    // The contrary of the former `unescaped_single_tab_marker_is_cut`: what
    // is saved now comes back whole.
    #[test]
    fn single_tab_marker_is_not_cut() {
        let tabs = vec![NoteTab::with_content("Note 1", "before\n[LOCKNOTE_VAULT]\nafter", None)];
        let p = parse_defaults(&compose(&tabs, &[], &[], &[], &SessionState::default()));
        assert_eq!(p.tabs.len(), 1);
        assert_eq!(p.tabs[0].content, "before\n[LOCKNOTE_VAULT]\nafter");
        assert!(p.secrets.is_empty());
    }

    // Decision (m) after the fix: a block that leaked into the text of an old
    // note and was read back as text is kept as text by the next saves (no
    // automatic cleaning), it is no longer taken for the session state.
    #[test]
    fn leaked_block_kept_as_text_survives_the_next_save() {
        let leaked = "texte\n[LOCKNOTE_STATE]\nactive=0\nline=3\n[/LOCKNOTE_STATE]";
        let tabs = vec![NoteTab::with_content("Note 1", leaked, None)];
        for state in [SessionState::default(), SessionState { active_tab: 0, cursor_line: 2, ..Default::default() }] {
            let p = parse_defaults(&compose(&tabs, &[], &[], &[], &state));
            assert_eq!(p.tabs[0].content, leaked);
            assert_eq!(p.session, if state == SessionState::default() { None } else { Some(state) });
        }
    }

    // The reserved list of the code is the contract list of these tests.
    #[test]
    fn reserved_list_is_the_contract_list() {
        assert_eq!(crate::features::markers::RESERVED, MARKERS);
    }

    // ---- issue 050: user text holding reserved markers must round-trip ----

    /// Every marker of the payload format, written out on purpose: this list
    /// is the contract, it must not follow a change of the code by itself.
    const MARKERS: [&str; 21] = [
        "[LOCKNOTE_TABS]",
        "[/LOCKNOTE_TABS]",
        "[TAB:",
        "[LOCKNOTE_HISTORY]",
        "[/LOCKNOTE_HISTORY]",
        "[SNAP:",
        "[/SNAP]",
        "[LOCKNOTE_PROMPTS]",
        "[/LOCKNOTE_PROMPTS]",
        "[PROMPT:",
        "[/PROMPT]",
        "[LOCKNOTE_VAULT]",
        "[/LOCKNOTE_VAULT]",
        "[SECRET:",
        "[/SECRET]",
        "[LOCKNOTE_ATTACHMENTS]",
        "[/LOCKNOTE_ATTACHMENTS]",
        "[FILE:",
        "[/FILE]",
        "[LOCKNOTE_STATE]",
        "[/LOCKNOTE_STATE]",
    ];

    /// Texts holding `m` at the start of a line, in every position.
    fn line_start_texts(m: &str) -> Vec<(&'static str, String)> {
        vec![
            ("middle line", format!("before\n{}\nafter", m)),
            ("first line", format!("{}\nafter", m)),
            ("last line", format!("before\n{}", m)),
            ("CRLF", format!("before\r\n{}\r\nafter", m)),
            ("indented", format!("before\n  {}\nafter", m)),
            ("line prefix", format!("before\n{}suite\nafter", m)),
        ]
    }

    /// The other blocks of the payload around the text under test.
    struct Around {
        label: &'static str,
        prompts: Vec<PromptEntry>,
        secrets: Vec<SecretEntry>,
        attachments: Vec<Attachment>,
        state: SessionState,
    }

    fn arounds() -> Vec<Around> {
        vec![
            Around { label: "alone", prompts: vec![], secrets: vec![], attachments: vec![], state: SessionState::default() },
            Around {
                label: "with prompts, vault, attachments, state",
                prompts: vec![PromptEntry::new("ask", "Explain:\r\nstep by step", "Work")],
                secrets: vec![SecretEntry::new("gmail", "s3cret", "Email")],
                attachments: vec![Attachment::new("a.txt", b"hello".to_vec())],
                state: SessionState { active_tab: 0, cursor_line: 2, ..Default::default() },
            },
        ]
    }

    fn tab_view(tabs: &[NoteTab]) -> Vec<(String, String, Option<String>)> {
        tabs.iter().map(|t| (t.name.clone(), t.content.clone(), t.color.clone())).collect()
    }

    /// `compose` then `parse_payload`: what differs from what went in, if
    /// anything.
    fn roundtrip_diff(
        tabs: &[NoteTab],
        prompts: &[PromptEntry],
        secrets: &[SecretEntry],
        attachments: &[Attachment],
        state: &SessionState,
    ) -> Option<String> {
        let p = parse_defaults(&compose(tabs, prompts, secrets, attachments, state));
        let expected_session = if *state == SessionState::default() { None } else { Some(state.clone()) };
        if tab_view(&p.tabs) != tab_view(tabs) {
            return Some(format!("tabs {:?}", tab_view(&p.tabs)));
        }
        if p.prompts != prompts {
            return Some(format!("prompts {:?}", p.prompts.iter().map(|x| &x.name).collect::<Vec<_>>()));
        }
        if p.secrets != secrets {
            return Some(format!("secrets {:?}", p.secrets));
        }
        if p.attachments != attachments {
            return Some(format!("attachments {:?}", p.attachments.iter().map(|a| &a.name).collect::<Vec<_>>()));
        }
        if p.session != expected_session {
            return Some(format!("session {:?}", p.session));
        }
        if !p.snapshots.is_empty() {
            return Some(format!("snapshots {:?}", p.snapshots));
        }
        None
    }

    /// Run every (marker, text, surrounding blocks) case through `tabs_of`
    /// and fail with the list of the cases that did not come back identical.
    fn assert_tab_texts_roundtrip(
        texts_of: &dyn Fn(&str) -> Vec<(&'static str, String)>,
        tabs_of: &dyn Fn(&str) -> Vec<NoteTab>,
    ) {
        let mut lost = Vec::new();
        let mut total = 0;
        for m in MARKERS {
            for (what, text) in texts_of(m) {
                for a in arounds() {
                    total += 1;
                    if let Some(diff) = roundtrip_diff(&tabs_of(&text), &a.prompts, &a.secrets, &a.attachments, &a.state) {
                        lost.push(format!("{} / {} / {}: got {}", m, what, a.label, diff));
                    }
                }
            }
        }
        assert!(lost.is_empty(), "{} of {} cases did not round-trip:\n{}", lost.len(), total, lost.join("\n"));
    }

    // (a) default single tab
    #[test]
    fn single_tab_with_marker_line_roundtrips() {
        assert_tab_texts_roundtrip(&line_start_texts, &|text| vec![NoteTab::with_content("Note 1", text, None)]);
    }

    // (b) several tabs, marker in the first one
    #[test]
    fn first_of_two_tabs_with_marker_line_roundtrips() {
        assert_tab_texts_roundtrip(&line_start_texts, &|text| {
            vec![NoteTab::with_content("A", text, None), NoteTab::with_content("B", "second tab", Some("blue"))]
        });
    }

    #[test]
    fn last_of_two_tabs_with_marker_line_roundtrips() {
        assert_tab_texts_roundtrip(&line_start_texts, &|text| {
            vec![NoteTab::with_content("A", "first tab", None), NoteTab::with_content("B", text, None)]
        });
    }

    // (e) a marker inside a line is plain text
    #[test]
    fn marker_inside_a_line_roundtrips_in_tabs() {
        let inside = |m: &str| {
            vec![
                ("inside a line", format!("voir {} ici", m)),
                ("inside a line, more lines", format!("a\nvoir {} ici\nb", m)),
                ("end of a line", format!("a\nvoir {}\nb", m)),
            ]
        };
        assert_tab_texts_roundtrip(&inside, &|text| vec![NoteTab::with_content("Note 1", text, None)]);
        assert_tab_texts_roundtrip(&inside, &|text| {
            vec![NoteTab::with_content("A", text, None), NoteTab::with_content("B", "second tab", None)]
        });
    }

    // (d) + (e) vault values
    #[test]
    fn secret_value_with_marker_roundtrips() {
        let mut lost = Vec::new();
        let mut total = 0;
        for m in MARKERS {
            let mut texts = line_start_texts(m);
            texts.push(("inside a line", format!("voir {} ici", m)));
            for (what, value) in texts {
                for a in arounds() {
                    for (tabs_label, tabs) in [
                        ("single tab", vec![NoteTab::with_content("Note 1", "note", None)]),
                        ("two tabs", vec![NoteTab::with_content("A", "a", None), NoteTab::with_content("B", "b", None)]),
                    ] {
                        total += 1;
                        let secrets = vec![
                            SecretEntry::new("first", &value, "Email"),
                            SecretEntry::new("second", "kept?", "Cloud"),
                        ];
                        if let Some(diff) = roundtrip_diff(&tabs, &a.prompts, &secrets, &a.attachments, &a.state) {
                            lost.push(format!("{} / {} / {} / {}: got {}", m, what, tabs_label, a.label, diff));
                        }
                    }
                }
            }
        }
        assert!(lost.is_empty(), "{} of {} cases did not round-trip:\n{}", lost.len(), total, lost.join("\n"));
    }

    // (c) a whole well-formed block pasted into the text (the worst case:
    // header, inner markers and footer of another layer).
    #[test]
    fn text_holding_whole_blocks_roundtrips() {
        let vault = crate::features::vault::serialize(&[SecretEntry::new("fake", "v", "Email")]);
        let attach = crate::features::attachments::serialize(&[Attachment::new("fake.txt", b"x".to_vec())]);
        let history = crate::features::snapshots::serialize(&[Snapshot::new(7, "old")]);
        let state = crate::features::session_state::serialize(&SessionState { active_tab: 3, cursor_line: 9, ..Default::default() });
        let tabs_block = crate::features::tabs::serialize(&[NoteTab::with_content("fake", "x", None)]);
        let prompts = crate::features::prompts::serialize(&[PromptEntry::new("fake", "p", "Cat")]);
        let all = [tabs_block.as_str(), &history, &prompts, &vault, &attach, &state].join("\n");
        let blocks = [
            ("tabs", tabs_block.as_str()),
            ("history", &history),
            ("prompts", &prompts),
            ("vault", &vault),
            ("attachments", &attach),
            ("state", &state),
            ("all", &all),
        ];

        let mut lost = Vec::new();
        let mut total = 0;
        for (block_label, block) in blocks {
            for (what, text) in [
                ("block at the end", format!("notes\n{}", block)),
                ("block in the middle", format!("notes\n{}\nmore notes", block)),
                ("block at the start", format!("{}\nmore notes", block)),
            ] {
                for a in arounds() {
                    let cases = [
                        ("single tab", vec![NoteTab::with_content("Note 1", &text, None)], a.prompts.clone(), a.secrets.clone()),
                        (
                            "first of two tabs",
                            vec![NoteTab::with_content("A", &text, None), NoteTab::with_content("B", "second tab", None)],
                            a.prompts.clone(),
                            a.secrets.clone(),
                        ),
                        (
                            "secret value",
                            vec![NoteTab::with_content("Note 1", "note", None)],
                            a.prompts.clone(),
                            vec![SecretEntry::new("first", &text, "Email"), SecretEntry::new("second", "kept?", "Cloud")],
                        ),
                        (
                            "prompt text",
                            vec![NoteTab::with_content("Note 1", "note", None)],
                            vec![PromptEntry::new("first", &text, "Cat"), PromptEntry::new("second", "kept?", "Cat")],
                            a.secrets.clone(),
                        ),
                    ];
                    for (where_label, tabs, prompts, secrets) in cases {
                        total += 1;
                        if let Some(diff) = roundtrip_diff(&tabs, &prompts, &secrets, &a.attachments, &a.state) {
                            lost.push(format!("{} / {} / {} / {}: got {}", block_label, what, where_label, a.label, diff));
                        }
                    }
                }
            }
        }
        assert!(lost.is_empty(), "{} of {} cases did not round-trip:\n{}", lost.len(), total, lost.join("\n"));
    }

    // The default single tab stays plain text (what LockNote 1.2.1 and the
    // older versions read) as long as no reader would take it for a block.
    #[test]
    fn default_single_tab_without_marker_line_stays_plain_text() {
        for text in ["plain", "a\r\nb", "voir [LOCKNOTE_VAULT] ici", "x [TAB:y] z\n- [/LOCKNOTE_STATE]", ""] {
            let tabs = vec![NoteTab::with_content("Note 1", text, None)];
            assert_eq!(compose(&tabs, &[], &[], &[], &SessionState::default()), text);
        }
    }

    // ... and goes to the tabs block, escaped, when one would: a header line
    // of the history, the vault or the attachments anywhere in the text.
    // Every reserved line of the text is then escaped, the others included.
    #[test]
    fn default_single_tab_with_a_block_header_line_uses_the_tabs_block() {
        for header in CUTTING_HEADERS {
            for m in MARKERS {
                let tabs = default_tab(&format!("before\n{}\n{}\nafter", m, header));
                let expected = format!("[LOCKNOTE_TABS]\n[TAB:Note 1]\nbefore\n\x00{}\n\x00{}\nafter\n[/LOCKNOTE_TABS]", m, header);
                assert_eq!(compose(&tabs, &[], &[], &[], &SessionState::default()), expected, "{} {}", header, m);
            }
            // CRLF text: the `\r` belongs to the header line.
            let tabs = default_tab(&format!("before\r\n{}\r\nafter", header));
            assert!(compose(&tabs, &[], &[], &[], &SessionState::default()).starts_with("[LOCKNOTE_TABS]\n"), "{}", header);
        }
    }

    // The tabs header only matters when it starts the text (leading
    // whitespace ignored, as `tabs::parse` does), a session block when it is
    // whole and ends it.
    #[test]
    fn default_single_tab_switches_on_a_leading_tabs_header_or_a_final_state_block() {
        let state_block = "[LOCKNOTE_STATE]\nactive=1\nline=2\n[/LOCKNOTE_STATE]";
        for text in [
            "[LOCKNOTE_TABS]".to_string(),
            "[LOCKNOTE_TABS]\r\nafter".to_string(),
            "[LOCKNOTE_TABS] x]\nafter".to_string(),
            "\r\n  [LOCKNOTE_TABS]\nafter".to_string(),
            state_block.to_string(),
            format!("before\r\n{}", state_block),
            format!("before\r\n{}\r\n\r\n", state_block),
            "before\n[LOCKNOTE_STATE]\n[/LOCKNOTE_STATE]".to_string(),
            format!("before\r\n{}", PROMPTS_BLOCK),
            format!("before\r\n{}\r\n\r\n", PROMPTS_BLOCK),
            "before\n[LOCKNOTE_PROMPTS]\n[/LOCKNOTE_PROMPTS]".to_string(),
        ] {
            let payload = compose_alone(&text);
            assert!(payload.starts_with("[LOCKNOTE_TABS]\n[TAB:Note 1]\n"), "{:?} -> {:?}", text, payload);
            let p = parse_defaults(&payload);
            assert_eq!(p.session, None, "{:?}", text);
            assert_eq!(text_lines(&p.tabs[0].content), text_lines(&text), "{:?}", text);
        }
    }

    // The decision against the readers, on the text as it is written (no
    // settings header, nothing after it):
    // * without a lone `\r`, exactly what the readers recognize: plain text
    //   is read as it is, switched text would not have been;
    // * with a lone `\r`, a superset: whatever is not read as it is
    //   switches, but a text may switch although the readers, given it as
    //   written, read it as it is (the reading chain may cut it differently,
    //   see `default_single_tab_survives_the_whole_reading_chain`).
    #[test]
    fn switch_decision_against_what_the_readers_recognize() {
        let (mut exact, mut superset_only) = (0, 0);
        for text in chain_corpus() {
            let p = parse_defaults(&text);
            let read_as_is = tab_view(&p.tabs) == tab_view(&default_tab(&text))
                && p.session.is_none()
                && p.secrets.is_empty()
                && p.prompts.is_empty()
                && p.attachments.is_empty()
                && p.snapshots.is_empty();
            let switches = plain_text_may_be_misread(&text);
            if has_lone_cr(&text) {
                assert!(switches || read_as_is, "misread but left plain: {:?}", text);
                superset_only += (switches && read_as_is) as usize;
            } else {
                assert_eq!(switches, !read_as_is, "{:?}", text);
                exact += 1;
            }
        }
        // Both halves of the corpus are exercised, and the superset is real.
        assert!(exact > 0 && superset_only > 0, "exact {} superset only {}", exact, superset_only);
    }

    // ---- pass 2: the default single tab leaves plain text only when a
    // reader would take that text for a block ----

    /// Markers whose line never made a reader take plain text for a block:
    /// footers and inner markers. (`[LOCKNOTE_TABS]` only matters when it
    /// starts the text, `[LOCKNOTE_STATE]` when a whole block ends it.)
    const HARMLESS_IN_PLAIN_TEXT: [&str; 18] = [
        "[LOCKNOTE_TABS]",
        "[/LOCKNOTE_TABS]",
        "[TAB:",
        "[/LOCKNOTE_HISTORY]",
        "[SNAP:",
        "[/SNAP]",
        "[LOCKNOTE_PROMPTS]",
        "[/LOCKNOTE_PROMPTS]",
        "[PROMPT:",
        "[/PROMPT]",
        "[/LOCKNOTE_VAULT]",
        "[SECRET:",
        "[/SECRET]",
        "[/LOCKNOTE_ATTACHMENTS]",
        "[FILE:",
        "[/FILE]",
        "[LOCKNOTE_STATE]",
        "[/LOCKNOTE_STATE]",
    ];

    /// Headers a reader finds on a line of their own, anywhere in the text.
    const CUTTING_HEADERS: [&str; 3] = ["[LOCKNOTE_HISTORY]", "[LOCKNOTE_VAULT]", "[LOCKNOTE_ATTACHMENTS]"];

    fn default_tab(text: &str) -> Vec<NoteTab> {
        vec![NoteTab::with_content("Note 1", text, None)]
    }

    /// The default single tab alone: no vault, no attachment, default session.
    fn compose_alone(text: &str) -> String {
        compose(&default_tab(text), &[], &[], &[], &SessionState::default())
    }

    /// What `text` must be after a save and a reload when it stayed plain
    /// text: the same bytes, and nothing else in the note.
    fn plain_and_identical(text: &str) -> Result<(), String> {
        let payload = compose_alone(text);
        if payload != text {
            return Err(format!("not plain text: {:?}", payload));
        }
        let p = parse_defaults(&payload);
        if tab_view(&p.tabs) != tab_view(&default_tab(text)) {
            return Err(format!("read back as {:?}", tab_view(&p.tabs)));
        }
        if p.session.is_some() || !p.secrets.is_empty() || !p.prompts.is_empty() || !p.attachments.is_empty() || !p.snapshots.is_empty() {
            return Err("a block was read out of the text".to_string());
        }
        Ok(())
    }

    // The regression of the first pass: every reserved marker sent the tab to
    // the tabs block, which does not give line breaks at the end of a text
    // back as they were. These texts were read back identical before issue
    // 050 and must still be.
    #[test]
    fn harmless_marker_lines_keep_the_default_single_tab_plain() {
        let mut wrong = Vec::new();
        let mut total = 0;
        let mut check = |label: String, text: String| {
            total += 1;
            if let Err(e) = plain_and_identical(&text) {
                wrong.push(format!("{} / {:?}: {}", label, text, e));
            }
        };
        for m in HARMLESS_IN_PLAIN_TEXT {
            check(format!("{} / marker line, final CRLF", m), format!("avant\r\n{} x]\r\naprès\r\n", m));
            check(format!("{} / marker line, two final CRLF", m), format!("avant\r\n{} x]\r\naprès\r\n\r\n", m));
            check(format!("{} / marker line, no final CRLF", m), format!("avant\r\n{} x]\r\naprès", m));
            check(format!("{} / exact marker line, final CRLF", m), format!("avant\r\n{}\r\naprès\r\n", m));
            check(format!("{} / exact marker line, two final CRLF", m), format!("avant\r\n{}\r\naprès\r\n\r\n", m));
            check(format!("{} / exact marker line, LF", m), format!("avant\n{}\naprès", m));
            check(format!("{} / indented marker, final CRLF", m), format!("avant\r\n  {} x]\r\naprès\r\n", m));
            check(format!("{} / last line", m), format!("avant\r\n{}", m));
        }
        // The headers the readers look for are only headers on a line of
        // their own: indented or followed by text they are text too.
        for m in CUTTING_HEADERS {
            check(format!("{} / followed by text, final CRLF", m), format!("avant\r\n{} x]\r\naprès\r\n", m));
            check(format!("{} / indented, final CRLF", m), format!("avant\r\n  {}\r\naprès\r\n", m));
            check(format!("{} / inside a line, final CRLF", m), format!("avant\r\nvoir {} ici\r\naprès\r\n\r\n", m));
        }
        // A session block that does not end the text is text.
        check(
            "state block followed by text".to_string(),
            "avant\r\n[LOCKNOTE_STATE]\nactive=1\nline=2\n[/LOCKNOTE_STATE]\r\naprès\r\n".to_string(),
        );
        assert!(wrong.is_empty(), "{} of {} texts did not stay plain and identical:\n{}", wrong.len(), total, wrong.join("\n"));
    }

    /// Lines of `text` (`\r\n` or `\n`), without the empty ones that open or
    /// close it: what the tabs block is known to give back.
    fn text_lines(text: &str) -> Vec<&str> {
        let lines: Vec<&str> = text.lines().collect();
        let first = lines.iter().position(|l| !l.is_empty()).unwrap_or(lines.len());
        let last = lines.iter().rposition(|l| !l.is_empty()).map_or(first, |i| i + 1);
        lines[first..last].to_vec()
    }

    /// Texts that try every marker in every place, and whole blocks.
    fn single_tab_corpus() -> Vec<String> {
        let mut texts: Vec<String> = ["", "plain", "a\r\nb\r\n", "\r\n\r\nx\r\n\r\n"].iter().map(|s| s.to_string()).collect();
        for m in MARKERS {
            for text in [
                format!("{}", m),
                format!("{}\r\n", m),
                format!("{}\naprès", m),
                format!("  {}\naprès", m),
                format!("\n{}\naprès", m),
                format!("avant\r\n{}\r\naprès\r\n", m),
                format!("avant\r\n{}\r\naprès\r\n\r\n", m),
                format!("avant\n{}", m),
                format!("avant\r\n{} x]\r\naprès\r\n", m),
                format!("avant\r\n  {}\r\naprès\r\n", m),
                format!("avant\r\nvoir {} ici\r\naprès\r\n", m),
                format!("{} x]\r\naprès\r\n", m),
            ] {
                texts.push(text);
            }
        }
        let vault = crate::features::vault::serialize(&[SecretEntry::new("fake", "v", "Email")]);
        let attach = crate::features::attachments::serialize(&[Attachment::new("fake.txt", b"x".to_vec())]);
        let history = crate::features::snapshots::serialize(&[Snapshot::new(7, "old")]);
        let state = crate::features::session_state::serialize(&SessionState { active_tab: 3, cursor_line: 9, ..Default::default() });
        let tabs_block = crate::features::tabs::serialize(&[NoteTab::with_content("fake", "x", None)]);
        let prompts = crate::features::prompts::serialize(&[PromptEntry::new("fake", "p", "Cat")]);
        for block in [&vault, &attach, &history, &state, &tabs_block, &prompts] {
            texts.push(block.to_string());
            texts.push(format!("{}\r\n", block));
            texts.push(format!("notes\r\n{}", block));
            texts.push(format!("notes\r\n{}\r\n", block));
            texts.push(format!("notes\r\n{}\r\nmore\r\n", block));
            texts.push(format!("{}\r\nmore", block));
        }
        texts
    }

    // The property of the default single tab, alone in the note: either it is
    // saved as its plain text and read back identical, or it is saved in the
    // tabs block and no line of it is lost.
    #[test]
    fn default_single_tab_is_plain_and_identical_or_switched_without_loss() {
        let mut wrong = Vec::new();
        let corpus = single_tab_corpus();
        let (mut plain, mut switched) = (0, 0);
        for text in &corpus {
            let payload = compose_alone(text);
            if payload == *text {
                plain += 1;
                if let Err(e) = plain_and_identical(text) {
                    wrong.push(format!("plain {:?}: {}", text, e));
                }
                continue;
            }
            switched += 1;
            let p = parse_defaults(&payload);
            let ok = payload.starts_with("[LOCKNOTE_TABS]\n[TAB:Note 1]\n")
                && p.tabs.len() == 1
                && p.tabs[0].name == "Note 1"
                && p.tabs[0].color.is_none()
                && text_lines(&p.tabs[0].content) == text_lines(text)
                && p.session.is_none()
                && p.secrets.is_empty()
                && p.prompts.is_empty()
                && p.attachments.is_empty()
                && p.snapshots.is_empty();
            if !ok {
                wrong.push(format!("switched {:?}: payload {:?}, read back {:?}", text, payload, tab_view(&p.tabs)));
            }
        }
        assert!(plain > 0 && switched > 0, "plain {} switched {}", plain, switched);
        assert!(wrong.is_empty(), "{} of {} texts broke the property:\n{}", wrong.len(), corpus.len(), wrong.join("\n"));
    }

    // ---- pass 3: the decision looks at the text as the reading chain will
    // present it to the readers, not only as it is written ----

    /// The whole reading chain of a saved note: settings header in front,
    /// `Settings::parse` (which drops the line breaks opening the note text),
    /// then `parse_payload`.
    fn through_settings(payload: &str) -> ParsedPayload {
        let stored = crate::settings::Settings::default().serialize(payload);
        let (_settings, note_text) = crate::settings::Settings::parse(&stored);
        parse_defaults(&note_text)
    }

    /// `text` without the line breaks (`\r`, `\n`) that open and close it:
    /// what a saved text is known to give back at best. `Settings::parse`
    /// drops the ones in front, a block after the text or the tabs block
    /// those at the end.
    fn core_text(text: &str) -> &str {
        text.trim_matches(|c| c == '\r' || c == '\n')
    }

    /// Nothing, each block alone, all of them: which reader trims the end of
    /// the text depends on the block that follows it.
    fn arounds_each() -> Vec<Around> {
        let prompts = vec![PromptEntry::new("ask", "Explain", "Work")];
        let secrets = vec![SecretEntry::new("gmail", "s3cret", "Email")];
        let attachments = vec![Attachment::new("a.txt", b"hello".to_vec())];
        let state = SessionState { active_tab: 0, cursor_line: 2, ..Default::default() };
        let d = SessionState::default;
        vec![
            Around { label: "alone", prompts: vec![], secrets: vec![], attachments: vec![], state: d() },
            Around { label: "+state", prompts: vec![], secrets: vec![], attachments: vec![], state: state.clone() },
            Around { label: "+prompts", prompts: prompts.clone(), secrets: vec![], attachments: vec![], state: d() },
            Around { label: "+vault", prompts: vec![], secrets: secrets.clone(), attachments: vec![], state: d() },
            Around { label: "+attachment", prompts: vec![], secrets: vec![], attachments: attachments.clone(), state: d() },
            Around { label: "+all", prompts, secrets, attachments, state },
        ]
    }

    /// Save the default single tab holding `text` with the blocks of `a`,
    /// read it back through the whole chain: what is wrong, if anything.
    /// Line breaks opening and closing the text are not compared.
    fn chain_diff(text: &str, a: &Around) -> Option<String> {
        let payload = compose(&default_tab(text), &a.prompts, &a.secrets, &a.attachments, &a.state);
        let p = through_settings(&payload);
        let expected_session = if a.state == SessionState::default() { None } else { Some(a.state.clone()) };
        if p.tabs.len() != 1 || p.tabs[0].name != "Note 1" || p.tabs[0].color.is_some() {
            return Some(format!("tabs {:?}", tab_view(&p.tabs)));
        }
        if core_text(&p.tabs[0].content) != core_text(text) {
            return Some(format!("text {:?}", p.tabs[0].content));
        }
        if p.prompts != a.prompts {
            return Some(format!("prompts {:?}", p.prompts.iter().map(|x| &x.name).collect::<Vec<_>>()));
        }
        if p.secrets != a.secrets {
            return Some(format!("secrets {:?}", p.secrets));
        }
        if p.attachments != a.attachments {
            return Some(format!("attachments {:?}", p.attachments.iter().map(|x| &x.name).collect::<Vec<_>>()));
        }
        if p.session != expected_session {
            return Some(format!("session {:?}", p.session));
        }
        if !p.snapshots.is_empty() {
            return Some(format!("snapshots {:?}", p.snapshots));
        }
        None
    }

    const STATE_BLOCK: &str = "[LOCKNOTE_STATE]\nactive=1\nline=2\n[/LOCKNOTE_STATE]";
    const PROMPTS_BLOCK: &str = "[LOCKNOTE_PROMPTS]\n[PROMPT:x:c]\nt\n[/PROMPT]\n[/LOCKNOTE_PROMPTS]";

    // The two holes of the second pass, smallest inputs.
    // A: `Settings::parse` drops the `\r` / `\n` opening the text, so a header
    //    behind a lone `\r` there becomes the first line.
    // B: the reader of the block that follows the text trims `\n` then `\r`
    //    from its end, so a header followed by `\r\r` becomes a whole line.
    #[test]
    fn headers_behind_a_lone_cr_are_not_cut_through_the_reading_chain() {
        let mut texts: Vec<String> = Vec::new();
        for h in CUTTING_HEADERS {
            texts.push(format!("\r{}\r\nb", h));
            texts.push(format!("\n\r{}\nb", h));
            texts.push(format!("\r\r{}\r\nb", h));
            texts.push(format!("a\r\n{}\r\r\n", h));
            texts.push(format!("a\r\n{}\r\r", h));
            texts.push(format!("a\n{}\r\r\r\n\n", h));
        }
        texts.push(format!("\r{}", STATE_BLOCK));
        texts.push(format!("\n\r{}\r\n", STATE_BLOCK));
        texts.push(format!("\r{}", PROMPTS_BLOCK));
        texts.push(format!("a\r{}\r\n", PROMPTS_BLOCK));

        let mut lost = Vec::new();
        let mut total = 0;
        for text in &texts {
            for a in arounds_each() {
                total += 1;
                if let Some(diff) = chain_diff(text, &a) {
                    lost.push(format!("{:?} / {}: got {}", text, a.label, diff));
                }
            }
        }
        assert!(lost.is_empty(), "{} of {} cases did not come back:\n{}", lost.len(), total, lost.join("\n"));
    }

    /// The corpus of the second pass plus every marker next to lone `\r`.
    fn chain_corpus() -> Vec<String> {
        let mut texts = single_tab_corpus();
        for m in MARKERS {
            for text in [
                format!("\r{}\r\nb", m),
                format!("\n\r{}\nb", m),
                format!("\r\n\r{}\nb", m),
                format!("\r\r{}\nb", m),
                format!("a\r{}\rb", m),
                format!("a\r{}\rb\r\n", m),
                format!("a\r\n\r{}\r\nb", m),
                format!("a\r\n{}\r\r\n", m),
                format!("a\r\n{}\r\r", m),
                format!("a\r\n{}\r\r\r\n\n", m),
                format!("a\r\n{}\n\r", m),
                format!("{}\r", m),
                format!("{}\r\r", m),
                format!("{}\r\r\n", m),
                format!("{}\n\r", m),
                format!("\r{}", m),
                format!("\r{}\r", m),
            ] {
                texts.push(text);
            }
        }
        let vault = crate::features::vault::serialize(&[SecretEntry::new("fake", "v", "Email")]);
        let attach = crate::features::attachments::serialize(&[Attachment::new("fake.txt", b"x".to_vec())]);
        let history = crate::features::snapshots::serialize(&[Snapshot::new(7, "old")]);
        let prompts = crate::features::prompts::serialize(&[PromptEntry::new("fake", "p", "Cat")]);
        let state_crlf = STATE_BLOCK.replace('\n', "\r\n");
        for block in [vault.as_str(), &attach, &history, &prompts, STATE_BLOCK, &state_crlf] {
            texts.push(format!("\r{}", block));
            texts.push(format!("\n\r{}\r\n", block));
            texts.push(format!("a\r{}", block));
            texts.push(format!("a\r\n{}\r\r\n", block));
            texts.push(format!("a\r\n{}\r", block));
        }
        // Session blocks whose header or footer the reader only sees once
        // the text is cut differently.
        texts.push("\r[LOCKNOTE_STATE]\rx\r[/LOCKNOTE_STATE]\ry\n[/LOCKNOTE_STATE]".to_string());
        texts.push("[LOCKNOTE_STATE]\nx\r[LOCKNOTE_STATE]\r[/LOCKNOTE_STATE]\ry\n[/LOCKNOTE_STATE]".to_string());
        texts.push("\r[LOCKNOTE_STATE]\n[/LOCKNOTE_STATE]\r\r\n".to_string());
        texts.push("a\r[LOCKNOTE_STATE]\r[/LOCKNOTE_STATE]".to_string());
        texts
    }

    /// True when `text` holds a `\r` that is not the first half of `\r\n`.
    fn has_lone_cr(text: &str) -> bool {
        let bytes = text.as_bytes();
        (0..bytes.len()).any(|i| bytes[i] == b'\r' && bytes.get(i + 1) != Some(&b'\n'))
    }

    // Whatever the decision is, plain text or tabs block, and whatever
    // follows the text, the whole chain gives the text back without losing a
    // line and without making up a block.
    #[test]
    fn default_single_tab_survives_the_whole_reading_chain() {
        let corpus = chain_corpus();
        let mut lost = Vec::new();
        let (mut total, mut plain) = (0, 0);
        for text in &corpus {
            let stays_plain = compose_alone(text) == *text;
            plain += stays_plain as usize;
            for a in arounds_each() {
                total += 1;
                if let Some(diff) = chain_diff(text, &a) {
                    lost.push(format!("{} {:?} / {}: got {}", if stays_plain { "plain" } else { "switched" }, text, a.label, diff));
                }
            }
        }
        assert!(plain > 0 && plain < corpus.len(), "plain {} of {}", plain, corpus.len());
        assert!(lost.is_empty(), "{} of {} cases did not come back:\n{}", lost.len(), total, lost.join("\n"));
    }

    // `Settings::parse` drops the line breaks opening the text; the tabs
    // header is looked for after any leading whitespace, so the decision
    // already saw it.
    #[test]
    fn tabs_header_behind_leading_line_breaks_is_seen_before_and_after_settings() {
        for text in [
            "\r\n[LOCKNOTE_TABS]\r\nx",
            "\r[LOCKNOTE_TABS]\nx",
            "\n\r\r\n[LOCKNOTE_TABS] x]\nx",
            "\r\n\r\n \t[LOCKNOTE_TABS]\nx",
            " \r\n[LOCKNOTE_TABS]\nx",
        ] {
            let stored = crate::settings::Settings::default().serialize(text);
            let (_settings, as_read) = crate::settings::Settings::parse(&stored);
            assert!(crate::features::tabs::starts_with_block(&as_read), "reader, {:?}", text);
            assert!(crate::features::tabs::starts_with_block(text), "decision, {:?}", text);
            assert!(compose_alone(text).starts_with("[LOCKNOTE_TABS]\n[TAB:Note 1]\n"), "{:?}", text);
            for a in arounds_each() {
                assert_eq!(chain_diff(text, &a), None, "{:?} / {}", text, a.label);
            }
        }
    }

    // No way back to the regression of the first pass: a lone `\r` does not
    // send a text to the tabs block by itself, nor does a marker next to one
    // when no reader could ever take it for a block.
    #[test]
    fn lone_cr_without_a_block_header_keeps_the_default_single_tab_plain() {
        let mut texts: Vec<String> = vec![
            "a\rb\r\n".to_string(),
            "\ra\r\r\n\r\n".to_string(),
            "a\r\n[LOCKNOTE_STATE]\r\r\n".to_string(),
            "\r[LOCKNOTE_STATE]\r\nactive=1\r\n".to_string(),
            "\r[/LOCKNOTE_STATE]\r\n".to_string(),
            "a\r\n[LOCKNOTE_VAULT] \r\r\n".to_string(),
            "a\r\n [LOCKNOTE_VAULT]\r\r\n".to_string(),
            "a\r\nvoir [LOCKNOTE_ATTACHMENTS]\r\r\n".to_string(),
        ];
        for m in HARMLESS_IN_PLAIN_TEXT {
            if m == "[LOCKNOTE_TABS]" {
                continue; // at the start of the text it is the tabs header
            }
            texts.push(format!("\r{} x]\r\naprès\r\n", m));
            texts.push(format!("avant\r\n{}\r\r\n", m));
            texts.push(format!("avant\r{}\raprès\r\n\r\n", m));
        }
        let mut wrong = Vec::new();
        for text in &texts {
            if let Err(e) = plain_and_identical(text) {
                wrong.push(format!("{:?}: {}", text, e));
            }
        }
        assert!(wrong.is_empty(), "{} of {} texts did not stay plain and identical:\n{}", wrong.len(), texts.len(), wrong.join("\n"));
    }

    // Every layer escapes every marker: a value of the vault is written with
    // the same prefix as a tab text.
    #[test]
    fn secret_value_lines_starting_with_any_marker_are_escaped() {
        let mut wrong = Vec::new();
        for m in MARKERS {
            let secrets = vec![SecretEntry::new("k", &format!("a\n{}\nb", m), "Email")];
            let expected = format!("note\n[LOCKNOTE_VAULT]\n[SECRET:k:Email]\na\n\x00{}\nb\n[/SECRET]\n[/LOCKNOTE_VAULT]", m);
            let got = compose(&[NoteTab::with_content("Note 1", "note", None)], &[], &secrets, &[], &SessionState::default());
            if got != expected {
                wrong.push(format!("{}: {:?}", m, got));
            }
        }
        assert!(wrong.is_empty(), "{} of {} not escaped:\n{}", wrong.len(), MARKERS.len(), wrong.join("\n"));
    }

    // ---- files written by the previous versions keep reading the same ----

    const HELLO_SHA256: &str = "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824";

    // Bytes as 1.2.x wrote them: tabs block (own markers escaped only), history
    // block, vault (own markers escaped only), attachments, session state.
    #[test]
    fn legacy_full_payload_literal_reads_the_same() {
        let payload = format!(
            "[LOCKNOTE_TABS]\n[TAB:A]\nx\r\n\x00[TAB:fake]\r\nvoir [SECRET:k] ici\n[TAB:B:blue]\ny\n\x00[/LOCKNOTE_TABS]\n[/LOCKNOTE_TABS]\n\
             [LOCKNOTE_HISTORY]\n[SNAP:5:label]\nold\n\x00[/SNAP]\n[TAB:not a tab]\n[/SNAP]\n[/LOCKNOTE_HISTORY]\n\
             [LOCKNOTE_VAULT]\n[SECRET:k:Email]\nv1\n\x00[/SECRET]\n\x00[LOCKNOTE_VAULT]\nv2\n[/SECRET]\n[SECRET:host:port]\n8080\n[/SECRET]\n[/LOCKNOTE_VAULT]\n\
             [LOCKNOTE_ATTACHMENTS]\n[FILE:a.txt|size:5|sha256:{}]\naGVsbG8=\n[/FILE]\n[/LOCKNOTE_ATTACHMENTS]\n\
             [LOCKNOTE_STATE]\nactive=1\nline=2\n[/LOCKNOTE_STATE]",
            HELLO_SHA256
        );
        let p = parse_defaults(&payload);
        assert_eq!(p.session, Some(SessionState { active_tab: 1, cursor_line: 2, ..Default::default() }));
        assert_eq!(
            tab_view(&p.tabs),
            vec![
                ("A".to_string(), "x\r\n[TAB:fake]\r\nvoir [SECRET:k] ici".to_string(), None),
                ("B".to_string(), "y\n[/LOCKNOTE_TABS]".to_string(), Some("blue".to_string())),
            ]
        );
        assert_eq!(p.snapshots, vec![Snapshot::with_name(5, "label", "old\n[/SNAP]\n[TAB:not a tab]")]);
        assert_eq!(
            p.secrets,
            vec![
                SecretEntry::new("k", "v1\n[/SECRET]\n[LOCKNOTE_VAULT]\nv2", "Email"),
                SecretEntry::new("host:port", "8080", "General"),
            ]
        );
        assert_eq!(p.attachments, vec![Attachment::new("a.txt", b"hello".to_vec())]);
    }

    // Default single tab as plain text (CRLF from the editor), each block
    // right behind it.
    #[test]
    fn legacy_plain_text_payload_literals_read_the_same() {
        let vault = "[LOCKNOTE_VAULT]\n[SECRET:k:Email]\nv\n[/SECRET]\n[/LOCKNOTE_VAULT]";
        let attach = format!("[LOCKNOTE_ATTACHMENTS]\n[FILE:a.txt|size:5|sha256:{}]\naGVsbG8=\n[/FILE]\n[/LOCKNOTE_ATTACHMENTS]", HELLO_SHA256);
        let state = "[LOCKNOTE_STATE]\nactive=0\nline=4\n[/LOCKNOTE_STATE]";
        let secrets = vec![SecretEntry::new("k", "v", "Email")];
        let atts = vec![Attachment::new("a.txt", b"hello".to_vec())];
        let session = Some(SessionState { active_tab: 0, cursor_line: 4, ..Default::default() });

        // (payload, text, vault?, attachments?, state?)
        let cases: Vec<(String, &str, bool, bool, bool)> = vec![
            (format!("line1\r\nline2\n{}\n{}\n{}", vault, attach, state), "line1\r\nline2", true, true, true),
            (format!("line1\r\nline2\r\n{}", vault), "line1\r\nline2", true, false, false),
            (format!("line1\r\nline2\r\n{}", attach), "line1\r\nline2", false, true, false),
            (format!("line1\r\nline2\r\n{}", state), "line1\r\nline2", false, false, true),
            // Empty text: the first block starts the payload.
            (format!("{}\n{}\n{}", vault, attach, state), "", true, true, true),
            (vault.to_string(), "", true, false, false),
            (attach.clone(), "", false, true, false),
            (state.to_string(), "", false, false, true),
            (format!("{}\n{}", attach, state), "", false, true, true),
        ];
        for (payload, text, has_vault, has_attach, has_state) in cases {
            let p = parse_defaults(&payload);
            assert_eq!(tab_view(&p.tabs), vec![("Note 1".to_string(), text.to_string(), None)], "{:?}", payload);
            assert_eq!(p.secrets, if has_vault { secrets.clone() } else { vec![] }, "{:?}", payload);
            assert_eq!(p.attachments, if has_attach { atts.clone() } else { vec![] }, "{:?}", payload);
            assert_eq!(p.session, if has_state { session.clone() } else { None }, "{:?}", payload);
        }
    }

    // ---- issue 058: the prompts block ----

    // Every line-start position of every marker in a prompt text, with and
    // without the other blocks, one tab or two.
    #[test]
    fn prompt_text_with_marker_roundtrips() {
        let mut lost = Vec::new();
        let mut total = 0;
        for m in MARKERS {
            let mut texts = line_start_texts(m);
            texts.push(("inside a line", format!("voir {} ici", m)));
            for (what, text) in texts {
                for a in arounds() {
                    for (tabs_label, tabs) in [
                        ("single tab", vec![NoteTab::with_content("Note 1", "note", None)]),
                        ("two tabs", vec![NoteTab::with_content("A", "a", None), NoteTab::with_content("B", "b", None)]),
                    ] {
                        total += 1;
                        let prompts = vec![PromptEntry::new("first", &text, "Cat"), PromptEntry::new("second", "kept?", "Other")];
                        if let Some(diff) = roundtrip_diff(&tabs, &prompts, &a.secrets, &a.attachments, &a.state) {
                            lost.push(format!("{} / {} / {} / {}: got {}", m, what, tabs_label, a.label, diff));
                        }
                    }
                }
            }
        }
        assert!(lost.is_empty(), "{} of {} cases did not round-trip:\n{}", lost.len(), total, lost.join("\n"));
    }

    // The cases the plan names: a prompt holding its own end marker, the
    // vault and prompts headers, NUL, CRLF; a tab holding the prompts header
    // on a line of its own.
    #[test]
    fn hostile_prompt_and_tab_texts_roundtrip() {
        let hostile = "start\r\n[/PROMPT]\r\n[LOCKNOTE_VAULT]\n[LOCKNOTE_PROMPTS]\n[/LOCKNOTE_PROMPTS]\nnul \0 here\n \0\n[PROMPT:fake:Cat]\nend\r\n";
        let tab_with_header = "notes\r\n[LOCKNOTE_PROMPTS]\r\n[PROMPT:fake:Cat]\r\nx\r\n[/PROMPT]\r\n[/LOCKNOTE_PROMPTS]\r\nmore notes";
        let prompts = vec![
            PromptEntry::new("hostile", hostile, "Cat"),
            PromptEntry::new("name:with:colons", "after", "Other"),
            PromptEntry::new("[LOCKNOTE_VAULT]", "named like a header", "[x]"),
        ];
        let mut lost = Vec::new();
        for a in arounds_each() {
            for tabs in [
                vec![NoteTab::with_content("Note 1", "note", None)],
                vec![NoteTab::with_content("Note 1", tab_with_header, None)],
                vec![NoteTab::with_content("A", tab_with_header, None), NoteTab::with_content("B", "start\r\n[/PROMPT]\r\n[LOCKNOTE_PROMPTS]\nnul \0 inside\nend", Some("red"))],
            ] {
                if let Some(diff) = roundtrip_diff(&tabs, &prompts, &a.secrets, &a.attachments, &a.state) {
                    lost.push(format!("{} / {:?}: got {}", a.label, tabs[0].content, diff));
                }
                // The tab alone (no prompt): its header line is text.
                if let Some(diff) = roundtrip_diff(&tabs, &[], &a.secrets, &a.attachments, &a.state) {
                    lost.push(format!("{} / no prompt / {:?}: got {}", a.label, tabs[0].content, diff));
                }
            }
        }
        assert!(lost.is_empty(), "{}", lost.join("\n"));
        // A whole block in the middle of the default single tab cannot be
        // taken for the prompts block (it does not end the text): the tab
        // stays plain text.
        assert_eq!(compose_alone(tab_with_header), tab_with_header);
    }

    // Same prefix as a tab text or a vault value (issue 050).
    #[test]
    fn prompt_text_lines_starting_with_any_marker_are_escaped() {
        let mut wrong = Vec::new();
        for m in MARKERS {
            let prompts = vec![PromptEntry::new("k", &format!("a\n{}\nb", m), "Cat")];
            let expected = format!("note\n[LOCKNOTE_PROMPTS]\n[PROMPT:k:Cat]\na\n\x00{}\nb\n[/PROMPT]\n[/LOCKNOTE_PROMPTS]", m);
            let got = compose(&[NoteTab::with_content("Note 1", "note", None)], &prompts, &[], &[], &SessionState::default());
            if got != expected {
                wrong.push(format!("{}: {:?}", m, got));
            }
        }
        assert!(wrong.is_empty(), "{} of {} not escaped:\n{}", wrong.len(), MARKERS.len(), wrong.join("\n"));
    }

    // Write order: tabs, prompts, vault, attachments, state.
    #[test]
    fn prompts_block_sits_between_the_tabs_and_the_vault() {
        let tabs = vec![NoteTab::with_content("A", "a", None), NoteTab::with_content("B", "b", None)];
        let prompts = vec![PromptEntry::new("p", "t", "Cat")];
        let secrets = vec![SecretEntry::new("gmail", "s3cret", "Email")];
        let atts = vec![Attachment::new("a.txt", b"hello".to_vec())];
        let state = SessionState { active_tab: 1, cursor_line: 2, ..Default::default() };
        let payload = compose(&tabs, &prompts, &secrets, &atts, &state);
        let at = |m: &str| crate::features::markers::find_marker_line(&payload, m, 0).unwrap_or_else(|| panic!("{} missing", m));
        let order = [
            at("[/LOCKNOTE_TABS]"),
            at("[LOCKNOTE_PROMPTS]"),
            at("[/LOCKNOTE_PROMPTS]"),
            at("[LOCKNOTE_VAULT]"),
            at("[LOCKNOTE_ATTACHMENTS]"),
            at("[LOCKNOTE_STATE]"),
        ];
        assert!(order.windows(2).all(|w| w[0] < w[1]), "{:?} in {:?}", order, payload);
        // No prompt, no block: the payload of the previous versions.
        assert!(!compose(&tabs, &[], &secrets, &atts, &state).contains("[LOCKNOTE_PROMPTS]"));
    }

    /// What 1.2.24 (no prompts block) does with a payload: the reading chain
    /// of `parse_payload` without the prompts reader, in its order.
    fn read_like_1_2_24(note_text: &str) -> ParsedPayload {
        let cats: Vec<String> = crate::features::vault::CATEGORIES.iter().map(|c| c.to_string()).collect();
        let (session, rest) = crate::features::session_state::parse(note_text);
        let (attachments, rest) = crate::features::attachments::parse(&rest);
        let (secrets, rest) = crate::features::vault::parse_with_categories(&rest, &cats);
        let (snapshots, rest) = crate::features::snapshots::parse(&rest);
        let tabs = crate::features::tabs::parse(&rest);
        ParsedPayload { session, attachments, secrets, prompts: Vec::new(), snapshots, tabs }
    }

    /// What 1.2.24 writes for a default single tab and the blocks it knows.
    /// Its decision to leave the text plain did not know the prompts header,
    /// so it never sends such a text to the tabs block.
    fn write_like_1_2_24(text: &str, secrets: &[SecretEntry], attachments: &[Attachment], state: &SessionState) -> String {
        let text = crate::features::vault::combine(text, secrets);
        let text = crate::features::attachments::combine(&text, attachments);
        crate::features::session_state::combine(&text, state)
    }

    // Downgrade (decision of issue 058): a note saved with prompts and opened
    // by 1.2.24 keeps its vault, attachments and session. With the default
    // single tab, its reader hands the prompts block to the tab as text. Not
    // a way to keep them: 1.2.24 puts that text in the edit control as it is,
    // and the `\x00` of an escaped line ends the text there (WM_SETTEXT), so
    // what it shows and saves is cut at the first escaped prompt line.
    #[test]
    fn downgrade_single_tab_shows_the_prompts_block_as_text_and_keeps_the_rest() {
        let prompts = vec![PromptEntry::new("p1", "line\n[/PROMPT]\nlast", "Cat"), PromptEntry::new("p2", "t", "Cat")];
        let secrets = vec![SecretEntry::new("gmail", "s3cret", "Email")];
        let atts = vec![Attachment::new("a.txt", b"hello".to_vec())];
        let state = SessionState { active_tab: 0, cursor_line: 3, ..Default::default() };
        let payload = compose(&default_tab("my notes"), &prompts, &secrets, &atts, &state);

        let old = read_like_1_2_24(&payload);
        assert_eq!(old.secrets, secrets);
        assert_eq!(old.attachments, atts);
        assert_eq!(old.session, Some(state.clone()));
        assert_eq!(old.tabs.len(), 1);
        assert_eq!(old.tabs[0].content, format!("my notes\n{}", crate::features::prompts::serialize(&prompts)));
        // The escaped `[/PROMPT]` line of p1: where 1.2.24's edit control cuts.
        assert!(old.tabs[0].content.contains("\n\x00[/PROMPT]\n"));

        // Only if the text came back untouched (no escaped line in it) would
        // a save by 1.2.24 keep the prompts readable here.
        let resaved = write_like_1_2_24(&old.tabs[0].content, &old.secrets, &old.attachments, &state);
        let p = parse_defaults(&resaved);
        assert_eq!(p.prompts, prompts);
        assert_eq!(tab_view(&p.tabs), tab_view(&default_tab("my notes")));
        assert_eq!(p.secrets, secrets);
        assert_eq!(p.attachments, atts);
    }

    // Downgrade with the tabs block: 1.2.24 stops reading at
    // `[/LOCKNOTE_TABS]`, so it does not show the prompts at all, and a save
    // by 1.2.24 drops them. The tabs, vault, attachments and session are
    // intact. Hence the rule of issue 058: once prompts are saved, do not go
    // back to 1.2.24 (or export a full backup first).
    #[test]
    fn downgrade_with_the_tabs_block_hides_the_prompts_and_keeps_the_rest() {
        let tabs = vec![NoteTab::with_content("A", "alpha", None), NoteTab::with_content("B", "beta", Some("blue"))];
        let prompts = vec![PromptEntry::new("p1", "t", "Cat")];
        let secrets = vec![SecretEntry::new("gmail", "s3cret", "Email")];
        let atts = vec![Attachment::new("a.txt", b"hello".to_vec())];
        let state = SessionState { active_tab: 1, cursor_line: 3, ..Default::default() };
        let old = read_like_1_2_24(&compose(&tabs, &prompts, &secrets, &atts, &state));
        assert_eq!(tab_view(&old.tabs), tab_view(&tabs));
        assert_eq!(old.secrets, secrets);
        assert_eq!(old.attachments, atts);
        assert_eq!(old.session, Some(state));
        assert!(old.snapshots.is_empty());
    }

    // ---- issue 058, pass 2: notes written by 1.2.24 (where
    // `[LOCKNOTE_PROMPTS]` was not reserved, so never escaped) are read whole.
    // The prompts block is only the one that ends what is left once the vault
    // is removed (`prompts::parse`). ----

    // A tabs block written by 1.2.24 whose first tab holds a line that is
    // exactly `[LOCKNOTE_PROMPTS]`: every tab is read back whole.
    #[test]
    fn old_tabs_block_with_a_prompts_header_line_keeps_every_tab() {
        let old = "[LOCKNOTE_TABS]\n[TAB:A]\nbefore\n[LOCKNOTE_PROMPTS]\nafter\n[TAB:B:blue]\nsecond tab\n[/LOCKNOTE_TABS]";
        let as_old = read_like_1_2_24(old);
        assert_eq!(names(&as_old.tabs), vec![("A", "before\n[LOCKNOTE_PROMPTS]\nafter"), ("B", "second tab")]);
        let p = parse_defaults(old);
        assert_eq!(names(&p.tabs), vec![("A", "before\n[LOCKNOTE_PROMPTS]\nafter"), ("B", "second tab")]);
        assert!(p.prompts.is_empty());
    }

    // Same with a plain default tab (1.2.24's `plain_text_may_be_misread`
    // did not know the prompts header).
    #[test]
    fn old_plain_tab_with_a_prompts_header_line_is_read_whole() {
        let old = "before\n[LOCKNOTE_PROMPTS]\nafter";
        assert_eq!(read_like_1_2_24(old).tabs[0].content, old);
        assert_eq!(parse_defaults(old).tabs[0].content, old);
    }

    // Header and footer lines (a whole pasted block) in the middle of a tab,
    // more text after them, every block of 1.2.24 around: read whole, with
    // the vault, attachments and session.
    #[test]
    fn old_notes_with_a_whole_prompts_block_inside_a_tab_are_read_whole() {
        let inside = "before\r\n[LOCKNOTE_PROMPTS]\r\n[PROMPT:x:c]\r\nt\r\n[/PROMPT]\r\n[/LOCKNOTE_PROMPTS]\r\nafter";
        let secrets = vec![SecretEntry::new("gmail", "s3cret", "Email")];
        let atts = vec![Attachment::new("a.txt", b"hello".to_vec())];
        let state = SessionState { active_tab: 1, cursor_line: 2, ..Default::default() };
        // Plain default tab, as 1.2.24 wrote it.
        let p = parse_defaults(&write_like_1_2_24(inside, &secrets, &atts, &state));
        assert_eq!(tab_view(&p.tabs), tab_view(&default_tab(inside)));
        assert!(p.prompts.is_empty());
        assert_eq!((p.secrets, p.attachments, p.session), (secrets.clone(), atts.clone(), Some(state.clone())));
        // Tabs block, as 1.2.24 wrote it (the prompts markers not escaped).
        let tabs = format!("[LOCKNOTE_TABS]\n[TAB:A]\n{}\n[TAB:B:blue]\nsecond\n[/LOCKNOTE_TABS]", inside);
        let p = parse_defaults(&write_like_1_2_24(&tabs, &secrets, &atts, &state));
        assert_eq!(names(&p.tabs), vec![("A", inside), ("B", "second")]);
        assert!(p.prompts.is_empty());
        assert_eq!((p.secrets, p.attachments, p.session), (secrets, atts, Some(state)));
    }

    // ... and once this version saves such a note with prompts, both the old
    // text and the prompts come back.
    #[test]
    fn old_text_with_a_prompts_header_line_roundtrips_with_new_prompts() {
        let inside = "before\n[LOCKNOTE_PROMPTS]\nafter\n[/LOCKNOTE_PROMPTS]";
        let prompts = vec![PromptEntry::new("p", "t", "Cat")];
        for a in arounds_each() {
            for tabs in [default_tab(inside), vec![NoteTab::with_content("A", inside, None), NoteTab::with_content("B", "b", None)]] {
                assert_eq!(roundtrip_diff(&tabs, &prompts, &a.secrets, &a.attachments, &a.state), None, "{}", a.label);
                assert_eq!(roundtrip_diff(&tabs, &[], &a.secrets, &a.attachments, &a.state), None, "{}", a.label);
            }
        }
        // A plain text ending with a whole block goes to the tabs block.
        assert!(compose_alone(inside).starts_with("[LOCKNOTE_TABS]\n"));
    }
    /// Issue #34: a new note starts with its first tab named in its
    /// language. English and French give the empty text of before.
    #[test]
    fn new_note_text_names_the_first_tab_in_its_language() {
        use crate::features::i18n::Language;
        assert_eq!(new_note_text(Language::English), "");
        assert_eq!(new_note_text(Language::French), "");
        for (lang, name) in [(Language::Spanish, "Nota 1"), (Language::German, "Notiz 1")] {
            let text = new_note_text(lang);
            let p = parse_defaults(&text);
            assert_eq!(tab_view(&p.tabs), vec![(name.to_string(), String::new(), None)], "{lang:?}");
            assert!(p.secrets.is_empty() && p.prompts.is_empty() && p.session.is_none(), "{lang:?}");
        }
    }

    /// Issue #34: once created, the translated name is stored: a save and
    /// a reload in another language keep it.
    #[test]
    fn a_translated_first_tab_name_survives_a_save_in_another_language() {
        let _g = crate::features::i18n::test_support::lang_guard();
        let p = parse_defaults(&new_note_text(crate::features::i18n::Language::German));
        let mut tabs = p.tabs;
        tabs[0].content = "hallo".to_string();
        crate::features::i18n::set_language(crate::features::i18n::Language::French);
        let saved = compose(&tabs, &[], &[], &[], &SessionState::default());
        let back = parse_defaults(&saved);
        assert_eq!(tab_view(&back.tabs), vec![("Notiz 1".to_string(), "hallo".to_string(), None)]);
    }

    /// Issue #34: an existing note with the English default names (default
    /// tab "Note 1" saved as plain text, secrets in default categories, no
    /// saved category list) loads with them in every language, and saves
    /// back byte for byte.
    #[test]
    fn an_existing_note_keeps_its_english_default_names_in_every_language() {
        use crate::features::i18n::{set_language, test_support::lang_guard, Language};
        let _g = lang_guard();
        let tabs = vec![NoteTab::with_content(DEFAULT_TAB_NAME, "my notes", None)];
        let secrets = vec![
            SecretEntry::in_category("gmail", "pw", "Email"),
            SecretEntry::in_category("aws", "key", "API Keys"),
            SecretEntry::in_category("misc", "x", "General"),
        ];
        let stored = compose(&tabs, &[], &secrets, &[], &SessionState::default());
        assert!(!stored.contains("[LOCKNOTE_TABS]"), "default tab saved as plain text: {stored}");
        for lang in [Language::English, Language::French, Language::Spanish, Language::German] {
            set_language(lang);
            let cats = crate::features::vault::categories::effective(&[]);
            let p = parse_payload(&stored, &cats);
            assert_eq!(tab_view(&p.tabs), vec![("Note 1".to_string(), "my notes".to_string(), None)], "{lang:?}");
            assert_eq!(p.secrets, secrets, "{lang:?}");
            let again = compose(&p.tabs, &p.prompts, &p.secrets, &p.attachments, &SessionState::default());
            assert_eq!(again, stored, "{lang:?}");
        }
    }

    // ---- Issue #42 characterisation (written and green before the change) ----

    /// A payload with every block: three tabs (a color, a CRLF, lines that
    /// start with reserved markers and get escaped, a text ending with a line
    /// break), prompts and vault values with escaped lines, an attachment
    /// spread over several base64 lines and an empty one, and a session with
    /// an agent key.
    fn issue42_every_block() -> (Vec<NoteTab>, Vec<PromptEntry>, Vec<SecretEntry>, Vec<Attachment>, SessionState) {
        let tabs = vec![
            NoteTab::with_content("Note 1", "first line\r\n[LOCKNOTE_VAULT]\n  [TAB:x]\nlast \u{e9}", None),
            NoteTab::with_content("Work", "", Some("blue")),
            NoteTab::with_content("\u{dc}n:i", "ends with a break\n", Some("green")),
        ];
        let prompts = vec![
            PromptEntry::new("R\u{e9}sum\u{e9}", "Summarize:\n[/PROMPT]\n\nend", "Writing"),
            PromptEntry::new("Fix", "x\r\n", "Code"),
        ];
        let secrets = vec![
            SecretEntry::in_category("gmail", "p@ss\n[SECRET:x:y]\n[/SECRET]", "Email"),
            SecretEntry::in_category("empty", "", "General"),
            SecretEntry::in_category("multi", "a\n\nb\n", "Custom"),
        ];
        let atts = vec![
            Attachment::new("a.bin", (0..=200u8).collect()),
            Attachment::new("e.txt", Vec::new()),
        ];
        let state = SessionState { active_tab: 2, cursor_line: 7, agent_keys: vec!["ssh-ed25519 AAAAC3 me@host".into()] };
        (tabs, prompts, secrets, atts, state)
    }

    #[test]
    fn issue42_compose_bytes_with_every_block_are_pinned() {
        let (tabs, prompts, secrets, atts, state) = issue42_every_block();
        assert_eq!(compose(&tabs, &prompts, &secrets, &atts, &state), "[LOCKNOTE_TABS]\n[TAB:Note 1]\nfirst line\r\n\0[LOCKNOTE_VAULT]\n\0  [TAB:x]\nlast é\n[TAB:Work:blue]\n[TAB:Ün:i:green]\nends with a break\n\n[/LOCKNOTE_TABS]\n[LOCKNOTE_PROMPTS]\n[PROMPT:Résumé:Writing]\nSummarize:\n\0[/PROMPT]\n\nend\n[/PROMPT]\n[PROMPT:Fix:Code]\nx\r\n\n[/PROMPT]\n[/LOCKNOTE_PROMPTS]\n[LOCKNOTE_VAULT]\n[SECRET:gmail:Email]\np@ss\n\0[SECRET:x:y]\n\0[/SECRET]\n[/SECRET]\n[SECRET:empty:General]\n[/SECRET]\n[SECRET:multi:Custom]\na\n\nb\n\n[/SECRET]\n[/LOCKNOTE_VAULT]\n[LOCKNOTE_ATTACHMENTS]\n[FILE:a.bin|size:201|sha256:747db6ff08731ff7908224c50f71f51fef1283e65341e2dbcdc664f0f41bf8c5]\nAAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8gISIjJCUmJygpKissLS4vMDEyMzQ1Njc4\nOTo7PD0+P0BBQkNERUZHSElKS0xNTk9QUVJTVFVWV1hZWltcXV5fYGFiY2RlZmdoaWprbG1ub3Bx\ncnN0dXZ3eHl6e3x9fn+AgYKDhIWGh4iJiouMjY6PkJGSk5SVlpeYmZqbnJ2en6ChoqOkpaanqKmq\nq6ytrq+wsbKztLW2t7i5uru8vb6/wMHCw8TFxsfI\n[/FILE]\n[FILE:e.txt|size:0|sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855]\n[/FILE]\n[/LOCKNOTE_ATTACHMENTS]\n[LOCKNOTE_STATE]\nactive=2\nline=7\nagent_key=ssh-ed25519 AAAAC3 me@host\n[/LOCKNOTE_STATE]");
        let single = [NoteTab::with_content("Note 1", "plain\r\ntext \u{e9}", None)];
        assert_eq!(compose(&single, &prompts, &secrets, &atts, &state), "plain\r\ntext é\n[LOCKNOTE_PROMPTS]\n[PROMPT:Résumé:Writing]\nSummarize:\n\0[/PROMPT]\n\nend\n[/PROMPT]\n[PROMPT:Fix:Code]\nx\r\n\n[/PROMPT]\n[/LOCKNOTE_PROMPTS]\n[LOCKNOTE_VAULT]\n[SECRET:gmail:Email]\np@ss\n\0[SECRET:x:y]\n\0[/SECRET]\n[/SECRET]\n[SECRET:empty:General]\n[/SECRET]\n[SECRET:multi:Custom]\na\n\nb\n\n[/SECRET]\n[/LOCKNOTE_VAULT]\n[LOCKNOTE_ATTACHMENTS]\n[FILE:a.bin|size:201|sha256:747db6ff08731ff7908224c50f71f51fef1283e65341e2dbcdc664f0f41bf8c5]\nAAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8gISIjJCUmJygpKissLS4vMDEyMzQ1Njc4\nOTo7PD0+P0BBQkNERUZHSElKS0xNTk9QUVJTVFVWV1hZWltcXV5fYGFiY2RlZmdoaWprbG1ub3Bx\ncnN0dXZ3eHl6e3x9fn+AgYKDhIWGh4iJiouMjY6PkJGSk5SVlpeYmZqbnJ2en6ChoqOkpaanqKmq\nq6ytrq+wsbKztLW2t7i5uru8vb6/wMHCw8TFxsfI\n[/FILE]\n[FILE:e.txt|size:0|sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855]\n[/FILE]\n[/LOCKNOTE_ATTACHMENTS]\n[LOCKNOTE_STATE]\nactive=2\nline=7\nagent_key=ssh-ed25519 AAAAC3 me@host\n[/LOCKNOTE_STATE]");
        assert_eq!(compose(&tabs[1..2], &[], &[], &[], &SessionState::default()), "[LOCKNOTE_TABS]\n[TAB:Work:blue]\n[/LOCKNOTE_TABS]");
        assert_eq!(compose(&[], &prompts[1..], &[], &[], &state), "[LOCKNOTE_PROMPTS]\n[PROMPT:Fix:Code]\nx\r\n\n[/PROMPT]\n[/LOCKNOTE_PROMPTS]\n[LOCKNOTE_STATE]\nactive=2\nline=7\nagent_key=ssh-ed25519 AAAAC3 me@host\n[/LOCKNOTE_STATE]");
        let empty_default = [NoteTab::with_content("Note 1", "", None)];
        assert_eq!(compose(&empty_default, &[], &secrets[1..2], &[], &SessionState::default()), "[LOCKNOTE_VAULT]\n[SECRET:empty:General]\n[/SECRET]\n[/LOCKNOTE_VAULT]");
        assert_eq!(compose(&empty_default, &[], &[], &atts[1..], &SessionState::default()), "[LOCKNOTE_ATTACHMENTS]\n[FILE:e.txt|size:0|sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855]\n[/FILE]\n[/LOCKNOTE_ATTACHMENTS]");
        assert_eq!(compose(&[], &[], &[], &[], &state), "[LOCKNOTE_STATE]\nactive=2\nline=7\nagent_key=ssh-ed25519 AAAAC3 me@host\n[/LOCKNOTE_STATE]");
    }

    /// The same payload through the block serializers: `compose` is them,
    /// joined by one line break.
    #[test]
    fn issue42_compose_is_the_block_serializers_joined() {
        let (tabs, prompts, secrets, atts, state) = issue42_every_block();
        let joined = [
            crate::features::tabs::serialize(&tabs),
            crate::features::prompts::serialize(&prompts),
            crate::features::vault::serialize(&secrets),
            crate::features::attachments::serialize(&atts),
            crate::features::session_state::serialize(&state),
        ]
        .join("\n");
        assert_eq!(compose(&tabs, &prompts, &secrets, &atts, &state), joined);
    }

    #[test]
    fn issue42_every_block_payload_reads_back() {
        let (tabs, prompts, secrets, atts, state) = issue42_every_block();
        let cats = vec!["Email".to_string(), "General".to_string(), "Custom".to_string()];
        let p = parse_payload(&compose(&tabs, &prompts, &secrets, &atts, &state), &cats);
        assert_eq!(format!("{:?}", tab_view(&p.tabs)), "[(\"Note 1\", \"first line\\r\\n[LOCKNOTE_VAULT]\\n  [TAB:x]\\nlast é\", None), (\"Work\", \"\", Some(\"blue\")), (\"Ün:i\", \"ends with a break\", Some(\"green\"))]");
        assert_eq!(format!("{:?}", p.prompts), "[PromptEntry { name: \"Résumé\", text: \"Summarize:\\n[/PROMPT]\\n\\nend\", category: \"Writing\" }, PromptEntry { name: \"Fix\", text: \"x\\r\\n\", category: \"Code\" }]");
        assert_eq!(format!("{:?}", p.secrets), "[SecretEntry { name: \"gmail\", value: \"p@ss\\n[SECRET:x:y]\\n[/SECRET]\", category: \"Email\" }, SecretEntry { name: \"empty\", value: \"\", category: \"General\" }, SecretEntry { name: \"multi\", value: \"a\\n\\nb\", category: \"Custom\" }]");
        assert_eq!(p.attachments, atts);
        assert_eq!(p.session, Some(state));
        assert!(p.snapshots.is_empty());
    }

    /// How the block readers take apart text no current writer produces:
    /// blank lines opening a tab or a value, CRLF, escaped lines, an unknown
    /// color or category, an attachment with a wrong hash, unclosed entries.
    #[test]
    fn issue42_block_readers_are_pinned() {
        let cats = vec!["Email".to_string(), "General".to_string()];
        let text = "[LOCKNOTE_TABS]\n[TAB:A]\n\n\nfirst\r\n\n\x00[TAB:esc]\nlast\n\n[TAB:B:mauve]\nb\n[TAB:C:pink]\n[/LOCKNOTE_TABS]\n\
                    [LOCKNOTE_PROMPTS]\n[PROMPT:p:c]\n\x00[/PROMPT]\r\nline\r\n[/PROMPT]\n[PROMPT:open]\nno end\n[/LOCKNOTE_PROMPTS]\n\
                    [LOCKNOTE_VAULT]\n[SECRET:s1:Email]\n\n\nv\n\x00[/SECRET]\n\n[/SECRET]\n[SECRET:s2:Nope]\nx\n[/SECRET]\n[SECRET:s3]\nunclosed\n[/LOCKNOTE_VAULT]\n\
                    [LOCKNOTE_ATTACHMENTS]\n[FILE:ok|size:3|sha256:039058c6f2c0cb492c533b0a4d14ef77cc0f78abccced5287d84a1a2011cfb81]\nAQ\nID\n[/FILE]\n\
                    [FILE:bad|size:3|sha256:00]\nAQID\n[/FILE]\n[FILE:open|size:1|sha256:6e340b9cffb37a989ca544e6bb780a2c78901d3fb33738768511a30617afa01d]\nAA==\n[/LOCKNOTE_ATTACHMENTS]";
        let p = parse_payload(text, &cats);
        assert_eq!(format!("{:?}", tab_view(&p.tabs)), "[(\"A\", \"first\\r\\n\\n[TAB:esc]\\nlast\", None), (\"B:mauve\", \"b\", None), (\"C\", \"\", Some(\"pink\"))]");
        assert_eq!(format!("{:?}", p.prompts), "[PromptEntry { name: \"p\", text: \"[/PROMPT]\\r\\nline\\r\", category: \"c\" }, PromptEntry { name: \"open\", text: \"no end\", category: \"General\" }]");
        assert_eq!(format!("{:?}", p.secrets), "[SecretEntry { name: \"s1\", value: \"v\\n[/SECRET]\", category: \"Email\" }, SecretEntry { name: \"s2:Nope\", value: \"x\", category: \"General\" }, SecretEntry { name: \"s3\", value: \"unclosed\", category: \"General\" }]");
        let atts: Vec<(String, Vec<u8>)> = p.attachments.iter().map(|a| (a.name.clone(), a.data.clone())).collect();
        assert_eq!(format!("{:?}", atts), "[(\"ok\", [1, 2, 3]), (\"open\", [0])]");
        // A legacy plain note, and text with a leading blank line before the tabs header.
        let p = parse_payload("\n\nplain\r\nnote\n", &cats);
        assert_eq!(format!("{:?}", tab_view(&p.tabs)), "[(\"Note 1\", \"\\n\\nplain\\r\\nnote\\n\", None)]");
        let p = parse_payload("  [LOCKNOTE_TABS]\n[TAB:x]\ny\n[/LOCKNOTE_TABS]", &cats);
        assert_eq!(format!("{:?}", tab_view(&p.tabs)), "[(\"x\", \"y\", None)]");
    }

    // ---- Issue #42 ----

    /// `unescape_content` is allocated for its longest result (one byte more
    /// than its input): it never reallocates, so leaves no copy behind.
    #[test]
    fn issue42_unescape_is_allocated_once() {
        for text in ["", "a", "plain text", "two\nlines", "\x00[TAB:x]\nb", "ends\n", "\r\n\r\n"] {
            let out = crate::features::markers::unescape_content(text);
            assert_eq!(out.capacity(), text.len() + 1, "{text:?}");
        }
    }

    /// The parts of the block writers and readers that tests cannot watch
    /// (freed memory), pinned in the source: one growing buffer for the whole
    /// payload, no intermediate text per block, every copy a reader makes
    /// wrapped or wiped.
    #[test]
    fn issue42_payload_blocks_leave_no_copies() {
        let read = |p: &str| crate::read_source(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(p));
        let region = |src: &str, start: &str| -> String {
            let at = src.find(start).unwrap_or_else(|| panic!("{start}"));
            let len = src[at..].find("\n}\n").unwrap();
            src[at..at + len].to_string()
        };
        let payload = read("src/features/payload/mod.rs");
        let compose = region(&payload, "pub fn compose(");
        assert!(compose.contains("let mut out = WipingString::new();"));
        for w in ["tabs::write_block(&mut out, tabs);", "prompts::write_block(&mut out, prompts);", "vault::write_block(&mut out, secrets);",
                  "attachments::write_block(&mut out, attachments);", "session_state::write_block(&mut out, state);"] {
            assert!(compose.contains(w), "{w}");
        }
        assert!(compose.contains("out.push_str(&tabs[0].content);"));
        for banned in ["combine(", "serialize(", "format!", ".clone()", ".to_string()"] {
            assert!(!compose.contains(banned), "an intermediate copy in compose: {banned}");
        }
        let parse = region(&payload, "pub fn parse_payload(");
        assert_eq!(parse.matches("::parse").count(), 6);
        assert_eq!(parse.matches("\n    let rest = Zeroizing::new(rest);\n").count(), 5, "a rest dropped unwiped");

        for (file, block) in [
            ("src/features/tabs/mod.rs", "pub fn write_block(out: &mut WipingString, tabs: &[NoteTab]) {"),
            ("src/features/prompts/mod.rs", "pub fn write_block(out: &mut WipingString, prompts: &[PromptEntry]) {"),
            ("src/features/vault/mod.rs", "pub fn write_block(out: &mut WipingString, secrets: &[SecretEntry]) {"),
            ("src/features/attachments/mod.rs", "pub fn write_block(out: &mut WipingString, attachments: &[Attachment]) {"),
            ("src/features/session_state/mod.rs", "pub fn write_block(out: &mut WipingString, state: &SessionState) {"),
        ] {
            let src = read(file);
            let body = region(&src, block);
            assert!(!body.contains("escape_content(") && !body.contains("format!"), "{file}: a copy of the text");
            let ser = region(&src, "pub fn serialize(");
            assert!(ser.contains("let mut out = WipingString::new();") && ser.contains("write_block(&mut out, "), "{file}");
        }
        for file in ["src/features/tabs/mod.rs", "src/features/prompts/mod.rs", "src/features/vault/mod.rs"] {
            assert!(read(file).contains("write_escaped(out, &"), "{file}");
        }
        let atts = read("src/features/attachments/mod.rs");
        let block = region(&atts, "pub fn write_block(");
        assert!(block.contains("let mut encoded = WipingString::with_capacity(base64::encoded_len(att.data.len(), true).unwrap_or(0));"));
        assert!(block.contains("encoded.push_base64(&att.data);"));
        assert!(block.contains("let mut b64_escaped = WipingString::with_capacity(encoded.len() + 1);"));

        // Readers: the accumulators grow without copies, the texts read are wiped.
        for (file, acc, wrapped, n) in [
            ("src/features/tabs/mod.rs", "let mut current_content = WipingString::new();",
             "Zeroizing::new(unescape_content(strip_trailing_newline(current_content.as_str())))", 2),
            ("src/features/vault/mod.rs", "let mut current_value = WipingString::new();",
             "Zeroizing::new(unescape_content(strip_trailing_newline(current_value.as_str())))", 3),
            ("src/features/snapshots/mod.rs", "let mut current_content = WipingString::new();",
             "Zeroizing::new(unescape_content(strip_trailing_newline(current_content.as_str())))", 3),
            ("src/features/attachments/mod.rs", "let mut current_b64 = WipingString::new();",
             "Zeroizing::new(unescape_content(current_b64.as_str()))", 3),
        ] {
            let src = read(file);
            assert!(src.contains(acc), "{file}");
            assert_eq!(src.matches(wrapped).count(), n, "{file}");
            assert_eq!(src.matches("unescape_content(").count(), n, "{file}: an unwrapped unescaped copy");
            if file != "src/features/attachments/mod.rs" {
                assert!(src.contains("fn strip_trailing_newline(s: &str) -> &str {"), "{file}: strip copies the text");
            }
        }
        let decode = region(&atts, "fn decode_attachment(");
        assert!(decode.contains("let mut clean: Zeroizing<String> = Zeroizing::new(String::with_capacity(b64.len()));"));
        assert_eq!(decode.matches("data.zeroize();\n        return None;").count(), 2, "refused data freed unwiped");
        let prompts = read("src/features/prompts/mod.rs");
        let parse = region(&prompts, "pub fn parse(");
        assert!(parse.contains("let mut text = WipingString::new();"));
        assert!(parse.contains("text.push_str(&zeroize::Zeroizing::new(unescape_content(l)));"));
        assert!(parse.contains("prompts.push(PromptEntry { name, text: text.into_string(), category });"));
        assert!(!parse.contains(".join("));
    }

    /// What `load_payload` hands back is wrapped, and `EditorForm::run`
    /// (which lasts the whole session) wipes the payload once loaded and the
    /// tab's text once in the editor. UI code: pinned in the source.
    #[test]
    fn issue42_the_editor_wipes_what_it_loads() {
        let read = |p: &str| crate::read_source(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(p));
        let edit_ops = read("src/ui/editor/edit_ops.rs");
        assert!(edit_ops.contains("    ) -> (zeroize::Zeroizing<String>, Option<u32>) {\n"));
        assert!(edit_ops.contains("        let content = zeroize::Zeroizing::new(ui_tabs[view.active].content.clone());\n"));
        let editor = read("src/ui/editor/mod.rs");
        let run = &editor[editor.find("    pub fn run(").unwrap()..];
        let run = &run[..run.find("\n    }\n").unwrap()];
        let first = run.find(") {\n").unwrap();
        assert!(run[first..].trim_start_matches(") {\n").trim_start().starts_with("// `run` lasts the whole session"));
        assert!(run.contains("        let payload = Zeroizing::new(content);\n"));
        assert!(run.contains(
            "        let (content, cursor_line) = form.load_payload(&payload, None);\n        drop(payload);\n        form.set_content(&content);\n        drop(content);\n"
        ));
        assert_eq!(run.matches("content").count(), 6, "another use of the payload or the tab text in run");
        let lock_ops = read("src/ui/editor/lock_ops.rs");
        assert_eq!(lock_ops.matches("self.load_payload(&note_text, self.locked_session_state.take());").count(), 2);
        assert_eq!(lock_ops.matches("self.editor.set_text(&note_text);").count(), 2);
    }
}
