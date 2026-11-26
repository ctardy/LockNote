// Command palette (Ctrl+Shift+P) and the command table dispatch.
// Part of the EditorForm implementation. The table, the scorer and the flow
// live in `ui::command_palette`, the dialog in `dialogs::command_palette`.
//
// The menus and the palette run a command through `run_command`: one
// dispatch, so a command does the same from both.
//
// Logs: `palette: open entries=N`, `palette: ran <kind>`, never a secret,
// a tab name or a prompt.

#[allow(unused_imports)]
use super::*;

use crate::ui::command_palette::{
    menu_label_parts, run_palette, save_shortcut_text, CommandId, PaletteEntry, PaletteOutcome, PaletteSteps,
    PaletteTarget, COMMANDS,
};

impl EditorForm {
    /// The menu item of a command.
    pub(super) fn menu_item(&self, id: CommandId) -> &nwg::MenuItem {
        use CommandId::*;
        match id {
            Save => &self.menu_file_save,
            Print => &self.menu_file_print,
            Export => &self.menu_file_export,
            BackupExport => &self.menu_file_backup_export,
            BackupImport => &self.menu_file_backup_import,
            ImportExe => &self.menu_file_import_exe,
            Backups => &self.menu_file_backups,
            Preferences => &self.menu_file_settings_preferences,
            Security => &self.menu_file_settings_security,
            TempFiles => &self.menu_file_settings_tempfiles,
            Quit => &self.menu_file_quit,
            Cut => &self.menu_edit_cut,
            Copy => &self.menu_edit_copy,
            Paste => &self.menu_edit_paste,
            PastePlain => &self.menu_edit_paste_plain,
            Find => &self.menu_edit_find,
            Replace => &self.menu_edit_replace,
            GotoLine => &self.menu_edit_goto_line,
            DuplicateLine => &self.menu_edit_duplicate_line,
            DeleteLine => &self.menu_edit_delete_line,
            SelectAll => &self.menu_edit_select_all,
            InsertTimestamp => &self.menu_edit_insert_timestamp,
            AlwaysOnTop => &self.menu_view_always_on_top,
            WordWrap => &self.menu_view_word_wrap,
            LineNumbers => &self.menu_view_line_numbers,
            NewTab => &self.menu_tab_new,
            CloseTab => &self.menu_tab_close,
            RenameTab => &self.menu_tab_rename,
            NextTab => &self.menu_tab_next,
            PrevTab => &self.menu_tab_prev,
            Vault => &self.menu_tools_vault,
            Prompts => &self.menu_tools_prompts,
            Totp => &self.menu_tools_totp,
            Attachments => &self.menu_tools_attachments,
            PasswordGenerator => &self.menu_tools_passgen,
            StegoHide => &self.menu_tools_stego_hide,
            StegoExtract => &self.menu_tools_stego_extract,
            SshGenerate => &self.menu_tools_ssh_generate,
            SshAgent => &self.menu_tools_ssh_agent,
            CheckUpdates => &self.menu_help_updates,
            About => &self.menu_help_about,
        }
    }

    /// The command of a menu-bar item, `None` for any other handle.
    pub(super) fn command_for_menu(&self, handle: nwg::ControlHandle) -> Option<CommandId> {
        COMMANDS.iter().map(|c| c.id).find(|&id| self.menu_item(id).handle == handle)
    }

    /// Run a command of the table (menu click or palette).
    pub(super) fn run_command(&self, id: CommandId) {
        use CommandId::*;
        match id {
            Save => {
                // on_save reports its own failure to the user.
                let _ = self.on_save();
            }
            Print => self.on_print(),
            Export => self.on_export(),
            BackupExport => self.on_export_backup(),
            BackupImport => self.on_import_backup(),
            ImportExe => self.on_import_from_exe(),
            Backups => self.on_backups(),
            Preferences => self.on_preferences(),
            Security => self.on_security(),
            TempFiles => {
                if self.on_temp_files() {
                    nwg::stop_thread_dispatch();
                }
            }
            Quit => {
                if self.on_close() {
                    nwg::stop_thread_dispatch();
                }
            }
            Cut => self.on_cut(),
            Copy => self.on_copy(),
            Paste => self.on_paste(),
            PastePlain => self.on_paste_plain(),
            Find => self.on_show_find(),
            Replace => self.on_show_replace(),
            GotoLine => self.on_goto_line(),
            DuplicateLine => self.on_duplicate_line(),
            DeleteLine => self.on_delete_line(),
            SelectAll => self.on_select_all(),
            InsertTimestamp => self.on_insert_timestamp(),
            AlwaysOnTop => self.on_toggle_always_on_top(),
            WordWrap => self.on_toggle_word_wrap(),
            LineNumbers => self.on_toggle_line_numbers(),
            NewTab => self.on_new_tab(),
            CloseTab => self.on_close_tab(),
            RenameTab => self.on_rename_tab(),
            NextTab => self.on_next_tab(),
            PrevTab => self.on_prev_tab(),
            Vault => self.on_toggle_vault(),
            Prompts => self.on_toggle_prompts(),
            Totp => self.on_toggle_totp(),
            Attachments => self.on_toggle_attachments(),
            PasswordGenerator => self.on_password_generator(),
            StegoHide => self.on_stego_hide(),
            StegoExtract => self.on_stego_extract(),
            SshGenerate => self.on_ssh_generate(),
            SshAgent => self.on_ssh_agent_load(),
            CheckUpdates => self.on_check_updates(),
            About => self.on_about(),
        }
    }

    /// The palette rows, built now from the menus, the tabs, the vault and
    /// the prompts (names only: no secret value, no prompt text, no borrow
    /// kept).
    fn palette_entries(&self) -> Vec<PaletteEntry> {
        let (save_mod, save_key) = {
            let s = self.settings.borrow();
            (s.save_modifiers, s.save_key)
        };
        let mut entries: Vec<PaletteEntry> = COMMANDS
            .iter()
            .map(|c| {
                let (label, accel) = menu_label_parts(&t(c.label_key));
                // The save shortcut is configurable: shown as the File menu
                // shows it, not as the default of the translation.
                let shortcut = if c.id == CommandId::Save { Some(save_shortcut_text(save_mod, save_key)) } else { accel };
                PaletteEntry { label, shortcut, target: PaletteTarget::Command(c.id) }
            })
            .collect();
        let tab_label = t("palette.tab");
        let tab_names: Vec<String> = self.tab_bar.borrow().all_tabs().into_iter().map(|tab| tab.name.clone()).collect();
        for (index, name) in tab_names.into_iter().enumerate() {
            entries.push(PaletteEntry {
                label: tab_label.replace("{name}", &name),
                shortcut: None,
                target: PaletteTarget::Tab { index, name },
            });
        }
        let vault_label = t("palette.vault");
        for (index, name) in self.vault_panel.names().into_iter().enumerate() {
            entries.push(PaletteEntry {
                label: vault_label.replace("{name}", &name),
                shortcut: None,
                target: PaletteTarget::Secret { index, name },
            });
        }
        let prompt_label = t("palette.prompt");
        for (index, name) in self.prompts_panel.names().into_iter().enumerate() {
            entries.push(PaletteEntry {
                label: prompt_label.replace("{name}", &name),
                shortcut: None,
                target: PaletteTarget::Prompt { index, name },
            });
        }
        entries
    }

    /// Ctrl+Shift+P: show the palette and run the chosen row.
    pub(super) fn on_command_palette(&self) {
        let entries = self.palette_entries();
        eprintln!("INFO palette: open entries={}", entries.len());
        let outcome = run_palette(&FormPaletteSteps { form: self }, &entries);
        match &outcome {
            PaletteOutcome::Cancelled => {}
            PaletteOutcome::DroppedByLock => eprintln!("INFO palette: choice dropped, the note locked meanwhile"),
            PaletteOutcome::Stale => eprintln!("INFO palette: choice dropped, the tab, secret or prompt changed meanwhile"),
            PaletteOutcome::Ran(target) => {
                let kind = match target {
                    PaletteTarget::Command(id) => format!("command {:?}", id),
                    PaletteTarget::Tab { .. } => "tab".to_string(),
                    PaletteTarget::Secret { .. } => "secret".to_string(),
                    PaletteTarget::Prompt { .. } => "prompt".to_string(),
                };
                eprintln!("INFO palette: ran {}", kind);
            }
        }
    }

    /// Copy the secret at `index` the way a click in the vault panel does:
    /// the panel is shown (its "Copied" line is the feedback), the secret
    /// selected in the tree, then the panel's own copy runs (clipboard
    /// auto-clear included).
    pub(super) fn copy_vault_secret_from_palette(&self, index: usize) {
        if !self.vault_panel.is_visible() {
            self.on_toggle_vault();
        }
        self.show_vault_secret(index);
        if self.selected_vault_secret() != Some(index) {
            eprintln!("WARN palette: secret not selected in the vault tree, nothing copied");
            return;
        }
        self.on_vault_copy();
    }
}

/// `PaletteSteps` on the real window.
struct FormPaletteSteps<'a> {
    form: &'a EditorForm,
}

impl PaletteSteps for FormPaletteSteps<'_> {
    fn lock_epoch(&self) -> u64 {
        self.form.lock_epoch()
    }

    fn still_valid(&self, epoch: u64) -> bool {
        self.form.still_valid(epoch)
    }

    fn show(&self, entries: &[PaletteEntry]) -> Option<usize> {
        let _modal = self.form.modal();
        crate::ui::dialogs::command_palette::CommandPaletteDialog::show(entries)
    }

    fn tab_name(&self, index: usize) -> Option<String> {
        self.form.tab_bar.borrow().all_tabs().get(index).map(|tab| tab.name.clone())
    }

    fn secret_name(&self, index: usize) -> Option<String> {
        self.form.vault_panel.names().get(index).cloned()
    }

    fn prompt_name(&self, index: usize) -> Option<String> {
        self.form.prompts_panel.names().get(index).cloned()
    }

    fn run_command(&self, id: CommandId) {
        self.form.editor.set_focus();
        self.form.run_command(id);
    }

    fn switch_to_tab(&self, index: usize) {
        self.form.editor.set_focus();
        self.form.switch_to_tab(index);
    }

    fn copy_secret(&self, index: usize) {
        self.form.copy_vault_secret_from_palette(index);
    }

    /// The panel's own insertion (`insert_prompt_at`): same expansion, same
    /// lock check, one undo step.
    fn insert_prompt(&self, index: usize) {
        self.form.insert_prompt_at(index);
    }
}

#[cfg(test)]
mod tests {
    fn source(file: &str) -> String {
        crate::read_source(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src").join("ui").join("editor").join(file))
    }

    fn body<'a>(src: &'a str, head: &str) -> &'a str {
        let start = src.find(head).unwrap_or_else(|| panic!("{head}"));
        let end = src[start..].find("\n    }\n").unwrap_or_else(|| panic!("end of {head}"));
        &src[start..start + end]
    }

    /// Every menu-bar item of the window is a command of the table: the
    /// palette lists every menu command, and the menus dispatch through it.
    #[test]
    fn every_menu_item_is_in_the_command_table() {
        let fields = source("mod.rs");
        let ops = source("palette_ops.rs");
        let mapping = body(&ops, "pub(super) fn menu_item(");
        let mut missing = Vec::new();
        for line in fields.lines() {
            let l = line.trim();
            if let Some(name) = l.strip_suffix(": nwg::MenuItem,").filter(|n| n.starts_with("menu_")) {
                if !mapping.contains(&format!("&self.{},", name)) {
                    missing.push(name.to_string());
                }
            }
        }
        assert!(missing.is_empty(), "menu items missing from the command table (add a CommandId, a COMMANDS row and a menu_item arm): {:?}", missing);
    }

    #[test]
    fn the_menus_dispatch_through_the_table() {
        let ev = source("events.rs");
        assert!(ev.contains("if let Some(id) = f.command_for_menu(handle) {\n                            f.run_command(id);"));
        // No menu-bar item is dispatched by hand any more.
        for line in ev.lines() {
            assert!(!line.contains("handle == f.menu_") || line.contains("&& handle != f.menu_"), "{}", line.trim());
        }
    }

    #[test]
    fn ctrl_shift_p_opens_the_palette() {
        let ed = source("edit_ops.rs");
        let shortcut = body(&ed, "pub(super) fn handle_shortcut(");
        let shifted = &shortcut[shortcut.find("} else if ctrl && shift {").expect("ctrl+shift branch")..];
        assert!(shifted.contains("0x50 => { self.on_command_palette(); return true; } // Ctrl+Shift+P"));
    }

    /// A secret is copied by the vault panel's own copy, after selecting it.
    #[test]
    fn a_secret_is_copied_through_the_vault_panel_path() {
        let ops = source("palette_ops.rs");
        let copy = body(&ops, "pub(super) fn copy_vault_secret_from_palette(");
        let select = copy.find("self.show_vault_secret(index);").expect("selects");
        let check = copy.find("self.selected_vault_secret() != Some(index)").expect("checks the selection");
        let run = copy.find("self.on_vault_copy();").expect("copies through on_vault_copy");
        assert!(select < check && check < run);
        assert!(!copy.contains("copy_secret_value"), "no second copy path");
    }

    /// The dialog is counted as a modal; the epoch is checked by
    /// `run_palette` (sequence tests in `ui::command_palette`).
    #[test]
    fn the_palette_dialog_is_a_counted_modal() {
        let ops = source("palette_ops.rs");
        let show = body(&ops, "    fn show(&self, entries: &[PaletteEntry])");
        assert!(show.contains("let _modal = self.form.modal();") && show.contains("CommandPaletteDialog::show(entries)"));
        let open = body(&ops, "pub(super) fn on_command_palette(");
        assert!(open.contains("run_palette(&FormPaletteSteps { form: self }, &entries)"));
    }

    /// A prompt row inserts through the panel's path, never a second one.
    #[test]
    fn a_prompt_is_inserted_through_the_panel_path() {
        let ops = source("palette_ops.rs");
        let insert = body(&ops, "    fn insert_prompt(&self, index: usize)");
        assert!(insert.contains("self.form.insert_prompt_at(index);"));
        let entries = body(&ops, "    fn palette_entries(&self)");
        assert!(entries.contains("self.prompts_panel.names()") && !entries.contains("get_prompts"), "names only, no prompt text");
    }

    #[test]
    fn the_save_menu_and_the_palette_show_the_same_shortcut_text() {
        let build = source("build.rs");
        assert!(build.contains("save_shortcut_text(settings.save_modifiers, settings.save_key)"));
        let ops = source("palette_ops.rs");
        assert!(ops.contains("save_shortcut_text(save_mod, save_key)"));
    }
}
