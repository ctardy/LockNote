// SSH key dialogs (issue 054)
//
// SshKeygenDialog: type, vault name, comment, optional passphrase (+ strength),
// Generate (worker thread: RSA-4096 takes seconds), then fingerprint and
// public line, Copy public key, Export... (owner-only files) and Save to
// vault. Returns the two vault secrets, or None (the key is then dropped
// and its private part zeroized).
//
// SshAgentDialog: pick a vault entry holding an OpenSSH private key, type
// its passphrase, read the warning (the agent may keep the key outside
// LockNote), Load. Returns (index, passphrase) or None.

use native_windows_gui as nwg;
use std::cell::{Cell, RefCell};
use std::path::Path;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use zeroize::Zeroizing;

use crate::features::ssh_keys::{self, GeneratedKey, KeyType, SshKeyError};
use crate::features::vault::SecretEntry;

fn t(key: &str) -> String {
    crate::features::i18n::get(key).to_string()
}

/// User-facing message for an SSH key error.
pub fn ssh_error_message(e: &SshKeyError) -> String {
    match e {
        SshKeyError::InvalidComment => t("ssh.err.invalid_comment"),
        SshKeyError::EmptyName => t("vault.name.empty"),
        SshKeyError::InvalidName => t("ssh.err.invalid_name"),
        SshKeyError::DuplicateName(n) => t("ssh.err.duplicate").replace("{name}", n),
        SshKeyError::PassphraseRequired => t("ssh.err.passphrase_required"),
        SshKeyError::WrongPassphrase => t("ssh.err.wrong_passphrase"),
        SshKeyError::AgentUnavailable => t("ssh.err.agent_unavailable"),
        SshKeyError::AgentFailure => t("ssh.err.agent_failure"),
        SshKeyError::AgentTimeout => t("ssh.err.agent_timeout"),
        SshKeyError::Generation(_) | SshKeyError::InvalidKey(_) | SshKeyError::AgentProtocol(_) | SshKeyError::Io(_) => {
            t("ssh.err.other").replace("{detail}", &e.to_string())
        }
    }
}

/// Vault name proposed for a key type (as `ssh-keygen` names its files).
fn default_entry_name(kind: KeyType) -> &'static str {
    match kind {
        KeyType::Ed25519 => "id_ed25519",
        KeyType::EcdsaP256 | KeyType::EcdsaP384 => "id_ecdsa",
        KeyType::Rsa3072 | KeyType::Rsa4096 => "id_rsa",
    }
}

fn type_label(kind: KeyType) -> String {
    match kind {
        KeyType::Ed25519 => format!("Ed25519 ({})", t("ssh.gen.recommended")),
        KeyType::EcdsaP256 => "ECDSA P-256".to_string(),
        KeyType::EcdsaP384 => "ECDSA P-384".to_string(),
        KeyType::Rsa3072 => "RSA 3072".to_string(),
        KeyType::Rsa4096 => "RSA 4096".to_string(),
    }
}

fn font() -> nwg::Font {
    let mut font = Default::default();
    nwg::Font::builder()
        .family("Segoe UI")
        .size(16)
        .build(&mut font)
        .expect("Failed to build font");
    font
}

fn label(window: &nwg::Window, font: &nwg::Font, text: &str, pos: (i32, i32), size: (i32, i32)) -> nwg::Label {
    let mut lbl = Default::default();
    nwg::Label::builder()
        .text(text)
        .parent(window)
        .position(pos)
        .size(size)
        .font(Some(font))
        .build(&mut lbl)
        .expect("Failed to build label");
    lbl
}

fn button(window: &nwg::Window, font: &nwg::Font, text: &str, pos: (i32, i32), size: (i32, i32)) -> nwg::Button {
    let mut btn = Default::default();
    nwg::Button::builder()
        .text(text)
        .parent(window)
        .position(pos)
        .size(size)
        .font(Some(font))
        .build(&mut btn)
        .expect("Failed to build button");
    btn
}

fn input(window: &nwg::Window, font: &nwg::Font, text: &str, pos: (i32, i32), size: (i32, i32), password: bool) -> nwg::TextInput {
    let mut txt = Default::default();
    nwg::TextInput::builder()
        .text(text)
        .parent(window)
        .position(pos)
        .size(size)
        .password(if password { Some('*') } else { None })
        .font(Some(font))
        .build(&mut txt)
        .expect("Failed to build text input");
    txt
}

type GenSlot = Arc<Mutex<Option<Result<GeneratedKey, SshKeyError>>>>;

pub struct SshKeygenDialog;

impl SshKeygenDialog {
    /// `existing`: current vault secrets (name collision check).
    pub fn show(existing: &[SecretEntry]) -> Option<[SecretEntry; 2]> {
        let mut window = Default::default();
        nwg::Window::builder()
            .size((520, 580))
            .position((260, 120))
            .title(&t("ssh.gen.title"))
            .flags(nwg::WindowFlags::WINDOW | nwg::WindowFlags::VISIBLE)
            .build(&mut window)
            .expect("Failed to build SshKeygenDialog window");
        super::apply_dialog_theme(&window);
        let font = font();
        let w = 480;

        let _l1 = label(&window, &font, &t("ssh.gen.type"), (20, 15), (w, 20));
        let mut cmb_type = Default::default();
        nwg::ComboBox::builder()
            .parent(&window)
            .position((20, 36))
            .size((w, 25))
            .font(Some(&font))
            .collection(KeyType::ALL.iter().map(|k| type_label(*k)).collect::<Vec<_>>())
            .selected_index(Some(0))
            .build(&mut cmb_type)
            .expect("Failed to build type combo");

        let _l2 = label(&window, &font, &t("ssh.gen.name"), (20, 70), (w, 20));
        let txt_name = input(&window, &font, default_entry_name(KeyType::Ed25519), (20, 91), (w, 25), false);
        let _l3 = label(&window, &font, &t("ssh.gen.comment"), (20, 125), (w, 20));
        let txt_comment = input(&window, &font, "", (20, 146), (w, 25), false);
        let _l4 = label(&window, &font, &t("ssh.gen.passphrase"), (20, 180), (w, 20));
        let txt_pass = input(&window, &font, "", (20, 201), (330, 25), true);
        let lbl_strength = label(&window, &font, "", (360, 204), (140, 20));
        let _l5 = label(&window, &font, &t("ssh.gen.confirm"), (20, 235), (w, 20));
        let txt_confirm = input(&window, &font, "", (20, 256), (330, 25), true);

        let btn_generate = button(&window, &font, &t("ssh.gen.generate"), (20, 292), (140, 30));
        let lbl_status = label(&window, &font, "", (170, 297), (330, 22));

        let _l6 = label(&window, &font, &t("ssh.gen.fingerprint"), (20, 335), (w, 20));
        let lbl_fp = label(&window, &font, "", (20, 356), (w, 22));
        let _l7 = label(&window, &font, &t("ssh.gen.public"), (20, 385), (w, 20));
        let mut txt_public = Default::default();
        nwg::TextBox::builder()
            .parent(&window)
            .position((20, 406))
            .size((w, 60))
            .font(Some(&font))
            .readonly(true)
            .flags(nwg::TextBoxFlags::VISIBLE | nwg::TextBoxFlags::VSCROLL | nwg::TextBoxFlags::AUTOVSCROLL | nwg::TextBoxFlags::TAB_STOP)
            .build(&mut txt_public)
            .expect("Failed to build public key box");

        let btn_copy = button(&window, &font, &t("ssh.gen.copy_public"), (20, 476), (140, 30));
        let btn_save = button(&window, &font, &t("ssh.gen.save_vault"), (165, 476), (140, 30));
        let btn_export = button(&window, &font, &t("ssh.gen.export"), (310, 476), (95, 30));
        let btn_close = button(&window, &font, &t("ssh.gen.close"), (410, 476), (90, 30));
        let lbl_msg = label(&window, &font, "", (20, 515), (w, 44));
        for b in [&btn_copy, &btn_save, &btn_export] {
            b.set_enabled(false);
        }

        let mut notice = Default::default();
        nwg::Notice::builder()
            .parent(&window)
            .build(&mut notice)
            .expect("Failed to build notice");

        let slot: GenSlot = Arc::new(Mutex::new(None));
        let key: Rc<RefCell<Option<GeneratedKey>>> = Rc::new(RefCell::new(None));
        let encrypted = Rc::new(Cell::new(false));
        let last_kind = Rc::new(Cell::new(KeyType::Ed25519));
        let result: Rc<RefCell<Option<[SecretEntry; 2]>>> = Rc::new(RefCell::new(None));
        let existing = existing.to_vec();
        let window_handle = window.handle;

        let (result_c, key_c) = (result.clone(), key.clone());
        let handler = nwg::full_bind_event_handler(&window_handle, move |evt, _data, handle| {
            let clear_secrets = || {
                txt_pass.set_text("");
                txt_confirm.set_text("");
            };
            match evt {
                nwg::Event::OnComboxBoxSelection if handle == cmb_type.handle => {
                    if let Some(kind) = cmb_type.selection().and_then(|i| KeyType::ALL.get(i).copied()) {
                        // Follow the type only while the name is still the proposed one.
                        if txt_name.text() == default_entry_name(last_kind.get()) {
                            txt_name.set_text(default_entry_name(kind));
                        }
                        last_kind.set(kind);
                    }
                }
                nwg::Event::OnTextInput if handle == txt_pass.handle => {
                    let pw = Zeroizing::new(txt_pass.text());
                    lbl_strength.set_text(&if pw.is_empty() {
                        String::new()
                    } else {
                        super::create_password::strength_label(super::create_password::password_score(&pw))
                    });
                }
                nwg::Event::OnButtonClick if handle == btn_generate.handle => {
                    let Some(kind) = cmb_type.selection().and_then(|i| KeyType::ALL.get(i).copied()) else {
                        return;
                    };
                    let pass = Zeroizing::new(txt_pass.text());
                    if *pass != *Zeroizing::new(txt_confirm.text()) {
                        lbl_status.set_text(&t("ssh.gen.mismatch"));
                        return;
                    }
                    let comment = txt_comment.text();
                    if comment.chars().any(char::is_control) {
                        lbl_status.set_text(&ssh_error_message(&SshKeyError::InvalidComment));
                        return;
                    }
                    for b in [&btn_generate, &btn_copy, &btn_save, &btn_export] {
                        b.set_enabled(false);
                    }
                    *key_c.borrow_mut() = None;
                    lbl_fp.set_text("");
                    txt_public.set_text("");
                    lbl_msg.set_text("");
                    lbl_status.set_text(&t("ssh.gen.generating"));
                    encrypted.set(!pass.is_empty());
                    let (slot, sender) = (slot.clone(), notice.sender());
                    std::thread::spawn(move || {
                        let r = ssh_keys::generate(kind, &comment, &pass);
                        if let Ok(mut s) = slot.lock() {
                            *s = Some(r);
                        }
                        sender.notice();
                    });
                }
                nwg::Event::OnNotice if handle == notice.handle => {
                    btn_generate.set_enabled(true);
                    let taken = slot.lock().ok().and_then(|mut s| s.take());
                    match taken {
                        Some(Ok(k)) => {
                            eprintln!("ssh: key generated fingerprint={}", k.fingerprint);
                            lbl_status.set_text("");
                            lbl_fp.set_text(&k.fingerprint);
                            txt_public.set_text(&k.public_line);
                            *key_c.borrow_mut() = Some(k);
                            for b in [&btn_copy, &btn_save, &btn_export] {
                                b.set_enabled(true);
                            }
                        }
                        Some(Err(e)) => {
                            eprintln!("ssh: generation failed err={}", e);
                            lbl_status.set_text(&ssh_error_message(&e));
                        }
                        None => {
                            eprintln!("ssh: generation produced no result");
                            lbl_status.set_text(&ssh_error_message(&SshKeyError::Generation("no result".into())));
                        }
                    }
                }
                nwg::Event::OnButtonClick if handle == btn_copy.handle => {
                    let Some(k) = key_c.borrow().as_ref().map(|k| k.public_line.clone()) else { return };
                    match clipboard_win::set_clipboard(clipboard_win::formats::Unicode, &k) {
                        Ok(()) => lbl_msg.set_text(&t("ssh.gen.copied")),
                        Err(e) => {
                            eprintln!("ssh: copy public key failed err={:?}", e);
                            lbl_msg.set_text(&t("vault.copy.failed"));
                        }
                    }
                }
                nwg::Event::OnButtonClick if handle == btn_export.handle => {
                    // Copy out before the modal boxes below: they run a nested
                    // message loop that must not find the key borrowed.
                    let Some((private, public, fingerprint)) = key_c.borrow().as_ref().map(|k| {
                        (Zeroizing::new(k.private_openssh.to_string()), k.public_line.clone(), k.fingerprint.clone())
                    }) else {
                        return;
                    };
                    if !encrypted.get() {
                        let params = nwg::MessageParams {
                            title: &t("ssh.gen.title"),
                            content: &t("ssh.gen.no_passphrase_export"),
                            buttons: nwg::MessageButtons::YesNo,
                            icons: nwg::MessageIcons::Warning,
                        };
                        if !matches!(nwg::message(&params), nwg::MessageChoice::Yes) {
                            return;
                        }
                    }
                    let mut dialog = nwg::FileDialog::default();
                    nwg::FileDialog::builder()
                        .title(&t("ssh.gen.export_title"))
                        .action(nwg::FileDialogAction::Save)
                        .filters("All Files (*.*)")
                        .build(&mut dialog)
                        .expect("Failed to build file save dialog");
                    if !dialog.run(Some(window_handle)) {
                        return;
                    }
                    let path = match dialog.get_selected_item() {
                        Ok(p) => p,
                        Err(e) => {
                            eprintln!("ssh: export path not read err={:?}", e);
                            return;
                        }
                    };
                    let path = path.to_string_lossy().into_owned();
                    match ssh_keys::win::export_key_pair(Path::new(&path), &private, &public) {
                        Ok(()) => {
                            eprintln!("ssh: key exported fingerprint={}", fingerprint);
                            lbl_msg.set_text(&t("ssh.gen.exported").replace("{path}", &path));
                        }
                        Err(e) => {
                            eprintln!("ssh: export failed err={}", e);
                            lbl_msg.set_text(&ssh_error_message(&e));
                        }
                    }
                }
                nwg::Event::OnButtonClick if handle == btn_save.handle => {
                    let guard = key_c.borrow();
                    let Some(k) = guard.as_ref() else { return };
                    match ssh_keys::vault_entries(&txt_name.text(), k, &existing) {
                        Ok(entries) => {
                            *result_c.borrow_mut() = Some(entries);
                            clear_secrets();
                            nwg::stop_thread_dispatch();
                        }
                        Err(e) => lbl_msg.set_text(&ssh_error_message(&e)),
                    }
                }
                nwg::Event::OnButtonClick if handle == btn_close.handle => {
                    clear_secrets();
                    nwg::stop_thread_dispatch();
                }
                nwg::Event::OnWindowClose => {
                    clear_secrets();
                    nwg::stop_thread_dispatch();
                }
                _ => {}
            }
        });

        nwg::dispatch_thread_events();
        nwg::unbind_event_handler(&handler);
        // Drops the generated key (private part zeroized) if not saved.
        key.borrow_mut().take();
        let out = result.borrow_mut().take();
        out
    }
}

pub struct SshAgentDialog;

impl SshAgentDialog {
    /// `names`: vault entries holding an OpenSSH private key.
    /// Returns the chosen index and the passphrase (empty if none typed).
    pub fn show(names: &[String]) -> Option<(usize, Zeroizing<String>)> {
        let mut window = Default::default();
        nwg::Window::builder()
            .size((480, 330))
            .position((280, 180))
            .title(&t("ssh.agent.title"))
            .flags(nwg::WindowFlags::WINDOW | nwg::WindowFlags::VISIBLE)
            .build(&mut window)
            .expect("Failed to build SshAgentDialog window");
        super::apply_dialog_theme(&window);
        let font = font();
        let w = 440;

        let _l1 = label(&window, &font, &t("ssh.agent.key"), (20, 15), (w, 20));
        let mut cmb_key = Default::default();
        nwg::ComboBox::builder()
            .parent(&window)
            .position((20, 36))
            .size((w, 25))
            .font(Some(&font))
            .collection(names.to_vec())
            .selected_index(Some(0))
            .build(&mut cmb_key)
            .expect("Failed to build key combo");
        let _l2 = label(&window, &font, &t("ssh.agent.passphrase"), (20, 70), (w, 20));
        let txt_pass = input(&window, &font, "", (20, 91), (w, 25), true);
        let _warning = label(
            &window,
            &font,
            &t("ssh.agent.warning"),
            (20, 128),
            (w, 130),
        );
        let btn_load = button(&window, &font, &t("ssh.agent.load"), (w + 20 - 185, 270), (85, 32));
        let btn_cancel = button(&window, &font, &t("settings.cancel"), (w + 20 - 90, 270), (90, 32));
        super::bind_enter_to_button(window.handle, btn_load.handle);
        txt_pass.set_focus();

        let result: Rc<RefCell<Option<(usize, Zeroizing<String>)>>> = Rc::new(RefCell::new(None));
        let result_c = result.clone();
        let window_handle = window.handle;
        let handler = nwg::full_bind_event_handler(&window_handle, move |evt, _data, handle| match evt {
            nwg::Event::OnButtonClick if handle == btn_load.handle => {
                if let Some(i) = cmb_key.selection() {
                    *result_c.borrow_mut() = Some((i, Zeroizing::new(txt_pass.text())));
                }
                txt_pass.set_text("");
                nwg::stop_thread_dispatch();
            }
            nwg::Event::OnButtonClick if handle == btn_cancel.handle => {
                txt_pass.set_text("");
                nwg::stop_thread_dispatch();
            }
            nwg::Event::OnWindowClose => {
                txt_pass.set_text("");
                nwg::stop_thread_dispatch();
            }
            _ => {}
        });

        nwg::dispatch_thread_events();
        nwg::unbind_event_handler(&handler);
        let out = result.borrow_mut().take();
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_names_follow_ssh_keygen() {
        assert_eq!(default_entry_name(KeyType::Ed25519), "id_ed25519");
        assert_eq!(default_entry_name(KeyType::EcdsaP384), "id_ecdsa");
        assert_eq!(default_entry_name(KeyType::Rsa4096), "id_rsa");
    }

    #[test]
    fn every_error_has_a_message() {
        let all = [
            SshKeyError::Generation("x".into()),
            SshKeyError::InvalidKey("x".into()),
            SshKeyError::InvalidComment,
            SshKeyError::EmptyName,
            SshKeyError::InvalidName,
            SshKeyError::DuplicateName("id_rsa".into()),
            SshKeyError::PassphraseRequired,
            SshKeyError::WrongPassphrase,
            SshKeyError::AgentFailure,
            SshKeyError::AgentProtocol("x".into()),
            SshKeyError::AgentUnavailable,
            SshKeyError::AgentTimeout,
            SshKeyError::Io("disk full".into()),
        ];
        for e in &all {
            let m = ssh_error_message(e);
            assert!(!m.is_empty() && !m.starts_with('['), "{:?} -> {}", e, m);
        }
        assert!(ssh_error_message(&SshKeyError::DuplicateName("id_rsa".into())).contains("id_rsa"));
        assert!(ssh_error_message(&SshKeyError::Io("disk full".into())).contains("disk full"));
        assert!(ssh_error_message(&SshKeyError::AgentUnavailable).contains("Start-Service ssh-agent"));
    }
}
