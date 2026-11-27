# Changelog

All notable changes to LockNote are documented in this file.
Up to 1.2.27, entries note the edition they affect: `(Free + Pro)` or
`(Pro)`. From 2.0.0 there is one edition.

## [Unreleased]

### Added
- **Command palette** (Ctrl+Shift+P, no menu item): a filter field above a
  list of every menu command (with its shortcut), one "Tab: <name>" row per
  tab, one "Vault: <name>" row per secret and one "Prompt: <name>" row per
  prompt. Fuzzy, case-insensitive filtering ("dl" finds "Delete Line").
  Up/Down move, Page Up/Page Down move one visible page, Enter or a double
  click on a row runs it, Esc closes. A command row does what its menu item
  does, a tab row switches to it, a vault row copies the secret as a click in
  the vault panel does (the value is never shown), a prompt row inserts the
  prompt into the note. Two secrets with the same name show their category.
  If the note locks, or the chosen tab, secret or prompt changes while the
  palette is open, nothing happens. Not available while locked.
- **Search All Tabs** (Edit > Search All Tabs..., Ctrl+Shift+F): searches
  every tab (the active one with its unsaved changes), the names and
  categories of the vault secrets (never their values) and the prompts.
  Results "<tab> : <line> : <excerpt>", "Vault: <category> / <name>",
  "Prompt: <name>"; Match case; at most 1000 results. Go to, Enter or a
  double click opens the tab at the match, or the vault or prompts panel on
  the item.
- **Regular expressions in Find and Find & Replace**: a "Regular expression"
  box (not saved, unchecked at each opening). Multi-line `^`/`$`, `\n` for a
  line break, Match case and Whole word still apply, replacement groups `$1`,
  `${2}`, `${name}`, `$$`. An invalid pattern shows "Invalid regular
  expression". Unicode property classes (`\p{L}`) are not available.
- **Edit > Go to Heading...** (Ctrl+Shift+O): lists the Markdown headings
  of the active tab (`#` to `######`, setext `===`/`---`, fenced code blocks
  ignored) with a filter field; Enter or a double click moves to the heading.
- **Tab > Compare with Last Save...**: a read-only unified line diff of the
  active tab against its text as of the last save (or as loaded), with 3
  lines of context. "No changes since the last save." when identical; tabs
  over 20000 lines are not compared. Nothing is written to disk.
- **Line operations**: Alt+Up / Alt+Down move the current or selected lines;
  new Edit > Lines submenu: Sort Lines Ascending / Descending (Windows locale,
  case and accents ignored, stable), Remove Duplicate Lines, Join Lines, Trim
  Trailing Whitespace. They act on the selected lines, or on the whole tab
  without a multi-line selection. One Ctrl+Z per operation.
- **Edit > Convert Case**: UPPERCASE (Ctrl+Shift+U), lowercase (Ctrl+U),
  Title Case (menu only), on the selection or the word under the caret. One
  Ctrl+Z per conversion. If the save shortcut is Ctrl+U, Ctrl+U saves.
- **Block indent / outdent**: Tab on a selection of two lines or more adds a
  tab at the start of each line; Shift+Tab removes one tab (or up to 4
  spaces) from each line, or from the current line without a multi-line
  selection. One Ctrl+Z per indent or outdent.
- **Numbered lists and checkboxes**: Enter on a "N. " or "N) " line starts
  the next number; Enter on a "[ ] " / "[x] " line starts an unchecked
  "[ ] " item; Enter on an empty item ends the list. Ctrl+Enter toggles the
  checkbox of the line, or adds "[ ] ". Lists stay plain text.
- **Zoom**: Ctrl+mouse wheel, Ctrl+= / Ctrl+- (and numpad + / -), 1 pt per
  step between 6 and 48 pt; Ctrl+0 returns to the size set in Preferences.
  Session only: never saved, not an edit, not an undo step.
- **Caret position in the status bar**: "Ln X, Col Y", plus "(N selected)"
  when text is selected.
- **Prompt variables and Insert**: `{date}`, `{time}`, `{datetime}`
  (local time), `{clipboard}` and `{tab}` are expanded when a prompt is copied
  or inserted; `{{` and `}}` write a brace. The prompts panel has a new
  **Insert** button that puts the expanded prompt into the note at the caret
  (one Ctrl+Z).

### Changed
- **Printing** uses the editor font and size, wraps long lines to the page,
  prints only the selection when there is one, and shows the tab name at the
  top and "Page N / M" at the bottom of each page. A print error (dialog,
  printer driver, page too small for the font) is now shown in a message
  instead of printing nothing.
- **Editor font applied at once**: a font family or size chosen in
  Preferences, or taken from an imported backup, applies on OK, without
  restarting. If it cannot be applied, a message says the editor keeps its
  current font.
- **Undo and tab switches**: the undo history is emptied each time the
  editor shows another tab, so Ctrl+Z / Ctrl+Y can never bring one tab's
  text into another. The history of the tab you left is not kept.
- **Ctrl+= no longer toggles subscript** and RichEdit's own Ctrl+wheel zoom
  is replaced by LockNote's zoom. AltGr combinations (AltGr+0, AltGr+=) still
  type their characters.
- **File > Save shortcut text** is the same in the File menu, the command
  palette and Preferences (a function key reads "Ctrl+F5").
- **Windows before decryption and names of a new note translated**: the
  windows shown before the notes are decrypted (create password, unlock, the
  "two versions of your notes" question, the startup errors) now follow the
  language of Windows when it is English, French, Spanish or German, English
  otherwise; once the notes are decrypted, the language chosen in the note
  applies as before. A new note is created in that language: its language
  setting, its first tab ("Nota 1", "Notiz 1") and its vault categories
  (General keeps its name). New tabs are named in the current language
  ("Notiz 3"). Names already stored never change: an existing note keeps its
  tabs and categories as they are, in every language. Still in English: the
  "SSH Keys" category of an SSH key import, and the technical details
  appended to some error messages (#34).
- **2FA secret in memory**: the copies made by the Security dialog and when
  a TOTP key is pasted are now wiped from memory after use (#35).
- **Insert Timestamp (F5) in local time**: the inserted date and time are now
  the local time of the computer (time zone and daylight saving time), as the
  `{date}`, `{time}` and `{datetime}` prompt variables, instead of UTC. Same
  format `YYYY-MM-DD HH:MM:SS`, no time zone shown.
- **2FA secret in memory**: the remaining copies (the previous secret when it
  is replaced, the TOTP setup and copy, each code refresh) are wiped too (#38).

### Fixed
- **Windows logoff during a close answered "Don't save"**: a logoff while a
  message of that close was still shown (the "Remember my choice" notice,
  an SSH key error) saved the changes the user had just declined. The answer
  "Don't save" (or "Close without saving" after a failed save) now stands; a
  close question not answered yet still saves, as before (#33).
- **Shutdown blocked for an unmodified note**: a failed save at logoff
  blocked the shutdown even with no unsaved changes, where only the window
  position was at stake. It is now logged and the shutdown goes on. The
  same false "could not save" block could follow a close that had already
  saved, when the logoff arrived as LockNote was closing (#33).
- **Select All with emoji**: Edit > Select All / Ctrl+A now selects the whole
  text when it contains emoji or other characters outside the BMP (the end
  was cut short by one position per such character).
- **Test suite**: the tests that drive a real RichEdit control (`live_*`) no
  longer run at the same time. Observed (Microsoft does not document it):
  several test threads using RichEdit at once sometimes killed the test
  binary with an access violation in `gdi32full`, or made a test read wrong
  font metrics. The application is not affected:
  it uses RichEdit from its one UI thread only.
- **Undo history left suspended**: a font change (zoom, Preferences), a
  theme change or the occurrence highlights pause the undo history while
  they run. If Windows refuses to resume it (after one retry), a message now
  says that the next changes might not be undoable and asks you to save the
  note, instead of only writing it to the log. It is shown once, after the
  zoom or the highlighting is over (not while LockNote is locked or another
  dialog is open), and again only if the history came back in between.
  A lock that closes the message before it is read shows it again after
  unlocking.
- **Auto-lock while typing in a dialog**: typing, clicking or moving the
  mouse in a LockNote window or message (editing a long vault secret, a
  prompt, a TOTP name, a question) did not count as activity, so the
  auto-lock could close the window and lose what was typed. It now counts,
  as in the main window; input in other applications still does not. A
  window left open with nobody at it is still closed when the delay runs
  out, and nothing typed under the "Notes hidden" screen postpones a lock
  already under way (#32).
- **Swap helper slow to start at exit**: when the helper that writes the
  last save into the program file is not ready within 15 s (a heavily
  loaded computer), LockNote now ends it before letting go of the program
  file, instead of leaving it running. Left running, it could act later
  for a session that was over, behind the next LockNote session: a false
  "swap failed" in the log, or a restart nobody asked for any more. Notes
  were never at risk; the save is still written into the program file at
  the next exit. The `swap-error.log` line now says "it was ended, nothing
  touched" instead of "it may still run" (#39).

## [2.1.1] - 2026-11-26

### Changed
- Dependencies updated to their current versions (crypto, Windows API,
  updater, build actions); ssh-encoding stays on 0.2, the version ssh-key
  uses, and the compression keeps its backend (miniz_oxide). No change in
  behaviour or in the files written. Two unused dependencies removed.

## [2.1.0] - 2026-11-26

### Added
- **TOTP: add the secret of an existing website**: Tools > TOTP > Add Key
  now lets you paste the secret given by the site, as Base32 text (spaces
  and lowercase accepted) or as an `otpauth://totp/` link; **Generate**
  still creates a new secret. An invalid secret is refused with the reason
  and never stored: not Base32, cut or pasted twice (impossible length),
  shorter than 80 bits, a link pasted twice, a link giving a parameter
  twice (two secrets), or a link asking for codes LockNote does not make
  (other than SHA1, 6 digits, 30 s) (#23).
- **View > Line Numbers**: shows or hides the line-number gutter; the text
  takes the freed width. The choice is the existing `show_line_numbers`
  setting, saved with the note and restored at the next start (#26).

### Fixed
- **Windows logoff or shutdown lost unsaved changes**: LockNote now saves
  the notes when the session ends (unless On close is "Never save"), then
  exits as on a normal close (ssh-agent keys removed, pending save
  promoted). A failed save blocks the shutdown with the reason instead of
  losing the text (#29).
- **Auto-save and auto-lock under an open dialog**: auto-save kept running
  while a dialog or a message of LockNote was open (it could write changes
  then declined); it now waits and restarts its delay when the dialog
  closes. Auto-lock could wipe the password under an open dialog (#30).
  Now, as soon as a lock is due (auto-lock, Lock button, Ctrl+L), the
  notes are hidden at once and only the password shows them again; every
  open LockNote window or question gets its harmless answer (Cancel, No,
  Keep open) and an answer clicked afterwards does nothing. LockNote
  saves and locks for good once no window is open; if the save fails,
  nothing is lost: the notes stay hidden with their changes, the save is
  retried every 10 seconds, and the password brings them back with the
  reason. The hidden screen names the window LockNote still waits for,
  auto-save waits while the notes are hidden, and a password being typed
  there is kept when the lock completes. LockNote cannot be closed while
  the notes are hidden (a Windows logoff follows the rule above). While
  locked the menus only allow Quit and About. A Quit still waiting for a
  window after 10 seconds names it and offers "Keep waiting" or "Close
  without saving"; that offer and the close question stay open when Quit
  is clicked again (#30).
- **"Copied" shown after a failed copy**: the TOTP panel and dialogs and
  the vault said "Copied" even when Windows refused the clipboard write.
  Now a failed copy says "Copy failed: ..." and "Copied" appears only after
  a successful copy. A clipboard auto-clear that fails is retried (#31).
- **Texts left in English**: the password confirmation (print, export), the
  vault delete confirmation, the update messages, the save and export
  errors, the status bar counts, the file dialog filters, the attachment
  errors, the lock screen buttons, the TOTP dialogs, the tab colour names and
  the Off / sec / min items of the auto-save, auto-lock and clipboard lists
  are now translated in French, Spanish and German. Still in English: the
  windows shown before the notes are decrypted (the language is stored
  inside them), the default vault categories and tab names of a new note
  (they are saved as data), and the system error details appended to some
  messages. A test fails if a new hard-coded text is handed to a control, a
  message box or a `const` list of the UI (#25).
- **Close prompt**: the English "Unsaved Changes" message box is replaced by
  the translated close dialog with **Remember my choice**. Remembering Save
  sets On close to Always save. Remembering Don't Save cannot be stored (the
  settings are inside the note, which is not saved) and LockNote says so.
  Cancel never changes the setting (#27).
- **A failed save at close lost the changes**: the save error was shown,
  then LockNote closed anyway. Now a save that fails while closing asks
  whether to keep LockNote open (the default, text and password intact, to
  retry or copy) or to close without saving (#22).
- **F2 renames the tab**: the Tabs menu showed F2 next to "Rename Tab..."
  but the key did nothing. F2 in the vault tree still does nothing (#24).
- **About box**: Help > About LockNote... is now translated and its crypto
  line comes from the crypto constants (Argon2id 64 MiB, 3 iterations,
  4 lanes), so it cannot drift from them. An unused About dialog of the
  code still said "PBKDF2 300k iterations"; it was never shown and is
  removed (#28).

## [2.0.0] - 2026-11-25

LockNote is free and open source (GPL-3.0-or-later), in one edition that
holds every feature of the former Pro edition. The format of the notes does
not change: 2.0 reads the exes and the `.lnbak` backups of 1.2.x.

### Changed
- **One edition, one program**: `LockNote.exe`, with tabs, vault, prompts,
  TOTP, attachments, SSH keys, safety backups, auto-lock, auto-save,
  clipboard auto-clear, global hotkey and the 4 languages. No licence key, no
  activation dialog; the About box has no licence lines any more.
- **Updates** come from the releases of `ctardy/LockNote`
  (`LockNote-windows-x64.zip`, signed `SHA256SUMS.txt`). LockNote 1.2.1
  updates itself to 2.0 and keeps its notes. LockNote Pro 1.2.x does not see
  2.0: migrate by hand (README, "Migrating from LockNote 1.2.x").
- **Defaults** of a new note are those of the former Pro edition (minimum
  password length 6); the settings of existing notes read back unchanged.

- **Update every LockNote copy you have.** A LockNote 1.2.1 started from
  any path deletes, at startup, the pending `.tmp` files of the other copies
  in `%LOCALAPPDATA%\LockNote` (those older than one minute), and a
  `.tmp` may be the only copy of their latest save. Do not keep 1.2.1 copies
  next to 2.0 ones.

### Removed
- The licence system (key check, `license.dat`, activation dialog, key
  generator). `%LOCALAPPDATA%\LockNote\license.dat` is no longer
  read and can be deleted by hand.

### Known limitations
- **Notes of LockNote 1.2.1 holding a block header line**: 1.2.1 saved the
  note as plain text, where a line made only of a LockNote block header
  (`[LOCKNOTE_PROMPTS]`, `[LOCKNOTE_VAULT]`, `[LOCKNOTE_ATTACHMENTS]`,
  `[LOCKNOTE_HISTORY]`, `[LOCKNOTE_STATE]`, or `[LOCKNOTE_TABS]` as the first
  line) was ordinary text. 2.0 reads such a line as the start of that block,
  as LockNote Pro 1.2.27 already did, so the rest of the note after that line
  may be dropped at the next save. Before the update, check that no line of a
  1.2.1 note is exactly one of these headers (change it, e.g. add a
  character).

## [1.2.27] - 2026-11-19

### Added
- **Vault search (Pro)**: a search field at the top of the Vault panel
  filters the secrets as you type, on the name only (case and accents
  ignored; never the value). Categories without a match are hidden while
  searching.

### Fixed
- **Restart after an update (Free + Pro)**: answering Yes to "Restart
  LockNote now?" closed LockNote but never started it again. The swap helper
  now starts the new version once it has replaced the program file (only if
  the replacement succeeded and the previous LockNote has exited; otherwise
  the reason is in `swap-error.log`). When the update cannot be applied at
  that exit, a message says so instead of closing silently.
  The restart is done by the version being left: the update *to* this version
  does not restart yet, the following ones do.

## [1.2.26] - 2026-11-19

### Changed
- **Prompts: one click copies (Pro)**: a click on a prompt copies it to the
  clipboard, also when it is already selected (it used to take a
  double-click). Moving with the arrow keys only shows the preview.

## [1.2.25] - 2026-11-19

Once prompts are saved, do not go back to 1.2.24: it keeps the tabs, vault
and attachments but does not reliably keep the prompts (issue 058).

### Added
- **Prompts panel (Pro)**: a new **Prompts** button (left of Vault) and
  **Tools > Prompts** open a side panel to store prompts and copy them in one
  click. Search on the name and the text (case and accents ignored), list
  sorted by category then name, read-only preview, Add / Edit / Delete / Copy,
  double-click to copy. A prompt is not a secret: the clipboard is not cleared
  after a copy. Prompts are stored encrypted in the note, travel in full
  backups (a backup made by an older version simply has none) and are cleared
  from memory on lock. Once prompts are saved, do not go back to 1.2.24: it
  keeps the tabs, vault and attachments, but does not reliably keep the
  prompts (issue 058).
- **Resizable side panels (Pro)**: the Vault, TOTP and Attach panels can be
  widened or narrowed by dragging the thin band between the editor and the
  panel. Each panel remembers its own width; the editor always keeps at least
  200 px. The width is kept for the session (lock and unlock, imports) and
  written with the next save of the note (Ctrl+S, auto-save, save on close).
  Resizing alone does not mark the note modified: closing without saving
  (answering No, or "never save") does not keep the new width. The widths
  belong to this computer: they are not part of a full backup and an import
  keeps the current ones.

### Fixed
- **Vault: Edit after an import (Pro)**: if the vault was replaced while the
  Edit dialog was open, the change no longer lands on another secret; a
  message says nothing was saved.
- **Attach panel position (Pro)**: since the resizable panels, the Attach
  panel covered the resize grip and left a 5 px strip on the right.
- **Black square around the Vault, TOTP and Attach button labels (Pro)**:
  after a click on one of these buttons, its keyboard focus ring was drawn
  once the keyboard had been used (often black on the open panel's button).
  The ring now shows only when the keyboard drives the button (Tab, Enter,
  Space, arrows); shortcuts such as Ctrl+S do not bring it back.

## [1.2.24] - 2026-11-17

Upgrading an exe 1.2.23 that runs from another drive than the system one
(USB key, second disk, network share) with a save still waiting: do not
accept the update offered by 1.2.23 (its swap helper cannot write to that
drive); put the clean `LockNote-Pro.exe` of this release in place of the
exe: it takes over the waiting save and writes it into the exe when it
closes.

### Added
- **Path of the LockNote file in the status bar (Free + Pro)**: between the
  counters and the save indicator, shortened in the middle when space is short.
  A click copies the full path.
- **File > Settings > Temporary files... (Free + Pro)**: explains what the
  temporary files under `%LOCALAPPDATA%\LockNote` are and how to reach them,
  lists the pending saves with the LockNote file each one belongs to, and offers
  to open the folder or copy its path, delete an orphan pending save (its
  LockNote file is gone while its folder is still there; with a confirmation,
  never the open file's), close LockNote now to write a pending save, and open
  the swap and import logs. To tell an orphan from a drive letter that now
  names another disk, the companion file of each pending save records the
  serial number of the volume of its LockNote file (new optional
  `volume=` line); without it, a pending save is never offered for deletion.
  The swap helper reads that line too: it must be built from this version.

### Fixed
- **Notes never written into the program file when it is not on the system
  drive (Free + Pro)**: with LockNote on a USB key, a second disk or a network
  share, every save stayed in the staging file under `%LOCALAPPDATA%\LockNote`
  of that computer (the move at exit failed with "not the same device") and
  never reached the `.exe`. The notes did not travel with the file. The helper
  now copies the staging file next to the `.exe`, checks the copy, and renames
  it over the `.exe`.
- **Pending save of one LockNote file deleted by another (Free + Pro)**:
  starting any LockNote removed every staging file older than a minute that was
  not its own, that is, the unsaved-to-exe notes of every other LockNote file on
  the computer. A staging file is no longer deleted by another file, and never
  on a guess.
- **Pending save not applied on some ways out (Free + Pro)**: closing while
  locked, five wrong passwords on the lock screen, or cancelling a startup
  dialog left the staging file where it was, and it was only applied after a
  later session that saved. It is now applied at the end of every session,
  including when it was left by an earlier one. A helper that cannot be started
  is logged (`swap-error.log`) instead of being ignored.
- **Staging file written in place (Free + Pro)**: a save interrupted half-way
  could leave a truncated staging file in place of the previous one. It is now
  written to a separate file and renamed once complete; the update download
  uses the same write.
- **Text lost after a line holding an internal marker (Pro)**: a note, or the
  value of a vault secret, with a line such as `[LOCKNOTE_VAULT]`,
  `[LOCKNOTE_ATTACHMENTS]`, `[LOCKNOTE_HISTORY]` or a whole `[LOCKNOTE_STATE]`
  block was cut there at the next reload: the rest of the tab, the following
  tabs or the following secrets were gone. The same happened when the marker
  was in the middle of a line. Such lines are now saved escaped, whatever the
  block they are written in, and a block is only recognized on a line of its
  own. A note with a single default tab is still saved as plain text, exactly as
  typed (as before, line breaks at its very start, or at its end when a vault,
  attachments or a session state follow, are not kept on reload), except when
  that text could itself be read as a block: `[LOCKNOTE_VAULT]`,
  `[LOCKNOTE_ATTACHMENTS]` or `[LOCKNOTE_HISTORY]` alone between line breaks
  (a lone carriage return counts), a whole `[LOCKNOTE_STATE]` block ending it,
  `[LOCKNOTE_TABS]` starting it. It is then saved in the tabs format.
  Files saved by earlier versions are read as before; a text they already cut
  on a marker line of its own stays cut when read (a marker inside a line is
  now read whole). Older versions reading a file saved by this one still cut at
  those lines. Full backups (`.lnbak`) were never affected and their format is
  unchanged.

### Changed
- **Program file changed elsewhere while a save was pending (Free + Pro)**:
  when the `.exe` no longer holds the notes the pending save was based on
  (written by another computer, replaced by hand, copied over), LockNote asks
  at startup, before the password, which version to keep, and shows both
  dates. The other version is moved to `%LOCALAPPDATA%\LockNote\conflicts` as a
  LockNote file (readable with File > Import from another LockNote... in Pro)
  and is never deleted automatically. Before, the pending save was used
  silently. This is decided by content, never by file dates, which a file
  server or a clock change can put in any order: a companion file next to the
  staging file records which notes the save was based on. A staging file left
  by 1.2.23 or earlier, which has none, is judged once by dates at startup and
  then taken up. When both hold the same notes, the staging file is simply
  dropped.
- **Program part of a pending save (Free + Pro)**: a pending save no longer
  brings an older program back over a newer `.exe` (copied with an older date,
  for instance); only a program downloaded by the update keeps its place. An
  update downloaded while the file held no notes yet is now applied, and the
  first save keeps the new program.
- **One swap at a time per program file (Free + Pro)**: LockNote waits at
  startup (up to a minute) while the save of the previous session is still
  being written into the `.exe`, and says so instead of starting over it; two
  such writes of the same file never overlap. Right before it replaces the
  `.exe`, the swap checks again that the `.exe` still holds the notes the
  save was based on; if not (another computer wrote it meanwhile), it leaves
  everything in place and the question comes at the next start.
- **Staging file that cannot be read (Free + Pro)**: when the pending save, or
  the companion file next to it, is held by another program or is not a
  regular file, LockNote now stops at startup with a message naming it,
  without changing anything, instead of opening the notes of the `.exe` and
  later setting that file aside (or, for the companion, asking which version
  to keep as if the `.exe` held a different one).
- **Import from a LockNote file whose pending save cannot be read (Pro)**: File
  > Import from another LockNote... now refuses with "Cannot read the file:"
  and the path of that staging file (or of its companion), and imports
  nothing, instead of importing the older notes of that `.exe` without a word.

## [1.2.23] - 2026-11-16

### Added
- **Password generator (Pro)**: Tools > Password Generator..., and a "Generate..."
  button in the vault's secret dialog whose "Use" fills the value. Length 8 to 64
  (16 by default), uppercase / lowercase / digits / symbols, an editable list of
  allowed symbols, optional exclusion of look-alike characters (`0 O o 1 l I |`).
  The password always holds at least one character of every ticked type. It is
  masked until "Show"; "Copy" puts it on the clipboard, cleared like a vault
  copy. The options are remembered with the note and travel in a full backup.

### Fixed
- **No taskbar entry at launch (Free + Pro)**: since 1.2.20 the unlock window shown
  at startup had no taskbar entry, so LockNote was nowhere in the taskbar until
  the password was typed. It is hidden from the taskbar only when it opens over
  the main window (unlock after a manual lock), as intended.

## [1.2.22] - 2026-11-16

### Fixed
- **Pale frame around the editor (Free + Pro)**: the editor no longer carries the
  system border and sunken edge, drawn in fixed greys and white whatever the
  theme (a white line on the right and bottom in the Dark theme).
- **Grey rectangle inside the password field (Free + Pro)**: on the unlock window
  and the lock screen, the field no longer shows the 1 px frame of the edit
  control inside its rounded box. The edit is now created without a border (one
  created with it keeps drawing the frame whatever is done afterwards) and its
  text is centred by the card layout.

## [1.2.21] - 2026-11-16

### Fixed
- **Editor showing the text of another tab (Free + Pro)**: since 1.2.20, after a
  tab switch (or any replacement of the whole text: unlock, import) the editor
  could keep the previous text on screen, entirely or line by line, until
  something else repainted it. The line spacing applied after each replacement
  turned the redraw off and back on without asking for a repaint. The stored
  text was never affected, only what was drawn.

## [1.2.20] - 2026-11-06

### Added
- **Safety backups (Pro)**: before "Import full backup", "Import from another
  LockNote" and an update, the current note is written to a verified `.lnbak` in
  `%LOCALAPPDATA%\LockNote\auto-backups\`, encrypted with the current password.
  If the backup cannot be written, the import or the update does not happen. The
  newest 3 of each kind are always kept; beyond that, files older than 30 days
  are purged, then the oldest until the folder fits 1 GiB. After a password
  change you are offered to delete them, since they still open with the previous
  password.
- **File > Safety backups... (Pro)**: the history of these backups, with Restore
  (the normal import: password, confirmation, a new safety backup first),
  **Create backup now** (a `manual` backup, never purged by the policy), Delete
  and Open folder.

### Changed
- **Colourful Light theme**: the Light theme is no longer flat grey. The title
  bar (Windows 11: caption, text and border colours), the menu bar (owner-painted
  like the Dark one, with a hover pill), the tab strip, the status bar and the lock
  screen backdrop use a soft blue tint of the accent with dark navy text; the
  editor and the active tab stay white, the accent is deeper, the primary buttons
  are accent-coloured. Text pairs stay >= 4.5:1 and glyphs / borders >= 3:1 in
  both themes (the "saved" dot is darkened on the Light chrome). The Dark theme
  does not change.
- **Look of the dialogs**: the same palette, type hierarchy (title / section /
  body / caption), spacing and button size in every dialog. The default (Enter)
  button is filled with the accent, the others are outlined; buttons sit in one
  right-aligned row. In the Dark theme edit boxes, combo boxes, lists, scroll
  bars and check boxes are dark too (the Light theme keeps the native controls).
  Dialogs are scaled to the monitor DPI, centered on the main window, and grow
  instead of clipping long translations (fr, es, de). Esc closes every dialog,
  Enter validates in About, Unsaved changes, Export encoding, Settings,
  Preferences and Security. The dark error color was lightened to reach
  4.5:1 contrast, and the password strength bar no longer recolors the
  separators.
- **Tabs**: every tab shows its full name (width measured from the text, no more 200 px cap or estimate). When they do not all fit, a drop-down button at the right end of the strip lists the tabs.
- **Look (Pro, Dark theme)**: dark title bar and menu bar (also the dialogs and
  the drop-down menus), a tab strip that joins the editor (rounded active tab,
  accent line, colour dot), a neutral status bar with a Saved / Modified dot and
  an Encrypted padlock instead of the saturated blue band, a 20 px left margin in
  the editor and a gutter whose numbers are right-aligned with the caret line
  highlighted. The Light theme keeps its structure with the light palette.
- **Editor surface (Pro, both themes)**: comfortable 1.5 line spacing, with the
  line numbers still aligned line by line (checked against a real RichEdit in
  the tests); a subtle band behind the caret line in the text and in the gutter
  (not drawn while text is selected, nothing is added to the note); dark
  scrollbars in the Dark theme; the Lock / Attach / TOTP / Vault buttons of the
  tab strip are rounded, with hover, pressed and keyboard-focus states, a
  centred padlock, readable labels on the accent colour and tooltips.
- **Internal**: `src/ui/editor.rs` (10 000 lines) is split into `src/ui/editor/`, one
  file per feature. No behavior change; the same tests pass.

### Removed
- **Snapshots (Pro)**: File > Snapshots (Create Snapshot, Version History) and
  the automatic snapshot at each Ctrl+S are gone; a snapshot only held one tab,
  and the safety backups cover the whole note. Existing snapshots are no longer
  read into the app and disappear at the next save. Backups keep a `snapshots`
  field (written empty) so every `.lnbak` stays readable.

### Fixed
- **Theme change applied only after a restart**: choosing Dark or Light in
  Preferences (or importing a backup made with the other theme) now applies at
  once: title bar, menu bar, pop-up menus, editor colours and scrollbars,
  gutter, tab strip, status bar, toolbar buttons, tooltips and the tool panels.
  The import notices no longer ask for a restart for the theme (the language
  still needs one).
- **Double-click word selection**: the widening now runs when the button is released, the link/filter notification structures follow `richedit.h` packing (a URL click read shifted offsets), a caret at the start of a CRLF line maps to the right character, and the highlight reset counts UTF-16 positions.
- **Caret in the password field (Pro)**: the lock screen and the unlock dialog now
  put the caret in the field as soon as the window gets the focus (Windows gave
  the focus back to the window itself right after the activation).
- **Updater picks the exe of its own edition (Pro)**: the release zip holds
  `LockNote-Pro.exe` and `LockNote.exe`; the updater took the first one it met.
  It now selects by exact file name for the running edition and fails with an
  explicit error when that file is missing (no fallback to the other edition).
- **White bands in the unlock popup and the lock screen (Pro)**: thick white
  lines showed above and below the title, the prompt and the status line of the
  password card. nwg centres the text of a label with a non-client band that it
  fills with the default window colour and never repaints; the card now paints
  those bands with its own surface colour (same code as the other dialogs), in
  both themes.

## [1.2.19] - 2026-11-05

### Fixed
- **Import password box (Pro)**: validating an empty password closed the box
  without any message, which looked like a silent failure. It now stays open and
  says a password is required.

### Added
- **Import log (Pro)**: every step of "Import from another LockNote" (file chosen,
  data read, password given, decoding time, confirmation, result) is appended to
  `%LOCALAPPDATA%\LockNote\import.log`. It holds step names, counts and sizes
  only: no password, path or note content. Capped at 64 KiB.

## [1.2.18] - 2026-11-05

### Added
- **Import from another LockNote (Pro)**: File > Import from another LockNote...
  brings the notes, vault, snapshots, attachments and settings of an older LockNote
  exe into the running one (password of the old file required). Replaces the
  external `scripts/migrate-exe.ps1` for the normal case; the source is only read.

## [1.2.17] - 2026-11-05

### Added
- **Full backup: export and import (Pro)**: File > Export full backup... writes
  every tab, snapshot, vault secret, attachment and setting (TOTP included) to a
  `.lnbak` file encrypted with the note password; the written file is read back
  and checked before it is kept. File > Import full backup... asks the backup's
  password, shows what it holds and, once confirmed, replaces the current
  content in memory (it is written to disk by the usual save). Window position,
  active tab, caret and ssh-agent keys are not part of a backup. The format is
  versioned: a newer LockNote can always read today's backups, and an older one
  refuses a newer file with a clear message before asking for the password.
  Specified in docs/SPEC.md section 10d.

### Fixed
- **Updater behind a TLS-intercepting proxy (Free + Pro)**: the update
  download failed with "invalid peer certificate" because certificates were
  checked against bundled Mozilla roots only. They are now verified with the
  Windows certificate store, so a corporate proxy CA installed on the machine
  is accepted. Verification stays on: an untrusted chain is still rejected,
  and the Ed25519 signature check on `SHA256SUMS.txt` is unchanged.

## [1.2.16] - 2026-10-07

### Fixed
- **Vault: copy on click (Pro)**: selecting a secret in the vault tree copies
  its value again (removed in 1.2.15); categories copy nothing.
- **Vault: multi-line secrets (Pro)**: in the Edit/New dialog, "Show" now
  displays the whole value in a multi-line field instead of the first line only.

### Added
- **SSH keys (Pro)**: Tools > SSH Keys > Generate SSH Key creates Ed25519,
  ECDSA (P-256, P-384) or RSA (3072, 4096) keys, with an optional passphrase,
  shows the fingerprint and public key, saves both to the vault (category
  "SSH Keys") or exports them as owner-only files usable by OpenSSH.
  Tools > SSH Keys > Load into ssh-agent hands a vault key to the Windows
  ssh-agent; LockNote removes the keys it loaded at lock and close, and after
  an abnormal end at the next unlock.
- **Vault: Copy button in the secret dialog (Pro)**.
- **Vault: "Copied: name" indicator (Pro)**: shown 2 s under the tree after a
  copy (name only, never the value).
- **Tabs: drag & drop reordering (Pro)**: press a tab and release it on
  another tab to move it there (the dragged tab is framed in the accent colour
  and an insertion mark shows where it will land); the moved tab stays active and the new order
  is saved with the note.

### Changed
- **Tabs: active tab more visible (Pro)**: accent-tinted background, bold
  name and a 5 px accent bar.
- **Vault panel open at launch (Pro)**: the panel is opened when the app starts
  (it can still be closed with Ctrl+B or the toolbar; not remembered).

## [1.2.15] - 2026-09-30

### Changed
- **Double-click selection (Free + Pro)**: double-click now selects the
  exact text between two spaces (`user@mail.com`, `v1.2`, `C:\dir\file`,
  `Pass@word1!` — nothing is stripped, so passwords stay whole), and no
  longer copies it to the clipboard.
- **Vault panel (Pro)**: the category drop-down filter and flat list are
  replaced by a tree — every category (even empty), alphabetical, with its
  secrets sorted by name (locale-aware, case- and accent-insensitive).
  Selecting a secret no longer copies it: use the Copy button. Revealed
  values appear for 5 s in a field under the tree, never in the list
  itself. The vault is cleared from memory when the app locks.
- **Vault categories (Pro)**: categories are now yours — right-click in
  the vault tree to create, rename or delete one (General stays as the
  fallback; secrets of a deleted category move to it). Note: going back
  to 1.2.14 or older after creating categories would show those secrets
  in General as `Name:Category`.
- **Vault drag & drop (Pro)**: drag a secret onto another category (or onto
  a secret of that category) to move it there.

### Fixed
- **Locked state (Pro)**: keyboard shortcuts (Ctrl+S, Ctrl+W, Ctrl+T,
  Ctrl+Tab, Ctrl+B…) and keystrokes no longer reach the hidden editor
  while the lock overlay is shown.
- **Save safety (Free + Pro)**: saving with an empty password (only possible
  after a lock wiped it) is now refused instead of re-encrypting the notes
  with an empty key.
- **Link click crash (Free + Pro)**: clicking an auto-detected URL placed
  after an accented character or an emoji could abort the app (UTF-16
  positions were used as UTF-8 byte offsets). Positions are now mapped
  exactly; an unmappable range opens nothing.
- **Active tab after unlock (Pro)**: unlocking (inline overlay or tray
  re-authentication) always reopened the first tab, and the next save then
  persisted it. The tab and caret line in use when locking are now
  restored. Also stops a `[LOCKNOTE_STATE]` block from appearing in the
  text of a single-tab note after unlock (blocks that already leaked into
  existing notes are left as is and can be deleted by hand).
- **Window size and position (Free + Pro)**: the window always reopened at
  900×650. The saved size, position and maximized state are now restored
  (without growing each session), clamped to the current screens; a
  position on a monitor that is no longer connected falls back to the
  primary screen.
- **Unwanted word selection (Free + Pro)**: the "slow double-click"
  fallback (re-enabled by accident in 1.2.14) turned two nearby single
  clicks, a click-and-drag or a Shift+click into a word selection plus a
  clipboard copy. It is removed.
- **Occurrence highlighting (Free + Pro)**: selections longer than 256
  characters are no longer searched, which could freeze the UI on long
  tokens (base64 keys, minified JSON).
- **Tab overflow (Pro)**: with many tabs the last one was cut under the
  toolbar buttons (the lock button was not accounted for, and the tab that
  overflowed stayed drawn). Tabs that do not fit now go into the "…" menu,
  the active tab is always visible (narrowed with an ellipsis if needed),
  and right-clicking "…" no longer opens the menu of a hidden tab.

## [1.2.14] - 2026-04-24

### Changed
- **Inline lock overlay (Pro)**: the modal unlock popup is replaced by an
  overlay inside the main window (password field, Unlock button, status
  line) with exponential backoff on failed attempts. Fixes the dialog not
  re-appearing after wake-from-sleep.

## [1.2.13] - 2026-04-22

### Security
- **Signed updates (Free + Pro)**: `SHA256SUMS.txt` is now Ed25519-signed
  at release time and verified fail-closed by the updater.
- **Hardening (Free + Pro)**: plaintext of tabs/snapshots/attachments wiped
  at auto-lock, `Zeroizing` master password end-to-end, redacted panic
  hook, `WM_DROPFILES` refused while locked, download size caps,
  VirtualLock'd key buffer, compression padding against CRIME-class
  length leaks, merged post-HMAC error variants.

## [1.2.12] - 2026-04-21

### Fixed
- **Manual lock icon**: the U+1F512 emoji did not render in the owner-drawn
  toolbar button with the default font. The button now draws a vector
  padlock (filled body + arc shackle) via GDI and is sized 32 px wide.
- **Manual lock prompt**: pressing `Ctrl+L` or clicking the lock button
  locked the app but the unlock popup did not always appear. The prompt
  is now posted as a deferred `WM_APP+6` message so it opens as soon as
  the triggering click/keyboard handler returns.
- **Taskbar duplication**: the `UnlockDialog` used a regular top-level
  window flag and showed up as a second taskbar entry next to the main
  window. It now carries `WS_EX_TOOLWINDOW` and stays out of the taskbar.

## [1.2.11] - 2026-04-21

### Added
- **Manual lock**: a dedicated lock button (🔒) in the toolbar and a
  `Ctrl+L` shortcut now let you lock LockNote on demand. The lock runs
  the same sequence as auto-lock (save-if-dirty, clear editor, zeroize
  password, show overlay), then minimizes the window so the unlock
  dialog opens naturally when you bring the app back.

## [1.2.10] - 2026-04-21

### Fixed
- **Manual-install downgrade after failed auto-update**: `write_data` in
  1.2.9 always sourced its binary prefix from `.tmp` when a valid marker
  was present. If the user manually replaced `.exe` with a newer version
  while a stale `.tmp` from a prior failed auto-update was still on disk,
  the next save would promote the old binary back and the next close would
  swap `.exe` down to that older version. `write_data` now only trusts the
  `.tmp` binary when its mtime is at least as recent as `.exe`; otherwise
  the running `.exe` wins.

## [1.2.9] - 2026-04-21

### Fixed
- **Auto-update did not apply**: After a successful download the new binary
  sat in the staging `.tmp`, but `has_pending_tmp` was never set, so
  `on_close` never launched the swap helper — the `.exe` was never replaced.
  Worse, any save between download and restart rewrote `.tmp` from the
  *current* (old) `.exe`, silently discarding the downloaded binary. The
  updater now flags the staged `.tmp` as pending, and `write_data` sources
  its binary prefix from a valid `.tmp` (preserving a staged update across
  user saves) and falls back to the running `.exe` only when `.tmp` is
  absent or corrupt.

### Added
- **Restart prompt after update**: Successful downloads now ask "Restart
  LockNote now to apply?" — choosing "Yes" closes the app so the swap
  helper can replace the binary, "No" keeps the update staged for the
  next natural close.

## [1.2.8] - 2026-04-21

### Fixed
- **In-app updater**: Clients were failing with "Integrity file missing from
  release" because `SHA256SUMS.txt` was only embedded inside the zip, not
  published as a standalone release asset. The updater fetches it from the
  release's asset list, so the integrity check could not find it. The release
  workflow now uploads `SHA256SUMS.txt` alongside the zip on both the private
  repo and the public mirror.

## [1.2.7] - 2026-04-21

### Fixed
- **Tab rename persistence**: Renaming the only open tab (or removing the color
  of a single renamed tab) lost the new name on save/reload. The save path was
  falling back to the plain-text single-note format whenever there was one tab
  without a color, ignoring the name. It now falls back to plain text only for
  the default untouched tab (`Note 1`, no color); any customization forces full
  tab serialization so names and colors round-trip correctly.

## [1.2.6] - 2026-04-20

### Changed
- **Updater target**: The in-app auto-update now queries
  `ctardy/LockNote-Pro-releases` (public) instead of `ctardy/LockNote`.
  Pro users had been missing updates because the private source repo was
  unreachable anonymously; the release workflow now mirrors signed
  artifacts to the public releases repo on every tag.

## [1.2.5] - 2026-04-20

### Added
- **Bullet lists**: `Ctrl+.` toggles a `• ` prefix on the current line. Plain
  Enter on a bullet line continues the list; pressing Enter on an empty
  bullet line exits the list. Persisted as plain text, no RTF coupling.
- **Session state**: The active tab index and the caret line are stored in
  a `[LOCKNOTE_STATE]` block at the tail of the encrypted payload, so
  reopening the file restores the tab you were on and the line you were
  editing. Absence of the block (older files, fresh installs) is treated
  as "no prior session".
- **Slow double-click**: Word-selection now also fires when the two clicks
  are slower than Windows' system double-click time, provided the second
  click lands within ~900 ms and a few characters of the first. Helps
  users whose cadence is just off the system threshold.

### Changed
- **Double-click selection**: Extracted the whitespace-trim + alphanumeric-
  expand logic into `expand_word_selection`, a pure function with ten unit
  tests (full-word recovery from every letter↔digit segment, Unicode
  alphabetics, Newline/punctuation boundaries, empty and whitespace-only
  ranges, caret-between-chars slow-double).

### Fixed
- **Snapshots**: Deleting multiple snapshots in the History dialog then saving
  no longer "revives" entries on reload. The dialog now returns the resulting
  list directly instead of a sequence of index-based actions that became
  invalid after each in-dialog deletion.
- **Tab key in editor**: Pressing Tab now inserts `\t` instead of moving
  focus. The editor's `WM_GETDLGCODE` response claims Tab and Enter so
  `IsDialogMessage` — used by nwg's dispatch loop — stops eating them.
- **Tab bar ghost pixels**: Stale accent/border pixels no longer linger
  briefly over the editor's top lines after resizing or restoring the
  window.
- **Unlock focus**: Returning to the app while locked now places the caret
  back on the password field on every window activation, not only the
  initial show.
- **Double-click selection**: Mixed alphanumeric strings like `CT6A61EN`
  now select in full regardless of where the click lands. The selection
  is expanded across adjacent alphanumeric characters after RichEdit's
  default word-break splits letters from digits.

## [1.2.4] - 2026-04-17

### Fixed
- **Test suite**: serialized i18n-dependent tests so parallel `set_language()`
  calls no longer race with assertions on localized strings.

### Changed
- **Build**: cleaned up compiler warnings (`unused_mut`, `unused_imports`,
  `unused_must_use`, `redundant_semicolons`) and migrated the deprecated
  `nwg::Timer` in the TOTP manager to `nwg::AnimationTimer`.

## [1.2.3] - 2026-04-17

### Changed
- **Drag & drop**: Dropping a file now prompts the user instead of auto-inserting.
  - Text files: choose between *Insert text* and *Add attachment*.
  - Binary files: offered as an attachment (previously rejected with an error).
- **Text/binary detection**: Extension blocklist + content sniffing on the first
  8 KiB (BOM, nul bytes, UTF-8 validity) to correctly classify files without a
  clear extension.
- **Reentrance guard**: Additional drops arriving while a choice dialog is open
  are ignored instead of stacking modals.
- **Folders**: Dropping a folder now shows an explanatory message instead of
  silently doing nothing.

### Added
- **i18n**: New keys `drop.choice.*` and `drop.directory` (EN/FR/ES/DE).

## [1.2.0] - 2026-04-16

### Added
- **Toolbar buttons**: Owner-drawn toggle buttons for Vault, TOTP, and Attachments in the tab bar area with themed rendering, border, and badge showing entry count
- **Tab close button**: Close (×) icon in top-right corner of each tab with confirmation dialog when tab has content
- **TOTP side panel**: Replaces the modal popup with a persistent side panel (live codes, countdown timer, copy/add/remove)
- **Mutual exclusion**: Only one tool panel (Vault, TOTP, Attachments) can be open at a time
- **Steganography dedicated password**: Export/import uses a password independent from the LockNote session (with confirmation on export)
- **Steganography extract to new tab**: Extracted notes open in a new tab named after the image file
- **Arrow cursor on tab bar**: WM_SETCURSOR override to show pointer instead of I-beam
- **i18n**: ~30 new translation keys for TOTP panel, tab context menu, tab close dialog, stego password dialog (EN/FR/ES/DE)
- **Tests**: +33 new tests (449 total) covering stego extension detection, vault edge cases, tabs serialization, storage hint parsing

### Fixed
- **Steganography file dialogs**: Fix NWG filter format crash on export/import (was using Win32 format instead of NWG format)
- **Tab close**: All close paths (×, Ctrl+W, context menu) now centralized through `close_tab_at()`

## [1.1.5] - 2026-04-16

### Fixed
- **Minimize to tray**: Window now hides from taskbar when minimized with the option enabled
- **System tray restore** (Free): ShowWindow(SW_RESTORE) was missing, window didn't restore properly
- **Enter key in dialogs**: Fixed Enter key not validating password dialogs (nwg's IsDialogMessage was intercepting VK_RETURN)
- **Dialog focus**: Password input now receives focus reliably on dialog open
- **Auto-lock unlock popup**: Clicking the lock overlay now shows the unlock dialog (was unresponsive)
- **Auto-lock close corruption**: Closing the window while locked no longer overwrites the file with empty data
- **Lock overlay i18n**: "LockNote is locked" and "Click to unlock" texts are now translatable (EN/FR/ES/DE)

## [1.1.4] - 2026-04-15

### Added
- **File drag & drop**: Drop text files (.txt, .conf, etc.) from Explorer to import content into the editor
- **Settings dialog**: Language, Auto-lock, Auto-save, Clipboard clear, Global hotkey options
- **Auto-lock**: Timer checks every 10s, auto-saves and locks on inactivity timeout
- **Auto-save**: Periodic background save when document is modified
- **Clipboard guard**: Auto-clears clipboard after configurable timeout
- **Global hotkey**: Ctrl+Shift+L toggles window visibility (registered via Win32 RegisterHotKey)
- **i18n**: Language is set on startup from settings
- **Tab shortcuts**: Ctrl+T (new), Ctrl+W (close), Ctrl+Tab / Ctrl+Shift+Tab (switch)
- **Tab menu handlers**: New, Close, Rename, Next, Previous fully wired
- **Always-on-top**: Implemented via Win32 SetWindowPos
- **Word wrap toggle**: Implemented via EM_SETTARGETDEVICE at runtime
- **Occurrence highlighting**: UTF-16/UTF-8 conversion helpers for RichTextBox selection

### Fixed
- Print and Export menu items now have event handlers in bind_events()
- Ctrl+T and Ctrl+W keyboard shortcuts no longer no-ops (were empty stubs)
- Settings dialog window height adjusts dynamically based on enabled Pro features
- Double-click word selection no longer causes the editor viewport to jump (occurrence highlighting now preserves the topmost visible line via EM_GETFIRSTVISIBLELINE / EM_LINESCROLL)

## [1.0.1] - 2026-04-12

### Added
- Compression integration (GZip before encryption)
- Configurable PBKDF2 iterations (50k to 1M)
- Vault UI (side panel with Add/Edit/Delete/Copy/Reveal)
- TOTP unlock flow (2FA on password entry)
- Version snapshots (up to 20 per file, with history dialog)
- Crypto v2 wire format (backward-compatible with v1)

## [1.0.0] - 2026-04-11

### Added
- Full Rust rewrite from .NET
- AES-256-CBC + HMAC-SHA256 encryption
- PBKDF2-SHA256 key derivation (100,000 iterations)
- Dark/Light theme system with WCAG contrast compliance
- Single-note encrypted editor
- System tray integration
- Auto-update check via GitHub releases
- Ctrl+F find, Ctrl+H replace, Ctrl+G go-to-line
- Window position/size persistence
- Context menu (right-click)
- Line number gutter
- Status bar (word/char/line count)

### Pro features (initial)
- Multi-tab support (tab bar, serialization)
- Vault (encrypted secret storage)
- TOTP two-factor authentication
- GZip compression
- Internationalization (EN, FR, ES, DE)
- Auto-lock on idle
- Auto-save
- Clipboard auto-clear
- Global hotkey (Ctrl+Shift+L)
- Version snapshots
