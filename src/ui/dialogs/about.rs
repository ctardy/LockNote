// The crypto line of Help > About LockNote (`help_ops::on_about`).
//
// The former AboutDialog was never shown by the application (only the
// snapshot tool opened it): it is gone (pass 3, #28).

/// The crypto line of the About box, built from the constants `encrypt` is
/// called with, so it cannot drift from them (issue #28). Argon2 notation
/// (m, t, p) needs no translation.
pub(crate) fn crypto_summary() -> String {
    format!(
        "AES-256-CBC + HMAC-SHA256 | Argon2id m={} MiB, t={}, p={}",
        crate::crypto::DEFAULT_M_COST / 1024,
        crate::crypto::DEFAULT_T_COST,
        crate::crypto::DEFAULT_P_LANES,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Issue #28: the About box said PBKDF2 while the key is derived with
    /// Argon2id; the line must state the constants `encrypt` is called with.
    #[test]
    fn crypto_summary_states_the_argon2id_constants() {
        let s = crypto_summary();
        // The MiB figure is exact only for a whole number of MiB.
        assert_eq!(crate::crypto::DEFAULT_M_COST % 1024, 0);
        assert!(s.contains("Argon2id"), "{}", s);
        assert!(!s.contains("PBKDF2"), "{}", s);
        let m = format!("m={} MiB", crate::crypto::DEFAULT_M_COST / 1024);
        let t = format!("t={}", crate::crypto::DEFAULT_T_COST);
        let p = format!("p={}", crate::crypto::DEFAULT_P_LANES);
        for part in [&m, &t, &p] {
            assert!(s.contains(part.as_str()), "{} not in {}", part, s);
        }
    }
}
