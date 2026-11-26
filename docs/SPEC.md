# LockNote — Complete Specification

> **Purpose**: Exhaustive specification for a ground-up rewrite of LockNote (Public + Pro) in Rust.
> **Generated**: 2026-04-11 — from audit of the C#/.NET 4.8 codebase.
> **Goal**: Single native binary, no runtime dependencies, same feature set.
>
> **Since 2.0 (issue 059)**: one edition, free and open source (GPL-3.0-or-later), every feature always built; there is no Cargo feature any more. What this document says of the Public / Free edition describes LockNote up to 1.2.x, kept because 2.0 reads its files. The C#/.NET parts describe the original codebase.

---

## Table of Contents

1. [Product Overview](#1-product-overview)
2. [Architecture](#2-architecture)
3. [Binary Format](#3-binary-format)
4. [Cryptography](#4-cryptography)
5. [Data Serialization Pipeline](#5-data-serialization-pipeline)
6. [Settings System](#6-settings-system)
7. [Tab System](#7-tab-system)
8. [Vault System](#8-vault-system)
9. [TOTP / 2FA](#9-totp--2fa)
10. [Compression](#10-compression)
10b. [Steganography](#10b-steganography)
10c. [SSH Keys](#10c-ssh-keys)
11. [UI Specification](#11-ui-specification)
12. [Internationalization](#12-internationalization)
13. [Auto-Lock, Clipboard Guard, Auto-Save](#13-auto-lock-clipboard-guard-auto-save)
14. [Updater](#14-updater)
15. [Lifecycle & File Swap](#15-lifecycle--file-swap)
16. [Keyboard Shortcuts](#16-keyboard-shortcuts)
17. [Feature Matrix](#17-feature-matrix)
18. [Constants & Magic Values](#18-constants--magic-values)
19. [Security Properties](#19-security-properties)
20. [Backward Compatibility](#20-backward-compatibility)
21. [Test Coverage](#21-test-coverage)
22. [Rust Rewrite Considerations](#22-rust-rewrite-considerations)

---

## 1. Product Overview

**LockNote** is a self-contained encrypted notepad for Windows. A single `.exe` serves as both the text editor and the encrypted vault. Notes are stored inside the executable itself after a binary marker. No installation, no dependencies, no plaintext on disk. A save goes first to an encrypted staging file (`%LOCALAPPDATA%\LockNote\{hash}.tmp`), which is moved over the executable when LockNote exits (§15).

### Editions

One edition since 2.0, distributed as `LockNote.exe` in the releases of github.com/ctardy/LockNote. Up to 1.2.x there were two: Public (LockNote, free) and Pro (LockNote Pro, paid, with the features marked Pro then).

### Core Promise

- **Single file** = program + encrypted data
- **Zero install** — runs from USB, desktop, anywhere (the save reaches the .exe on another volume than `%LOCALAPPDATA%` too: USB key, second disk, network share, §15)
- **No plaintext on disk** — the payload is only ever written encrypted (the staging `.tmp` holds the same encrypted image as the .exe)
- **Portable** — copy the .exe, your notes travel with it. Copy it after LockNote has exited and the swap is done: until then the latest save is in the staging `.tmp` of that computer, not in the .exe (§15)

---

## 2. Architecture

### Current Stack (C#)

| Component | Technology |
|-----------|-----------|
| Language | C# 5 / .NET Framework 4.8 |
| UI | WinForms |
| Crypto | AES-256-CBC + HMAC-SHA256 (System.Security.Cryptography) |
| KDF | PBKDF2-SHA256, 100,000 iterations |
| Build | csc.exe (built into Windows, no SDK) |
| Platform | Windows x64 only |

### Build-Time Model (C#, Pro up to 1.x)

Pro uses **build-time file replacement**: `src-pro/EditorForm.cs` replaces the public `src/EditorForm.cs` at compile time. Additional Pro files (`TabBar.cs`, `Lang.cs`, `VaultPanel.cs`, etc.) are only compiled in the Pro build. The public and Pro builds share the same core files:

**Shared files**: Program.cs, Storage.cs, Crypto.cs, Theme.cs, LineNumberTextBox.cs, SearchBar.cs, CreatePasswordDialog.cs, UnlockDialog.cs, CloseConfirmDialog.cs, GoToLineDialog.cs, AboutDialog.cs, Updater.cs

**Pro-replaced**: EditorForm.cs, Settings.cs, SettingsDialog.cs

**Pro-only**: TabBar.cs, TabStore.cs, NoteTab.cs, RenameTabDialog.cs, Lang.cs, Compression.cs, VaultPanel.cs, SecretEntry.cs, SecretStore.cs, SecretEditDialog.cs, Totp.cs, TotpEntry.cs, TotpSetupDialog.cs, ProFeatures.cs

---

## 3. Binary Format

### Executable Structure

```
┌──────────────────────────────┐
│  Original .exe binary        │  (PE executable)
├──────────────────────────────┤
│  Marker (16 bytes)           │  XOR-obfuscated sentinel
├──────────────────────────────┤
│  Encrypted payload (N bytes) │  AES-256-CBC ciphertext
└──────────────────────────────┘
```

### Marker

16-byte sentinel, stored XORed with `0xAA` to avoid literal string in the binary:

```
Raw marker:   4C 4E 5F 44 41 54 41 5F DE AD BE EF CA FE F0 0D
              L  N  _  D  A  T  A  _  (magic bytes)
XOR key:      0xAA (repeated)
Stored as:    E6 E4 F5 EE EB FE EB F5 74 07 14 45 60 54 5A A7
```

The marker is reconstructed at runtime via `byte[i] ^ 0xAA`.

### Marker Search

- **Direction**: backward linear scan from EOF
- **Returns**: byte offset of marker start, or -1 if not found
- **Rationale**: marker is always near EOF; backward scan is fastest

### Minimum Sizes

| Component | Size |
|-----------|------|
| Marker | 16 bytes |
| Salt | 16 bytes |
| IV | 16 bytes |
| HMAC | 32 bytes |
| Min ciphertext | 16 bytes (one AES block) |
| **Min payload** | **80 bytes** |
| **Min appended data** | **96 bytes** (marker + payload) |

---

## 4. Cryptography

### Algorithm Suite

| Function | Algorithm | Parameters |
|----------|-----------|-----------|
| Encryption | AES-256-CBC | PKCS7 padding |
| Authentication | HMAC-SHA256 | Encrypt-then-MAC |
| Key Derivation | PBKDF2-SHA256 | 100,000 iterations |
| Salt | Random | 16 bytes |
| IV | Random | 16 bytes |

The table and the wire format below describe the C# format. The Rust code (`src/crypto`) derives the key with **Argon2id** (`DEFAULT_M_COST` 65,536 KiB = 64 MiB, `DEFAULT_T_COST` 3, `DEFAULT_P_LANES` 4), stored in the 12-byte Argon2 params field of the payload (`MIN_PAYLOAD_SIZE` 92). The About box states these constants (`about::crypto_summary`).

### Key Derivation

```
Input:  password (UTF-8 string), salt (16 random bytes)
Output: 64 bytes of key material

PBKDF2-SHA256(password, salt, iterations=100000, outputLen=64)
  → encKey = bytes[0..31]   (32 bytes — AES-256 key)
  → macKey = bytes[32..63]  (32 bytes — HMAC-SHA256 key)
```

### Encryption (Encrypt)

```
1. Generate salt[16] — cryptographic random
2. Generate iv[16]   — cryptographic random
3. Derive (encKey, macKey) from (password, salt)
4. ciphertext = AES-256-CBC(encKey, iv, plaintext_utf8, PKCS7)
5. hmac = HMAC-SHA256(macKey, salt || iv || ciphertext)
6. output = salt || iv || hmac || ciphertext
7. Clear: encKey, macKey, salt, iv, plaintext bytes
```

### Decryption (Decrypt)

```
1. Validate: len(data) ≥ 80
2. Extract: salt = data[0..15], iv = data[16..31], stored_hmac = data[32..63], ciphertext = data[64..]
3. Derive (encKey, macKey) from (password, salt)
4. computed_hmac = HMAC-SHA256(macKey, salt || iv || ciphertext)
5. If NOT constant_time_equals(stored_hmac, computed_hmac) → return null
6. plaintext = AES-256-CBC-Decrypt(encKey, iv, ciphertext, PKCS7)
7. Clear: encKey, macKey, all intermediate buffers
8. Return plaintext as UTF-8 string
```

### Wire Format

```
Offset  Size     Field
0       16       Salt (random)
16      16       IV (random)
32      32       HMAC-SHA256 (over salt + iv + ciphertext)
64      variable Ciphertext (AES-256-CBC, PKCS7 padded)
```

### Constant-Time Comparison

```
function constant_time_equals(a, b):
    if len(a) != len(b): return false
    result = 0
    for i in 0..len(a):
        result |= a[i] ^ b[i]
    return result == 0
```

---

## 5. Data Serialization Pipeline

### Full Pipeline

```
                    ┌─────────────────────────────────┐
                    │  Settings header                 │
                    │  [LOCKNOTE_SETTINGS]             │
                    │  key=value...                    │
                    │  [/LOCKNOTE_SETTINGS]            │
                    ├─────────────────────────────────┤
                    │  Tab data                        │
Plaintext           │  [LOCKNOTE_TABS]                 │
(UTF-8)             │  [TAB:Name:color]content...      │
                    │  [/LOCKNOTE_TABS]                │
                    ├─────────────────────────────────┤
                    │  Prompts (optional)              │
                    │  [LOCKNOTE_PROMPTS]              │
                    │  [PROMPT:name:cat]text[/PROMPT]  │
                    │  [/LOCKNOTE_PROMPTS]             │
                    ├─────────────────────────────────┤
                    │  Vault data (optional)           │
                    │  [LOCKNOTE_VAULT]                │
                    │  [SECRET:name:cat]value[/SECRET] │
                    │  [/LOCKNOTE_VAULT]               │
                    ├─────────────────────────────────┤
                    │  Attachments (optional)           │
                    │  [LOCKNOTE_ATTACHMENTS]           │
                    │  [FILE:name|size:N|sha256:hex]   │
                    │  <base64 content>                 │
                    │  [/FILE]                          │
                    │  [/LOCKNOTE_ATTACHMENTS]          │
                    └────────────┬────────────────────┘
                                 │
                    ┌────────────▼────────────────────┐
                    │  GZip + Base64                    │
        Compressed  │  "LNGZ:" prefix                  │
                    └────────────┬────────────────────┘
                                 │
                    ┌────────────▼────────────────────┐
                    │  AES-256-CBC + HMAC-SHA256        │
        Encrypted   │  [salt][iv][hmac][ciphertext]    │
                    └────────────┬────────────────────┘
                                 │
                    ┌────────────▼────────────────────┐
                    │  [exe binary][marker][payload]    │
        On disk     │  Single .exe file                │
                    └─────────────────────────────────┘
```

### Serialization Order (encrypt)

Steps 1 to 4b are `features::payload::compose(tabs, prompts, secrets, attachments, state)`, a pure function called by `on_save`:

1. Tab text: `tabs::serialize(tabs)` → tab block. Exception: the default single tab (one tab, named `Note 1`, no color) is written as its plain text, without the block, unless the reading chain could take that text for a block (see *Default single tab* under *Reserved markers, escaping and block recognition* below)
2. (No longer written.) Versions with snapshots wrote the history block here: `snapshots::combine(tabData, snapshots)` → tabs + history block. Old files still hold it and it is still read
2b. `prompts::combine(tabData, prompts)` → tabs + `[LOCKNOTE_PROMPTS]` block (issue 058; omitted when there is no prompt, so a note without prompts is written as before)
3. `vault::combine(tabData, secrets)` → tabs + vault block
4. `attachments::combine(data, attachments)` → + attachments block
4b. `session_state::combine(data, state)` → + `[LOCKNOTE_STATE]` block (active tab + caret line + one `agent_key=` line per SSH key loaded into ssh-agent; omitted when all are empty/0)
5. `Settings.PrependTo(combined)` → settings header + all blocks
6. `Compression.Compress(payload)` → `"LNGZ:"` + Base64(GZip(payload))
7. `Crypto.Encrypt(compressed, password)` → `[salt][iv][hmac][ciphertext]`
8. `Storage.WriteData(exePath, encrypted)` → write to .tmp staging file

### Deserialization Order (decrypt)

1. `Storage.ReadData(filePath)` → encrypted bytes (from .tmp or .exe)
2. `Crypto.Decrypt(data, password)` → compressed string (or null if wrong password)
3. `Compression.Decompress(text)` → plaintext (passes through if no `"LNGZ:"` prefix)
4. `Settings.ParseFrom(text, out noteText)` → settings + remaining text
5. `features::payload::parse_payload(noteText)` — the single pipeline used at startup and by both unlock paths:
   1. `session_state::parse` → `[LOCKNOTE_STATE]` (outermost block, parsed first)
   2. `attachments::parse` → attachments list + remaining data
   3. `vault::parse` → secrets list + remaining data (vault sits after the history block and the prompts block)
   4. `snapshots::parse` → snapshots list + remaining data
   5. `prompts::parse` → prompts list + remaining data. The history block (old files) and the prompts block both sit between the tabs and the vault and never coexist (no version writes both). History first: an old snapshot was written before `[LOCKNOTE_PROMPTS]` was reserved and may hold that line unescaped, while prompt texts and names never hold a history header line
   6. `tabs::parse` → list of NoteTab objects (always at least one)
6. `payload::build_view` → active tab + caret line: the state captured in memory at lock time wins over the disk one, index clamped to the tab count

### Reserved markers, escaping and block recognition

The payload mixes structure (the blocks) and user text (a tab, a prompt, a vault value, the content of an old snapshot). Two rules keep them apart; both live in `src/features/markers.rs`, shared by every block (issue 050).

**Reserved markers** (one list, `markers::RESERVED`):

| Block | Header | Footer | Inner markers |
|---|---|---|---|
| Tabs | `[LOCKNOTE_TABS]` | `[/LOCKNOTE_TABS]` | `[TAB:` |
| History (read only) | `[LOCKNOTE_HISTORY]` | `[/LOCKNOTE_HISTORY]` | `[SNAP:`, `[/SNAP]` |
| Prompts (issue 058) | `[LOCKNOTE_PROMPTS]` | `[/LOCKNOTE_PROMPTS]` | `[PROMPT:`, `[/PROMPT]` |
| Vault | `[LOCKNOTE_VAULT]` | `[/LOCKNOTE_VAULT]` | `[SECRET:`, `[/SECRET]` |
| Attachments | `[LOCKNOTE_ATTACHMENTS]` | `[/LOCKNOTE_ATTACHMENTS]` | `[FILE:`, `[/FILE]` |
| Session state | `[LOCKNOTE_STATE]` | `[/LOCKNOTE_STATE]` | — |

A line is what `\n` separates; a `\r` before the `\n` (the editor produces `\r\n`) belongs to the line.

**Writing (escaping).** Every line of user text written inside a block that starts with *any* reserved marker (leading whitespace ignored, anything may follow the marker) is prefixed with a null byte `\x00`: `[LOCKNOTE_VAULT]` → `\x00[LOCKNOTE_VAULT]`, `  [TAB:x]` → `\x00  [TAB:x]`. This applies to the text of a tab, the text of a prompt and the value of a vault secret, with the same list (`markers::escape_content`). Known limit (all blocks): a line that already starts with `\x00` is not escaped and loses that byte on reload. A marker inside a line (`see [LOCKNOTE_VAULT] here`) is not escaped. Reading removes the `\x00` that starts a line (`markers::unescape_content`).

**Reading (recognition).** The header or footer of a block is recognized only when it is a whole line of its own: at the start of a line, followed by the end of the line (an optional `\r` is accepted). Inside a line, behind `\x00`, indented or followed by other characters, it is text (`markers::find_marker_line`). This is how the history, prompts, vault, attachments and session state blocks are found: the first such header line (`[LOCKNOTE_STATE]` and `[LOCKNOTE_PROMPTS]`: the last one, and the block must end the text its reader is given: only whitespace after its footer line). The tabs block is what is left once they are removed: its header must start that text, otherwise the text is the plain single tab. Inside a block, the inner markers and the footer are matched line by line as before (leading whitespace ignored, never on a line starting with `\x00`).

**Default single tab.** One tab named `Note 1` without a color is the note of someone who does not use tabs. It is written as plain text (no `[LOCKNOTE_TABS]` block, no escaping), which is what the Free edition and the older versions read. The text is written byte for byte. It is read back byte for byte when no block follows it and it does not open with a line break: `Settings::parse` drops the `\r` / `\n` opening the note text, and the reader of a block that follows the text trims the line breaks ending it (both older than this rule). The tabs block is less faithful at both ends of a text (line breaks ending it, an empty first line), which is why it is not used for every note.

Plain text cannot carry the `\x00` escape, so the tab is written in the tabs block like any other, escaped, when the reading chain could take a part of the plain text for a block. The decision is `payload::plain_text_may_be_misread`. It asks the block modules, with the markers and the search of their readers, but it is deliberately wider than what the readers recognize in the text as written, because they do not see it as written: the line breaks dropped by `Settings::parse` at the start and by a block reader at the end can turn a header next to a lone `\r` into a whole line. So for this decision, and only for it, a lone `\r` bounds a line like `\n` does (`markers::find_loose_marker_line`). The readers are unchanged.

| The text holds | Asked to | Compared with the reader |
|---|---|---|
| `[LOCKNOTE_ATTACHMENTS]`, `[LOCKNOTE_VAULT]` or `[LOCKNOTE_HISTORY]` between the start of the text, `\n` or `\r`, and the end of the text, `\n` or `\r` | `attachments::loose_header_line`, `vault::loose_header_line`, `snapshots::loose_header_line` | The reader (`header_line`) wants `\n` on both sides (`\r\n` accepted after). Same search, `\r` added as a bound |
| a `[/LOCKNOTE_STATE]` line with only whitespace after it, and before it a `[LOCKNOTE_STATE]` between the start of the text, `\n` or `\r`, and `\n` or `\r`, with no other `[/LOCKNOTE_STATE]` line in between | `session_state::may_end_with_block` | The reader (`block_span`) wants the header on a line of its own. Same footer, header search with `\r` added as a bound |
| a `[/LOCKNOTE_PROMPTS]` with only whitespace after it, and before it a `[LOCKNOTE_PROMPTS]`, both between the start of the text, `\n` or `\r`, and `\n` or `\r`, with no other `[/LOCKNOTE_PROMPTS]` line in between | `prompts::may_end_with_block` | The reader (`block_span`) wants both on a line of their own. The footer too gets `\r` as a bound: the reader of the block that follows trims the `\n` then the `\r` ending the text |
| `[LOCKNOTE_TABS]` at its very start (leading whitespace, line breaks included, ignored; anything may follow) | `tabs::starts_with_block` | The very test of `tabs::parse` |

Without a lone `\r` in the text the decision is exactly what the readers recognize (`switch_decision_against_what_the_readers_recognize`). Any other line starting with a reserved marker (`[TAB:`, `[FILE: x]`, `[/SECRET]`, a footer, an indented or continued header, `[LOCKNOTE_STATE]` or `[LOCKNOTE_PROMPTS]` without a whole block of its kind ending the text) is plain text for every reader and leaves the tab in plain text, lone `\r` or not. The wider decision only catches texts that hold one of the headers above next to a lone `\r`; versions up to 1.2.23 searched those headers anywhere in the text and cut such a text. Once in the tabs block, every line starting with any reserved marker is escaped (a line starting with `\r` then a marker too: `\r` is leading whitespace), as for any tab. It is read back as the same single `Note 1` tab.

**Compatibility.**

- Reading: a payload written by a previous version that it read back correctly is read the same. Before this rule each block only escaped its own markers and the parsers searched their header anywhere in the text; the headers they wrote were always whole lines.
- A file written *before* this rule whose text holds a line that is exactly a block header (unescaped: single tab, or a marker of another block) is still cut there: that line cannot be told from a real header. Saved by this version, the same text is escaped and comes back whole. A marker inside a line, which older versions cut too, is now read as text.
- Leaked `[LOCKNOTE_STATE]` blocks (issue 046): unchanged when read (a leaked block ending the payload is still taken as the session state; one followed by the real block stays in the text). No automatic cleaning. When this version saves a text that holds such a block, it is escaped and stays text.
- Downgrade: a version before this rule reads the new files, but cuts a tab or a value at a block header of another layer even though it is escaped (it searches the header anywhere). The full backup (`.lnbak`) is not concerned: it carries tabs and values as JSON strings, never this escaping.
- Prompts block (issue 058): versions up to 1.2.24 did not reserve `[LOCKNOTE_PROMPTS]`, so an existing note may hold that line, or a whole pasted prompts block, unescaped in a tab. Every payload this version writes has the prompts block at the very end of what is left once the vault is removed, so only a block there is read (the last header line, its footer followed by whitespace only); anywhere else the lines are text and the note is read whole, tabs block or plain tab (`old_*_prompts_*` tests in `features::payload`). Accepted limit, same as the leaked session block: an old plain tab whose text ends with a whole prompts block is read as that block. Downgrade to 1.2.24 once prompts are saved: see §8a.

### Public Edition up to 1.2.1 (simplified)

The public edition (up to 1.2.1, whose files 2.0 reads) skipped compression, tabs (single note), vault, and TOTP:

1. `Settings.PrependTo(noteText)` → settings + note
2. `Crypto.Encrypt(payload, password)` → encrypted bytes
3. `Storage.WriteData(exePath, encrypted)` → .tmp file

---

## 6. Settings System

### Serialization Format

```
[LOCKNOTE_SETTINGS]
key=value
key=value
...
[/LOCKNOTE_SETTINGS]
<note content follows>
```

### All Settings Keys

| Key | Type | Default up to 1.2.1 (Public) | Default | Values |
|-----|------|----------|---------|--------|
| `save_on_close` | enum | ask | ask | ask, always, never |
| `theme` | enum | dark | dark | dark, light |
| `word_wrap` | bool | on | on | on, off |
| `minimize_to_tray` | bool | on | on | on, off |
| `min_password_length` | int | 4 | 6 (a file without the key reads 4) | 1–64 |
| `font_family` | string | Consolas | Consolas | any font name |
| `font_size` | float | 11 | 11 | 6.0–72.0 |
| `language` | enum | — | en | en, fr, es, de |
| `active_tab` | int | — | 0 | 0+ |
| `auto_lock` | int | — | 15 | 0=disabled, minutes |
| `clipboard_clear` | int | — | 30 | 0=disabled, seconds |
| `auto_save` | int | — | 30 | 0=disabled, seconds |
| `show_line_numbers` | bool | — | on | on, off |
| `totp_2fa` | bool | — | off | on, off |
| `totp_2fa_secret` | string | — | "" | Base32 string |
| `totp_entry` | repeated | — | (none) | name\|BASE32SECRET |
| `hotkey_modifiers` | int | — | 6 | bitmask (1=Alt, 2=Ctrl, 4=Shift) |
| `hotkey_key` | int | — | 76 | virtual key code (76 = L) |
| `max_attachment_size_mb` | int | — | 10 | 1–50 (MB per file) |
| `max_total_attachments_mb` | int | — | 50 | 10–200 (MB total) |
| `window_x`, `window_y` | int | (unset) | (unset) | written only once saved (`window_x != i32::MIN`) |
| `window_width`, `window_height` | int | 900 / 650 | 900 / 650 | 200–8000 / 150–8000 |
| `window_maximized` | bool | off | off | on, off |
| `vault_panel_width`, `totp_panel_width`, `attach_panel_width`, `prompts_panel_width` | int | — | 300 / 280 / 280 / 320 | 220–8000 (clamped when read); written only when different from the default |
| `vault_category` | repeated | — | (none = 7 defaults) | category name, one key per category (issue 052) |
| `passgen_length` | int | — | 16 | 8–64 (clamped when read) |
| `passgen_upper`, `passgen_lower`, `passgen_digits`, `passgen_symbols` | bool | — | on | on, off |
| `passgen_exclude_ambiguous` | bool | — | off | on, off |
| `passgen_symbol_set` | string | — | ``!@#$%^&*()-_=+[]{};:,.?`` | distinct ASCII punctuation, may be empty; a value holding anything else is ignored |

**Window geometry.** The `window_*` keys hold `WINDOWPLACEMENT.rcNormalPosition`: the *window* rect (frame included) in *workspace* coordinates, captured at close (`GetWindowPlacement`; closing while minimized keeps the maximized state via `WPF_RESTORETOMAXIMIZED`). At startup the window is created hidden and the rect is re-applied with `SetWindowPlacement` (same rect and coordinate space, so no drift), then shown normal or maximized. Rules (`src/ui/window_geometry.rs`): the saved position is ignored when at least 64 px of the title bar would not be on a current monitor (default 900×650 at 300,200, still maximized if it was — e.g. an unplugged second screen); the size is clamped to the chosen monitor's work area (portable exe on a smaller screen); corrupted values that overflow fall back to the default. Not saved when closing while locked or after answering *No* to the save prompt (settings live in the encrypted payload). Moving between monitors of different DPI does not rescale the window (`WM_DPICHANGED` is not handled).

### Parsing Rules

- Missing header → all defaults, entire input is note text
- Null/empty input → all defaults, empty note
- Unknown keys → silently ignored (forward compatibility)
- Invalid numeric values → use default
- Font size clamped to [6, 72]
- Password length clamped to [1, 64]
- `totp_entry` can appear multiple times (one per TOTP entry)
- Leading CR/LF after `[/LOCKNOTE_SETTINGS]` is stripped

---

## 7. Tab System

### Data Model: NoteTab

| Field | Type | Description |
|-------|------|-------------|
| Name | string | Tab display name |
| Content | string | Current editor text |
| SavedContent | string | Last saved text (for dirty detection) |
| Color | string? | Color name or null |

**Modified** = `Content != SavedContent`
**MarkClean()** = sets `SavedContent = Content`

### Serialization Format

```
[LOCKNOTE_TABS]
[TAB:Tab Name]
Content of first tab
[TAB:Second Tab:blue]
Content of second tab
[/LOCKNOTE_TABS]
```

### Tab Colors

8 predefined colors:

| Name | RGB |
|------|-----|
| blue | (66, 133, 244) |
| green | (52, 168, 83) |
| red | (234, 67, 53) |
| orange | (251, 188, 4) |
| purple | (171, 71, 188) |
| yellow | (255, 235, 59) |
| pink | (236, 64, 122) |
| gray | (158, 158, 158) |

Color format in serialization: `[TAB:Name:color]` — color is optional.

### Escaping

A line of a tab's text that starts with a reserved marker is escaped with a null-byte prefix. The list is the one of the whole payload, not only the tab markers (§5, *Reserved markers, escaping and block recognition*):

| Line of the text starts with | Written as |
|---|---|
| `[TAB:` | `\x00[TAB:` |
| `[LOCKNOTE_TABS]`, `[/LOCKNOTE_TABS]` | `\x00[LOCKNOTE_TABS]`, `\x00[/LOCKNOTE_TABS]` |
| `[LOCKNOTE_HISTORY]`, `[/LOCKNOTE_HISTORY]`, `[SNAP:`, `[/SNAP]` | `\x00` + the line |
| `[LOCKNOTE_VAULT]`, `[/LOCKNOTE_VAULT]`, `[SECRET:`, `[/SECRET]` | `\x00` + the line |
| `[LOCKNOTE_ATTACHMENTS]`, `[/LOCKNOTE_ATTACHMENTS]`, `[FILE:`, `[/FILE]` | `\x00` + the line |
| `[LOCKNOTE_STATE]`, `[/LOCKNOTE_STATE]` | `\x00` + the line |

Unescape: remove the `\x00` that starts a line during parse. Versions up to 1.2.23 only escaped the first two rows.

### Tab Numbering

New tabs are named `"Note N"` where N = `max(existing "Note N" numbers) + 1`.

### Legacy / Backward Compatibility

- Input with no `[LOCKNOTE_TABS]` header → single tab named "Note 1"
- Null input → single empty tab named "Note 1"
- Writing: the default single tab (`Note 1`, no color) is saved as plain text without the block, unless the reading chain could take that text for a block (a lone `\r` counts as a line boundary in that decision); it is then saved in the block, escaped (§5, *Default single tab*)
- Public edition extracts first tab only when reading Pro-formatted data

---

## 8. Vault System

### Data Model: SecretEntry

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| Name | string | — | Secret identifier (e.g., "Gmail") |
| Value | string | — | Secret value (password, API key, etc.) |
| Category | string | "General" | Organization category |

### Predefined Categories

General, Email, Cloud, Finance, API Keys, Passwords, Crypto

### Serialization Format

```
[LOCKNOTE_VAULT]
[SECRET:Gmail:Email]
mypassword123
[/SECRET]
[SECRET:AWS Key:Cloud]
AKIA1234567890EXAMPLE
[/SECRET]
[/LOCKNOTE_VAULT]
```

- Category is optional: `[SECRET:Token]` → category = "General"
- Multi-line values are supported (SSH keys, PEM certs, JSON)

### Escaping

Same rule and same list as tabs (§5, *Reserved markers, escaping and block recognition*): a line of a secret's value that starts with any reserved marker of the payload gets a null-byte prefix.

| Line of the value starts with | Written as |
|---------|---------|
| `[SECRET:` | `\x00[SECRET:` |
| `[/SECRET]` | `\x00[/SECRET]` |
| `[LOCKNOTE_VAULT]`, `[/LOCKNOTE_VAULT]` | `\x00[LOCKNOTE_VAULT]`, `\x00[/LOCKNOTE_VAULT]` |
| any other reserved marker (tabs, history, attachments, session state) | `\x00` + the line |

Versions up to 1.2.23 only escaped the vault's own markers: a value holding a `[LOCKNOTE_ATTACHMENTS]` line lost what followed, and the secrets after it.

The `[LOCKNOTE_VAULT]` header is recognized only as a whole line of its own (§5).

### Combination with Tabs

`SecretStore.Combine(tabData, secrets)` appends the vault block after the tab block:

```
[LOCKNOTE_TABS]...[/LOCKNOTE_TABS]
[LOCKNOTE_VAULT]...[/LOCKNOTE_VAULT]
```

If no secrets, only tab data is returned.

### UI Behavior (VaultPanel)

(Rewritten for issue 048; the former card layout described the C# edition.)

- **Tree, one level** (`nwg::TreeView`, `src/ui/vault_panel.rs`): every category of the user's list is always listed, even empty, sorted by name (`CompareStringEx`); each secret is a leaf under its category
- **Search field** above the tree (`vault_panel::filter_groups`): filters as you type on the secret **name** only, anywhere in it, ignoring case and accents (`FindNLSStringEx`). Never the value (typing a password must not show which entry holds it), nor the category name. Empty (or spaces only): the whole tree, empty categories included; otherwise only the matching secrets and their categories. The selected secret stays selected when it still matches, without being copied again. A secret just added or edited that the search would hide clears the search (`search_hides`), so it shows selected. Also cleared when the category list changes (a new or renamed category holds no match and would not show), on backup import (the imported vault shows whole) and on lock. A failed comparison shows the tree error line, never a silently empty tree
- **Categories are editable** (issue 052, right click in the tree): *New category…* everywhere, *Rename…* / *Delete* on a category node. Names: 1–40 characters, no `: [ ]` or line break, unique ignoring case and accents, 50 categories max. **General** is the fallback: it always exists and can be neither renamed nor deleted; deleting a category moves its secrets to General after confirmation; renaming moves them to the new name. The 7 defaults (General, Email, Cloud, Finance, API Keys, Passwords, Crypto, in English) are only the starting list
- **Storage**: the list is saved in the settings block as repeated `vault_category=<name>` keys (absent = the 7 defaults). A secret keeps `[SECRET:Name:Category]`; on load a category is accepted only if it is in the list. Downgrade note: v1.2.14 and older only know the 7 defaults and read `[SECRET:Gmail:Perso]` as name `Gmail:Perso` in General (values are not lost)
- **Leaf label = secret name only.** A value never goes into the tree (no tooltip, UI Automation or control-heap exposure)
- **Sort**: names compared with `CompareStringEx` (user locale, case- and accent-insensitive), ties kept in storage order. Display only: the stored order and the saved payload are unchanged (compatible with v1.2.14)
- **Always expanded**: the tree is rebuilt fully expanded after load, unlock, Add, Edit and Delete; the expanded state is not saved
- **Buttons** (below the tree): Add always enabled; Edit, Delete, Copy, Reveal enabled only when a secret (leaf) is selected
- **Selecting a secret copies its value** (clipboard guard applies); selecting a category copies nothing. The Copy button does the same on demand. Double-click and F2 do nothing (`TVS_EDITLABELS` removed). Reverses decision (t) of issue 048 (user request, 2026-10-02)
- **"Copied" indicator**: after a copy, a line under the tree reads "Copied: <name>" (i18n `vault.copied.named`, name only, never the value) for 2 seconds, then clears
- **Open at launch**: the panel is opened when the app starts (not remembered; close with Ctrl+B or the toolbar button)
- **Edit/New dialog**: the value is masked on one line; "Show" switches to a multi-line field (the whole value, all lines) and a Copy button copies the value from either view (clipboard guard applies, kept out of Win+V history)
- **Drag & drop** (issue 053): drag a secret onto a category, or onto another secret, to move it to that category; the target is highlighted while dragging. Categories are not draggable. Dropping on nothing or on the same category, Escape, or losing the mouse capture changes nothing
- **Reveal**: the value is shown for 5 seconds in a read-only field under the tree, then cleared and hidden (also on Add/Edit/Delete, panel close and lock)
- **Lock**: the reveal field is cleared, secrets are dropped (zeroized) and the tree emptied; unlock reloads them
- **Error**: if the tree cannot be built (unknown category, comparison failure) a single translated node `vault.tree.error` is shown, never an empty tree
- Leaf lParam = storage index + 1; categories carry 0. No tree item handle is kept across rebuilds

---

## 8a. Prompts

Issue 058. Feature flag: `prompts` (in `pro`). Modules: `src/features/prompts/mod.rs` (block), `src/ui/prompts_panel.rs` (state, pure helpers), `src/ui/editor/prompts_ops.rs` (panel actions), `src/ui/dialogs/prompt_edit.rs` (Add / Edit dialog).

### Data Model: PromptEntry

| Field | Type | Description |
|-------|------|-------------|
| name | string | Shown in the list; no line break or NUL |
| text | string | The prompt (several lines); wiped on drop (`ZeroizeOnDrop`), at most 1 MiB (`MAX_PROMPT_TEXT`) |
| category | string | Free text (no list, unlike the vault); no `:`, line break or NUL |

### Serialization Format

```
[LOCKNOTE_PROMPTS]
[PROMPT:Summarize:Writing]
Summarize this text
in three points
[/PROMPT]
[/LOCKNOTE_PROMPTS]
```

- Written right after the tabs, before the vault (§5); no block when there is no prompt.
- The category is what follows the **last** `:` of the header line, the name what precedes it (the vault reads `[SECRET:Name:Category]` the same way). So a name may hold `:` (`host:port review`) and `]`; a category with `:` is refused before it is written (dialog, and `Corrupt` at import). A header without `:` (never written) is a name in `General`.
- Escaping: same rule and same list as tabs and vault values (§5). The text is read back byte for byte, leading empty lines and final line breaks included (the vault reader drops one final line break).
- Recognition: the block is read only when it ends the text left once the vault is removed: last `[LOCKNOTE_PROMPTS]` line, first `[/LOCKNOTE_PROMPTS]` line after it, only whitespace after that (§5, the rule of the session block). A header without its footer, or a block followed by other text, is no block: those lines stay tab text, nothing is dropped.
- Reader bound: inside a block, a prompt without `[/PROMPT]` ends at the footer; one over 2 MiB of escaped text is dropped and reading stops (`WARN prompts:` log); the prompts read before are kept.

### Downgrade to 1.2.24 (decision of issue 058)

1.2.24 does not know the block. Its tabs, vault, attachments and session state stay intact in every case (`downgrade_*` tests in `features::payload`); **the prompts are not reliably kept in any case**:
- **Single default tab** (plain text): its reader hands the prompts block to the tab as text, but 1.2.24 puts that text in the edit control as it is, and the `\x00` of the first escaped line (a prompt line starting with a marker) ends the text there (`WM_SETTEXT`). What 1.2.24 shows, and saves, is cut at that point.
- **Tabs block** (several tabs, a renamed or coloured tab): 1.2.24 stops reading at `[/LOCKNOTE_TABS]`, so it does not show the prompts, and a save by 1.2.24 drops them.

Rule: once prompts are saved, do not go back to 1.2.24. If a downgrade is needed anyway, export a full backup first: the `.lnbak` keeps the prompts for a later import by this version (1.2.24 itself ignores the `prompts` field when it imports the file, contract point 9 below).

### UI Behavior (PromptsPanel)

- **Toolbar button** "Prompts" (left-most, 60 px), badge = number of prompts, accent when the panel is open; **Tools > Prompts** menu item (access key R in English, French and German, P in Spanish: free in each Tools menu), checked while open. One side panel at a time (`close_other_panels("prompts")`); default width 320 px, resizable (`prompts_panel_width`, see **Side panel width**)
- **Search** field on top: filters on the name and the text, ignoring case and accents for the user's locale (`FindNLSStringEx`, `LINGUISTIC_IGNORECASE | LINGUISTIC_IGNOREDIACRITIC`, the flags of the vault sort); empty = every prompt. The status line shows "N prompt(s)" or "shown of total"
- **List**: one row per prompt, "Category · Name", sorted by category then name (`CompareStringEx`, same flags), storage order for ties. Display only: the stored order is unchanged
- **Preview**: read-only multi-line field with the text of the selected prompt
- **Buttons**: Add always; Edit, Delete, Copy when a prompt is selected. **A click** on a row = Copy (also on the row already selected; a click below the last row copies nothing). Moving the selection with the keyboard only updates the preview
- **Copy**: the text goes to the clipboard with `\r\n` line ends (`CF_UNICODETEXT`); "Copied: <name>" in the status line. A prompt is not a secret: the clipboard guard is not armed (no automatic clearing) and the copy is not hidden from the clipboard history
- **Add / Edit dialog**: Name (required), Category (text field, with a list of the categories in use, translated "General" first; choosing one fills the field), Text (multi-line: Enter adds a line, Tab adds a tab, Ctrl+Tab leaves the field). A typed or changed text is stored with `\n` line ends; Edit then OK on an untouched text keeps it byte for byte. Explicit errors: empty name, category or text, `:` in the category, text over 1 MiB; nothing is filled in silently
- **Delete** asks for confirmation (name shown)
- **After a modal** (Edit dialog, Delete confirmation): the change applies only if the prompt is still the one the modal was opened on (an import may have replaced the list meanwhile); otherwise nothing changes, a message says so, `INFO prompts: edit|delete dropped`. The vault's Edit follows the same rule (`vault.edit.gone`)
- **Lock**: prompts dropped (zeroized), list, preview and search emptied; unlock reloads them from disk. A dialog left open across a lock changes nothing (issue 049 rule)
- **Theme**: frame handler for `WM_CTLCOLOREDIT/LISTBOX/STATIC` and `WM_ERASEBKGND`, palette read on every message (live theme switch included)
- **Logs**: `INFO prompts: add|edit|delete|copy count=N`; never a prompt text

---

## 8b. Attachments System

### Data Model: Attachment

| Field | Type | Description |
|-------|------|-------------|
| name | String | Original file name (e.g., "rapport.pdf") |
| data | Vec\<u8\> | Raw binary content |
| sha256 | String | SHA-256 hash of data, hex-encoded (64 chars) |

### Serialization Format

```
[LOCKNOTE_ATTACHMENTS]
[FILE:rapport.pdf|size:245760|sha256:abc123...]
<base64 content, split into 76-char lines>
[/FILE]
[FILE:photo.jpg|size:102400|sha256:def456...]
<base64 content>
[/FILE]
[/LOCKNOTE_ATTACHMENTS]
```

### Escaping

The block holds no user text: the content is base64, which cannot form a marker. The shared escaping of §5 is still applied to it. What matters here is the other direction: the `[LOCKNOTE_ATTACHMENTS]` header is recognized only as a whole line of its own, and the text of a tab or of a vault value that holds such a line is written escaped (`\x00[LOCKNOTE_ATTACHMENTS]`), so it is never taken for this block.

### Size Limits

| Limit | Default | Range |
|-------|---------|-------|
| Per file | 10 MB | 1–50 MB |
| Total | 50 MB | 10–200 MB |

Configurable via settings keys `max_attachment_size_mb` and `max_total_attachments_mb`.

Base64 encoding adds ~33% overhead, partially compensated by GZip compression.

### SHA-256 Verification

- Hash is computed at attachment creation (`Attachment::new`)
- Hash is verified at deserialization — corrupted files are silently skipped
- Hash is verified before extraction — user is warned if mismatch

### UI Behavior (AttachmentsPanel)

- Toggle: `Tools > Attachments`, `Ctrl+J`, or **📎 button in the menu bar**
- Right-side panel (280 px wide by default, resizable: see **Side panel width**), can coexist with Vault panel
- Positioned below the tab bar to avoid overlap
- ListBox showing "filename (size)" for each attachment
- Buttons: Add, Open, Save, Del
- Status label: "N file(s), total size"
- **Add**: file dialog, check limits, add to list
- **Open**: save dialog (pre-filled filename + extension filter via `GetSaveFileNameW`), write to chosen path, open with ShellExecuteW
- **Save**: save dialog (pre-filled filename + extension filter), write to chosen path
- **Del**: confirmation dialog, remove from list
- **Double-click** on list item: same as Open
- Panel follows dark/light theme (via `WM_ERASEBKGND` + `WM_CTLCOLORLISTBOX` handlers)

### Source Files

| File | Purpose |
|------|---------|
| `src/features/attachments/mod.rs` | Parse, serialize, combine, check_limits, Attachment struct |
| `src/ui/attachments_panel.rs` | AttachmentsPanel state, format_entry, status_text |
| `src/ui/editor/` | UI controls, event handlers, pipeline integration (see "Editor module layout") |
| `src/settings/mod.rs` | max_attachment_size_mb, max_total_attachments_mb |

### Editor module layout

`src/ui/editor/` is one Rust module: `mod.rs` holds the `EditorForm` struct and `run`, and every other file adds an `impl EditorForm` block for one feature. Private fields stay reachable (a child module sees its parent's private items); a method used from another file is `pub(super)`. New behavior goes in the file of its feature, not in `mod.rs`.

| File | Holds |
|---|---|
| `mod.rs` | `EditorForm` struct, `run`, the module list |
| `build.rs`, `events.rs` | control construction (`build`), event wiring (`bind_events`) |
| `content.rs` | content get/set, modified flag, status bar, line-number gutter |
| `surface.rs` | editor surface: line spacing, current-line band, dark scrollbars (subclass of the RichEdit) |
| `toolbar.rs` | tab-strip buttons (Lock, Attach, TOTP, Vault, Prompts): state colours, padlock glyph, hover tracking, tooltips |
| `panel_grip.rs` | resize grip of the side panels: grip window, mouse drag (geometry in `ui/side_panel.rs`) |
| `tab_paint.rs`, `layout.rs` | tab bar painting, control layout |
| `file_ops.rs` | save, plain-text export |
| `backup_ops.rs` | full backups, import, restore, safety backups and their window |
| `stego_ops.rs`, `print_ops.rs`, `prefs_ops.rs` | steganography, printing, password / preferences / security |
| `edit_ops.rs` | view, edit, search/replace, go to line |
| `help_ops.rs` | updates, about, tray |
| `vault_ops.rs`, `ssh_ops.rs`, `attach_ops.rs`, `prompts_ops.rs` | vault, SSH keys, attachments and TOTP panels, prompts panel |
| `highlight.rs`, `close_flow.rs`, `tab_ops.rs` | occurrence highlighting, window state and close, tab operations |
| `lock_ops.rs`, `lock_overlay.rs` | auto-lock, lock overlay methods and its window procedure (the look is `ui/lock_card.rs`, shared with `dialogs/unlock.rs`) |
| `runtime_ops.rs` | auto-save, clipboard guard, global hotkey |
| `helpers.rs` | free helpers: fonts, dates, placement, UTF-16 offsets, search, file sniffing |
| `tests.rs` | unit tests; `all_source()` (test only) joins the module's files for tests that check the code as text |

### Feature Flag

`attachments` in `Cargo.toml`, included in `pro` feature set.

---

## 9. TOTP / 2FA

### Algorithm (RFC 6238 / RFC 4226)

| Parameter | Value |
|-----------|-------|
| Hash | HMAC-SHA1 |
| Period | 30 seconds |
| Digits | 6 |
| Secret length | 20 bytes (160 bits) |
| Encoding | Base32 (RFC 4648) |

### Code Generation

```
1. counter = floor(unix_timestamp / 30)
2. counter_bytes = counter as big-endian 8-byte integer
3. hash = HMAC-SHA1(secret, counter_bytes)
4. offset = hash[19] & 0x0F
5. code = ((hash[offset] & 0x7F) << 24
         | hash[offset+1] << 16
         | hash[offset+2] << 8
         | hash[offset+3]) % 1_000_000
6. return zero-padded 6-digit string
```

### Verification

- Tolerance: ±1 counter step (checks current, previous, and next period)
- Returns true if any of the 3 codes match

### Base32

- Alphabet: `ABCDEFGHIJKLMNOPQRSTUVWXYZ234567`
- Decoding accepts lowercase and padding (`=`)
- Empty input → empty output

### OtpAuth URI

```
otpauth://totp/{label}?secret={BASE32}&issuer={issuer}
```

- Label may include issuer prefix: `Issuer:user@example.com`
- Only `totp` type supported (not `hotp`)

### TotpEntry Serialization

Stored in settings as `totp_entry=Name|BASE32SECRET` (one line per entry).

### TOTP UI

- **Toolbar** in editor: lock icon 🔑 + dropdown (entries) + progress bar (30s countdown) + seconds label
- Dropdown displays: `"Name — ABC DEF"` (code split into two 3-digit groups)
- Progress bar: red text when ≤5 seconds remaining
- Selecting entry auto-copies code to clipboard (triggers clipboard guard)
- Buttons: + (add entry), − (remove selected entry)

### TOTP Setup Dialog

- Generates random 20-byte secret if none provided
- Displays: Base32 secret, otpauth URI, test code input
- Copy button for Base32 secret
- **Verify before confirm**: user must enter a valid code before the Confirm button enables
- Tolerance ±1 for verification
- **Add Key of the TOTP panel** (`TotpSetupDialog::show_add_key`, issue #23): the secret field is editable and starts empty (pass 2: a pre-filled secret turned a paste without select-all into a longer valid secret, and a forgotten paste stored a random one), label `totp.add.secret`, placeholder `totp.add.secret_hint`; **Generate** (`totp.add.generate`) fills it with `generate_secret()`. `totp::parse_secret_input` accepts Base32 (whitespace, lowercase, trailing `=` accepted; stored uppercase without them) or an `otpauth://totp/` link (scheme case-insensitive, `secret` percent-decoded on bytes and refused when the result is not UTF-8; label and issuer decoded the same way, never by `str` offsets); it refuses, with `SecretInputError`: `Empty`, `InvalidBase32` (outside the RFC 4648 alphabet, or no byte), `InvalidLength` (unpadded length of 8k+1, 8k+3 or 8k+6 characters, which no byte count gives: a cut or doubled paste), `TooShort` (under `MIN_SECRET_BYTES` = 10 bytes, 80 bits), `DoubledPaste` (an `otpauth://` link pasted twice; Base32 alone is not checked, since a real secret can have two equal halves, RFC 6238's own does), `DuplicateParameter` (the same parameter twice, two `secret=` included: never last-wins; parameters are compared once percent-decoded), `NotTotpUri` (`otpauth://hotp/`...), `UriWithoutSecret`, `UnsupportedParameters` (`algorithm` other than SHA1, `digits` other than 6, `period` other than 30: `generate_code` would produce codes the site never accepts). The URI field follows the typed secret (empty while invalid). Confirm is enabled from the start (an empty field is refused with `totp.secret.err.empty`) and stores the parsed secret; a refused secret shows its reason (`totp.secret.err.*`, logged `WARN totp: pasted secret refused`) and nothing is stored. Verify is optional in this mode (the site waits for LockNote's code, there is no other authenticator to compare with). The label typed in the URI is not used: the entry keeps the name typed before. The unlock 2FA setup (`show`) is unchanged

### 2FA for App Unlock

When `totp_2fa` is enabled:
- After correct master password, a TOTP code prompt appears
- Uses `totp_2fa_secret` as the shared secret
- Must verify before access is granted

---

## 10. Compression

### Format

```
"LNGZ:" + Base64(GZip(plaintext_utf8))
```

### Behavior

- **Compress**: UTF-8 encode → GZip → Base64 → prepend `"LNGZ:"`
- **Decompress**: if starts with `"LNGZ:"` → strip prefix → Base64 decode → GZip inflate → UTF-8 string
- **No prefix**: pass through unchanged (backward compatibility with uncompressed data)
- **Null input**: returns null
- **Empty input**: returns empty string
- Byte buffers cleared after compression

### Typical Ratio

60–80% compression for typical note text (repetitive prose, code).

---

## 10b. Steganography

### Overview

Hide encrypted LockNote payloads inside image files (JPEG, PNG, BMP). The encrypted payload is appended after the image's logical end marker. No magic bytes or identifiable markers are written — detection relies solely on HMAC-SHA256 verification from the existing crypto module, providing plausible deniability.

### Supported Formats

| Format | Magic bytes | Extensions (case-insensitive) | End marker | Detection method |
|--------|-----------|-------------------------------|-----------|-----------------|
| JPEG | `FF D8 FF` | .jpg, .jpeg, .jpe, .jfif | `FF D9` (EOI) | Forward scan through markers, byte-by-byte in SOS segments |
| PNG | `89 50 4E 47 0D 0A 1A 0A` | .png | `IEND` chunk + CRC | Sequential chunk traversal |
| BMP | `42 4D` ("BM") | .bmp, .dib | File size in header (bytes 2-5, u32 LE) | Direct read from header |

Format detection uses magic bytes first, with case-insensitive file extension as fallback.

### Binary Layout

```
┌──────────────────────────────┐
│  Original image (unchanged)  │  ← image viewers see this
├──────────────────────────────┤
│  Salt         16 bytes       │  ← random (indistinguishable from noise)
│  IV           16 bytes       │  ← random
│  Argon2 params 12 bytes      │  ← m_cost + t_cost + p_lanes (LE)
│  HMAC-SHA256  32 bytes       │  ← authentication
│  Ciphertext    N bytes       │  ← AES-256-CBC (PKCS7)
└──────────────────────────────┘
```

No marker is written. The appended data is the standard crypto wire format, indistinguishable from random noise without the password.

### Detection Algorithm

```
1. Detect image format (magic bytes, fallback to extension)
2. Find logical end of image (format-specific)
3. If no data after end → "No hidden data" (certain)
4. If data after end ≥ MIN_PAYLOAD_SIZE (92 bytes):
   → Attempt crypto::decrypt(payload, password)
   → HMAC match = valid LockNote data
   → HMAC mismatch = wrong password OR not LockNote data (ambiguous by design)
```

### Security Properties

1. **No detectable signature** — grep, strings, hex analysis reveal nothing identifiable
2. **Plausible deniability** — extra bytes after image end could be padding, corruption, or metadata
3. **Reuses existing crypto** — same Argon2id + AES-256-CBC + HMAC-SHA256 pipeline, no new attack surface
4. **Ambiguous failure** — cannot distinguish "wrong password" from "not LockNote data" without the correct password

### Constraints

- **Payload ratio warning**: embed rejected if encrypted payload > 50% of carrier image size
- **Maximum payload**: 100 MiB
- **Image recompression destroys data**: WhatsApp, Instagram, Discord etc. recompress images — a warning is shown at export
- **Image integrity**: the original image bytes up to the logical end are preserved unchanged

### UI Integration

- Submenu: **Tools → Steganography → Hide in Image...** (export)
- Submenu: **Tools → Steganography → Extract from Image...** (import)
- Password confirmation required before export
- Export hides the current tab's content only
- Extract loads notes into the current tab
- All messages translated (EN, FR, ES, DE)
- Feature flag up to 1.2.x: `steganography` (Pro only); always built since 2.0

### Module

```
src/features/stego/mod.rs
```

Functions:
- `detect_format(data) → Option<ImageFormat>` — identify JPEG/PNG/BMP from magic bytes
- `detect_format_by_extension(path) → Option<ImageFormat>` — case-insensitive extension fallback
- `find_image_end(data) → Result<(ImageFormat, usize)>` — locate logical end
- `find_image_end_with_path(data, path) → Result<(ImageFormat, usize)>` — with extension fallback
- `embed(image_data, encrypted) → Result<Vec<u8>>` — append payload
- `embed_with_path(image_data, encrypted, path) → Result<Vec<u8>>` — with extension fallback
- `extract(file_data, password) → Result<String>` — find end, decrypt
- `has_hidden_data(file_data) → bool` — quick check without decryption

---

## 10c. SSH Keys

Issue 054. Feature flag up to 1.2.x: `ssh-keys` (Pro only); always built since 2.0. Dependencies: `ssh-key 0.6.7` (OpenSSH formats, generation, passphrase encryption) and `ssh-encoding 0.2` (agent message layout).

### Generation

- Menu: **Tools → SSH Keys → Generate SSH Key...**
- Types: Ed25519 (default), ECDSA P-256, ECDSA P-384, RSA 3072, RSA 4096. Generated on a worker thread (RSA 4096 takes seconds).
- Comment: one line; line breaks, tabs and other control characters are refused (they would break the public line and the `agent_key=` line).
- Optional passphrase (+ confirmation, strength label): the private key is then encrypted in the standard OpenSSH format (`aes256-ctr` + `bcrypt-pbkdf`), readable by `ssh`, `ssh-keygen -y` and Git for Windows.
- Shows the `SHA256:` fingerprint (as `ssh-keygen -l`) and the public line.
- **Save to vault**: two secrets in category `SSH Keys` (created if missing; an existing category with the same name for the user's locale is reused): `<name>` = private key without its final newline (the vault drops it on reload), `<name>.pub` = public line. The name is trimmed; empty or already used (`<name>` or `<name>.pub`, exact match) is refused.
- **Export...**: writes `<file>` and `<file>.pub`, never overwriting. The private file is created with no sharing (`share_mode(0)`) and gets, through that handle and while still empty, a protected DACL granting the current user only — Windows OpenSSH refuses a private key other accounts can read. Without a passphrase, a warning asks for confirmation first.
- Closing the dialog without saving drops the key (private part zeroized).

### ssh-agent

- Menu: **Tools → SSH Keys → Load into ssh-agent...**: pick any vault secret holding an OpenSSH private key (any category: keys can be moved by drag & drop), type its passphrase, read the warning, Load.
- Transport: named pipe `\\.\pipe\openssh-ssh-agent` (Windows OpenSSH agent service), one connection per request, 5 s timeout (worker thread). Protocol: draft-miller-ssh-agent — `ADD_IDENTITY` (17), `REMOVE_IDENTITY` (18), `REQUEST_IDENTITIES` (11). No lifetime constraint: the Windows agent refuses it (`agent refused operation`, verified 2026-10-05).
- The Windows agent may keep keys outside LockNote (Microsoft: keys stored in the user's Windows security context). LockNote therefore tracks the keys it loaded: after an add, the key joins the session state (`agent_key=<type> <base64>` line in `[LOCKNOTE_STATE]`, comment dropped so free text cannot break the block) and the file is **saved at once**; if that save fails, the key is removed from the agent again. An add whose outcome is unknown (timeout, I/O, unreadable answer) is tracked as loaded too, as *unconfirmed* for the session: while absent from the agent it stays tracked (the request may still land), and it is removed once it shows up; only a refusal, a stopped service, a missing/wrong passphrase or an invalid key are definitive.
- Removal (only listed keys, never "remove all"): at lock (no modal; failures logged), at close, at unlock and at startup (keys left by an abnormal end). The identities are listed first, since the agent answers FAILURE both for "absent" and for a refusal: an absent key is dropped without a remove request; a key that could not be removed stays listed for the next attempt. A shorter list is saved at once, silently when nothing else is pending.
- A key added by hand with `ssh-add` that is the same key is removed too (identical keys cannot be told apart) — stated in the warning.
- Errors are explicit: service not running (with the PowerShell commands to enable it), refusal, timeout, wrong / missing passphrase.

### Module

```
src/features/ssh_keys/mod.rs      pure: generate, public_line_from_private, fingerprint, same_key,
                             vault_entries, agent message encode/decode, read_frame,
                             Agent trait + remove_loaded_keys (removal policy)
src/features/ssh_keys/win.rs      PipeAgent (named pipe + timeout), export_key_pair, restrict_to_current_user
src/ui/dialogs/ssh_keygen.rs SshKeygenDialog, SshAgentDialog, ssh_error_message
```

Interop tests (manual, `cargo test -- --ignored`): `ssh-keygen -y` / `-l` read exported keys (with and without passphrase), and a throwaway key goes through the real agent (add, list, remove, absent).

---

## 10d. Full backup

Feature flag: `pro`. Menu: **File > Export full backup... / Import full backup...**. Module: `src/features/backup/mod.rs`; dialogs in `src/ui/editor/backup_ops.rs` (`on_export_backup`, `on_import_backup`, `apply_backup`) and `src/ui/dialogs/backup_password.rs`.

The plain-text **Export...** only writes the active tab, unencrypted. The full backup carries everything the note holds and is always encrypted.

Verified by tests on every path: export (`to_json`), import (`parse_v1`), import from another exe (`decode_exe_payload`, through `parse_payload`), automatic safety backups (through `encode`), post-export check (prompt count compared).

### What it contains

| Block | Included | Notes |
|---|---|---|
| Tabs | yes | name, content, colour |
| Snapshots | no | removed feature: not created, not exported (written as an empty list); an old file that has some is read and the snapshots are ignored |
| Prompts | yes | name, text, category (issue 058, added inside format 1: absent from older files = no prompt) |
| Vault | yes | name, value, category (custom categories kept) |
| Attachments | yes | name + raw bytes |
| Settings | yes | including TOTP seeds and the user's vault categories |
| Window geometry, side panel widths, active tab | no | machine / session state (reset in `portable_settings`; an import keeps this machine's values) |
| Caret line, keys loaded in ssh-agent | no | session state; never part of a backup |

### File format

```
[MAGIC "LNBACKUP" 8][format u16 LE][crypto wire bytes]
crypto plaintext = compression::compress(JSON)
```

The crypto layer is the one used for the note itself (Argon2id + AES-256-CBC + HMAC-SHA256, see section 4), keyed with the **note password**. The file name extension is `.lnbak`.

JSON object (format 1):

```
{ "format": 1, "kind": "locknote-full-backup", "created_unix": N, "app_version": "x.y.z",
  "settings": "<[LOCKNOTE_SETTINGS] header text>",
  "tabs":        [ { "name", "content", "color"? } ],
  "snapshots":   [ { "timestamp", "name"?, "content" } ],
  "prompts":     [ { "name", "text", "category" } ],
  "vault":       [ { "name", "value", "category"? } ],
  "attachments": [ { "name", "data_b64" } ] }
```

JSON is used instead of the marker text of the exe payload: markers are fragile in free text (issue 050) and a block added by a later version would be glued into the tabs by an older reader.

### Compatibility contract (do not break)

1. `format` is the major version. A build reads every format up to `FORMAT` and writes `FORMAT`.
2. A file with a higher format is refused **before the password is asked** (`peek_format`), with a message asking to update LockNote.
3. Inside a format, additions are compatible: readers ignore unknown JSON fields; absent optional fields take their default (no tab at all gives one empty "Note 1", as `tabs::parse`).
4. Changing the meaning of an existing field, or removing one, bumps `FORMAT` and adds a reader for the old format next to the new one.
5. The format is repeated inside the authenticated JSON and must equal the clear header, so the header cannot be edited to downgrade.
6. Wrong types and structurally invalid content are errors (`BackupError::Corrupt`), never silently defaulted (`created_unix`, `app_version`, tabs, prompts, vault, snapshots, attachments). A prompt the note cannot hold (line break or NUL in its name or category, `:` in its category, text over 1 MiB) is `Corrupt` too. `kind` is informational and not checked. A wrong password and an altered file are the same error on purpose.
7. `src/features/backup/fixtures/v1_basic.lnbak` (password `fixture-v1`), `v1_all_settings.lnbak` (password `fixture-v1-settings`, every setting set, TOTP included) and `v1_prompts.lnbak` (password `fixture-v1-prompts`, issue 058: three prompts, `:` in a name, markers and CRLF in a text; written once by `json_tool` pack) are frozen format-1 files. `frozen_v1_fixture_still_decodes`, `frozen_v1_all_settings_fixture_still_decodes` and `frozen_v1_prompts_fixture_still_decodes` must pass forever (the first two also check they hold no prompt). Never regenerate them; a new format gets its own fixtures.
8. The `settings` string is the text of `Settings::serialize` and is read by `Settings::parse` (section 6: unknown keys ignored, invalid values fall back to defaults). That text is part of the format: a renamed key, a changed default or a narrower range must keep reading old backups the same way, or bump `FORMAT`. `Settings::parse` is the one tolerant reader; every other field of the backup is strict.
9. Known limit: a reader ignores fields a newer writer added inside the same format, so import followed by export drops them.
10. The escaping of the exe payload (`\x00` before a line starting with a reserved marker, section 5) is not part of this format: tabs, snapshots and vault values are JSON strings and carry such lines as they are (`reserved_markers_travel_as_plain_json_text`, `hostile_content_survives`). Issue 050 changed the exe payload only; `FORMAT` stayed 1.

Adding a field to `Settings` stops `every_settings_field_has_a_decision` from compiling until the field is classified: it travels (add it to `settings_roundtrip_every_field`) or it is machine state (reset it in `portable_settings`).

### Export flow

1. The note password is asked again (`ConfirmPasswordDialog`).
2. A name typed without extension gets `.lnbak`; if that file already exists, a confirmation is asked (the save dialog only checked the name as typed).
3. **Lock re-check.** Dialogs are modal while the auto-lock timer keeps ticking; if it fired, password and contents are already wiped. The export stops (`backup: export refused reason=locked`) instead of writing an empty backup encrypted with `""`. `backup::encode` also refuses an empty password.
4. The active tab is synced from the editor, the backup is built and encrypted. A body over `MAX_JSON_BYTES` (200 MiB) is refused: it could not be read back.
5. The bytes are written to `<file>.<pid>.tmp` (`create_new`, never an existing file), **read back and decoded** with the password, counts compared, then renamed to the final name. A failed check removes the temporary file; nothing half-written keeps the final name.
6. The password borrow ends before any message box (the auto-lock handler needs `borrow_mut` while one is open). The in-memory `Backup` copy is wiped (`Backup::wipe`) once the file is written.
7. Log lines (no content): `backup: export ok format= tabs= snapshots= secrets= attachments= prompts= bytes=`, `backup: export failed stage=`, `backup: export refused reason=locked`.

### Import flow

1. File picked, size capped (`MAX_FILE_SIZE`, 512 MiB), header checked (`NotABackup`, `NewerFormat`). Before any key derivation, the Argon2id cost announced by the (unauthenticated) header is checked against `MAX_KDF_*` (512 MiB, 20 iterations, 16 lanes), so a forged file cannot freeze the app.
2. The password **of the backup** is asked (it is the password of the note the backup was made from); a wrong one allows another try.
3. A confirmation shows the date and the counts (tabs, prompts, vault secrets, attachments), and says everything current is replaced, settings and two-factor authentication included (a backup made before 2FA was enabled turns it off).
4. **Lock re-check** after the last dialog: if auto-lock fired meanwhile, the import stops (`reason=locked`) rather than put plaintext back under the lock overlay. Then `apply_backup` replaces settings (window geometry stays this machine's), attachments, vault, prompts and tabs **in memory** (snapshots of an old backup are ignored). Nothing is written to disk until the usual save, so closing without saving (when the setting allows) keeps the old note.
5. Keys already loaded in ssh-agent are left alone (they are tracked per session).
6. The imported theme applies at once (`apply_theme_live`, no restart); the language setting applies after saving and restarting, like a language change.

Log lines: `backup: import ok tabs= snapshots= secrets= attachments= prompts=`, `backup: import refused reason=`, `backup: import cancelled reason=user`.

Memory hygiene: the export copy is wiped and the JSON buffer is allocated once (a growing `String` would leave unwiped copies). On import the parsed JSON tree (`serde_json::Value`) cannot be wiped; the decoded stores take ownership of the strings and `Backup::wipe` clears the rest on the abort paths. Accepted limit.

### Tests

`cargo test features::backup`: round trips of every block and every setting, hostile content (markers, NUL, quotes, multi-line secrets, Unicode), machine state not exported, file really encrypted, wrong password, tampering, bad magic, truncation, newer format refused before the password, header downgrade refused, unknown fields ignored, defaults for absent fields, wrong types refused, JSON escaping against `serde_json`, empty password refused, oversized body refused, forged Argon2 cost refused before any derivation, wipe of the export copy, no reallocation of the JSON buffer, prompts (round trip, hostile text, absent field, wrong types, a prompt the note cannot hold) and the three frozen format-1 fixtures. `every_settings_field_has_a_decision` stops compiling when a `Settings` field is added without deciding whether it travels.

### Import from another LockNote file

File > Import from another LockNote... moves the data of an older LockNote exe into the running one, without any external tool. Replacing the exe by a release one loses the notes (they live inside the exe, after the marker), so this is the supported way to move to a new build.

Flow: file dialog (`*.exe`) -> refuse the running file itself -> `storage::read_data` on the chosen file (same rule as at startup, §15: its pending staging file is read when it is the latest state, the exe otherwise, in particular while the two are in an unresolved conflict; when that staging file or its companion cannot be read, nothing is imported and the message "Cannot read the file: …" names it, since it may hold notes more recent than the exe; the source is only read, never modified) -> password of THAT data -> `backup::decode_exe_payload` (Argon2 cost ceilings first, then `crypto::decrypt`, `compression::decompress`, `Settings::parse`, `payload::parse_payload`) -> the same confirmation, lock re-check and `apply_backup` as a backup import. Window geometry, active tab, caret and ssh-agent keys are not carried over. After saving, the data is protected by the password of the running file, not the imported one.

Every step is also appended to `%LOCALAPPDATA%\LockNote\import.log` (`features::import_log`: step names, counts, sizes, never a password, path or content; 64 KiB cap). The password box refuses an empty password in place instead of closing.

Log lines: `import_exe: import ok tabs= snapshots= secrets= attachments=`, `import_exe: refused reason=same_file|too_large|read|no_data|pending_unreadable|wrong_password|invalid|locked`, `import_exe: import cancelled reason=user`.

Tests: `exe_payload_migrates_every_block` (with and without hint prefix, compressed or not), `exe_payload_drops_machine_and_session_state`, `exe_payload_wrong_password_and_garbage`, `exe_payload_refuses_forged_kdf_cost`, `exe_file_layout_roundtrip`, `exe_payload_with_reserved_markers_migrates_whole` (text holding every reserved marker, payload built by `payload::compose`). The payload is built with the same functions as `on_save`, so a change to the save pipeline that this import does not follow fails here.

### Automatic safety backups

Module: `src/features/auto_backup/mod.rs`; glue in `src/ui/editor/backup_ops.rs` (`write_auto_backup`, `build_current_backup`, `offer_purge_auto_backups`).

Importing (a `.lnbak` or another exe) replaces the whole note in memory, and the Save that follows overwrites the payload inside the exe. An update swaps that exe. Both are preceded by a backup that needs no dialog.

**When.**

| Action | Kind | Where in the flow |
|---|---|---|
| Import full backup / Import from another LockNote | `pre-import` | after the confirmation and the lock re-check, before `apply_backup` |
| Help > Check for updates, after "Yes" | `pre-update` | before `download_and_update` |
| File > Safety backups... > Create backup now | `manual` | immediately |

**What.** The file is the one "Export full backup" writes (`write_backup_file`: encrypted with the **current** note password, temporary file, read back and decoded, counts compared, then renamed). The current editor text is synced first (`build_current_backup`, shared with the export). It restores through the normal "Import full backup".

**Where.** `%LOCALAPPDATA%\LockNote\auto-backups\` (OS temp dir when the variable is missing). Names: `pre-import-<unix>.lnbak`, `pre-update-<unix>.lnbak`, `manual-<unix>.lnbak`. Two backups in the same second get consecutive stamps; a name is never reused. The name is the whole contract of what may be purged: any other file in the directory is never touched.

**Fail-fast.** If the backup cannot be written or verified, the action does not happen: the import is refused (`reason=auto_backup`), the update is not downloaded. There is no "continue without backup". A locked note is refused too.

**Retention** (constants in `auto_backup`, not settings: a setting would add a `Settings` field and so a backup field):

1. The `KEEP_NEWEST` = 3 newest files **of each automatic kind** (`pre-import`, `pre-update`) are never purged. Kinds are counted apart, so a burst of imports cannot push out the last pre-update backup. `manual` backups are never purged by the policy: the user made them on purpose and deletes them from the window. They still count in the total size.
2. Beyond those, a file older than `MAX_AGE_SECS` = 30 days is purged. A file dated in the future counts as age 0.
3. Beyond those, the oldest are purged, across both kinds, until the total is at most `MAX_TOTAL_BYTES` = 1 GiB. The protected files (the floor and every `manual` file) are never purged for size: the cap can stay exceeded, and the log says so (`over_cap=true`).
4. A `<name>.<pid>.tmp` left by a crashed write is removed once older than `STALE_TMP_SECS` = 1 hour (a younger one may be a write in progress).

The purge runs **only after** a new backup was written and verified, never at startup or in the background. A write that fails costs no older backup. A purge that cannot delete a file, or cannot list the directory, does not undo the backup: it is counted in the report and logged `WARN`.

**Password change.** The backups stay encrypted with the password of their time, so they still open with the previous one. After a password change (Security dialog), if any exist, a confirmation offers to delete them all (`purge_all`). Answering No keeps them (logged `reason=user`). Files that cannot be deleted are listed to the user with the directory.

**Window (File > Safety backups...).** `src/ui/dialogs/backups_dialog.rs`. Lists the files newest first (date in UTC, kind, size), read from the folder: no password and no decryption to list. Details on the right. Buttons:

- **Restore**: the normal "Import full backup" flow (`import_backup_file`): size cap, header check, the password the backup had (it may be a previous one), decode, confirmation with the counts, lock re-check, a new `pre-import` safety backup of the current note, then `apply_backup`. Nothing is written until the usual save.
- **Create backup now**: `write_auto_backup(Kind::Manual)`. Same fail-fast rules; the result is reported.
- **Delete**: confirmation, then `auto_backup::delete`, which refuses any name that is not an automatic backup name (no path separators, no other extension). Logged `auto_backup: deleted 1 by user`.
- **Open folder**: opens the directory in Explorer (created if missing).

A restore replaces the whole note. Restoring one tab only is not offered.

**Snapshots.** The per-tab snapshots (created at each Ctrl+S or by hand, 20 kept) were removed: File > Snapshots and the history window are gone, Ctrl+S only saves. `features::snapshots` keeps the reader: a file written by an older LockNote still holds the `[LOCKNOTE_HISTORY]` block, which `payload::parse_payload` strips so it is not glued into the last tab. The data is dropped at the next save. The backup format keeps the `snapshots` field (always written empty) to stay inside the compatibility contract.

Log lines in `import.log` (no path, no content): `auto_backup: pre_import|pre_update|manual ok bytes= purged= purge_failed= over_cap=`, `auto_backup: <kind> failed stage=locked|dir|encode|write|verify|rename`, `WARN auto_backup: ...`, `auto_backup: purge_all removed= failed=`, `auto_backup: kept <n> after password change reason=user`.

Tests: `cargo test features::auto_backup`: exact name matching (foreign names, suffixes, case, path separators, overflowing stamps), the floor, the 30-day limit at its boundary, kinds counted apart, the size cap (oldest first, stops as soon as it fits, never touches protected files, spans both kinds, reports `over_cap`), future-dated files, a directory with foreign files, stale versus fresh `.tmp`, `purge_all`, `store` (name, purge after success, **no purge when the write fails**, no name reuse, unusable directory) and a stored file restored through `backup::decode`, `manual` files never purged but counted in the size, `list` (newest first, foreign files ignored), `delete` (refuses foreign and path-like names) and `format_size`. The editor glue (refusal of the import or update when the backup fails, the password-change prompt) is not unit-tested: it lives in native-windows-gui handlers.

---

## 11. UI Specification

### 11.1 Main Window (EditorForm)

**Layout** (top to bottom):
1. **MenuStrip** — File, View, Tab, Edit, Tools, Help, 📎 (attachments toggle)
2. **TOTP Toolbar** (optional) — lock icon, dropdown, progress bar
3. **TabBar** — horizontal tab strip with + and Vault buttons
   - Look (`ui/editor/tab_paint.rs`, geometry in `ui/tab_layout.rs`): 36 px strip in `palette.chrome`, double-buffered; tabs sit on the bottom edge with a 4 px gap and 18 px horizontal padding. The active tab uses `palette.editor_background` (joins the editor), 8 px rounded top corners, 2 px `palette.accent` line on top, `text_primary` text; inactive tabs are transparent with `text_secondary` text (no hover state). A tab with a color shows an 8 px dot of that color before its name. Click hit-testing of the close box shares the painting geometry (`tab_layout::close_hit`).
   - Buttons (`ui/editor/toolbar.rs`): Lock, Attach, TOTP, Vault are owner-drawn rounded buttons (radius 6) in the slots of `tab_layout::toolbar_slots`. Idle = `chrome` fill, `chrome_border` edge, `text_secondary`; hot (pointer over, tracked with `TME_LEAVE`) = `surface_light`, `text_primary`; pressed = `border`; open panel = `accent` (`accent_hover` when hot) with the most readable of `text_primary`/`chrome`/white/black; keyboard focus = 1 px inner ring (`accent`, or the label colour on an open panel), drawn only when the focus came from the keyboard: a click marks the button as mouse-focused (`WM_LBUTTONDOWN` in the hover subclass), a navigation or activation key on the button (Tab, Enter, Space, arrows, without Ctrl or Alt: `toolbar::key_shows_focus`) or the loss of focus clears the mark (`toolbar::is_mouse_focused`); modifiers and shortcuts such as Ctrl+S keep it, so a clicked button never shows the ring even after the keyboard has been used elsewhere (Windows then stops sending `ODS_NOFOCUSRECT`). The count badge inverts the button (accent badge, or the label colour when the accent is under 3:1 against the button). The padlock glyph is 10 x 14 px, centred (`toolbar::lock_glyph`). Each button has a tooltip (`menu.toggle.*` strings), dark in the Dark theme. Labels and glyphs >= 4.5:1 and badges >= 3:1 in every state, both palettes (pure tests)
   - Pop-up menus (editor context menu, tab context menu, "..." overflow menu, menu bar drop-downs) are native menus; `dark_chrome::apply_popup_menu_theme` (SetPreferredAppMode ForceDark in Dark, Default in Light) themes all of them
4. **SearchBar** (hidden by default) — find/replace panel
5. **LineNumberTextBox** — main editor with gutter
6. **VaultPanel** (open at launch, closable) — secrets management, right side
7. **AttachmentsPanel** (hidden by default) — file attachments, right side (left of Vault)
   - **Side panel width** (Vault / TOTP / Attach / Prompts): one panel is open at a time. A 5 px grip (`chrome` colour, `IDC_SIZEWE` cursor, `ui/editor/panel_grip.rs`) sits between the editor and the open panel; dragging it with the left button resizes the panel live, and the width is written to the panel's setting (`vault_panel_width`, `totp_panel_width`, `attach_panel_width`, `prompts_panel_width`) in memory on release, only when the width shown changed (a click, or a window too narrow to let the panel move, keeps the setting). Persistence: the width is kept for the session, lock/unlock and imports included (`side_panel::keep_machine_panel_widths`, used by both unlock paths and `apply_backup`); it reaches the disk with the next save of the note (Ctrl+S, auto-save, save on close). Resizing alone does not mark the note modified, and closing without saving (Ask → No, or `save_on_close=never`) does not write it: the next launch shows the last saved width. Defaults 300 / 280 / 280 / 320 px. Bounds (`ui/side_panel.rs`, pure tests): at least 220 px, and the editor always keeps 200 px (gutter and grip not counted), so a small window shrinks the panel below 220 px, down to 0, and it regrows with the window; the stored value stays in 220–8000. The tree, lists, reveal field and status labels follow the width (20 px of margins); buttons stay anchored on the left. A click without a move stores nothing. A drag is ignored while the note is locked, dropped if the note locks during it, the mouse capture is lost (Alt+Tab, a dialog) or another panel is opened meanwhile (shortcut): the panel goes back to its stored width. The grip is hidden while locked. Widths are machine state: not in a full backup, not taken from an imported one
8. **StatusStrip** — word count, character count, line count

**Window Title**: `"LockNote"` or `"LockNote *"` (if modified)

**System Tray**:
- Icon in notification area when minimized (if `minimize_to_tray` enabled)
- Double-click restores window
- Context menu: Restore, Quit

### 11.2 LineNumberTextBox (Editor Control)

**Components**:
- Left gutter panel (BufferedPanel) with right-aligned line numbers
- RichTextBox as main editor

**Gutter**:
- Width: `gutter_width` = `max(40, digit_count * 10 + padding + 4)` pixels (`content.rs`, shared by `layout_controls` and `paint_gutter`)
- Shown or hidden (issue #26): View > Line Numbers (checkable, `menu.view.linenumbers`) toggles `show_line_numbers`; `shown_gutter_width` is 0 when hidden, `layout_controls` hides the gutter window (`SWP_HIDEWINDOW`) and `editor_columns` gives its width to the editor; `paint_gutter` draws nothing. Like word wrap, the toggle does not mark the note modified: the next save writes it (the final save at close included). The menu checks (word wrap and line numbers, `sync_view_menu`, tested through `view_menu_checks`) follow a backup import and an unlock, both of which replace the settings
- Padding: 14px right; numbers right-aligned in `gutter_text`, the caret line's number in `text_primary`; no separator line
- The caret line is refreshed on the 200 ms highlight timer tick (no selection-change hook), plus every normal repaint
- Redraws on scroll, resize, text change
- The RichEdit has a 20px left inner margin (`EM_SETMARGINS`, `EDITOR_LEFT_MARGIN`)
- **Line spacing** (`surface.rs`): every paragraph has `PARAFORMAT2.dyLineSpacing` = 30 (1.5 lines) with `bLineSpacingRule` = 5 (multiple of a line, in 20ths). `line_pitch(natural)` = `(natural * 30 + 10) / 20` is the distance between two line tops (24 px font line -> 36 px). With rule 5 RichEdit adds the extra space below the glyphs, so a glyph keeps its offset from the `EM_POSFROMCHAR` line top and the gutter, which draws each number at that top, stays aligned; rules 3 and 4 push the glyphs down and are not used. The gutter's fallback position for a line RichEdit cannot place and its row height use the same `line_pitch`. `WM_SETTEXT` resets the paragraph format, so the editor subclass re-applies it after every text replacement; applying it restores the selection, the modified flag, the undo state and masks `EN_CHANGE` (presentation only: the saved text is unchanged, nothing is written to the document)
- Proven by live RichEdit tests (`surface.rs`, `live_*`): pitch for 18/24/33/41 px fonts, glyph rows identical with and without spacing, gutter glyph rows equal the editor's rows line by line
- **Current-line band**: while the selection is empty, the caret line (its whole spaced height) is tinted. RichEdit paints its own background, so after each `WM_PAINT` the band is alpha-blended (`CURRENT_LINE_ALPHA` = 15/255, about 6 %) in `editor_text` colour over the update rectangle only, with the caret hidden meanwhile. Text pixels already have that colour and do not change; the background moves 6 % towards it; occurrence highlights are tinted by the same 6 %. No selection: no band, so selection colours are untouched. The band is not a character or paragraph attribute: it does not change the text or the saved data, and it follows the caret without an `EN_CHANGE`. The gutter paints the same band behind the caret line's number (`surface::current_line_rect` serves both)
- Contrast (pure tests, both palettes): editor text and gutter text >= 4.5:1 on their background, editor text and the caret-line number >= 4.5:1 on the band, a highlighted match under the band stays >= 4.5:1 with the text
- **Scrollbars**: `SetWindowTheme(DarkMode_Explorer)` on the RichEdit in the Dark theme, `SetWindowTheme(NULL, NULL)` (the control's default theme) in Light; a failure is reported once on stderr (`WARN editor_surface: ...`) and the previous bars stay
- **Colours**: `EM_SETBKGNDCOLOR` = `editor_background` and `EM_SETCHARFORMAT(SCF_DEFAULT, CFM_COLOR)` = `editor_text` (no `CFE_AUTOCOLOR`), at build and on a live switch; the text, modified flag, undo stack and selection are untouched. A refusal logs `WARN theme: ...`. The selection colour is the system's (RichEdit has no per-theme selection colour)

**Status bar** (`content.rs`):
- 28px high (`STATUS_BAR_HEIGHT`), positioned by `layout_controls` (the control has CCS_NORESIZE | CCS_NOPARENTALIGN); fully custom-painted by a subclass handler (`bind_status_bar_paint`): `chrome` background, 1px `chrome_border` top line, `text_secondary` text; the "saved" dot is `theme::status_saved_color` (green tab colour, darkened to 3:1 on the chrome)
- Left: words / characters / lines (the part texts set by `update_status_bar`). Right, in order: save indicator (7px dot, green = saved, `accent` = modified, text `status.saved` / `status.modified`), padlock in `accent` + `status.encrypted`, then the detected encoding (Pro)
- Middle (issue 057): the full path of the LockNote file of the session (`EditorForm::exe_path`), from one gap after the counters to one gap before the save dot (`PathSlot`); `DT_PATH_ELLIPSIS` shortens it in the middle, keeping the file name, when it does not fit; not drawn when that room is under `STATUS_PATH_MIN_WIDTH` (60 px). Nothing else moves for it. The slot is always sized on the path, also while the click feedback is shown in it. Hand cursor over it (client area only); a left click (button pressed and released on it, mouse captured meanwhile; a drag that only ends there does nothing) copies the full path to the clipboard (Unicode text; the clipboard guard is not armed: a path is not a secret) and shows `status.path_copied` (or `status.path_copy_failed`, with `WARN status_bar: clipboard ...` on stderr) in its place for 2 s (timer on the status bar window)
- Pure layout in `status_layout`, covered by unit tests (`status_path_*` for the path)

**Double-click**:
- Selects the whole whitespace-delimited word (`char::is_whitespace`; U+00A0 splits, U+200B does not; CJK without spaces is one word)
- Nothing is stripped: the selection is exactly the text between whitespace (`Pass@word1!`, `(mot),`, `fin.`, `user@x.com.` are selected as is). The editor mostly holds secrets, and any punctuation rule would truncate some password
- Nothing is copied to the clipboard (copy is an explicit Ctrl+C). There is no "slow double-click" fallback: only the system double-click selects a word

**Occurrence Highlighting**:
- Triggers on selection change with 200ms debounce
- Requirements: selection ≥ 2 bytes and ≤ 256 characters, no newlines, no leading/trailing whitespace (the search is O(n·m))
- Known issue: re-applying the selection loses its direction (issue 051)
- Highlights all occurrences (case-insensitive) except the selection itself
- Highlight color: `Theme.MatchHighlight`
- Clears previous highlights before applying new ones
- Caches `lastHighlightedWord` to skip redundant work

**Context Menu**: Cut, Copy, Paste, Paste plain text, Select all

**Drag & Drop**:
- File drop: reads file as text, inserts at cursor (up to 1.2.x the Public edition refused it)
- Text drop: inserts directly

**URL Handling**: clicks on detected links open in default browser.

### 11.3 SearchBar

**Modes**:
- **Find only** (Ctrl+F): height=32px, search field + Find Next button
- **Find & Replace** (Ctrl+H): height=62px, adds replace field + Replace/Replace All buttons

**Find Next**:
- Case-insensitive `IndexOf` from `lastIndex + 1`
- Wraps to beginning when reaching end
- Selects and scrolls to match
- Shows "Text not found" if no match

**Replace**:
- If current selection matches search term (case-insensitive): replaces selection
- Adjusts `lastIndex` by length difference
- Then calls Find Next

**Replace All**:
- Builds new string with all replacements (case-insensitive)
- Shows count: "N replacement(s) made"
- Resets `lastIndex`

**Keyboard**: Enter = Find Next / Replace, Escape = hide and refocus editor

### 11.4 Dialogs

#### CreatePasswordDialog

- **Fields**: password input (masked `*`), strength bar, strength label, confirm input
- **Strength scoring** (0–7):
  - Length ≥ 8: +1, ≥ 12: +1, ≥ 16: +1
  - Has lowercase: +1, uppercase: +1, digit: +1, symbol: +1
- **Strength display**:
  - 0–1: "Weak" (red, 25%)
  - 2–3: "Fair" (orange, 50%)
  - 4–5: "Strong" (green, 75%)
  - 6–7: "Very strong" (bright green, 100%)
- **Validation**: meets minimum length, matches confirmation
- **Security**: TextBox contents cleared on Dispose

#### UnlockDialog

- **Fields**: password input (masked `*`), attempts label, Unlock button
- **Max attempts**: 5
- **Feedback**: "Wrong password (N/5)" after each failure
- **Max reached**: dialog "Maximum number of attempts reached", then close
- **Async decryption**: BackgroundWorker to avoid UI freeze during PBKDF2
- **Status**: "Decrypting..." while working, input disabled
- **Security**: TextBox cleared on Dispose
- **Look (Rust port, shared with the lock screen, `ui/lock_card.rs`)**: one card: padlock (drawn with GDI, no icon font), title, prompt (`unlock.desc`), a rounded password field (masked with `●`) holding a show/hide eye button, one status line, then the buttons: Cancel (secondary: card fill, 3:1 border) left of Unlock (primary: theme accent). Palette colours come from `ThemePalette` in both themes and are corrected by `ensure_contrast` where the palette alone is too weak: every text >= 4.5:1 on its background (primary button text is near-black on the light dark-theme accent and white on the light-theme accent, so it never falls to the former ~2.7:1), field border, focus ring, padlock and secondary border >= 3:1. Pressed state uses `accent_hover` / `surface_light`; no hover state
- **Sizing**: every length is a 96-DPI value scaled by `GetDpiForWindow` (`scale(px, dpi)`, rounded, 1 px hairlines stay >= 1 px); fonts (Segoe UI: 30 title, 17 prompt/buttons, 22 field, 16 status, cell heights at 96 DPI) are rebuilt for the DPI. Client size 440 x ~340 at 100 %. The window is created hidden, measured, then centred on the parent (or the work area of the primary monitor when there is no usable parent, e.g. minimised) and clamped into that monitor's work area
- **Status line**: one label, three tones (neutral, error, warning), colour by tone. An explicit message (`Wrong password (N/5)`, `Retry in Ns...`, crypto error, `Decrypting...`) wins over the hint; with no message, `Caps Lock is on` (`unlock.capslock`) is shown while Caps Lock is on **and** the field has the focus (`caps_hint_visible`). Refreshed on field focus/blur and on the Caps Lock key
- **Eye button**: toggles the edit's password character; accessible name `unlock.show` / `unlock.hide` (the action a click will do). The text is hidden again whenever the lock screen is shown or the session is unlocked. Nothing in this module reads or logs the password
- **Keyboard**: Enter = Unlock, Esc = Cancel, Tab order field -> eye -> Unlock -> Cancel. The caret is in the field as soon as the window has the focus: `lock_card::keep_focus_on` re-focuses the field on WM_ACTIVATE and again on the WM_SETFOCUS that `DefWindowProc` sends right after; it is bound by this popup and by the create-password, confirm-password, 2FA code and backup-password dialogs
- **Tests**: `cargo test lock_card` (scaling, geometry at 96..288 DPI, popup placement/clamping, WCAG contrast of every text/graphic pair in both palettes, status-line decision) and `features::i18n::tests::unlock_card_keys_exist_in_every_language`

#### SettingsDialog

**Base settings**:
- On close behavior (Ask/Always/Never)
- Theme (Dark/Light)
- Min password length (1, 4, 6, 8, 12, 16)
- Minimize to tray (checkbox)
- Font family (Consolas, Courier New, Lucida Console, Cascadia Mono, Segoe UI)
- Font size (8, 9, 10, 11, 12, 14, 16, 18, 20, 24)

**Also** (~380×720px; up to 1.2.x, the Pro edition only):
- Language (English, Français, Español, Deutsch)
- Auto-lock (Disabled, 5, 10, 15, 30 min)
- Clipboard auto-clear (Disabled, 10, 30, 60 sec)
- Auto-save interval (Disabled, 15, 30, 60, 120 sec)
- Show line numbers (checkbox)
- Global hotkey (read-only field + Clear button, captures key combos)
- 2FA section: enable checkbox, Setup button, status label

#### TempFilesDialog (File > Settings > Temporary files...)

`src/ui/dialogs/temp_files_dialog.rs`, issue 057. What the staging directory of §15 is, how to reach it, and what can be done with it:

- **Explanation** (`tempfiles.dlg.intro`): why saves go to a temporary file first, that it is the only copy of the latest save until LockNote closes, what else the folder holds (`{hash}.base`, `conflicts\`, `swap-error.log`, `import.log`, `auto-backups\`), and how to reach it (Open folder, or `%LOCALAPPDATA%\LockNote` in the Explorer address bar). The folder path is shown.
- **Open folder** (Explorer; a folder that does not exist yet is not created, a message says nothing was written there) and **Copy path** (the folder path to the clipboard).
- **List** of the pending `.tmp` files, classified by `storage::inventory::list_staging_in` (date UTC, size, status; details on the right: `.tmp` path, LockNote file named by its companion, what the status means). Statuses (`inventory::Owner`): `Current` (the open file), `ExePresent`, `Orphan` (the companion names a LockNote file that does not exist, in a folder that does, **on the volume the companion recorded**), `FolderMissing` (neither the file nor its folder: drive or share disconnected, folder removed), `OtherVolume` (the folder exists but on another volume: the drive letter or share now names another disk, e.g. a USB key replaced by another), `Unreachable` (existence or volume cannot be checked, or the companion recorded no volume), `NoCompanion` (left by ≤ 1.2.23), `CompanionUnusable` (unreadable, malformed, or names a file that cannot be trusted: a hash other than the one of the `.tmp` name, a relative path such as `C:x.exe`, a name holding U+FFFD because Windows allowed what UTF-8 cannot hold), `TmpUnreadable` (the `.tmp` itself cannot be examined). Only `{16 lowercase hex}.tmp` regular files are listed; a file gone while the folder is read is skipped, an error on one file only concerns that file, an error reading the folder is shown, a missing folder is an empty list.
- **Reading off the UI thread**: the list, the deletion and the decision run on a worker thread (`nwg::Notice` brings the result back); the dialog shows "Reading..." meanwhile. Measured by the refuter: `metadata` of an exe on a server that does not answer takes about 21 s the first time. The editor window is disabled while the dialog is open (modal), so File > Quit or a second dialog cannot run under it; the tray menu still reaches the editor, so `EditorForm::on_close` refuses (logged `INFO close: refused while a modal dialog is open`) while a modal is open (`modal_open`, issue #30). The dialog does not close (Close button, title bar) while a reading or a deletion is in progress, so the outcome of a deletion is always shown.
- Volume serial number 0 (some virtual file systems give it to every volume) counts as unknown, at writing and at reading: such a `.tmp` is never an orphan. Known limit: two volumes with the same non-zero serial number (disks cloned from one image) would pass for one; so would, with Windows Offline Files, a cached folder whose exe is not cached.
- **Delete orphan...** (enabled on an `Orphan` only): Yes/No confirmation naming the `.tmp` and the missing file, then `inventory::delete_orphan_in`: refused for the open file (compared by hash, whatever thread runs the deletion: the swap lock would refuse it from another thread, but a Win32 mutex can be taken again by the thread that owns it), refused when the swap lock `Local\LockNote_Swap_{hash}` of that file cannot be taken at once (`SwapLock::try_acquire`: a session or a helper of it is at work; a lock left abandoned is logged and the deletion goes on), refused when the file is no longer an orphan once the lock is held; then the `.tmp`, then its companion (§15 order). Logged in `swap-error.log`: `orphan staging file deleted by the user: tmp=... exe=...` or `WARN orphan staging file not deleted: <reason> tmp=...`. The list is read again afterwards.
- **This LockNote file** (`storage::pending_decision_in` on the open file): `Promote` says the size and that the save goes into the file at exit, and enables **Close LockNote now** (the dialog closes, then the normal close flow runs: save question per the preferences, Cancel keeps LockNote open); `None` / `Stale` say nothing is waiting; `Conflict`, `TmpUnreadable`, `ExeUnreadable`, `Ignored` say closing will not write it and the next start decides.
- **Swap log** / **Import log**: opened with the program associated with `.log` (`ShellExecuteW`); a missing file says no event was recorded; a failure shows the error.
- Nothing else is ever deleted from the folder by this dialog.

#### CloseConfirmDialog

- Message: "Unsaved changes. Save before closing?" (`close.msg`, translated)
- Buttons: Save (DialogResult.Yes), Don't save (DialogResult.No), Cancel
- Checkbox: "Remember my choice" → sets `SaveOnClose` to Always/Never
- Rust (issue #27): `ui/dialogs/close_confirm.rs` is the prompt of `on_close` (setting Ask), shown under a `modal()` guard and owned by the main window (`show(Some(owner))`: never behind an always-on-top main window; the shared dialog pass centers it on that window and sizes labels and buttons from their measured text). `close_flow::remembered_close_action`: Save + remember → `save_on_close = always`, set before the save that writes it; Cancel never stores anything; Don't save + remember → nothing is written, not even in memory (decision of 2026-11-26), since the settings are inside the encrypted note and storing them would save the declined changes: LockNote says so and names the setting (`close_remember_notice`: `close.remember_not_saved` filled with `settings.onclose`, `settings.never` and the File > Settings > Preferences labels without `&`; shown under a `modal()` guard; logged `WARN close: remember=never not stored`)

#### GoToLineDialog

- Input: line number (pre-filled with current line)
- Validation: integer between 1 and total line count
- Label: "Line number (1 – N):"

#### RenameTabDialog

- Input: new tab name (pre-filled with current name)
- Validation: non-empty
- Buttons: OK, Cancel

#### SecretEditDialog

- Fields: Name (TextBox), Category (ComboBox), Value (masked single-line field)
- Show/Hide toggle: "Show" replaces the masked field with a multi-line text box holding the whole value (a secret can span several lines; CRLF in the control, LF stored); "Hide" masks it again
- Copy button: copies the current value (either view) to the clipboard, outside Win+V history; the clipboard guard is armed by the caller, also when the dialog is cancelled
- Generate... button: opens the PasswordGeneratorDialog (this dialog is disabled meanwhile); its "Use" button puts the password in the value, in whichever view is active
- Categories: the user's category list (issue 052; the 7 defaults General, Email, Cloud, Finance, API Keys, Passwords, Crypto until customized)
- Validation: name cannot be empty

#### PasswordGeneratorDialog

Opened from Tools > Password Generator... and from the SecretEditDialog. Logic in `src/features/passgen/mod.rs`.

- Password field: masked, Show/Hide toggle; a password is drawn when the dialog opens, on "Generate", and whenever an option below changes
- Copy: copies the password, outside Win+V history; the caller arms the clipboard guard (auto-clear)
- Length: 8–64, default 16
- Character types (check boxes, all on by default): Uppercase (A-Z), Lowercase (a-z), Digits (0-9), Symbols
- Allowed symbols: editable list, ASCII punctuation only, duplicates dropped; default ``!@#$%^&*()-_=+[]{};:,.?``
- Exclude look-alike characters (off by default): removes `0 O o 1 l I |` from every type
- Guarantee: the password always holds at least one character of every ticked type; every position is drawn uniformly with the process CSPRNG (`rand::rng()`), then the whole is shuffled
- Impossible options (length out of range, no type ticked, a non-symbol in the list, no symbol left): the field is emptied and the reason is shown; nothing is generated
- Use (only when opened from the SecretEditDialog): closes and hands the password back
- The options last used successfully are kept in the settings (`passgen_*`), so they are saved with the note and travel in a full backup; changing them marks the note modified

#### TotpSetupDialog

- See [Section 9 — TOTP Setup Dialog](#totp-setup-dialog)

#### AboutDialog

- Shield icon, product name, version
- Description: "Self-contained encrypted notepad for Windows"
- Crypto info (Rust, issue #28): `about::crypto_summary()`, built from `crypto::DEFAULT_M_COST` / `DEFAULT_T_COST` / `DEFAULT_P_LANES`, e.g. "AES-256-CBC + HMAC-SHA256 | Argon2id m=64 MiB, t=3, p=4"; Help > About LockNote... (`on_about`, a message box) shows the same line. The C# dialog showed "PBKDF2 100k iterations"
- GitHub link (clickable, opens browser)

### 11.5 TabBar

- Height: 36px (`TAB_STRIP_H`), docked at top (below menu/TOTP toolbar)
- Tab width: the measured width of the full name plus padding (no minimum, no maximum)
- Close button: 16×16px, drawn on every tab when there is more than one
- Add button (+): 28px wide, right after the last drawn tab
- Toolbar buttons (Lock, Attachments, TOTP, Vault, Prompts) sit right-aligned, see below
- Modified indicator: bullet (•) in accent color appended to tab name
- Color tinting: tab color blended into editor background at 5–25% opacity
- Accent line: 2px on top of the active tab (see **Active tab** below)
- Context menu (right-click): Rename, Color submenu (8 colors + None), Close
- **Active tab**: editor background (joins the editor), bold name, 2 px accent line on top
- **Drag & drop**: press on a tab and release on another drawn tab to move it to that position (`TabBar::move_tab`, `tab_layout::drag_target`). Below 4 px of travel it is a plain click. The mouse is captured during the press (resize-arrow cursor); while over a valid target the dragged tab is framed in the accent colour and a 4 px insertion mark is drawn on the landing edge of the target (`tab_layout::drop_edge_x`: right edge when moving right, left edge when moving left); releasing on "…", "+", outside the tabs or on the same tab changes nothing. The moved tab stays active; the document is marked modified (the order is saved in the payload)
- Toolbar buttons (right to left): Lock 32px (step 36), Attachments / TOTP / Vault 50px (step 55), Prompts 60px (step 65, longest label). Tabs, "…" and "+" stop 4px left of the left-most button (`src/ui/tab_layout.rs`, single source for layout_controls, painting and hit-tests)

**Overflow** (`compute_tab_layout`):
- Names are never cut to make room: a tab is as wide as its full name (measured with the strip font, bold; plus padding, color dot and close box)
- When the tabs + "+" do not fit, a drop-down button (32px, triangle) appears at the right end of the strip, 8px left of the toolbar side; "+" stays right after the last drawn tab. Clicking the button lists every tab (active one checked) and switches to the chosen tab
- No drawn tab crosses the room left for "…" and "+"; tabs are never drawn under the toolbar
- The active tab is always drawn: when it would be hidden it takes the last visible position; when it is wider than the room it is narrowed and its name ends with an ellipsis
- A single tab wider than the room is narrowed without a "…" button
- With less than 36px of room no tab is drawn, only "…" and "+"
- Right click resolves only drawn tabs (never a hidden one behind "…")
- Only a tab wider than the room left for it (alone in the strip, or the active tab next to the drop-down) is narrowed, and its name then ends with an ellipsis. Measured widths get 2px of slack so a name that fits is never clipped. Below about 68px of room the "+" and drop-down buttons stay inside the room but may overlap

### 11.6 Theme System

**Two modes**: Dark, Light

**Dark Palette**:

| Token | RGB |
|-------|-----|
| Background | (30, 30, 30) |
| Surface | (37, 37, 38) |
| SurfaceLight | (45, 45, 48) |
| Border | (60, 60, 60) |
| TextPrimary | (220, 220, 220) |
| TextSecondary | (150, 150, 150) |
| TextMuted | (140, 140, 140) |
| Accent | (86, 156, 214) |
| AccentHover | (120, 180, 230) |
| EditorBackground | (30, 30, 30) |
| EditorText | (220, 220, 220) |
| GutterBackground | (30, 30, 30) |
| GutterText | (140, 140, 140) |
| StatusBackground | (0, 122, 204) |
| StatusText | (255, 255, 255) |
| MenuBackground | (37, 37, 38) |
| MenuText | (220, 220, 220) |
| MenuHover | (62, 62, 64) |
| InputBackground | (45, 45, 48) |
| InputBorder | (60, 60, 60) |
| ButtonBackground | (60, 60, 60) |
| ButtonText | (220, 220, 220) |
| ButtonSecondary | (80, 80, 80) |
| ErrorText | (244, 71, 71) |
| MatchHighlight | (80, 80, 0) |

**Light Palette**:

| Token | RGB |
|-------|-----|
| Background | (247, 250, 255) |
| Surface | (238, 244, 253) |
| SurfaceLight | (222, 234, 251) |
| Border | (160, 184, 224) |
| Chrome (title bar, menu bar, tab strip, status bar, lock-screen backdrop) | (219, 232, 252) |
| ChromeBorder (1 px lines under the bar / around the strip) | (170, 196, 238) |
| ChromeHover (menu bar item pill) | (190, 212, 246) |
| TextPrimary | (20, 32, 56) |
| TextSecondary | (64, 80, 110) |
| TextMuted | (88, 100, 124) |
| Accent | (0, 102, 204) |
| AccentHover | (0, 78, 160) |
| EditorBackground | (255, 255, 255) |
| EditorText | (30, 30, 30) |
| GutterBackground | (240, 245, 253) |
| GutterText | (84, 96, 120) |
| StatusBackground | (0, 102, 204) |
| StatusText | (255, 255, 255) |
| MenuBackground | (219, 232, 252) |
| MenuText | (20, 32, 56) |
| MenuHover | (190, 212, 246) |
| InputBackground | (255, 255, 255) |
| InputBorder | (120, 142, 184) |
| ButtonBackground | (214, 228, 250) |
| ButtonText | (20, 32, 56) |
| ButtonSecondary | (190, 210, 240) |
| ErrorText | (178, 30, 30) |
| MatchHighlight | (255, 255, 0) |

The Light theme is coloured, not flat grey: the chrome is a soft tint of the accent blue with dark navy text, the active tab and the editor stay white (the active tab keeps its 2 px accent line), the status bar and the lock-screen backdrop use the chrome tint, the "saved" dot is the green tab colour darkened to 3:1 on the chrome (`theme::status_saved_color`). Every text / background pair is >= 4.5:1 and every glyph / border >= 3:1 in both palettes (tests in `theme`, `dark_chrome`, `toolbar`, `surface`, `lock_card`, `dialogs/metrics`). The Dark palette is the authoritative `src/theme/mod.rs` table (the Dark rows above predate the Rust port); it gained only `ChromeHover` = SurfaceLight (53, 57, 66), so Dark looks as before.

**Live switching**: the theme chosen in Preferences (and the theme of an imported backup) applies at once, no restart: `EditorForm::apply_theme_live` sets `theme::set_mode`, then runs `theme_switch::refresh_plan` (pure, tested): title bar, popup-menu mode, menu bar redraw, editor colours + scrollbar theme, tooltips, vault tree, a repaint of every window, then the gutter, tab strip, status bar and toolbar buttons. Text, modified flag, undo stack and selection are not touched. Brushes handed to `WM_CTLCOLOR*` are cached by colour (`lock_card::cached_brush`) and the panel handlers read the palette per message, so nothing keeps the previous theme. Only the language still needs a restart.

**Fonts**:

| Token | Family | Size | Style |
|-------|--------|------|-------|
| UIFont | Segoe UI | 9pt | Regular |
| UIFontBold | Segoe UI | 9pt | Bold |
| EditorFont | Consolas | 11pt | Regular (customizable) |
| StatusFont | Segoe UI | 8.5pt | Regular |

**Theme application**: recursive traversal of all controls, applying colors based on control type (Button, TextBox, Label, CheckBox, ComboBox). Custom renderers for MenuStrip and StatusStrip.

#### Dark window chrome (`src/ui/dark_chrome.rs`)

The native title bar, menu bar and popup menus follow the theme in both modes, with the palette (`chrome`, `chrome_border`, `chrome_hover`, `menu_text`). `theme::set_mode` is called at startup (`ui::run`) and on every live switch (see "Live switching" above); the attributes are applied when the main window and each dialog are built and re-applied on a switch.

| Part | Mechanism |
|------|-----------|
| Title bar (main window + every dialog via `apply_dialog_theme`) | `DwmSetWindowAttribute(DWMWA_USE_IMMERSIVE_DARK_MODE)`, attribute 20, then 19 (early Windows 10), value 1 Dark / 0 Light. Light also sets, on Windows 11 (build 22000+), `DWMWA_CAPTION_COLOR` (35) = `chrome`, `DWMWA_TEXT_COLOR` (36) = `text_primary` (>= 4.5:1 on the caption, tested), `DWMWA_BORDER_COLOR` (34) = `chrome_border`; going back to Dark resets the three to `DWMWA_COLOR_DEFAULT` (0xFFFFFFFF), only if they were set. On Windows 10 the three attributes are refused: one `WARN dark_chrome: title bar colour attribute ...` line, the system caption stays. Needs the `Win32_Graphics_Dwm` feature of the `windows` crate. |
| Menu bar background and items | Undocumented `WM_UAHDRAWMENU` (0x91) and `WM_UAHDRAWMENUITEM` (0x92) handled in the main window's raw handler (`UAHMENU` / `UAHDRAWMENUITEM` layouts checked by unit tests). Both themes: bar `chrome`, label `menu_text` (`text_muted` when disabled); hot or selected item: rounded `chrome_hover` pill (= `surface_light` in Dark). |
| 1 px line under the bar | Repainted with `chrome_border` after `WM_NCPAINT` / `WM_NCACTIVATE` (default processing runs first), both themes. |
| Popup menus | `uxtheme.dll` ordinal 135 (`SetPreferredAppMode`: ForceDark = 2 in Dark so the app theme does not depend on the OS mode, Default = 0 in Light) then ordinal 136 (`FlushMenuThemes`), resolved with `GetProcAddress`. |
| Redraw on a live switch | `DrawMenuBar` then `RedrawWindow(RDW_FRAME, RDW_INVALIDATE, RDW_UPDATENOW)` on the main window. |

**Degradation**: if an API is unavailable (older Windows, missing ordinal) the native look is kept and one line `WARN dark_chrome: ...` is written to stderr. No silent fallback: errors are explicit (`DarkChromeError`).

#### Dialog look (`src/ui/dialogs/metrics.rs`, `style.rs`)

Every dialog that calls `apply_dialog_theme` gets the same look from `apply_dialog_theme`, called right after the window is built. It registers the window and posts a message; a single pass runs from the first message of the dialog's loop (before the first paint, when every control exists). Pure decisions (scaling, fonts, reflow, colors) are in `metrics.rs` and unit-tested; `style.rs` measures real controls and applies them.

| Aspect | Rule |
|--------|------|
| DPI | Dialogs are authored in 96-DPI pixels. The pass scales window and controls to the monitor DPI (`GetDpiForWindow`, per-monitor aware; edges are scaled, not sizes, so neighbours stay adjacent), 1 px separators stay at least 1 px. |
| Fonts | One Segoe UI family, role chosen from the authored font: Title 13 pt semibold (authored height >= 22), Section 10 pt semibold (bold), Body 9 pt (16), Caption 8 pt in `text_secondary` (<= 15). Other faces (Consolas) only scale. |
| Spacing | The dialog's own left margin is the right margin of the footer; gap between buttons 8 dp; buttons 30 dp high, at least 88 dp wide, caption + 2 x 16 dp. |
| Footer | The bottom row of push buttons, when no input shares the row, becomes one right-aligned row of equal-width buttons; the window widens if it does not fit. Other buttons only grow to fit their caption (the input on their left gives up the space). |
| Long translations | Labels and check boxes whose caption does not fit first take the free width of their row, then wrap; controls below move down and the window grows. Measured with the real font (`DrawText`). Dynamic texts set after build (error messages) are not re-measured. |
| Buttons | Default (Enter) button = the one given to `bind_enter_to_button` = primary: accent fill, caption white or near-black whichever is more legible (>= 4.5:1, tested for normal / hover / pressed in both palettes). Dark: all buttons are owner-drawn (secondary = `button_background`, visible border, accent border on hover). Light: only the primary is drawn; the others keep the native look. |
| Esc | `WM_COMMAND(IDCANCEL)` from `IsDialogMessage` closes the dialog like the title-bar button. |
| Native controls (Dark) | Edit `DarkMode_CFD` (multi-line `DarkMode_Explorer`), combo box `DarkMode_CFD` and its list `DarkMode_Explorer`, list box `DarkMode_Explorer`; check boxes are painted from the palette (visual styles have no dark check box; focus, hover, disabled handled). Light: unchanged. |
| Colors | `WM_CTLCOLOR*`: labels `text_primary` (captions `text_secondary`, error labels `error_text` via `mark_error`), edits and lists `input_background`. Hairlines and bars are per-control frames (`set_frame_color`), not a window-class brush (all nwg frames share one class). |
| Placement | Centered on the application's main window (largest visible top-level window of the thread), kept inside its monitor's work area; on the primary monitor when there is none. |

Contrast: `metrics::contrast_pairs` lists every text / background pair drawn by the dialogs; a test checks them in both palettes (4.5:1 text, 3:1 borders and disabled text). The Dark `error_text` was lightened to (248, 92, 92) for this (4.47:1 before).

Visual check: `cargo test dialog_snapshots -- --ignored --test-threads=1` with `LOCKNOTE_DIALOG_SNAPSHOTS=<dir>` (and optionally `SNAP_LANG`, `SNAP_THEME`, `SNAP_DPI`, `SNAP_ONLY`) opens each dialog, captures only its own window with `PrintWindow` to a PNG and closes it (`src/ui/dialogs/snapshot.rs`).

---

## 12. Internationalization

### Supported Languages

| Code | Language | Enum value |
|------|----------|------------|
| en | English | English |
| fr | Français | French |
| es | Español | Spanish |
| de | Deutsch | German |

### API

```
Lang.SetLanguage(AppLanguage.French)
Lang.Get("menu.file.save")  // → "Enregistrer"
```

### Fallback Chain

1. Current language dictionary
2. English dictionary
3. `"[key]"` placeholder (key not found in any language)

### Translation Key Categories

| Category | Example Keys |
|----------|-------------|
| App | `app.title`, `app.title.modified` |
| File menu | `menu.file`, `menu.file.save`, `menu.file.changepass`, `menu.file.settings`, `menu.file.quit`, `menu.file.print`, `menu.file.export` |
| View menu | `menu.view`, `menu.view.ontop`, `menu.view.vault` |
| Tab menu | `menu.tab`, `menu.tab.new`, `menu.tab.close`, `menu.tab.rename`, `menu.tab.next`, `menu.tab.prev` |
| Edit menu | `menu.edit`, `menu.edit.cut`, `menu.edit.copy`, `menu.edit.paste`, `menu.edit.pasteplain`, `menu.edit.find`, `menu.edit.replace`, `menu.edit.goto`, `menu.edit.dupline`, `menu.edit.delline`, `menu.edit.selectall`, `menu.edit.timestamp` |
| Help menu | `menu.help`, `menu.help.update`, `menu.help.about` |
| Context menu | `ctx.cut`, `ctx.copy`, `ctx.paste`, `ctx.pasteplain`, `ctx.selectall`, `ctx.rename`, `ctx.close` |
| Colors | `ctx.color`, `color.none`, `color.blue`, `color.green`, `color.red`, `color.orange`, `color.purple`, `color.yellow`, `color.pink`, `color.gray` |
| Status bar | `status.stats` (format string with word/char/line counts), `status.saved`, `status.modified`, `status.encrypted`, `status.path_copied`, `status.path_copy_failed` |
| Temporary files | `menu.settings.tempfiles`, `tempfiles.*` (dialog, statuses, "apply now", logs, deletion); the English dictionary equals the fallbacks (`temp_files_dialog::tests::english_dictionary_matches_the_fallbacks`) |
| Create password | `createpwd.title`, `.password`, `.confirm`, `.create`, `.cancel`, `.empty`, `.tooshort`, `.mismatch`, `.weak`, `.fair`, `.strong`, `.verystrong` |
| Unlock | `unlock.title`, `.password`, `.btn`, `.cancel`, `.empty`, `.decrypting`, `.maxattempts`, `.wrong` |
| Close confirm | `close.unsaved`, `.remember`, `.save`, `.dontsave`, `.cancel` |
| Search | `search.find`, `.replace`, `.next`, `.btn.replace`, `.btn.replaceall`, `.close`, `.notfound`, `.title`, `.replaceall.title`, `.replaceall.msg` |
| Go to line | `goto.title`, `.label`, `.ok`, `.cancel`, `.invalid` |
| Rename | `rename.title`, `.label`, `.ok`, `.cancel`, `.empty` |
| Settings | `settings.title`, `.onclose`, `.ask`, `.always`, `.never`, `.theme`, `.dark`, `.light`, `.language`, `.minpwdlen`, `.font`, `.ok`, `.cancel`, `.hotkey`, `.2fa`, `.2fa.enable`, `.2fa.setup`, `.2fa.status.*` |
| Update | `update.fail`, `.norelease`, `.uptodate`, `.available`, `.success`, `.error` |
| TOTP | `totp.empty`, `.add`, `.add.title`, `.add.name`, `.add.secret`, `.add.invalid`, `.remove`, `.remove.confirm`, `.copied` |
| Export | `export.prompt`, `.btn`, `.warning`, `.dlg.title`, `.dlg.filter`, `.fail`, `.success.encoding` |
| Former hard-coded texts (issue #25) | `confirmpwd.title` / `.export` / `.print`, `about.version`, `status.word(s)` / `.char(s)` / `.line(s)` (`{n}`; singular per `helpers::is_singular`: 0 and 1 in French, 1 only in EN/ES/DE), `save.error.encrypt`, `filter.text` / `.all` / `.images` / `.image` / `.ext` (file dialog labels; the `(*.ext)` part stays, nwg reads the pattern from it), `update.available.*`, `update.downloaded.*`, `update.error.title`, `update.uptodate.*`, `update.checkfailed.*`, `vault.delete.title` / `.msg`, `attach.error.title`, `attach.limit.title`, `attach.error.toolarge` / `.totaltoolarge`, `totp.manager.copied`, `totp.setup.for`, `2fa.invalid.left`, `close.remember_not_saved` |

**No hard-coded UI text** (issue #25): `features::i18n::tests::the_ui_hands_no_hard_coded_text_to_its_controls` scans `src/ui` (test items and test-only files left out) for a string literal, or the format string of a `format!`, passed to `.text(`, `.title(`, `set_text(`, `.filters(`, `modal_*_message(`, `*input_dialog(`, or as the `title:` / `content:` of a message box, that contains letters once `{...}` placeholders, escapes and URLs are left out. `UNTRANSLATED_UI_TEXT` lists the names allowed as they are ("LockNote"). It also scans the items of `const X: &[&str] = &[...]` lists (combo boxes), except `UNTRANSLATED_UI_LISTS` (language names, fonts, sizes, file extensions); duration items come from `dialogs::seconds_options` / `minutes_options` (`option.off`, `option.seconds`, `option.minutes`). Menus built with `AppendMenuW` take their label from a variable, which the scan cannot follow: the tab colour submenu uses `tab_color_label` (`color.<stored name>`, tested), the other labels are `t("...")` literals checked by the key tests. `every_translation_keeps_the_english_placeholders` checks that every `{placeholder}` of the English text exists in FR/ES/DE. Texts built elsewhere and handed over in a variable are not seen by the scan. Still English by design: the windows shown before the note is decrypted (the language is in the encrypted settings), the default name of a new tab ("Note N", saved in the note), font names, and the `Display` of crypto / storage errors appended to translated messages.

---

## 13. Auto-Lock, Clipboard Guard, Auto-Save

### Auto-Lock

- **Setting**: `auto_lock` (minutes, 0 = disabled, default: 15)
- **Timer**: interval = `auto_lock * 60 * 1000` ms
- **Trigger**: timer tick (no user input for N minutes)
- **Action**: auto-save if dirty → capture session (active tab + caret line, in memory) → clear editor text → zeroize password and in-memory plaintext → show the inline lock overlay (the same card as the unlock popup, see UnlockDialog > Look: padlock, title `lock.overlay.title`, prompt, password field with eye button, status line with Caps Lock hint, one full-width Unlock button, no Cancel; exponential backoff after failures). The card is centred on a backdrop (`chrome` in both themes) that covers the client area, 440 px at 100 % DPI, narrowed with the window (16 px margins, 280 px minimum), re-laid out at the window's DPI on every resize. Enter unlocks; the caret is in the field when the window is activated (WM_ACTIVATE + WM_SETFOCUS)
- **While locked**: keyboard shortcuts are ignored and the hidden editor receives no keystrokes; `on_save` refuses an empty password (fail-fast)
- **Unlock**: decrypt → `parse_payload` → restore the tab and caret line captured at lock time (falls back to the `[LOCKNOTE_STATE]` block on disk, index clamped). If the save at lock time failed, the disk is stale and the in-memory state wins
- **Close while locked**: exits without saving (nothing can be encrypted without the password)
- **Reset on**: mouse move / left click on the main window, key press, text change (clicks and scrolling inside the editor do not reset it, see issue 049)
- **UI values**: Disabled, 5, 10, 15, 30 minutes

### Clipboard Guard

- **Setting**: `clipboard_clear` (seconds, 0 = disabled, default: 30)
- **Trigger**: on copy/cut from editor or vault
- **Action**: snapshot clipboard content → start timer → on tick: if clipboard still matches snapshot, clear it
- **UI values**: Disabled, 10, 30, 60 seconds
- **Vault integration**: copy from vault card also triggers the guard
- **Write results** (issue #31): no clipboard write discards its result (`ui::tests::no_clipboard_write_discards_its_result` scans `src`). The status text comes from `ui::copy_feedback`: the "Copied" text only after a successful write, `clipboard.failed` ("Copy failed: {err}") otherwise; `TotpPanel::copy_code` and `VaultPanel::copy_secret_value` return the write result, and the guard is armed only after a successful vault copy. The history exclusion logs each failed step (`WARN clipboard: history exclusion ...`). An auto-clear that cannot open or empty the clipboard logs `WARN clipboard: auto-clear failed` and stays armed, so the next tick tries again. The read before it follows `clipboard_guard::clear_step` (pass 3): our copy → clear, another content → done (guard disarmed), a failed read (another application holds the clipboard) → retry, guard kept armed, logged on the first failure then once in 60 (`warn_read_failure`). The secret copied with the Copy button of Add Key arms the guard too (`show_add_key` hands it back)

### Modal dialogs and the timers (issue #30)

- **One guard**: `ui/editor/modal_guard.rs`. `EditorForm::modal()` returns an RAII `ModalGuard` (nesting counter `ModalDepth`); its drop (also on an early return or a panic) restarts the auto-save delay when the last modal closes. Used by the close prompt, its notice, the save-failure question, the temporary files dialog and Rename Tab.
- **Every other modal is seen too**: `modal_open()` is true while a guard is alive, while a LockNote dialog is open (`dialogs::style::open_dialog_count()`: registered by `apply_dialog_theme`, forgotten at `WM_NCDESTROY`) or while a standard dialog `#32770` of the thread is visible (nwg message boxes, file dialogs).
- **Auto-save**: its ticks go through `TimerGate::tick(modal_open, locked)`: `Skip` while a modal is open or the note is locked, `RestartDelay` on the first tick after a modal closed (`autosave.mark_saved()`), then `Run`. Tested (nesting, panic, tick sequence).
- **Auto-lock is never postponed** (owner decision on #30, pass 3): when its decision is due, `request_lock` locks at once without a modal; with one, `cancel_open_dialogs` posts WM_CLOSE to every registered LockNote dialog and IDCANCEL + WM_CLOSE to every visible `#32770` of the thread (their input is lost, the note is not), and `lock_pending` is run by `run_pending_requests` on the 200 ms highlight tick: `pending_step` → `Run` once no modal is left (the lock runs from the application's own loop, after every dialog caller got Cancel), `Force` after `LOCK_FORCE_AFTER` (2 s) when a box cannot be cancelled (a Yes/No message box), the lock then runs under it (`WARN lock: a dialog did not close, locking under it`; the callers re-check `is_locked`). The Lock button / Ctrl+L goes through the same `request_lock(true)`. A close requested under a dialog (`on_close` while `modal_open()`) cancels the dialogs the same way and runs the close flow once they are gone (never forced). Tested: `lock_request`, `pending_step`.
- **No change under or after a lock**: while locked every menu command is ignored except File > Quit, Help > About and the tray items (`INFO menu: command ignored while locked`); Rename Tab drops its result if the note locked meanwhile. `on_close` defers while `modal_open()` (see above).

### Auto-Save

- **Setting**: `auto_save` (seconds, 0 = disabled, default: 30)
- **Timer**: interval = `auto_save * 1000` ms
- **Trigger**: timer tick
- **Action**: if any tab is modified, perform full save (same as Ctrl+S)
- **UI values**: Disabled, 15, 30, 60, 120 seconds

---

## 14. Updater

### Version & Endpoint

```
CurrentVersion = "1.0.1"
GitHubRepo = "ctardy/LockNote"
ApiUrl = "https://api.github.com/repos/ctardy/LockNote/releases/latest"
```

### Startup Check (silent)

1. BackgroundWorker fetches latest release from GitHub API
2. Compares version (semantic version parsing)
3. If newer: prompts user with version comparison
4. On confirm: downloads, extracts, migrates data

### Manual Check (Help menu)

Same flow but shows feedback even if up to date:
- "You are running the latest version."
- Or error message if check fails

### Update Flow

1. Download `.zip` from GitHub release assets
2. Extract via Shell32 COM (`shell.NameSpace().CopyHere()` with flag 0x14)
3. Find `LockNote.exe` in extracted directory (recursive search)
4. Read current encrypted payload (`storage::read_data`, §15)
5. Write new exe + that payload to the staging .tmp file, atomically (`storage::write_staged_image`); being more recent than the .exe, it is promoted at exit like a save
6. Clean up temp directory
7. Ask "Restart LockNote now to apply it?". Yes: close (final save), the swap helper replaces the exe and starts it again (`--relaunch`, see the swap helper). No: the exe is replaced at the next exit, without restart.

### Version Comparison

Semantic version: `major.minor.patch` — compares each component numerically. Leading `"v"` stripped from tag names.

### Data Preservation

The encrypted payload is migrated to the new binary using the same marker system. The marker bytes are obtained via `Storage.GetMarkerForUpdate()`.

### TLS Requirement

.NET 4.8 `WebClient` requires TLS 1.2 forced at startup:
```csharp
ServicePointManager.SecurityProtocol = (SecurityProtocolType)3072; // TLS 1.2
```

---

## 15. Lifecycle & File Swap

The running `.exe` cannot overwrite itself. Every save writes the whole image (`[program][marker][payload]`) to a staging file, `%LOCALAPPDATA%\LockNote\{hash}.tmp`, and a helper moves that file over the `.exe` once the process has exited. **Until that move succeeds, the `.tmp` is the only copy of the latest save**: the `.exe` still holds the previous one, and copying the `.exe` elsewhere at that moment does not carry the latest notes. Nothing in LockNote deletes a `.tmp` on a guess (issue 055).

Whether a `.tmp` is the latest state is decided **by content, never by modification dates**: a file server, a clock change or a copy can put dates in any order. A companion file records which notes of the `.exe` the pending save was based on; as long as the `.exe` still holds them, nothing else has written it and the `.tmp` wins.

Code: `src/storage/mod.rs` (staging, order of the pair, session, conflict, cleanup, swap lock), `src/storage/inventory.rs` (the pending `.tmp` files of the directory and who they belong to, deletion of an orphan asked by the user), `src/storage/decision.rs` (image format, companion, facts and decision; compiled into the helper too, which takes the decision again before renaming), `src/storage/swap_common.rs` (what the application and the helper share: names, log, swap lock), `src/bin/locknote_swap.rs` (the helper, embedded in the main binary as `assets/locknote-swap-embedded.exe`), `src/ui/mod.rs` (`run`, `settle_and_open`), `src/main.rs` (swap lock at startup).

### Files

```
%LOCALAPPDATA%\LockNote\{hash}.tmp     pending image: [program][marker][payload]
%LOCALAPPDATA%\LockNote\{hash}.base    its companion (text)
%LOCALAPPDATA%\LockNote\conflicts\     versions set aside by a conflict (never deleted)
%LOCALAPPDATA%\LockNote\swap-error.log one dated line per event
```

- `hash` = first 8 bytes of SHA256(exePath.ToUpperInvariant()) as hex (`swap_common::exe_hash`); unchanged since the first version, so a `.tmp` left by an earlier version is found under the same name
- Case-insensitive path hashing (Windows paths)
- One `.tmp` per exe path: exes at different paths do not collide, and none touches the `.tmp` of another
- OS temp dir instead of `%LOCALAPPDATA%` when that variable is missing (`swap_common::staging_dir`; the swap log follows it)

Companion (`storage::Companion`), written atomically like the `.tmp`, strict reading (unknown or repeated key, bad value: unusable):

```
LockNote staging companion 1
exe=<full path of the exe>
base=<SHA-256 of the payload the session started from | none>
program=<exe | updater>
volume=<8 hex: serial number of the volume of the exe>   (optional)
```

- `volume` (issue 057): written by every companion write (`write_companion_in`, `inventory::volume_serial` of the exe); a volume that cannot be read (or whose serial number is 0) is logged once per process (`WARN companion written without volume: ...`) and the line left out, the save goes on. The decision ignores it (`Companion::parse`); only the Temporary files dialog reads it (`Companion::parse_with_volume`) to tell an orphan from an exe whose drive letter now names another disk. At most one line, exactly 8 hex digits, else the companion is malformed. The swap helper parses companions with the same code: the embedded helper must be rebuilt from this source (`scripts/build-pro.cmd` / `build.cmd` do it), or an older one would judge such a companion malformed and abandon the swap.

- `base`: the payload the session opened (the `.exe`'s, or the base already recorded when it opened the `.tmp`). Every write of the `.tmp` keeps it.
- `program=updater`: the program part of the `.tmp` was downloaded by the updater and is kept; otherwise the program of the `.exe` is the one that counts.
- The `.tmp`, the payload and `.lnbak` formats are unchanged.

Order of the pair (`storage::stage_pair_in`, the only function that writes a `.tmp`; issue 055, pass 3). **Invariant: the companion on disk never claims more than the `.tmp` on disk justifies**, whatever step fails or is cut short by a crash:

- writing: when a `.tmp` is already there, its companion is first brought to what holds for that `.tmp` and for the new one (the `base` they share, else the current one; `program=updater` only if both say it); then the `.tmp`; then the final companion. A `.tmp` staged again without a new save (conflict resolved by "Yes") keeps its modification date;
- removing: the `.tmp` goes first (moved over the `.exe`, into `conflicts\`, or deleted when stale); its companion is removed only once the `.tmp` is gone (a stale `.tmp` that cannot be deleted keeps its companion).

A cut between two steps leaves the previous pair, or the new `.tmp` under a companion that claims less than the final one. Proof: `storage::pending_tests::every_failure_of_a_pair_step_leaves_a_state_judged_without_loss_or_downgrade` (a failure injected at each of the 30 steps of every function that writes, moves or removes the pair, then the next start, a session and two exits) and `brute_force_no_payload_is_lost` (the same injection at every step of the first life of each of its combinations).

### What is done with an existing .tmp (`storage::PendingDecision`)

One pure function, `decide_pending(tmp, companion, exe)`, used at startup, when opening the session, when reading another LockNote file (import) and at exit:

| Order | When | Decision | Effect |
|---|---|---|---|
| 1 | no `.tmp` | `None` | open the `.exe` |
| 2a | the `.tmp` cannot be read, or is not a regular file; or its companion exists but cannot be read (an I/O error: held without sharing by an antivirus or a backup tool, access denied) or is not a regular file | `TmpUnreadable` | the `.tmp` may be the latest save, and without its companion nothing tells what it was based on: logged; no session is opened (message naming the file that cannot be read, then LockNote quits without changing anything); nothing promoted or set aside, at startup, at exit or by the helper, as long as the file stays unreadable (released by the next exit, the companion gives the normal decision again); an import from that file is refused, naming the same file |
| 2b | the `.tmp` holds no usable image: no marker (unless `program=updater`), fewer than 80 bytes after the marker | `Ignored(reason)` | neither promoted nor deleted; logged; the session opens on the `.exe`, and its first save moves that `.tmp` into `conflicts\` instead of overwriting it |
| 3 | the `.exe` cannot be read | `ExeUnreadable` | logged; no session is opened (message naming the file, then LockNote quits without changing anything); nothing promoted |
| 4 | the `.exe` holds the same payload as the `.tmp` (and the `.tmp` holds no program from the updater that differs) | `Stale` | `.tmp` and companion deleted at startup (logged) |
| 5 | companion present, `base` = payload of the `.exe` | `Promote` | the session opens on the `.tmp`; it is moved over the `.exe` at exit — **whatever the dates** |
| 6 | companion present, `base` ≠ payload of the `.exe` | `Conflict(ExeChanged)` | the `.exe` was written by something else: question before any password |
| 7 | companion read but unusable (malformed, not UTF-8, written for another exe) | `Conflict(CompanionInvalid)` | question (no guess) |
| 8 | no companion (left by a version ≤ 1.2.23): `.exe` strictly more recent | `Conflict(LegacyExeNewer)` | question |
| 9 | no companion, otherwise | `Promote` | at startup, the companion is written at once with `base` = payload of the `.exe` now; from then on rule 5/6 applies |

Rules 8-9 are the only place dates are compared, at startup only. At an exit that did not go through the startup step (it failed before the `.tmp` was settled), a `.tmp` without companion is not promoted (logged); the next start decides.

A companion without `.tmp` is removed at startup (logged).

Validity is structural (marker, minimum size). A `.tmp` truncated inside its payload but still 80 bytes long is not detected here; LockNote no longer writes such a file (atomic staging write).

### The session (`storage::open_session`)

After the startup step, `open_session` opens the notes per the decision and keeps, in memory, the encrypted payload it opened and later the one it last wrote. Reading the notes again during the session (`read_session_data`: unlock after a lock, the updater) returns that payload, never what the files hold at that moment: nothing written by something else can replace the session's last save, whatever the dates.

### Startup Flow

```
main():
  1. single-instance guard
  2. swap lock of this exe (`Local\LockNote_Swap_{hash}`), waited up to 60 s;
     not obtained -> message, quit, nothing touched (logged)
  3. clean debris older than 1 minute (see Cleanup): no .tmp is deleted
  4. note recent swap-log entries in LockNote-crash.log (log < 5 min old)
  5. ui::run(exe):
       a. settle_pending_at_startup: companion without .tmp removed; Stale -> deleted;
          Ignored / TmpUnreadable / ExeUnreadable / Conflict logged; legacy .tmp to promote
          -> companion written
          TmpUnreadable (the .tmp or its companion), ExeUnreadable or a failure
          -> message naming the file, quit without changing anything
          Conflict -> question below
       b. open_session -> payload (failure -> message, quit)
       c. no data  -> CreatePasswordDialog -> EditorForm (new note)
          data     -> UnlockDialog (-> TOTP) -> EditorForm
       d. on return, whatever the way out: storage::promote_pending_on_exit,
          then the swap lock is released (already handed to the helper if one was launched)
```

### Conflict

Typical causes: the `.exe` was replaced or written elsewhere (manual update, copy from another machine, another computer on a share) while a save was still waiting in the `.tmp`; or a `.tmp` left by ≤ 1.2.23 is older than the `.exe`.

A Yes / No / Cancel message box (`conflict.*` keys) shows the two modification dates (UTC), the path of the `.exe` and the folder where the other version goes. It appears before the notes are decrypted, hence before the language of the encrypted settings is known: like the unlock window, it is shown in the language in force at startup (English); the translations exist but are not reached there.

- **Yes, keep the pending save** (`resolve_conflict_keep_tmp`): the whole `.exe` is first copied to `conflicts\` and read back; unless `program=updater`, the `.tmp` is staged again as `[program of the .exe][marker][payload of the .tmp]`, keeping its date; last, the companion takes the payload the `.exe` holds now as its `base` (order of the pair: a failure at any step leaves the conflict unresolved, and the message "could not be applied" is true). The decision is then `Promote`. No date is involved (an `.exe` dated in the future is no obstacle).
- **No, keep the program file** (`resolve_conflict_keep_exe`): the `.tmp` is moved to `conflicts\`, its companion removed.
- **Cancel**: LockNote quits; both files stay as they are and the question comes back at the next start.

Each resolution is logged with the choice and the path of the version set aside. A failure while applying the choice is shown, logged, and LockNote quits; nothing is deleted.

`%LOCALAPPDATA%\LockNote\conflicts\{hash}-{unix seconds}-{exe|tmp}.exe` (a counter is inserted before the origin when the name is taken; names are created exclusively, never reused). Both origins are complete LockNote files, hence `.exe`: they open with File > Import from another LockNote... (its file filter is `*.exe`) and the password they had. LockNote never deletes these files. The name is reserved (an empty file) before the version moves in: a crash or power cut between the two leaves an empty `.exe` there, with no loss (the version is still where it was).

When the `.exe` holds no notes at all (a clean program dropped over it), the message says so; keeping the pending save puts it behind that new program.

A conflict that appears during a session (the `.exe` written elsewhere while LockNote runs from it) does not interrupt the session: its saves keep going to the `.tmp`; at exit the `.tmp` is not promoted (logged) and the question comes at the next start.

### Save Flow

```
Save():
  1. Collect all data (settings + tabs + vault + attachments)
  2. Compress
  3. Encrypt with current password
  4. storage::write_data (open session only, else an error):
       program  = .tmp's if companion says program=updater, else the .exe's (dates play no part)
       a .tmp the session did not open is moved into conflicts\ first
       then, in the order of the pair (stage_pair_in), each file atomically:
       companion as it holds for the old and new .tmp -> {hash}.base (if it changes)
       [program][marker][payload]                     -> {hash}.tmp
       final companion (base of the session)          -> {hash}.base (if it changes)
```

- Atomic write (`write_staging_file`): the bytes go to `{name}.{16 hex}.part` (exclusive creation), are flushed (`sync_all`), then that file is renamed over the target. A failure at any step is reported to the caller, removes the part file and leaves the previous file untouched.
- The updater stages `[new program][marker][payload of the session]` (or the program alone when there are no notes yet) through `storage::write_staged_update`, with `program=updater` in the companion, written only once the `.tmp` holds that program (order of the pair): the exit promotes it even without notes, and later saves keep that program. A downloaded program that already holds a marker is refused.

### Exit / Swap

**Close flow** (`ui/editor/close_flow.rs`, issue #22): `on_close` captures the window state, then `run_close` decides (pure, tested through the `CloseSteps` trait): with unsaved changes, `save_on_close` Always saves, Never declines, Ask shows the prompt (Cancel stays open); then the ssh-agent keys are unloaded and, unless the save was declined, a final save runs; only then are the password and the text wiped. **A failed save stops the close, unless the user then chooses to close without saving** (pass 2): the saves of the close flow use `save_note` (no "Save Error" box; the reason is logged in English by the save: `WARN save: failed stage=encrypt|write err=...`), then `close_without_saving` shows one dialog holding the translated reason (`close_confirm::SaveFailedDialog`, pass 3: LockNote's own buttons `close.keep_open` / `close.close_unsaved` instead of a Yes/No box labelled in the Windows language, owned by the main window, shown under a `modal()` guard). **"Keep LockNote open" is the default** (Enter; Esc and the title bar give `Dismissed`, which keeps it open too: `closes_without_saving`, tested) (`on_close` returns false, text and password stay, logged `WARN close: ... save failed, window stays open`); "Close without saving" closes without any further save (logged `... closing without saving (user choice)`), so a save that keeps failing (unreachable share, full disk) cannot make LockNote impossible to close.

ssh-agent ordering: the keys are unloaded after the prompt and before the final save, so the saved list matches the agent; when that unload shortens the list of an unmodified note, `unload_agent_keys` saves it with `save_note` (no box) and, on failure, marks the note modified, so the final save, or its single question, reports the problem once.

**No borrow across a dialog** (pass 3): a dialog's nested loop can run WM_ENDSESSION or the lock, which wipe the password. Dialogs get `password_copy()` (a `Zeroizing` copy), never a `Ref` (`no_dialog_holds_a_borrow_of_the_password` scans the editor), and every wipe goes through `wipe_or_defer`: `try_borrow_mut`, and when the password is still borrowed a `WARN wipe: ... deferred` and a retry on the next 200 ms highlight tick, never a panic (the release build aborts on panic).

**Logoff / shutdown** (`ui/editor/end_session.rs`, issue #29; no dialog may be shown): WM_QUERYENDSESSION runs `query_end_session`: `end_session_saves(locked, modified, save_on_close)` is true unless the note is locked (password wiped, saved at lock) or modified with `never`; Ask saves like Always, since no prompt is possible and losing the text is worse. The save is `save_note` (no box). On failure the shutdown is blocked: `ShutdownBlockReasonCreate` with `endsession.blocked` (translated, holds the reason) and FALSE returned (`WARN endsession: save failed, shutdown blocked`). WM_ENDSESSION with wParam TRUE runs `end_session` (pass 3): `mark_exiting` first (`exiting` and `is_locked` set: `save_note` refuses every save, shortcuts and menu commands are refused as on a locked note, so no command such as Security > change password can run on the wiped state), `unload_agent_keys(false)` (logs only; being "locked" it does not save the shortened ssh-agent list: no second save in the shutdown time budget, the keys stay listed on disk and are removed again at the next unlock), `wipe_plaintext_stores` (as the lock: text, password, tabs, attachments, vault, prompts) plus `wipe_totp_secrets`, then `ui::finish_exit_once` (the same promote + swap-lock release as `ui::run`; a flag makes it run once whichever comes first; if Windows kills the process during the promote, the swap is atomic and the `.tmp` is kept for the next start), then `nwg::stop_thread_dispatch`. wParam FALSE (cancelled) only removes the block reason (`end_session_message`, tested both ways); a later successful save also removes it (`clear_shutdown_block`). A close prompt open at WM_QUERYENDSESSION does not change the rule: Ask saves, as documented in the user guides (the prompt cannot be answered any more). Tested: decision table, block with reason, order unload → wipe → exit, run-once.

`ui::run` calls `storage::promote_pending_on_exit` once (through `finish_exit_once`), after the session, for every way out: window closed, closed while locked, too many wrong passwords on the lock overlay, create-password / unlock / TOTP dialog cancelled. `EditorForm::on_close` does not launch anything.

```
promote_pending_on_exit(exe):
  decision == Promote:
     no companion yet               -> logged, not promoted (decided at the next start)
     program of .tmp != program of .exe and not program=updater
                                     -> .tmp staged again behind the program of the .exe (logged)
     extract the helper to %TEMP%\locknote-swap-{pid}-{32 hex}.exe, create the event
     Local\LockNote_SwapReady_{16 hex}, start the helper detached:
        <parent pid> <tmp> <exe> <ready event> [--relaunch]
        (--relaunch: the user answered Yes to "Restart LockNote now?" after
        an update, storage::request_relaunch_after_swap)
     wait (up to 15 s) for the event or for the end of the helper process,
     whichever comes first, then release the swap lock
  Conflict / TmpUnreadable / ExeUnreadable / Ignored -> logged, nothing launched
  otherwise                          -> nothing
  helper not extracted / not started / not ready -> logged, .tmp kept
```

Swap lock: the named mutex `Local\LockNote_Swap_{hash}` (`swap_common::SwapLock`). The application holds it from startup to exit; the helper signals the ready event in the same call that queues it on the mutex (`SignalObjectAndWait`), so it owns the lock as soon as the application releases it, before any other LockNote of this exe can start (the single-instance mutex is still held by the exiting process). Two swaps of the same exe, or a swap and a session, never overlap. When an owner dies holding it (crash, kill), the mutex is destroyed with its last handle: the next owner gets it as `Free`, without a log line. `Abandoned` (logged) is reported only when another process held a handle on it at that moment (it was waiting for it).

The lock is `Local\`: it only covers the sessions and helpers of one Windows session on one computer. Another computer writing the same `.exe` on a share, or another Windows session, is not kept out; the helper's re-check (below) narrows that window but does not close it.

Helper (`locknote-swap`):

1. Take the swap lock (signalling the ready event when given), up to 60 s. Not obtained, or the ready event cannot be opened: log, exit code 3, nothing touched.
2. Wait for the parent process to exit (30 s cap).
3. Re-check: take the decision again (`decision::decide_pending`, the function the application uses) on the `.tmp`, its companion and the `.exe` as they are now. Anything but `Promote` stops the swap: log "swap abandoned, nothing touched" with the cause, exit code 1, the `.tmp` and its companion stay, and the next start judges them:
   - the `.exe` no longer holds the `base` (written by something else), or the companion became invalid: `Conflict`, the question at the next start;
   - the `.exe` already holds the notes of the `.tmp` (`Stale`): the `.tmp` is deleted at the next start, without a question;
   - the `.exe`, the `.tmp` or its companion cannot be read (`ExeUnreadable`, `TmpUnreadable`): the next start shows the file named and quits without changing anything while it stays unreadable;
   - the `.tmp` no longer holds a usable image (`Ignored`), or is gone (`None`): nothing to promote.
   Then `MoveFileExW(tmp, exe, REPLACE_EXISTING | WRITE_THROUGH)`.
4. Only on `ERROR_NOT_SAME_DEVICE` (17: the `.exe` is on another volume than `%LOCALAPPDATA%`: USB key, second disk, network share):
   1. read the `.tmp` (size, SHA-256, modification time);
   2. write it next to the `.exe` as `{exe name}.{16 hex}.lnswap` (exclusive creation), `sync_all`;
   3. read that copy back and compare size and SHA-256 with the `.tmp`;
   4. give the copy the modification time of the `.tmp`;
   5. re-check as in step 3 (the copy may have taken long), then `MoveFileExW(copy, exe, REPLACE_EXISTING | WRITE_THROUGH)`: a rename within one volume;
   6. delete the `.tmp` only if it is unchanged since step 1 (same modification time, size and SHA-256).
   A failure at steps 2-5 removes the copy and keeps the `.tmp`. `MOVEFILE_COPY_ALLOWED` is not used: it copies straight onto the target, and an interruption would leave a half-written `.exe`.
5. Once the `.tmp` is gone, its companion `{hash}.base` is removed (a failure is logged; the next start removes it).
6. Any failure is appended to `%LOCALAPPDATA%\LockNote\swap-error.log` (`[unix seconds] message`, with the Win32 error), the helper exits with code 1, and the `.tmp` stays: the next start opens it and the next exit tries again. Exit codes: 0 done, 1 swap failed or abandoned by the re-check, 2 bad arguments, 3 lock not obtained, 4 the `.exe` was replaced but could not be started again.
7. With `--relaunch`, after the swap lock is released and the lines of the swap are logged: when the swap is done and the parent has exited, start the `.exe` again (in its own folder, no argument; a failure is logged, exit code 4). When the swap did not complete, or the parent was still running at the 30 s cap (it holds the single-instance mutex: the new session would quit at once), nothing is started: the log says "LockNote not restarted after the update". On the application side, when the user asked for the restart but no helper is queued at exit (`promote_pending_on_exit` returns false: nothing to promote, conflict, launch failure), a message says LockNote was not restarted and names `swap-error.log`, before the swap lock is released. The restart is done by the helper embedded in the version being left: an update *to* the version that brings it does not restart yet, the next one does.

Remaining window: the re-check reads the whole `.exe`, so a writer the `Local\` lock does not cover (another computer on a share, another Windows session) can still write the `.exe` from the start of that read to the end of the rename; that write is then replaced. Measured (refuter, issue 055, 2026-11-16): on an SMB share, 166 ms of re-check + 5 ms of rename for a 3 MB `.exe`, 1.9 s + 111 ms for 48 MB; on `D:` (exFAT), 124 + 13 ms and 1.67 s + 53 ms. Known limit: one computer is the supported use (confirmed by the user). Also outside that use: an `.exe` rewritten in place, not atomically, by another computer while the companion says `base=none` could be read half-written and judged promotable.

Known failure codes: 17 handled as above; 5 the `.exe` is still running, read-only (NTFS) or held by another program; 183 read-only target on exFAT; 3 drive or folder absent.

Not covered: a crash or kill of LockNote, and the end of the Windows session, do not go through `promote_pending_on_exit`. The `.tmp` is then promoted at the end of the next session.

### Cleanup (startup)

`storage::cleanup_swap_leftovers` runs once the swap lock is held (no helper of this exe is at work). It removes only what LockNote itself may have left, by exact name pattern, regular files only (`symlink_metadata`), older than 1 minute:

- `{hash}.tmp.{16 hex}.part` and `{hash}.base.{16 hex}.part` of the running exe in `%LOCALAPPDATA%\LockNote\` (a save interrupted before its rename);
- `locknote-swap-*.exe` in `%TEMP%` (extracted helpers);
- `{exe name}.{16 hex}.lnswap` next to the running exe (a cross-volume swap interrupted between its copy and its rename).

It never removes a `.tmp`, whichever exe it belongs to. The only way a `.tmp` of another exe goes away other than by its own exe is a deletion asked by the user in File > Settings > Temporary files, for an orphan only (§11.4 TempFilesDialog, issue 057).

### Tests

`cargo test storage::pending_tests` (decision table without clock, rule by content in both date orders, legacy `.tmp` of the real case, session reads, program part (A5) and update without notes (A8), unreadable exe, unusable / ignored / stale `.tmp`, both resolutions with their log lines, names never reused, atomic write and failed rename, cleanup, brute force over `.tmp` × exe × dates × companion, the runbook procedure and the log lines it announces; issue 055 pass 3: the test hook `pair_step` that makes each step changing the pair fail in turn, `every_failure_of_a_pair_step_leaves_a_state_judged_without_loss_or_downgrade`, the N1, R3 and N2 cases, `unreadable_tmp_stops_the_startup_like_an_unreadable_exe` (R1), `unreadable_exe_is_named_in_the_message` (R5), `a_helper_that_ends_before_it_is_ready_is_not_waited_for` (R6); final touch: `a1_companion_held_at_startup_is_not_a_conflict`, `a1b_companion_held_through_the_answer`, `a2_import_with_an_unreadable_tmp_does_not_return_the_older_notes`), `cargo test storage::inventory` (issue 057: each status of a listed `.tmp`, files not listed, missing folder, files deleted while listing, deletion of an orphan and its log line, refusal for every other status, for the open file, for a file back between the listing and the deletion, while another thread holds the swap lock; review: another volume, no volume recorded, unreachable folder, altered or relative path, a real name with an unpaired surrogate, an abandoned lock), `pending_tests::companion_volume_line_is_optional_and_read_strictly`, `cargo test storage::swap_common` (hash of existing names, swap lock between threads, abandoned lock), `cargo test --bin locknote-swap` (cross-volume path through injected primitives: success, failed rename, copy reading back different, `.tmp` changed meanwhile, missing folder; swap lock: two swaps, hand-over, timeout, abandoned; exit codes, companion; N3, the re-check before each rename: `exe_written_after_the_exit_decision_is_not_overwritten`, `exe_written_during_a_cross_volume_copy_is_not_overwritten`, `swap_goes_on_while_the_decision_is_still_to_promote`, `companion_unreadable_at_the_recheck_abandons_the_swap`), `cargo test --test swap_helper` (the real helper binary, `LOCALAPPDATA` redirected: exit codes 0 / 1 / 3, `helper_touches_nothing_when_the_pending_save_is_no_longer_current`). A real second volume: `set LOCKNOTE_TEST_OTHER_VOLUME=<new directory on another volume>` then `cargo test --test swap_helper -- --ignored` and `cargo test cleanup_next_to_an_exe_on_another_volume -- --ignored`. Every test works in a directory unique to its process; no test writes to the real `%LOCALAPPDATA%\LockNote`.

---

## 16. Keyboard Shortcuts

### Editor Shortcuts

| Shortcut | Action |
|----------|--------|
| Ctrl+S | Save |
| Ctrl+Q | Quit |
| Ctrl+F | Find |
| Ctrl+H | Find & Replace |
| Ctrl+G | Go to line |
| Ctrl+D | Duplicate line |
| Ctrl+Shift+K | Delete line |
| Ctrl+Shift+V | Paste plain text |
| Ctrl+A | Select all |
| F5 | Insert timestamp (`yyyy-MM-dd HH:mm`) |
| Ctrl+T | New tab |
| Ctrl+W | Close tab |
| F2 | Rename the active tab (issue #24; inert in the vault tree, see §8) |
| Ctrl+Tab | Next tab |
| Ctrl+Shift+Tab | Previous tab |
| Ctrl+B | Toggle vault panel |
| Ctrl+J | Toggle attachments panel |
| Ctrl+P | Print |

### Global Hotkey

- Default: Ctrl+Shift+L
- Configurable in Settings
- Toggles window minimize/restore
- Registered via Windows `RegisterHotKey` API
- Requires at least one modifier (Ctrl, Alt, or Shift)

### Dialog Shortcuts

| Shortcut | Context | Action |
|----------|---------|--------|
| Enter | SearchBar | Find Next / Replace |
| Escape | SearchBar | Hide search, refocus editor |
| Enter | Any dialog | OK / Confirm |
| Escape | Any dialog | Cancel |

---

## 17. Feature Matrix

Since 2.0 every feature is in LockNote: what the Public edition had (single encrypted exe, themes, find & replace, go to line, line numbers, occurrence highlighting, word selection by double-click, window placement, word wrap, font, tray, auto-update, line commands, timestamp, password strength meter, paste as plain text) and what was Pro only up to 1.2.x: multi-tab notes and tab colours, compression, i18n (EN/FR/ES/DE), vault (category tree), prompts, password generator, TOTP authenticator and 2FA at unlock, auto-lock, clipboard auto-clear, auto-save, global hotkey, show/hide line numbers, file drag & drop import, encrypted attachments, steganography, print / export, SSH keys, full backup and safety backups.

---

## 18. Constants & Magic Values

| Value | Use | Location |
|-------|-----|----------|
| `0xAA` | Marker XOR key | Storage |
| `100,000` | PBKDF2 iterations | Crypto |
| `256` | AES key size (bits) | Crypto |
| `16` | Salt size (bytes) | Crypto |
| `16` | IV size (bytes) | Crypto |
| `32` | HMAC-SHA256 size (bytes) | Crypto |
| `64` | Total derived key material (bytes) | Crypto |
| `80` | Minimum encrypted payload (bytes) | Crypto/Storage |
| `16` | Marker size (bytes) | Storage |
| `"LNGZ:"` | Compression prefix | Compression |
| `30` | TOTP period (seconds) | Totp |
| `6` | TOTP digit count | Totp |
| `20` | TOTP secret size (bytes) | Totp |
| `5` | Max unlock attempts | UnlockDialog |
| `200` | Highlight debounce (ms) | LineNumberTextBox |
| `2` | Min selection length for highlight | LineNumberTextBox |
| `6` | Default min password length | Settings |
| `4` | Min password length of a file without the key (`Settings::parse_fallback`) | Settings |
| `1–64` | Password length range | Settings |
| `6–72` | Font size range (pt) | Theme/Settings |
| `80–200` | Tab width range (px) | TabBar |
| `30` | TabBar height (px) | TabBar |
| `52` | Vault card height (px) | VaultPanel |
| `0x4C4E` | Hotkey ID ("LN") | EditorForm |
| `0x0312` | WM_HOTKEY message | EditorForm |
| `50` | Stego payload/image ratio warning (%) | Stego |
| `100 MiB` | Stego max payload size | Stego |
| `30 s` | Swap helper: longest wait for the parent process to exit | locknote-swap |
| `17` | `ERROR_NOT_SAME_DEVICE`: the only error that starts the copy-then-rename swap | locknote-swap |
| `60 s` | Minimum age of the debris removed at startup | Storage |
| `0x14` | Shell32 CopyHere flags (no UI) | Updater |
| `"1.0.1"` | Current version | Updater |

---

## 19. Security Properties

### Cryptographic Guarantees

1. **Encrypt-then-MAC**: HMAC is computed over `salt || iv || ciphertext`. Verification before decryption prevents padding oracle attacks.
2. **Constant-time HMAC comparison**: prevents timing side-channel attacks.
3. **Random salt + IV per encryption**: same plaintext produces different ciphertext (IND-CPA).
4. **100k PBKDF2 iterations**: GPU-resistant key derivation.
5. **Separate keys**: encryption key and MAC key derived independently from 64-byte key material.

### Memory Safety

- All sensitive buffers (keys, plaintexts, ciphertexts) cleared with `Array.Clear()` after use
- Password cleared from memory on app close and auto-lock
- Editor text cleared on close
- TOTP secrets cleared on Dispose

### Marker Obfuscation

Binary marker stored XORed to avoid:
- Antivirus false positives from pattern matching
- Casual discovery of data boundary via hex editor/strings

### Clipboard Protection

- Auto-clear after configurable timeout
- Only clears if clipboard still contains the copied secret (avoids clearing unrelated data)

### Auto-Lock

- Locks after configurable inactivity period
- Clears editor and password from memory
- Requires re-authentication to resume

### Attack Surface

- No network communication except update checks (opt-in)
- No temp files with unencrypted content
- No registry storage of secrets
- Encrypted payload never decrypted to disk

---

## 20. Backward Compatibility

| Scenario | Behavior |
|----------|----------|
| No settings header | All defaults applied |
| Unknown settings keys | Silently ignored |
| No `[LOCKNOTE_TABS]` | Single tab "Note 1" with all content |
| No `[LOCKNOTE_VAULT]` | Empty vault |
| Block header inside a line of text, or escaped with `\x00` | Text, not a block (§5) |
| File from 1.2.23 or older with an unescaped block header line in a text | Still cut at that line when read; escaped at the next save by this version (§5) |
| Default single tab whose plain text the reading chain could take for a block (block header between line breaks, `\r` alone included; whole session block at its end; tabs header at its start) | Saved in the tabs block, escaped, instead of plain text (§5) |
| No `"LNGZ:"` prefix | Treated as uncompressed |
| No tab color | Color = null (no tinting) |
| Pro data opened in Public | First tab extracted, rest ignored |
| Public data opened in Pro | Single tab created from content |

---

## 21. Test Coverage

### Public Tests (21 tests, 4 classes)

| Class | Tests | Coverage |
|-------|-------|----------|
| CryptoTests | 9 | Round-trip, empty, Unicode, large text, wrong password, truncated, corrupted, randomness, min size |
| StorageTests | 7 | Tmp path (deterministic, different paths, case-insensitive), read non-existent, round-trip, no marker, overwrite |
| SettingsTests | 16 | Defaults, header parsing, close actions, unknown keys, round-trip, multiline, font validation, word wrap, crypto chain |

### Pro Tests (53 tests, 8 classes)

| Class | Tests | Coverage |
|-------|-------|----------|
| TabStoreTests | 14 | Legacy, null, single/multi tabs, escaping, round-trip, empty content, delimiters, tab numbering, modified flag, active tab |
| CompressionTests | 8 | Round-trip, prefix, legacy pass-through, null, empty, large text, Unicode, encrypt pipeline |
| SecretStoreTests | 13 | No vault, null, single/multi secrets, no category, round-trip, multiline, combine, full pipeline |
| TotpTests | 23 | Base32 encode/decode, RFC 6238 vectors (5), verify, null inputs, remaining seconds, URI parse/build, secret generation, entry serialization |
| AutoLockTests | 5 | Default, round-trip, disabled, parsed, missing |
| ClipboardClearTests | 5 | Default, round-trip, disabled, parsed, missing |
| AutoSaveTests | 5 | Default, round-trip, disabled, parsed, missing |

### Test Infrastructure

- Custom `[Test]` attribute + reflection-based runner
- Assert: IsTrue, IsFalse, AreEqual, AreNotEqual, IsNull, IsNotNull, Throws<T>
- No external dependencies (NUnit-compatible API)
- Exit code 1 on any failure

---

## 22. Rust Rewrite Considerations

### Single Binary Goal

The Rust binary must remain a single `.exe` that embeds encrypted data after itself — identical to the current design. The PE format allows appending arbitrary data after the image.

### Recommended Crate Mapping

| Feature | Current (.NET) | Rust Crate |
|---------|---------------|-----------|
| AES-256-CBC | System.Security.Cryptography | `aes`, `cbc`, `cipher` |
| HMAC-SHA256 | System.Security.Cryptography | `hmac`, `sha2` |
| PBKDF2-SHA256 | Rfc2898DeriveBytes | `pbkdf2` |
| Random bytes | RNGCryptoServiceProvider | `rand`, `getrandom` |
| GZip | System.IO.Compression | `flate2` |
| Base32 | Manual | `data-encoding` or `base32` |
| Base64 | System.Convert | `base64` (std or crate) |
| HMAC-SHA1 (TOTP) | System.Security.Cryptography | `hmac`, `sha1` |
| GUI | WinForms | See UI section below |
| System tray | NotifyIcon | `tray-icon` or Win32 API |
| Global hotkey | RegisterHotKey (Win32) | `windows` crate (raw Win32) |
| Clipboard | System.Windows.Forms | `clipboard-win` or `arboard` |
| HTTP (updates) | WebClient | `ureq` (sync, no async runtime) |
| JSON (updates) | Manual parsing | `serde_json` or `jzon` |
| Registry (future) | Microsoft.Win32 | `winreg` |
| Shell32 ZIP extract | COM/reflection | `zip` crate (pure Rust) |

### UI Framework Options

| Framework | Native feel | Size | Maturity | Notes |
|-----------|------------|------|----------|-------|
| **egui** + eframe | Custom (GPU) | ~3 MB | Mature | Immediate mode, very portable |
| **Slint** | Near-native | ~2 MB | Mature | Declarative, good for forms |
| **iced** | Custom | ~4 MB | Growing | Elm-like, good theming |
| **native-windows-gui** | 100% native | ~1 MB | Stable | WinAPI wrapper, Windows only |
| **Win32 raw** | 100% native | Minimal | N/A | Maximum control, most work |

**Recommendation**: `native-windows-gui` or raw Win32 via the `windows` crate for closest parity with current WinForms look. Keeps binary small and truly native. Alternative: `egui` for cross-platform potential (future macOS/Linux).

### Binary Self-Modification

In Rust, the same marker-based approach works:
1. Read own executable path via `std::env::current_exe()`
2. Read file, search for marker (backward scan)
3. Write new data to .tmp staging file (atomic write)
4. On exit: start the embedded `locknote-swap` helper, which waits for the process to end and moves the .tmp over the .exe with `MoveFileExW` (§15). The original spawned `cmd.exe /c ping ... & move`; the helper replaced it in issue 034.

### Memory Safety Advantages

- Rust's ownership system prevents many buffer-related bugs
- `zeroize` crate for guaranteed secure memory clearing
- No garbage collector means deterministic cleanup
- `secrecy::Secret<T>` wrapper to prevent accidental logging of sensitive data

### Build-Time Feature Flags (Open-Core)

```toml
[features]
default = ["public"]
public = []
pro = ["tabs", "vault", "totp", "compression", "i18n", "auto-lock", "clipboard-guard", "auto-save", "global-hotkey", "steganography"]
tabs = []
vault = []
totp = []
compression = []
i18n = []
auto-lock = []
clipboard-guard = []
auto-save = []
global-hotkey = []
steganography = []
```

Build commands:
```bash
cargo build --release                    # up to 1.2.x: Public edition
cargo build --release --features pro     # up to 1.2.x: Pro edition
```

Since 2.0 there is no feature: `cargo build --release` builds the one edition (`scripts\build.cmd`).

This replaces the current "file replacement at build time" model with Cargo's native feature flag system, which is more idiomatic and maintainable.

### Estimated Binary Size

| Component | Estimated |
|-----------|----------|
| Rust runtime (no-std not needed) | ~200 KB |
| Crypto (aes, hmac, sha2, pbkdf2) | ~100 KB |
| GUI (native-windows-gui) | ~300 KB |
| Compression (flate2) | ~100 KB |
| HTTP (ureq) | ~200 KB |
| Application code | ~200 KB |
| **Total (stripped, LTO)** | **~1–1.5 MB** |

Current C# .exe: ~200 KB (but requires .NET Framework 4.8 runtime, ~60 MB pre-installed on Windows).

### Migration Strategy

1. **Phase 1 — Core**: Crypto, Storage, Settings, binary format (ensure byte-compatible output)
2. **Phase 2 — Public UI**: Main editor, dialogs, theme, search, updater
3. **Phase 3 — Pro features**: Tabs, vault, TOTP, compression, i18n
4. **Phase 4 — Pro extras**: Auto-lock, clipboard guard, auto-save, global hotkey
5. ~~**Phase 5 — Licensing**~~: dropped, LockNote is free and open source from 2.0 (issue 059)

### Critical Compatibility Requirement

The Rust binary **MUST** read data written by the C# binary and vice versa. This means:
- Identical wire format: `[salt 16][iv 16][hmac 32][ciphertext]`
- Identical PBKDF2 parameters: SHA-256, 100,000 iterations, 64-byte output
- Identical AES parameters: 256-bit key, CBC mode, PKCS7 padding
- Identical marker bytes (XOR with 0xAA)
- Identical text serialization (settings header, tab format, vault format)
- UTF-8 encoding throughout
- Same compression prefix `"LNGZ:"`

**This must be verified with cross-implementation test vectors**: encrypt in C#, decrypt in Rust and vice versa.

---

## Appendix A: Open Issues / Planned Features

| # | Feature | Status | Notes |
|---|---------|--------|-------|
| 10 | Remember window size | To-do | Persist window bounds in settings |
| 15 | ~~Password hint~~ | Removed | Removed — security risk (hints leak info) |
| 16 | Configurable PBKDF2 iterations | Thinking | Format impact (need to store iteration count) |
| 27 | OpenBao sync | Thinking | REST API to external vault server |
| 28 | Version history | Replaced | Snapshots removed; whole-note safety backups instead (section 10d) |

## Appendix B: File-Level Dependency Graph

```
Program.cs
  ├── Storage.cs        (read encrypted data)
  ├── Crypto.cs         (decrypt)
  ├── Compression.cs    (decompress, Pro)
  ├── Settings.cs       (parse settings)
  ├── CreatePasswordDialog.cs
  ├── UnlockDialog.cs
  └── EditorForm.cs
        ├── LineNumberTextBox.cs
        │     └── Theme.cs
        ├── SearchBar.cs
        ├── TabBar.cs           (Pro)
        ├── TabStore.cs         (Pro)
        │     └── NoteTab.cs
        ├── VaultPanel.cs       (Pro)
        │     ├── SecretStore.cs
        │     │     └── SecretEntry.cs
        │     └── SecretEditDialog.cs
        ├── Totp.cs             (Pro)
        │     └── TotpEntry.cs
        ├── TotpSetupDialog.cs  (Pro)
        ├── Lang.cs             (Pro)
        ├── SettingsDialog.cs
        ├── CloseConfirmDialog.cs
        ├── GoToLineDialog.cs
        ├── RenameTabDialog.cs  (Pro)
        ├── AboutDialog.cs
        └── Updater.cs
```

---

*End of specification.*
