# CLAUDE.md — Context for Claude Code

## Absolute rules

- **Language**: English for code, comments, commits, and documentation
- **Shell paths**: always use absolute paths
- **Git safety**: check `git status` before committing, never `git add .`
- **No backward compatibility**: project is in v0.x, breaking changes are fine
- **C# 5 only**: the .NET Framework 4.8 csc.exe only supports C# 5 — no `?.`, `$""`, `nameof()`, etc.

## Project description

**LockNote** — Self-contained encrypted notepad for Windows.

A single .exe that serves as both the text editor and the encrypted vault. Notes are stored inside the executable itself after a binary marker. No installation, no dependencies, no temp files.

### Technical stack

| Component | Technology |
|-----------|-----------|
| Language | C# / .NET Framework 4.8 |
| UI | WinForms |
| Crypto | AES-256-CBC + HMAC-SHA256 (System.Security.Cryptography) |
| KDF | PBKDF2-SHA256, 100 000 iterations |
| Build | csc.exe (built into Windows, no SDK needed) |

### Build command

```cmd
build.cmd
```

### Architecture

```
src/
├── Program.cs              Entry point, .tmp swap, password flow
├── Storage.cs              Binary marker, read/write encrypted payload
├── Crypto.cs               AES-256-CBC + HMAC-SHA256, PBKDF2
├── Settings.cs             User settings (theme, save-on-close)
├── Theme.cs                Dark/light theme system, color palette
├── EditorForm.cs           Main editor window, menus, shortcuts
├── LineNumberTextBox.cs    RichTextBox with line numbers + occurrence highlighting
├── SearchBar.cs            Ctrl+F find panel
├── TabBar.cs               Tab strip for multi-note support
├── TabStore.cs             Tab serialization/deserialization
├── NoteTab.cs              Single note tab model
├── CreatePasswordDialog.cs Password creation (entry + confirm)
├── UnlockDialog.cs         Password prompt (5 attempts max)
├── CloseConfirmDialog.cs   Save-on-close confirmation dialog
├── GoToLineDialog.cs       Ctrl+G go-to-line dialog
├── RenameTabDialog.cs      Tab rename dialog
├── SettingsDialog.cs       Settings UI (theme, behavior)
└── Updater.cs              Auto-update check via GitHub releases
```
