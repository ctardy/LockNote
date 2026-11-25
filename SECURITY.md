# Security Policy

## Supported versions

| Version | Supported |
|---------|-----------|
| latest release | Yes |
| older | No: update first |

## Reporting a vulnerability

Please report it **privately**, through GitHub: **Security** tab of this
repository > **Report a vulnerability** (private vulnerability reporting).

1. **Do not** open a public issue or pull request about it.
2. Include: description, steps to reproduce, affected version, impact.
3. Do not test against notes that are not yours.

The report stays private until a fix is released.

## Cryptographic design

LockNote is written in Rust and uses RustCrypto crates (`aes`, `cbc`, `hmac`,
`sha2`, `argon2`) plus `ed25519-dalek` for release signatures:

- AES-256-CBC with PKCS7 padding, HMAC-SHA256 over salt, IV, KDF parameters
  and ciphertext (encrypt-then-MAC), constant-time comparison;
- key derivation: Argon2id, 64 MiB, 3 iterations, 4 lanes (stored with each
  file);
- plaintext buffers wiped after use (`zeroize`).

See `docs/SPEC.md`, section Cryptography.

## Verifying downloads

Each release holds `LockNote-windows-x64.zip`, `SHA256SUMS.txt` and
`SHA256SUMS.txt.sig` (Ed25519 signature of `SHA256SUMS.txt`). The built-in
updater refuses a release whose signature does not verify against the public
key compiled into LockNote. `LockNote.exe` is also Authenticode-signed.

To check a download by hand:

```powershell
(Get-FileHash LockNote-windows-x64.zip -Algorithm SHA256).Hash
# Compare with the line of SHA256SUMS.txt for that file
```
