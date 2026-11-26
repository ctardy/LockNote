// i18n module — Internationalization system
//
// Supports English, French, Spanish, German with fallback chain:
// current language -> English -> "[key]" placeholder.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::OnceLock;

// ---------------------------------------------------------------------------
// Language enum
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Language {
    English,
    French,
    Spanish,
    German,
}

impl Language {
    /// BCP-47 code.
    pub fn code(&self) -> &'static str {
        match self {
            Language::English => "en",
            Language::French => "fr",
            Language::Spanish => "es",
            Language::German => "de",
        }
    }
}

// ---------------------------------------------------------------------------
// Global state
// ---------------------------------------------------------------------------

static CURRENT_LANG: AtomicU8 = AtomicU8::new(0); // 0=EN

fn lang_to_u8(lang: Language) -> u8 {
    match lang {
        Language::English => 0,
        Language::French  => 1,
        Language::Spanish => 2,
        Language::German  => 3,
    }
}

fn u8_to_lang(v: u8) -> Language {
    match v {
        1 => Language::French,
        2 => Language::Spanish,
        3 => Language::German,
        _ => Language::English,
    }
}

pub fn set_language(lang: Language) {
    CURRENT_LANG.store(lang_to_u8(lang), Ordering::SeqCst);
}

pub fn current_language() -> Language {
    u8_to_lang(CURRENT_LANG.load(Ordering::SeqCst))
}

// ---------------------------------------------------------------------------
// Dictionary storage
// ---------------------------------------------------------------------------

type Dict = HashMap<&'static str, &'static str>;

fn en_dict() -> &'static Dict {
    static DICT: OnceLock<Dict> = OnceLock::new();
    DICT.get_or_init(|| build_en())
}

fn fr_dict() -> &'static Dict {
    static DICT: OnceLock<Dict> = OnceLock::new();
    DICT.get_or_init(|| build_fr())
}

fn es_dict() -> &'static Dict {
    static DICT: OnceLock<Dict> = OnceLock::new();
    DICT.get_or_init(|| build_es())
}

fn de_dict() -> &'static Dict {
    static DICT: OnceLock<Dict> = OnceLock::new();
    DICT.get_or_init(|| build_de())
}

fn dict_for(lang: Language) -> &'static Dict {
    match lang {
        Language::English => en_dict(),
        Language::French  => fr_dict(),
        Language::Spanish => es_dict(),
        Language::German  => de_dict(),
    }
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Look up a translation key. Fallback: current lang -> English -> "[key]".
///
/// For keys containing `{}` placeholders (like `status.stats`), callers must
/// do their own replacement on the returned template string.
pub fn get(key: &str) -> &str {
    let lang = current_language();
    if let Some(val) = dict_for(lang).get(key) {
        return val;
    }
    if lang != Language::English {
        if let Some(val) = en_dict().get(key) {
            return val;
        }
    }
    // Return a leaked "[key]" placeholder so we can return &str.
    // This only happens for genuinely unknown keys, so the leak is bounded.
    Box::leak(format!("[{}]", key).into_boxed_str())
}

// ---------------------------------------------------------------------------
// English translations
// ---------------------------------------------------------------------------

fn build_en() -> Dict {
    let mut m = HashMap::new();

    // App
    m.insert("app.title", "LockNote");
    m.insert("app.title.modified", "LockNote *");

    // File menu
    m.insert("menu.file", "&File");
    m.insert("menu.file.save", "&Save\tCtrl+S");
    m.insert("menu.file.changepass", "Change &Password...");
    m.insert("menu.file.settings", "S&ettings...");
    m.insert("menu.file.quit", "&Quit\tAlt+F4");
    m.insert("menu.file.print", "&Print...\tCtrl+P");
    m.insert("print.footer", "Page {n} / {m}");
    m.insert("print.error.title", "Print");
    m.insert("print.error.failed", "Printing failed: {error}");
    m.insert("print.error.too_small", "The printable area of the page is too small for the editor font.");
    m.insert("menu.file.export", "E&xport...");

    // View menu
    m.insert("menu.view", "&View");
    m.insert("menu.view.ontop", "Always on &Top");
    m.insert("menu.view.wordwrap", "&Word Wrap");
    m.insert("menu.view.linenumbers", "&Line Numbers");
    m.insert("settings.font.failed", "The new editor font could not be applied; the editor keeps its current font.\n\nDetails: {detail}");
    m.insert("menu.view.vault", "&Vault Manager...");

    // Tab menu
    m.insert("menu.tab", "&Tab");
    m.insert("menu.tab.new", "&New Tab\tCtrl+T");
    m.insert("menu.tab.close", "&Close Tab\tCtrl+W");
    m.insert("menu.tab.rename", "&Rename Tab...\tF2");
    m.insert("menu.tab.next", "Next Tab\tCtrl+Tab");
    m.insert("menu.tab.prev", "Previous Tab\tCtrl+Shift+Tab");
    m.insert("menu.tab.compare", "Compare with Last &Save...");
    m.insert("compare.title", "Compare with Last Save");
    m.insert("compare.identical", "No changes since the last save.");
    m.insert("compare.too_large", "This tab is too long to compare (more than {} lines on one side).");
    m.insert("compare.failed", "The comparison could not be computed.");
    m.insert("compare.close", "Close");
    m.insert("menu.edit.goto_heading", "Go to &Heading...\tCtrl+Shift+O");
    m.insert("heading.title", "Go to Heading");
    m.insert("heading.filter", "Filter:");
    m.insert("heading.none", "No heading in this tab.");
    m.insert("heading.no_match", "No heading matches the filter.");
    m.insert("tab.limit.title", "Tab Limit");
    m.insert("tab.limit.msg", "Maximum number of tabs reached (20).");
    m.insert("tab.close.title", "Close Tab");
    m.insert("tab.close.msg", "Close tab \"{name}\"?\nIts content will be lost.");
    m.insert("tab.ctx.rename", "Rename Tab...");
    m.insert("tab.ctx.color", "Color");
    m.insert("tab.ctx.no_color", "No color");
    m.insert("tab.ctx.close", "Close Tab");

    // Tools menu
    m.insert("menu.tools", "&Tools");
    m.insert("menu.tools.changepass", "Change &Password...");
    m.insert("menu.tools.totp", "&TOTP Manager");

    // Settings submenu
    m.insert("menu.settings.preferences", "&Preferences...");
    m.insert("menu.settings.security", "&Security...");
    m.insert("menu.settings.tempfiles", "&Temporary files...");

    // Path of the LockNote file in the status bar, temporary files dialog (issue 057)
    m.insert("status.path_copied", "Path copied");
    m.insert("status.path_copy_failed", "Could not copy the path");
    m.insert("tempfiles.owner.current", "This LockNote file");
    m.insert("tempfiles.owner.present", "Another LockNote file");
    m.insert("tempfiles.owner.orphan", "Orphan: its LockNote file is gone");
    m.insert("tempfiles.owner.folder_missing", "Its folder is absent (drive disconnected?)");
    m.insert("tempfiles.owner.unreachable", "Its LockNote file cannot be checked");
    m.insert("tempfiles.owner.no_companion", "Left by an older version");
    m.insert("tempfiles.owner.companion_unusable", "Companion file unusable");
    m.insert("tempfiles.meaning.current", "The pending save of the LockNote file that is open. It is written into it when LockNote closes.");
    m.insert("tempfiles.meaning.present", "A save waiting for that LockNote file: it is written into it the next time that file is opened then closed. Kept.");
    m.insert("tempfiles.meaning.orphan", "That LockNote file no longer exists, but its folder does. This file holds the last save made with it: deleting it loses those notes.");
    m.insert("tempfiles.meaning.folder_missing", "Neither that LockNote file nor its folder can be found: drive or network share disconnected, or folder removed. Kept: reconnect the drive and open that file.");
    m.insert("tempfiles.meaning.unreachable", "Whether that LockNote file exists cannot be checked ({error}). Kept.");
    m.insert("tempfiles.meaning.no_companion", "Left by LockNote 1.2.23 or older, which did not record the LockNote file it belongs to. Kept: that file takes it over when it is opened.");
    m.insert("tempfiles.meaning.companion_unusable", "Its companion file cannot be used ({reason}). Kept.");
    m.insert("tempfiles.owner.other_volume", "Its drive now names another disk");
    m.insert("tempfiles.owner.tmp_unreadable", "Cannot be examined");
    m.insert("tempfiles.meaning.other_volume", "That LockNote file is not found, and its drive letter or share now names another disk than when it was saved (a USB key replaced by another). Kept: put the right disk back and open that file.");
    m.insert("tempfiles.meaning.tmp_unreadable", "This file cannot be examined ({error}). Kept.");
    m.insert("tempfiles.dlg.reading", "Reading...");
    m.insert("tempfiles.dlg.title", "Temporary files");
    m.insert("tempfiles.dlg.intro", "LockNote cannot rewrite its own program file while it runs. Each save is first written to a temporary file in the folder below; when LockNote closes, a helper copies it into the LockNote file. Until then, the temporary file is the only copy of your latest save: do not delete it by hand.\n\nThe folder also holds: {hash}.base (companion of each temporary file), conflicts\\ (versions set aside by a conflict, never deleted), swap-error.log (log of the copies), import.log and auto-backups\\. To reach it: Open folder, or type %LOCALAPPDATA%\\LockNote in the Explorer address bar.");
    m.insert("tempfiles.dlg.folder", "Folder: {dir}");
    m.insert("tempfiles.dlg.open_folder", "Open folder");
    m.insert("tempfiles.dlg.copy_path", "Copy path");
    m.insert("tempfiles.dlg.list", "Waiting saves (.tmp):");
    m.insert("tempfiles.dlg.delete", "Delete orphan...");
    m.insert("tempfiles.dlg.apply", "This LockNote file:");
    m.insert("tempfiles.dlg.close_app", "Close LockNote now");
    m.insert("tempfiles.dlg.swap_log", "Swap log");
    m.insert("tempfiles.dlg.import_log", "Import log");
    m.insert("tempfiles.dlg.close", "Close");
    m.insert("tempfiles.dlg.empty", "No temporary file: every save has been written.");
    m.insert("tempfiles.dlg.list_failed", "Cannot read the folder: {msg}");
    m.insert("tempfiles.dlg.folder_absent", "Nothing has been written there yet: the folder does not exist.");
    m.insert("tempfiles.dlg.folder_failed", "Cannot open the folder: {msg}");
    m.insert("tempfiles.dlg.copy_failed", "Cannot copy the path: {msg}");
    m.insert("tempfiles.dlg.exe_unknown", "unknown");
    m.insert("tempfiles.dlg.details", "File: {tmp}\nLockNote file: {exe}\n{size}, modified {date} (UTC)\n\n{meaning}");
    m.insert("tempfiles.apply.pending", "A save of {size} is waiting in {tmp}. It will be written into {exe} when LockNote closes.");
    m.insert("tempfiles.apply.nothing", "Nothing waiting: {exe} holds the latest save.");
    m.insert("tempfiles.apply.undecided", "A file is waiting in {tmp}, but closing LockNote will not write it into {exe} ({reason}): the next start decides.");
    m.insert("tempfiles.apply.reason.conflict", "the LockNote file was changed elsewhere");
    m.insert("tempfiles.apply.reason.tmp_unreadable", "it cannot be read");
    m.insert("tempfiles.apply.reason.exe_unreadable", "the LockNote file cannot be read");
    m.insert("tempfiles.apply.reason.ignored", "it holds no usable save");
    m.insert("tempfiles.log.empty", "No event recorded yet ({file}).");
    m.insert("tempfiles.log.failed", "Cannot open {file}: {msg}");
    m.insert("tempfiles.delete.title", "Delete an orphan temporary file");
    m.insert("tempfiles.delete.confirm", "Delete {tmp}?\n\nIt holds the last save made with {exe}, which no longer exists. The notes it holds will be lost.");
    m.insert("tempfiles.delete.failed", "Not deleted: {msg}");

    // Edit menu
    m.insert("menu.edit", "&Edit");
    m.insert("menu.edit.cut", "Cu&t\tCtrl+X");
    m.insert("menu.edit.copy", "&Copy\tCtrl+C");
    m.insert("menu.edit.paste", "&Paste\tCtrl+V");
    m.insert("menu.edit.pasteplain", "Paste as &Plain Text\tCtrl+Shift+V");
    m.insert("menu.edit.find", "&Find...\tCtrl+F");
    m.insert("menu.edit.replace", "&Replace...\tCtrl+H");
    m.insert("menu.edit.searchall", "Search All Ta&bs...\tCtrl+Shift+F");
    m.insert("searchall.title", "Search All Tabs");
    m.insert("searchall.query", "Search:");
    m.insert("searchall.go", "Go to");
    m.insert("searchall.vault", "Vault: {category} / {name}");
    m.insert("searchall.prompt", "Prompt: {name}");
    m.insert("searchall.more", "More results not shown (limit: {max})");
    m.insert("search.regex", "Regular expression");
    m.insert("search.regex.invalid", "Invalid regular expression");
    m.insert("menu.edit.goto", "&Go to Line...\tCtrl+G");
    m.insert("menu.edit.dupline", "&Duplicate Line\tCtrl+D");
    m.insert("menu.edit.delline", "De&lete Line\tCtrl+Shift+K");
    m.insert("menu.edit.case", "Convert Ca&se");
    m.insert("menu.edit.case.upper", "&UPPERCASE\tCtrl+Shift+U");
    m.insert("menu.edit.case.lower", "&lowercase\tCtrl+U");
    m.insert("menu.edit.case.title", "&Title Case");
    m.insert("case.error.title", "Convert Case");
    m.insert("case.error.mapping", "The case was not converted: {msg}");
    m.insert("menu.edit.lines", "Li&nes");
    m.insert("menu.edit.lines.sortasc", "Sort Lines &Ascending");
    m.insert("menu.edit.lines.sortdesc", "Sort Lines &Descending");
    m.insert("menu.edit.lines.dedup", "Remove D&uplicate Lines");
    m.insert("menu.edit.lines.join", "&Join Lines");
    m.insert("menu.edit.lines.trim", "&Trim Trailing Whitespace");
    m.insert("lines.error.title", "Lines");
    m.insert("lines.error.sort", "The lines were not sorted: {msg}");
    m.insert("menu.edit.selectall", "Select &All\tCtrl+A");
    m.insert("menu.edit.timestamp", "Insert &Timestamp\tF5");

    // Help menu
    m.insert("menu.help", "&Help");
    m.insert("menu.help.update", "Check for &Updates...");
    m.insert("menu.help.about", "&About LockNote...");

    // Context menu
    m.insert("ctx.cut", "Cut");
    m.insert("ctx.copy", "Copy");
    m.insert("ctx.paste", "Paste");
    m.insert("ctx.pasteplain", "Paste as Plain Text");
    m.insert("ctx.selectall", "Select All");
    m.insert("ctx.rename", "Rename Tab...");
    m.insert("ctx.close", "Close Tab");

    // Colors
    m.insert("ctx.color", "Tab Color");
    m.insert("color.none", "None");
    m.insert("color.blue", "Blue");
    m.insert("color.green", "Green");
    m.insert("color.red", "Red");
    m.insert("color.orange", "Orange");
    m.insert("color.purple", "Purple");
    m.insert("color.yellow", "Yellow");
    m.insert("color.pink", "Pink");
    m.insert("color.gray", "Gray");
    m.insert("option.off", "Off");
    m.insert("option.seconds", "{n} sec");
    m.insert("option.minutes", "{n} min");

    // Status bar
    m.insert("status.stats", "{words} words | {chars} chars | {lines} lines");
    m.insert("status.saved", "Saved");
    m.insert("status.modified", "Modified");
    m.insert("status.encrypted", "Encrypted");

    // Create password
    m.insert("createpwd.title", "Create Password");
    m.insert("createpwd.password", "Password:");
    m.insert("createpwd.confirm", "Confirm:");
    m.insert("createpwd.create", "Create");
    m.insert("createpwd.cancel", "Cancel");
    m.insert("createpwd.empty", "Password cannot be empty.");
    m.insert("createpwd.tooshort", "Password is too short.");
    m.insert("createpwd.mismatch", "Passwords do not match.");
    m.insert("createpwd.weak", "Weak");
    m.insert("createpwd.fair", "Fair");
    m.insert("createpwd.strong", "Strong");
    m.insert("createpwd.verystrong", "Very strong");
    m.insert("createpwd.welcome", "Welcome to LockNote");
    m.insert("createpwd.minlen", "Password (min. {} characters):");
    m.insert("createpwd.desc", "Your notes are encrypted and stored inside this executable.\nChoose a password to protect them.");

    // Unlock
    m.insert("unlock.title", "Unlock");
    m.insert("unlock.password", "Password:");
    m.insert("unlock.btn", "Unlock");
    m.insert("unlock.cancel", "Cancel");
    m.insert("unlock.empty", "Please enter a password.");
    m.insert("unlock.decrypting", "Decrypting...");
    m.insert("unlock.maxattempts", "Maximum attempts reached. Exiting.");
    m.insert("unlock.wrong", "Incorrect password.");
    m.insert("unlock.desc", "Enter your password to unlock your notes.");
    m.insert("unlock.locked.title", "Locked Out");
    m.insert("unlock.retryin", "Retry in {seconds}s\u{2026}");
    m.insert("unlock.cryptoerror", "Crypto error: {msg}");
    m.insert("lock.overlay.title", "LockNote is locked");
    m.insert("lock.curtain", "Enter your password to show your notes.");
    m.insert("lock.curtain.title", "Notes hidden");
    m.insert("lock.curtain.waiting_for", "LockNote locks once \"{window}\" has closed. Enter your password to show your notes.");
    m.insert("lock.curtain.save_failed", "The save before the lock failed: nothing was lost, your changes are kept. Enter your password to show them.");
    m.insert("lock.curtain.show", "Show notes");
    m.insert("lock.curtain.quit_refused", "LockNote cannot close now: your unsaved changes are behind this screen. Enter your password to show them, then save or close.");
    m.insert("lock.save_failed", "LockNote could not save before locking:\n{error}\n\nYour notes were not locked and your unsaved changes are kept. They are still not saved: try again (Ctrl+S) or copy your text.");
    m.insert("lock.overlay.click", "Click to unlock");
    m.insert("unlock.show", "Show password");
    m.insert("unlock.hide", "Hide password");
    m.insert("unlock.capslock", "Caps Lock is on");

    // Close confirm
    m.insert("close.unsaved", "You have unsaved changes.");
    m.insert("close.remember", "Remember my choice");
    m.insert("close.save", "Save");
    m.insert("close.dontsave", "Don't Save");
    m.insert("close.cancel", "Cancel");

    // Search
    m.insert("search.find", "Find:");
    m.insert("search.replace", "Replace:");
    m.insert("search.next", "Next");
    m.insert("search.btn.replace", "Replace");
    m.insert("search.btn.replaceall", "Replace All");
    m.insert("search.close", "Close");
    m.insert("search.notfound", "Not found");
    m.insert("search.title", "Find & Replace");
    m.insert("search.find.title", "Find");
    m.insert("search.prev", "Previous");
    m.insert("search.case", "Match case");
    m.insert("search.wholeword", "Whole word");
    m.insert("search.count", "{current} / {total}");
    m.insert("search.count.total", "{total} matches");
    m.insert("search.count.none", "No matches");
    m.insert("search.replaceall.title", "Replace All");
    m.insert("search.replaceall.msg", "{count} occurrences replaced.");

    // Go to line
    m.insert("goto.title", "Go to Line");
    m.insert("goto.label", "Line number:");
    m.insert("goto.ok", "OK");
    m.insert("goto.cancel", "Cancel");
    m.insert("goto.invalid", "Invalid line number.");

    // Rename
    m.insert("rename.title", "Rename Tab");
    m.insert("rename.label", "Name:");
    m.insert("rename.ok", "OK");
    m.insert("rename.cancel", "Cancel");
    m.insert("rename.empty", "Name cannot be empty.");

    // Settings
    m.insert("settings.title", "Settings");
    m.insert("settings.onclose", "On close:");
    m.insert("settings.ask", "Ask");
    m.insert("settings.always", "Always save");
    m.insert("settings.never", "Never save");
    m.insert("settings.theme", "Theme:");
    m.insert("settings.dark", "Dark");
    m.insert("settings.light", "Light");
    m.insert("settings.language", "Language:");
    m.insert("settings.minpwdlen", "Min. password length:");
    m.insert("settings.section.appearance", "Appearance");
    m.insert("settings.section.security", "Security");
    m.insert("settings.section.behavior", "Behavior");
    m.insert("settings.font", "Editor font:");
    m.insert("settings.ok", "OK");
    m.insert("settings.cancel", "Cancel");
    m.insert("settings.savekey", "Save shortcut:");
    m.insert("settings.fontsize", "Font size:");
    m.insert("settings.pbkdf2", "PBKDF2 iterations:");
    m.insert("settings.hint", "Password hint:");
    m.insert("settings.mintray", "Minimize to tray");
    m.insert("settings.autolock", "Auto-lock:");
    m.insert("settings.autosave", "Auto-save:");
    m.insert("settings.clipboard", "Clipboard clear:");
    m.insert("settings.hotkey", "Global hotkey:");
    m.insert("settings.restart", "Please restart LockNote to apply the language change.");
    m.insert("update.restart.failed", "The update could not be applied at this exit, so LockNote was not restarted.\n\nStart LockNote again.\n\nDetails: {log}");
    m.insert("settings.2fa", "Two-Factor Auth");
    m.insert("settings.2fa.enable", "Enable TOTP two-factor authentication");
    m.insert("settings.2fa.setup", "Setup 2FA...");
    m.insert("settings.2fa.status.enabled", "2FA is enabled");
    m.insert("settings.2fa.status.disabled", "2FA is disabled");

    // Security dialog
    m.insert("security.title", "Security");
    m.insert("security.changepw", "Change Password...");
    m.insert("security.section", "Security");

    // Preferences dialog
    m.insert("preferences.title", "Preferences");

    // Update
    m.insert("update.fail", "Failed to check for updates.");
    m.insert("update.norelease", "No release information available.");
    m.insert("update.uptodate", "You are up to date!");
    m.insert("update.available", "A new version is available: {version}");
    m.insert("update.success", "Update downloaded successfully.");
    m.insert("update.error", "Update failed: {error}");

    // TOTP
    m.insert("totp.empty", "No TOTP keys configured.");
    m.insert("totp.add", "Add Key");
    m.insert("totp.add.title", "Add TOTP Key");
    m.insert("totp.add.name", "Name:");
    m.insert("totp.add.secret", "Secret:");
    m.insert("totp.add.invalid", "Invalid TOTP secret.");
    m.insert("totp.add.secret_hint", "Base32 or otpauth:// link");
    m.insert("totp.add.generate", "Generate");
    m.insert("totp.secret.err.length", "This secret has a length Base32 never gives: part of it is missing or it was pasted twice. Paste it again, alone.");
    m.insert("totp.secret.err.short", "This secret is too short (under 80 bits): part of it is probably missing.");
    m.insert("totp.secret.err.doubled", "The same text appears twice: it was probably pasted twice. Clear the field and paste the secret once.");
    m.insert("totp.secret.err.duplicate", "This link gives the same parameter twice (for example two secrets): it is refused, as the right one cannot be known.");
    m.insert("totp.secret.err.empty", "Enter the secret given by the site.");
    m.insert("totp.secret.err.base32", "This secret is not valid Base32 (letters A-Z and digits 2-7).");
    m.insert("totp.secret.err.not_totp", "Only otpauth://totp/ links are supported.");
    m.insert("totp.secret.err.no_secret", "This otpauth:// link has no secret.");
    m.insert("totp.secret.err.unsupported", "This link asks for codes LockNote does not make (only SHA1, 6 digits, 30 seconds).");
    m.insert("totp.remove", "Remove");
    m.insert("totp.remove.confirm", "Remove this TOTP key?");
    m.insert("totp.copied", "Code copied to clipboard.");
    m.insert("clipboard.failed", "Copy failed: {err}");
    m.insert("totp.secret.copied", "Secret copied.");
    m.insert("totp.select_first", "Select an entry first.");
    m.insert("totp.added", "Entry added.");
    m.insert("totp.removed", "Entry removed.");
    m.insert("totp.remove.title", "Remove TOTP Entry");
    m.insert("totp.remove.msg", "Remove \"{name}\"? This cannot be undone.");
    m.insert("totp.add.prompt", "Entry name:");

    // TOTP Manager
    m.insert("totp.manager.copy", "Copy Code");

    // 2FA / TOTP Verify
    m.insert("2fa.title", "LockNote \u{2014} Two-Factor Authentication");
    m.insert("2fa.header", "Two-factor authentication");
    m.insert("2fa.prompt", "Enter the 6-digit code from your authenticator app.");
    m.insert("2fa.verify", "Verify");
    m.insert("2fa.cancel", "Cancel");
    m.insert("2fa.enter6", "Please enter a 6-digit code.");
    m.insert("2fa.maxattempts", "Maximum attempts reached.");
    m.insert("2fa.failed", "2FA Failed");
    m.insert("2fa.setup.title", "TOTP Setup");
    m.insert("2fa.setup.secret", "Base32 Secret:");
    m.insert("2fa.setup.copy", "Copy");
    m.insert("2fa.setup.uri", "OtpAuth URI:");
    m.insert("2fa.setup.verify", "Enter a code from your authenticator to verify:");
    m.insert("2fa.setup.confirm", "Confirm");
    m.insert("2fa.setup.verified", "Verified!");
    m.insert("2fa.setup.invalid", "Invalid code, try again");
    m.insert("2fa.pwchanged", "Password changed \u{2713}");
    m.insert("2fa.retryin", "Retry in {seconds}s\u{2026}");

    // Vault / Secret edit
    m.insert("vault.edit", "Edit Secret");
    m.insert("vault.new", "New Secret");
    m.insert("vault.name", "Name:");
    m.insert("vault.category", "Category:");
    m.insert("vault.value", "Value:");
    m.insert("vault.show", "Show");
    m.insert("vault.name.empty", "Name cannot be empty");
    m.insert("vault.btn.add", "Add");
    m.insert("vault.btn.edit", "Edit");
    m.insert("vault.btn.del", "Del");
    m.insert("vault.btn.copy", "Copy");
    m.insert("vault.btn.copied", "Copied!");
    m.insert("vault.copied.named", "Copied: {name}");
    // SSH keys (issue 054)
    m.insert("menu.tools.ssh", "SSH &Keys");
    m.insert("menu.tools.ssh_generate", "&Generate SSH Key...");
    m.insert("menu.tools.ssh_agent", "&Load into ssh-agent...");
    // Password generator
    m.insert("menu.tools.passgen", "&Password Generator...");
    m.insert("vault.generate", "Generate...");
    m.insert("passgen.title", "Password Generator");
    m.insert("passgen.password", "Password:");
    m.insert("passgen.show", "Show");
    m.insert("passgen.hide", "Hide");
    m.insert("passgen.generate", "Generate");
    m.insert("passgen.length", "Length ({min}-{max}):");
    m.insert("passgen.upper", "Uppercase (A-Z)");
    m.insert("passgen.lower", "Lowercase (a-z)");
    m.insert("passgen.digits", "Digits (0-9)");
    m.insert("passgen.symbols", "Symbols");
    m.insert("passgen.symbol_set", "Allowed symbols:");
    m.insert("passgen.exclude_ambiguous", "Exclude look-alike characters ({chars})");
    m.insert("passgen.use", "Use");
    m.insert("passgen.close", "Close");
    m.insert("passgen.err.length", "The length must be between {min} and {max}.");
    m.insert("passgen.err.no_class", "Select at least one type of character.");
    m.insert("passgen.err.not_a_symbol", "\"{char}\" is not a symbol.");
    m.insert("passgen.err.no_symbol", "No symbol is left: add symbols or untick Symbols.");
    m.insert("ssh.title", "SSH Keys");
    m.insert("ssh.gen.title", "Generate SSH Key");
    m.insert("ssh.gen.type", "Type:");
    m.insert("ssh.gen.recommended", "recommended");
    m.insert("ssh.gen.name", "Name in the vault:");
    m.insert("ssh.gen.comment", "Comment:");
    m.insert("ssh.gen.passphrase", "Passphrase (optional):");
    m.insert("ssh.gen.confirm", "Confirm passphrase:");
    m.insert("ssh.gen.generate", "&Generate");
    m.insert("ssh.gen.generating", "Generating...");
    m.insert("ssh.gen.fingerprint", "Fingerprint:");
    m.insert("ssh.gen.public", "Public key:");
    m.insert("ssh.gen.copy_public", "Copy public key");
    m.insert("ssh.gen.copied", "Public key copied.");
    m.insert("ssh.gen.save_vault", "Save to vault");
    m.insert("ssh.gen.export", "Export...");
    m.insert("ssh.gen.export_title", "Export SSH Key");
    m.insert("ssh.gen.close", "Close");
    m.insert("ssh.gen.mismatch", "Passphrases do not match.");
    m.insert("ssh.gen.no_passphrase_export", "This private key has no passphrase: anyone who can read the file can use it. Export anyway?");
    m.insert("ssh.gen.exported", "Exported to {path} (+ .pub).");
    m.insert("ssh.agent.title", "Load into ssh-agent");
    m.insert("ssh.agent.key", "Key:");
    m.insert("ssh.agent.passphrase", "Passphrase (if the key has one):");
    m.insert("ssh.agent.warning", "The key will be handed to the Windows SSH agent, which may keep it outside LockNote. It is removed when LockNote locks or closes. After an abnormal shutdown, it is removed the next time this file is unlocked. The same key added by hand with ssh-add is removed too. Loading the key also saves this file.");
    m.insert("ssh.agent.load", "&Load");
    m.insert("ssh.agent.none", "No SSH private key in the vault. Create one with Tools > SSH Keys > Generate SSH Key.");
    m.insert("ssh.agent.loaded", "Key loaded into ssh-agent:\n{fingerprint}");
    m.insert("ssh.agent.save_failed", "The key was loaded, but this file could not be saved, so the key was removed from ssh-agent again.\n{detail}");
    m.insert("ssh.agent.unload_failed", "Some SSH keys could not be removed from ssh-agent. They stay listed and LockNote will try again at the next unlock.\n{detail}");
    m.insert("ssh.err.invalid_comment", "The comment must fit on one line, without tabs or control characters.");
    m.insert("ssh.err.duplicate", "A secret named \"{name}\" already exists in the vault.");
    m.insert("ssh.err.passphrase_required", "This key is protected: enter its passphrase.");
    m.insert("ssh.err.wrong_passphrase", "Wrong passphrase.");
    m.insert("ssh.err.agent_unavailable", "The Windows ssh-agent service is not running.\nIn an administrator PowerShell:\nGet-Service ssh-agent | Set-Service -StartupType Automatic\nStart-Service ssh-agent");
    m.insert("ssh.err.agent_failure", "ssh-agent refused the operation.");
    m.insert("ssh.err.agent_timeout", "ssh-agent did not answer in time.");
    m.insert("ssh.err.other", "Error: {detail}");
    m.insert("ssh.agent.save_failed_still_loaded", "This file could not be saved and the key is still in ssh-agent. LockNote will try to remove it when this file is locked or closed.\n{detail}");
    m.insert("ssh.err.invalid_name", "The name must not contain [ ] or control characters.");
    m.insert("vault.copy.failed", "Copy failed");
    m.insert("vault.btn.reveal", "Reveal");
    m.insert("vault.tree.error", "Vault could not be displayed");
    m.insert("vault.search", "Search a name");
    m.insert("vault.category.none", "Choose a category");
    m.insert("vault.cat.menu.new", "New category\u{2026}");
    m.insert("vault.cat.menu.rename", "Rename\u{2026}");
    m.insert("vault.cat.menu.delete", "Delete");
    m.insert("vault.cat.new.title", "New category");
    m.insert("vault.cat.rename.title", "Rename category");
    m.insert("vault.cat.name", "Category name:");
    m.insert("vault.cat.delete.title", "Delete category");
    m.insert("vault.cat.delete.confirm", "Delete the category \u{201C}{name}\u{201D}? Its {count} secret(s) will move to General.");
    m.insert("vault.cat.error.title", "Category");
    m.insert("vault.cat.err.empty", "The name cannot be empty.");
    m.insert("vault.cat.err.toolong", "The name is too long ({max} characters max).");
    m.insert("vault.cat.err.char", "The character \u{201C}{char}\u{201D} is not allowed.");
    m.insert("vault.cat.err.duplicate", "A category named \u{201C}{name}\u{201D} already exists.");
    m.insert("vault.cat.err.toomany", "Too many categories ({max} max).");
    m.insert("vault.cat.err.general", "The General category cannot be renamed or deleted.");
    m.insert("vault.cat.err.notfound", "Category not found: {name}");
    m.insert("vault.cat.err.compare", "Name comparison failed: {msg}");

    // Menu bar toggle buttons
    m.insert("menu.toggle.vault", "Vault");
    m.insert("menu.toggle.totp", "TOTP");
    m.insert("menu.toggle.attach", "Attach");
    m.insert("menu.toggle.lock", "\u{1F512} Lock");

    // Attachments
    m.insert("menu.tools.attachments", "&Attachments\tCtrl+J");
    // Prompts panel (issue 058)
    m.insert("menu.toggle.prompts", "Prompts");
    m.insert("menu.tools.prompts", "P&rompts");
    m.insert("palette.prompt", "Prompt: {name}");
    m.insert("prompts.btn.insert", "Insert");
    m.insert("prompts.inserted.named", "Inserted: {name}");
    m.insert("palette.title", "Command Palette");
    m.insert("palette.tab", "Tab: {name}");
    m.insert("palette.vault", "Vault: {name}");
    m.insert("prompts.search", "Search name or text");
    m.insert("prompts.btn.add", "Add");
    m.insert("prompts.btn.edit", "Edit");
    m.insert("prompts.btn.del", "Del");
    m.insert("prompts.btn.copy", "Copy");
    m.insert("prompts.copied.named", "Copied: {name}");
    m.insert("prompts.copy.failed", "Copy failed: {err}");
    m.insert("prompts.count", "{count} prompt(s)");
    m.insert("prompts.count.filtered", "{shown} of {total} prompt(s)");
    m.insert("prompts.list.error", "The list cannot be shown: {msg}");
    m.insert("prompts.new", "New Prompt");
    m.insert("prompts.edit", "Edit Prompt");
    m.insert("prompts.name", "Name:");
    m.insert("prompts.category", "Category:");
    m.insert("prompts.text", "Text:");
    m.insert("prompts.category.default", "General");
    m.insert("prompts.err.name", "Name cannot be empty");
    m.insert("prompts.err.category", "Category cannot be empty");
    m.insert("prompts.err.text", "Text cannot be empty");
    m.insert("prompts.err.toolong", "Text is too long (max {max} MB)");
    m.insert("prompts.err.name.char", "The name cannot contain {char}");
    m.insert("prompts.err.category.char", "The category cannot contain {char}");
    m.insert("prompts.delete.title", "Delete Prompt");
    m.insert("prompts.delete.confirm", "Delete the prompt \"{name}\"?");
    m.insert("prompts.gone", "This prompt was changed or removed meanwhile (import?): nothing was saved.");
    m.insert("vault.edit.gone", "This secret was changed or removed meanwhile (import?): nothing was saved.");
    m.insert("attach.btn.add", "Add");
    m.insert("attach.btn.open", "Open");
    m.insert("attach.btn.save", "Save");
    m.insert("attach.btn.del", "Del");
    m.insert("attach.error.read", "Failed to read file: {}");
    m.insert("attach.error.corrupt", "Attachment data is corrupted (SHA-256 mismatch).");
    m.insert("attach.error.write_temp", "Failed to write temp file: {}");
    m.insert("attach.error.save", "Failed to save file: {}");
    m.insert("attach.risky.title", "Open attachment?");
    m.insert("attach.risky.body", "This attachment has an executable file type. Opening it will run code on your computer. Only continue if you trust the source of this vault.\n\nOpen anyway?");
    m.insert("attach.confirm.delete", "Delete attachment \"{}\"?");
    m.insert("attach.confirm.title", "Confirm");
    m.insert("attach.save.title", "Save \"{}\" as");
    m.insert("attach.add.title", "Attach file");

    // Steganography
    m.insert("menu.tools.stego", "&Steganography");
    m.insert("menu.tools.stego_hide", "&Hide in Image...");
    m.insert("menu.tools.stego_extract", "E&xtract from Image...");
    m.insert("stego.confirm_action", "steganography export");
    m.insert("stego.dlg.open_carrier", "Select Carrier Image");
    m.insert("stego.dlg.save", "Save Steganographic Image");
    m.insert("stego.dlg.open_extract", "Select Image to Extract From");
    m.insert("stego.title", "Steganography");
    m.insert("stego.success", "Content hidden successfully.");
    m.insert("stego.size", "Size");
    m.insert("stego.warning_recompress", "Do not share via WhatsApp, Instagram, etc.\nImage recompression destroys hidden data.");
    m.insert("stego.extracted", "Hidden notes extracted successfully.");
    m.insert("stego.nodata_title", "No Hidden Data");
    m.insert("stego.nodata", "No hidden data detected in this image.");
    m.insert("stego.error", "Error");
    m.insert("stego.error.read_image", "Failed to read image");
    m.insert("stego.error.format_title", "Unsupported Format");
    m.insert("stego.error.format", "Not a supported image (JPEG, PNG, BMP).");
    m.insert("stego.error.write", "Failed to write output file");
    m.insert("stego.error.toolarge_title", "Payload Too Large");
    m.insert("stego.error.toolarge", "Notes too large for this image.");
    m.insert("stego.error.toolarge_hint", "Choose a larger carrier image.");
    m.insert("stego.error.embed", "Failed to embed data");
    m.insert("stego.error.encrypt", "Encryption failed during image export: {msg}");
    m.insert("stego.error.decrypt_title", "Extraction Failed");
    m.insert("stego.error.decrypt", "Wrong password or not LockNote data.");
    m.insert("stego.error.extract", "Failed to extract data");
    m.insert("stego.pw.title_hide", "Choose Image Password");
    m.insert("stego.pw.title_extract", "Image Password");
    m.insert("stego.pw.prompt_hide", "Password to protect this image:");
    m.insert("stego.pw.prompt_extract", "Password used to hide the data:");
    m.insert("stego.pw.confirm", "Confirm password:");
    m.insert("stego.pw.empty", "Password cannot be empty.");
    m.insert("stego.pw.mismatch", "Passwords do not match.");

    // History


    // Export
    m.insert("export.prompt", "Export note as plain text?");
    m.insert("export.btn", "Export");
    m.insert("export.warning", "The exported file will NOT be encrypted.");
    m.insert("export.dlg.title", "Export Note");
    m.insert("export.dlg.filter", "Text Files (*.txt)|*.txt|All Files (*.*)|*.*");
    m.insert("export.fail", "Failed to export the file.");

    // Export encoding
    m.insert("export.encoding.title", "Export Encoding");
    m.insert("export.encoding.label", "Choose encoding for export:");

    // Drag & drop
    m.insert("drop.binary", "This file type is not supported. LockNote can only import text files.");
    m.insert("drop.invalid", "This file could not be read as text. It may be a binary file.");
    m.insert("drop.choice.title", "Dropped file");
    m.insert("drop.choice.text", "How do you want to handle this text file?");
    m.insert("drop.choice.binary", "This file can be added as an attachment.");
    m.insert("drop.choice.insert", "Insert text");
    m.insert("drop.choice.attach", "Add attachment");
    m.insert("drop.choice.cancel", "Cancel");
    m.insert("drop.directory", "Folders cannot be dropped into LockNote. Please drop individual files.");

    // Save/error
    m.insert("save.error.title", "Save Error");
    m.insert("save.error.msg", "Failed to save: {}");
    m.insert("export.success.title", "Export");
    m.insert("export.success.msg", "Note exported to:\n{}");
    m.insert("export.error.title", "Export Error");
    m.insert("export.error.msg", "Failed to export: {}");
    m.insert("export.warning_title", "Export Warning");
    m.insert("export.warning_lossy", "Some characters cannot be represented in this encoding and were replaced. Continue?");
    m.insert("menu.file.backup_export", "Export full backup...");
    m.insert("menu.file.backup_import", "Import full backup...");
    m.insert("backup.export.save_title", "Save full backup");
    m.insert("backup.filter", "LockNote backup (*.lnbak)|All Files (*.*)");
    m.insert("backup.export.done", "Full backup saved to:\n{path}\n\nIt holds all tabs, snapshots, vault, attachments and settings, encrypted with this note's password.");
    m.insert("backup.export.overwrite", "{path}\n\nThis file already exists. Replace it?");
    m.insert("backup.export.error", "Backup failed: {msg}");
    m.insert("backup.import.open_title", "Import full backup");
    m.insert("backup.import.pw_title", "Backup password");
    m.insert("backup.import.pw_label", "Password of this backup:");
    m.insert("backup.import.wrong_password", "Wrong password, or the file is corrupted.");
    m.insert("backup.import.not_backup", "This file is not a LockNote backup.");
    m.insert("backup.import.newer", "This backup comes from a newer LockNote (format {found}, this version reads up to {supported}). Please update LockNote.");
    m.insert("backup.import.corrupt", "This backup is damaged: {msg}");
    m.insert("backup.import.read_error", "Cannot read the file: {msg}");
    m.insert("backup.import.too_large", "This file is too large to be a backup.");
    m.insert("backup.import.confirm", "Replace ALL current content with this backup?\n\nBackup created {date}:\n- {tabs} tab(s)\n- {prompts} prompt(s)\n- {secrets} vault secret(s)\n- {attachments} attachment(s)\n\nYour current tabs, prompts, vault, attachments and settings (including two-factor authentication) will be replaced. This cannot be undone once the note is saved.");
    m.insert("backup.import.done", "Backup imported. Save to keep it. The language setting applies after saving and restarting LockNote.");
    m.insert("autobackup.failed.import", "A safety backup could not be written, so nothing was imported.\n\n{msg}");
    m.insert("autobackup.failed.update", "A safety backup could not be written, so the update was not downloaded.\n\n{msg}");
    m.insert("autobackup.purge.ask", "{n} automatic backup(s) were made with your previous password and can still be opened with it.\n\nDelete them now?");
    m.insert("autobackup.purge.partial", "{n} automatic backup(s) could not be deleted. Delete them by hand in:\n{dir}");
    m.insert("menu.file.backups", "Safety &backups...");
    m.insert("autobackup.dlg.title", "Safety backups");
    m.insert("autobackup.dlg.list", "Backups (newest first):");
    m.insert("autobackup.dlg.empty", "No safety backup yet.");
    m.insert("autobackup.dlg.details", "{kind}\n{date} (UTC)\n{size}\n\nOpens with the password you had when it was made.");
    m.insert("autobackup.dlg.restore", "Restore");
    m.insert("autobackup.dlg.create", "Create backup now");
    m.insert("totp.nextcode", "Next code in {n} s");
    m.insert("autobackup.dlg.delete", "Delete");
    m.insert("autobackup.dlg.folder", "Open folder");
    m.insert("autobackup.dlg.close", "Close");
    m.insert("autobackup.dlg.delete.title", "Delete backup");
    m.insert("autobackup.dlg.delete.confirm", "Permanently delete this backup?");
    m.insert("autobackup.dlg.delete.failed", "Cannot delete this backup: {msg}");
    m.insert("autobackup.dlg.folder.failed", "Cannot open the folder: {msg}");
    m.insert("autobackup.dlg.list_failed", "Cannot read the backups folder: {msg}");
    m.insert("autobackup.kind.pre_import", "Before import");
    m.insert("autobackup.kind.pre_update", "Before update");
    m.insert("autobackup.kind.manual", "Manual");
    m.insert("autobackup.manual.done", "Backup created.");
    m.insert("autobackup.manual.failed", "The backup could not be written.\n\n{msg}");
    m.insert("menu.file.import_exe", "Import from another LockNote...");
    m.insert("import_exe.title", "Import from another LockNote");
    m.insert("import_exe.open_title", "Choose the LockNote file to import");
    m.insert("import_exe.filter", "LockNote (*.exe)|All Files (*.*)");
    m.insert("import_exe.same_file", "This is the file you are running. Choose your older LockNote file.");
    // Startup conflict between the pending save and a more recent program file (issue 055)
    m.insert("conflict.title", "LockNote - Two versions of your notes");
    m.insert("conflict.body", "LockNote found two different versions of your notes.\n\nPending save, not yet written into the program file:\n{tmp_date} (UTC)\n\nProgram file, holding a different version, last modified:\n{exe_date} (UTC)\n{exe}\n\nYes: keep the pending save.\nNo: keep the notes of the program file.\nCancel: quit without changing anything.\n\nThe version you do not keep is not deleted. It is moved to:\n{dir}");
    m.insert("conflict.exe_empty", "Note: the program file holds no notes at all.");
    m.insert("conflict.failed", "Your choice could not be applied. Nothing was deleted.\n\n{msg}\n\nLockNote will close.");
    m.insert("pending.failed", "LockNote cannot open your notes safely. Nothing was changed.\n\n{msg}\n\nLockNote will close.");
    m.insert("import_exe.no_data", "No LockNote data found in this file.");
    m.insert("import_exe.pw_title", "Password of the imported file");
    m.insert("import_exe.pw_label", "Password of the LockNote file you are importing:");
    m.insert("import_exe.confirm", "Replace ALL current content with the data of this LockNote file?\n\nIt holds:\n- {tabs} tab(s)\n- {prompts} prompt(s)\n- {secrets} vault secret(s)\n- {attachments} attachment(s)\n\nYour current tabs, prompts, vault, attachments and settings (including two-factor authentication) will be replaced. The imported file itself is not modified.");
    m.insert("import_exe.done", "Data imported. Save to keep it: it will then be protected by the password of THIS file. The language setting applies after saving and restarting LockNote.");

    // Tray
    m.insert("tray.restore", "&Restore");
    m.insert("tray.quit", "&Quit");

    // About
    m.insert("about.title", "About LockNote");
    m.insert("about.desc", "Self-contained encrypted notepad for Windows");

    // Texts that were hard-coded in English in src/ui (issue #25)
    m.insert("confirmpwd.title", "Confirm Password");
    m.insert("confirmpwd.export", "Enter your password to export:");
    m.insert("confirmpwd.print", "Enter your password to print:");
    m.insert("about.version", "Version {version}");
    m.insert("totp.setup.for", "Setting up TOTP for: {name}");
    m.insert("2fa.invalid.left", "Invalid code. Attempts left: {n}.");
    m.insert("attach.error.title", "Error");
    m.insert("attach.limit.title", "Attachment");
    m.insert("attach.error.toolarge", "File too large: {size} (max {max})");
    m.insert("attach.error.totaltoolarge", "Total attachments too large: {size} (max {max})");
    m.insert("status.word", "{n} word");
    m.insert("status.words", "{n} words");
    m.insert("status.char", "{n} character");
    m.insert("status.chars", "{n} characters");
    m.insert("status.line", "{n} line");
    m.insert("status.lines", "{n} lines");
    m.insert("status.caret", "Ln {line}, Col {col}");
    m.insert("status.selected.one", "{n} selected");
    m.insert("status.selected.many", "{n} selected");
    m.insert("save.error.encrypt", "Encryption failed: {}");
    m.insert("export.success.encoding", "Note exported as {encoding} to:\n{path}");
    m.insert("filter.text", "Text Files (*.txt)");
    m.insert("filter.all", "All Files (*.*)");
    m.insert("filter.images", "Images (*.jpg;*.jpeg;*.png;*.bmp)");
    m.insert("filter.image", "Image (*.{ext})");
    m.insert("update.available.title", "Update Available");
    m.insert("update.available.ask", "A new version ({version}) is available.\nWould you like to download it?");
    m.insert("update.downloaded.title", "Update Downloaded");
    m.insert("update.downloaded.ask", "Update downloaded successfully.\n\nRestart LockNote now to apply it?");
    m.insert("update.error.title", "Update Error");
    m.insert("update.uptodate.title", "Up to Date");
    m.insert("update.uptodate.msg", "You are running the latest version ({version}).");
    m.insert("update.checkfailed.title", "Update Check Failed");
    m.insert("update.checkfailed.msg", "Could not check for updates: {error}");
    m.insert("vault.delete.title", "Delete Secret");
    m.insert("vault.delete.msg", "Are you sure you want to delete this secret?");
    m.insert("filter.ext", "{EXT} files (*.{ext})");

    // Close confirm
    m.insert("close.title", "Unsaved Changes");
    m.insert("close.msg", "Unsaved changes. Save before closing?");
    m.insert("close.save_failed", "LockNote could not save:\n{error}\n\nYour unsaved changes are still in the window. Keep LockNote open to try again or copy your text; closing without saving loses them.");
    m.insert("close.keep_open", "Keep LockNote open");
    m.insert("close.close_unsaved", "Close without saving");
    m.insert("close.stuck", "LockNote cannot close yet: it waits for \"{window}\" to close.\n\nKeep waiting, or close without saving: the unsaved changes are then lost.");
    m.insert("close.keep_waiting", "Keep waiting");
    m.insert("endsession.blocked", "LockNote could not save your notes ({error}). Cancel, then save or copy your text.");
    m.insert("close.remember_not_saved", "Your choice was not remembered: LockNote keeps its settings inside the note, and the note is not saved. To stop this question, choose \"{onclose} {never}\" in {menu}, then save.");

    m
}

// ---------------------------------------------------------------------------
// French translations
// ---------------------------------------------------------------------------

fn build_fr() -> Dict {
    let mut m = HashMap::new();

    m.insert("app.title", "LockNote");
    m.insert("app.title.modified", "LockNote *");

    m.insert("menu.file", "&Fichier");
    m.insert("menu.file.save", "&Enregistrer\tCtrl+S");
    m.insert("menu.file.changepass", "Changer le mot de &passe...");
    m.insert("menu.file.settings", "&Param\u{00e8}tres...");
    m.insert("menu.file.quit", "&Quitter\tAlt+F4");
    m.insert("menu.file.print", "&Imprimer...\tCtrl+P");
    m.insert("print.footer", "Page {n} / {m}");
    m.insert("print.error.title", "Impression");
    m.insert("print.error.failed", "L'impression a \u{00e9}chou\u{00e9} : {error}");
    m.insert("print.error.too_small", "La zone imprimable de la page est trop petite pour la police de l'\u{00e9}diteur.");
    m.insert("menu.file.export", "E&xporter...");

    m.insert("menu.view", "&Affichage");
    m.insert("menu.view.ontop", "Toujours au &premier plan");
    m.insert("menu.view.wordwrap", "Retour \u{00e0} la &ligne");
    m.insert("menu.view.linenumbers", "&Num\u{00e9}ros de ligne");
    m.insert("settings.font.failed", "La nouvelle police de l'\u{00e9}diteur n'a pas pu \u{00ea}tre appliqu\u{00e9}e ; l'\u{00e9}diteur garde sa police actuelle.\n\nD\u{00e9}tails : {detail}");
    m.insert("menu.view.vault", "Gestionnaire de &coffre...");

    m.insert("menu.tab", "&Onglet");
    m.insert("menu.tab.new", "&Nouvel onglet\tCtrl+T");
    m.insert("menu.tab.close", "&Fermer l'onglet\tCtrl+W");
    m.insert("menu.tab.rename", "&Renommer l'onglet...\tF2");
    m.insert("menu.tab.next", "Onglet suivant\tCtrl+Tab");
    m.insert("menu.tab.prev", "Onglet pr\u{00e9}c\u{00e9}dent\tCtrl+Shift+Tab");
    m.insert("menu.tab.compare", "&Comparer avec le dernier enregistrement...");
    m.insert("compare.title", "Comparer avec le dernier enregistrement");
    m.insert("compare.identical", "Aucune modification depuis le dernier enregistrement.");
    m.insert("compare.too_large", "Cet onglet est trop long pour \u{00ea}tre compar\u{00e9} (plus de {} lignes d\u{2019}un c\u{00f4}t\u{00e9}).");
    m.insert("compare.failed", "La comparaison n\u{2019}a pas pu \u{00ea}tre calcul\u{00e9}e.");
    m.insert("compare.close", "Fermer");
    m.insert("menu.edit.goto_heading", "Aller au t&itre...\tCtrl+Shift+O");
    m.insert("heading.title", "Aller au titre");
    m.insert("heading.filter", "Filtre :");
    m.insert("heading.none", "Aucun titre dans cet onglet.");
    m.insert("heading.no_match", "Aucun titre ne correspond au filtre.");
    m.insert("tab.limit.title", "Limite d\u{2019}onglets");
    m.insert("tab.limit.msg", "Nombre maximum d\u{2019}onglets atteint (20).");
    m.insert("tab.close.title", "Fermer l\u{2019}onglet");
    m.insert("tab.close.msg", "Fermer l\u{2019}onglet \u{00ab} {name} \u{00bb} ?\nSon contenu sera perdu.");
    m.insert("tab.ctx.rename", "Renommer l\u{2019}onglet...");
    m.insert("tab.ctx.color", "Couleur");
    m.insert("tab.ctx.no_color", "Aucune couleur");
    m.insert("tab.ctx.close", "Fermer l\u{2019}onglet");

    // Tools menu
    m.insert("menu.tools", "O&utils");
    m.insert("menu.tools.changepass", "Changer le mot de &passe...");
    m.insert("menu.tools.totp", "Gestionnaire &TOTP");

    m.insert("menu.settings.preferences", "&Pr\u{00e9}f\u{00e9}rences...");
    m.insert("menu.settings.security", "&S\u{00e9}curit\u{00e9}...");
    m.insert("menu.settings.tempfiles", "Fichiers &temporaires...");

    m.insert("status.path_copied", "Chemin copi\u{00e9}");
    m.insert("status.path_copy_failed", "Impossible de copier le chemin");
    m.insert("tempfiles.owner.current", "Ce fichier LockNote");
    m.insert("tempfiles.owner.present", "Un autre fichier LockNote");
    m.insert("tempfiles.owner.orphan", "Orphelin : son fichier LockNote n'existe plus");
    m.insert("tempfiles.owner.folder_missing", "Son dossier est absent (lecteur d\u{00e9}connect\u{00e9} ?)");
    m.insert("tempfiles.owner.unreachable", "Son fichier LockNote ne peut pas \u{00ea}tre v\u{00e9}rifi\u{00e9}");
    m.insert("tempfiles.owner.no_companion", "Laiss\u{00e9} par une ancienne version");
    m.insert("tempfiles.owner.companion_unusable", "Fichier compagnon inutilisable");
    m.insert("tempfiles.meaning.current", "La sauvegarde en attente du fichier LockNote ouvert. Elle y est \u{00e9}crite \u{00e0} la fermeture de LockNote.");
    m.insert("tempfiles.meaning.present", "Une sauvegarde qui attend ce fichier LockNote : elle y est \u{00e9}crite la prochaine fois que ce fichier est ouvert puis ferm\u{00e9}. Conserv\u{00e9}.");
    m.insert("tempfiles.meaning.orphan", "Ce fichier LockNote n'existe plus, mais son dossier existe. Ce fichier contient la derni\u{00e8}re sauvegarde faite avec lui : le supprimer fait perdre ces notes.");
    m.insert("tempfiles.meaning.folder_missing", "Ni ce fichier LockNote ni son dossier ne sont trouv\u{00e9}s : lecteur ou partage r\u{00e9}seau d\u{00e9}connect\u{00e9}, ou dossier supprim\u{00e9}. Conserv\u{00e9} : reconnectez le lecteur et ouvrez ce fichier.");
    m.insert("tempfiles.meaning.unreachable", "L'existence de ce fichier LockNote ne peut pas \u{00ea}tre v\u{00e9}rifi\u{00e9}e ({error}). Conserv\u{00e9}.");
    m.insert("tempfiles.meaning.no_companion", "Laiss\u{00e9} par LockNote 1.2.23 ou plus ancien, qui n'enregistrait pas le fichier LockNote auquel il appartient. Conserv\u{00e9} : ce fichier le reprend \u{00e0} son ouverture.");
    m.insert("tempfiles.meaning.companion_unusable", "Son fichier compagnon est inutilisable ({reason}). Conserv\u{00e9}.");
    m.insert("tempfiles.owner.other_volume", "Son lecteur d\u{00e9}signe maintenant un autre disque");
    m.insert("tempfiles.owner.tmp_unreadable", "Ne peut pas \u{00ea}tre examin\u{00e9}");
    m.insert("tempfiles.meaning.other_volume", "Ce fichier LockNote est introuvable, et sa lettre de lecteur ou son partage d\u{00e9}signe maintenant un autre disque qu'au moment de la sauvegarde (une cl\u{00e9} USB remplac\u{00e9}e par une autre). Conserv\u{00e9} : remettez le bon disque et ouvrez ce fichier.");
    m.insert("tempfiles.meaning.tmp_unreadable", "Ce fichier ne peut pas \u{00ea}tre examin\u{00e9} ({error}). Conserv\u{00e9}.");
    m.insert("tempfiles.dlg.reading", "Lecture...");
    m.insert("tempfiles.dlg.title", "Fichiers temporaires");
    m.insert("tempfiles.dlg.intro", "LockNote ne peut pas r\u{00e9}\u{00e9}crire son propre fichier pendant qu'il tourne. Chaque sauvegarde est d'abord \u{00e9}crite dans un fichier temporaire du dossier ci-dessous ; \u{00e0} la fermeture de LockNote, un programme d'aide la recopie dans le fichier LockNote. Jusque-l\u{00e0}, le fichier temporaire est la seule copie de votre derni\u{00e8}re sauvegarde : ne le supprimez pas \u{00e0} la main.\n\nLe dossier contient aussi : {hash}.base (compagnon de chaque fichier temporaire), conflicts\\ (versions \u{00e9}cart\u{00e9}es par un conflit, jamais supprim\u{00e9}es), swap-error.log (journal des recopies), import.log et auto-backups\\. Pour y acc\u{00e9}der : Ouvrir le dossier, ou tapez %LOCALAPPDATA%\\LockNote dans la barre d'adresse de l'Explorateur.");
    m.insert("tempfiles.dlg.folder", "Dossier : {dir}");
    m.insert("tempfiles.dlg.open_folder", "Ouvrir le dossier");
    m.insert("tempfiles.dlg.copy_path", "Copier le chemin");
    m.insert("tempfiles.dlg.list", "Sauvegardes en attente (.tmp) :");
    m.insert("tempfiles.dlg.delete", "Supprimer l'orphelin...");
    m.insert("tempfiles.dlg.apply", "Ce fichier LockNote :");
    m.insert("tempfiles.dlg.close_app", "Fermer LockNote maintenant");
    m.insert("tempfiles.dlg.swap_log", "Journal des recopies");
    m.insert("tempfiles.dlg.import_log", "Journal des imports");
    m.insert("tempfiles.dlg.close", "Fermer");
    m.insert("tempfiles.dlg.empty", "Aucun fichier temporaire : toutes les sauvegardes ont \u{00e9}t\u{00e9} \u{00e9}crites.");
    m.insert("tempfiles.dlg.list_failed", "Impossible de lire le dossier : {msg}");
    m.insert("tempfiles.dlg.folder_absent", "Rien n'y a encore \u{00e9}t\u{00e9} \u{00e9}crit : le dossier n'existe pas.");
    m.insert("tempfiles.dlg.folder_failed", "Impossible d'ouvrir le dossier : {msg}");
    m.insert("tempfiles.dlg.copy_failed", "Impossible de copier le chemin : {msg}");
    m.insert("tempfiles.dlg.exe_unknown", "inconnu");
    m.insert("tempfiles.dlg.details", "Fichier : {tmp}\nFichier LockNote : {exe}\n{size}, modifi\u{00e9} le {date} (UTC)\n\n{meaning}");
    m.insert("tempfiles.apply.pending", "Une sauvegarde de {size} attend dans {tmp}. Elle sera \u{00e9}crite dans {exe} \u{00e0} la fermeture de LockNote.");
    m.insert("tempfiles.apply.nothing", "Rien en attente : {exe} contient la derni\u{00e8}re sauvegarde.");
    m.insert("tempfiles.apply.undecided", "Un fichier attend dans {tmp}, mais fermer LockNote ne l'\u{00e9}crira pas dans {exe} ({reason}) : le prochain d\u{00e9}marrage tranchera.");
    m.insert("tempfiles.apply.reason.conflict", "le fichier LockNote a \u{00e9}t\u{00e9} modifi\u{00e9} ailleurs");
    m.insert("tempfiles.apply.reason.tmp_unreadable", "il ne peut pas \u{00ea}tre lu");
    m.insert("tempfiles.apply.reason.exe_unreadable", "le fichier LockNote ne peut pas \u{00ea}tre lu");
    m.insert("tempfiles.apply.reason.ignored", "il ne contient aucune sauvegarde utilisable");
    m.insert("tempfiles.log.empty", "Aucun \u{00e9}v\u{00e9}nement enregistr\u{00e9} ({file}).");
    m.insert("tempfiles.log.failed", "Impossible d'ouvrir {file} : {msg}");
    m.insert("tempfiles.delete.title", "Supprimer un fichier temporaire orphelin");
    m.insert("tempfiles.delete.confirm", "Supprimer {tmp} ?\n\nIl contient la derni\u{00e8}re sauvegarde faite avec {exe}, qui n'existe plus. Les notes qu'il contient seront perdues.");
    m.insert("tempfiles.delete.failed", "Non supprim\u{00e9} : {msg}");

    m.insert("menu.edit", "&\u{00c9}dition");
    m.insert("menu.edit.cut", "Co&uper\tCtrl+X");
    m.insert("menu.edit.copy", "&Copier\tCtrl+C");
    m.insert("menu.edit.paste", "C&oller\tCtrl+V");
    m.insert("menu.edit.pasteplain", "Coller en texte &brut\tCtrl+Shift+V");
    m.insert("menu.edit.find", "&Rechercher...\tCtrl+F");
    m.insert("menu.edit.replace", "Re&mplacer...\tCtrl+H");
    m.insert("menu.edit.searchall", "Rechercher dans tous les o&nglets...\tCtrl+Shift+F");
    m.insert("searchall.title", "Rechercher dans tous les onglets");
    m.insert("searchall.query", "Rechercher :");
    m.insert("searchall.go", "Afficher");
    m.insert("searchall.vault", "Coffre : {category} / {name}");
    m.insert("searchall.prompt", "Prompt : {name}");
    m.insert("searchall.more", "R\u{00e9}sultats suivants non affich\u{00e9}s (limite : {max})");
    m.insert("search.regex", "Expression r\u{00e9}guli\u{00e8}re");
    m.insert("search.regex.invalid", "Expression r\u{00e9}guli\u{00e8}re invalide");
    m.insert("menu.edit.goto", "&Aller \u{00e0} la ligne...\tCtrl+G");
    m.insert("menu.edit.dupline", "&Dupliquer la ligne\tCtrl+D");
    m.insert("menu.edit.delline", "Supprimer la &ligne\tCtrl+Shift+K");
    m.insert("menu.edit.case", "Convertir la ca&sse");
    m.insert("menu.edit.case.upper", "&MAJUSCULES\tCtrl+Shift+U");
    m.insert("menu.edit.case.lower", "m&inuscules\tCtrl+U");
    m.insert("menu.edit.case.title", "&Premi\u{00e8}re lettre des mots en majuscule");
    m.insert("case.error.title", "Convertir la casse");
    m.insert("case.error.mapping", "La casse n'a pas \u{00e9}t\u{00e9} convertie : {msg}");
    m.insert("menu.edit.lines", "Lig&nes");
    m.insert("menu.edit.lines.sortasc", "Trier par ordre &croissant");
    m.insert("menu.edit.lines.sortdesc", "Trier par ordre &d\u{00e9}croissant");
    m.insert("menu.edit.lines.dedup", "Supprimer les lignes en d&ouble");
    m.insert("menu.edit.lines.join", "&Joindre les lignes");
    m.insert("menu.edit.lines.trim", "Supprimer les &espaces de fin");
    m.insert("lines.error.title", "Lignes");
    m.insert("lines.error.sort", "Les lignes n'ont pas \u{00e9}t\u{00e9} tri\u{00e9}es : {msg}");
    m.insert("menu.edit.selectall", "&Tout s\u{00e9}lectionner\tCtrl+A");
    m.insert("menu.edit.timestamp", "Ins\u{00e9}rer un &horodatage\tF5");

    m.insert("menu.help", "&Aide");
    m.insert("menu.help.update", "V\u{00e9}rifier les mises \u{00e0} &jour...");
    m.insert("menu.help.about", "\u{00c0} &propos de LockNote...");

    m.insert("ctx.cut", "Couper");
    m.insert("ctx.copy", "Copier");
    m.insert("ctx.paste", "Coller");
    m.insert("ctx.pasteplain", "Coller en texte brut");
    m.insert("ctx.selectall", "Tout s\u{00e9}lectionner");
    m.insert("ctx.rename", "Renommer l'onglet...");
    m.insert("ctx.close", "Fermer l'onglet");

    m.insert("ctx.color", "Couleur de l'onglet");
    m.insert("color.none", "Aucune");
    m.insert("color.blue", "Bleu");
    m.insert("color.green", "Vert");
    m.insert("color.red", "Rouge");
    m.insert("color.orange", "Orange");
    m.insert("color.purple", "Violet");
    m.insert("color.yellow", "Jaune");
    m.insert("color.pink", "Rose");
    m.insert("color.gray", "Gris");
    m.insert("option.off", "D\u{00e9}sactiv\u{00e9}");
    m.insert("option.seconds", "{n} s");
    m.insert("option.minutes", "{n} min");

    m.insert("status.stats", "{words} mots | {chars} caract\u{00e8}res | {lines} lignes");
    m.insert("status.saved", "Enregistr\u{00e9}");
    m.insert("status.modified", "Modifi\u{00e9}");
    m.insert("status.encrypted", "Chiffr\u{00e9}");

    m.insert("createpwd.title", "Cr\u{00e9}er un mot de passe");
    m.insert("createpwd.password", "Mot de passe :");
    m.insert("createpwd.confirm", "Confirmer :");
    m.insert("createpwd.create", "Cr\u{00e9}er");
    m.insert("createpwd.cancel", "Annuler");
    m.insert("createpwd.empty", "Le mot de passe ne peut pas \u{00ea}tre vide.");
    m.insert("createpwd.tooshort", "Le mot de passe est trop court.");
    m.insert("createpwd.mismatch", "Les mots de passe ne correspondent pas.");
    m.insert("createpwd.weak", "Faible");
    m.insert("createpwd.fair", "Moyen");
    m.insert("createpwd.strong", "Fort");
    m.insert("createpwd.verystrong", "Tr\u{00e8}s fort");
    m.insert("createpwd.welcome", "Bienvenue sur LockNote");
    m.insert("createpwd.minlen", "Mot de passe (min. {} caract\u{00e8}res) :");
    m.insert("createpwd.desc", "Vos notes sont chiffr\u{00e9}es et stock\u{00e9}es dans cet ex\u{00e9}cutable.\nChoisissez un mot de passe pour les prot\u{00e9}ger.");

    m.insert("unlock.title", "D\u{00e9}verrouiller");
    m.insert("unlock.password", "Mot de passe :");
    m.insert("unlock.btn", "D\u{00e9}verrouiller");
    m.insert("unlock.cancel", "Annuler");
    m.insert("unlock.empty", "Veuillez entrer un mot de passe.");
    m.insert("unlock.decrypting", "D\u{00e9}chiffrement...");
    m.insert("unlock.maxattempts", "Nombre maximum de tentatives atteint. Fermeture.");
    m.insert("unlock.wrong", "Mot de passe incorrect.");
    m.insert("unlock.desc", "Entrez votre mot de passe pour d\u{00e9}verrouiller vos notes.");
    m.insert("unlock.locked.title", "Verrouill\u{00e9}");
    m.insert("unlock.retryin", "Nouvelle tentative dans {seconds}s\u{2026}");
    m.insert("unlock.cryptoerror", "Erreur de chiffrement : {msg}");
    m.insert("lock.overlay.title", "LockNote est verrouill\u{00e9}");
    m.insert("lock.curtain", "Saisissez votre mot de passe pour afficher vos notes.");
    m.insert("lock.curtain.title", "Notes masqu\u{00e9}es");
    m.insert("lock.curtain.waiting_for", "LockNote se verrouille d\u{00e8}s que \u{00ab} {window} \u{00bb} est ferm\u{00e9}. Saisissez votre mot de passe pour afficher vos notes.");
    m.insert("lock.curtain.save_failed", "L'enregistrement avant le verrouillage a \u{00e9}chou\u{00e9} : rien n'est perdu, vos modifications sont conserv\u{00e9}es. Saisissez votre mot de passe pour les afficher.");
    m.insert("lock.curtain.show", "Afficher les notes");
    m.insert("lock.curtain.quit_refused", "LockNote ne peut pas se fermer maintenant : vos modifications non enregistr\u{00e9}es sont derri\u{00e8}re cet \u{00e9}cran. Saisissez votre mot de passe pour les afficher, puis enregistrez ou fermez.");
    m.insert("lock.save_failed", "LockNote n'a pas pu enregistrer avant de se verrouiller :\n{error}\n\nVos notes n'ont pas \u{00e9}t\u{00e9} verrouill\u{00e9}es et vos modifications non enregistr\u{00e9}es sont conserv\u{00e9}es. Elles ne sont toujours pas enregistr\u{00e9}es : r\u{00e9}essayez (Ctrl+S) ou copiez votre texte.");
    m.insert("lock.overlay.click", "Cliquez pour d\u{00e9}verrouiller");
    m.insert("unlock.show", "Afficher le mot de passe");
    m.insert("unlock.hide", "Masquer le mot de passe");
    m.insert("unlock.capslock", "Verr. Maj activ\u{00e9}");

    m.insert("close.unsaved", "Vous avez des modifications non enregistr\u{00e9}es.");
    m.insert("close.remember", "Se souvenir de mon choix");
    m.insert("close.save", "Enregistrer");
    m.insert("close.dontsave", "Ne pas enregistrer");
    m.insert("close.cancel", "Annuler");

    m.insert("search.find", "Rechercher :");
    m.insert("search.replace", "Remplacer :");
    m.insert("search.next", "Suivant");
    m.insert("search.btn.replace", "Remplacer");
    m.insert("search.btn.replaceall", "Tout remplacer");
    m.insert("search.close", "Fermer");
    m.insert("search.notfound", "Non trouv\u{00e9}");
    m.insert("search.title", "Rechercher et remplacer");
    m.insert("search.find.title", "Rechercher");
    m.insert("search.prev", "Pr\u{00e9}c\u{00e9}dent");
    m.insert("search.case", "Respecter la casse");
    m.insert("search.wholeword", "Mot entier");
    m.insert("search.count", "{current} / {total}");
    m.insert("search.count.total", "{total} occurrences");
    m.insert("search.count.none", "Aucune correspondance");
    m.insert("search.replaceall.title", "Tout remplacer");
    m.insert("search.replaceall.msg", "{count} occurrences remplac\u{00e9}es.");

    m.insert("goto.title", "Aller \u{00e0} la ligne");
    m.insert("goto.label", "Num\u{00e9}ro de ligne :");
    m.insert("goto.ok", "OK");
    m.insert("goto.cancel", "Annuler");
    m.insert("goto.invalid", "Num\u{00e9}ro de ligne invalide.");

    m.insert("rename.title", "Renommer l'onglet");
    m.insert("rename.label", "Nom :");
    m.insert("rename.ok", "OK");
    m.insert("rename.cancel", "Annuler");
    m.insert("rename.empty", "Le nom ne peut pas \u{00ea}tre vide.");

    m.insert("settings.title", "Param\u{00e8}tres");
    m.insert("settings.onclose", "\u{00c0} la fermeture :");
    m.insert("settings.ask", "Demander");
    m.insert("settings.always", "Toujours enregistrer");
    m.insert("settings.never", "Ne jamais enregistrer");
    m.insert("settings.theme", "Th\u{00e8}me :");
    m.insert("settings.dark", "Sombre");
    m.insert("settings.light", "Clair");
    m.insert("settings.language", "Langue :");
    m.insert("settings.minpwdlen", "Long. min. mot de passe :");
    m.insert("settings.section.appearance", "Apparence");
    m.insert("settings.section.security", "S\u{00e9}curit\u{00e9}");
    m.insert("settings.section.behavior", "Comportement");
    m.insert("settings.font", "Police de l'\u{00e9}diteur :");
    m.insert("settings.ok", "OK");
    m.insert("settings.cancel", "Annuler");
    m.insert("settings.savekey", "Raccourci de sauvegarde :");
    m.insert("settings.fontsize", "Taille de police :");
    m.insert("settings.pbkdf2", "It\u{00e9}rations PBKDF2 :");
    m.insert("settings.hint", "Indice du mot de passe :");
    m.insert("settings.mintray", "R\u{00e9}duire dans la zone de notification");
    m.insert("settings.autolock", "Verrouillage auto :");
    m.insert("settings.autosave", "Sauvegarde auto :");
    m.insert("settings.clipboard", "Effacement presse-papiers :");
    m.insert("settings.hotkey", "Raccourci global :");
    m.insert("settings.restart", "Veuillez red\u{00e9}marrer LockNote pour appliquer le changement de langue.");
    m.insert("update.restart.failed", "La mise \u{00e0} jour n'a pas pu \u{00ea}tre appliqu\u{00e9}e \u{00e0} cette fermeture, LockNote n'a donc pas \u{00e9}t\u{00e9} relanc\u{00e9}.\n\nRelancez LockNote.\n\nD\u{00e9}tails : {log}");
    m.insert("settings.2fa", "Authentification \u{00e0} deux facteurs");
    m.insert("settings.2fa.enable", "Activer l'authentification TOTP \u{00e0} deux facteurs");
    m.insert("settings.2fa.setup", "Configurer la 2FA...");
    m.insert("settings.2fa.status.enabled", "La 2FA est activ\u{00e9}e");
    m.insert("settings.2fa.status.disabled", "La 2FA est d\u{00e9}sactiv\u{00e9}e");

    m.insert("security.title", "S\u{00e9}curit\u{00e9}");
    m.insert("security.changepw", "Changer le mot de passe...");
    m.insert("security.section", "S\u{00e9}curit\u{00e9}");
    m.insert("preferences.title", "Pr\u{00e9}f\u{00e9}rences");

    m.insert("update.fail", "\u{00c9}chec de la v\u{00e9}rification des mises \u{00e0} jour.");
    m.insert("update.norelease", "Aucune information de version disponible.");
    m.insert("update.uptodate", "Vous \u{00ea}tes \u{00e0} jour !");
    m.insert("update.available", "Une nouvelle version est disponible : {version}");
    m.insert("update.success", "Mise \u{00e0} jour t\u{00e9}l\u{00e9}charg\u{00e9}e avec succ\u{00e8}s.");
    m.insert("update.error", "\u{00c9}chec de la mise \u{00e0} jour : {error}");

    m.insert("totp.empty", "Aucune cl\u{00e9} TOTP configur\u{00e9}e.");
    m.insert("totp.add", "Ajouter une cl\u{00e9}");
    m.insert("totp.add.title", "Ajouter une cl\u{00e9} TOTP");
    m.insert("totp.add.name", "Nom :");
    m.insert("totp.add.secret", "Secret :");
    m.insert("totp.add.invalid", "Secret TOTP invalide.");
    m.insert("totp.add.secret_hint", "Base32 ou lien otpauth://");
    m.insert("totp.add.generate", "G\u{00e9}n\u{00e9}rer");
    m.insert("totp.secret.err.length", "Ce secret a une longueur que le Base32 ne donne jamais : une partie manque ou il a \u{00e9}t\u{00e9} coll\u{00e9} deux fois. Collez-le \u{00e0} nouveau, seul.");
    m.insert("totp.secret.err.short", "Ce secret est trop court (moins de 80 bits) : il en manque sans doute une partie.");
    m.insert("totp.secret.err.doubled", "Le m\u{00ea}me texte appara\u{00ee}t deux fois : il a sans doute \u{00e9}t\u{00e9} coll\u{00e9} deux fois. Videz le champ et collez le secret une seule fois.");
    m.insert("totp.secret.err.duplicate", "Ce lien donne deux fois le m\u{00ea}me param\u{00e8}tre (par exemple deux secrets) : il est refus\u{00e9}, impossible de savoir lequel est le bon.");
    m.insert("totp.secret.err.empty", "Saisissez le secret donn\u{00e9} par le site.");
    m.insert("totp.secret.err.base32", "Ce secret n'est pas du Base32 valide (lettres A-Z et chiffres 2-7).");
    m.insert("totp.secret.err.not_totp", "Seuls les liens otpauth://totp/ sont pris en charge.");
    m.insert("totp.secret.err.no_secret", "Ce lien otpauth:// ne contient pas de secret.");
    m.insert("totp.secret.err.unsupported", "Ce lien demande des codes que LockNote ne produit pas (seulement SHA1, 6 chiffres, 30 secondes).");
    m.insert("totp.remove", "Supprimer");
    m.insert("totp.remove.confirm", "Supprimer cette cl\u{00e9} TOTP ?");
    m.insert("totp.copied", "Code copi\u{00e9} dans le presse-papiers.");
    m.insert("clipboard.failed", "\u{00c9}chec de la copie : {err}");
    m.insert("totp.secret.copied", "Secret copi\u{00e9}.");
    m.insert("totp.select_first", "S\u{00e9}lectionnez d\u{2019}abord une entr\u{00e9}e.");
    m.insert("totp.added", "Entr\u{00e9}e ajout\u{00e9}e.");
    m.insert("totp.removed", "Entr\u{00e9}e supprim\u{00e9}e.");
    m.insert("totp.remove.title", "Supprimer l\u{2019}entr\u{00e9}e TOTP");
    m.insert("totp.remove.msg", "Supprimer \u{00ab} {name} \u{00bb} ? Cette action est irr\u{00e9}versible.");
    m.insert("totp.add.prompt", "Nom de l\u{2019}entr\u{00e9}e :");

    // TOTP Manager
    m.insert("totp.manager.copy", "Copier le code");

    // 2FA / TOTP Verify
    m.insert("2fa.title", "LockNote \u{2014} Authentification \u{00e0} deux facteurs");
    m.insert("2fa.header", "Authentification \u{00e0} deux facteurs");
    m.insert("2fa.prompt", "Entrez le code \u{00e0} 6 chiffres de votre application d'authentification.");
    m.insert("2fa.verify", "V\u{00e9}rifier");
    m.insert("2fa.cancel", "Annuler");
    m.insert("2fa.enter6", "Veuillez entrer un code \u{00e0} 6 chiffres.");
    m.insert("2fa.maxattempts", "Nombre maximum de tentatives atteint.");
    m.insert("2fa.failed", "\u{00c9}chec de la 2FA");
    m.insert("2fa.setup.title", "Configuration TOTP");
    m.insert("2fa.setup.secret", "Secret Base32 :");
    m.insert("2fa.setup.copy", "Copier");
    m.insert("2fa.setup.uri", "URI OtpAuth :");
    m.insert("2fa.setup.verify", "Entrez un code de votre authentificateur pour v\u{00e9}rifier :");
    m.insert("2fa.setup.confirm", "Confirmer");
    m.insert("2fa.setup.verified", "V\u{00e9}rifi\u{00e9} !");
    m.insert("2fa.setup.invalid", "Code invalide, r\u{00e9}essayez");
    m.insert("2fa.pwchanged", "Mot de passe chang\u{00e9} \u{2713}");
    m.insert("2fa.retryin", "Nouvelle tentative dans {seconds}s\u{2026}");

    // Vault / Secret edit
    m.insert("vault.edit", "Modifier le secret");
    m.insert("vault.new", "Nouveau secret");
    m.insert("vault.name", "Nom :");
    m.insert("vault.category", "Cat\u{00e9}gorie :");
    m.insert("vault.value", "Valeur :");
    m.insert("vault.show", "Afficher");
    m.insert("vault.name.empty", "Le nom ne peut pas \u{00ea}tre vide");
    m.insert("vault.btn.add", "Ajout.");
    m.insert("vault.btn.edit", "Modif.");
    m.insert("vault.btn.del", "Suppr.");
    m.insert("vault.btn.copy", "Copier");
    m.insert("vault.btn.copied", "Copi\u{00e9} !");
    m.insert("vault.copied.named", "Copi\u{00e9} : {name}");
    // SSH keys (issue 054)
    m.insert("menu.tools.ssh", "Cl\u{00e9}s &SSH");
    m.insert("menu.tools.ssh_generate", "&G\u{00e9}n\u{00e9}rer une cl\u{00e9} SSH...");
    m.insert("menu.tools.ssh_agent", "&Charger dans ssh-agent...");
    // Password generator
    m.insert("menu.tools.passgen", "&G\u{00e9}n\u{00e9}rateur de mots de passe...");
    m.insert("vault.generate", "G\u{00e9}n\u{00e9}rer...");
    m.insert("passgen.title", "G\u{00e9}n\u{00e9}rateur de mots de passe");
    m.insert("passgen.password", "Mot de passe :");
    m.insert("passgen.show", "Afficher");
    m.insert("passgen.hide", "Masquer");
    m.insert("passgen.generate", "G\u{00e9}n\u{00e9}rer");
    m.insert("passgen.length", "Longueur ({min}-{max}) :");
    m.insert("passgen.upper", "Majuscules (A-Z)");
    m.insert("passgen.lower", "Minuscules (a-z)");
    m.insert("passgen.digits", "Chiffres (0-9)");
    m.insert("passgen.symbols", "Symboles");
    m.insert("passgen.symbol_set", "Symboles autoris\u{00e9}s :");
    m.insert("passgen.exclude_ambiguous", "Exclure les caract\u{00e8}res ambigus ({chars})");
    m.insert("passgen.use", "Utiliser");
    m.insert("passgen.close", "Fermer");
    m.insert("passgen.err.length", "La longueur doit \u{00ea}tre comprise entre {min} et {max}.");
    m.insert("passgen.err.no_class", "S\u{00e9}lectionnez au moins un type de caract\u{00e8}re.");
    m.insert("passgen.err.not_a_symbol", "\u{00ab} {char} \u{00bb} n'est pas un symbole.");
    m.insert("passgen.err.no_symbol", "Aucun symbole disponible : ajoutez des symboles ou d\u{00e9}cochez Symboles.");
    m.insert("ssh.title", "Cl\u{00e9}s SSH");
    m.insert("ssh.gen.title", "G\u{00e9}n\u{00e9}rer une cl\u{00e9} SSH");
    m.insert("ssh.gen.type", "Type :");
    m.insert("ssh.gen.recommended", "recommand\u{00e9}");
    m.insert("ssh.gen.name", "Nom dans le Vault :");
    m.insert("ssh.gen.comment", "Commentaire :");
    m.insert("ssh.gen.passphrase", "Passphrase (facultative) :");
    m.insert("ssh.gen.confirm", "Confirmer la passphrase :");
    m.insert("ssh.gen.generate", "&G\u{00e9}n\u{00e9}rer");
    m.insert("ssh.gen.generating", "G\u{00e9}n\u{00e9}ration...");
    m.insert("ssh.gen.fingerprint", "Empreinte :");
    m.insert("ssh.gen.public", "Cl\u{00e9} publique :");
    m.insert("ssh.gen.copy_public", "Copier la cl\u{00e9} publique");
    m.insert("ssh.gen.copied", "Cl\u{00e9} publique copi\u{00e9}e.");
    m.insert("ssh.gen.save_vault", "Enregistrer (Vault)");
    m.insert("ssh.gen.export", "Exporter...");
    m.insert("ssh.gen.export_title", "Exporter la cl\u{00e9} SSH");
    m.insert("ssh.gen.close", "Fermer");
    m.insert("ssh.gen.mismatch", "Les passphrases ne correspondent pas.");
    m.insert("ssh.gen.no_passphrase_export", "Cette cl\u{00e9} priv\u{00e9}e n'a pas de passphrase : quiconque peut lire le fichier peut l'utiliser. Exporter quand m\u{00ea}me ?");
    m.insert("ssh.gen.exported", "Export\u{00e9}e vers {path} (+ .pub).");
    m.insert("ssh.agent.title", "Charger dans ssh-agent");
    m.insert("ssh.agent.key", "Cl\u{00e9} :");
    m.insert("ssh.agent.passphrase", "Passphrase (si la cl\u{00e9} en a une) :");
    m.insert("ssh.agent.warning", "La cl\u{00e9} sera confi\u{00e9}e \u{00e0} l'agent SSH de Windows, qui peut la conserver hors de LockNote. Elle est retir\u{00e9}e au verrouillage et \u{00e0} la fermeture. En cas d'arr\u{00ea}t anormal, elle sera retir\u{00e9}e au prochain d\u{00e9}verrouillage de ce fichier. Une m\u{00ea}me cl\u{00e9} ajout\u{00e9}e \u{00e0} la main avec ssh-add sera retir\u{00e9}e aussi. Le chargement enregistre aussi ce fichier.");
    m.insert("ssh.agent.load", "&Charger");
    m.insert("ssh.agent.none", "Aucune cl\u{00e9} priv\u{00e9}e SSH dans le Vault. Cr\u{00e9}ez-en une avec Outils > Cl\u{00e9}s SSH > G\u{00e9}n\u{00e9}rer une cl\u{00e9} SSH.");
    m.insert("ssh.agent.loaded", "Cl\u{00e9} charg\u{00e9}e dans ssh-agent :\n{fingerprint}");
    m.insert("ssh.agent.save_failed", "La cl\u{00e9} a \u{00e9}t\u{00e9} charg\u{00e9}e, mais ce fichier n'a pas pu \u{00ea}tre enregistr\u{00e9} : la cl\u{00e9} a donc \u{00e9}t\u{00e9} retir\u{00e9}e de ssh-agent.\n{detail}");
    m.insert("ssh.agent.unload_failed", "Certaines cl\u{00e9}s SSH n'ont pas pu \u{00ea}tre retir\u{00e9}es de ssh-agent. Elles restent list\u{00e9}es et LockNote r\u{00e9}essaiera au prochain d\u{00e9}verrouillage.\n{detail}");
    m.insert("ssh.err.invalid_comment", "Le commentaire doit tenir sur une ligne, sans tabulation ni caract\u{00e8}re de contr\u{00f4}le.");
    m.insert("ssh.err.duplicate", "Un secret nomm\u{00e9} \u{00ab} {name} \u{00bb} existe d\u{00e9}j\u{00e0} dans le Vault.");
    m.insert("ssh.err.passphrase_required", "Cette cl\u{00e9} est prot\u{00e9}g\u{00e9}e : saisissez sa passphrase.");
    m.insert("ssh.err.wrong_passphrase", "Passphrase incorrecte.");
    m.insert("ssh.err.agent_unavailable", "Le service ssh-agent de Windows n'est pas d\u{00e9}marr\u{00e9}.\nDans un PowerShell administrateur :\nGet-Service ssh-agent | Set-Service -StartupType Automatic\nStart-Service ssh-agent");
    m.insert("ssh.err.agent_failure", "ssh-agent a refus\u{00e9} l'op\u{00e9}ration.");
    m.insert("ssh.err.agent_timeout", "ssh-agent n'a pas r\u{00e9}pondu \u{00e0} temps.");
    m.insert("ssh.err.other", "Erreur : {detail}");
    m.insert("ssh.agent.save_failed_still_loaded", "Ce fichier n'a pas pu \u{00ea}tre enregistr\u{00e9} et la cl\u{00e9} est toujours dans ssh-agent. LockNote tentera de la retirer au verrouillage ou \u{00e0} la fermeture de ce fichier.\n{detail}");
    m.insert("ssh.err.invalid_name", "Le nom ne doit contenir ni [ ] ni caract\u{00e8}re de contr\u{00f4}le.");
    m.insert("vault.copy.failed", "\u{00c9}chec de la copie");
    m.insert("vault.btn.reveal", "Voir");
    m.insert("vault.tree.error", "Impossible d'afficher le coffre");
    m.insert("vault.search", "Rechercher un nom");
    m.insert("vault.category.none", "Choisissez une cat\u{00e9}gorie");
    m.insert("vault.cat.menu.new", "Nouvelle cat\u{00e9}gorie\u{2026}");
    m.insert("vault.cat.menu.rename", "Renommer\u{2026}");
    m.insert("vault.cat.menu.delete", "Supprimer");
    m.insert("vault.cat.new.title", "Nouvelle cat\u{00e9}gorie");
    m.insert("vault.cat.rename.title", "Renommer la cat\u{00e9}gorie");
    m.insert("vault.cat.name", "Nom de la cat\u{00e9}gorie :");
    m.insert("vault.cat.delete.title", "Supprimer la cat\u{00e9}gorie");
    m.insert("vault.cat.delete.confirm", "Supprimer la cat\u{00e9}gorie \u{00AB} {name} \u{00BB} ? Ses {count} secret(s) iront dans General.");
    m.insert("vault.cat.error.title", "Cat\u{00e9}gorie");
    m.insert("vault.cat.err.empty", "Le nom ne peut pas \u{00ea}tre vide.");
    m.insert("vault.cat.err.toolong", "Le nom est trop long ({max} caract\u{00e8}res maximum).");
    m.insert("vault.cat.err.char", "Le caract\u{00e8}re \u{00AB} {char} \u{00BB} n'est pas autoris\u{00e9}.");
    m.insert("vault.cat.err.duplicate", "Une cat\u{00e9}gorie \u{00AB} {name} \u{00BB} existe d\u{00e9}j\u{00e0}.");
    m.insert("vault.cat.err.toomany", "Trop de cat\u{00e9}gories ({max} maximum).");
    m.insert("vault.cat.err.general", "La cat\u{00e9}gorie General ne peut \u{00ea}tre ni renomm\u{00e9}e ni supprim\u{00e9}e.");
    m.insert("vault.cat.err.notfound", "Cat\u{00e9}gorie introuvable : {name}");
    m.insert("vault.cat.err.compare", "La comparaison des noms a \u{00e9}chou\u{00e9} : {msg}");

    // Menu bar toggle buttons
    m.insert("menu.toggle.vault", "Coffre");
    m.insert("menu.toggle.totp", "TOTP");
    m.insert("menu.toggle.attach", "Fichiers");
    m.insert("menu.toggle.lock", "\u{1F512} Verrouiller");

    // Attachments
    m.insert("menu.tools.attachments", "&Pi\u{00e8}ces jointes\tCtrl+J");
    // Prompts panel (issue 058)
    m.insert("menu.toggle.prompts", "Prompts");
    m.insert("menu.tools.prompts", "P&rompts");
    m.insert("palette.prompt", "Prompt : {name}");
    m.insert("prompts.btn.insert", "Ins\u{00e9}rer");
    m.insert("prompts.inserted.named", "Ins\u{00e9}r\u{00e9} : {name}");
    m.insert("palette.title", "Palette de commandes");
    m.insert("palette.tab", "Onglet : {name}");
    m.insert("palette.vault", "Coffre : {name}");
    m.insert("prompts.search", "Rechercher (nom ou texte)");
    m.insert("prompts.btn.add", "Ajout.");
    m.insert("prompts.btn.edit", "Modif.");
    m.insert("prompts.btn.del", "Suppr.");
    m.insert("prompts.btn.copy", "Copier");
    m.insert("prompts.copied.named", "Copi\u{00e9} : {name}");
    m.insert("prompts.copy.failed", "\u{00c9}chec de la copie : {err}");
    m.insert("prompts.count", "{count} prompt(s)");
    m.insert("prompts.count.filtered", "{shown} sur {total} prompt(s)");
    m.insert("prompts.list.error", "Impossible d'afficher la liste : {msg}");
    m.insert("prompts.new", "Nouveau prompt");
    m.insert("prompts.edit", "Modifier le prompt");
    m.insert("prompts.name", "Nom :");
    m.insert("prompts.category", "Cat\u{00e9}gorie :");
    m.insert("prompts.text", "Texte :");
    m.insert("prompts.category.default", "G\u{00e9}n\u{00e9}ral");
    m.insert("prompts.err.name", "Le nom ne peut pas \u{00ea}tre vide");
    m.insert("prompts.err.category", "La cat\u{00e9}gorie ne peut pas \u{00ea}tre vide");
    m.insert("prompts.err.text", "Le texte ne peut pas \u{00ea}tre vide");
    m.insert("prompts.err.toolong", "Texte trop long (max {max} Mo)");
    m.insert("prompts.err.name.char", "Le nom ne peut pas contenir {char}");
    m.insert("prompts.err.category.char", "La cat\u{00e9}gorie ne peut pas contenir {char}");
    m.insert("prompts.delete.title", "Supprimer le prompt");
    m.insert("prompts.delete.confirm", "Supprimer le prompt \u{00ab} {name} \u{00bb} ?");
    m.insert("prompts.gone", "Ce prompt a chang\u{00e9} ou disparu entre-temps (import ?) : rien n'a \u{00e9}t\u{00e9} enregistr\u{00e9}.");
    m.insert("vault.edit.gone", "Ce secret a chang\u{00e9} ou disparu entre-temps (import ?) : rien n'a \u{00e9}t\u{00e9} enregistr\u{00e9}.");
    m.insert("attach.btn.add", "Ajout.");
    m.insert("attach.btn.open", "Ouvrir");
    m.insert("attach.btn.save", "Enreg.");
    m.insert("attach.btn.del", "Suppr.");
    m.insert("attach.error.read", "Impossible de lire le fichier : {}");
    m.insert("attach.error.corrupt", "Pi\u{00e8}ce jointe corrompue (SHA-256 invalide).");
    m.insert("attach.error.write_temp", "\u{00c9}chec d\u{2019}\u{00e9}criture du fichier temporaire : {}");
    m.insert("attach.error.save", "Impossible d\u{2019}enregistrer le fichier : {}");
    m.insert("attach.risky.title", "Ouvrir la pi\u{00e8}ce jointe ?");
    m.insert("attach.risky.body", "Cette pi\u{00e8}ce jointe est un fichier ex\u{00e9}cutable. L\u{2019}ouvrir va lancer du code sur votre ordinateur. Ne continuez que si vous faites confiance \u{00e0} la source de ce coffre.\n\nOuvrir quand m\u{00ea}me ?");
    m.insert("attach.confirm.delete", "Supprimer la pi\u{00e8}ce jointe \u{00ab} {} \u{00bb} ?");
    m.insert("attach.confirm.title", "Confirmation");
    m.insert("attach.save.title", "Enregistrer \u{00ab} {} \u{00bb} sous");
    m.insert("attach.add.title", "Joindre un fichier");

    // St\u{00e9}ganographie
    m.insert("menu.tools.stego", "St\u{00e9}&ganographie");
    m.insert("menu.tools.stego_hide", "&Cacher dans une image...");
    m.insert("menu.tools.stego_extract", "E&xtraire depuis une image...");
    m.insert("stego.confirm_action", "export st\u{00e9}ganographique");
    m.insert("stego.dlg.open_carrier", "Choisir l\u{2019}image porteuse");
    m.insert("stego.dlg.save", "Enregistrer l\u{2019}image st\u{00e9}ganographique");
    m.insert("stego.dlg.open_extract", "Choisir l\u{2019}image \u{00e0} extraire");
    m.insert("stego.title", "St\u{00e9}ganographie");
    m.insert("stego.success", "Contenu cach\u{00e9} avec succ\u{00e8}s.");
    m.insert("stego.size", "Taille");
    m.insert("stego.warning_recompress", "Ne pas partager via WhatsApp, Instagram, etc.\nLa recompression d\u{00e9}truit les donn\u{00e9}es cach\u{00e9}es.");
    m.insert("stego.extracted", "Notes cach\u{00e9}es extraites avec succ\u{00e8}s.");
    m.insert("stego.nodata_title", "Aucune donn\u{00e9}e cach\u{00e9}e");
    m.insert("stego.nodata", "Aucune donn\u{00e9}e cach\u{00e9}e dans cette image.");
    m.insert("stego.error", "Erreur");
    m.insert("stego.error.read_image", "Impossible de lire l\u{2019}image");
    m.insert("stego.error.format_title", "Format non support\u{00e9}");
    m.insert("stego.error.format", "Format non support\u{00e9} (JPEG, PNG, BMP).");
    m.insert("stego.error.write", "\u{00c9}criture du fichier impossible");
    m.insert("stego.error.toolarge_title", "Donn\u{00e9}es trop volumineuses");
    m.insert("stego.error.toolarge", "Notes trop volumineuses pour cette image.");
    m.insert("stego.error.toolarge_hint", "Choisir une image porteuse plus grande.");
    m.insert("stego.error.embed", "Impossible de cacher les donn\u{00e9}es");
    m.insert("stego.error.encrypt", "\u{00c9}chec du chiffrement lors de l\u{2019}export image : {msg}");
    m.insert("stego.error.decrypt_title", "Extraction \u{00e9}chou\u{00e9}e");
    m.insert("stego.error.decrypt", "Mot de passe incorrect ou donn\u{00e9}es non LockNote.");
    m.insert("stego.error.extract", "Impossible d\u{2019}extraire les donn\u{00e9}es");
    m.insert("stego.pw.title_hide", "Mot de passe de l\u{2019}image");
    m.insert("stego.pw.title_extract", "Mot de passe de l\u{2019}image");
    m.insert("stego.pw.prompt_hide", "Mot de passe pour prot\u{00e9}ger cette image :");
    m.insert("stego.pw.prompt_extract", "Mot de passe utilis\u{00e9} pour cacher les donn\u{00e9}es :");
    m.insert("stego.pw.confirm", "Confirmer le mot de passe :");
    m.insert("stego.pw.empty", "Le mot de passe ne peut pas \u{00ea}tre vide.");
    m.insert("stego.pw.mismatch", "Les mots de passe ne correspondent pas.");

    // History


    m.insert("export.prompt", "Exporter la note en texte brut ?");
    m.insert("export.btn", "Exporter");
    m.insert("export.warning", "Le fichier export\u{00e9} ne sera PAS chiffr\u{00e9}.");
    m.insert("export.dlg.title", "Exporter la note");
    m.insert("export.dlg.filter", "Fichiers texte (*.txt)|*.txt|Tous les fichiers (*.*)|*.*");
    m.insert("export.fail", "\u{00c9}chec de l'exportation du fichier.");

    // Export encoding
    m.insert("export.encoding.title", "Encodage d'exportation");
    m.insert("export.encoding.label", "Choisir l'encodage pour l'exportation :");

    // Drag & drop
    m.insert("drop.binary", "Ce type de fichier n'est pas pris en charge. LockNote ne peut importer que des fichiers texte.");
    m.insert("drop.invalid", "Ce fichier n'a pas pu \u{00ea}tre lu comme du texte. Il s'agit peut-\u{00ea}tre d'un fichier binaire.");
    m.insert("drop.choice.title", "Fichier d\u{00e9}pos\u{00e9}");
    m.insert("drop.choice.text", "Que voulez-vous faire avec ce fichier texte ?");
    m.insert("drop.choice.binary", "Ce fichier peut \u{00ea}tre ajout\u{00e9} en pi\u{00e8}ce jointe.");
    m.insert("drop.choice.insert", "Ins\u{00e9}rer le texte");
    m.insert("drop.choice.attach", "Ajouter en pi\u{00e8}ce jointe");
    m.insert("drop.choice.cancel", "Annuler");
    m.insert("drop.directory", "Les dossiers ne peuvent pas \u{00ea}tre d\u{00e9}pos\u{00e9}s dans LockNote. D\u{00e9}posez plut\u{00f4}t des fichiers individuels.");

    m.insert("save.error.title", "Erreur de sauvegarde");
    m.insert("save.error.msg", "\u{00c9}chec de la sauvegarde : {}");
    m.insert("export.success.title", "Exportation");
    m.insert("export.success.msg", "Note export\u{00e9}e vers :\n{}");
    m.insert("export.error.title", "Erreur d'exportation");
    m.insert("export.error.msg", "\u{00c9}chec de l'exportation : {}");
    m.insert("export.warning_title", "Avertissement d'exportation");
    m.insert("export.warning_lossy", "Certains caract\u{00e8}res ne peuvent pas \u{00ea}tre repr\u{00e9}sent\u{00e9}s dans cet encodage et ont \u{00e9}t\u{00e9} remplac\u{00e9}s. Continuer ?");
    m.insert("menu.file.backup_export", "Exporter une sauvegarde compl\u{00e8}te...");
    m.insert("menu.file.backup_import", "Importer une sauvegarde compl\u{00e8}te...");
    m.insert("backup.export.save_title", "Enregistrer la sauvegarde compl\u{00e8}te");
    m.insert("backup.filter", "Sauvegarde LockNote (*.lnbak)|Tous les fichiers (*.*)");
    m.insert("backup.export.done", "Sauvegarde compl\u{00e8}te enregistr\u{00e9}e dans :\n{path}\n\nElle contient tous les onglets, instantan\u{00e9}s, le coffre, les pi\u{00e8}ces jointes et les param\u{00e8}tres, chiffr\u{00e9}s avec le mot de passe de cette note.");
    m.insert("backup.export.overwrite", "{path}\n\nCe fichier existe d\u{00e9}j\u{00e0}. Le remplacer ?");
    m.insert("backup.export.error", "\u{00c9}chec de la sauvegarde : {msg}");
    m.insert("backup.import.open_title", "Importer une sauvegarde compl\u{00e8}te");
    m.insert("backup.import.pw_title", "Mot de passe de la sauvegarde");
    m.insert("backup.import.pw_label", "Mot de passe de cette sauvegarde :");
    m.insert("backup.import.wrong_password", "Mot de passe incorrect, ou fichier corrompu.");
    m.insert("backup.import.not_backup", "Ce fichier n'est pas une sauvegarde LockNote.");
    m.insert("backup.import.newer", "Cette sauvegarde provient d'une version plus r\u{00e9}cente de LockNote (format {found}, cette version lit jusqu'au format {supported}). Mettez LockNote \u{00e0} jour.");
    m.insert("backup.import.corrupt", "Cette sauvegarde est endommag\u{00e9}e : {msg}");
    m.insert("backup.import.read_error", "Impossible de lire le fichier : {msg}");
    m.insert("backup.import.too_large", "Ce fichier est trop volumineux pour \u{00ea}tre une sauvegarde.");
    m.insert("backup.import.confirm", "Remplacer TOUT le contenu actuel par cette sauvegarde ?\n\nSauvegarde cr\u{00e9}\u{00e9}e le {date} :\n- {tabs} onglet(s)\n- {prompts} prompt(s)\n- {secrets} secret(s) du coffre\n- {attachments} pi\u{00e8}ce(s) jointe(s)\n\nVos onglets, prompts, coffre, pi\u{00e8}ces jointes et param\u{00e8}tres actuels (dont l'authentification \u{00e0} deux facteurs) seront remplac\u{00e9}s. Impossible \u{00e0} annuler une fois la note enregistr\u{00e9}e.");
    m.insert("backup.import.done", "Sauvegarde import\u{00e9}e. Enregistrez pour la conserver. Le r\u{00e9}glage de langue s'applique apr\u{00e8}s enregistrement et red\u{00e9}marrage de LockNote.");
    m.insert("autobackup.failed.import", "La sauvegarde de s\u{00e9}curit\u{00e9} n'a pas pu \u{00ea}tre \u{00e9}crite, rien n'a donc \u{00e9}t\u{00e9} import\u{00e9}.\n\n{msg}");
    m.insert("autobackup.failed.update", "La sauvegarde de s\u{00e9}curit\u{00e9} n'a pas pu \u{00ea}tre \u{00e9}crite, la mise \u{00e0} jour n'a donc pas \u{00e9}t\u{00e9} t\u{00e9}l\u{00e9}charg\u{00e9}e.\n\n{msg}");
    m.insert("autobackup.purge.ask", "{n} sauvegarde(s) automatique(s) ont \u{00e9}t\u{00e9} faites avec votre ancien mot de passe et restent ouvrables avec lui.\n\nLes supprimer maintenant ?");
    m.insert("autobackup.purge.partial", "{n} sauvegarde(s) automatique(s) n'ont pas pu \u{00ea}tre supprim\u{00e9}es. Supprimez-les \u{00e0} la main dans :\n{dir}");
    m.insert("menu.file.backups", "&Sauvegardes de s\u{00e9}curit\u{00e9}...");
    m.insert("autobackup.dlg.title", "Sauvegardes de s\u{00e9}curit\u{00e9}");
    m.insert("autobackup.dlg.list", "Sauvegardes (la plus r\u{00e9}cente en premier) :");
    m.insert("autobackup.dlg.empty", "Aucune sauvegarde de s\u{00e9}curit\u{00e9} pour l'instant.");
    m.insert("autobackup.dlg.details", "{kind}\n{date} (UTC)\n{size}\n\nS'ouvre avec le mot de passe que vous aviez \u{00e0} ce moment-l\u{00e0}.");
    m.insert("autobackup.dlg.restore", "Restaurer");
    m.insert("autobackup.dlg.create", "Cr\u{00e9}er maintenant");
    m.insert("totp.nextcode", "Prochain code dans {n} s");
    m.insert("autobackup.dlg.delete", "Supprimer");
    m.insert("autobackup.dlg.folder", "Ouvrir le dossier");
    m.insert("autobackup.dlg.close", "Fermer");
    m.insert("autobackup.dlg.delete.title", "Supprimer la sauvegarde");
    m.insert("autobackup.dlg.delete.confirm", "Supprimer d\u{00e9}finitivement cette sauvegarde ?");
    m.insert("autobackup.dlg.delete.failed", "Impossible de supprimer cette sauvegarde : {msg}");
    m.insert("autobackup.dlg.folder.failed", "Impossible d'ouvrir le dossier : {msg}");
    m.insert("autobackup.dlg.list_failed", "Impossible de lire le dossier des sauvegardes : {msg}");
    m.insert("autobackup.kind.pre_import", "Avant import");
    m.insert("autobackup.kind.pre_update", "Avant mise \u{00e0} jour");
    m.insert("autobackup.kind.manual", "Manuelle");
    m.insert("autobackup.manual.done", "Sauvegarde cr\u{00e9}\u{00e9}e.");
    m.insert("autobackup.manual.failed", "La sauvegarde n'a pas pu \u{00ea}tre \u{00e9}crite.\n\n{msg}");
    m.insert("menu.file.import_exe", "Importer depuis un autre LockNote...");
    m.insert("import_exe.title", "Importer depuis un autre LockNote");
    m.insert("import_exe.open_title", "Choisir le fichier LockNote \u{00e0} importer");
    m.insert("import_exe.filter", "LockNote (*.exe)|Tous les fichiers (*.*)");
    m.insert("import_exe.same_file", "C'est le fichier que vous utilisez. Choisissez votre ancien fichier LockNote.");
    // Startup conflict between the pending save and a more recent program file (issue 055)
    m.insert("conflict.title", "LockNote - Deux versions de vos notes");
    m.insert("conflict.body", "LockNote a trouv\u{00e9} deux versions diff\u{00e9}rentes de vos notes.\n\nSauvegarde en attente, pas encore \u{00e9}crite dans le fichier programme :\n{tmp_date} (UTC)\n\nFichier programme, qui contient une autre version, derni\u{00e8}re modification :\n{exe_date} (UTC)\n{exe}\n\nOui : garder la sauvegarde en attente.\nNon : garder les notes du fichier programme.\nAnnuler : quitter sans rien modifier.\n\nLa version que vous ne gardez pas n'est pas supprim\u{00e9}e. Elle est d\u{00e9}plac\u{00e9}e dans :\n{dir}");
    m.insert("conflict.exe_empty", "Remarque : le fichier programme ne contient aucune note.");
    m.insert("conflict.failed", "Votre choix n'a pas pu \u{00ea}tre appliqu\u{00e9}. Rien n'a \u{00e9}t\u{00e9} supprim\u{00e9}.\n\n{msg}\n\nLockNote va se fermer.");
    m.insert("pending.failed", "LockNote ne peut pas ouvrir vos notes en s\u{00e9}curit\u{00e9}. Rien n'a \u{00e9}t\u{00e9} modifi\u{00e9}.\n\n{msg}\n\nLockNote va se fermer.");
    m.insert("import_exe.no_data", "Aucune donn\u{00e9}e LockNote dans ce fichier.");
    m.insert("import_exe.pw_title", "Mot de passe du fichier import\u{00e9}");
    m.insert("import_exe.pw_label", "Mot de passe du fichier LockNote que vous importez :");
    m.insert("import_exe.confirm", "Remplacer TOUT le contenu actuel par les donn\u{00e9}es de ce fichier LockNote ?\n\nIl contient :\n- {tabs} onglet(s)\n- {prompts} prompt(s)\n- {secrets} secret(s) du coffre\n- {attachments} pi\u{00e8}ce(s) jointe(s)\n\nVos onglets, prompts, coffre, pi\u{00e8}ces jointes et param\u{00e8}tres actuels (dont l'authentification \u{00e0} deux facteurs) seront remplac\u{00e9}s. Le fichier import\u{00e9} n'est pas modifi\u{00e9}.");
    m.insert("import_exe.done", "Donn\u{00e9}es import\u{00e9}es. Enregistrez pour les conserver : elles seront alors prot\u{00e9}g\u{00e9}es par le mot de passe de CE fichier. Le r\u{00e9}glage de langue s'applique apr\u{00e8}s enregistrement et red\u{00e9}marrage de LockNote.");

    m.insert("tray.restore", "&Restaurer");
    m.insert("tray.quit", "&Quitter");

    m.insert("about.title", "\u{00c0} propos de LockNote");
    m.insert("about.desc", "Bloc-notes chiffr\u{00e9} autonome pour Windows");

    m.insert("confirmpwd.title", "Confirmer le mot de passe");
    m.insert("confirmpwd.export", "Saisissez votre mot de passe pour exporter :");
    m.insert("confirmpwd.print", "Saisissez votre mot de passe pour imprimer :");
    m.insert("about.version", "Version {version}");
    m.insert("totp.setup.for", "Configuration TOTP pour : {name}");
    m.insert("2fa.invalid.left", "Code invalide. Tentatives restantes : {n}.");
    m.insert("attach.error.title", "Erreur");
    m.insert("attach.limit.title", "Pi\u{00e8}ce jointe");
    m.insert("attach.error.toolarge", "Fichier trop volumineux : {size} (max. {max})");
    m.insert("attach.error.totaltoolarge", "Pi\u{00e8}ces jointes trop volumineuses au total : {size} (max. {max})");
    m.insert("status.word", "{n} mot");
    m.insert("status.words", "{n} mots");
    m.insert("status.char", "{n} caract\u{00e8}re");
    m.insert("status.chars", "{n} caract\u{00e8}res");
    m.insert("status.line", "{n} ligne");
    m.insert("status.lines", "{n} lignes");
    m.insert("status.caret", "Ligne {line}, col. {col}");
    m.insert("status.selected.one", "{n} s\u{00e9}lectionn\u{00e9}");
    m.insert("status.selected.many", "{n} s\u{00e9}lectionn\u{00e9}s");
    m.insert("save.error.encrypt", "\u{00c9}chec du chiffrement : {}");
    m.insert("export.success.encoding", "Note export\u{00e9}e en {encoding} vers :\n{path}");
    m.insert("filter.text", "Fichiers texte (*.txt)");
    m.insert("filter.all", "Tous les fichiers (*.*)");
    m.insert("filter.images", "Images (*.jpg;*.jpeg;*.png;*.bmp)");
    m.insert("filter.image", "Image (*.{ext})");
    m.insert("update.available.title", "Mise \u{00e0} jour disponible");
    m.insert("update.available.ask", "Une nouvelle version ({version}) est disponible.\nVoulez-vous la t\u{00e9}l\u{00e9}charger ?");
    m.insert("update.downloaded.title", "Mise \u{00e0} jour t\u{00e9}l\u{00e9}charg\u{00e9}e");
    m.insert("update.downloaded.ask", "Mise \u{00e0} jour t\u{00e9}l\u{00e9}charg\u{00e9}e.\n\nRed\u{00e9}marrer LockNote maintenant pour l'appliquer ?");
    m.insert("update.error.title", "Erreur de mise \u{00e0} jour");
    m.insert("update.uptodate.title", "\u{00c0} jour");
    m.insert("update.uptodate.msg", "Vous utilisez la derni\u{00e8}re version ({version}).");
    m.insert("update.checkfailed.title", "\u{00c9}chec de la v\u{00e9}rification des mises \u{00e0} jour");
    m.insert("update.checkfailed.msg", "Impossible de v\u{00e9}rifier les mises \u{00e0} jour : {error}");
    m.insert("vault.delete.title", "Supprimer le secret");
    m.insert("vault.delete.msg", "Voulez-vous vraiment supprimer ce secret ?");
    m.insert("filter.ext", "Fichiers {EXT} (*.{ext})");
    m.insert("close.title", "Modifications non enregistr\u{00e9}es");
    m.insert("close.msg", "Modifications non enregistr\u{00e9}es. Enregistrer avant de fermer ?");
    m.insert("close.save_failed", "LockNote n'a pas pu enregistrer :\n{error}\n\nVos modifications non enregistr\u{00e9}es sont toujours dans la fen\u{00ea}tre. Gardez LockNote ouvert pour r\u{00e9}essayer ou copier votre texte ; si vous fermez sans enregistrer, elles seront perdues.");
    m.insert("close.keep_open", "Garder LockNote ouvert");
    m.insert("close.close_unsaved", "Fermer sans enregistrer");
    m.insert("close.stuck", "LockNote ne peut pas encore se fermer : il attend la fermeture de \u{00ab} {window} \u{00bb}.\n\nContinuez d'attendre, ou fermez sans enregistrer : les modifications non enregistr\u{00e9}es seront alors perdues.");
    m.insert("close.keep_waiting", "Continuer d'attendre");
    m.insert("endsession.blocked", "LockNote n'a pas pu enregistrer vos notes ({error}). Annulez, puis enregistrez ou copiez votre texte.");
    m.insert("close.remember_not_saved", "Votre choix n'a pas \u{00e9}t\u{00e9} m\u{00e9}moris\u{00e9} : LockNote garde ses param\u{00e8}tres dans la note, et la note n'est pas enregistr\u{00e9}e. Pour ne plus \u{00ea}tre questionn\u{00e9}, choisissez \u{00ab} {onclose} {never} \u{00bb} dans {menu}, puis enregistrez.");

    m
}

// ---------------------------------------------------------------------------
// Spanish translations
// ---------------------------------------------------------------------------

fn build_es() -> Dict {
    let mut m = HashMap::new();

    m.insert("app.title", "LockNote");
    m.insert("app.title.modified", "LockNote *");

    m.insert("menu.file", "&Archivo");
    m.insert("menu.file.save", "&Guardar\tCtrl+S");
    m.insert("menu.file.changepass", "Cambiar &contrase\u{00f1}a...");
    m.insert("menu.file.settings", "&Ajustes...");
    m.insert("menu.file.quit", "&Salir\tAlt+F4");
    m.insert("menu.file.print", "&Imprimir...\tCtrl+P");
    m.insert("print.footer", "P\u{00e1}gina {n} / {m}");
    m.insert("print.error.title", "Imprimir");
    m.insert("print.error.failed", "La impresi\u{00f3}n ha fallado: {error}");
    m.insert("print.error.too_small", "El \u{00e1}rea imprimible de la p\u{00e1}gina es demasiado peque\u{00f1}a para la fuente del editor.");
    m.insert("menu.file.export", "E&xportar...");

    m.insert("menu.view", "&Ver");
    m.insert("menu.view.ontop", "Siempre &encima");
    m.insert("menu.view.wordwrap", "Ajuste de &l\u{00ed}nea");
    m.insert("menu.view.linenumbers", "&N\u{00fa}meros de l\u{00ed}nea");
    m.insert("settings.font.failed", "No se pudo aplicar la nueva fuente del editor; el editor conserva su fuente actual.\n\nDetalles: {detail}");
    m.insert("menu.view.vault", "Gestor de &b\u{00f3}veda...");

    m.insert("menu.tab", "&Pesta\u{00f1}a");
    m.insert("menu.tab.new", "&Nueva pesta\u{00f1}a\tCtrl+T");
    m.insert("menu.tab.close", "&Cerrar pesta\u{00f1}a\tCtrl+W");
    m.insert("menu.tab.rename", "&Renombrar pesta\u{00f1}a...\tF2");
    m.insert("menu.tab.next", "Pesta\u{00f1}a siguiente\tCtrl+Tab");
    m.insert("menu.tab.prev", "Pesta\u{00f1}a anterior\tCtrl+Shift+Tab");
    m.insert("menu.tab.compare", "Com&parar con el \u{00fa}ltimo guardado...");
    m.insert("compare.title", "Comparar con el \u{00fa}ltimo guardado");
    m.insert("compare.identical", "No hay cambios desde el \u{00fa}ltimo guardado.");
    m.insert("compare.too_large", "Esta pesta\u{00f1}a es demasiado larga para compararla (m\u{00e1}s de {} l\u{00ed}neas en un lado).");
    m.insert("compare.failed", "No se pudo calcular la comparaci\u{00f3}n.");
    m.insert("compare.close", "Cerrar");
    m.insert("menu.edit.goto_heading", "Ir al &encabezado...\tCtrl+Shift+O");
    m.insert("heading.title", "Ir al encabezado");
    m.insert("heading.filter", "Filtro:");
    m.insert("heading.none", "No hay encabezados en esta pesta\u{00f1}a.");
    m.insert("heading.no_match", "Ning\u{00fa}n encabezado coincide con el filtro.");
    m.insert("tab.limit.title", "L\u{00ed}mite de pesta\u{00f1}as");
    m.insert("tab.limit.msg", "N\u{00fa}mero m\u{00e1}ximo de pesta\u{00f1}as alcanzado (20).");
    m.insert("tab.close.title", "Cerrar pesta\u{00f1}a");
    m.insert("tab.close.msg", "\u{00bf}Cerrar la pesta\u{00f1}a \u{00ab}{name}\u{00bb}?\nSu contenido se perder\u{00e1}.");
    m.insert("tab.ctx.rename", "Renombrar pesta\u{00f1}a...");
    m.insert("tab.ctx.color", "Color");
    m.insert("tab.ctx.no_color", "Sin color");
    m.insert("tab.ctx.close", "Cerrar pesta\u{00f1}a");

    // Tools menu
    m.insert("menu.tools", "&Herramientas");
    m.insert("menu.tools.changepass", "Cambiar &contrase\u{00f1}a...");
    m.insert("menu.tools.totp", "Gestor &TOTP");

    m.insert("menu.settings.preferences", "&Preferencias...");
    m.insert("menu.settings.security", "&Seguridad...");
    m.insert("menu.settings.tempfiles", "Archivos &temporales...");

    m.insert("status.path_copied", "Ruta copiada");
    m.insert("status.path_copy_failed", "No se pudo copiar la ruta");
    m.insert("tempfiles.owner.current", "Este archivo LockNote");
    m.insert("tempfiles.owner.present", "Otro archivo LockNote");
    m.insert("tempfiles.owner.orphan", "Hu\u{00e9}rfano: su archivo LockNote ya no existe");
    m.insert("tempfiles.owner.folder_missing", "Su carpeta no est\u{00e1} (\u{00bf}unidad desconectada?)");
    m.insert("tempfiles.owner.unreachable", "Su archivo LockNote no se puede comprobar");
    m.insert("tempfiles.owner.no_companion", "Dejado por una versi\u{00f3}n anterior");
    m.insert("tempfiles.owner.companion_unusable", "Archivo compa\u{00f1}ero inutilizable");
    m.insert("tempfiles.meaning.current", "El guardado pendiente del archivo LockNote abierto. Se escribe en \u{00e9}l al cerrar LockNote.");
    m.insert("tempfiles.meaning.present", "Un guardado que espera a ese archivo LockNote: se escribe en \u{00e9}l la pr\u{00f3}xima vez que ese archivo se abra y se cierre. Se conserva.");
    m.insert("tempfiles.meaning.orphan", "Ese archivo LockNote ya no existe, pero su carpeta s\u{00ed}. Este archivo contiene el \u{00fa}ltimo guardado hecho con \u{00e9}l: eliminarlo hace perder esas notas.");
    m.insert("tempfiles.meaning.folder_missing", "No se encuentran ni ese archivo LockNote ni su carpeta: unidad o recurso de red desconectado, o carpeta eliminada. Se conserva: vuelva a conectar la unidad y abra ese archivo.");
    m.insert("tempfiles.meaning.unreachable", "No se puede comprobar si ese archivo LockNote existe ({error}). Se conserva.");
    m.insert("tempfiles.meaning.no_companion", "Dejado por LockNote 1.2.23 o anterior, que no registraba el archivo LockNote al que pertenece. Se conserva: ese archivo lo recupera al abrirse.");
    m.insert("tempfiles.meaning.companion_unusable", "Su archivo compa\u{00f1}ero no se puede usar ({reason}). Se conserva.");
    m.insert("tempfiles.owner.other_volume", "Su unidad ahora designa otro disco");
    m.insert("tempfiles.owner.tmp_unreadable", "No se puede examinar");
    m.insert("tempfiles.meaning.other_volume", "No se encuentra ese archivo LockNote, y su letra de unidad o recurso compartido designa ahora otro disco que cuando se guard\u{00f3} (una llave USB sustituida por otra). Se conserva: vuelva a poner el disco correcto y abra ese archivo.");
    m.insert("tempfiles.meaning.tmp_unreadable", "Este archivo no se puede examinar ({error}). Se conserva.");
    m.insert("tempfiles.dlg.reading", "Leyendo...");
    m.insert("tempfiles.dlg.title", "Archivos temporales");
    m.insert("tempfiles.dlg.intro", "LockNote no puede reescribir su propio archivo mientras se ejecuta. Cada guardado se escribe primero en un archivo temporal de la carpeta de abajo; al cerrar LockNote, un programa auxiliar lo copia en el archivo LockNote. Hasta entonces, el archivo temporal es la \u{00fa}nica copia de su \u{00fa}ltimo guardado: no lo elimine a mano.\n\nLa carpeta tambi\u{00e9}n contiene: {hash}.base (compa\u{00f1}ero de cada archivo temporal), conflicts\\ (versiones apartadas por un conflicto, nunca eliminadas), swap-error.log (registro de las copias), import.log y auto-backups\\. Para acceder: Abrir carpeta, o escriba %LOCALAPPDATA%\\LockNote en la barra de direcciones del Explorador.");
    m.insert("tempfiles.dlg.folder", "Carpeta: {dir}");
    m.insert("tempfiles.dlg.open_folder", "Abrir carpeta");
    m.insert("tempfiles.dlg.copy_path", "Copiar ruta");
    m.insert("tempfiles.dlg.list", "Guardados pendientes (.tmp):");
    m.insert("tempfiles.dlg.delete", "Eliminar hu\u{00e9}rfano...");
    m.insert("tempfiles.dlg.apply", "Este archivo LockNote:");
    m.insert("tempfiles.dlg.close_app", "Cerrar LockNote ahora");
    m.insert("tempfiles.dlg.swap_log", "Registro de copias");
    m.insert("tempfiles.dlg.import_log", "Registro de importaciones");
    m.insert("tempfiles.dlg.close", "Cerrar");
    m.insert("tempfiles.dlg.empty", "Ning\u{00fa}n archivo temporal: todos los guardados se han escrito.");
    m.insert("tempfiles.dlg.list_failed", "No se pudo leer la carpeta: {msg}");
    m.insert("tempfiles.dlg.folder_absent", "A\u{00fa}n no se ha escrito nada: la carpeta no existe.");
    m.insert("tempfiles.dlg.folder_failed", "No se pudo abrir la carpeta: {msg}");
    m.insert("tempfiles.dlg.copy_failed", "No se pudo copiar la ruta: {msg}");
    m.insert("tempfiles.dlg.exe_unknown", "desconocido");
    m.insert("tempfiles.dlg.details", "Archivo: {tmp}\nArchivo LockNote: {exe}\n{size}, modificado el {date} (UTC)\n\n{meaning}");
    m.insert("tempfiles.apply.pending", "Un guardado de {size} espera en {tmp}. Se escribir\u{00e1} en {exe} al cerrar LockNote.");
    m.insert("tempfiles.apply.nothing", "Nada pendiente: {exe} contiene el \u{00fa}ltimo guardado.");
    m.insert("tempfiles.apply.undecided", "Un archivo espera en {tmp}, pero cerrar LockNote no lo escribir\u{00e1} en {exe} ({reason}): el pr\u{00f3}ximo inicio decidir\u{00e1}.");
    m.insert("tempfiles.apply.reason.conflict", "el archivo LockNote se modific\u{00f3} en otro lugar");
    m.insert("tempfiles.apply.reason.tmp_unreadable", "no se puede leer");
    m.insert("tempfiles.apply.reason.exe_unreadable", "el archivo LockNote no se puede leer");
    m.insert("tempfiles.apply.reason.ignored", "no contiene ning\u{00fa}n guardado utilizable");
    m.insert("tempfiles.log.empty", "A\u{00fa}n no hay eventos registrados ({file}).");
    m.insert("tempfiles.log.failed", "No se pudo abrir {file}: {msg}");
    m.insert("tempfiles.delete.title", "Eliminar un archivo temporal hu\u{00e9}rfano");
    m.insert("tempfiles.delete.confirm", "\u{00bf}Eliminar {tmp}?\n\nContiene el \u{00fa}ltimo guardado hecho con {exe}, que ya no existe. Las notas que contiene se perder\u{00e1}n.");
    m.insert("tempfiles.delete.failed", "No eliminado: {msg}");

    m.insert("menu.edit", "&Editar");
    m.insert("menu.edit.cut", "Cor&tar\tCtrl+X");
    m.insert("menu.edit.copy", "&Copiar\tCtrl+C");
    m.insert("menu.edit.paste", "&Pegar\tCtrl+V");
    m.insert("menu.edit.pasteplain", "Pegar como texto &plano\tCtrl+Shift+V");
    m.insert("menu.edit.find", "&Buscar...\tCtrl+F");
    m.insert("menu.edit.replace", "&Reemplazar...\tCtrl+H");
    m.insert("menu.edit.searchall", "Buscar en todas las pe&sta\u{00f1}as...\tCtrl+Shift+F");
    m.insert("searchall.title", "Buscar en todas las pesta\u{00f1}as");
    m.insert("searchall.query", "Buscar:");
    m.insert("searchall.go", "Ir a");
    m.insert("searchall.vault", "B\u{00f3}veda: {category} / {name}");
    m.insert("searchall.prompt", "Prompt: {name}");
    m.insert("searchall.more", "No se muestran m\u{00e1}s resultados (l\u{00ed}mite: {max})");
    m.insert("search.regex", "Expresi\u{00f3}n regular");
    m.insert("search.regex.invalid", "Expresi\u{00f3}n regular no v\u{00e1}lida");
    m.insert("menu.edit.goto", "&Ir a la l\u{00ed}nea...\tCtrl+G");
    m.insert("menu.edit.dupline", "&Duplicar l\u{00ed}nea\tCtrl+D");
    m.insert("menu.edit.delline", "E&liminar l\u{00ed}nea\tCtrl+Shift+K");
    m.insert("menu.edit.case", "Ca&mbiar may\u{00fa}sculas/min\u{00fa}sculas");
    m.insert("menu.edit.case.upper", "&MAY\u{00da}SCULAS\tCtrl+Shift+U");
    m.insert("menu.edit.case.lower", "mi&n\u{00fa}sculas\tCtrl+U");
    m.insert("menu.edit.case.title", "&Tipo T\u{00ed}tulo");
    m.insert("case.error.title", "Convertir may\u{00fa}sculas");
    m.insert("case.error.mapping", "No se convirtieron las may\u{00fa}sculas: {msg}");
    m.insert("menu.edit.lines", "L\u{00ed}&neas");
    m.insert("menu.edit.lines.sortasc", "Ordenar l\u{00ed}neas &ascendente");
    m.insert("menu.edit.lines.sortdesc", "Ordenar l\u{00ed}neas &descendente");
    m.insert("menu.edit.lines.dedup", "Eliminar l\u{00ed}neas du&plicadas");
    m.insert("menu.edit.lines.join", "&Unir l\u{00ed}neas");
    m.insert("menu.edit.lines.trim", "Quitar espacios &finales");
    m.insert("lines.error.title", "L\u{00ed}neas");
    m.insert("lines.error.sort", "No se ordenaron las l\u{00ed}neas: {msg}");
    m.insert("menu.edit.selectall", "Seleccionar &todo\tCtrl+A");
    m.insert("menu.edit.timestamp", "Insertar marca de &tiempo\tF5");

    m.insert("menu.help", "A&yuda");
    m.insert("menu.help.update", "Buscar &actualizaciones...");
    m.insert("menu.help.about", "&Acerca de LockNote...");

    m.insert("ctx.cut", "Cortar");
    m.insert("ctx.copy", "Copiar");
    m.insert("ctx.paste", "Pegar");
    m.insert("ctx.pasteplain", "Pegar como texto plano");
    m.insert("ctx.selectall", "Seleccionar todo");
    m.insert("ctx.rename", "Renombrar pesta\u{00f1}a...");
    m.insert("ctx.close", "Cerrar pesta\u{00f1}a");

    m.insert("ctx.color", "Color de pesta\u{00f1}a");
    m.insert("color.none", "Ninguno");
    m.insert("color.blue", "Azul");
    m.insert("color.green", "Verde");
    m.insert("color.red", "Rojo");
    m.insert("color.orange", "Naranja");
    m.insert("color.purple", "Morado");
    m.insert("color.yellow", "Amarillo");
    m.insert("color.pink", "Rosa");
    m.insert("color.gray", "Gris");
    m.insert("option.off", "Desactivado");
    m.insert("option.seconds", "{n} s");
    m.insert("option.minutes", "{n} min");

    m.insert("status.stats", "{words} palabras | {chars} caracteres | {lines} l\u{00ed}neas");
    m.insert("status.saved", "Guardado");
    m.insert("status.modified", "Modificado");
    m.insert("status.encrypted", "Cifrado");

    m.insert("createpwd.title", "Crear contrase\u{00f1}a");
    m.insert("createpwd.password", "Contrase\u{00f1}a:");
    m.insert("createpwd.confirm", "Confirmar:");
    m.insert("createpwd.create", "Crear");
    m.insert("createpwd.cancel", "Cancelar");
    m.insert("createpwd.empty", "La contrase\u{00f1}a no puede estar vac\u{00ed}a.");
    m.insert("createpwd.tooshort", "La contrase\u{00f1}a es demasiado corta.");
    m.insert("createpwd.mismatch", "Las contrase\u{00f1}as no coinciden.");
    m.insert("createpwd.weak", "D\u{00e9}bil");
    m.insert("createpwd.fair", "Regular");
    m.insert("createpwd.strong", "Fuerte");
    m.insert("createpwd.verystrong", "Muy fuerte");
    m.insert("createpwd.welcome", "Bienvenido a LockNote");
    m.insert("createpwd.minlen", "Contrase\u{00f1}a (m\u{00ed}n. {} caracteres):");
    m.insert("createpwd.desc", "Sus notas est\u{00e1}n cifradas y almacenadas dentro de este ejecutable.\nElija una contrase\u{00f1}a para protegerlas.");

    m.insert("unlock.title", "Desbloquear");
    m.insert("unlock.password", "Contrase\u{00f1}a:");
    m.insert("unlock.btn", "Desbloquear");
    m.insert("unlock.cancel", "Cancelar");
    m.insert("unlock.empty", "Por favor, introduzca una contrase\u{00f1}a.");
    m.insert("unlock.decrypting", "Descifrando...");
    m.insert("unlock.maxattempts", "N\u{00fa}mero m\u{00e1}ximo de intentos alcanzado. Saliendo.");
    m.insert("unlock.wrong", "Contrase\u{00f1}a incorrecta.");
    m.insert("unlock.desc", "Introduce tu contrase\u{00f1}a para desbloquear tus notas.");
    m.insert("unlock.locked.title", "Bloqueado");
    m.insert("unlock.retryin", "Reintentar en {seconds}s\u{2026}");
    m.insert("unlock.cryptoerror", "Error criptogr\u{00e1}fico: {msg}");
    m.insert("lock.overlay.title", "LockNote est\u{00e1} bloqueado");
    m.insert("lock.curtain", "Introduzca su contrase\u{00f1}a para mostrar sus notas.");
    m.insert("lock.curtain.title", "Notas ocultas");
    m.insert("lock.curtain.waiting_for", "LockNote se bloquea en cuanto se cierre \u{00ab}{window}\u{00bb}. Introduzca su contrase\u{00f1}a para mostrar sus notas.");
    m.insert("lock.curtain.save_failed", "El guardado antes del bloqueo ha fallado: no se ha perdido nada, sus cambios se conservan. Introduzca su contrase\u{00f1}a para mostrarlos.");
    m.insert("lock.curtain.show", "Mostrar notas");
    m.insert("lock.curtain.quit_refused", "LockNote no puede cerrarse ahora: sus cambios sin guardar est\u{00e1}n detr\u{00e1}s de esta pantalla. Introduzca su contrase\u{00f1}a para mostrarlos y luego guarde o cierre.");
    m.insert("lock.save_failed", "LockNote no pudo guardar antes de bloquearse:\n{error}\n\nSus notas no se bloquearon y sus cambios sin guardar se conservan. Siguen sin guardarse: vuelva a intentarlo (Ctrl+S) o copie su texto.");
    m.insert("lock.overlay.click", "Haga clic para desbloquear");
    m.insert("unlock.show", "Mostrar contrase\u{00f1}a");
    m.insert("unlock.hide", "Ocultar contrase\u{00f1}a");
    m.insert("unlock.capslock", "Bloq May\u{00fa}s activado");

    m.insert("close.unsaved", "Tiene cambios sin guardar.");
    m.insert("close.remember", "Recordar mi elecci\u{00f3}n");
    m.insert("close.save", "Guardar");
    m.insert("close.dontsave", "No guardar");
    m.insert("close.cancel", "Cancelar");

    m.insert("search.find", "Buscar:");
    m.insert("search.replace", "Reemplazar:");
    m.insert("search.next", "Siguiente");
    m.insert("search.btn.replace", "Reemplazar");
    m.insert("search.btn.replaceall", "Reemplazar todo");
    m.insert("search.close", "Cerrar");
    m.insert("search.notfound", "No encontrado");
    m.insert("search.title", "Buscar y reemplazar");
    m.insert("search.find.title", "Buscar");
    m.insert("search.prev", "Anterior");
    m.insert("search.case", "Coincidir may\u{00fa}sculas");
    m.insert("search.wholeword", "Palabra completa");
    m.insert("search.count", "{current} / {total}");
    m.insert("search.count.total", "{total} coincidencias");
    m.insert("search.count.none", "Sin coincidencias");
    m.insert("search.replaceall.title", "Reemplazar todo");
    m.insert("search.replaceall.msg", "{count} ocurrencias reemplazadas.");

    m.insert("goto.title", "Ir a la l\u{00ed}nea");
    m.insert("goto.label", "N\u{00fa}mero de l\u{00ed}nea:");
    m.insert("goto.ok", "Aceptar");
    m.insert("goto.cancel", "Cancelar");
    m.insert("goto.invalid", "N\u{00fa}mero de l\u{00ed}nea no v\u{00e1}lido.");

    m.insert("rename.title", "Renombrar pesta\u{00f1}a");
    m.insert("rename.label", "Nombre:");
    m.insert("rename.ok", "Aceptar");
    m.insert("rename.cancel", "Cancelar");
    m.insert("rename.empty", "El nombre no puede estar vac\u{00ed}o.");

    m.insert("settings.title", "Ajustes");
    m.insert("settings.onclose", "Al cerrar:");
    m.insert("settings.ask", "Preguntar");
    m.insert("settings.always", "Guardar siempre");
    m.insert("settings.never", "No guardar nunca");
    m.insert("settings.theme", "Tema:");
    m.insert("settings.dark", "Oscuro");
    m.insert("settings.light", "Claro");
    m.insert("settings.language", "Idioma:");
    m.insert("settings.minpwdlen", "Long. m\u{00ed}n. de contrase\u{00f1}a:");
    m.insert("settings.section.appearance", "Apariencia");
    m.insert("settings.section.security", "Seguridad");
    m.insert("settings.section.behavior", "Comportamiento");
    m.insert("settings.font", "Fuente del editor:");
    m.insert("settings.ok", "Aceptar");
    m.insert("settings.cancel", "Cancelar");
    m.insert("settings.savekey", "Atajo de guardado:");
    m.insert("settings.fontsize", "Tama\u{00f1}o de fuente:");
    m.insert("settings.pbkdf2", "Iteraciones PBKDF2:");
    m.insert("settings.hint", "Pista de contrase\u{00f1}a:");
    m.insert("settings.mintray", "Minimizar a la bandeja");
    m.insert("settings.autolock", "Bloqueo autom\u{00e1}tico:");
    m.insert("settings.autosave", "Guardado autom\u{00e1}tico:");
    m.insert("settings.clipboard", "Borrado del portapapeles:");
    m.insert("settings.hotkey", "Atajo global:");
    m.insert("settings.restart", "Reinicie LockNote para aplicar el cambio de idioma.");
    m.insert("update.restart.failed", "La actualizaci\u{00f3}n no se pudo aplicar al cerrar, por lo que LockNote no se reinici\u{00f3}.\n\nInicie LockNote de nuevo.\n\nDetalles: {log}");
    m.insert("settings.2fa", "Autenticaci\u{00f3}n de dos factores");
    m.insert("settings.2fa.enable", "Activar autenticaci\u{00f3}n TOTP de dos factores");
    m.insert("settings.2fa.setup", "Configurar 2FA...");
    m.insert("settings.2fa.status.enabled", "La 2FA est\u{00e1} activada");
    m.insert("settings.2fa.status.disabled", "La 2FA est\u{00e1} desactivada");

    m.insert("security.title", "Seguridad");
    m.insert("security.changepw", "Cambiar contrase\u{00f1}a...");
    m.insert("security.section", "Seguridad");
    m.insert("preferences.title", "Preferencias");

    m.insert("update.fail", "Error al buscar actualizaciones.");
    m.insert("update.norelease", "No hay informaci\u{00f3}n de versi\u{00f3}n disponible.");
    m.insert("update.uptodate", "\u{00a1}Est\u{00e1} actualizado!");
    m.insert("update.available", "Hay una nueva versi\u{00f3}n disponible: {version}");
    m.insert("update.success", "Actualizaci\u{00f3}n descargada correctamente.");
    m.insert("update.error", "Error en la actualizaci\u{00f3}n: {error}");

    m.insert("totp.empty", "No hay claves TOTP configuradas.");
    m.insert("totp.add", "Agregar clave");
    m.insert("totp.add.title", "Agregar clave TOTP");
    m.insert("totp.add.name", "Nombre:");
    m.insert("totp.add.secret", "Secreto:");
    m.insert("totp.add.invalid", "Secreto TOTP no v\u{00e1}lido.");
    m.insert("totp.add.secret_hint", "Base32 o enlace otpauth://");
    m.insert("totp.add.generate", "Generar");
    m.insert("totp.secret.err.length", "Este secreto tiene una longitud que Base32 nunca da: falta una parte o se peg\u{00f3} dos veces. P\u{00e9}guelo de nuevo, solo.");
    m.insert("totp.secret.err.short", "Este secreto es demasiado corto (menos de 80 bits): probablemente falta una parte.");
    m.insert("totp.secret.err.doubled", "El mismo texto aparece dos veces: probablemente se peg\u{00f3} dos veces. Vac\u{00ed}e el campo y pegue el secreto una sola vez.");
    m.insert("totp.secret.err.duplicate", "Este enlace da dos veces el mismo par\u{00e1}metro (por ejemplo dos secretos): se rechaza, ya que no se puede saber cu\u{00e1}l es el correcto.");
    m.insert("totp.secret.err.empty", "Introduzca el secreto dado por el sitio.");
    m.insert("totp.secret.err.base32", "Este secreto no es Base32 v\u{00e1}lido (letras A-Z y d\u{00ed}gitos 2-7).");
    m.insert("totp.secret.err.not_totp", "Solo se admiten enlaces otpauth://totp/.");
    m.insert("totp.secret.err.no_secret", "Este enlace otpauth:// no contiene ning\u{00fa}n secreto.");
    m.insert("totp.secret.err.unsupported", "Este enlace pide c\u{00f3}digos que LockNote no genera (solo SHA1, 6 d\u{00ed}gitos, 30 segundos).");
    m.insert("totp.remove", "Eliminar");
    m.insert("totp.remove.confirm", "\u{00bf}Eliminar esta clave TOTP?");
    m.insert("totp.copied", "C\u{00f3}digo copiado al portapapeles.");
    m.insert("clipboard.failed", "Error al copiar: {err}");
    m.insert("totp.secret.copied", "Secreto copiado.");
    m.insert("totp.select_first", "Seleccione una entrada primero.");
    m.insert("totp.added", "Entrada a\u{00f1}adida.");
    m.insert("totp.removed", "Entrada eliminada.");
    m.insert("totp.remove.title", "Eliminar entrada TOTP");
    m.insert("totp.remove.msg", "\u{00bf}Eliminar \u{00ab}{name}\u{00bb}? Esta acci\u{00f3}n no se puede deshacer.");
    m.insert("totp.add.prompt", "Nombre de la entrada:");

    // TOTP Manager
    m.insert("totp.manager.copy", "Copiar c\u{00f3}digo");

    // 2FA / TOTP Verify
    m.insert("2fa.title", "LockNote \u{2014} Autenticaci\u{00f3}n de dos factores");
    m.insert("2fa.header", "Autenticaci\u{00f3}n de dos factores");
    m.insert("2fa.prompt", "Introduce el c\u{00f3}digo de 6 d\u{00ed}gitos de tu aplicaci\u{00f3}n de autenticaci\u{00f3}n.");
    m.insert("2fa.verify", "Verificar");
    m.insert("2fa.cancel", "Cancelar");
    m.insert("2fa.enter6", "Introduce un c\u{00f3}digo de 6 d\u{00ed}gitos.");
    m.insert("2fa.maxattempts", "N\u{00fa}mero m\u{00e1}ximo de intentos alcanzado.");
    m.insert("2fa.failed", "Fallo de 2FA");
    m.insert("2fa.setup.title", "Configuraci\u{00f3}n TOTP");
    m.insert("2fa.setup.secret", "Secreto Base32:");
    m.insert("2fa.setup.copy", "Copiar");
    m.insert("2fa.setup.uri", "URI OtpAuth:");
    m.insert("2fa.setup.verify", "Introduce un c\u{00f3}digo de tu autenticador para verificar:");
    m.insert("2fa.setup.confirm", "Confirmar");
    m.insert("2fa.setup.verified", "\u{00a1}Verificado!");
    m.insert("2fa.setup.invalid", "C\u{00f3}digo inv\u{00e1}lido, int\u{00e9}ntalo de nuevo");
    m.insert("2fa.pwchanged", "Contrase\u{00f1}a cambiada \u{2713}");
    m.insert("2fa.retryin", "Reintentar en {seconds}s\u{2026}");

    // Vault / Secret edit
    m.insert("vault.edit", "Editar secreto");
    m.insert("vault.new", "Nuevo secreto");
    m.insert("vault.name", "Nombre:");
    m.insert("vault.category", "Categor\u{00ed}a:");
    m.insert("vault.value", "Valor:");
    m.insert("vault.show", "Mostrar");
    m.insert("vault.name.empty", "El nombre no puede estar vac\u{00ed}o");
    m.insert("vault.btn.add", "A\u{00f1}adir");
    m.insert("vault.btn.edit", "Editar");
    m.insert("vault.btn.del", "Elim.");
    m.insert("vault.btn.copy", "Copiar");
    m.insert("vault.btn.copied", "\u{00a1}Copiado!");
    m.insert("vault.copied.named", "Copiado: {name}");
    // SSH keys (issue 054)
    m.insert("menu.tools.ssh", "Claves &SSH");
    m.insert("menu.tools.ssh_generate", "&Generar clave SSH...");
    m.insert("menu.tools.ssh_agent", "&Cargar en ssh-agent...");
    // Password generator
    m.insert("menu.tools.passgen", "&Generador de contrase\u{00f1}as...");
    m.insert("vault.generate", "Generar...");
    m.insert("passgen.title", "Generador de contrase\u{00f1}as");
    m.insert("passgen.password", "Contrase\u{00f1}a:");
    m.insert("passgen.show", "Mostrar");
    m.insert("passgen.hide", "Ocultar");
    m.insert("passgen.generate", "Generar");
    m.insert("passgen.length", "Longitud ({min}-{max}):");
    m.insert("passgen.upper", "May\u{00fa}sculas (A-Z)");
    m.insert("passgen.lower", "Min\u{00fa}sculas (a-z)");
    m.insert("passgen.digits", "D\u{00ed}gitos (0-9)");
    m.insert("passgen.symbols", "S\u{00ed}mbolos");
    m.insert("passgen.symbol_set", "S\u{00ed}mbolos permitidos:");
    m.insert("passgen.exclude_ambiguous", "Excluir caracteres ambiguos ({chars})");
    m.insert("passgen.use", "Usar");
    m.insert("passgen.close", "Cerrar");
    m.insert("passgen.err.length", "La longitud debe estar entre {min} y {max}.");
    m.insert("passgen.err.no_class", "Seleccione al menos un tipo de car\u{00e1}cter.");
    m.insert("passgen.err.not_a_symbol", "\u{00ab}{char}\u{00bb} no es un s\u{00ed}mbolo.");
    m.insert("passgen.err.no_symbol", "No queda ning\u{00fa}n s\u{00ed}mbolo: a\u{00f1}ada s\u{00ed}mbolos o desmarque S\u{00ed}mbolos.");
    m.insert("ssh.title", "Claves SSH");
    m.insert("ssh.gen.title", "Generar clave SSH");
    m.insert("ssh.gen.type", "Tipo:");
    m.insert("ssh.gen.recommended", "recomendado");
    m.insert("ssh.gen.name", "Nombre en el Vault:");
    m.insert("ssh.gen.comment", "Comentario:");
    m.insert("ssh.gen.passphrase", "Frase de contrase\u{00f1}a (opcional):");
    m.insert("ssh.gen.confirm", "Confirmar frase de contrase\u{00f1}a:");
    m.insert("ssh.gen.generate", "&Generar");
    m.insert("ssh.gen.generating", "Generando...");
    m.insert("ssh.gen.fingerprint", "Huella:");
    m.insert("ssh.gen.public", "Clave p\u{00fa}blica:");
    m.insert("ssh.gen.copy_public", "Copiar clave p\u{00fa}blica");
    m.insert("ssh.gen.copied", "Clave p\u{00fa}blica copiada.");
    m.insert("ssh.gen.save_vault", "Guardar en el Vault");
    m.insert("ssh.gen.export", "Exportar...");
    m.insert("ssh.gen.export_title", "Exportar clave SSH");
    m.insert("ssh.gen.close", "Cerrar");
    m.insert("ssh.gen.mismatch", "Las frases de contrase\u{00f1}a no coinciden.");
    m.insert("ssh.gen.no_passphrase_export", "Esta clave privada no tiene frase de contrase\u{00f1}a: cualquiera que pueda leer el archivo puede usarla. \u{00bf}Exportar de todos modos?");
    m.insert("ssh.gen.exported", "Exportada a {path} (+ .pub).");
    m.insert("ssh.agent.title", "Cargar en ssh-agent");
    m.insert("ssh.agent.key", "Clave:");
    m.insert("ssh.agent.passphrase", "Frase de contrase\u{00f1}a (si la clave tiene una):");
    m.insert("ssh.agent.warning", "La clave se entregar\u{00e1} al agente SSH de Windows, que puede conservarla fuera de LockNote. Se retira al bloquear o cerrar LockNote. Tras un cierre anormal, se retira la pr\u{00f3}xima vez que se desbloquee este archivo. La misma clave a\u{00f1}adida a mano con ssh-add tambi\u{00e9}n se retira. Cargar la clave tambi\u{00e9}n guarda este archivo.");
    m.insert("ssh.agent.load", "&Cargar");
    m.insert("ssh.agent.none", "No hay ninguna clave privada SSH en el Vault. Cree una con Herramientas > Claves SSH > Generar clave SSH.");
    m.insert("ssh.agent.loaded", "Clave cargada en ssh-agent:\n{fingerprint}");
    m.insert("ssh.agent.save_failed", "La clave se carg\u{00f3}, pero este archivo no se pudo guardar, as\u{00ed} que la clave se retir\u{00f3} de ssh-agent.\n{detail}");
    m.insert("ssh.agent.unload_failed", "Algunas claves SSH no se pudieron retirar de ssh-agent. Siguen en la lista y LockNote lo intentar\u{00e1} de nuevo en el pr\u{00f3}ximo desbloqueo.\n{detail}");
    m.insert("ssh.err.invalid_comment", "El comentario debe caber en una l\u{00ed}nea, sin tabulaciones ni caracteres de control.");
    m.insert("ssh.err.duplicate", "Ya existe un secreto llamado \"{name}\" en el Vault.");
    m.insert("ssh.err.passphrase_required", "Esta clave est\u{00e1} protegida: introduzca su frase de contrase\u{00f1}a.");
    m.insert("ssh.err.wrong_passphrase", "Frase de contrase\u{00f1}a incorrecta.");
    m.insert("ssh.err.agent_unavailable", "El servicio ssh-agent de Windows no se est\u{00e1} ejecutando.\nEn un PowerShell de administrador:\nGet-Service ssh-agent | Set-Service -StartupType Automatic\nStart-Service ssh-agent");
    m.insert("ssh.err.agent_failure", "ssh-agent rechaz\u{00f3} la operaci\u{00f3}n.");
    m.insert("ssh.err.agent_timeout", "ssh-agent no respondi\u{00f3} a tiempo.");
    m.insert("ssh.err.other", "Error: {detail}");
    m.insert("ssh.agent.save_failed_still_loaded", "No se pudo guardar este archivo y la clave sigue en ssh-agent. LockNote intentar\u{00e1} retirarla al bloquear o cerrar este archivo.\n{detail}");
    m.insert("ssh.err.invalid_name", "El nombre no debe contener [ ] ni caracteres de control.");
    m.insert("vault.copy.failed", "Error al copiar");
    m.insert("vault.btn.reveal", "Ver");
    m.insert("vault.tree.error", "No se puede mostrar la bóveda");
    m.insert("vault.search", "Buscar un nombre");
    m.insert("vault.category.none", "Elija una categor\u{00ed}a");
    m.insert("vault.cat.menu.new", "Nueva categor\u{00ed}a\u{2026}");
    m.insert("vault.cat.menu.rename", "Renombrar\u{2026}");
    m.insert("vault.cat.menu.delete", "Eliminar");
    m.insert("vault.cat.new.title", "Nueva categor\u{00ed}a");
    m.insert("vault.cat.rename.title", "Renombrar categor\u{00ed}a");
    m.insert("vault.cat.name", "Nombre de la categor\u{00ed}a:");
    m.insert("vault.cat.delete.title", "Eliminar categor\u{00ed}a");
    m.insert("vault.cat.delete.confirm", "\u{00BF}Eliminar la categor\u{00ed}a \u{00AB}{name}\u{00BB}? Sus {count} secreto(s) pasar\u{00e1}n a General.");
    m.insert("vault.cat.error.title", "Categor\u{00ed}a");
    m.insert("vault.cat.err.empty", "El nombre no puede estar vac\u{00ed}o.");
    m.insert("vault.cat.err.toolong", "El nombre es demasiado largo ({max} caracteres m\u{00e1}x.).");
    m.insert("vault.cat.err.char", "El car\u{00e1}cter \u{00AB}{char}\u{00BB} no est\u{00e1} permitido.");
    m.insert("vault.cat.err.duplicate", "Ya existe una categor\u{00ed}a \u{00AB}{name}\u{00BB}.");
    m.insert("vault.cat.err.toomany", "Demasiadas categor\u{00ed}as ({max} m\u{00e1}x.).");
    m.insert("vault.cat.err.general", "La categor\u{00ed}a General no se puede renombrar ni eliminar.");
    m.insert("vault.cat.err.notfound", "Categor\u{00ed}a no encontrada: {name}");
    m.insert("vault.cat.err.compare", "Fall\u{00f3} la comparaci\u{00f3}n de nombres: {msg}");

    // Menu bar toggle buttons
    m.insert("menu.toggle.vault", "B\u{00f3}veda");
    m.insert("menu.toggle.totp", "TOTP");
    m.insert("menu.toggle.attach", "Adjuntos");
    m.insert("menu.toggle.lock", "\u{1F512} Bloquear");

    // Attachments
    m.insert("menu.tools.attachments", "&Adjuntos\tCtrl+J");
    // Prompts panel (issue 058)
    m.insert("menu.toggle.prompts", "Prompts");
    m.insert("menu.tools.prompts", "&Prompts");
    m.insert("palette.prompt", "Prompt: {name}");
    m.insert("prompts.btn.insert", "Insertar");
    m.insert("prompts.inserted.named", "Insertado: {name}");
    m.insert("palette.title", "Paleta de comandos");
    m.insert("palette.tab", "Pesta\u{00f1}a: {name}");
    m.insert("palette.vault", "B\u{00f3}veda: {name}");
    m.insert("prompts.search", "Buscar (nombre o texto)");
    m.insert("prompts.btn.add", "A\u{00f1}adir");
    m.insert("prompts.btn.edit", "Editar");
    m.insert("prompts.btn.del", "Elim.");
    m.insert("prompts.btn.copy", "Copiar");
    m.insert("prompts.copied.named", "Copiado: {name}");
    m.insert("prompts.copy.failed", "Error al copiar: {err}");
    m.insert("prompts.count", "{count} prompt(s)");
    m.insert("prompts.count.filtered", "{shown} de {total} prompt(s)");
    m.insert("prompts.list.error", "No se puede mostrar la lista: {msg}");
    m.insert("prompts.new", "Nuevo prompt");
    m.insert("prompts.edit", "Editar prompt");
    m.insert("prompts.name", "Nombre:");
    m.insert("prompts.category", "Categor\u{00ed}a:");
    m.insert("prompts.text", "Texto:");
    m.insert("prompts.category.default", "General");
    m.insert("prompts.err.name", "El nombre no puede estar vac\u{00ed}o");
    m.insert("prompts.err.category", "La categor\u{00ed}a no puede estar vac\u{00ed}a");
    m.insert("prompts.err.text", "El texto no puede estar vac\u{00ed}o");
    m.insert("prompts.err.toolong", "Texto demasiado largo (m\u{00e1}x. {max} MB)");
    m.insert("prompts.err.name.char", "El nombre no puede contener {char}");
    m.insert("prompts.err.category.char", "La categor\u{00ed}a no puede contener {char}");
    m.insert("prompts.delete.title", "Eliminar prompt");
    m.insert("prompts.delete.confirm", "\u{00bf}Eliminar el prompt \u{00ab}{name}\u{00bb}?");
    m.insert("prompts.gone", "Este prompt cambi\u{00f3} o desapareci\u{00f3} mientras tanto (\u{00bf}importaci\u{00f3}n?): no se guard\u{00f3} nada.");
    m.insert("vault.edit.gone", "Este secreto cambi\u{00f3} o desapareci\u{00f3} mientras tanto (\u{00bf}importaci\u{00f3}n?): no se guard\u{00f3} nada.");
    m.insert("attach.btn.add", "A\u{00f1}adir");
    m.insert("attach.btn.open", "Abrir");
    m.insert("attach.btn.save", "Guard.");
    m.insert("attach.btn.del", "Elim.");
    m.insert("attach.error.read", "No se pudo leer el archivo: {}");
    m.insert("attach.error.corrupt", "Adjunto corrupto (SHA-256 inv\u{00e1}lido).");
    m.insert("attach.error.write_temp", "Error al escribir archivo temporal: {}");
    m.insert("attach.error.save", "No se pudo guardar el archivo: {}");
    m.insert("attach.risky.title", "\u{00bf}Abrir adjunto?");
    m.insert("attach.risky.body", "Este adjunto es un archivo ejecutable. Abrirlo ejecutar\u{00e1} c\u{00f3}digo en su ordenador. Contin\u{00fa}e solo si conf\u{00ed}a en la fuente de este cofre.\n\n\u{00bf}Abrir de todos modos?");
    m.insert("attach.confirm.delete", "\u{00bf}Eliminar adjunto \"{}\"?");
    m.insert("attach.confirm.title", "Confirmar");
    m.insert("attach.save.title", "Guardar \"{}\" como");
    m.insert("attach.add.title", "Adjuntar archivo");

    // Esteganograf\u{00ed}a
    m.insert("menu.tools.stego", "Este&ganograf\u{00ed}a");
    m.insert("menu.tools.stego_hide", "&Ocultar en imagen...");
    m.insert("menu.tools.stego_extract", "E&xtraer de imagen...");
    m.insert("stego.confirm_action", "exportaci\u{00f3}n esteganogr\u{00e1}fica");
    m.insert("stego.dlg.open_carrier", "Seleccionar imagen portadora");
    m.insert("stego.dlg.save", "Guardar imagen esteganogr\u{00e1}fica");
    m.insert("stego.dlg.open_extract", "Seleccionar imagen a extraer");
    m.insert("stego.title", "Esteganograf\u{00ed}a");
    m.insert("stego.success", "Contenido ocultado correctamente.");
    m.insert("stego.size", "Tama\u{00f1}o");
    m.insert("stego.warning_recompress", "No compartir v\u{00ed}a WhatsApp, Instagram, etc.\nLa recompresi\u{00f3}n destruye los datos ocultos.");
    m.insert("stego.extracted", "Notas ocultas extra\u{00ed}das con \u{00e9}xito.");
    m.insert("stego.nodata_title", "Sin datos ocultos");
    m.insert("stego.nodata", "No se detectaron datos ocultos en esta imagen.");
    m.insert("stego.error", "Error");
    m.insert("stego.error.read_image", "No se pudo leer la imagen");
    m.insert("stego.error.format_title", "Formato no compatible");
    m.insert("stego.error.format", "Formato no compatible (JPEG, PNG, BMP).");
    m.insert("stego.error.write", "No se pudo escribir el archivo");
    m.insert("stego.error.toolarge_title", "Datos demasiado grandes");
    m.insert("stego.error.toolarge", "Notas demasiado grandes para esta imagen.");
    m.insert("stego.error.toolarge_hint", "Elija una imagen portadora m\u{00e1}s grande.");
    m.insert("stego.error.embed", "No se pudieron ocultar los datos");
    m.insert("stego.error.encrypt", "Fall\u{00f3} el cifrado durante la exportaci\u{00f3}n de imagen: {msg}");
    m.insert("stego.error.decrypt_title", "Extracci\u{00f3}n fallida");
    m.insert("stego.error.decrypt", "Contrase\u{00f1}a incorrecta o datos no LockNote.");
    m.insert("stego.error.extract", "No se pudieron extraer los datos");
    m.insert("stego.pw.title_hide", "Contrase\u{00f1}a de la imagen");
    m.insert("stego.pw.title_extract", "Contrase\u{00f1}a de la imagen");
    m.insert("stego.pw.prompt_hide", "Contrase\u{00f1}a para proteger esta imagen:");
    m.insert("stego.pw.prompt_extract", "Contrase\u{00f1}a usada para ocultar los datos:");
    m.insert("stego.pw.confirm", "Confirmar contrase\u{00f1}a:");
    m.insert("stego.pw.empty", "La contrase\u{00f1}a no puede estar vac\u{00ed}a.");
    m.insert("stego.pw.mismatch", "Las contrase\u{00f1}as no coinciden.");

    // Snapshot

    // History

    m.insert("export.prompt", "\u{00bf}Exportar nota como texto plano?");
    m.insert("export.btn", "Exportar");
    m.insert("export.warning", "El archivo exportado NO estar\u{00e1} cifrado.");
    m.insert("export.dlg.title", "Exportar nota");
    m.insert("export.dlg.filter", "Archivos de texto (*.txt)|*.txt|Todos los archivos (*.*)|*.*");
    m.insert("export.fail", "Error al exportar el archivo.");

    // Export encoding
    m.insert("export.encoding.title", "Codificaci\u{00f3}n de exportaci\u{00f3}n");
    m.insert("export.encoding.label", "Elegir codificaci\u{00f3}n para exportar:");

    // Drag & drop
    m.insert("drop.binary", "Este tipo de archivo no es compatible. LockNote solo puede importar archivos de texto.");
    m.insert("drop.invalid", "Este archivo no se pudo leer como texto. Puede ser un archivo binario.");
    m.insert("drop.choice.title", "Archivo soltado");
    m.insert("drop.choice.text", "\u{00bf}C\u{00f3}mo desea tratar este archivo de texto?");
    m.insert("drop.choice.binary", "Este archivo puede a\u{00f1}adirse como adjunto.");
    m.insert("drop.choice.insert", "Insertar texto");
    m.insert("drop.choice.attach", "A\u{00f1}adir como adjunto");
    m.insert("drop.choice.cancel", "Cancelar");
    m.insert("drop.directory", "No se pueden soltar carpetas en LockNote. Suelta archivos individuales.");

    m.insert("save.error.title", "Error al guardar");
    m.insert("save.error.msg", "Error al guardar: {}");
    m.insert("export.success.title", "Exportaci\u{00f3}n");
    m.insert("export.success.msg", "Nota exportada a:\n{}");
    m.insert("export.error.title", "Error de exportaci\u{00f3}n");
    m.insert("export.error.msg", "Error al exportar: {}");
    m.insert("export.warning_title", "Advertencia de exportaci\u{00f3}n");
    m.insert("export.warning_lossy", "Algunos caracteres no se pueden representar en esta codificaci\u{00f3}n y se han reemplazado. \u{00bf}Continuar?");
    m.insert("menu.file.backup_export", "Exportar copia completa...");
    m.insert("menu.file.backup_import", "Importar copia completa...");
    m.insert("backup.export.save_title", "Guardar copia completa");
    m.insert("backup.filter", "Copia de LockNote (*.lnbak)|Todos los archivos (*.*)");
    m.insert("backup.export.done", "Copia completa guardada en:\n{path}\n\nContiene todas las pesta\u{00f1}as, instant\u{00e1}neas, la b\u{00f3}veda, los adjuntos y los ajustes, cifrados con la contrase\u{00f1}a de esta nota.");
    m.insert("backup.export.overwrite", "{path}\n\nEste archivo ya existe. \u{00bf}Reemplazarlo?");
    m.insert("backup.export.error", "Error en la copia: {msg}");
    m.insert("backup.import.open_title", "Importar copia completa");
    m.insert("backup.import.pw_title", "Contrase\u{00f1}a de la copia");
    m.insert("backup.import.pw_label", "Contrase\u{00f1}a de esta copia:");
    m.insert("backup.import.wrong_password", "Contrase\u{00f1}a incorrecta o archivo da\u{00f1}ado.");
    m.insert("backup.import.not_backup", "Este archivo no es una copia de LockNote.");
    m.insert("backup.import.newer", "Esta copia procede de una versi\u{00f3}n m\u{00e1}s reciente de LockNote (formato {found}; esta versi\u{00f3}n lee hasta el formato {supported}). Actualice LockNote.");
    m.insert("backup.import.corrupt", "Esta copia est\u{00e1} da\u{00f1}ada: {msg}");
    m.insert("backup.import.read_error", "No se puede leer el archivo: {msg}");
    m.insert("backup.import.too_large", "Este archivo es demasiado grande para ser una copia.");
    m.insert("backup.import.confirm", "\u{00bf}Reemplazar TODO el contenido actual por esta copia?\n\nCopia creada el {date}:\n- {tabs} pesta\u{00f1}a(s)\n- {prompts} prompt(s)\n- {secrets} secreto(s) de la b\u{00f3}veda\n- {attachments} adjunto(s)\n\nSe reemplazar\u{00e1}n sus pesta\u{00f1}as, prompts, b\u{00f3}veda, adjuntos y ajustes actuales (incluida la autenticaci\u{00f3}n en dos pasos). No se puede deshacer una vez guardada la nota.");
    m.insert("backup.import.done", "Copia importada. Guarde para conservarla. El ajuste de idioma se aplica tras guardar y reiniciar LockNote.");
    m.insert("autobackup.failed.import", "No se pudo escribir la copia de seguridad, as\u{00ed} que no se import\u{00f3} nada.\n\n{msg}");
    m.insert("autobackup.failed.update", "No se pudo escribir la copia de seguridad, as\u{00ed} que no se descarg\u{00f3} la actualizaci\u{00f3}n.\n\n{msg}");
    m.insert("autobackup.purge.ask", "{n} copia(s) autom\u{00e1}tica(s) se hicieron con su contrase\u{00f1}a anterior y se pueden abrir con ella.\n\n\u{00bf}Eliminarlas ahora?");
    m.insert("autobackup.purge.partial", "No se pudieron eliminar {n} copia(s) autom\u{00e1}tica(s). Elim\u{00ed}nelas a mano en:\n{dir}");
    m.insert("menu.file.backups", "Copias de &seguridad...");
    m.insert("autobackup.dlg.title", "Copias de seguridad");
    m.insert("autobackup.dlg.list", "Copias (la m\u{00e1}s reciente primero):");
    m.insert("autobackup.dlg.empty", "A\u{00fa}n no hay copias de seguridad.");
    m.insert("autobackup.dlg.details", "{kind}\n{date} (UTC)\n{size}\n\nSe abre con la contrase\u{00f1}a que ten\u{00ed}a en ese momento.");
    m.insert("autobackup.dlg.restore", "Restaurar");
    m.insert("autobackup.dlg.create", "Crear copia ahora");
    m.insert("totp.nextcode", "Pr\u{00f3}ximo c\u{00f3}digo en {n} s");
    m.insert("autobackup.dlg.delete", "Eliminar");
    m.insert("autobackup.dlg.folder", "Abrir carpeta");
    m.insert("autobackup.dlg.close", "Cerrar");
    m.insert("autobackup.dlg.delete.title", "Eliminar copia");
    m.insert("autobackup.dlg.delete.confirm", "\u{00bf}Eliminar definitivamente esta copia?");
    m.insert("autobackup.dlg.delete.failed", "No se pudo eliminar esta copia: {msg}");
    m.insert("autobackup.dlg.folder.failed", "No se pudo abrir la carpeta: {msg}");
    m.insert("autobackup.dlg.list_failed", "No se pudo leer la carpeta de copias: {msg}");
    m.insert("autobackup.kind.pre_import", "Antes de importar");
    m.insert("autobackup.kind.pre_update", "Antes de actualizar");
    m.insert("autobackup.kind.manual", "Manual");
    m.insert("autobackup.manual.done", "Copia creada.");
    m.insert("autobackup.manual.failed", "No se pudo escribir la copia.\n\n{msg}");
    m.insert("menu.file.import_exe", "Importar desde otro LockNote...");
    m.insert("import_exe.title", "Importar desde otro LockNote");
    m.insert("import_exe.open_title", "Elegir el archivo LockNote que se va a importar");
    m.insert("import_exe.filter", "LockNote (*.exe)|Todos los archivos (*.*)");
    m.insert("import_exe.same_file", "Este es el archivo que est\u{00e1} usando. Elija su archivo LockNote anterior.");
    // Startup conflict between the pending save and a more recent program file (issue 055)
    m.insert("conflict.title", "LockNote - Dos versiones de sus notas");
    m.insert("conflict.body", "LockNote ha encontrado dos versiones diferentes de sus notas.\n\nGuardado pendiente, a\u{00fa}n no escrito en el archivo del programa:\n{tmp_date} (UTC)\n\nArchivo del programa, que contiene otra versi\u{00f3}n, \u{00fa}ltima modificaci\u{00f3}n:\n{exe_date} (UTC)\n{exe}\n\nS\u{00ed}: conservar el guardado pendiente.\nNo: conservar las notas del archivo del programa.\nCancelar: salir sin modificar nada.\n\nLa versi\u{00f3}n que no conserve no se elimina. Se mueve a:\n{dir}");
    m.insert("conflict.exe_empty", "Nota: el archivo del programa no contiene ninguna nota.");
    m.insert("conflict.failed", "No se pudo aplicar su elecci\u{00f3}n. No se ha eliminado nada.\n\n{msg}\n\nLockNote se cerrar\u{00e1}.");
    m.insert("pending.failed", "LockNote no puede abrir sus notas de forma segura. No se ha modificado nada.\n\n{msg}\n\nLockNote se cerrar\u{00e1}.");
    m.insert("import_exe.no_data", "No hay datos de LockNote en este archivo.");
    m.insert("import_exe.pw_title", "Contrase\u{00f1}a del archivo importado");
    m.insert("import_exe.pw_label", "Contrase\u{00f1}a del archivo LockNote que importa:");
    m.insert("import_exe.confirm", "\u{00bf}Reemplazar TODO el contenido actual por los datos de este archivo LockNote?\n\nContiene:\n- {tabs} pesta\u{00f1}a(s)\n- {prompts} prompt(s)\n- {secrets} secreto(s) de la b\u{00f3}veda\n- {attachments} adjunto(s)\n\nSe reemplazar\u{00e1}n sus pesta\u{00f1}as, prompts, b\u{00f3}veda, adjuntos y ajustes actuales (incluida la autenticaci\u{00f3}n en dos pasos). El archivo importado no se modifica.");
    m.insert("import_exe.done", "Datos importados. Guarde para conservarlos: quedar\u{00e1}n protegidos por la contrase\u{00f1}a de ESTE archivo. El ajuste de idioma se aplica tras guardar y reiniciar LockNote.");

    m.insert("tray.restore", "&Restaurar");
    m.insert("tray.quit", "&Salir");

    m.insert("about.title", "Acerca de LockNote");
    m.insert("about.desc", "Bloc de notas cifrado aut\u{00f3}nomo para Windows");

    m.insert("confirmpwd.title", "Confirmar contrase\u{00f1}a");
    m.insert("confirmpwd.export", "Introduzca su contrase\u{00f1}a para exportar:");
    m.insert("confirmpwd.print", "Introduzca su contrase\u{00f1}a para imprimir:");
    m.insert("about.version", "Versi\u{00f3}n {version}");
    m.insert("totp.setup.for", "Configurando TOTP para: {name}");
    m.insert("2fa.invalid.left", "C\u{00f3}digo no v\u{00e1}lido. Intentos restantes: {n}.");
    m.insert("attach.error.title", "Error");
    m.insert("attach.limit.title", "Archivo adjunto");
    m.insert("attach.error.toolarge", "Archivo demasiado grande: {size} (m\u{00e1}x. {max})");
    m.insert("attach.error.totaltoolarge", "Total de adjuntos demasiado grande: {size} (m\u{00e1}x. {max})");
    m.insert("status.word", "{n} palabra");
    m.insert("status.words", "{n} palabras");
    m.insert("status.char", "{n} car\u{00e1}cter");
    m.insert("status.chars", "{n} caracteres");
    m.insert("status.line", "{n} l\u{00ed}nea");
    m.insert("status.lines", "{n} l\u{00ed}neas");
    m.insert("status.caret", "L\u{00ed}n. {line}, col. {col}");
    m.insert("status.selected.one", "{n} seleccionado");
    m.insert("status.selected.many", "{n} seleccionados");
    m.insert("save.error.encrypt", "Error de cifrado: {}");
    m.insert("export.success.encoding", "Nota exportada como {encoding} en:\n{path}");
    m.insert("filter.text", "Archivos de texto (*.txt)");
    m.insert("filter.all", "Todos los archivos (*.*)");
    m.insert("filter.images", "Im\u{00e1}genes (*.jpg;*.jpeg;*.png;*.bmp)");
    m.insert("filter.image", "Imagen (*.{ext})");
    m.insert("update.available.title", "Actualizaci\u{00f3}n disponible");
    m.insert("update.available.ask", "Hay una nueva versi\u{00f3}n ({version}) disponible.\n\u{00bf}Desea descargarla?");
    m.insert("update.downloaded.title", "Actualizaci\u{00f3}n descargada");
    m.insert("update.downloaded.ask", "Actualizaci\u{00f3}n descargada correctamente.\n\n\u{00bf}Reiniciar LockNote ahora para aplicarla?");
    m.insert("update.error.title", "Error de actualizaci\u{00f3}n");
    m.insert("update.uptodate.title", "Actualizado");
    m.insert("update.uptodate.msg", "Est\u{00e1} usando la \u{00fa}ltima versi\u{00f3}n ({version}).");
    m.insert("update.checkfailed.title", "Error al buscar actualizaciones");
    m.insert("update.checkfailed.msg", "No se pudieron buscar actualizaciones: {error}");
    m.insert("vault.delete.title", "Eliminar secreto");
    m.insert("vault.delete.msg", "\u{00bf}Seguro que desea eliminar este secreto?");
    m.insert("filter.ext", "Archivos {EXT} (*.{ext})");
    m.insert("close.title", "Cambios sin guardar");
    m.insert("close.msg", "Cambios sin guardar. \u{00bf}Guardar antes de cerrar?");
    m.insert("close.save_failed", "LockNote no pudo guardar:\n{error}\n\nSus cambios sin guardar siguen en la ventana. Mantenga LockNote abierto para reintentar o copiar su texto; si cierra sin guardar, se perder\u{00e1}n.");
    m.insert("close.keep_open", "Mantener LockNote abierto");
    m.insert("close.close_unsaved", "Cerrar sin guardar");
    m.insert("close.stuck", "LockNote a\u{00fa}n no puede cerrarse: espera a que se cierre \u{00ab}{window}\u{00bb}.\n\nSiga esperando, o cierre sin guardar: entonces se perder\u{00e1}n los cambios sin guardar.");
    m.insert("close.keep_waiting", "Seguir esperando");
    m.insert("endsession.blocked", "LockNote no pudo guardar sus notas ({error}). Cancele y luego guarde o copie su texto.");
    m.insert("close.remember_not_saved", "No se ha recordado su elecci\u{00f3}n: LockNote guarda sus ajustes dentro de la nota, y la nota no se guarda. Para no ver m\u{00e1}s esta pregunta, elija \u{00ab}{onclose} {never}\u{00bb} en {menu} y guarde.");

    m
}

// ---------------------------------------------------------------------------
// German translations
// ---------------------------------------------------------------------------

fn build_de() -> Dict {
    let mut m = HashMap::new();

    m.insert("app.title", "LockNote");
    m.insert("app.title.modified", "LockNote *");

    m.insert("menu.file", "&Datei");
    m.insert("menu.file.save", "&Speichern\tStrg+S");
    m.insert("menu.file.changepass", "&Passwort \u{00e4}ndern...");
    m.insert("menu.file.settings", "&Einstellungen...");
    m.insert("menu.file.quit", "&Beenden\tAlt+F4");
    m.insert("menu.file.print", "&Drucken...\tStrg+P");
    m.insert("print.footer", "Seite {n} / {m}");
    m.insert("print.error.title", "Drucken");
    m.insert("print.error.failed", "Drucken fehlgeschlagen: {error}");
    m.insert("print.error.too_small", "Der druckbare Bereich der Seite ist f\u{00fc}r die Editor-Schriftart zu klein.");
    m.insert("menu.file.export", "E&xportieren...");

    m.insert("menu.view", "&Ansicht");
    m.insert("menu.view.ontop", "Immer im &Vordergrund");
    m.insert("menu.view.wordwrap", "&Zeilenumbruch");
    m.insert("menu.view.linenumbers", "Zeilen&nummern");
    m.insert("settings.font.failed", "Die neue Editor-Schriftart konnte nicht angewendet werden; der Editor beh\u{00e4}lt seine aktuelle Schriftart.\n\nDetails: {detail}");
    m.insert("menu.view.vault", "&Tresor-Manager...");

    m.insert("menu.tab", "&Tab");
    m.insert("menu.tab.new", "&Neuer Tab\tStrg+T");
    m.insert("menu.tab.close", "Tab &schlie\u{00df}en\tStrg+W");
    m.insert("menu.tab.rename", "Tab &umbenennen...\tF2");
    m.insert("menu.tab.next", "N\u{00e4}chster Tab\tStrg+Tab");
    m.insert("menu.tab.prev", "Vorheriger Tab\tStrg+Umschalt+Tab");
    m.insert("menu.tab.compare", "Mit letzter Speicherung &vergleichen...");
    m.insert("compare.title", "Mit letzter Speicherung vergleichen");
    m.insert("compare.identical", "Keine \u{00c4}nderungen seit der letzten Speicherung.");
    m.insert("compare.too_large", "Dieser Tab ist zu lang f\u{00fc}r einen Vergleich (mehr als {} Zeilen auf einer Seite).");
    m.insert("compare.failed", "Der Vergleich konnte nicht berechnet werden.");
    m.insert("compare.close", "Schlie\u{00df}en");
    m.insert("menu.edit.goto_heading", "Gehe zu \u{00dc}&berschrift...\tStrg+Umschalt+O");
    m.insert("heading.title", "Gehe zu \u{00dc}berschrift");
    m.insert("heading.filter", "Filter:");
    m.insert("heading.none", "Keine \u{00dc}berschrift in diesem Tab.");
    m.insert("heading.no_match", "Keine \u{00dc}berschrift passt zum Filter.");
    m.insert("tab.limit.title", "Tab-Limit");
    m.insert("tab.limit.msg", "Maximale Anzahl an Tabs erreicht (20).");
    m.insert("tab.close.title", "Tab schlie\u{00df}en");
    m.insert("tab.close.msg", "Tab \u{201e}{name}\u{201c} schlie\u{00df}en?\nDer Inhalt geht verloren.");
    m.insert("tab.ctx.rename", "Tab umbenennen...");
    m.insert("tab.ctx.color", "Farbe");
    m.insert("tab.ctx.no_color", "Keine Farbe");
    m.insert("tab.ctx.close", "Tab schlie\u{00df}en");

    // Tools menu
    m.insert("menu.tools", "&Werkzeuge");
    m.insert("menu.tools.changepass", "&Passwort \u{00e4}ndern...");
    m.insert("menu.tools.totp", "&TOTP-Manager");

    m.insert("menu.settings.preferences", "&Einstellungen...");
    m.insert("menu.settings.security", "&Sicherheit...");
    m.insert("menu.settings.tempfiles", "&Tempor\u{00e4}re Dateien...");

    m.insert("status.path_copied", "Pfad kopiert");
    m.insert("status.path_copy_failed", "Pfad konnte nicht kopiert werden");
    m.insert("tempfiles.owner.current", "Diese LockNote-Datei");
    m.insert("tempfiles.owner.present", "Eine andere LockNote-Datei");
    m.insert("tempfiles.owner.orphan", "Verwaist: ihre LockNote-Datei existiert nicht mehr");
    m.insert("tempfiles.owner.folder_missing", "Ihr Ordner fehlt (Laufwerk getrennt?)");
    m.insert("tempfiles.owner.unreachable", "Ihre LockNote-Datei kann nicht gepr\u{00fc}ft werden");
    m.insert("tempfiles.owner.no_companion", "Von einer \u{00e4}lteren Version hinterlassen");
    m.insert("tempfiles.owner.companion_unusable", "Begleitdatei unbrauchbar");
    m.insert("tempfiles.meaning.current", "Die ausstehende Speicherung der ge\u{00f6}ffneten LockNote-Datei. Sie wird beim Schlie\u{00df}en von LockNote hineingeschrieben.");
    m.insert("tempfiles.meaning.present", "Eine Speicherung, die auf diese LockNote-Datei wartet: Sie wird hineingeschrieben, wenn diese Datei das n\u{00e4}chste Mal ge\u{00f6}ffnet und geschlossen wird. Wird behalten.");
    m.insert("tempfiles.meaning.orphan", "Diese LockNote-Datei existiert nicht mehr, ihr Ordner schon. Diese Datei enth\u{00e4}lt die letzte damit gemachte Speicherung: Wer sie l\u{00f6}scht, verliert diese Notizen.");
    m.insert("tempfiles.meaning.folder_missing", "Weder diese LockNote-Datei noch ihr Ordner wurden gefunden: Laufwerk oder Netzwerkfreigabe getrennt, oder Ordner gel\u{00f6}scht. Wird behalten: Laufwerk wieder verbinden und diese Datei \u{00f6}ffnen.");
    m.insert("tempfiles.meaning.unreachable", "Ob diese LockNote-Datei existiert, kann nicht gepr\u{00fc}ft werden ({error}). Wird behalten.");
    m.insert("tempfiles.meaning.no_companion", "Von LockNote 1.2.23 oder \u{00e4}lter hinterlassen, das die zugeh\u{00f6}rige LockNote-Datei nicht vermerkte. Wird behalten: Diese Datei \u{00fc}bernimmt sie beim \u{00d6}ffnen.");
    m.insert("tempfiles.meaning.companion_unusable", "Ihre Begleitdatei ist unbrauchbar ({reason}). Wird behalten.");
    m.insert("tempfiles.owner.other_volume", "Ihr Laufwerk bezeichnet jetzt einen anderen Datentr\u{00e4}ger");
    m.insert("tempfiles.owner.tmp_unreadable", "Kann nicht gepr\u{00fc}ft werden");
    m.insert("tempfiles.meaning.other_volume", "Diese LockNote-Datei wird nicht gefunden, und ihr Laufwerksbuchstabe oder ihre Freigabe bezeichnet jetzt einen anderen Datentr\u{00e4}ger als beim Speichern (ein USB-Stick durch einen anderen ersetzt). Wird behalten: den richtigen Datentr\u{00e4}ger wieder einstecken und diese Datei \u{00f6}ffnen.");
    m.insert("tempfiles.meaning.tmp_unreadable", "Diese Datei kann nicht gepr\u{00fc}ft werden ({error}). Wird behalten.");
    m.insert("tempfiles.dlg.reading", "Lese...");
    m.insert("tempfiles.dlg.title", "Tempor\u{00e4}re Dateien");
    m.insert("tempfiles.dlg.intro", "LockNote kann seine eigene Programmdatei nicht \u{00fc}berschreiben, w\u{00e4}hrend es l\u{00e4}uft. Jede Speicherung wird zuerst in eine tempor\u{00e4}re Datei im Ordner unten geschrieben; beim Schlie\u{00df}en von LockNote kopiert ein Hilfsprogramm sie in die LockNote-Datei. Bis dahin ist die tempor\u{00e4}re Datei die einzige Kopie Ihrer letzten Speicherung: nicht von Hand l\u{00f6}schen.\n\nDer Ordner enth\u{00e4}lt au\u{00df}erdem: {hash}.base (Begleitdatei jeder tempor\u{00e4}ren Datei), conflicts\\ (bei einem Konflikt beiseitegelegte Versionen, nie gel\u{00f6}scht), swap-error.log (Protokoll der Kopien), import.log und auto-backups\\. Zugang: Ordner \u{00f6}ffnen, oder %LOCALAPPDATA%\\LockNote in die Adressleiste des Explorers eingeben.");
    m.insert("tempfiles.dlg.folder", "Ordner: {dir}");
    m.insert("tempfiles.dlg.open_folder", "Ordner \u{00f6}ffnen");
    m.insert("tempfiles.dlg.copy_path", "Pfad kopieren");
    m.insert("tempfiles.dlg.list", "Ausstehende Speicherungen (.tmp):");
    m.insert("tempfiles.dlg.delete", "Verwaiste l\u{00f6}schen...");
    m.insert("tempfiles.dlg.apply", "Diese LockNote-Datei:");
    m.insert("tempfiles.dlg.close_app", "LockNote jetzt schlie\u{00df}en");
    m.insert("tempfiles.dlg.swap_log", "Kopierprotokoll");
    m.insert("tempfiles.dlg.import_log", "Importprotokoll");
    m.insert("tempfiles.dlg.close", "Schlie\u{00df}en");
    m.insert("tempfiles.dlg.empty", "Keine tempor\u{00e4}re Datei: Alle Speicherungen wurden geschrieben.");
    m.insert("tempfiles.dlg.list_failed", "Der Ordner kann nicht gelesen werden: {msg}");
    m.insert("tempfiles.dlg.folder_absent", "Dort wurde noch nichts geschrieben: Der Ordner existiert nicht.");
    m.insert("tempfiles.dlg.folder_failed", "Der Ordner konnte nicht ge\u{00f6}ffnet werden: {msg}");
    m.insert("tempfiles.dlg.copy_failed", "Der Pfad konnte nicht kopiert werden: {msg}");
    m.insert("tempfiles.dlg.exe_unknown", "unbekannt");
    m.insert("tempfiles.dlg.details", "Datei: {tmp}\nLockNote-Datei: {exe}\n{size}, ge\u{00e4}ndert am {date} (UTC)\n\n{meaning}");
    m.insert("tempfiles.apply.pending", "Eine Speicherung von {size} wartet in {tmp}. Sie wird beim Schlie\u{00df}en von LockNote in {exe} geschrieben.");
    m.insert("tempfiles.apply.nothing", "Nichts ausstehend: {exe} enth\u{00e4}lt die letzte Speicherung.");
    m.insert("tempfiles.apply.undecided", "Eine Datei wartet in {tmp}, aber das Schlie\u{00df}en von LockNote schreibt sie nicht in {exe} ({reason}): Der n\u{00e4}chste Start entscheidet.");
    m.insert("tempfiles.apply.reason.conflict", "die LockNote-Datei wurde anderswo ge\u{00e4}ndert");
    m.insert("tempfiles.apply.reason.tmp_unreadable", "sie kann nicht gelesen werden");
    m.insert("tempfiles.apply.reason.exe_unreadable", "die LockNote-Datei kann nicht gelesen werden");
    m.insert("tempfiles.apply.reason.ignored", "sie enth\u{00e4}lt keine brauchbare Speicherung");
    m.insert("tempfiles.log.empty", "Noch keine Ereignisse aufgezeichnet ({file}).");
    m.insert("tempfiles.log.failed", "{file} kann nicht ge\u{00f6}ffnet werden: {msg}");
    m.insert("tempfiles.delete.title", "Verwaiste tempor\u{00e4}re Datei l\u{00f6}schen");
    m.insert("tempfiles.delete.confirm", "{tmp} l\u{00f6}schen?\n\nSie enth\u{00e4}lt die letzte mit {exe} gemachte Speicherung; diese Datei existiert nicht mehr. Die enthaltenen Notizen gehen verloren.");
    m.insert("tempfiles.delete.failed", "Nicht gel\u{00f6}scht: {msg}");

    m.insert("menu.edit", "&Bearbeiten");
    m.insert("menu.edit.cut", "&Ausschneiden\tStrg+X");
    m.insert("menu.edit.copy", "&Kopieren\tStrg+C");
    m.insert("menu.edit.paste", "&Einf\u{00fc}gen\tStrg+V");
    m.insert("menu.edit.pasteplain", "Als &reinen Text einf\u{00fc}gen\tStrg+Umschalt+V");
    m.insert("menu.edit.find", "&Suchen...\tStrg+F");
    m.insert("menu.edit.replace", "&Ersetzen...\tStrg+H");
    m.insert("menu.edit.searchall", "In allen &Tabs suchen...\tStrg+Umschalt+F");
    m.insert("searchall.title", "In allen Tabs suchen");
    m.insert("searchall.query", "Suchen:");
    m.insert("searchall.go", "Anzeigen");
    m.insert("searchall.vault", "Tresor: {category} / {name}");
    m.insert("searchall.prompt", "Prompt: {name}");
    m.insert("searchall.more", "Weitere Ergebnisse nicht angezeigt (Grenze: {max})");
    m.insert("search.regex", "Regul\u{00e4}rer Ausdruck");
    m.insert("search.regex.invalid", "Ung\u{00fc}ltiger regul\u{00e4}rer Ausdruck");
    m.insert("menu.edit.goto", "&Gehe zu Zeile...\tStrg+G");
    m.insert("menu.edit.dupline", "Zeile &duplizieren\tStrg+D");
    m.insert("menu.edit.delline", "Zeile &l\u{00f6}schen\tStrg+Umschalt+K");
    m.insert("menu.edit.case", "Gr&o\u{00df}-/Kleinschreibung");
    m.insert("menu.edit.case.upper", "&GROSSBUCHSTABEN\tStrg+Umschalt+U");
    m.insert("menu.edit.case.lower", "&kleinbuchstaben\tStrg+U");
    m.insert("menu.edit.case.title", "&Wortanf\u{00e4}nge gro\u{00df}");
    m.insert("case.error.title", "Gro\u{00df}-/Kleinschreibung");
    m.insert("case.error.mapping", "Die Schreibweise wurde nicht umgewandelt: {msg}");
    m.insert("menu.edit.lines", "Ze&ilen");
    m.insert("menu.edit.lines.sortasc", "&Aufsteigend sortieren");
    m.insert("menu.edit.lines.sortdesc", "Ab&steigend sortieren");
    m.insert("menu.edit.lines.dedup", "&Doppelte Zeilen entfernen");
    m.insert("menu.edit.lines.join", "Zeilen &verbinden");
    m.insert("menu.edit.lines.trim", "&Leerzeichen am Zeilenende entfernen");
    m.insert("lines.error.title", "Zeilen");
    m.insert("lines.error.sort", "Die Zeilen wurden nicht sortiert: {msg}");
    m.insert("menu.edit.selectall", "Alles aus&w\u{00e4}hlen\tStrg+A");
    m.insert("menu.edit.timestamp", "&Zeitstempel einf\u{00fc}gen\tF5");

    m.insert("menu.help", "&Hilfe");
    m.insert("menu.help.update", "Nach &Updates suchen...");
    m.insert("menu.help.about", "\u{00dc}&ber LockNote...");

    m.insert("ctx.cut", "Ausschneiden");
    m.insert("ctx.copy", "Kopieren");
    m.insert("ctx.paste", "Einf\u{00fc}gen");
    m.insert("ctx.pasteplain", "Als reinen Text einf\u{00fc}gen");
    m.insert("ctx.selectall", "Alles ausw\u{00e4}hlen");
    m.insert("ctx.rename", "Tab umbenennen...");
    m.insert("ctx.close", "Tab schlie\u{00df}en");

    m.insert("ctx.color", "Tab-Farbe");
    m.insert("color.none", "Keine");
    m.insert("color.blue", "Blau");
    m.insert("color.green", "Gr\u{00fc}n");
    m.insert("color.red", "Rot");
    m.insert("color.orange", "Orange");
    m.insert("color.purple", "Lila");
    m.insert("color.yellow", "Gelb");
    m.insert("color.pink", "Rosa");
    m.insert("color.gray", "Grau");
    m.insert("option.off", "Aus");
    m.insert("option.seconds", "{n} s");
    m.insert("option.minutes", "{n} min");

    m.insert("status.stats", "{words} W\u{00f6}rter | {chars} Zeichen | {lines} Zeilen");
    m.insert("status.saved", "Gespeichert");
    m.insert("status.modified", "Ge\u{00e4}ndert");
    m.insert("status.encrypted", "Verschl\u{00fc}sselt");

    m.insert("createpwd.title", "Passwort erstellen");
    m.insert("createpwd.password", "Passwort:");
    m.insert("createpwd.confirm", "Best\u{00e4}tigen:");
    m.insert("createpwd.create", "Erstellen");
    m.insert("createpwd.cancel", "Abbrechen");
    m.insert("createpwd.empty", "Das Passwort darf nicht leer sein.");
    m.insert("createpwd.tooshort", "Das Passwort ist zu kurz.");
    m.insert("createpwd.mismatch", "Die Passw\u{00f6}rter stimmen nicht \u{00fc}berein.");
    m.insert("createpwd.weak", "Schwach");
    m.insert("createpwd.fair", "Mittel");
    m.insert("createpwd.strong", "Stark");
    m.insert("createpwd.verystrong", "Sehr stark");
    m.insert("createpwd.welcome", "Willkommen bei LockNote");
    m.insert("createpwd.minlen", "Passwort (mind. {} Zeichen):");
    m.insert("createpwd.desc", "Ihre Notizen werden verschl\u{00fc}sselt und in dieser Datei gespeichert.\nW\u{00e4}hlen Sie ein Passwort zum Schutz.");

    m.insert("unlock.title", "Entsperren");
    m.insert("unlock.password", "Passwort:");
    m.insert("unlock.btn", "Entsperren");
    m.insert("unlock.cancel", "Abbrechen");
    m.insert("unlock.empty", "Bitte geben Sie ein Passwort ein.");
    m.insert("unlock.decrypting", "Entschl\u{00fc}sselung...");
    m.insert("unlock.maxattempts", "Maximale Anzahl an Versuchen erreicht. Beenden.");
    m.insert("unlock.wrong", "Falsches Passwort.");
    m.insert("unlock.desc", "Geben Sie Ihr Passwort ein, um Ihre Notizen zu entsperren.");
    m.insert("unlock.locked.title", "Gesperrt");
    m.insert("unlock.retryin", "Erneut versuchen in {seconds}s\u{2026}");
    m.insert("unlock.cryptoerror", "Verschl\u{00fc}sselungsfehler: {msg}");
    m.insert("lock.overlay.title", "LockNote ist gesperrt");
    m.insert("lock.curtain", "Geben Sie Ihr Passwort ein, um Ihre Notizen anzuzeigen.");
    m.insert("lock.curtain.title", "Notizen ausgeblendet");
    m.insert("lock.curtain.waiting_for", "LockNote wird gesperrt, sobald \u{201e}{window}\u{201c} geschlossen ist. Geben Sie Ihr Passwort ein, um Ihre Notizen anzuzeigen.");
    m.insert("lock.curtain.save_failed", "Das Speichern vor dem Sperren ist fehlgeschlagen: nichts ging verloren, Ihre \u{00c4}nderungen bleiben erhalten. Geben Sie Ihr Passwort ein, um sie anzuzeigen.");
    m.insert("lock.curtain.show", "Notizen anzeigen");
    m.insert("lock.curtain.quit_refused", "LockNote kann jetzt nicht geschlossen werden: Ihre ungespeicherten \u{00c4}nderungen liegen hinter diesem Bildschirm. Geben Sie Ihr Passwort ein, um sie anzuzeigen, und speichern oder schlie\u{00df}en Sie dann.");
    m.insert("lock.save_failed", "LockNote konnte vor dem Sperren nicht speichern:\n{error}\n\nIhre Notizen wurden nicht gesperrt, Ihre ungespeicherten \u{00c4}nderungen bleiben erhalten. Sie sind weiterhin nicht gespeichert: versuchen Sie es erneut (Strg+S) oder kopieren Sie Ihren Text.");
    m.insert("lock.overlay.click", "Klicken zum Entsperren");
    m.insert("unlock.show", "Passwort anzeigen");
    m.insert("unlock.hide", "Passwort verbergen");
    m.insert("unlock.capslock", "Feststelltaste ist aktiv");

    m.insert("close.unsaved", "Sie haben nicht gespeicherte \u{00c4}nderungen.");
    m.insert("close.remember", "Meine Wahl merken");
    m.insert("close.save", "Speichern");
    m.insert("close.dontsave", "Nicht speichern");
    m.insert("close.cancel", "Abbrechen");

    m.insert("search.find", "Suchen:");
    m.insert("search.replace", "Ersetzen:");
    m.insert("search.next", "Weiter");
    m.insert("search.btn.replace", "Ersetzen");
    m.insert("search.btn.replaceall", "Alle ersetzen");
    m.insert("search.close", "Schlie\u{00df}en");
    m.insert("search.notfound", "Nicht gefunden");
    m.insert("search.title", "Suchen und Ersetzen");
    m.insert("search.find.title", "Suchen");
    m.insert("search.prev", "Zur\u{00fc}ck");
    m.insert("search.case", "Gro\u{00df}-/Kleinschreibung");
    m.insert("search.wholeword", "Ganzes Wort");
    m.insert("search.count", "{current} / {total}");
    m.insert("search.count.total", "{total} Treffer");
    m.insert("search.count.none", "Keine Treffer");
    m.insert("search.replaceall.title", "Alle ersetzen");
    m.insert("search.replaceall.msg", "{count} Vorkommen ersetzt.");

    m.insert("goto.title", "Gehe zu Zeile");
    m.insert("goto.label", "Zeilennummer:");
    m.insert("goto.ok", "OK");
    m.insert("goto.cancel", "Abbrechen");
    m.insert("goto.invalid", "Ung\u{00fc}ltige Zeilennummer.");

    m.insert("rename.title", "Tab umbenennen");
    m.insert("rename.label", "Name:");
    m.insert("rename.ok", "OK");
    m.insert("rename.cancel", "Abbrechen");
    m.insert("rename.empty", "Der Name darf nicht leer sein.");

    m.insert("settings.title", "Einstellungen");
    m.insert("settings.onclose", "Beim Schlie\u{00df}en:");
    m.insert("settings.ask", "Nachfragen");
    m.insert("settings.always", "Immer speichern");
    m.insert("settings.never", "Nie speichern");
    m.insert("settings.theme", "Design:");
    m.insert("settings.dark", "Dunkel");
    m.insert("settings.light", "Hell");
    m.insert("settings.language", "Sprache:");
    m.insert("settings.minpwdlen", "Min. Passwortl\u{00e4}nge:");
    m.insert("settings.section.appearance", "Darstellung");
    m.insert("settings.section.security", "Sicherheit");
    m.insert("settings.section.behavior", "Verhalten");
    m.insert("settings.font", "Editor-Schriftart:");
    m.insert("settings.ok", "OK");
    m.insert("settings.cancel", "Abbrechen");
    m.insert("settings.savekey", "Speicher-Tastk\u{00fc}rzel:");
    m.insert("settings.fontsize", "Schriftgr\u{00f6}\u{00df}e:");
    m.insert("settings.pbkdf2", "PBKDF2-Iterationen:");
    m.insert("settings.hint", "Passworthinweis:");
    m.insert("settings.mintray", "In den Infobereich minimieren");
    m.insert("settings.autolock", "Automatisch sperren:");
    m.insert("settings.autosave", "Automatisch speichern:");
    m.insert("settings.clipboard", "Zwischenablage leeren:");
    m.insert("settings.hotkey", "Globales Tastk\u{00fc}rzel:");
    m.insert("settings.restart", "Bitte starten Sie LockNote neu, um die Sprach\u{00e4}nderung anzuwenden.");
    m.insert("update.restart.failed", "Das Update konnte beim Beenden nicht angewendet werden, daher wurde LockNote nicht neu gestartet.\n\nStarten Sie LockNote erneut.\n\nDetails: {log}");
    m.insert("settings.2fa", "Zwei-Faktor-Authentifizierung");
    m.insert("settings.2fa.enable", "TOTP-Zwei-Faktor-Authentifizierung aktivieren");
    m.insert("settings.2fa.setup", "2FA einrichten...");
    m.insert("settings.2fa.status.enabled", "2FA ist aktiviert");
    m.insert("settings.2fa.status.disabled", "2FA ist deaktiviert");

    m.insert("security.title", "Sicherheit");
    m.insert("security.changepw", "Passwort \u{00e4}ndern...");
    m.insert("security.section", "Sicherheit");
    m.insert("preferences.title", "Einstellungen");

    m.insert("update.fail", "Fehler bei der Update-Pr\u{00fc}fung.");
    m.insert("update.norelease", "Keine Versionsinformationen verf\u{00fc}gbar.");
    m.insert("update.uptodate", "Sie sind auf dem neuesten Stand!");
    m.insert("update.available", "Eine neue Version ist verf\u{00fc}gbar: {version}");
    m.insert("update.success", "Update erfolgreich heruntergeladen.");
    m.insert("update.error", "Update fehlgeschlagen: {error}");

    m.insert("totp.empty", "Keine TOTP-Schl\u{00fc}ssel konfiguriert.");
    m.insert("totp.add", "Schl\u{00fc}ssel hinzuf\u{00fc}gen");
    m.insert("totp.add.title", "TOTP-Schl\u{00fc}ssel hinzuf\u{00fc}gen");
    m.insert("totp.add.name", "Name:");
    m.insert("totp.add.secret", "Geheimnis:");
    m.insert("totp.add.invalid", "Ung\u{00fc}ltiges TOTP-Geheimnis.");
    m.insert("totp.add.secret_hint", "Base32 oder otpauth://-Link");
    m.insert("totp.add.generate", "Erzeugen");
    m.insert("totp.secret.err.length", "Dieses Geheimnis hat eine L\u{00e4}nge, die Base32 nie ergibt: ein Teil fehlt oder es wurde doppelt eingef\u{00fc}gt. F\u{00fc}gen Sie es erneut allein ein.");
    m.insert("totp.secret.err.short", "Dieses Geheimnis ist zu kurz (unter 80 Bit): wahrscheinlich fehlt ein Teil.");
    m.insert("totp.secret.err.doubled", "Derselbe Text steht zweimal da: er wurde wohl zweimal eingef\u{00fc}gt. Leeren Sie das Feld und f\u{00fc}gen Sie das Geheimnis einmal ein.");
    m.insert("totp.secret.err.duplicate", "Dieser Link enth\u{00e4}lt denselben Parameter zweimal (zum Beispiel zwei Geheimnisse): er wird abgelehnt, da das richtige nicht bekannt ist.");
    m.insert("totp.secret.err.empty", "Geben Sie das vom Dienst angegebene Geheimnis ein.");
    m.insert("totp.secret.err.base32", "Dieses Geheimnis ist kein g\u{00fc}ltiges Base32 (Buchstaben A-Z und Ziffern 2-7).");
    m.insert("totp.secret.err.not_totp", "Nur otpauth://totp/-Links werden unterst\u{00fc}tzt.");
    m.insert("totp.secret.err.no_secret", "Dieser otpauth://-Link enth\u{00e4}lt kein Geheimnis.");
    m.insert("totp.secret.err.unsupported", "Dieser Link verlangt Codes, die LockNote nicht erzeugt (nur SHA1, 6 Ziffern, 30 Sekunden).");
    m.insert("totp.remove", "Entfernen");
    m.insert("totp.remove.confirm", "Diesen TOTP-Schl\u{00fc}ssel entfernen?");
    m.insert("totp.copied", "Code in die Zwischenablage kopiert.");
    m.insert("clipboard.failed", "Kopieren fehlgeschlagen: {err}");
    m.insert("totp.secret.copied", "Geheimnis kopiert.");
    m.insert("totp.select_first", "Bitte zuerst einen Eintrag ausw\u{00e4}hlen.");
    m.insert("totp.added", "Eintrag hinzugef\u{00fc}gt.");
    m.insert("totp.removed", "Eintrag entfernt.");
    m.insert("totp.remove.title", "TOTP-Eintrag entfernen");
    m.insert("totp.remove.msg", "\u{201e}{name}\u{201c} entfernen? Dies kann nicht r\u{00fc}ckg\u{00e4}ngig gemacht werden.");
    m.insert("totp.add.prompt", "Name des Eintrags:");

    // TOTP Manager
    m.insert("totp.manager.copy", "Code kopieren");

    // 2FA / TOTP Verify
    m.insert("2fa.title", "LockNote \u{2014} Zwei-Faktor-Authentifizierung");
    m.insert("2fa.header", "Zwei-Faktor-Authentifizierung");
    m.insert("2fa.prompt", "Geben Sie den 6-stelligen Code aus Ihrer Authenticator-App ein.");
    m.insert("2fa.verify", "Best\u{00e4}tigen");
    m.insert("2fa.cancel", "Abbrechen");
    m.insert("2fa.enter6", "Bitte geben Sie einen 6-stelligen Code ein.");
    m.insert("2fa.maxattempts", "Maximale Anzahl an Versuchen erreicht.");
    m.insert("2fa.failed", "2FA fehlgeschlagen");
    m.insert("2fa.setup.title", "TOTP-Einrichtung");
    m.insert("2fa.setup.secret", "Base32-Geheimnis:");
    m.insert("2fa.setup.copy", "Kopieren");
    m.insert("2fa.setup.uri", "OtpAuth-URI:");
    m.insert("2fa.setup.verify", "Geben Sie einen Code aus Ihrem Authenticator zur Best\u{00e4}tigung ein:");
    m.insert("2fa.setup.confirm", "Best\u{00e4}tigen");
    m.insert("2fa.setup.verified", "Best\u{00e4}tigt!");
    m.insert("2fa.setup.invalid", "Ung\u{00fc}ltiger Code, versuchen Sie es erneut");
    m.insert("2fa.pwchanged", "Passwort ge\u{00e4}ndert \u{2713}");
    m.insert("2fa.retryin", "Erneut versuchen in {seconds}s\u{2026}");

    // Vault / Secret edit
    m.insert("vault.edit", "Geheimnis bearbeiten");
    m.insert("vault.new", "Neues Geheimnis");
    m.insert("vault.name", "Name:");
    m.insert("vault.category", "Kategorie:");
    m.insert("vault.value", "Wert:");
    m.insert("vault.show", "Anzeigen");
    m.insert("vault.name.empty", "Name darf nicht leer sein");
    m.insert("vault.btn.add", "Neu");
    m.insert("vault.btn.edit", "Bearb.");
    m.insert("vault.btn.del", "L\u{00f6}sch.");
    m.insert("vault.btn.copy", "Kopier.");
    m.insert("vault.btn.copied", "Kopiert!");
    m.insert("vault.copied.named", "Kopiert: {name}");
    // SSH keys (issue 054)
    m.insert("menu.tools.ssh", "&SSH-Schl\u{00fc}ssel");
    m.insert("menu.tools.ssh_generate", "SSH-Schl\u{00fc}ssel &erzeugen...");
    m.insert("menu.tools.ssh_agent", "In ssh-agent &laden...");
    // Password generator
    m.insert("menu.tools.passgen", "&Passwortgenerator...");
    m.insert("vault.generate", "Generieren...");
    m.insert("passgen.title", "Passwortgenerator");
    m.insert("passgen.password", "Passwort:");
    m.insert("passgen.show", "Anzeigen");
    m.insert("passgen.hide", "Verbergen");
    m.insert("passgen.generate", "Generieren");
    m.insert("passgen.length", "L\u{00e4}nge ({min}-{max}):");
    m.insert("passgen.upper", "Gro\u{00df}buchstaben (A-Z)");
    m.insert("passgen.lower", "Kleinbuchstaben (a-z)");
    m.insert("passgen.digits", "Ziffern (0-9)");
    m.insert("passgen.symbols", "Sonderzeichen");
    m.insert("passgen.symbol_set", "Erlaubte Sonderzeichen:");
    m.insert("passgen.exclude_ambiguous", "Verwechselbare Zeichen ausschlie\u{00df}en ({chars})");
    m.insert("passgen.use", "\u{00dc}bernehmen");
    m.insert("passgen.close", "Schlie\u{00df}en");
    m.insert("passgen.err.length", "Die L\u{00e4}nge muss zwischen {min} und {max} liegen.");
    m.insert("passgen.err.no_class", "W\u{00e4}hlen Sie mindestens eine Zeichenart.");
    m.insert("passgen.err.not_a_symbol", "\u{201e}{char}\u{201c} ist kein Sonderzeichen.");
    m.insert("passgen.err.no_symbol", "Kein Sonderzeichen \u{00fc}brig: Sonderzeichen hinzuf\u{00fc}gen oder \u{201e}Sonderzeichen\u{201c} abw\u{00e4}hlen.");
    m.insert("ssh.title", "SSH-Schl\u{00fc}ssel");
    m.insert("ssh.gen.title", "SSH-Schl\u{00fc}ssel erzeugen");
    m.insert("ssh.gen.type", "Typ:");
    m.insert("ssh.gen.recommended", "empfohlen");
    m.insert("ssh.gen.name", "Name im Vault:");
    m.insert("ssh.gen.comment", "Kommentar:");
    m.insert("ssh.gen.passphrase", "Passphrase (optional):");
    m.insert("ssh.gen.confirm", "Passphrase best\u{00e4}tigen:");
    m.insert("ssh.gen.generate", "&Erzeugen");
    m.insert("ssh.gen.generating", "Wird erzeugt...");
    m.insert("ssh.gen.fingerprint", "Fingerabdruck:");
    m.insert("ssh.gen.public", "\u{00d6}ffentlicher Schl\u{00fc}ssel:");
    m.insert("ssh.gen.copy_public", "\u{00d6}ffentl. Schl\u{00fc}ssel kopieren");
    m.insert("ssh.gen.copied", "\u{00d6}ffentlicher Schl\u{00fc}ssel kopiert.");
    m.insert("ssh.gen.save_vault", "Im Vault speichern");
    m.insert("ssh.gen.export", "Exportieren...");
    m.insert("ssh.gen.export_title", "SSH-Schl\u{00fc}ssel exportieren");
    m.insert("ssh.gen.close", "Schlie\u{00df}en");
    m.insert("ssh.gen.mismatch", "Die Passphrasen stimmen nicht \u{00fc}berein.");
    m.insert("ssh.gen.no_passphrase_export", "Dieser private Schl\u{00fc}ssel hat keine Passphrase: Wer die Datei lesen kann, kann ihn verwenden. Trotzdem exportieren?");
    m.insert("ssh.gen.exported", "Exportiert nach {path} (+ .pub).");
    m.insert("ssh.agent.title", "In ssh-agent laden");
    m.insert("ssh.agent.key", "Schl\u{00fc}ssel:");
    m.insert("ssh.agent.passphrase", "Passphrase (falls vorhanden):");
    m.insert("ssh.agent.warning", "Der Schl\u{00fc}ssel wird dem SSH-Agenten von Windows \u{00fc}bergeben, der ihn au\u{00df}erhalb von LockNote aufbewahren kann. Er wird beim Sperren und Schlie\u{00df}en entfernt. Nach einem unerwarteten Beenden wird er beim n\u{00e4}chsten Entsperren dieser Datei entfernt. Derselbe, manuell mit ssh-add hinzugef\u{00fc}gte Schl\u{00fc}ssel wird ebenfalls entfernt. Beim Laden wird diese Datei au\u{00df}erdem gespeichert.");
    m.insert("ssh.agent.load", "&Laden");
    m.insert("ssh.agent.none", "Kein privater SSH-Schl\u{00fc}ssel im Vault. Erzeugen Sie einen mit Werkzeuge > SSH-Schl\u{00fc}ssel > SSH-Schl\u{00fc}ssel erzeugen.");
    m.insert("ssh.agent.loaded", "Schl\u{00fc}ssel in ssh-agent geladen:\n{fingerprint}");
    m.insert("ssh.agent.save_failed", "Der Schl\u{00fc}ssel wurde geladen, aber diese Datei konnte nicht gespeichert werden; er wurde daher wieder aus ssh-agent entfernt.\n{detail}");
    m.insert("ssh.agent.unload_failed", "Einige SSH-Schl\u{00fc}ssel konnten nicht aus ssh-agent entfernt werden. Sie bleiben vermerkt; LockNote versucht es beim n\u{00e4}chsten Entsperren erneut.\n{detail}");
    m.insert("ssh.err.invalid_comment", "Der Kommentar muss in eine Zeile passen, ohne Tabulatoren oder Steuerzeichen.");
    m.insert("ssh.err.duplicate", "Im Vault gibt es bereits ein Geheimnis namens \u{201e}{name}\u{201c}.");
    m.insert("ssh.err.passphrase_required", "Dieser Schl\u{00fc}ssel ist gesch\u{00fc}tzt: Geben Sie seine Passphrase ein.");
    m.insert("ssh.err.wrong_passphrase", "Falsche Passphrase.");
    m.insert("ssh.err.agent_unavailable", "Der Windows-Dienst ssh-agent l\u{00e4}uft nicht.\nIn einer Administrator-PowerShell:\nGet-Service ssh-agent | Set-Service -StartupType Automatic\nStart-Service ssh-agent");
    m.insert("ssh.err.agent_failure", "ssh-agent hat den Vorgang abgelehnt.");
    m.insert("ssh.err.agent_timeout", "ssh-agent hat nicht rechtzeitig geantwortet.");
    m.insert("ssh.err.other", "Fehler: {detail}");
    m.insert("ssh.agent.save_failed_still_loaded", "Diese Datei konnte nicht gespeichert werden, und der Schl\u{00fc}ssel ist noch in ssh-agent. LockNote versucht, ihn beim Sperren oder Schlie\u{00df}en dieser Datei zu entfernen.\n{detail}");
    m.insert("ssh.err.invalid_name", "Der Name darf weder [ ] noch Steuerzeichen enthalten.");
    m.insert("vault.copy.failed", "Kopieren fehlgeschlagen");
    m.insert("vault.btn.reveal", "Zeigen");
    m.insert("vault.tree.error", "Tresor kann nicht angezeigt werden");
    m.insert("vault.search", "Namen suchen");
    m.insert("vault.category.none", "Kategorie w\u{00e4}hlen");
    m.insert("vault.cat.menu.new", "Neue Kategorie\u{2026}");
    m.insert("vault.cat.menu.rename", "Umbenennen\u{2026}");
    m.insert("vault.cat.menu.delete", "L\u{00f6}schen");
    m.insert("vault.cat.new.title", "Neue Kategorie");
    m.insert("vault.cat.rename.title", "Kategorie umbenennen");
    m.insert("vault.cat.name", "Name der Kategorie:");
    m.insert("vault.cat.delete.title", "Kategorie l\u{00f6}schen");
    m.insert("vault.cat.delete.confirm", "Kategorie \u{201E}{name}\u{201C} l\u{00f6}schen? Ihre {count} Geheimnis(se) werden nach General verschoben.");
    m.insert("vault.cat.error.title", "Kategorie");
    m.insert("vault.cat.err.empty", "Der Name darf nicht leer sein.");
    m.insert("vault.cat.err.toolong", "Der Name ist zu lang (max. {max} Zeichen).");
    m.insert("vault.cat.err.char", "Das Zeichen \u{201E}{char}\u{201C} ist nicht erlaubt.");
    m.insert("vault.cat.err.duplicate", "Eine Kategorie \u{201E}{name}\u{201C} existiert bereits.");
    m.insert("vault.cat.err.toomany", "Zu viele Kategorien (max. {max}).");
    m.insert("vault.cat.err.general", "Die Kategorie General kann weder umbenannt noch gel\u{00f6}scht werden.");
    m.insert("vault.cat.err.notfound", "Kategorie nicht gefunden: {name}");
    m.insert("vault.cat.err.compare", "Namensvergleich fehlgeschlagen: {msg}");

    // Menu bar toggle buttons
    m.insert("menu.toggle.vault", "Tresor");
    m.insert("menu.toggle.totp", "TOTP");
    m.insert("menu.toggle.attach", "Dateien");
    m.insert("menu.toggle.lock", "\u{1F512} Sperren");

    // Attachments
    m.insert("menu.tools.attachments", "&Anh\u{00e4}nge\tCtrl+J");
    // Prompts panel (issue 058)
    m.insert("menu.toggle.prompts", "Prompts");
    m.insert("menu.tools.prompts", "P&rompts");
    m.insert("palette.prompt", "Prompt: {name}");
    m.insert("prompts.btn.insert", "Einf\u{00fc}g.");
    m.insert("prompts.inserted.named", "Eingef\u{00fc}gt: {name}");
    m.insert("palette.title", "Befehlspalette");
    m.insert("palette.tab", "Tab: {name}");
    m.insert("palette.vault", "Tresor: {name}");
    m.insert("prompts.search", "Suchen (Name oder Text)");
    m.insert("prompts.btn.add", "Neu");
    m.insert("prompts.btn.edit", "Bearb.");
    m.insert("prompts.btn.del", "L\u{00f6}sch.");
    m.insert("prompts.btn.copy", "Kopier.");
    m.insert("prompts.copied.named", "Kopiert: {name}");
    m.insert("prompts.copy.failed", "Kopieren fehlgeschlagen: {err}");
    m.insert("prompts.count", "{count} Prompt(s)");
    m.insert("prompts.count.filtered", "{shown} von {total} Prompt(s)");
    m.insert("prompts.list.error", "Die Liste kann nicht angezeigt werden: {msg}");
    m.insert("prompts.new", "Neuer Prompt");
    m.insert("prompts.edit", "Prompt bearbeiten");
    m.insert("prompts.name", "Name:");
    m.insert("prompts.category", "Kategorie:");
    m.insert("prompts.text", "Text:");
    m.insert("prompts.category.default", "Allgemein");
    m.insert("prompts.err.name", "Name darf nicht leer sein");
    m.insert("prompts.err.category", "Kategorie darf nicht leer sein");
    m.insert("prompts.err.text", "Text darf nicht leer sein");
    m.insert("prompts.err.toolong", "Text zu lang (max. {max} MB)");
    m.insert("prompts.err.name.char", "Der Name darf {char} nicht enthalten");
    m.insert("prompts.err.category.char", "Die Kategorie darf {char} nicht enthalten");
    m.insert("prompts.delete.title", "Prompt l\u{00f6}schen");
    m.insert("prompts.delete.confirm", "Prompt \u{201e}{name}\u{201c} l\u{00f6}schen?");
    m.insert("prompts.gone", "Dieser Prompt wurde inzwischen ge\u{00e4}ndert oder entfernt (Import?): nichts wurde gespeichert.");
    m.insert("vault.edit.gone", "Dieses Geheimnis wurde inzwischen ge\u{00e4}ndert oder entfernt (Import?): nichts wurde gespeichert.");
    m.insert("attach.btn.add", "Neu");
    m.insert("attach.btn.open", "\u{00d6}ffnen");
    m.insert("attach.btn.save", "Speich.");
    m.insert("attach.btn.del", "L\u{00f6}sch.");
    m.insert("attach.error.read", "Datei konnte nicht gelesen werden: {}");
    m.insert("attach.error.corrupt", "Anhang besch\u{00e4}digt (SHA-256 ung\u{00fc}ltig).");
    m.insert("attach.error.write_temp", "Tempor\u{00e4}re Datei konnte nicht geschrieben werden: {}");
    m.insert("attach.error.save", "Datei konnte nicht gespeichert werden: {}");
    m.insert("attach.risky.title", "Anhang \u{00f6}ffnen?");
    m.insert("attach.risky.body", "Dieser Anhang ist eine ausf\u{00fc}hrbare Datei. Beim \u{00d6}ffnen wird Code auf Ihrem Computer ausgef\u{00fc}hrt. Fahren Sie nur fort, wenn Sie der Quelle dieses Tresors vertrauen.\n\nTrotzdem \u{00f6}ffnen?");
    m.insert("attach.confirm.delete", "Anhang \"{}\" l\u{00f6}schen?");
    m.insert("attach.confirm.title", "Best\u{00e4}tigen");
    m.insert("attach.save.title", "\"{}\" speichern unter");
    m.insert("attach.add.title", "Datei anh\u{00e4}ngen");

    // Steganographie
    m.insert("menu.tools.stego", "Ste&ganographie");
    m.insert("menu.tools.stego_hide", "In Bild &verstecken...");
    m.insert("menu.tools.stego_extract", "Aus Bild e&xtrahieren...");
    m.insert("stego.confirm_action", "steganographischer Export");
    m.insert("stego.dlg.open_carrier", "Tr\u{00e4}gerbild ausw\u{00e4}hlen");
    m.insert("stego.dlg.save", "Steganographisches Bild speichern");
    m.insert("stego.dlg.open_extract", "Bild zum Extrahieren ausw\u{00e4}hlen");
    m.insert("stego.title", "Steganographie");
    m.insert("stego.success", "Inhalt erfolgreich versteckt.");
    m.insert("stego.size", "Gr\u{00f6}\u{00df}e");
    m.insert("stego.warning_recompress", "Nicht via WhatsApp, Instagram usw. teilen.\nBildkomprimierung zerst\u{00f6}rt versteckte Daten.");
    m.insert("stego.extracted", "Versteckte Notizen erfolgreich extrahiert.");
    m.insert("stego.nodata_title", "Keine versteckten Daten");
    m.insert("stego.nodata", "Keine versteckten Daten in diesem Bild.");
    m.insert("stego.error", "Fehler");
    m.insert("stego.error.read_image", "Bild konnte nicht gelesen werden");
    m.insert("stego.error.format_title", "Nicht unterst\u{00fc}tztes Format");
    m.insert("stego.error.format", "Nicht unterst\u{00fc}tztes Format (JPEG, PNG, BMP).");
    m.insert("stego.error.write", "Datei konnte nicht geschrieben werden");
    m.insert("stego.error.toolarge_title", "Daten zu gro\u{00df}");
    m.insert("stego.error.toolarge", "Notizen zu gro\u{00df} f\u{00fc}r dieses Bild.");
    m.insert("stego.error.toolarge_hint", "W\u{00e4}hlen Sie ein gr\u{00f6}\u{00df}eres Tr\u{00e4}gerbild.");
    m.insert("stego.error.embed", "Daten konnten nicht versteckt werden");
    m.insert("stego.error.encrypt", "Verschl\u{00fc}sselung beim Bildexport fehlgeschlagen: {msg}");
    m.insert("stego.error.decrypt_title", "Extraktion fehlgeschlagen");
    m.insert("stego.error.decrypt", "Falsches Passwort oder keine LockNote-Daten.");
    m.insert("stego.error.extract", "Daten konnten nicht extrahiert werden");
    m.insert("stego.pw.title_hide", "Bild-Passwort w\u{00e4}hlen");
    m.insert("stego.pw.title_extract", "Bild-Passwort");
    m.insert("stego.pw.prompt_hide", "Passwort zum Schutz dieses Bildes:");
    m.insert("stego.pw.prompt_extract", "Passwort, das zum Verstecken verwendet wurde:");
    m.insert("stego.pw.confirm", "Passwort best\u{00e4}tigen:");
    m.insert("stego.pw.empty", "Das Passwort darf nicht leer sein.");
    m.insert("stego.pw.mismatch", "Die Passw\u{00f6}rter stimmen nicht \u{00fc}berein.");

    // Snapshot

    // History

    m.insert("export.prompt", "Notiz als Klartext exportieren?");
    m.insert("export.btn", "Exportieren");
    m.insert("export.warning", "Die exportierte Datei wird NICHT verschl\u{00fc}sselt.");
    m.insert("export.dlg.title", "Notiz exportieren");
    m.insert("export.dlg.filter", "Textdateien (*.txt)|*.txt|Alle Dateien (*.*)|*.*");
    m.insert("export.fail", "Fehler beim Exportieren der Datei.");

    // Export encoding
    m.insert("export.encoding.title", "Export-Kodierung");
    m.insert("export.encoding.label", "Kodierung f\u{00fc}r den Export w\u{00e4}hlen:");

    // Drag & drop
    m.insert("drop.binary", "Dieser Dateityp wird nicht unterst\u{00fc}tzt. LockNote kann nur Textdateien importieren.");
    m.insert("drop.invalid", "Diese Datei konnte nicht als Text gelesen werden. M\u{00f6}glicherweise handelt es sich um eine Bin\u{00e4}rdatei.");
    m.insert("drop.choice.title", "Abgelegte Datei");
    m.insert("drop.choice.text", "Wie m\u{00f6}chten Sie diese Textdatei behandeln?");
    m.insert("drop.choice.binary", "Diese Datei kann als Anhang hinzugef\u{00fc}gt werden.");
    m.insert("drop.choice.insert", "Text einf\u{00fc}gen");
    m.insert("drop.choice.attach", "Als Anhang hinzuf\u{00fc}gen");
    m.insert("drop.choice.cancel", "Abbrechen");
    m.insert("drop.directory", "Ordner k\u{00f6}nnen nicht in LockNote abgelegt werden. Bitte einzelne Dateien ablegen.");

    m.insert("save.error.title", "Speicherfehler");
    m.insert("save.error.msg", "Speichern fehlgeschlagen: {}");
    m.insert("export.success.title", "Export");
    m.insert("export.success.msg", "Notiz exportiert nach:\n{}");
    m.insert("export.error.title", "Exportfehler");
    m.insert("export.error.msg", "Export fehlgeschlagen: {}");
    m.insert("export.warning_title", "Exportwarnung");
    m.insert("export.warning_lossy", "Einige Zeichen k\u{00f6}nnen in dieser Kodierung nicht dargestellt werden und wurden ersetzt. Fortfahren?");
    m.insert("menu.file.backup_export", "Vollst\u{00e4}ndige Sicherung exportieren...");
    m.insert("menu.file.backup_import", "Vollst\u{00e4}ndige Sicherung importieren...");
    m.insert("backup.export.save_title", "Vollst\u{00e4}ndige Sicherung speichern");
    m.insert("backup.filter", "LockNote-Sicherung (*.lnbak)|Alle Dateien (*.*)");
    m.insert("backup.export.done", "Vollst\u{00e4}ndige Sicherung gespeichert unter:\n{path}\n\nSie enth\u{00e4}lt alle Tabs, Schnappsch\u{00fc}sse, den Tresor, die Anh\u{00e4}nge und die Einstellungen, verschl\u{00fc}sselt mit dem Passwort dieser Notiz.");
    m.insert("backup.export.overwrite", "{path}\n\nDiese Datei existiert bereits. Ersetzen?");
    m.insert("backup.export.error", "Sicherung fehlgeschlagen: {msg}");
    m.insert("backup.import.open_title", "Vollst\u{00e4}ndige Sicherung importieren");
    m.insert("backup.import.pw_title", "Passwort der Sicherung");
    m.insert("backup.import.pw_label", "Passwort dieser Sicherung:");
    m.insert("backup.import.wrong_password", "Falsches Passwort oder besch\u{00e4}digte Datei.");
    m.insert("backup.import.not_backup", "Diese Datei ist keine LockNote-Sicherung.");
    m.insert("backup.import.newer", "Diese Sicherung stammt von einer neueren LockNote-Version (Format {found}, diese Version liest bis Format {supported}). Bitte aktualisieren Sie LockNote.");
    m.insert("backup.import.corrupt", "Diese Sicherung ist besch\u{00e4}digt: {msg}");
    m.insert("backup.import.read_error", "Datei kann nicht gelesen werden: {msg}");
    m.insert("backup.import.too_large", "Diese Datei ist zu gro\u{00df} f\u{00fc}r eine Sicherung.");
    m.insert("backup.import.confirm", "Den GESAMTEN aktuellen Inhalt durch diese Sicherung ersetzen?\n\nSicherung vom {date}:\n- {tabs} Tab(s)\n- {prompts} Prompt(s)\n- {secrets} Tresor-Geheimnis(se)\n- {attachments} Anhang/Anh\u{00e4}nge\n\nIhre aktuellen Tabs, Prompts, Ihr Tresor, Anh\u{00e4}nge und Einstellungen (einschlie\u{00df}lich Zwei-Faktor-Authentifizierung) werden ersetzt. Nach dem Speichern der Notiz nicht mehr r\u{00fc}ckg\u{00e4}ngig zu machen.");
    m.insert("backup.import.done", "Sicherung importiert. Speichern Sie, um sie zu behalten. Die Spracheinstellung gilt nach dem Speichern und Neustarten von LockNote.");
    m.insert("autobackup.failed.import", "Die Sicherheitskopie konnte nicht geschrieben werden, daher wurde nichts importiert.\n\n{msg}");
    m.insert("autobackup.failed.update", "Die Sicherheitskopie konnte nicht geschrieben werden, daher wurde das Update nicht heruntergeladen.\n\n{msg}");
    m.insert("autobackup.purge.ask", "{n} automatische Sicherung(en) wurden mit Ihrem alten Passwort erstellt und lassen sich damit weiterhin \u{00f6}ffnen.\n\nJetzt l\u{00f6}schen?");
    m.insert("autobackup.purge.partial", "{n} automatische Sicherung(en) konnten nicht gel\u{00f6}scht werden. L\u{00f6}schen Sie sie von Hand in:\n{dir}");
    m.insert("menu.file.backups", "Sicherheits&kopien...");
    m.insert("autobackup.dlg.title", "Sicherheitskopien");
    m.insert("autobackup.dlg.list", "Sicherungen (neueste zuerst):");
    m.insert("autobackup.dlg.empty", "Noch keine Sicherheitskopie.");
    m.insert("autobackup.dlg.details", "{kind}\n{date} (UTC)\n{size}\n\nL\u{00e4}sst sich mit dem Passwort \u{00f6}ffnen, das Sie damals hatten.");
    m.insert("autobackup.dlg.restore", "Wiederherstellen");
    m.insert("autobackup.dlg.create", "Jetzt sichern");
    m.insert("totp.nextcode", "N\u{00e4}chster Code in {n} s");
    m.insert("autobackup.dlg.delete", "L\u{00f6}schen");
    m.insert("autobackup.dlg.folder", "Ordner \u{00f6}ffnen");
    m.insert("autobackup.dlg.close", "Schlie\u{00df}en");
    m.insert("autobackup.dlg.delete.title", "Sicherung l\u{00f6}schen");
    m.insert("autobackup.dlg.delete.confirm", "Diese Sicherung endg\u{00fc}ltig l\u{00f6}schen?");
    m.insert("autobackup.dlg.delete.failed", "Diese Sicherung konnte nicht gel\u{00f6}scht werden: {msg}");
    m.insert("autobackup.dlg.folder.failed", "Der Ordner konnte nicht ge\u{00f6}ffnet werden: {msg}");
    m.insert("autobackup.dlg.list_failed", "Der Sicherungsordner konnte nicht gelesen werden: {msg}");
    m.insert("autobackup.kind.pre_import", "Vor Import");
    m.insert("autobackup.kind.pre_update", "Vor Update");
    m.insert("autobackup.kind.manual", "Manuell");
    m.insert("autobackup.manual.done", "Sicherung erstellt.");
    m.insert("autobackup.manual.failed", "Die Sicherung konnte nicht geschrieben werden.\n\n{msg}");
    m.insert("menu.file.import_exe", "Aus anderem LockNote importieren...");
    m.insert("import_exe.title", "Aus anderem LockNote importieren");
    m.insert("import_exe.open_title", "Zu importierende LockNote-Datei w\u{00e4}hlen");
    m.insert("import_exe.filter", "LockNote (*.exe)|Alle Dateien (*.*)");
    m.insert("import_exe.same_file", "Das ist die Datei, die Sie gerade ausf\u{00fc}hren. W\u{00e4}hlen Sie Ihre \u{00e4}ltere LockNote-Datei.");
    // Startup conflict between the pending save and a more recent program file (issue 055)
    m.insert("conflict.title", "LockNote - Zwei Versionen Ihrer Notizen");
    m.insert("conflict.body", "LockNote hat zwei verschiedene Versionen Ihrer Notizen gefunden.\n\nAusstehende Speicherung, noch nicht in die Programmdatei geschrieben:\n{tmp_date} (UTC)\n\nProgrammdatei, enth\u{00e4}lt eine andere Version, zuletzt ge\u{00e4}ndert:\n{exe_date} (UTC)\n{exe}\n\nJa: die ausstehende Speicherung behalten.\nNein: die Notizen der Programmdatei behalten.\nAbbrechen: beenden, ohne etwas zu \u{00e4}ndern.\n\nDie Version, die Sie nicht behalten, wird nicht gel\u{00f6}scht. Sie wird verschoben nach:\n{dir}");
    m.insert("conflict.exe_empty", "Hinweis: Die Programmdatei enth\u{00e4}lt keine Notizen.");
    m.insert("conflict.failed", "Ihre Auswahl konnte nicht angewendet werden. Es wurde nichts gel\u{00f6}scht.\n\n{msg}\n\nLockNote wird beendet.");
    m.insert("pending.failed", "LockNote kann Ihre Notizen nicht sicher \u{00f6}ffnen. Es wurde nichts ge\u{00e4}ndert.\n\n{msg}\n\nLockNote wird beendet.");
    m.insert("import_exe.no_data", "Keine LockNote-Daten in dieser Datei gefunden.");
    m.insert("import_exe.pw_title", "Passwort der importierten Datei");
    m.insert("import_exe.pw_label", "Passwort der LockNote-Datei, die Sie importieren:");
    m.insert("import_exe.confirm", "Den GESAMTEN aktuellen Inhalt durch die Daten dieser LockNote-Datei ersetzen?\n\nSie enth\u{00e4}lt:\n- {tabs} Tab(s)\n- {prompts} Prompt(s)\n- {secrets} Tresor-Geheimnis(se)\n- {attachments} Anhang/Anh\u{00e4}nge\n\nIhre aktuellen Tabs, Prompts, Ihr Tresor, Anh\u{00e4}nge und Einstellungen (einschlie\u{00df}lich Zwei-Faktor-Authentifizierung) werden ersetzt. Die importierte Datei selbst wird nicht ver\u{00e4}ndert.");
    m.insert("import_exe.done", "Daten importiert. Speichern Sie, um sie zu behalten: Sie sind dann durch das Passwort DIESER Datei gesch\u{00fc}tzt. Die Spracheinstellung gilt nach dem Speichern und Neustarten von LockNote.");

    m.insert("tray.restore", "&Wiederherstellen");
    m.insert("tray.quit", "&Beenden");

    m.insert("about.title", "\u{00dc}ber LockNote");
    m.insert("about.desc", "Eigenst\u{00e4}ndiger verschl\u{00fc}sselter Notizblock f\u{00fc}r Windows");

    m.insert("confirmpwd.title", "Passwort best\u{00e4}tigen");
    m.insert("confirmpwd.export", "Geben Sie Ihr Passwort ein, um zu exportieren:");
    m.insert("confirmpwd.print", "Geben Sie Ihr Passwort ein, um zu drucken:");
    m.insert("about.version", "Version {version}");
    m.insert("totp.setup.for", "TOTP-Einrichtung f\u{00fc}r: {name}");
    m.insert("2fa.invalid.left", "Ung\u{00fc}ltiger Code. Verbleibende Versuche: {n}.");
    m.insert("attach.error.title", "Fehler");
    m.insert("attach.limit.title", "Anhang");
    m.insert("attach.error.toolarge", "Datei zu gro\u{00df}: {size} (max. {max})");
    m.insert("attach.error.totaltoolarge", "Anh\u{00e4}nge insgesamt zu gro\u{00df}: {size} (max. {max})");
    m.insert("status.word", "{n} Wort");
    m.insert("status.words", "{n} W\u{00f6}rter");
    m.insert("status.char", "{n} Zeichen");
    m.insert("status.chars", "{n} Zeichen");
    m.insert("status.line", "{n} Zeile");
    m.insert("status.lines", "{n} Zeilen");
    m.insert("status.caret", "Zeile {line}, Spalte {col}");
    m.insert("status.selected.one", "{n} ausgew\u{00e4}hlt");
    m.insert("status.selected.many", "{n} ausgew\u{00e4}hlt");
    m.insert("save.error.encrypt", "Verschl\u{00fc}sselung fehlgeschlagen: {}");
    m.insert("export.success.encoding", "Notiz als {encoding} exportiert nach:\n{path}");
    m.insert("filter.text", "Textdateien (*.txt)");
    m.insert("filter.all", "Alle Dateien (*.*)");
    m.insert("filter.images", "Bilder (*.jpg;*.jpeg;*.png;*.bmp)");
    m.insert("filter.image", "Bild (*.{ext})");
    m.insert("update.available.title", "Update verf\u{00fc}gbar");
    m.insert("update.available.ask", "Eine neue Version ({version}) ist verf\u{00fc}gbar.\nM\u{00f6}chten Sie sie herunterladen?");
    m.insert("update.downloaded.title", "Update heruntergeladen");
    m.insert("update.downloaded.ask", "Update erfolgreich heruntergeladen.\n\nLockNote jetzt neu starten, um es anzuwenden?");
    m.insert("update.error.title", "Update-Fehler");
    m.insert("update.uptodate.title", "Aktuell");
    m.insert("update.uptodate.msg", "Sie verwenden die neueste Version ({version}).");
    m.insert("update.checkfailed.title", "Update-Pr\u{00fc}fung fehlgeschlagen");
    m.insert("update.checkfailed.msg", "Updates konnten nicht gepr\u{00fc}ft werden: {error}");
    m.insert("vault.delete.title", "Geheimnis l\u{00f6}schen");
    m.insert("vault.delete.msg", "M\u{00f6}chten Sie dieses Geheimnis wirklich l\u{00f6}schen?");
    m.insert("filter.ext", "{EXT}-Dateien (*.{ext})");
    m.insert("close.title", "Ungespeicherte \u{00c4}nderungen");
    m.insert("close.msg", "Ungespeicherte \u{00c4}nderungen. Vor dem Schlie\u{00df}en speichern?");
    m.insert("close.save_failed", "LockNote konnte nicht speichern:\n{error}\n\nIhre ungespeicherten \u{00c4}nderungen sind noch im Fenster. Lassen Sie LockNote ge\u{00f6}ffnet, um es erneut zu versuchen oder Ihren Text zu kopieren; Schlie\u{00df}en ohne Speichern verwirft sie.");
    m.insert("close.keep_open", "LockNote offen lassen");
    m.insert("close.close_unsaved", "Ohne Speichern schlie\u{00df}en");
    m.insert("close.stuck", "LockNote kann sich noch nicht schlie\u{00df}en: es wartet, bis \u{201e}{window}\u{201c} geschlossen ist.\n\nWarten Sie weiter oder schlie\u{00df}en Sie ohne Speichern: die ungespeicherten \u{00c4}nderungen gehen dann verloren.");
    m.insert("close.keep_waiting", "Weiter warten");
    m.insert("endsession.blocked", "LockNote konnte Ihre Notizen nicht speichern ({error}). Brechen Sie ab und speichern oder kopieren Sie dann Ihren Text.");
    m.insert("close.remember_not_saved", "Ihre Wahl wurde nicht gemerkt: LockNote speichert seine Einstellungen in der Notiz, und die Notiz wird nicht gespeichert. Um diese Frage nicht mehr zu sehen, w\u{00e4}hlen Sie \u{201e}{onclose} {never}\u{201c} unter {menu} und speichern Sie.");

    m
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

// Sérialise les tests qui mutent ou lisent CURRENT_LANG (y compris hors de ce
// module : vault_panel, create_password, etc.). Sans lock, l'exécution
// parallèle de `set_language()` par un autre test écrase l'état avant
// l'assert d'un test qui lit une chaîne localisée.
#[cfg(test)]
pub(crate) mod test_support {
    use std::sync::{Mutex, MutexGuard};

    static LANG_LOCK: Mutex<()> = Mutex::new(());

    /// Held by every test that sets or reads a translated text. Dropping it
    /// (also when an assert fails) puts English back, so a test can never
    /// leak French into the next one (pass 3: flaky live dialog test).
    pub(crate) struct LangGuard {
        _lock: MutexGuard<'static, ()>,
    }

    impl Drop for LangGuard {
        fn drop(&mut self) {
            super::set_language(super::Language::English);
        }
    }

    pub(crate) fn lang_guard() -> LangGuard {
        let lock = LANG_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        // Start from English too: a test that took no guard may have
        // changed the language.
        super::set_language(super::Language::English);
        LangGuard { _lock: lock }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::test_support::lang_guard;

    /// Issue 055, pass 3: the pending save is judged by content, so the date
    /// shown for the program file may be older than the pending save's. The
    /// question must not say that the file changed "since then".
    #[test]
    fn conflict_text_does_not_say_the_program_file_changed_since() {
        for lang in [Language::English, Language::French, Language::Spanish, Language::German] {
            let body = dict_for(lang).get("conflict.body").expect("conflict.body");
            for wrong in ["changed since then", "modifi\u{00e9} depuis", "modificado despu\u{00e9}s", "seitdem ge\u{00e4}ndert"] {
                assert!(!body.contains(wrong), "{:?}: {}", lang, body);
            }
            assert!(body.contains("{exe_date}") && body.contains("{tmp_date}"), "{:?}", lang);
        }
    }

    #[test]
    fn english_common_keys() {
        let _g = lang_guard();
        set_language(Language::English);
        assert_eq!(get("app.title"), "LockNote");
        assert_eq!(get("menu.file"), "&File");
        assert_eq!(get("menu.edit.copy"), "&Copy\tCtrl+C");
        assert_eq!(get("unlock.title"), "Unlock");
        assert_eq!(get("settings.title"), "Settings");
    }

    #[test]
    fn french_translations() {
        let _g = lang_guard();
        set_language(Language::French);
        assert_eq!(get("menu.file"), "&Fichier");
        assert_eq!(get("unlock.title"), "D\u{00e9}verrouiller");
        assert_eq!(get("close.save"), "Enregistrer");
        assert_eq!(get("color.blue"), "Bleu");
    }

    #[test]
    fn spanish_translations() {
        let _g = lang_guard();
        set_language(Language::Spanish);
        assert_eq!(get("menu.file"), "&Archivo");
        assert_eq!(get("unlock.title"), "Desbloquear");
        assert_eq!(get("close.save"), "Guardar");
        assert_eq!(get("color.red"), "Rojo");
    }

    #[test]
    fn german_translations() {
        let _g = lang_guard();
        set_language(Language::German);
        assert_eq!(get("menu.file"), "&Datei");
        assert_eq!(get("unlock.title"), "Entsperren");
        assert_eq!(get("close.save"), "Speichern");
        assert_eq!(get("color.green"), "Gr\u{00fc}n");
    }

    #[test]
    fn fallback_to_english() {
        let _g = lang_guard();
        set_language(Language::French);
        // app.title is the same in French, but let's test a key that exists in EN
        // All keys exist in all languages, so test with a hypothetical missing key
        // by verifying the fallback mechanism works via the placeholder path
        assert_eq!(get("app.title"), "LockNote");
    }

    #[test]
    fn unlock_card_keys_exist_in_every_language() {
        let _g = lang_guard();
        for lang in [Language::English, Language::French, Language::Spanish, Language::German] {
            set_language(lang);
            for key in ["unlock.show", "unlock.hide", "unlock.capslock"] {
                assert!(dict_for(lang).get(key).is_some(), "{key} missing for {lang:?}");
            }
            assert_ne!(get("unlock.show"), get("unlock.hide"));
        }
    }

    // Issue 058, pass 2: the access key of Tools > Prompts is free in the
    // Tools menu of every language (the items of `editor::build`).
    #[test]
    fn prompts_menu_access_key_is_free_in_every_language() {
        let key_of = |label: &str| -> Option<char> {
            let i = label.find('&')?;
            label[i + 1..].chars().next().map(|c| c.to_lowercase().next().unwrap_or(c))
        };
        let tools = [
            "menu.view.vault",
            "menu.tools.totp",
            "menu.tools.attachments",
            "menu.tools.passgen",
            "menu.tools.stego",
            "menu.tools.ssh",
        ];
        for lang in [Language::English, Language::French, Language::Spanish, Language::German] {
            let d = dict_for(lang);
            let mine = key_of(d["menu.tools.prompts"]).unwrap_or_else(|| panic!("no access key {lang:?}"));
            for item in tools {
                assert_ne!(key_of(d[item]), Some(mine), "{item} of {lang:?} uses the same key as Prompts");
            }
        }
    }

    // Go to Heading: its access key is free in the Edit menu of every
    // language (the items of `editor::build`), and the label names its
    // shortcut.
    #[test]
    fn goto_heading_menu_access_key_is_free_in_every_language() {
        let key_of = |label: &str| -> Option<char> {
            let i = label.find('&')?;
            label[i + 1..].chars().next().map(|c| c.to_lowercase().next().unwrap_or(c))
        };
        let edit = [
            "menu.edit.cut",
            "menu.edit.copy",
            "menu.edit.paste",
            "menu.edit.pasteplain",
            "menu.edit.find",
            "menu.edit.replace",
            "menu.edit.goto",
            "menu.edit.dupline",
            "menu.edit.delline",
            "menu.edit.selectall",
            "menu.edit.timestamp",
        ];
        for lang in [Language::English, Language::French, Language::Spanish, Language::German] {
            let d = dict_for(lang);
            let label = d["menu.edit.goto_heading"];
            let mine = key_of(label).unwrap_or_else(|| panic!("no access key {lang:?}"));
            for item in edit {
                assert_ne!(key_of(d[item]), Some(mine), "{item} of {lang:?} uses the same key as Go to Heading");
            }
            assert!(label.ends_with("+O"), "{label}");
        }
    }

    // Compare with Last Save: its access key is free in the Tab menu of
    // every language (the items of `editor::build`); no shortcut.
    #[test]
    fn compare_menu_access_key_is_free_in_every_language() {
        let key_of = |label: &str| -> Option<char> {
            let i = label.find('&')?;
            label[i + 1..].chars().next().map(|c| c.to_lowercase().next().unwrap_or(c))
        };
        let tab = ["menu.tab.new", "menu.tab.close", "menu.tab.rename", "menu.tab.next", "menu.tab.prev"];
        for lang in [Language::English, Language::French, Language::Spanish, Language::German] {
            let d = dict_for(lang);
            let label = d["menu.tab.compare"];
            let mine = key_of(label).unwrap_or_else(|| panic!("no access key {lang:?}"));
            for item in tab {
                assert_ne!(key_of(d[item]), Some(mine), "{item} of {lang:?} uses the same key as Compare");
            }
            assert!(!label.contains('\t'), "{label}");
            for key in ["compare.title", "compare.identical", "compare.failed", "compare.close"] {
                assert!(d.contains_key(key), "{key} missing for {lang:?}");
            }
            assert!(d["compare.too_large"].contains("{}"), "{lang:?}");
        }
    }

    // Edit > Lines (feature A3): its access key is free in the Edit menu and
    // its items have distinct keys, in every language.
    #[test]
    fn lines_menu_access_keys_are_unique_in_every_language() {
        let key_of = |label: &str| -> Option<char> {
            let i = label.find('&')?;
            label[i + 1..].chars().next().map(|c| c.to_lowercase().next().unwrap_or(c))
        };
        let edit = [
            "menu.edit.cut", "menu.edit.copy", "menu.edit.paste", "menu.edit.pasteplain", "menu.edit.find",
            "menu.edit.replace", "menu.edit.goto", "menu.edit.dupline", "menu.edit.delline",
            "menu.edit.case", "menu.edit.selectall", "menu.edit.timestamp",
        ];
        let lines = [
            "menu.edit.lines.sortasc", "menu.edit.lines.sortdesc", "menu.edit.lines.dedup",
            "menu.edit.lines.join", "menu.edit.lines.trim",
        ];
        for lang in [Language::English, Language::French, Language::Spanish, Language::German] {
            let d = dict_for(lang);
            let mine = key_of(d["menu.edit.lines"]).unwrap_or_else(|| panic!("no access key {lang:?}"));
            for item in edit {
                assert_ne!(key_of(d[item]), Some(mine), "{item} of {lang:?} uses the key of Lines");
            }
            let keys: Vec<Option<char>> = lines.iter().map(|k| key_of(d[k])).collect();
            for (i, k) in keys.iter().enumerate() {
                assert!(k.is_some(), "{} of {lang:?} has no access key", lines[i]);
                assert!(!keys[..i].contains(k), "{} of {lang:?} repeats an access key", lines[i]);
            }
        }
    }

    // Edit > Convert Case (feature A4): its access key is free in the Edit
    // menu, its items have distinct keys and show their shortcuts.
    #[test]
    fn convert_case_menu_access_keys_are_unique_in_every_language() {
        let key_of = |label: &str| -> Option<char> {
            let i = label.find('&')?;
            label[i + 1..].chars().next().map(|c| c.to_lowercase().next().unwrap_or(c))
        };
        let edit = [
            "menu.edit.cut", "menu.edit.copy", "menu.edit.paste", "menu.edit.pasteplain", "menu.edit.find",
            "menu.edit.replace", "menu.edit.goto", "menu.edit.dupline", "menu.edit.delline",
            "menu.edit.lines", "menu.edit.selectall", "menu.edit.timestamp",
        ];
        let items = ["menu.edit.case.upper", "menu.edit.case.lower", "menu.edit.case.title"];
        for lang in [Language::English, Language::French, Language::Spanish, Language::German] {
            let d = dict_for(lang);
            let mine = key_of(d["menu.edit.case"]).unwrap_or_else(|| panic!("no access key {lang:?}"));
            for item in edit {
                assert_ne!(key_of(d[item]), Some(mine), "{item} of {lang:?} uses the key of Convert Case");
            }
            let keys: Vec<Option<char>> = items.iter().map(|k| key_of(d[k])).collect();
            for (i, k) in keys.iter().enumerate() {
                assert!(k.is_some(), "{} of {lang:?} has no access key", items[i]);
                assert!(!keys[..i].contains(k), "{} of {lang:?} repeats an access key", items[i]);
            }
            let (upper, lower) = match lang {
                Language::German => ("\tStrg+Umschalt+U", "\tStrg+U"),
                _ => ("\tCtrl+Shift+U", "\tCtrl+U"),
            };
            assert!(d["menu.edit.case.upper"].ends_with(upper), "UPPERCASE shortcut of {lang:?}");
            assert!(d["menu.edit.case.lower"].ends_with(lower), "lowercase shortcut of {lang:?}");
            for key in ["case.error.title", "case.error.mapping"] {
                assert!(!d[key].is_empty(), "{key} of {lang:?}");
            }
            assert!(d["case.error.mapping"].contains("{msg}"), "case.error.mapping of {lang:?} shows the cause");
            assert!(!d["menu.edit.case.title"].contains('\t'), "Title Case has no shortcut");
        }
    }

    // Issue 058: the prompts panel is translated everywhere the vault is,
    // and both import confirmations show the prompt count.
    #[test]
    fn prompts_keys_exist_in_every_language() {
        for lang in [Language::English, Language::French, Language::Spanish, Language::German] {
            let d = dict_for(lang);
            for key in d.keys().filter(|k| k.starts_with("prompts.")).chain(["menu.toggle.prompts", "menu.tools.prompts"].iter()) {
                assert!(dict_for(Language::English).contains_key(key), "{key} of {lang:?} missing in English");
            }
            let count = d.keys().filter(|k| k.starts_with("prompts.")).count();
            assert_eq!(count, 27, "{lang:?}");
            for key in ["backup.import.confirm", "import_exe.confirm"] {
                assert!(d[key].contains("{prompts}"), "{key} of {lang:?}");
            }
            for key in ["prompts.copied.named", "prompts.inserted.named", "prompts.delete.confirm"] {
                assert!(d[key].contains("{name}"), "{key} of {lang:?}");
            }
            assert!(d["prompts.err.toolong"].contains("{max}"), "{lang:?}");
        }
    }

    #[test]
    fn unknown_key_returns_placeholder() {
        let _g = lang_guard();
        set_language(Language::English);
        assert_eq!(get("nonexistent.key"), "[nonexistent.key]");
    }

    #[test]
    fn unknown_key_placeholder_non_english() {
        let _g = lang_guard();
        set_language(Language::German);
        assert_eq!(get("totally.unknown"), "[totally.unknown]");
    }

    #[test]
    fn language_switching() {
        let _g = lang_guard();
        set_language(Language::English);
        assert_eq!(current_language(), Language::English);
        assert_eq!(get("close.save"), "Save");

        set_language(Language::French);
        assert_eq!(current_language(), Language::French);
        assert_eq!(get("close.save"), "Enregistrer");

        set_language(Language::Spanish);
        assert_eq!(current_language(), Language::Spanish);
        assert_eq!(get("close.save"), "Guardar");

        set_language(Language::German);
        assert_eq!(current_language(), Language::German);
        assert_eq!(get("close.save"), "Speichern");
    }

    #[test]
    fn language_codes() {
        assert_eq!(Language::English.code(), "en");
        assert_eq!(Language::French.code(), "fr");
        assert_eq!(Language::Spanish.code(), "es");
        assert_eq!(Language::German.code(), "de");
    }

    #[test]
    fn status_stats_has_placeholders() {
        let _g = lang_guard();
        set_language(Language::English);
        let s = get("status.stats");
        assert!(s.contains("{words}"));
        assert!(s.contains("{chars}"));
        assert!(s.contains("{lines}"));
    }

    #[test]
    fn all_languages_have_common_keys() {
        let common_keys = [
            "app.title", "menu.file", "menu.edit", "menu.help",
            "unlock.title", "createpwd.title", "settings.title",
            "close.save", "close.cancel", "search.find",
            "goto.title", "rename.title", "export.btn",
            "status.saved", "status.modified", "status.encrypted",
        ];
        for lang in [Language::English, Language::French, Language::Spanish, Language::German] {
            let dict = dict_for(lang);
            for key in &common_keys {
                assert!(
                    dict.contains_key(key),
                    "Language {:?} missing key '{}'", lang, key
                );
            }
        }
    }

    #[test]
    fn all_en_keys_present_in_all_languages() {
        let en = en_dict();
        for lang in [Language::French, Language::Spanish, Language::German] {
            let d = dict_for(lang);
            for key in en.keys() {
                assert!(
                    d.contains_key(key),
                    "Language {:?} missing key '{}'", lang, key
                );
            }
        }
    }

    /// Keys the code passes to `t` through a variable, not as a literal (the
    /// scan below cannot see them). Keep in sync with those call sites.
    const KEYS_PASSED_BY_VARIABLE: &[&str] = &[
        // ui/dialogs/drop_choice.rs: msg_key
        "drop.choice.text",
        "drop.choice.binary",
        // ui/dialogs/find_replace.rs: title_key
        "search.title",
        "search.find.title",
        // ui/editor/content.rs: save_indicator_text
        "status.saved",
        "status.modified",
        // ui/editor/ssh_ops.rs: the save failure after an ssh-agent change
        "ssh.agent.save_failed_still_loaded",
        "ssh.agent.save_failed",
        // ui/editor/helpers.rs: count_text (status bar)
        "status.word",
        "status.words",
        "status.char",
        "status.chars",
        "status.line",
        "status.lines",
        // ui/editor/content.rs: caret_status_text (count_text)
        "status.selected.one",
        "status.selected.many",
        // ui/editor/helpers.rs: tab_color_label, "color." + theme::TAB_COLORS name
        "color.blue",
        "color.green",
        "color.red",
        "color.orange",
        "color.purple",
        "color.yellow",
        "color.pink",
        "color.gray",
    ];

    /// Every literal key of a `t("...")` or `get("...")` call in the sources,
    /// with the file.
    fn keys_used_by_the_code() -> Vec<(String, String)> {
        fn walk(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
            for entry in std::fs::read_dir(dir).unwrap_or_else(|e| panic!("read {}: {}", dir.display(), e)) {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    walk(&path, out);
                } else if path.extension().map_or(false, |x| x == "rs") {
                    out.push(path);
                }
            }
        }
        let mut files = Vec::new();
        walk(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src"), &mut files);
        assert!(files.len() > 50, "sources not found: {} files", files.len());
        let mut keys = Vec::new();
        for file in files {
            let text = crate::read_source(&file);
            let b = text.as_bytes();
            // `t(...)` helpers, and direct `get(...)` calls (ui/vault_panel.rs).
            // This module's own tests ask `get` for unknown keys on purpose.
            let own_module = file.ends_with(std::path::Path::new("features").join("i18n").join("mod.rs"));
            for needle in ["t(", "get("] {
                if own_module && needle == "get(" {
                    continue;
                }
                let mut i = 0;
                while let Some(pos) = text[i..].find(needle) {
                    let at = i + pos;
                    i = at + needle.len();
                    let boundary = at == 0 || !(b[at - 1].is_ascii_alphanumeric() || b[at - 1] == b'_' || b[at - 1] == b'.');
                    if !boundary {
                        continue;
                    }
                    let rest = text[at + needle.len()..].trim_start();
                    // A key is `word.word...` (lowercase, digits, `_`): anything
                    // else after the `(` is the text of a comment or of a string.
                    if let Some(lit) = rest.strip_prefix('"') {
                        let key = &lit[..lit.find('"').unwrap_or(0)];
                        let is_key = key.contains('.')
                            && key.split('.').all(|w| !w.is_empty() && w.bytes().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'_'));
                        if is_key {
                            keys.push((key.to_string(), file.display().to_string()));
                        }
                    }
                }
            }
        }
        keys
    }

    /// `get` shows a placeholder for a key it does not know: every key the
    /// code asks for must exist in every language.
    #[test]
    fn every_key_the_code_asks_for_exists_in_every_language() {
        let mut keys = keys_used_by_the_code();
        assert!(keys.len() > 300, "too few t(\"...\") calls found: {}", keys.len());
        keys.extend(KEYS_PASSED_BY_VARIABLE.iter().map(|k| (k.to_string(), "KEYS_PASSED_BY_VARIABLE".to_string())));
        let mut missing = Vec::new();
        for lang in [Language::English, Language::French, Language::Spanish, Language::German] {
            let d = dict_for(lang);
            for (key, file) in &keys {
                if !d.contains_key(key.as_str()) {
                    missing.push(format!("{:?} {} ({})", lang, key, file));
                }
            }
        }
        assert!(missing.is_empty(), "keys missing:\n{}", missing.join("\n"));
    }

    /// Literal texts the UI may show untranslated: names and notations that
    /// are the same in every language.
    const UNTRANSLATED_UI_TEXT: &[&str] = &[
        "LockNote",
    ];

    /// `const` string lists of `src/ui` that are not translated on purpose:
    /// language names (each in its own language), font names, file
    /// extensions.
    const UNTRANSLATED_UI_LISTS: &[&str] = &["LANGUAGES", "FONT_FAMILIES", "FONT_SIZES", "MIN_PW_LENGTHS", "RISKY_EXTS", "BINARY_EXTS"];

    /// The `{name}` placeholders of a text, sorted (`{}` counted too).
    fn placeholders(text: &str) -> Vec<String> {
        let mut out = Vec::new();
        let mut rest = text;
        while let Some(open) = rest.find('{') {
            match rest[open..].find('}') {
                Some(close) => {
                    out.push(rest[open..open + close + 1].to_string());
                    rest = &rest[open + close + 1..];
                }
                None => break,
            }
        }
        out.sort();
        out
    }

    /// Pass 2: a translation that drops or renames a placeholder shows the
    /// raw `{name}` or loses the value. Every language has the placeholders
    /// of the English text.
    #[test]
    fn every_translation_keeps_the_english_placeholders() {
        let en = en_dict();
        let mut wrong = Vec::new();
        for lang in [Language::French, Language::Spanish, Language::German] {
            let d = dict_for(lang);
            for (key, text) in en {
                if let Some(tr) = d.get(key) {
                    if placeholders(text) != placeholders(tr) {
                        wrong.push(format!("{:?} {}: {:?} vs {:?}", lang, key, placeholders(text), placeholders(tr)));
                    }
                }
            }
        }
        wrong.sort();
        assert!(wrong.is_empty(), "{} texts:\n{}", wrong.len(), wrong.join("\n"));
    }

    /// `text` without the `#[cfg(test)]` items (test modules and helpers):
    /// their strings are never shown to a user.
    fn without_test_items(text: &str) -> String {
        let mut out = String::new();
        let mut rest = text;
        while let Some(pos) = rest.find("#[cfg(test)]") {
            out.push_str(&rest[..pos]);
            let after = &rest[pos..];
            // The item ends at its `;` (a `mod x;`) or at the brace closing
            // its first `{`, whichever comes first.
            let semi = after.find(';');
            let brace = after.find('{');
            let end = match (semi, brace) {
                (Some(s), Some(b)) if s < b => s + 1,
                (_, Some(b)) => {
                    let mut depth = 0usize;
                    let mut end = after.len();
                    for (i, c) in after[b..].char_indices() {
                        match c {
                            '{' => depth += 1,
                            '}' => {
                                depth -= 1;
                                if depth == 0 {
                                    end = b + i + 1;
                                    break;
                                }
                            }
                            _ => {}
                        }
                    }
                    end
                }
                (Some(s), None) => s + 1,
                (None, None) => after.len(),
            };
            rest = &after[end..];
        }
        out.push_str(rest);
        out
    }

    /// The top-level arguments of the call whose `(` is at the start of `s`.
    fn call_args(s: &str) -> Vec<&str> {
        let b = s.as_bytes();
        let mut args = Vec::new();
        let (mut depth, mut start, mut in_str, mut i) = (0usize, 1usize, false, 0usize);
        while i < b.len() {
            let c = b[i];
            if in_str {
                if c == b'\\' {
                    i += 1;
                } else if c == b'"' {
                    in_str = false;
                }
            } else {
                match c {
                    b'"' => in_str = true,
                    b'(' | b'[' | b'{' => depth += 1,
                    b')' | b']' | b'}' => {
                        depth -= 1;
                        if depth == 0 {
                            args.push(s[start..i].trim());
                            break;
                        }
                    }
                    b',' if depth == 1 => {
                        args.push(s[start..i].trim());
                        start = i + 1;
                    }
                    _ => {}
                }
            }
            i += 1;
        }
        args
    }

    /// The text of a string literal starting `s` (`"..."`, or the format
    /// string of `format!("...")`), `None` for anything else.
    fn literal_text(s: &str) -> Option<&str> {
        let s = s.trim_start_matches('&').trim_start();
        let s = s.strip_prefix("format!(").map(str::trim_start).unwrap_or(s);
        let lit = s.strip_prefix('"')?;
        let mut end = 0;
        let b = lit.as_bytes();
        while end < b.len() && b[end] != b'"' {
            if b[end] == b'\\' {
                end += 1;
            }
            end += 1;
        }
        Some(&lit[..end.min(lit.len())])
    }

    /// Text a user reads: letters once the `{...}` placeholders, escapes and
    /// URLs are left out, and not a name of `UNTRANSLATED_UI_TEXT`.
    fn is_user_text(lit: &str) -> bool {
        // Addresses are not translated: drop each `http(s)://...` up to a
        // space or an escape.
        let mut no_urls = String::new();
        let mut rest = lit;
        while let Some(pos) = rest.find("http") {
            no_urls.push_str(&rest[..pos]);
            let tail = &rest[pos..];
            if tail.starts_with("http://") || tail.starts_with("https://") {
                let end = tail.find(|c: char| c.is_whitespace() || c == '\\').unwrap_or(tail.len());
                rest = &tail[end..];
            } else {
                no_urls.push_str("http");
                rest = &tail[4..];
            }
        }
        no_urls.push_str(rest);
        let lit = no_urls.as_str();
        let mut words = String::new();
        let mut depth = 0;
        let mut chars = lit.chars().peekable();
        while let Some(c) = chars.next() {
            match c {
                '{' => depth += 1,
                '}' => depth -= 1,
                // `\n`, `\u{...}`: an escape, not a word.
                '\\' => {
                    if let Some('u') = chars.next() {
                        for c in chars.by_ref() {
                            if c == '}' {
                                break;
                            }
                        }
                    }
                }
                _ if depth == 0 => words.push(c),
                _ => {}
            }
        }
        let mut rest = words;
        for name in UNTRANSLATED_UI_TEXT {
            rest = rest.replace(name, "");
        }
        rest.chars().any(|c| c.is_alphabetic())
    }

    /// Hard-coded texts handed to the UI in `src/ui`: arguments of the calls
    /// that set a visible text, and the `title` / `content` of message boxes.
    fn hard_coded_ui_texts() -> Vec<String> {
        fn walk(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
            for entry in std::fs::read_dir(dir).unwrap_or_else(|e| panic!("read {}: {}", dir.display(), e)) {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    walk(&path, out);
                } else if path.extension().map_or(false, |x| x == "rs") {
                    out.push(path);
                }
            }
        }
        let mut files = Vec::new();
        walk(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src").join("ui"), &mut files);
        assert!(files.len() > 30, "ui sources not found: {} files", files.len());
        // Test-only files (declared under `#[cfg(test)]`).
        files.retain(|f| {
            let name = f.file_name().unwrap().to_string_lossy();
            name != "tests.rs" && name != "snapshot.rs"
        });
        let calls = [
            ".text(", ".title(", "set_text(", ".filters(",
            "modal_info_message(", "modal_error_message(", "modal_warning_message(",
            "modal_fatal_message(", "simple_message(", "error_message(",
            "input_dialog(",
        ];
        let fields = ["title:", "content:"];
        let mut found = Vec::new();
        for file in files {
            let text = without_test_items(&crate::read_source(&file));
            let rel = file.strip_prefix(env!("CARGO_MANIFEST_DIR")).unwrap().display().to_string();
            let line_of = |at: usize| text[..at].matches('\n').count() + 1;
            for needle in calls {
                let mut i = 0;
                while let Some(pos) = text[i..].find(needle) {
                    let at = i + pos;
                    i = at + needle.len();
                    let open = at + needle.len() - 1;
                    for arg in call_args(&text[open..]) {
                        if let Some(lit) = literal_text(arg) {
                            if is_user_text(lit) {
                                found.push(format!("{}:{}: {}", rel, line_of(at), lit));
                            }
                        }
                    }
                }
            }
            for needle in fields {
                let mut i = 0;
                while let Some(pos) = text[i..].find(needle) {
                    let at = i + pos;
                    i = at + needle.len();
                    if let Some(lit) = literal_text(text[i..].trim_start()) {
                        if is_user_text(lit) {
                            found.push(format!("{}:{}: {}", rel, line_of(at), lit));
                        }
                    }
                }
            }
            // `const X: &[&str] = &[...]` lists: the items of combo boxes
            // (pass 2: "Off" / "10 sec" were missed). Menus built with
            // AppendMenuW get their label from a variable, which this scan
            // cannot follow: their texts are covered by
            // `tab_color_labels_are_translated` and the key tests.
            let mut i = 0;
            while let Some(pos) = text[i..].find(": &[&str] = &[") {
                let at = i + pos;
                let name_start = text[..at].rfind(|c: char| !(c.is_ascii_uppercase() || c == '_')).map_or(0, |p| p + 1);
                let name = &text[name_start..at];
                let body_start = at + ": &[&str] = &[".len();
                let body_end = text[body_start..].find("];").map_or(text.len(), |e| body_start + e);
                i = body_end;
                if UNTRANSLATED_UI_LISTS.contains(&name) {
                    continue;
                }
                for item in text[body_start..body_end].split(',') {
                    if let Some(lit) = literal_text(item.trim()) {
                        if is_user_text(lit) {
                            found.push(format!("{}:{}: {} in {}", rel, line_of(at), lit, name));
                        }
                    }
                }
            }
        }
        found
    }

    /// Issue #25: every text the UI shows goes through `t` and exists in the
    /// four languages. A literal handed straight to a control or a message
    /// box stays in English whatever the language.
    #[test]
    fn the_ui_hands_no_hard_coded_text_to_its_controls() {
        let found = hard_coded_ui_texts();
        assert!(found.is_empty(), "{} hard-coded UI texts (use t(\"key\")):\n{}", found.len(), found.join("\n"));
    }

    /// The scan itself finds the forms it looks for, and skips tests.
    #[test]
    fn the_hard_coded_text_scan_sees_literals_and_formats() {
        assert_eq!(call_args("(&self.window, \"Error\", &msg)"), ["&self.window", "\"Error\"", "&msg"]);
        assert_eq!(literal_text("&format!(\"Version {}\", V)"), Some("Version {}"));
        assert_eq!(literal_text("&t(\"about.title\")"), None);
        assert!(is_user_text("Wrong password"));
        assert!(!is_user_text("LockNote"));
        assert!(!is_user_text("{} / {}"));
        assert!(!is_user_text("\u{1F6E1}"));
        assert!(!is_user_text("\\u{1F6E1}"));
        assert!(!is_user_text("https://github.com/x"));
        assert!(!is_user_text("{}\n\nhttps://github.com/{}\nhttps://uitguard.com"));
        assert!(is_user_text("see https://x.com now"));
        let src = "a(\"x\");\n#[cfg(test)]\nmod tests {\n fn f() { b(\"y\"); }\n}\nc();\n#[cfg(test)]\nmod snap;\nd();";
        assert_eq!(without_test_items(src), "a(\"x\");\n\nc();\n\nd();");
    }

    // CURRENT_LANG is an AtomicU8 read by `get()` on every translation lookup
    // and written by `set_language()` whenever the user switches language.
    // This test hammers both from multiple threads to confirm: no data race,
    // no panic, and `get()` always returns a string from SOME valid language
    // (i.e. never corrupted / garbage).
    #[test]
    fn concurrent_set_and_get_is_thread_safe() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::thread;

        let _g = lang_guard();
        let langs = [Language::English, Language::French, Language::Spanish, Language::German];

        // Collect every valid localized spelling of `close.save` — any thread
        // observation of t("close.save") must equal one of these.
        let mut valid: Vec<String> = Vec::new();
        for lang in langs {
            set_language(lang);
            valid.push(get("close.save").to_string());
        }

        let stop = Arc::new(AtomicBool::new(false));
        let valid = Arc::new(valid);

        let writer = {
            let stop = Arc::clone(&stop);
            thread::spawn(move || {
                let mut i = 0usize;
                while !stop.load(Ordering::Relaxed) {
                    set_language(langs[i % langs.len()]);
                    i += 1;
                }
            })
        };

        let readers: Vec<_> = (0..3)
            .map(|_| {
                let stop = Arc::clone(&stop);
                let valid = Arc::clone(&valid);
                thread::spawn(move || {
                    let mut iters = 0u64;
                    while !stop.load(Ordering::Relaxed) {
                        let v = get("close.save").to_string();
                        assert!(
                            valid.contains(&v),
                            "concurrent read returned unexpected value {:?}; expected one of {:?}",
                            v, valid
                        );
                        iters += 1;
                    }
                    iters
                })
            })
            .collect();

        // Let the threads hammer for a short window.
        thread::sleep(std::time::Duration::from_millis(80));
        stop.store(true, Ordering::Relaxed);

        writer.join().unwrap();
        let total: u64 = readers.into_iter().map(|h| h.join().unwrap()).sum();
        assert!(total > 0, "readers must have observed at least one read");
    }

    // Regression guard: `createpwd.{weak,fair,strong,verystrong}` are displayed
    // in the password-strength meter of the create-password dialog. Previously
    // only the EN variants had test coverage; here we lock down the FR
    // translations so a future FR dict edit cannot silently break the UI.
    #[test]
    fn password_strength_labels_french() {
        let _g = lang_guard();
        set_language(Language::French);
        // Lock down the exact FR strings shown to users. If these change, the
        // dialog-level tests in create_password.rs must be updated to match.
        assert_eq!(get("createpwd.weak"), "Faible");
        assert_eq!(get("createpwd.fair"), "Moyen");
        assert_eq!(get("createpwd.strong"), "Fort");
        assert_eq!(get("createpwd.verystrong"), "Tr\u{00e8}s fort");
    }

    /// The guard puts English back when dropped, even after a failure.
    #[test]
    fn lang_guard_resets_english_on_drop() {
        let r = std::panic::catch_unwind(|| {
            let _g = lang_guard();
            set_language(Language::French);
            panic!("assert failed");
        });
        assert!(r.is_err());
        let _g = lang_guard();
        assert_eq!(current_language(), Language::English);
    }

    // The vault tree error node (issue 048) must be translated.
    #[test]
    fn vault_tree_error_french() {
        let _g = lang_guard();
        set_language(Language::French);
        assert_eq!(get("vault.tree.error"), "Impossible d'afficher le coffre");
    }
}
