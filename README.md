# LockNote

A single .exe that is both a text editor **and** an encrypted vault. Your notes are stored inside the executable itself — no config files, no temp files, no installation, no dependencies.

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

## The problem

You need to store sensitive text (passwords, keys, private notes) on a USB stick, a shared PC, or a network drive. Traditional note apps leave cleartext files on disk. Password managers require installation. Cloud solutions require connectivity and trust.

## The solution

LockNote is a **self-contained** encrypted notepad. Copy a single `.exe` anywhere — USB drive, desktop, network share — and your encrypted notes travel with it. Nothing is ever written to disk in cleartext.

| What you type | What's on disk | What an attacker sees |
|---------------|----------------|----------------------|
| `my secret API key: sk-1234` | `AES-256 ciphertext` | Random bytes |

## Quick start

### Download

Grab `LockNote.exe` from the [latest release](../../releases/latest) — a single 16 KB file, ready to use.

### Build from source

Requires only the C# compiler built into Windows (no SDK, no Visual Studio):

```cmd
build.cmd
```

Or manually:

```cmd
C:\Windows\Microsoft.NET\Framework64\v4.0.30319\csc.exe /target:winexe /platform:x64 /optimize+ /out:LockNote.exe src\Program.cs src\Storage.cs src\Crypto.cs src\EditorForm.cs src\CreatePasswordDialog.cs src\UnlockDialog.cs src\SearchBar.cs
```

### First launch

1. Create a password (entry + confirmation)
2. Type your notes in the editor
3. `Ctrl+S` to encrypt and save — data is written inside the .exe itself

### Subsequent launches

1. Enter your password (5 attempts max)
2. Edit your notes
3. `Ctrl+S` to re-encrypt with fresh salt and IV

## Keyboard shortcuts

| Shortcut | Action |
|----------|--------|
| `Ctrl+S` | Save (encrypt & write) |
| `Ctrl+F` | Find text |
| `Ctrl+A` | Select all |
| `Ctrl+Q` | Quit |

## How it works

### Self-modifying executable

LockNote appends encrypted data after a binary marker at the end of its own `.exe`. Since Windows locks a running executable, saves go to a `.tmp` file that is swapped in on next launch.

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
| Key derivation | PBKDF2-SHA256, 100 000 iterations |
| Salt | 16 bytes, random per save |
| IV | 16 bytes, random per save |
| Verification | HMAC checked before decryption (constant-time comparison) |

Wire format: `[salt 16 bytes][IV 16 bytes][HMAC 32 bytes][ciphertext]`

Separate keys are derived for encryption (32 bytes) and MAC (32 bytes) from a single PBKDF2 call (64 bytes).

### Security properties

- **Zero cleartext on disk** — no temp files, no swap, no config
- **Fresh randomness** — salt and IV regenerated on every save
- **Encrypt-then-MAC** — ciphertext integrity verified before decryption
- **Constant-time comparison** — HMAC check is not vulnerable to timing attacks
- **Buffer cleanup** — all sensitive byte arrays zeroed with `Array.Clear` after use
- **Brute-force protection** — 5 password attempts max, then the program exits
- **.NET native crypto only** — `System.Security.Cryptography`, no third-party code

## Architecture

```
src/
├── Program.cs              Entry point, .tmp swap, password flow
├── Storage.cs              Binary marker, read/write encrypted payload
├── Crypto.cs               AES-256-CBC + HMAC-SHA256, PBKDF2
├── EditorForm.cs           Main editor window, menus, shortcuts
├── CreatePasswordDialog.cs Password creation (entry + confirm)
├── UnlockDialog.cs         Password prompt (5 attempts max)
└── SearchBar.cs            Ctrl+F find panel
```

## Requirements

- **Runtime**: .NET Framework 4.8 (pre-installed on Windows 10 and 11)
- **Build**: `csc.exe` included with .NET Framework — no SDK, no Visual Studio, no NuGet
- **Output**: ~16 KB portable `.exe`
- **Platforms**: Windows 10, Windows 11 (x64)

## License

[MIT](LICENSE)
