# LockNote

English | [Français](README.fr.md)

**Documentation:** [User guide](docs/en/user-guide.md) (every function of the
app, step by step) · [Guide utilisateur](docs/fr/guide-utilisateur.md)

Self-contained encrypted notepad for Windows: the program and your notes are
one `.exe`. Nothing to install, no account, no cloud. Free and open source
(GPL-3.0-or-later), written in **Rust** with a native Windows GUI
(native-windows-gui).

## Features

| Feature | Description |
|---------|-------------|
| Encryption | AES-256-CBC + HMAC-SHA256 (encrypt-then-MAC), key derived with Argon2id (64 MiB, 3 iterations, 4 lanes); every plaintext buffer is wiped after use |
| Multi-tab notes | Organize notes in separate tabs within a single encrypted file; drag & drop to reorder them, the active tab is highlighted, overflowing tabs go into a "…" menu, the active tab and caret line are restored after lock/unlock |
| Encrypted vault | Secret manager shown as a category tree, open at launch: click a secret to copy it ("Copied: name" indicator), search by name, create/rename/delete categories, drag & drop secrets between them, reveal (5 s, dedicated field), multi-line secrets with a Copy button in the edit dialog |
| Prompts | Stored prompts, searched by name and text, copied in one click |
| TOTP two-factor auth | Time-based one-time passwords (RFC 6238), and an optional 2FA code at unlock |
| Attachments | Files stored encrypted inside the note |
| Safety backups | An encrypted backup of the whole note is written before every import and update, and on demand; File > Safety backups lists them, restores one or creates a new one |
| Full backup | File > Export / Import full backup: all tabs, vault, prompts, attachments and settings in one encrypted `.lnbak` file, versioned so future versions keep reading it |
| Import from another LockNote | File > Import from another LockNote...: moves the data of another LockNote exe into the running one |
| SSH keys | Tools > SSH Keys: generate Ed25519 / ECDSA / RSA keys (optional passphrase), save them in the vault, export owner-only files, load them into the Windows ssh-agent; LockNote removes the keys it loaded at lock, close and next unlock |
| Password generator | Tools > Password Generator, and a Generate... button in the vault's secret dialog: length 8–64, uppercase / lowercase / digits / symbols, your own symbol list, optional exclusion of look-alike characters |
| Auto-lock, auto-save, clipboard auto-clear | Each with its own delay |
| Global hotkey | `Ctrl+Shift+L` to show/hide from anywhere |
| Multi-language | English, French, Spanish, German |
| Find & Replace | Previous/Next, live match counter, match case, whole word |
| Updates | Checked against the signed releases of this repository: `SHA256SUMS.txt` is verified with an Ed25519 signature before any hash is trusted |

Window size and position are restored between sessions. A double-click
selects the exact text between spaces (passwords stay whole, nothing is copied
automatically).

## How the notes are stored

The notes live at the end of the exe, after a marker. A running program cannot
rewrite its own file, so every save goes to a staging file in
`%LOCALAPPDATA%\LockNote` (`<hash>.tmp`, with its companion `<hash>.base`);
when LockNote closes, a small helper embedded in the exe copies it over the
exe. Until then, that `.tmp` is the only copy of the latest save: never delete
it by hand (File > Settings > Temporary files shows them). See `docs/SPEC.md`.

## Build

Requires Rust (stable) on Windows. The Windows resources (icon, version,
manifest) are compiled by `rc.exe` (Windows SDK, MSVC toolchain) or `windres`
(MinGW, GNU toolchain); the build fails if they cannot be.

```cmd
scripts\build.cmd     REM → build\LockNote.exe
```

The main binary embeds the swap helper (`assets\locknote-swap-embedded.exe`,
generated, not in git): `build.cmd` builds it first. To do it by hand:

```cmd
cargo build --release --bin locknote-swap
copy /y target\release\locknote-swap.exe assets\locknote-swap-embedded.exe
cargo build --release --bin locknote
```

## Test

```cmd
scripts\test.cmd
```

It stages the swap helper, then runs `cargo test` with `LOCALAPPDATA`, `TEMP`
and `TMP` pointing to a throwaway folder, so that no test ever touches the
real `%LOCALAPPDATA%\LockNote` (it may hold the only copy of a save). Run
`cargo test` by hand only with the same redirection.

## Migrating from LockNote 1.2.x

- **Update every LockNote copy you have.** A LockNote 1.2.1 deletes, when it
  starts, the pending `.tmp` files of the other copies (older than one
  minute) in `%LOCALAPPDATA%\LockNote`, which may hold their latest save.
- **LockNote 1.2.1 (free edition)**: it updates itself to 2.0 and keeps your
  notes (2.0 reads the 1.2.1 format; a frozen 1.2.1 file is part of the
  tests). Known limitation: a line made only of a LockNote block header
  (such as `[LOCKNOTE_VAULT]`) in a 1.2.1 note is read as that block by 2.0
  (see CHANGELOG 2.0.0); check your note before the update.
- **LockNote Pro 1.2.x**: it does not see 2.0 (it reads another release
  feed). Migrate by hand:
  1. In the old LockNote: File > Export full backup (`.lnbak`), then **close**
     it.
  2. Put the new `LockNote.exe` next to the old exe, open it and choose a
     password.
  3. File > Import full backup with the `.lnbak` of step 1. Or File > Import
     from another LockNote..., choosing the old exe **by its usual path** (the
     drive letter you always use, not a UNC path: a pending save is tied to
     the path). Check the tabs, the vault, the TOTP codes, the prompts and the
     attachments. Not imported: the password hint, the session state and
     the keys loaded into ssh-agent.
  4. Before deleting the old exe, check in File > Settings > Temporary files that it has
     no pending `.tmp` left.
  5. Do not run a LockNote 1.2.1 on those PCs: its cleanup deletes the `.tmp`
     files older than one minute in `%LOCALAPPDATA%\LockNote`.
  6. `%LOCALAPPDATA%\LockNote\license.dat` (the licence of the Pro
     edition) is no longer used: delete it by hand if you wish.

## Documentation

- [docs/en/user-guide.md](docs/en/user-guide.md): user guide, one section per
  feature (English).
- [docs/fr/guide-utilisateur.md](docs/fr/guide-utilisateur.md): the same guide
  in French.
- `docs/SPEC.md`: full specification (formats, cryptography, file swap, UI).
- `docs/release-signing-setup.md`: release signing keys.
- `scripts/migrate-exe.ps1`: replace the program of an exe that already
  holds notes.
- `CHANGELOG.md`, `CONTRIBUTING.md`, `SECURITY.md`.

## License

Copyright (C) 2025-2026 Christophe Tardy.

LockNote is free software: you can redistribute it and/or modify it under the
terms of the GNU General Public License as published by the Free Software
Foundation, either version 3 of the License, or (at your option) any later
version. It is distributed in the hope that it will be useful, but WITHOUT ANY
WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR
A PARTICULAR PURPOSE. See [LICENSE](LICENSE) for the full text.
