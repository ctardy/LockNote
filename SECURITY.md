# Security Policy

## Supported versions

| Version | Supported |
|---------|-----------|
| latest  | Yes       |

## Reporting a vulnerability

If you discover a security vulnerability in LockNote, please report it responsibly:

1. **Do not** open a public GitHub issue
2. Email: christophe.tardy@gmail.com
3. Include: description, reproduction steps, and impact assessment

You should receive a response within 48 hours.

## Cryptographic design

LockNote is written in Rust and uses only well-audited cryptographic crates (`aes`, `cbc`, `hmac`, `sha2`, `pbkdf2`, `rand`):

- AES-256-CBC with PKCS7 padding
- HMAC-SHA256 (encrypt-then-MAC)
- PBKDF2-SHA256 with 300,000 iterations
- Constant-time HMAC comparison
- All sensitive buffers zeroed after use (`zeroize`)

No third-party cryptographic code is used — only RustCrypto crates.

## Verifying downloads

Each GitHub release includes a `SHA256SUMS.txt` file. To verify:

```powershell
# PowerShell
(Get-FileHash LockNote.exe -Algorithm SHA256).Hash
# Compare with the hash in SHA256SUMS.txt
```
