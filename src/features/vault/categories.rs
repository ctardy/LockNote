// Vault categories — user-editable list (issue 052).
//
// The list lives in the encrypted settings block as repeated
// `vault_category=<name>` keys (older versions ignore unknown keys). An empty
// saved list means "never customized": the 7 defaults of `CATEGORIES`.
// `General` is the fallback category: it always exists and can be neither
// renamed nor deleted; secrets of a deleted category move to it.

use super::{SecretEntry, CATEGORIES};

pub const GENERAL: &str = "General";
pub const MAX_CATEGORIES: usize = 50;
pub const MAX_NAME_CHARS: usize = 40;
/// Characters that would break `[SECRET:name:category]` or the settings line.
pub const FORBIDDEN_CHARS: &[char] = &[':', '[', ']', '\n', '\r', '\0'];

#[derive(Debug, PartialEq, Eq)]
pub enum CategoryError {
    Empty,
    TooLong,
    ForbiddenChar(char),
    /// Same name as an existing category (case/accent-insensitive).
    Duplicate(String),
    TooMany,
    GeneralIsFixed,
    NotFound(String),
    Compare(String),
}

/// Equality used for duplicates (case/accent-insensitive in the UI).
pub type SameName<'a> = &'a dyn Fn(&str, &str) -> Result<bool, CategoryError>;

//// The categories in use: the saved list, or the defaults when never
/// customized. `General` is always present.
pub fn effective(saved: &[String]) -> Vec<String> {
    let mut list: Vec<String> = if saved.is_empty() {
        CATEGORIES.iter().map(|c| c.to_string()).collect()
    } else {
        saved.to_vec()
    };
    if !list.iter().any(|c| c == GENERAL) {
        list.insert(0, GENERAL.to_string());
    }
    list
}

/// Validate a new category name; returns it trimmed. `renaming` is the
/// current name when renaming (it does not count as a duplicate).
pub fn validate_name(
    name: &str,
    existing: &[String],
    renaming: Option<&str>,
    same: SameName,
) -> Result<String, CategoryError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(CategoryError::Empty);
    }
    if name.chars().count() > MAX_NAME_CHARS {
        return Err(CategoryError::TooLong);
    }
    if let Some(c) = name.chars().find(|c| FORBIDDEN_CHARS.contains(c)) {
        return Err(CategoryError::ForbiddenChar(c));
    }
    for other in existing {
        if Some(other.as_str()) == renaming {
            continue;
        }
        if same(other, name)? {
            return Err(CategoryError::Duplicate(other.clone()));
        }
    }
    Ok(name.to_string())
}

pub fn add(list: &mut Vec<String>, name: &str, same: SameName) -> Result<String, CategoryError> {
    if list.len() >= MAX_CATEGORIES {
        return Err(CategoryError::TooMany);
    }
    let name = validate_name(name, list, None, same)?;
    list.push(name.clone());
    Ok(name)
}

/// Rename a category; every secret in it follows.
pub fn rename(
    list: &mut [String],
    secrets: &mut [SecretEntry],
    old: &str,
    new: &str,
    same: SameName,
) -> Result<String, CategoryError> {
    if old == GENERAL {
        return Err(CategoryError::GeneralIsFixed);
    }
    let Some(pos) = list.iter().position(|c| c == old) else {
        return Err(CategoryError::NotFound(old.to_string()));
    };
    let new = validate_name(new, list, Some(old), same)?;
    list[pos] = new.clone();
    for s in secrets.iter_mut().filter(|s| s.category == old) {
        s.category = new.clone();
    }
    Ok(new)
}

/// Delete a category; its secrets move to `General`. Returns how many moved.
pub fn delete(list: &mut Vec<String>, secrets: &mut [SecretEntry], name: &str) -> Result<usize, CategoryError> {
    if name == GENERAL {
        return Err(CategoryError::GeneralIsFixed);
    }
    let Some(pos) = list.iter().position(|c| c == name) else {
        return Err(CategoryError::NotFound(name.to_string()));
    };
    list.remove(pos);
    let mut moved = 0;
    for s in secrets.iter_mut().filter(|s| s.category == name) {
        s.category = GENERAL.to_string();
        moved += 1;
    }
    Ok(moved)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn same(a: &str, b: &str) -> Result<bool, CategoryError> {
        Ok(a.to_lowercase() == b.to_lowercase())
    }

    fn defaults() -> Vec<String> {
        CATEGORIES.iter().map(|c| c.to_string()).collect()
    }

    fn secret(name: &str, cat: &str) -> SecretEntry {
        SecretEntry::in_category(name, "v", cat)
    }

    #[test]
    fn never_customized_gives_defaults() {
        assert_eq!(effective(&[]), defaults());
    }

    #[test]
    fn saved_list_is_used_and_general_forced() {
        let saved = vec!["Perso".to_string(), "Work".to_string()];
        let e = effective(&saved);
        assert!(e.contains(&"Perso".to_string()) && e.contains(&"Work".to_string()));
        assert!(e.contains(&GENERAL.to_string()));
        assert_eq!(e.len(), 3);
    }

    #[test]
    fn validate_rules() {
        let l = defaults();
        assert_eq!(validate_name("  Banque  ", &l, None, &same), Ok("Banque".into()));
        assert_eq!(validate_name("   ", &l, None, &same), Err(CategoryError::Empty));
        assert_eq!(validate_name(&"x".repeat(41), &l, None, &same), Err(CategoryError::TooLong));
        assert_eq!(validate_name(&"é".repeat(40), &l, None, &same), Ok("é".repeat(40)));
        for c in [':', '[', ']', '\n'] {
            assert_eq!(validate_name(&format!("a{}b", c), &l, None, &same), Err(CategoryError::ForbiddenChar(c)));
        }
        assert_eq!(validate_name("email", &l, None, &same), Err(CategoryError::Duplicate("Email".into())));
        // Renaming to the same name with another case is allowed.
        assert_eq!(validate_name("EMAIL", &l, Some("Email"), &same), Ok("EMAIL".into()));
    }

    #[test]
    fn add_category() {
        let mut l = defaults();
        assert_eq!(add(&mut l, "Serveurs", &same), Ok("Serveurs".into()));
        assert!(l.contains(&"Serveurs".to_string()));
        assert_eq!(add(&mut l, "serveurs", &same), Err(CategoryError::Duplicate("Serveurs".into())));
    }

    #[test]
    fn max_categories() {
        let mut l: Vec<String> = (0..MAX_CATEGORIES - 1).map(|i| format!("c{}", i)).collect();
        l.push(GENERAL.into());
        assert_eq!(add(&mut l, "one more", &same), Err(CategoryError::TooMany));
    }

    #[test]
    fn rename_moves_secrets() {
        let mut l = defaults();
        let mut s = vec![secret("a", "Email"), secret("b", "Cloud"), secret("c", "Email")];
        assert_eq!(rename(&mut l, &mut s, "Email", "Mails", &same), Ok("Mails".into()));
        assert!(l.contains(&"Mails".to_string()) && !l.contains(&"Email".to_string()));
        assert_eq!(s.iter().map(|x| x.category.as_str()).collect::<Vec<_>>(), vec!["Mails", "Cloud", "Mails"]);
    }

    #[test]
    fn rename_refuses_existing_name() {
        let mut l = defaults();
        let mut s = vec![secret("a", "Email")];
        assert_eq!(rename(&mut l, &mut s, "Email", "cloud", &same), Err(CategoryError::Duplicate("Cloud".into())));
        assert_eq!(s[0].category, "Email");
    }

    #[test]
    fn general_is_fixed() {
        let mut l = defaults();
        let mut s = vec![secret("a", GENERAL)];
        assert_eq!(rename(&mut l, &mut s, GENERAL, "Misc", &same), Err(CategoryError::GeneralIsFixed));
        assert_eq!(delete(&mut l, &mut s, GENERAL), Err(CategoryError::GeneralIsFixed));
    }

    #[test]
    fn delete_moves_secrets_to_general() {
        let mut l = defaults();
        let mut s = vec![secret("a", "Crypto"), secret("b", "Cloud"), secret("c", "Crypto")];
        assert_eq!(delete(&mut l, &mut s, "Crypto"), Ok(2));
        assert!(!l.contains(&"Crypto".to_string()));
        assert_eq!(s.iter().map(|x| x.category.as_str()).collect::<Vec<_>>(), vec![GENERAL, "Cloud", GENERAL]);
    }

    #[test]
    fn unknown_category_errors() {
        let mut l = defaults();
        let mut s = vec![];
        assert_eq!(delete(&mut l, &mut s, "Nope"), Err(CategoryError::NotFound("Nope".into())));
        assert_eq!(rename(&mut l, &mut s, "Nope", "X", &same), Err(CategoryError::NotFound("Nope".into())));
    }

    #[test]
    fn empty_category_is_kept() {
        let mut l = defaults();
        add(&mut l, "Vide", &same).unwrap();
        assert!(effective(&l).contains(&"Vide".to_string()));
    }
}
