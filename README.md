# LockNote

[![License](https://img.shields.io/badge/License-Free%20for%20personal%20use-green.svg)](LICENSE)
[![Platform: Windows](https://img.shields.io/badge/Platform-Windows%2010%2F11-0078D6)](https://github.com/ctardy/LockNote/releases)

A single `.exe` that is both a text editor **and** an encrypted vault. Your notes are stored inside the executable itself — no config files, no temp files, no installation, no dependencies.

```
 ┌─────────────────────────────────────────┐
 │            LockNote.exe                 │
 │  ┌───────────────┬───────────────────┐  │
 │  │   Program     │  Encrypted data   │  │
 │  │   (Rust)      │  (AES-256-CBC)    │  │
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

Grab `LockNote.exe` from the [latest release](../../releases/latest) — a single portable file, ready to use.

### Build from source

Requires Rust (stable, `x86_64-pc-windows-gnu` target):

```cmd
scripts\build.cmd
```

Output: `build\LockNote.exe`

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
- Find & replace (`Ctrl+F`) with wrap-around search
- Go to line (`Ctrl+G`)
- Insert timestamp (`F5` — inserts `yyyy-MM-dd HH:mm`)
- Duplicate line (`Ctrl+D`) and delete line (`Ctrl+Shift+K`)
- Word, character, and line count in status bar
- Cut / Copy / Paste / Paste plain text (`Ctrl+Shift+V`)
- Right-click context menu with clipboard operations
- Select all (`Ctrl+A`)

### Security
- AES-256-CBC encryption with HMAC-SHA256 authentication
- PBKDF2-SHA256 key derivation (300,000 iterations)
- Password strength indicator on creation
- 5 unlock attempts max (brute-force protection)
- Zero cleartext on disk — ever
- All sensitive buffers zeroed after use

### Settings
- Dark / light theme with system-aware defaults
- Save-on-close behavior: Ask / Always / Never
- Configurable font family and size
- Minimize to system tray
- Always on top (View menu toggle)
- Auto-update check via GitHub releases

---

## Keyboard shortcuts

| Shortcut | Action |
|----------|--------|
| `Ctrl+S` | Save (encrypt & write) |
| `Ctrl+F` | Find / Replace |
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

LockNote appends encrypted data after a binary marker at the end of its own `.exe`. Since Windows locks a running executable, saves go through an atomic swap mechanism.

```
┌──────────────────────┬────────┬──────────────────────────────┐
│   Rust PE executable │ Marker │  [salt][iv][hmac][ciphertext] │
│                      │(16 B)  │        (variable)            │
└──────────────────────┴────────┴──────────────────────────────┘
```

### Cryptography

| Component | Detail |
|-----------|--------|
| Cipher | AES-256-CBC (PKCS7 padding) |
| Authentication | HMAC-SHA256 (encrypt-then-MAC) |
| Key derivation | PBKDF2-SHA256, 300,000 iterations |
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
- **Buffer cleanup** — all sensitive byte arrays zeroed after use
- **Brute-force protection** — 5 password attempts max, then the program exits

---

## Architecture

```
src/
├── main.rs                     Entry point, panic handling
├── crypto/mod.rs               AES-256-CBC + HMAC-SHA256, PBKDF2
├── storage/mod.rs              Binary marker, read/write encrypted payload
├── settings/mod.rs             User settings (theme, save-on-close)
├── theme/mod.rs                Dark/light theme system
├── updater.rs                  Auto-update check via GitHub releases
├── integration_tests.rs        Integration tests
├── ui/
│   ├── mod.rs                  UI entry point, password flow
│   ├── editor.rs               Editor, menus, shortcuts, status bar
│   ├── search_bar.rs           Ctrl+F find/replace panel
│   └── dialogs/
│       ├── create_password.rs  Password creation dialog
│       ├── unlock.rs           Password prompt (5 attempts max)
│       ├── close_confirm.rs    Save-on-close confirmation
│       ├── goto_line.rs        Go-to-line dialog
│       ├── settings_dialog.rs  Settings UI
│       ├── preferences_dialog.rs  User preferences (font, behavior)
│       ├── security_dialog.rs  Security settings (password change)
│       └── about.rs            About dialog
```

Built with Rust using [native-windows-gui](https://github.com/aspect-build/native-windows-gui) for the native Windows UI, and pure Rust cryptography crates (aes, cbc, hmac, sha2, pbkdf2).

---

## Testing

```cmd
cargo test
```

---

## Requirements

| Component | Requirement |
|-----------|-------------|
| **OS** | Windows 10 or 11 (x64) |
| **Runtime** | None — native Rust executable |
| **Build** | Rust stable (`x86_64-pc-windows-gnu`) |
| **Dependencies** | None at runtime |

---

## LockNote Pro

Looking for more features? [LockNote Pro](https://uitguard.com) adds:

- Multi-tab notes
- Encrypted password vault
- TOTP two-factor authentication
- Named snapshots / version history
- Multi-language support (EN, FR, ES, DE)
- Auto-lock, auto-save, clipboard auto-clear
- GZip compression
- Global hotkey

---

## Contributing

1. Fork the repo
2. Create a feature branch
3. Make sure `scripts\build.cmd` and `cargo test` pass
4. Open a Pull Request

---

## License

[Custom License](LICENSE) — Created by [UITGuard](https://uitguard.com)

- **Personal use**: Free to use, modify, and redistribute
- **Commercial / Enterprise use**: Paid license required — contact [contact@uitguard.com](mailto:contact@uitguard.com)
