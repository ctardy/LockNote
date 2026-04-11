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

LockNote uses only .NET Framework built-in cryptographic primitives (`System.Security.Cryptography`):

- AES-256-CBC with PKCS7 padding
- HMAC-SHA256 (encrypt-then-MAC)
- PBKDF2-SHA256 with 100,000 iterations
- Constant-time HMAC comparison
- All sensitive buffers zeroed after use

No third-party cryptographic code is used.
