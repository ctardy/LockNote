# LockNote

[![Build](https://github.com/ctardy/LockNote/actions/workflows/release.yml/badge.svg)](https://github.com/ctardy/LockNote/actions)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Platform: Windows](https://img.shields.io/badge/Platform-Windows%2010%2F11-0078D6)](https://github.com/ctardy/LockNote/releases)
[![.NET Framework 4.8](https://img.shields.io/badge/.NET%20Framework-4.8-512BD4)](https://dotnet.microsoft.com/download/dotnet-framework)

A single `.exe` that is both a text editor **and** an encrypted vault. Your notes are stored inside the executable itself — no config files, no temp files, no installation, no dependencies.

```
 ┌─────────────────────────────────────────┐
 │            LockNote.exe                 │
 │  ┌───────────────┬───────────────────┐  │
 │  │   Program     │  Encrypted data   │  │
 │  │   (16 KB)     │  (AES-256-CBC)    │  │
 │  └───────────────┴───────────────────┘  │
 └─────────────────────────────────────────┘
        One file. Zero footprint.
```

---

## The problem

You need to store sensitive text (passwords, keys, private notes) on a USB stick, a shared PC, or a network drive. Traditional note apps leave cleartext files on disk. Password managers require installation. Cloud solutions require connectivity and trust.

## The solution

LockNote is a **self-contained** encrypted notepad. Copy a single `.exe` anywhere — USB drive, desktop, network share — and your encrypted notes travel with it. Nothing is ever written to disk in cleartext.

| What you type | What's on disk | What an attacker sees |
|---------------|----------------|----------------------|
| `my secret API key: sk-1234` | `AES-256 ciphertext` | Random bytes |

---

## Quick start

### Download

Grab `LockNote.exe` from the [latest release](../../releases/latest) — a single 16 KB file, ready to use.

### Build from source

Requires only the C# compiler built into Windows (no SDK, no Visual Studio, no NuGet):

```cmd
build.cmd
```

### First launch

1. Create a password (entry + confirmation, with strength indicator)
2. Type your notes in the editor
3. `Ctrl+S` to encrypt and save — data is written inside the .exe itself

### Subsequent launches

1. Enter your password (5 attempts max)
2. Edit your notes
3. `Ctrl+S` to re-encrypt with fresh salt and IV

---

## Features

### Editor
- Line numbers gutter (auto-adjusting width)
- Find text (`Ctrl+F`) with wrap-around search
- Go to line (`Ctrl+G`)
- Insert timestamp (`F5` — inserts `yyyy-MM-dd HH:mm`)
- Duplicate line (`Ctrl+D`) and delete line (`Ctrl+Shift+K`)
- Word, character, and line count in status bar
- Cut / Copy / Paste / Paste plain text (`Ctrl+Shift+V`)
- Right-click context menu with clipboard operations
- Clickable URL detection (opens in default browser)
- Drag & drop text files into the editor
- Select all (`Ctrl+A`)

### Security
- AES-256-CBC encryption with HMAC-SHA256 authentication
- PBKDF2-SHA256 key derivation (100,000 iterations)
- Password strength indicator on creation
- 5 unlock attempts max (brute-force protection)
- Zero cleartext on disk — ever
- All sensitive buffers zeroed after use

### Settings
- Save-on-close behavior: Ask / Always / Never (with "Remember my choice")
- Always on top (View menu toggle)

---

## Keyboard shortcuts

| Shortcut | Action |
|----------|--------|
| `Ctrl+S` | Save (encrypt & write) |
| `Ctrl+F` | Find text |
| `Ctrl+G` | Go to line |
| `Ctrl+D` | Duplicate current line |
| `Ctrl+A` | Select all |
| `Ctrl+Q` | Quit |
| `Ctrl+Shift+V` | Paste as plain text |
| `Ctrl+Shift+K` | Delete current line |
| `F5` | Insert timestamp |

---

## How it works

### Self-modifying executable

LockNote appends encrypted data after a binary marker at the end of its own `.exe`. Since Windows locks a running executable, saves go to a `.tmp` staging file in `%LOCALAPPDATA%\LockNote\` that is swapped in on next launch via a hidden `cmd.exe` process.

```
┌──────────────────────┬────────┬──────────────────────────────┐
│  .NET PE executable  │ Marker │  [salt][iv][hmac][ciphertext] │
│       (16 KB)        │(16 B)  │        (variable)            │
└──────────────────────┴────────┴──────────────────────────────┘
```

### Cryptography

| Component | Detail |
|-----------|--------|
| Cipher | AES-256-CBC (PKCS7 padding) |
| Authentication | HMAC-SHA256 (encrypt-then-MAC) |
| Key derivation | PBKDF2-SHA256, 100,000 iterations |
| Salt | 16 bytes, random per save |
| IV | 16 bytes, random per save |
| Key material | 64 bytes (32 enc + 32 mac) from single PBKDF2 call |
| Verification | HMAC checked before decryption (constant-time comparison) |

Wire format: `[salt 16B][IV 16B][HMAC 32B][ciphertext]`

### Security properties

- **Zero cleartext on disk** — no temp files, no swap, no config
- **Fresh randomness** — salt and IV regenerated on every save
- **Encrypt-then-MAC** — ciphertext integrity verified before decryption
- **Constant-time comparison** — HMAC check is not vulnerable to timing attacks
- **Buffer cleanup** — all sensitive byte arrays zeroed with `Array.Clear` after use
- **Brute-force protection** — 5 password attempts max, then the program exits
- **.NET native crypto only** — `System.Security.Cryptography`, no third-party code

---

## Architecture

```
src/
├── Program.cs              Entry point, .tmp swap, password flow
├── Storage.cs              Binary marker, read/write encrypted payload
├── Crypto.cs               AES-256-CBC + HMAC-SHA256, PBKDF2
├── Settings.cs             User preferences (serialized in encrypted payload)
├── EditorForm.cs           Main editor window, menus, shortcuts, status bar
├── LineNumberTextBox.cs    RichTextBox with line number gutter
├── CreatePasswordDialog.cs Password creation with strength indicator
├── UnlockDialog.cs         Password prompt (5 attempts max)
├── SearchBar.cs            Ctrl+F find panel
├── GoToLineDialog.cs       Ctrl+G go-to-line dialog
├── SettingsDialog.cs       Settings dialog (close behavior)
└── CloseConfirmDialog.cs   Unsaved changes prompt

tests/
├── TestFramework.cs        Minimal test framework (no NuGet)
├── CryptoTests.cs          Encryption/decryption round-trip tests
├── SettingsTests.cs        Settings serialization tests
├── StorageTests.cs         Binary marker read/write tests
└── TestMain.cs             Test runner entry point
```

---

## Testing

Run the test suite (29 tests covering Crypto, Settings, and Storage):

```cmd
test.cmd
```

No external test framework required — uses a built-in minimal test runner compiled with the same `csc.exe`.

---

## Requirements

| Component | Requirement |
|-----------|-------------|
| **OS** | Windows 10 or 11 (x64) |
| **Runtime** | .NET Framework 4.8 (pre-installed on Windows 10+) |
| **Build** | `csc.exe` included with .NET Framework — no SDK, no Visual Studio, no NuGet |
| **Output** | ~16 KB portable `.exe` |
| **Dependencies** | None |

---

## Roadmap

See the [open issues](../../issues) for planned features, organized by theme:

- **Editor** — Find & replace, word wrap toggle, print support
- **Settings** — Dark theme, configurable font, remember window position
- **Security** — Auto-lock on inactivity, clipboard auto-clear, configurable PBKDF2 iterations
- **UX** — System tray, import/export, always on top

---

## Contributing

1. Fork the repo
2. Create a feature branch (`git checkout -b feature/my-feature`)
3. Make sure `build.cmd` and `test.cmd` both pass
4. Commit your changes
5. Open a Pull Request

**Important constraints:**
- C# 5 only (no `?.`, `$""`, `nameof()`, etc.)
- .NET Framework 4.8 — no .NET Core/5/6/7/8
- No NuGet packages — only `System.*` BCL namespaces
- All code must compile with the built-in `csc.exe`

---

## License

[MIT](LICENSE) — Christophe Tardy
