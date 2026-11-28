// Vault / SecretStore — Encrypted secret storage
//
// Format:
//   [LOCKNOTE_VAULT]
//   [SECRET:Name:Category]
//   value
//   [/SECRET]
//   [/LOCKNOTE_VAULT]
//
// A line of a value that starts with a reserved marker of the payload (any
// block, see `features::markers`) is written with a `\x00` prefix.

use zeroize::{Zeroize, ZeroizeOnDrop};

pub mod categories;

/// A single secret entry in the vault.
///
/// The `value` field holds sensitive data and is automatically
/// zeroized when the entry is dropped (memory-dump protection).
/// `name` and `category` are considered non-sensitive (displayed
/// in the UI) and are skipped by the zeroizer.
#[derive(Debug, Clone, PartialEq, Zeroize, ZeroizeOnDrop)]
pub struct SecretEntry {
    #[zeroize(skip)]
    pub name: String,
    pub value: String,
    #[zeroize(skip)]
    pub category: String,
}

/// Valid categories.
pub const CATEGORIES: &[&str] = &[
    "General",
    "Email",
    "Cloud",
    "Finance",
    "API Keys",
    "Passwords",
    "Crypto",
];

use crate::crypto::wiping::WipingString;
use crate::features::markers::{
    find_loose_marker_line, find_marker_line, unescape_content, write_escaped, SECRET_END, SECRET_PREFIX, VAULT_FOOTER, VAULT_HEADER,
};
use zeroize::Zeroizing;

/// Ceiling on a single secret's value accumulator. Defensive bound for a
/// malformed `[SECRET:…]` block without its `[/SECRET]` terminator.
const MAX_SECRET_VALUE: usize = 1 * 1024 * 1024; // 1 MiB per secret value

impl SecretEntry {
    /// Entry in one of the 7 default categories (unknown → General). Test
    /// helper since issue 052: production code uses `in_category`.
    #[cfg(test)]
    pub fn new(name: &str, value: &str, category: &str) -> Self {
        let cat = if CATEGORIES.contains(&category) {
            category.to_string()
        } else {
            "General".to_string()
        };
        SecretEntry {
            name: name.to_string(),
            value: value.to_string(),
            category: cat,
        }
    }

    /// Entry in a category already validated against the user's list
    /// (issue 052): no fallback to General.
    pub fn in_category(name: &str, value: &str, category: &str) -> Self {
        SecretEntry {
            name: name.to_string(),
            value: value.to_string(),
            category: category.to_string(),
        }
    }
}

/// Parse vault from input, accepting the user's category list `known`
/// (issue 052). A category absent from `known` keeps the legacy reading.
pub fn parse_with_categories(input: &str, known: &[String]) -> (Vec<SecretEntry>, String) {
    parse_inner(input, &|c: &str| known.iter().any(|k| k == c))
}

/// Parse vault from input. Returns (secrets, remaining_tab_data).
/// The vault block is expected at the end of the input; everything before it is tab data.
/// Only the 7 default categories are recognized (see `parse_with_categories`).
#[cfg(test)]
pub fn parse(input: &str) -> (Vec<SecretEntry>, String) {
    parse_inner(input, &|c: &str| CATEGORIES.contains(&c))
}

/// Where the parser sees the vault block start in `input`: the first line
/// that is the header and nothing else, never text inside a line nor an
/// escaped line of a tab (issue 050).
pub fn header_line(input: &str) -> Option<usize> {
    find_marker_line(input, VAULT_HEADER, 0)
}

/// `header_line` with a lone `\r` counted as a line bound too: where the
/// parser sees the vault header, or could once the reading chain has dropped
/// line breaks around it (`markers::find_loose_marker_line`). For the
/// decision made when writing (`payload::compose`), not for reading.
pub fn loose_header_line(input: &str) -> Option<usize> {
    find_loose_marker_line(input, VAULT_HEADER, 0)
}

fn parse_inner(input: &str, is_known: &dyn Fn(&str) -> bool) -> (Vec<SecretEntry>, String) {
    if input.is_empty() {
        return (Vec::new(), String::new());
    }

    let vault_start = match header_line(input) {
        Some(pos) => pos,
        None => return (Vec::new(), input.to_string()),
    };

    let tab_data = &input[..vault_start];
    // Trim trailing newline from tab data
    let tab_data = tab_data.trim_end_matches('\n').trim_end_matches('\r');

    let after_header = vault_start + VAULT_HEADER.len();

    // Parse line by line (escaped lines start with \x00, so real markers are distinguishable)
    let block = &input[after_header..];

    let mut secrets: Vec<SecretEntry> = Vec::new();
    let mut current_name: Option<String> = None;
    let mut current_category = String::new();
    // Grows without leaving copies; the value read is wiped once in its
    // entry (issue #42).
    let mut current_value = WipingString::new();

    for line in block.split('\n') {
        let trimmed = line.trim_start();

        // Check for real (unescaped) vault footer
        if trimmed == VAULT_FOOTER {
            break;
        }

        // Check for [SECRET:name:category] or [SECRET:name]
        if trimmed.starts_with(SECRET_PREFIX) && !line.starts_with('\x00') {
            if let Some(end) = trimmed.find(']') {
                let inner = &trimmed[SECRET_PREFIX.len()..end];

                // Save previous secret
                if let Some(name) = current_name.take() {
                    let value = Zeroizing::new(unescape_content(strip_trailing_newline(current_value.as_str())));
                    secrets.push(SecretEntry::in_category(&name, &value, &current_category));
                    current_value.clear();
                }

                // Parse name and optional category
                if let Some(colon_pos) = inner.rfind(':') {
                    let maybe_cat = &inner[colon_pos + 1..];
                    if is_known(maybe_cat) {
                        current_name = Some(inner[..colon_pos].to_string());
                        current_category = maybe_cat.to_string();
                    } else {
                        current_name = Some(inner.to_string());
                        current_category = "General".to_string();
                    }
                } else {
                    current_name = Some(inner.to_string());
                    current_category = "General".to_string();
                }
                continue;
            }
        }

        // Check for [/SECRET]
        if trimmed == SECRET_END {
            let name = match current_name.take() {
                Some(n) => n,
                None => continue,
            };
            let value = Zeroizing::new(unescape_content(strip_trailing_newline(current_value.as_str())));
            secrets.push(SecretEntry::in_category(&name, &value, &current_category));
            current_value.clear();
            current_category.clear();
            continue;
        }

        if current_name.is_some() {
            if !current_value.is_empty() {
                current_value.push('\n');
            }
            current_value.push_str(line);
            if current_value.len() > MAX_SECRET_VALUE {
                // Abort this entry and stop parsing further secrets.
                current_name = None;
                current_value.clear();
                current_category.clear();
                break;
            }
        }
    }

    // Save any unclosed secret
    if let Some(name) = current_name.take() {
        let value = Zeroizing::new(unescape_content(strip_trailing_newline(current_value.as_str())));
        secrets.push(SecretEntry::in_category(&name, &value, &current_category));
    }

    (secrets, tab_data.to_string())
}

/// `s` without one trailing `\n` (and the `\r` before it): a slice, not a
/// copy of the text (issue #42).
fn strip_trailing_newline(s: &str) -> &str {
    match s.strip_suffix('\n') {
        Some(rest) => rest.strip_suffix('\r').unwrap_or(rest),
        None => s,
    }
}

/// Serialize secrets into the vault format.
/// (`payload::compose` writes the block with `write_block`.)
#[cfg(test)]
pub fn serialize(secrets: &[SecretEntry]) -> String {
    let mut out = WipingString::new();
    write_block(&mut out, secrets);
    out.into_string()
}

/// The vault block, appended to `out` (nothing for no secret). `out` grows
/// without leaving copies of the values (issue #42).
pub fn write_block(out: &mut WipingString, secrets: &[SecretEntry]) {
    if secrets.is_empty() {
        return;
    }

    out.push_str(VAULT_HEADER);
    out.push('\n');

    for secret in secrets {
        out.push_str(SECRET_PREFIX);
        out.push_str(&secret.name);
        out.push(':');
        out.push_str(&secret.category);
        out.push_str("]\n");
        write_escaped(out, &secret.value);
        if !secret.value.is_empty() && !secret.value.ends_with('\n') {
            out.push('\n');
        }
        out.push_str(SECRET_END);
        out.push('\n');
    }

    out.push_str(VAULT_FOOTER);
}

/// Combine tab data and secrets into a single string (what
/// `payload::compose` writes, for the tests that build a payload block by
/// block).
#[cfg(test)]
pub fn combine(tab_data: &str, secrets: &[SecretEntry]) -> String {
    let vault_block = serialize(secrets);
    if vault_block.is_empty() {
        return tab_data.to_string();
    }
    if tab_data.is_empty() {
        return vault_block;
    }
    format!("{}\n{}", tab_data, vault_block)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn custom_category_roundtrip() {
        let known = vec!["General".to_string(), "Perso".to_string()];
        let secrets = vec![SecretEntry::in_category("Gmail", "pw", "Perso")];
        let (parsed, rest) = parse_with_categories(&combine("note", &secrets), &known);
        assert_eq!(rest, "note");
        assert_eq!(parsed, secrets);
    }

    #[test]
    fn category_absent_from_list_keeps_legacy_reading() {
        // Name containing ':' written without category by old versions,
        // or a category the list does not know: unchanged legacy behaviour.
        let known = vec!["General".to_string()];
        let (parsed, _) = parse_with_categories("[LOCKNOTE_VAULT]\n[SECRET:user:pass]\nv\n[/SECRET]\n[/LOCKNOTE_VAULT]", &known);
        assert_eq!(parsed[0].name, "user:pass");
        assert_eq!(parsed[0].category, "General");
    }

    #[test]
    fn test_no_vault() {
        let (secrets, remaining) = parse("Just tab data here");
        assert!(secrets.is_empty());
        assert_eq!(remaining, "Just tab data here");
    }

    #[test]
    fn test_empty_input() {
        let (secrets, remaining) = parse("");
        assert!(secrets.is_empty());
        assert_eq!(remaining, "");
    }

    #[test]
    fn test_single_secret() {
        let input = "[LOCKNOTE_VAULT]\n[SECRET:Gmail:Email]\nmypassword123\n[/SECRET]\n[/LOCKNOTE_VAULT]";
        let (secrets, remaining) = parse(input);
        assert_eq!(secrets.len(), 1);
        assert_eq!(secrets[0].name, "Gmail");
        assert_eq!(secrets[0].value, "mypassword123");
        assert_eq!(secrets[0].category, "Email");
        assert_eq!(remaining, "");
    }

    #[test]
    fn test_multi_secrets() {
        let input = "[LOCKNOTE_VAULT]\n[SECRET:Gmail:Email]\npass1\n[/SECRET]\n[SECRET:AWS Key:Cloud]\nAKIA123\n[/SECRET]\n[/LOCKNOTE_VAULT]";
        let (secrets, _) = parse(input);
        assert_eq!(secrets.len(), 2);
        assert_eq!(secrets[0].name, "Gmail");
        assert_eq!(secrets[0].category, "Email");
        assert_eq!(secrets[1].name, "AWS Key");
        assert_eq!(secrets[1].category, "Cloud");
        assert_eq!(secrets[1].value, "AKIA123");
    }

    #[test]
    fn test_no_category_defaults_general() {
        let input = "[LOCKNOTE_VAULT]\n[SECRET:MySecret]\nvalue\n[/SECRET]\n[/LOCKNOTE_VAULT]";
        let (secrets, _) = parse(input);
        assert_eq!(secrets[0].category, "General");
    }

    #[test]
    fn test_roundtrip() {
        let secrets = vec![
            SecretEntry::new("Gmail", "password123", "Email"),
            SecretEntry::new("AWS", "AKIAIOSFODNN7EXAMPLE", "Cloud"),
            SecretEntry::new("Bitcoin", "xprv9s21ZrQH...", "Crypto"),
        ];
        let serialized = serialize(&secrets);
        let (parsed, remaining) = parse(&serialized);
        assert_eq!(remaining, "");
        assert_eq!(parsed.len(), 3);
        assert_eq!(parsed[0], secrets[0]);
        assert_eq!(parsed[1], secrets[1]);
        assert_eq!(parsed[2], secrets[2]);
    }

    #[test]
    fn test_multiline_values() {
        let secret = SecretEntry::new("SSH Key", "-----BEGIN RSA PRIVATE KEY-----\nMIIEpAIBAAKCAQEA\n-----END RSA PRIVATE KEY-----", "Passwords");
        let serialized = serialize(&[secret.clone()]);
        let (parsed, _) = parse(&serialized);
        assert_eq!(parsed[0].value, secret.value);
    }

    #[test]
    fn test_combine_with_tabs() {
        let tab_data = "[LOCKNOTE_TABS]\n[TAB:Note 1]\nHello\n[/LOCKNOTE_TABS]";
        let secrets = vec![SecretEntry::new("Test", "value", "General")];
        let combined = combine(tab_data, &secrets);

        assert!(combined.starts_with("[LOCKNOTE_TABS]"));
        assert!(combined.contains("[LOCKNOTE_VAULT]"));

        let (parsed_secrets, parsed_tabs) = parse(&combined);
        assert_eq!(parsed_secrets.len(), 1);
        assert_eq!(parsed_secrets[0].name, "Test");
        assert_eq!(parsed_tabs, tab_data);
    }

    #[test]
    fn test_combine_empty_secrets() {
        let tab_data = "tab content";
        let combined = combine(tab_data, &[]);
        assert_eq!(combined, "tab content");
    }

    #[test]
    fn test_combine_empty_tabs() {
        let secrets = vec![SecretEntry::new("Test", "val", "General")];
        let combined = combine("", &secrets);
        assert!(combined.starts_with("[LOCKNOTE_VAULT]"));
    }

    #[test]
    fn test_serialize_empty() {
        assert_eq!(serialize(&[]), "");
    }

    #[test]
    fn test_all_categories() {
        for cat in CATEGORIES {
            let secret = SecretEntry::new("Test", "value", cat);
            assert_eq!(secret.category, *cat);
        }
    }

    #[test]
    fn test_invalid_category_defaults_general() {
        let secret = SecretEntry::new("Test", "value", "InvalidCat");
        assert_eq!(secret.category, "General");
    }

    #[test]
    fn test_escaping_vault_markers_in_value() {
        let secret = SecretEntry::new("Test", "[SECRET:fake]\n[/SECRET]\n[LOCKNOTE_VAULT]", "General");
        let serialized = serialize(&[secret.clone()]);
        let (parsed, _) = parse(&serialized);
        assert_eq!(parsed[0].value, secret.value);
    }

    #[test]
    fn test_tab_data_before_vault() {
        let input = "Some tab data\nMore data\n[LOCKNOTE_VAULT]\n[SECRET:Key:General]\nval\n[/SECRET]\n[/LOCKNOTE_VAULT]";
        let (secrets, tab_data) = parse(input);
        assert_eq!(tab_data, "Some tab data\nMore data");
        assert_eq!(secrets.len(), 1);
        assert_eq!(secrets[0].name, "Key");
    }

    #[test]
    fn test_empty_name_and_value() {
        let secret = SecretEntry::new("", "", "General");
        assert_eq!(secret.name, "");
        assert_eq!(secret.value, "");
        let serialized = serialize(&[secret]);
        let (parsed, _) = parse(&serialized);
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].name, "");
    }

    #[test]
    fn test_unicode_name_and_value() {
        let secret = SecretEntry::new("Clé secrète 🔑", "Café données 日本語", "General");
        let serialized = serialize(&[secret.clone()]);
        let (parsed, _) = parse(&serialized);
        assert_eq!(parsed[0].name, secret.name);
        assert_eq!(parsed[0].value, secret.value);
    }

    #[test]
    fn test_multiline_value() {
        let secret = SecretEntry::new("Multi", "line1\nline2\nline3", "General");
        let serialized = serialize(&[secret.clone()]);
        let (parsed, _) = parse(&serialized);
        assert_eq!(parsed[0].value, "line1\nline2\nline3");
    }

    #[test]
    fn test_parse_no_vault_returns_empty() {
        let input = "Just plain text, no vault";
        let (secrets, tab_data) = parse(input);
        assert!(secrets.is_empty());
        assert_eq!(tab_data, input);
    }

    #[test]
    fn test_multiple_secrets_roundtrip() {
        let secrets = vec![
            SecretEntry::new("A", "val_a", "Email"),
            SecretEntry::new("B", "val_b", "Finance"),
            SecretEntry::new("C", "val_c", "Cloud"),
        ];
        let serialized = serialize(&secrets);
        let (parsed, _) = parse(&serialized);
        assert_eq!(parsed.len(), 3);
        assert_eq!(parsed[0].name, "A");
        assert_eq!(parsed[1].category, "Finance");
        assert_eq!(parsed[2].value, "val_c");
    }

    #[test]
    fn test_colon_in_name() {
        let secret = SecretEntry::new("host:port", "8080", "General");
        let serialized = serialize(&[secret]);
        let (parsed, _) = parse(&serialized);
        assert_eq!(parsed[0].name, "host:port");
    }

    // Regression test for 6859d13: "Fix vault secrets lost on reload after manual save".
    // Write order is tabs → snapshots → vault, so [VAULT] sits AFTER [HISTORY].
    // Read order must be vault BEFORE snapshots: vault::parse() splits on its own header
    // and returns everything before it as remaining. If snapshots::parse ran first, it
    // would strip [HISTORY] but also discard the [VAULT] block that follows it.
    #[test]
    fn test_regression_vault_survives_after_snapshots_block() {
        use crate::features::snapshots::{self, Snapshot};

        let tab_data = "Some note content\nwith multiple lines";
        let snaps = vec![
            Snapshot::new(1_700_000_000, "old version 1"),
            Snapshot::new(1_700_000_100, "old version 2"),
        ];
        let secrets = vec![
            SecretEntry::new("Gmail", "pass1", "Email"),
            SecretEntry::new("AWS", "AKIA123", "Cloud"),
        ];

        // Write order: tabs → snapshots → vault (matches editor.rs:3796-3808)
        let payload = snapshots::combine(tab_data, &snaps);
        let payload = combine(&payload, &secrets);

        // Read in correct order: vault FIRST, then snapshots
        let (parsed_secrets, remaining) = parse(&payload);
        let (parsed_snaps, remaining) = snapshots::parse(&remaining);

        assert_eq!(parsed_secrets.len(), 2, "vault secrets must survive snapshots block");
        assert_eq!(parsed_secrets[0].name, "Gmail");
        assert_eq!(parsed_secrets[1].value, "AKIA123");
        assert_eq!(parsed_snaps.len(), 2);
        assert_eq!(parsed_snaps[0].content, "old version 1");
        assert_eq!(remaining, tab_data);
    }

    // The wrong order (snapshots before vault) is what caused the bug: it silently
    // dropped the vault block. This test documents that failure mode so the fix
    // stays honest.
    #[test]
    fn test_regression_wrong_order_loses_vault() {
        use crate::features::snapshots::{self, Snapshot};

        let tab_data = "tab";
        let snaps = vec![Snapshot::new(1_700_000_000, "v1")];
        let secrets = vec![SecretEntry::new("Key", "secret", "General")];

        let payload = snapshots::combine(tab_data, &snaps);
        let payload = combine(&payload, &secrets);

        // Wrong read order: snapshots first — the [VAULT] block trailing [/HISTORY]
        // is discarded because snapshots::parse returns only content BEFORE its header.
        let (_parsed_snaps, remaining) = snapshots::parse(&payload);
        let (parsed_secrets, _) = parse(&remaining);

        assert!(parsed_secrets.is_empty(), "wrong order must drop vault — proves bug existed");
    }
}
