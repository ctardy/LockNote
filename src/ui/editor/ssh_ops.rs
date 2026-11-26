// SSH keys: generation and ssh-agent.
// Part of the EditorForm implementation, split out of the former editor.rs.

#[allow(unused_imports)]
use super::*;

impl EditorForm {
    // =====================================================================
    // SSH keys (issue 054)
    // =====================================================================

    pub(super) fn show_ssh_error(&self, e: &crate::features::ssh_keys::SshKeyError) {
        nwg::modal_info_message(
            &self.window,
            &t("ssh.title"),
            &crate::ui::dialogs::ssh_keygen::ssh_error_message(e),
        );
    }

    /// Tools > SSH Keys > Generate: the dialog returns the two vault secrets
    /// (private key, public line); they go to the `SSH Keys` category, which
    /// is created if missing.
    pub(super) fn on_ssh_generate(&self) {
        use crate::features::ssh_keys::VAULT_CATEGORY;
        let existing = self.vault_panel.get_secrets();
        let Some(mut entries) = crate::ui::dialogs::ssh_keygen::SshKeygenDialog::show(&existing) else {
            return;
        };
        drop(existing);
        if *self.is_locked.borrow() {
            return; // locked while the dialog was open (issue 049)
        }
        // Reuse a category that already matches "SSH Keys" for the user's
        // locale (same rule as the category dialogs), else add it.
        let mut list = self.vault_categories();
        let same = crate::ui::vault_panel::same_category_name;
        let mut found = None;
        for c in &list {
            match same(c, VAULT_CATEGORY) {
                Ok(true) => {
                    found = Some(c.clone());
                    break;
                }
                Ok(false) => {}
                Err(e) => {
                    self.show_vault_category_error(&e);
                    return;
                }
            }
        }
        let category = match found {
            Some(c) => c,
            None => match crate::features::vault::categories::add(&mut list, VAULT_CATEGORY, &same) {
                Ok(c) => {
                    self.apply_vault_categories(list);
                    c
                }
                Err(e) => {
                    self.show_vault_category_error(&e);
                    return;
                }
            },
        };
        for e in entries.iter_mut() {
            e.category = category.clone();
        }
        let [private, public] = entries;
        eprintln!(
            "ssh: key saved to vault fingerprint={}",
            crate::features::ssh_keys::fingerprint(&public.value).unwrap_or_else(|e| format!("<{}>", e)),
        );
        self.vault_panel.add_secret(private);
        self.vault_panel.add_secret(public);
        match self.vault_panel.count().checked_sub(2) {
            Some(new_idx) => self.show_vault_secret(new_idx),
            None => self.refresh_vault_tree(None),
        }
        self.mark_modified();
    }

    /// Tools > SSH Keys > Load into ssh-agent. The key's public line joins
    /// the session list and the file is saved at once: a key is never in
    /// the agent without a trace in the file. If that save fails, the key
    /// is taken out of the agent again.
    pub(super) fn on_ssh_agent_load(&self) {
        use crate::features::ssh_keys::{self as sk, Agent as _};
        const PEM_HEADER: &str = "-----BEGIN OPENSSH PRIVATE KEY-----";
        // Any category: keys can be moved out of "SSH Keys" (drag & drop).
        let keys: Vec<(String, Zeroizing<String>)> = self
            .vault_panel
            .get_secrets()
            .iter()
            .filter(|s| s.value.trim_start().starts_with(PEM_HEADER))
            .map(|s| (s.name.clone(), Zeroizing::new(s.value.clone())))
            .collect();
        if keys.is_empty() {
            nwg::modal_info_message(
                &self.window,
                &t("ssh.title"),
                &t("ssh.agent.none"),
            );
            return;
        }
        let names: Vec<String> = keys.iter().map(|(n, _)| n.clone()).collect();
        let Some((idx, passphrase)) = crate::ui::dialogs::ssh_keygen::SshAgentDialog::show(&names) else {
            return;
        };
        if *self.is_locked.borrow() {
            return; // locked while the dialog was open (issue 049)
        }
        let Some((_, private)) = keys.get(idx) else { return };

        let public_line = match sk::public_line_from_private(private, &passphrase) {
            Ok(line) => line,
            Err(e) => {
                eprintln!("ssh-agent: load refused level=warn err={}", e);
                self.show_ssh_error(&e);
                return;
            }
        };
        // What the session list keeps: comment dropped (it could break the
        // [LOCKNOTE_STATE] block).
        let (tracked, fingerprint) = match sk::agent_key_line(&public_line)
            .and_then(|line| sk::fingerprint(&line).map(|fp| (line, fp)))
        {
            Ok(pair) => pair,
            Err(e) => {
                eprintln!("ssh-agent: load refused level=warn err={}", e);
                self.show_ssh_error(&e);
                return;
            }
        };

        let mut agent = sk::win::PipeAgent;
        let added = agent.add(private, &passphrase);
        match &added {
            Ok(()) => eprintln!("ssh-agent: key added level=info fingerprint={}", fingerprint),
            Err(e) => {
                eprintln!("ssh-agent: add failed level=warn fingerprint={} err={}", fingerprint, e);
                if sk::add_error_is_definitive(e) {
                    self.show_ssh_error(e);
                    return;
                }
                // Timeout / I/O: the request may have reached the agent. Track
                // the key anyway, as unconfirmed: removal keeps it while absent.
                let mut unconfirmed = self.ssh_agent_unconfirmed.borrow_mut();
                if !unconfirmed.iter().any(|k| sk::same_key(k, &tracked)) {
                    unconfirmed.push(tracked.clone());
                }
            }
        }

        let newly_listed = {
            let mut list = self.ssh_agent_keys.borrow_mut();
            if list.iter().any(|k| sk::same_key(k, &tracked)) {
                false
            } else {
                list.push(tracked.clone());
                true
            }
        };
        if let Err(msg) = self.on_save() {
            eprintln!("ssh-agent: save after add failed level=warn fingerprint={} err={}", fingerprint, msg);
            // Never leave a key in the agent with no trace in the file: take
            // it out again, unless an earlier save already lists it.
            let still_loaded = if newly_listed {
                let out = sk::remove_loaded_keys(&mut agent, &[tracked.clone()], &self.ssh_agent_unconfirmed.borrow());
                if out.pending.is_empty() {
                    self.ssh_agent_keys.borrow_mut().retain(|k| !sk::same_key(k, &tracked));
                    false
                } else {
                    eprintln!("ssh-agent: rollback failed level=warn fingerprint={} err={:?}", fingerprint, out.error);
                    true
                }
            } else {
                true
            };
            let key = if still_loaded { "ssh.agent.save_failed_still_loaded" } else { "ssh.agent.save_failed" };
            nwg::modal_info_message(&self.window, &t("ssh.title"), &t(key).replace("{detail}", &msg));
            return;
        }
        match added {
            Ok(()) => {
                nwg::modal_info_message(
                    &self.window,
                    &t("ssh.title"),
                    &t("ssh.agent.loaded").replace("{fingerprint}", &fingerprint),
                );
            }
            // Outcome unknown, key tracked and saved: report the error.
            Err(e) => self.show_ssh_error(&e),
        }
    }

    /// Remove from ssh-agent the keys this file loaded. Keys that could not
    /// be removed stay listed (retried at the next lock, close or unlock).
    /// `interactive`: show the error (unlock, startup, close); the lock path
    /// passes false and only logs.
    pub(super) fn unload_agent_keys(&self, interactive: bool) {
        let loaded = self.ssh_agent_keys.borrow().clone();
        if loaded.is_empty() {
            return;
        }
        let out = crate::features::ssh_keys::remove_loaded_keys(
            &mut crate::features::ssh_keys::win::PipeAgent,
            &loaded,
            &self.ssh_agent_unconfirmed.borrow(),
        );
        eprintln!(
            "ssh-agent: unload level={} removed={} absent={} pending={} err={:?}",
            if out.error.is_some() { "warn" } else { "info" },
            out.removed.len(),
            out.absent.len(),
            out.pending.len(),
            out.error,
        );
        let changed = out.pending.len() != loaded.len();
        // Unconfirmed keys seen in the agent (removed) or no longer tracked
        // are confirmed / done.
        self.ssh_agent_unconfirmed
            .borrow_mut()
            .retain(|u| out.pending.iter().any(|p| crate::features::ssh_keys::same_key(p, u)));
        *self.ssh_agent_keys.borrow_mut() = out.pending;
        // Write the shorter list now, otherwise a stale disk list would remove,
        // at a later unlock, the same key added by hand. Saved silently when
        // nothing else is pending (no "unsaved changes" prompt for it);
        // otherwise the pending save writes it. Not while locked: the
        // password is wiped. A failure shows no box (save_note): the note
        // stays modified, so the next save, or the close flow's question when
        // closing, reports it once.
        let locked = *self.is_locked.borrow();
        if changed && !locked && !self.is_modified.load(Ordering::Relaxed) {
            if let Err(f) = self.save_note() {
                let msg = f.message();
                eprintln!("ssh-agent: list save failed level=warn err={}", msg);
                // Keep the shorter list for the next save rather than lose it.
                self.mark_modified();
            }
        }
        if let (Some(e), true) = (out.error, interactive) {
            nwg::modal_info_message(
                &self.window,
                &t("ssh.title"),
                &t("ssh.agent.unload_failed")
                    .replace("{detail}", &crate::ui::dialogs::ssh_keygen::ssh_error_message(&e)),
            );
        }
    }

    /// Handle vault Add button.
    pub(super) fn on_vault_add(&self) {
        let categories = self.vault_categories();
        let mut copied = None;
        let mut passgen = self.settings.borrow().passgen.clone();
        let result =
            crate::ui::dialogs::secret_edit::SecretEditDialog::show(None, &categories, &mut copied, &mut passgen);
        self.arm_clipboard_guard(copied);
        self.store_passgen_options(passgen);
        if let Some(entry) = result {
            if *self.is_locked.borrow() {
                return; // locked while the dialog was open (issue 049)
            }
            let vault_entry = crate::features::vault::SecretEntry::in_category(
                &entry.name, &entry.value, &entry.category,
            );
            self.vault_panel.add_secret(vault_entry);
            match self.vault_panel.count().checked_sub(1) {
                Some(new_idx) => self.show_vault_secret(new_idx),
                None => self.refresh_vault_tree(None),
            }
            self.mark_modified();
        }
    }

    /// Handle vault Edit button.
    pub(super) fn on_vault_edit(&self) {
        let Some(global_idx) = self.selected_vault_secret() else { return };
        let Some(secret) = self.vault_panel.get_secret(global_idx) else { return };
        let edit_entry = crate::ui::dialogs::secret_edit::SecretEntry {
            name: secret.name.clone(),
            category: secret.category.clone(),
            value: secret.value.clone(),
        };
        let categories = self.vault_categories();
        let mut copied = None;
        let mut passgen = self.settings.borrow().passgen.clone();
        let result = crate::ui::dialogs::secret_edit::SecretEditDialog::show(
            Some(&edit_entry),
            &categories,
            &mut copied,
            &mut passgen,
        );
        self.arm_clipboard_guard(copied);
        self.store_passgen_options(passgen);
        if let Some(updated) = result {
            if *self.is_locked.borrow() {
                return; // locked while the dialog was open (issue 049)
            }
            let vault_entry = crate::features::vault::SecretEntry::in_category(
                &updated.name, &updated.value, &updated.category,
            );
            // Only the secret the dialog was opened on: an import may have
            // replaced the vault while it was open.
            if !self.vault_panel.update_secret_if_unchanged(global_idx, &secret, vault_entry) {
                eprintln!("INFO vault: edit dropped, secret changed meanwhile count={}", self.vault_panel.count());
                self.refresh_vault_tree(None);
                nwg::modal_info_message(
                    &self.window,
                    "LockNote",
                    &t("vault.edit.gone"),
                );
                return;
            }
            self.show_vault_secret(global_idx);
            self.mark_modified();
        }
    }

    /// Handle vault Delete button.
    pub(super) fn on_vault_delete(&self) {
        let Some(global_idx) = self.selected_vault_secret() else { return };
        let title = t("vault.delete.title");
        let content = t("vault.delete.msg");
        let params = nwg::MessageParams {
            title: &title,
            content: &content,
            buttons: nwg::MessageButtons::YesNo,
            icons: nwg::MessageIcons::Warning,
        };
        if self.confirm(&params) {
            if *self.is_locked.borrow() {
                return; // locked while the dialog was open (issue 049)
            }
            self.vault_panel.delete_secret(global_idx);
            self.refresh_vault_tree(None);
            self.mark_modified();
        }
    }

    /// Handle vault Copy button (the only way to copy a secret).
    pub(super) fn on_vault_copy(&self) {
        let Some(global_idx) = self.selected_vault_secret() else { return };
        let Some(result) = self.vault_panel.copy_secret_value(global_idx) else { return };
        let name = self.vault_panel.get_secret(global_idx).map(|e| e.name.clone()).unwrap_or_default();
        let copied = crate::ui::vault_panel::copied_message(&t("vault.copied.named"), &name);
        let shown = match result {
            Ok(copied_text) => {
                self.clipboard_guard.borrow_mut().on_copy(&copied_text);
                Ok(())
            }
            Err(e) => Err(e),
        };
        self.vault_copied.set_text(&crate::ui::copy_feedback(&shown, &copied));
        self.copy_feedback_time.set(Some(std::time::Instant::now()));
    }

    /// Handle vault Reveal button: the value goes to the dedicated read-only
    /// field for 5 seconds, never into the tree (F27).
    pub(super) fn on_vault_reveal(&self) {
        let Some(global_idx) = self.selected_vault_secret() else { return };
        let Some(secret) = self.vault_panel.get_secret(global_idx) else { return };
        self.vault_panel.reveal(global_idx);
        self.vault_reveal.set_text(&secret.value);
        self.vault_reveal.set_visible(true);
    }

    /// Handle vault timer tick.
    pub(super) fn on_vault_timer_tick(&self) {
        if self.vault_panel.check_reveal_expired() {
            self.hide_vault_reveal();
        }
        if let Some(copied_at) = self.copy_feedback_time.get() {
            if copied_at.elapsed() >= std::time::Duration::from_secs(2) {
                self.vault_copied.set_text("");
                self.copy_feedback_time.set(None);
            }
        }
    }
}
