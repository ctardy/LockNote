#!/usr/bin/env python3
"""
Sign SHA256SUMS.txt with the release Ed25519 key.

Invoked from the GitHub Actions release workflow. Reads the 32-byte
Ed25519 private-key seed from the environment variable
RELEASE_SIGNING_KEY_HEX (64 hex characters, no 0x prefix, no whitespace)
and writes a raw 64-byte detached signature to SHA256SUMS.txt.sig next
to the input file.

Never commit the private key. Never log its value. The matching public
key is embedded in src/updater.rs (RELEASE_PUBLIC_KEY).

Usage: python sign-sha256sums.py <path/to/SHA256SUMS.txt>
"""
import os
import sys
from pathlib import Path

try:
    from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey
except ImportError:
    sys.stderr.write(
        "cryptography package missing — add `pip install cryptography` to the CI step.\n"
    )
    sys.exit(2)


def main() -> int:
    if len(sys.argv) != 2:
        sys.stderr.write(f"usage: {sys.argv[0]} <path/to/SHA256SUMS.txt>\n")
        return 2

    sums_path = Path(sys.argv[1])
    if not sums_path.is_file():
        sys.stderr.write(f"not a file: {sums_path}\n")
        return 2

    hex_key = os.environ.get("RELEASE_SIGNING_KEY_HEX", "").strip()
    if len(hex_key) != 64:
        sys.stderr.write(
            "RELEASE_SIGNING_KEY_HEX must be exactly 64 hex characters (32 bytes).\n"
        )
        return 2
    try:
        seed = bytes.fromhex(hex_key)
    except ValueError:
        sys.stderr.write("RELEASE_SIGNING_KEY_HEX is not valid hex.\n")
        return 2

    sk = Ed25519PrivateKey.from_private_bytes(seed)
    payload = sums_path.read_bytes()
    sig = sk.sign(payload)
    assert len(sig) == 64, "Ed25519 signature must be 64 bytes"

    out_path = sums_path.with_name(sums_path.name + ".sig")
    out_path.write_bytes(sig)
    print(f"Wrote {len(sig)}-byte signature to {out_path}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
