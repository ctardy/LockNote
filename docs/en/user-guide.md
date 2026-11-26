# LockNote 2.1 user guide

[Français](../fr/guide-utilisateur.md) | [README](../../README.md)

LockNote is an encrypted notepad for Windows that stores the program and your
notes in a single `.exe` file. You install nothing, you need no account, and
nothing goes to a cloud. Since 2.0.0 every feature is free; there is no Pro
edition any more.

This guide describes every function of LockNote 2.1. Menu names are the
English labels of the application. The French labels are in the
[French guide](../fr/guide-utilisateur.md).

## Contents

1. [First launch and password](#1-first-launch-and-password)
2. [How your notes are stored](#2-how-your-notes-are-stored)
3. [Unlocking at startup](#3-unlocking-at-startup)
4. [Locking and the lock screen](#4-locking-and-the-lock-screen)
5. [The main window](#5-the-main-window)
6. [Saving and closing](#6-saving-and-closing)
7. [Tabs](#7-tabs)
8. [Editor features](#8-editor-features)
9. [Vault](#9-vault)
10. [Prompts](#10-prompts)
11. [TOTP manager](#11-totp-manager)
12. [Two-factor authentication at unlock](#12-two-factor-authentication-at-unlock)
13. [Attachments](#13-attachments)
14. [Steganography](#14-steganography)
15. [SSH keys and ssh-agent](#15-ssh-keys-and-ssh-agent)
16. [Password generator](#16-password-generator)
17. [Auto-lock, auto-save and clipboard auto-clear](#17-auto-lock-auto-save-and-clipboard-auto-clear)
18. [Global hotkey and system tray](#18-global-hotkey-and-system-tray)
19. [Language and theme](#19-language-and-theme)
20. [Safety backups](#20-safety-backups)
21. [Full backup (.lnbak)](#21-full-backup-lnbak)
22. [Import from another LockNote](#22-import-from-another-locknote)
23. [Temporary files](#23-temporary-files)
24. [Updates](#24-updates)
25. [Migrating from LockNote 1.2.x](#25-migrating-from-locknote-12x)
26. [Settings reference](#26-settings-reference)
27. [Keyboard shortcuts](#27-keyboard-shortcuts)
28. [What is written unencrypted to disk](#28-what-is-written-unencrypted-to-disk)

---

## 1. First launch and password

**What it does.** The first time you open a `LockNote.exe` that holds no notes,
LockNote asks you to choose the password that encrypts them.

**How to use it.**

1. Double-click `LockNote.exe`.
2. The **Create Password** window opens ("Welcome to LockNote"). Type your
   password in **Password (min. 6 characters):**, then the same password in
   **Confirm:**.
3. Click **Create** (or press Enter).

**Options and defaults.**

- Minimum length: 6 characters for a new note. You can change it later in
  File > Settings > Security (see [section 26](#26-settings-reference)).
- The strength meter under the password is for information only. It adds one
  point for each of: at least 8 characters, at least 12, at least 16, a
  lowercase letter, an uppercase letter, a digit, another character. 0 to 1
  point reads **Weak**, 2 to 3 **Fair**, 4 to 5 **Strong**, 6 to 7
  **Very strong**. Only the minimum length is enforced.

**Warnings.**

- Errors are shown in the window: "Password is too short." or
  "Passwords do not match."
- **Cancel** closes LockNote without creating anything.
- There is no way to recover a forgotten password. Your notes are encrypted
  with it (AES-256-CBC + HMAC-SHA256, key derived with Argon2id, 64 MiB,
  3 iterations, 4 lanes).
- This window, the unlock window and the startup messages are shown in
  English: the language you choose is stored inside the encrypted notes and is
  only known once they are decrypted.

## 2. How your notes are stored

**What it does.** Your notes are stored at the end of the `.exe` itself. Copy
the exe (to a USB key, another computer) and your notes travel with it.

**How a save works.** A running program cannot rewrite its own file. Every
save therefore goes first to a staging file in `%LOCALAPPDATA%\LockNote`,
named `<hash>.tmp`, with a companion file `<hash>.base`. When LockNote closes,
a small helper built into the exe copies the `.tmp` over the exe. This also
works when the exe is on another drive (USB key, second disk, network share).

**Warnings.**

- **Until LockNote has closed, the `.tmp` file is the only copy of your
  latest save. Never delete it by hand.** File > Settings > Temporary files...
  shows these files and what each one is ([section 23](#23-temporary-files)).
- Copy the exe only after LockNote has closed. A copy made while it runs does
  not hold the latest save.
- The `.tmp` holds the same encrypted data as the exe: nothing is written in
  clear.
- Only one LockNote runs at a time. Starting a second one brings the window of
  the running one to the front.

**Two versions of your notes.** If, at startup, the exe holds other notes than
the ones the pending save was based on (the exe was replaced or written
elsewhere while a save was waiting), LockNote asks before any password:
"LockNote - Two versions of your notes". It shows both dates (UTC) and the
folder where the version you do not keep goes:

- **Yes**: keep the pending save.
- **No**: keep the notes of the program file.
- **Cancel**: quit without changing anything; the question comes back at the
  next start.

The version you do not keep is never deleted: it is moved to
`%LOCALAPPDATA%\LockNote\conflicts\` as a complete LockNote `.exe`, which you
can open with File > Import from another LockNote... and the password it had.

If LockNote cannot read the program file or a pending save, it shows a message
naming the file and closes without changing anything.

## 3. Unlocking at startup

**What it does.** When the exe already holds notes, LockNote asks for the
password before showing them.

**How to use it.**

1. Start LockNote. The **Unlock** window asks: "Enter your password to unlock
   your notes."
2. Type the password. The eye button shows or hides what you type. "Caps Lock
   is on" is shown while Caps Lock is on and the field has the focus.
3. Press Enter or click **Unlock**. "Decrypting..." is shown while the key is
   derived.
4. If two-factor authentication is on, the 6-digit code is asked next
   ([section 12](#12-two-factor-authentication-at-unlock)).

**Limits.**

- 5 attempts. After a wrong password the window shows "Incorrect password.
  (N/5)". From the 2nd wrong attempt you must wait before the next one: 1 s
  after the 2nd, 2 s after the 3rd, 4 s after the 4th ("Retry in Ns..."). After
  the 5th, "Maximum attempts reached. Exiting." and LockNote closes.
- **Cancel** or Esc closes LockNote.

## 4. Locking and the lock screen

**What it does.** Locking hides your notes and wipes the password and the
decrypted contents from memory until you type the password again.

**Where it is.**

- The **Lock** button (padlock) at the right end of the tab strip.
- **Ctrl+L**.
- Automatically after a period of inactivity (auto-lock,
  [section 17](#17-auto-lock-auto-save-and-clipboard-auto-clear)).

**What happens when LockNote locks.**

1. The SSH keys this file loaded into ssh-agent are removed from it.
2. Unsaved changes are saved.
3. The active tab and the caret line are remembered.
4. The editor, the vault, the prompts and the attachments are emptied in
   memory.
5. The window shows the lock screen: "LockNote is locked", a password field
   with an eye button, and **Unlock**.

**How to unlock.** Type the password and press Enter or click **Unlock**. You
come back to the same tab and line.

**Limits and warnings.**

- 5 attempts, with the same waiting times as at startup (1 s, 2 s, 4 s). After
  the 5th wrong password LockNote closes.
- The lock screen asks only for the password, not for the two-factor code.
- While locked, keyboard shortcuts do nothing and files dropped on the window
  are ignored.
- Closing LockNote while it is locked exits without saving: the save was made
  when it locked.

## 5. The main window

**Menu bar.** File, Edit, View, Tab, Tools, Help.

| Menu | Items |
|---|---|
| File | Save, Print..., Export..., Export full backup..., Import full backup..., Import from another LockNote..., Safety backups..., Settings... (Preferences..., Security..., Temporary files...), Quit |
| Edit | Cut, Copy, Paste, Paste as Plain Text, Find..., Replace..., Go to Line..., Duplicate Line, Delete Line, Select All, Insert Timestamp |
| View | Always on Top, Word Wrap, Line Numbers |
| Tab | New Tab, Close Tab, Rename Tab..., Next Tab, Previous Tab |
| Tools | Vault Manager..., Prompts, TOTP Manager, Attachments, Password Generator..., Steganography (Hide in Image..., Extract from Image...), SSH Keys (Generate SSH Key..., Load into ssh-agent...) |
| Help | Check for Updates..., About LockNote... |

**Tab strip.** Your tabs, the **+** button (new tab) and, on the right, the
buttons **Prompts**, **Vault**, **TOTP**, **Attach** and **Lock**. Each button
opens or closes its side panel; it is highlighted while its panel is open.

**Side panels.** Vault, Prompts, TOTP and Attachments open on the right of the
editor, one at a time. The Vault panel is open when LockNote starts. Drag the
thin bar between the editor and the panel to resize it. The width is kept for
the session and saved with the note; it is not part of a full backup.

**Status bar.**

- Left: words, characters and lines of the active tab.
- Middle: the full path of the LockNote file. Click it to copy the path to the
  clipboard ("Path copied").
- Right: a dot with **Saved** or **Modified**, then **Encrypted**, then the
  encoding (UTF-8).

**Window title.** "LockNote", or "LockNote *" when there are unsaved changes.

**Window position.** The size, position and maximized state are restored at
the next start (saved with the note when it closes). If the saved position is
no longer on a screen (unplugged monitor), the window opens at a default place.

**About.** Help > About LockNote... shows the version, the encryption used and
the address of the project.

## 6. Saving and closing

**Save.** File > Save, or **Ctrl+S**. The save shortcut can be changed in
File > Settings > Preferences > **Save shortcut:** (click the field, then press
the new combination: Ctrl, Shift and/or Alt with a letter A-Z or F1-F24; at
least one modifier is required). The new shortcut works at once; the menu shows
it from the next start.

**On close.** File > Quit, Ctrl+Q, Alt+F4 or the window's close button.
What happens with unsaved changes depends on File > Settings > Preferences >
**On close:**

- **Ask** (default): "Unsaved changes. Save before closing?" **Save** saves,
  **Don't Save** closes without saving, **Cancel** keeps LockNote open.
  **Remember my choice** with Save sets On close to Always save, so the
  question is not asked again. With Don't Save it cannot be remembered: the
  settings are stored inside the note, which is not saved; LockNote says so
  (choose Never save in the Preferences and save instead). Cancel never
  changes the setting.
- **Always save**: saves without asking.
- **Never save**: closes without saving.

Unless you chose not to save, LockNote saves once more when it closes, which
also stores the window position and the settings.

**Auto-save.** See [section 17](#17-auto-lock-auto-save-and-clipboard-auto-clear).

**Windows logoff or shutdown.** LockNote cannot ask anything then. It saves
the notes, as if you had answered Save, unless On close is **Never save**
(then unsaved changes are lost, as with a normal close); a locked LockNote
was already saved when it locked. The same rule applies while the notes are
hidden after a lock that could not save yet: they are saved with their
changes, unless On close is **Never save**, which you chose: then those
unsaved changes are lost. Then it closes as usual (SSH keys removed
from ssh-agent, an update or a pending save applied). If the save fails,
Windows shows "LockNote could not save your notes (...)" on its shutdown
screen: cancel, then save or copy your text. The list of SSH keys this file
loaded is not saved again at that moment: at the next unlock LockNote
removes those keys from ssh-agent once more, so a key you added to
ssh-agent by hand in between is removed too; add it again if needed.

**Warning.** If a save fails, a "Save Error" message says why. Your changes are
still in the window: try again before closing. When a save fails while
closing (**Save** at the prompt, Always save, or the last save), LockNote
says "LockNote could not save: ..." with the reason. **Keep LockNote open**
(the default, also Enter, Esc or the title bar's close button) keeps your
text in the window; **Close without saving** closes and the unsaved changes
are lost. The SSH keys loaded into ssh-agent by this file were already
removed at that point; load them again if you keep LockNote open.

## 7. Tabs

**What it does.** One LockNote file holds up to 20 tabs, each with its own
text, name and optional colour.

**How to use it.**

- **New tab**: the **+** button, Tab > New Tab or **Ctrl+T**. New tabs are
  named "Note N". At 20 tabs: "Maximum number of tabs reached (20)."
- **Switch**: click a tab, or **Ctrl+Tab** / **Ctrl+Shift+Tab** (Tab > Next Tab /
  Previous Tab).
- **Close**: the close box on the tab, Tab > Close Tab, **Ctrl+W**, or right
  click > Close Tab. A tab that holds text asks first: "Close tab "name"? Its
  content will be lost." The last tab cannot be closed.
- **Rename**: Tab > Rename Tab... (**F2**), or right click on the tab > Rename Tab....
  The name cannot be empty.
- **Colour**: right click on the tab > Color > a colour (blue, green, red,
  orange, purple, yellow, pink, gray), or No color. The tab shows a dot of that
  colour.
- **Reorder**: press on a tab, drag it onto another tab and release. A mark
  shows where it will land.
- **Hidden tabs**: when the tabs do not fit, a drop-down button appears at the
  right of the strip. It lists every tab; choose one to switch to it.

The active tab and the caret line are restored after a lock and at the next
start.

## 8. Editor features

### Editing commands

| Command | Where | Shortcut |
|---|---|---|
| Cut, Copy, Paste | Edit menu, right click in the editor | Ctrl+X, Ctrl+C, Ctrl+V |
| Paste as Plain Text | Edit menu, right click | Ctrl+Shift+V |
| Select All | Edit menu, right click | Ctrl+A |
| Undo, Redo | keyboard only | Ctrl+Z, Ctrl+Y |
| Duplicate Line | Edit menu | Ctrl+D |
| Delete Line | Edit menu | Ctrl+Shift+K |
| Insert Timestamp | Edit menu | F5 |
| Bullet on the current line | keyboard only | Ctrl+. (period) |

- **Insert Timestamp** inserts the date and time as `YYYY-MM-DD HH:MM:SS`, in
  UTC.
- **Bullets**: Ctrl+. adds "• " at the start of the line, or removes it. On a
  bullet line, Enter starts a new bullet line; Enter on an empty bullet line
  ends the list.
- **Tab key** inserts a tab character in the text.
- **Double-click** selects the whole run of text between two spaces, so a
  password such as `Pass@word1!` is selected whole. Nothing is copied: copy is
  always Ctrl+C.
- **Occurrence highlighting**: when you select a word (2 to 256 characters, on
  one line), its other occurrences in the tab are highlighted.
- **Links**: text starting with `http://` or `https://` is shown as a link; a
  click opens it in your browser.
- **Line numbers** are shown in a margin on the left of the text.

### Find and replace

- **Edit > Find...** (Ctrl+F) opens the **Find** window; **Edit > Replace...**
  (Ctrl+H) opens **Find & Replace**. The selected text, if any, is the starting
  search.
- **Find:** the text to look for. **Previous** and **Next** move between
  matches. The counter shows "3 / 12", "12 matches" or "No matches".
- Options: **Match case**, **Whole word**.
- In Find & Replace: **Replace:** the new text, **Replace** (the current
  match), **Replace All** ("N occurrences replaced.").
- **Close** closes the window.

### Go to line

Edit > Go to Line... (Ctrl+G). The field shows the current line; type a number
between 1 and the number of lines shown ("Line number (1 - N):"), then OK.
Otherwise: "Invalid line number."

### Word wrap

View > Word Wrap. On by default. The choice is saved with the note.

### Line numbers

View > Line Numbers shows or hides the line numbers on the left of the text.
On by default. When hidden, the text takes their width. The choice is saved
with the note (at the next save, the save at close included) and restored at
the next start.

### Always on top

View > Always on Top keeps the window above the others. It is not remembered:
it is off at each start.

### Font

File > Settings > Preferences: **Editor font:** (Consolas, Courier New, Lucida
Console, Cascadia Mono, Segoe UI; default Consolas) and **Font size:** (8 to 24;
default 11). The editor uses the new font from the next start of LockNote.

### Print

File > Print... (Ctrl+P).

1. Confirm your password ("Enter your password to print:").
2. Choose the printer in the Windows print dialog.

Only the active tab is printed, in Consolas 10 pt with 1-inch margins. Long
lines are not wrapped on paper.

### Export as a text file

File > Export...

1. Confirm your password ("Enter your password to export:").
2. **Export Encoding**: choose **UTF-8** (default), **UTF-8 with BOM**,
   **Windows-1252** or **ISO-8859-1 (Latin-1)**, then OK.
3. Choose the file name (Text Files (*.txt)).

**Warnings.** Only the active tab is exported, and **the file is not
encrypted**. If some characters cannot be written in the chosen encoding, they
are replaced and LockNote asks whether to continue.

### Import a text file (drag and drop)

Drop a file from Explorer onto the LockNote window. A window "Dropped file"
offers:

- for a text file: **Insert text** (inserted at the caret), **Add attachment**
  or **Cancel**;
- for another file: **Add attachment** or **Cancel**.

Limits: only the first file of a multiple drop is taken; folders are refused;
inserted text must be UTF-8 and at most 32 MiB, otherwise "This file could not
be read as text."

## 9. Vault

**What it does.** The vault stores secrets (passwords, API keys, multi-line
values such as keys or certificates) by category, encrypted with the note.

**Where it is.** The **Vault** button of the tab strip, Tools > Vault Manager...
or **Ctrl+B**. The panel is open when LockNote starts.

**The panel.**

- A search field ("Search a name") at the top.
- A tree: every category, sorted by name, with its secrets under it. Only the
  name of a secret is shown, never its value.
- Buttons: **Add**, **Edit**, **Del**, **Copy**, **Reveal**. Edit, Del, Copy and
  Reveal need a selected secret.

**Copy a secret.** Click the secret in the tree, or select it and click
**Copy**. The value goes to the clipboard and "Copied: name" is shown for
2 seconds. The copy is kept out of the Windows clipboard history (Win+V) and is
cleared by the clipboard auto-clear. Clicking a category copies nothing.

**Reveal.** **Reveal** shows the value for 5 seconds in a read-only field under
the tree, then hides it.

**Add or edit a secret.**

1. **Add** (or select a secret and **Edit**).
2. In **New Secret** / **Edit Secret**: **Name:** (required), **Category:**,
   **Value:**.
3. The value is masked on one line. **Show** switches to a multi-line field
   that shows the whole value; click it again to mask it.
4. **Copy** copies the value. **Generate...** opens the password generator; its
   **Use** button puts the password in the value.
5. OK saves the secret in the vault; Cancel closes without change.

**Delete a secret.** Select it, **Del**, then confirm.

**Search.** Type in the search field: the tree shows only the secrets whose
name contains the text, ignoring case and accents. The search never looks at
values.

**Categories.** Right click in the tree:

- **New category…** (anywhere);
- **Rename…** and **Delete** (on a category).

Rules: 1 to 40 characters, no `:`, `[` or `]`, names unique (ignoring case and
accents), 50 categories at most. **General** always exists and cannot be renamed
or deleted. Deleting a category moves its secrets to General after confirmation.
A new note starts with General, Email, Cloud, Finance, API Keys, Passwords and
Crypto.

**Drag and drop.** Drag a secret onto a category (or onto a secret of that
category) to move it there. Esc cancels the drag.

**After a change**, the vault is saved with the next save of the note.

## 10. Prompts

**What it does.** Stores texts you reuse (for example prompts for an AI tool)
and copies them in one click.

**Where it is.** The **Prompts** button of the tab strip, or Tools > Prompts.

**How to use it.**

- **Search name or text**: filters on the name and the text, ignoring case and
  accents. The status line shows the count ("N prompt(s)", "N of M prompt(s)").
- The list shows "Category · Name", sorted by category then name. Selecting a
  prompt shows its text in the preview below.
- **A click on a prompt copies its text** ("Copied: name"). The **Copy** button
  does the same.
- **Add** / **Edit**: **Name:**, **Category:** (free text, with a list of the
  categories already used, "General" first) and **Text:** (several lines; Enter
  adds a line, Tab adds a tab, Ctrl+Tab leaves the field). All three are
  required.
- **Del** deletes the selected prompt after confirmation.

**Limits.** A text is at most 1 MiB. A category cannot contain `:`.

**Warning.** A prompt is not treated as a secret: its copy is not cleared by
the clipboard auto-clear and is not hidden from the clipboard history.

## 11. TOTP manager

**What it does.** Shows the current 6-digit one-time codes (RFC 6238, 30-second
period) of the TOTP keys stored in the note.

**Where it is.** The **TOTP** button of the tab strip, or Tools > TOTP Manager.

**The panel.** A list "Name — code", refreshed every second, and the time
left before the next code. Buttons: **Copy Code**, **Add Key**, **Remove**.

**Copy a code.** Select an entry, then **Copy Code** ("Code copied to
clipboard."). If Windows refuses the copy (another application holds the
clipboard), the panel says "Copy failed: ..." instead: nothing was copied.
The same rule applies to every Copy button of LockNote.

**Add a key.**

1. **Add Key**, then type the **Entry name:**.
2. In the **TOTP Setup** window, the **Secret:** field is empty ("Base32 or
   otpauth:// link"). Paste the secret the site gives: the Base32 text
   (spaces and lowercase are accepted, e.g. `jbsw y3dp ehpk 3pxp`) or the
   `otpauth://totp/...` link of its QR code. Or click **Generate** for a new
   random secret, then enter it at the site or in your authenticator app
   (**Copy**, or the **OtpAuth URI:** shown below).
3. **Confirm** adds the key. A secret that is not valid is refused with the
   reason (empty, not Base32, a length Base32 never gives because part of it
   is missing or it was pasted twice, shorter than 80 bits, a link that is not
   `otpauth://totp/`, a link without secret, or a link asking for other codes
   than SHA1 / 6 digits / 30 seconds) and nothing is added. **Verify** is optional here: type a code
   and click it to check the secret before confirming.
4. Enter the code LockNote then shows in the panel where the site asks for it.

**Remove a key.** Select it, **Remove**, then confirm ("This cannot be undone.").

**Warnings.** A TOTP code you copy is not cleared by the clipboard auto-clear.
The keys are saved with the note and travel in a full backup.

## 12. Two-factor authentication at unlock

**What it does.** Asks for a 6-digit code from an authenticator app after the
password, when LockNote starts.

**Where it is.** File > Settings > Security... > **Enable TOTP two-factor
authentication**.

**Turn it on.**

1. Tick the box. The **TOTP Setup** window opens with a new secret.
2. Add the secret (Base32 or OtpAuth URI) to your authenticator app.
3. Type a code, **Verify**, then **Confirm**.
4. Click OK in the Security window, then save the note.

Closing the setup window without confirming unticks the box.

**At startup.** After the password, the **Two-factor authentication** window
asks: "Enter the 6-digit code from your authenticator app." Type it and click
**Verify**. 3 attempts; after the 2nd wrong code you wait 2 s. After the 3rd,
"Maximum attempts reached." and LockNote closes. **Cancel** closes LockNote.

**Turn it off.** Untick the box: LockNote asks for a current code first. A
wrong code keeps the option on.

**Warnings.**

- The code is asked at startup only. Unlocking the lock screen asks only for
  the password.
- Keep the secret somewhere safe: without the app or the secret you cannot open
  your notes.
- Importing a full backup or another LockNote replaces this setting with the
  one of the imported data.

## 13. Attachments

**What it does.** Stores files inside the encrypted note.

**Where it is.** The **Attach** button of the tab strip, Tools > Attachments,
or **Ctrl+J**. You can also drop a file onto the window and choose **Add
attachment**.

**The panel.** A list "name (size)", the number of files and their total size.
Buttons: **Add**, **Open**, **Save**, **Del**.

- **Add**: choose a file.
- **Save**: choose where to write a copy of the selected attachment.
- **Open** (or double-click): choose where to write a copy, then LockNote opens
  that copy with its default program. For an executable type (`.exe`, `.bat`,
  `.cmd`, `.ps1`, `.js`, `.lnk`, `.msi`, `.reg` and similar) LockNote asks
  first: "Open attachment?".
- **Del**: deletes the selected attachment after confirmation.

**Limits.** 10 MB per file and 50 MB in total. A file over the limit is
refused. Before Open, LockNote checks the SHA-256 of the stored data; damaged
data gives "Attachment data is corrupted (SHA-256 mismatch)."

**Warning.** Open and Save write the file **unencrypted** where you chose.
Delete that copy yourself when you no longer need it.

## 14. Steganography

**What it does.** Hides the text of the active tab, encrypted, inside a copy of
an image (JPEG, PNG or BMP). The image still looks the same.

**Hide.** Tools > Steganography > Hide in Image...

1. **Select Carrier Image**: choose the image.
2. **Save Steganographic Image**: choose where to write the new image.
3. **Choose Image Password**: a password for this image, typed twice. It can
   differ from your note password.
4. "Content hidden successfully." shows the size.

**Extract.** Tools > Steganography > Extract from Image...

1. **Select Image to Extract From**.
2. **Image Password**: the password used to hide the data.
3. The text opens in a new tab named after the image file. When 20 tabs are
   already open, it replaces the text of the active tab.

**Limits and warnings.**

- Only the text of the active tab is hidden (no other tab, vault or
  attachment).
- If the hidden data would exceed 50 % of the image size: "Notes too large for
  this image." Choose a larger image.
- "Do not share via WhatsApp, Instagram, etc. Image recompression destroys
  hidden data."
- "Wrong password or not LockNote data.": by design LockNote cannot tell the
  two apart. "No hidden data detected in this image." means there is nothing
  after the end of the image.
- Formats other than JPEG, PNG and BMP are refused.

## 15. SSH keys and ssh-agent

### Generate a key

Tools > SSH Keys > Generate SSH Key...

1. **Type:** Ed25519 (recommended, default), ECDSA P-256, ECDSA P-384,
   RSA 3072 or RSA 4096.
2. **Name in the vault:**, **Comment:** (one line, no tab or control
   character), **Passphrase (optional):** and **Confirm passphrase:**. With a
   passphrase, the private key is encrypted in the standard OpenSSH format.
3. **Generate** ("Generating...": RSA 4096 takes a few seconds). The
   **Fingerprint:** (`SHA256:...`) and the **Public key:** are shown.
4. Then:
   - **Copy public key**;
   - **Save to vault**: stores two secrets in the category **SSH Keys**
     (created if needed): `<name>` (private key) and `<name>.pub` (public key).
     A name already used is refused;
   - **Export...**: writes `<file>` and `<file>.pub`. The private file is
     readable by your Windows account only. LockNote never overwrites a file.
     Without a passphrase, it asks for confirmation first.
5. **Close**. A key you neither saved nor exported is dropped.

### Load a key into ssh-agent

Tools > SSH Keys > Load into ssh-agent...

1. **Key:** choose a vault secret that holds an OpenSSH private key (any
   category).
2. **Passphrase (if the key has one):**.
3. Read the warning, then **Load**. "Key loaded into ssh-agent" shows its
   fingerprint.

**What LockNote does with it.** The key is handed to the Windows OpenSSH agent
service. LockNote removes it from the agent when it locks and when it closes,
and, after an abnormal end, at the next start or unlock of this file. Loading a
key also saves the file. The same key added by hand with `ssh-add` is removed
too.

**Errors.** If the service is not running, LockNote shows the commands to run
in an administrator PowerShell:

```powershell
Get-Service ssh-agent | Set-Service -StartupType Automatic
Start-Service ssh-agent
```

Other messages: "Wrong passphrase.", "This key is protected: enter its
passphrase.", "ssh-agent refused the operation.", "ssh-agent did not answer in
time.". If no secret holds a private key: "No SSH private key in the vault."

## 16. Password generator

**Where it is.** Tools > Password Generator..., and the **Generate...** button
of the secret window of the vault.

**How to use it.**

- A password is drawn when the window opens, on **Generate**, and whenever an
  option changes. **Show** / **Hide** reveals or masks it.
- **Copy** copies it (kept out of the clipboard history, cleared by the
  clipboard auto-clear).
- **Use** (only from the secret window) closes the generator and puts the
  password in the value of the secret.
- **Close** closes the window.

**Options and defaults.**

| Option | Default |
|---|---|
| Length (8-64) | 16 |
| Uppercase (A-Z), Lowercase (a-z), Digits (0-9), Symbols | all ticked |
| Allowed symbols | ``!@#$%^&*()-_=+[]{};:,.?`` (ASCII punctuation only, duplicates dropped) |
| Exclude look-alike characters (0Oo1lI\|) | not ticked |

The password always holds at least one character of every ticked type. When
the options make it impossible, the field is emptied and the reason is shown
(for example "Select at least one type of character."). The options you last
used are kept with the note.

## 17. Auto-lock, auto-save and clipboard auto-clear

### Auto-lock

File > Settings > Security... > **Auto-lock:** Off, 1, 5, 10, 15 (default), 30
or 60 min.

LockNote locks ([section 4](#4-locking-and-the-lock-screen)) when there has been
no key press, no mouse movement or click on its window and no change to the
text for that long. It checks every 10 seconds. Unsaved changes are saved
first; if that save fails, LockNote does not lock and tries again after another
period. A new value applies at once.

While a LockNote window or message is open over the main window (a dialog,
the close question...), auto-save waits and its delay starts again when it
closes. It also waits while the notes are hidden (see below): LockNote's own
save attempts are the only ones then.

**When LockNote locks** (auto-lock delay reached, Lock button, Ctrl+L), your
notes are hidden at once ("Notes hidden"); only your password shows them
again (**Show notes**; wrong passwords are slowed down as on the lock
screen). At that moment every open LockNote window or question gets its
harmless answer: the close question is cancelled, "Close without saving?"
keeps LockNote open, a Yes/No question is answered **No**, other windows
are closed as Cancel (what you typed in them is lost, your notes are not).
An answer clicked after that does nothing.

LockNote then saves and locks for good as soon as no window is open any
more. If a window cannot close at once (the temporary files window while it
reads a network share), the screen names it and LockNote waits. If the save
fails, nothing is lost: your notes stay hidden with their unsaved changes,
LockNote tries to save again every 10 seconds, and when you type your
password they come back with their changes and the reason of the failed
save. To drop the changes, show the notes, then close without saving.

LockNote cannot be closed while the notes are hidden (Quit says so on that
screen). A Windows logoff or shutdown follows its usual rule
([section 6](#6-saving-and-closing)): the hidden notes are saved with their
changes, unless On close is **Never save** (then nothing is saved and those
changes are lost, since you chose never to save on close). If that save
fails, Windows shows LockNote's warning; a shutdown you force anyway loses
the unsaved changes.

Closing LockNote (tray Quit) while a window is open cancels that window
first, then closes as usual; if the window still has not closed after 10
seconds, LockNote names it and offers **Keep waiting** or **Close without
saving**.

While LockNote is locked, the menus only offer File > Quit and Help >
About.

### Auto-save

File > Settings > Preferences... > **Auto-save:** Off, 10, 30 (default), 60, 120
or 300 sec.

When there are unsaved changes, LockNote saves them at that interval. It never
saves while locked. A new value applies from the next start of LockNote.

### Clipboard auto-clear

File > Settings > Security... > **Clipboard clear:** Off, 10, 30 (default), 60
or 120 sec.

After that delay LockNote empties the clipboard, only if it still holds the
value it copied (it checks every 5 seconds). It applies to copies of a vault
secret (tree, **Copy** button, secret window) and of the password generator. It
does not apply to text copied from the editor, TOTP codes, prompts or the file
path. A new value applies from the next start of LockNote.

## 18. Global hotkey and system tray

**Global hotkey.** **Ctrl+Shift+L**, from any program, hides the LockNote
window or shows it again. When LockNote is locked, it shows the lock screen.
File > Settings > Preferences shows the hotkey; it cannot be changed there. If
another program already uses this combination, the hotkey does nothing.

**System tray.** File > Settings > Preferences > **Minimize to tray** (off by
default). When it is on, minimizing the window hides it; click the LockNote
icon in the notification area to bring it back. A right click on the icon
offers **Restore** and **Quit**.

## 19. Language and theme

**Language.** File > Settings > Preferences > **Language:** English, Français,
Español or Deutsch (default English). After OK: "Please restart LockNote to
apply the language change." The choice is saved with the note. The startup
windows stay in English ([section 1](#1-first-launch-and-password)).

**Theme.** File > Settings > Preferences > **Theme:** Dark (default) or Light.
It applies at once, without restart.

## 20. Safety backups

**What it does.** Before an action that replaces your notes, LockNote writes an
encrypted backup of the whole note, without asking:

- before **Import full backup** and **Import from another LockNote** ("Before
  import");
- before downloading an **update** ("Before update");
- when you ask for one ("Manual").

If that backup cannot be written, the action does not happen ("A safety backup
could not be written, so nothing was imported." / "...so the update was not
downloaded.").

**Where they are.** `%LOCALAPPDATA%\LockNote\auto-backups\`, as `.lnbak` files
encrypted with the note password of that moment.

**The window.** File > Safety backups...

- The list, newest first (date in UTC, kind, size), and the details of the
  selected backup.
- **Restore**: asks for the password the backup was made with, shows what it
  holds, writes a new "Before import" backup of the current note, then replaces
  the whole note. Save to keep the result.
- **Create backup now**: writes a manual backup ("Backup created.").
- **Delete**: deletes the selected backup after confirmation.
- **Open folder**: opens the folder in Explorer.
- **Close**.

**Retention.** After each new backup, LockNote deletes old automatic ones: it
always keeps the 3 newest of each kind ("Before import", "Before update"), then
deletes those older than 30 days, then the oldest until the folder holds at
most 1 GiB. Manual backups are never deleted automatically.

**After a password change.** Backups keep the password they were made with.
LockNote offers to delete them ("...can still be opened with it. Delete them
now?"). Answer No to keep them.

**Limit.** A restore replaces the whole note; you cannot restore one tab only.

## 21. Full backup (.lnbak)

**What it does.** Writes everything the note holds in one encrypted `.lnbak`
file, which any later LockNote can read.

| Included | Not included |
|---|---|
| Tabs (name, text, colour), prompts, vault (with your categories), attachments, settings (including TOTP keys and two-factor authentication) | Window position and size, side panel widths, active tab and caret line, keys loaded in ssh-agent |

### Export

File > Export full backup...

1. Confirm your password.
2. Choose the file. A name without extension gets `.lnbak`; an existing file
   asks before being replaced.
3. LockNote writes the file, reads it back to check it, then shows "Full backup
   saved to: ...".

The file is encrypted with the password of the note.

### Import

File > Import full backup...

1. Choose the `.lnbak` file.
2. **Password of this backup:** the password of the note the backup was made
   from. A wrong password lets you try again.
3. The confirmation shows the date and the numbers of tabs, prompts, secrets and
   attachments: "Replace ALL current content with this backup?"
4. A safety backup of the current note is written, then the content is
   replaced in the window.
5. "Backup imported. Save to keep it." Nothing is written until you save.

**Warnings.**

- Everything is replaced, settings and two-factor authentication included.
- The imported theme applies at once; the language applies after saving and
  restarting.
- A backup made by a newer LockNote is refused before any password: update
  LockNote. Files over 512 MiB are refused.

## 22. Import from another LockNote

**What it does.** Moves the data of another LockNote `.exe` into the one you are
running, for example to move to a new copy of the program.

**How to use it.** File > Import from another LockNote...

1. Choose the other `.exe` (not the one that is running). Choose it by its usual
   path, with the drive letter you always use: a pending save of that file is
   linked to its path, and LockNote reads that pending save when it is the
   latest one.
2. **Password of the LockNote file you are importing:**.
3. Confirm the replacement (numbers of tabs, prompts, secrets, attachments).
4. A safety backup of the current note is written, then the data is replaced.
5. Save. The data is then protected by the password of **this** file.

**What is not carried over.** Window position, active tab, caret line, keys
loaded in ssh-agent. The other file is only read, never modified.

## 23. Temporary files

**Where it is.** File > Settings > Temporary files...

**What it shows.**

- An explanation of the staging files ([section 2](#2-how-your-notes-are-stored))
  and the folder path, with **Open folder** and **Copy path**.
- **Waiting saves (.tmp):** each pending `.tmp`, with its date (UTC), size and
  status. Select one to see its details:
  - **This LockNote file**: the pending save of the file that is open. It is
    written into it when LockNote closes.
  - **Another LockNote file**: a save waiting for another LockNote file. Kept.
  - **Orphan: its LockNote file is gone**: the LockNote file no longer exists,
    but its folder does.
  - **Its folder is absent (drive disconnected?)**, **Its drive now names
    another disk**, **Its LockNote file cannot be checked**, **Left by an older
    version**, **Companion file unusable**, **Cannot be examined**: kept;
    reconnect the drive and open that file if it applies.
- **This LockNote file:** whether a save is waiting for the open file. When one
  is, **Close LockNote now** closes LockNote (with the usual save question) so
  that it is written into the file.
- **Swap log** and **Import log** open the log files.

**Delete orphan...** is enabled only for an orphan. After confirmation it
deletes that `.tmp` and its companion. **The notes it holds are lost.** Nothing
else in the folder is ever deleted by this window.

## 24. Updates

**Where it is.** Help > Check for Updates... LockNote checks only when you ask.

**How it works.**

1. LockNote reads the latest release of the public repository
   (github.com/ctardy/LockNote). If none is newer: "You are running the latest
   version (x.y.z)."
2. Otherwise: "A new version (x.y.z) is available. Would you like to download
   it?" Answer Yes.
3. A safety backup ("Before update") is written. If it fails, nothing is
   downloaded.
4. LockNote downloads `LockNote-windows-x64.zip` and checks it: the list of
   hashes `SHA256SUMS.txt` must carry a valid Ed25519 signature, and the
   archive must match its hash. Otherwise the update stops with an error.
5. The new program is placed in the staging file with your notes.
   "Restart LockNote now to apply it?"
   - **Yes**: LockNote closes (with a final save), the program file is
     replaced, and LockNote starts again.
   - **No**: the program file is replaced the next time LockNote closes.

Your notes are kept. If the replacement fails at that exit, a message says that
LockNote was not restarted and names the log file.

## 25. Migrating from LockNote 1.2.x

LockNote 2.0 reads the files of 1.2.x (exe and `.lnbak`).

1. **Update every LockNote copy you have.** A LockNote 1.2.1 deletes, when it
   starts, the pending `.tmp` files of the other copies (older than one minute)
   in `%LOCALAPPDATA%\LockNote`, which may hold their latest save. Do not keep
   1.2.1 copies next to 2.0 ones.
2. **If Help > Check for Updates of your copy offers 2.0**, accept: it keeps
   your notes. Before that, check that no line of your note is exactly a
   LockNote block header (`[LOCKNOTE_VAULT]`, `[LOCKNOTE_PROMPTS]`,
   `[LOCKNOTE_ATTACHMENTS]`, `[LOCKNOTE_HISTORY]`, `[LOCKNOTE_STATE]`, or
   `[LOCKNOTE_TABS]` as the first line): 2.0 reads such a line as the start of
   that block, and the rest of the note may be lost at the next save. Change
   such a line (add a character).
3. **If your copy does not offer 2.0**, migrate by hand:
   1. In the old LockNote: File > Export full backup... (`.lnbak`), then close
      it.
   2. Put the new `LockNote.exe` next to the old exe, open it and choose a
      password.
   3. File > Import full backup... with the `.lnbak` of step 1. Or File >
      Import from another LockNote..., choosing the old exe by its usual path.
   4. Check the tabs, the vault, the TOTP codes, the prompts and the
      attachments, then save.
   5. Before deleting the old exe, check in File > Settings > Temporary
      files... that it has no pending `.tmp` left.

**Not imported:** the password hint of old versions, the session state (active
tab and caret line) and the keys loaded into ssh-agent.

**Old licence file.** `%LOCALAPPDATA%\LockNote\license.dat`, left by older
versions, is no longer used. You can delete it.

## 26. Settings reference

The settings are stored inside the encrypted note and travel in a full backup
(except the window and panel sizes). Click OK to apply, then save the note.

### File > Settings > Preferences...

| Section | Option | Values | Default | Applies |
|---|---|---|---|---|
| Appearance | Theme: | Dark, Light | Dark | at once |
| Appearance | Language: | English, Français, Español, Deutsch | English | after restart |
| Appearance | Editor font: | Consolas, Courier New, Lucida Console, Cascadia Mono, Segoe UI | Consolas | next start |
| Appearance | Font size: | 8, 9, 10, 11, 12, 14, 16, 18, 20, 24 | 11 | next start |
| Behavior | On close: | Ask, Always save, Never save | Ask | at once |
| Behavior | Save shortcut: | modifier(s) + A-Z or F1-F24 | Ctrl+S | at once |
| Behavior | Auto-save: | Off, 10, 30, 60, 120, 300 sec | 30 sec | next start |
| Behavior | Minimize to tray | on / off | off | at once |
| Behavior | Global hotkey: | shown only | Ctrl+Shift+L | - |

### File > Settings > Security...

| Option | Values | Default | Applies |
|---|---|---|---|
| Change Password... | opens Create Password (new password twice) | - | after saving |
| Min. password length: | 1, 4, 6, 8, 12, 16 | 6 (4 for notes made with versions that had no such setting) | next password creation or change |
| Auto-lock: | Off, 1, 5, 10, 15, 30, 60 min | 15 min | at once |
| Clipboard clear: | Off, 10, 30, 60, 120 sec | 30 sec | next start |
| Enable TOTP two-factor authentication | on / off | off | next start (after saving) |

**Change Password...**: "Password changed ✓" is shown in the window. The new
password protects the note once you click OK and save. LockNote then offers to
delete the safety backups made with the previous password.

### File > Settings > Temporary files...

See [section 23](#23-temporary-files).

## 27. Keyboard shortcuts

| Shortcut | Action |
|---|---|
| Ctrl+S | Save (can be changed) |
| Ctrl+Q, Alt+F4 | Quit |
| Ctrl+F | Find |
| Ctrl+H | Find & Replace |
| Ctrl+G | Go to line |
| Ctrl+D | Duplicate line |
| Ctrl+Shift+K | Delete line |
| Ctrl+. | Add or remove a bullet on the line |
| Ctrl+A | Select all |
| Ctrl+Z, Ctrl+Y | Undo, redo |
| Ctrl+X, Ctrl+C, Ctrl+V | Cut, copy, paste |
| Ctrl+Shift+V | Paste as plain text |
| F5 | Insert timestamp |
| Ctrl+T | New tab |
| Ctrl+W | Close tab |
| F2 | Rename tab |
| Ctrl+Tab, Ctrl+Shift+Tab | Next tab, previous tab |
| Ctrl+B | Vault panel |
| Ctrl+J | Attachments panel |
| Ctrl+L | Lock |
| Ctrl+P | Print |
| Esc | Back to the editor |
| Ctrl+Shift+L | Show or hide LockNote, from any program |

In dialogs, Enter validates and Esc cancels.

## 28. What is written unencrypted to disk

LockNote never writes your notes in clear, except when you ask for it:

- File > Export... (text file of the active tab);
- **Open** and **Save** of an attachment (a copy of the file);
- **Export...** of an SSH key (the private key file is protected by its
  passphrase if you gave one, and readable by your account only).

The staging files, the safety backups, the full backups and the images made by
the steganography are encrypted.
