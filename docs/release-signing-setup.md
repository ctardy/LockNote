# Release signing setup

Step-by-step to wire up the Ed25519 release-signing key used by the in-app
updater. Do this once. The in-app updater refuses to trust a release whose
`SHA256SUMS.txt` is not accompanied by a valid `SHA256SUMS.txt.sig` once
`RELEASE_PUBLIC_KEY` in `src/updater.rs` is flipped from `None` to
`Some([...])`.

## 1. Generate the keypair offline

**Do this on a machine you trust.** Ideally air-gapped or at least not
one that's running random background services. Never run this inside a
shell session that might be logged (screen recorders, shell history
sync, pair-programming tools, AI assistants with shell access).

```bash
# Any machine with Python 3 + cryptography
python3 - <<'EOF'
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey
from cryptography.hazmat.primitives import serialization as ser

sk = Ed25519PrivateKey.generate()
priv_seed = sk.private_bytes(ser.Encoding.Raw, ser.PrivateFormat.Raw, ser.NoEncryption())
pub = sk.public_key().public_bytes(ser.Encoding.Raw, ser.PublicFormat.Raw)

print("PRIVATE KEY (keep secret — install as GitHub Actions secret):")
print("  RELEASE_SIGNING_KEY_HEX =", priv_seed.hex())
print()
print("PUBLIC KEY (paste into src/updater.rs):")
print("  [", ", ".join(f"0x{b:02x}" for b in pub), "]")
EOF
```

The output has two halves:

- The **private** 32-byte seed as 64 hex chars. This is the only thing
  that must stay secret. Anyone who holds it can sign a fake release
  that every installed LockNote will accept.
- The **public** key as a Rust byte array, ready to paste.

## 2. Install the private key as a GitHub Actions secret

1. In GitHub, go to the `LockNote` repo (`LockNote-Pro` before 2.0) → Settings → Secrets and
   variables → Actions → New repository secret.
2. Name: `RELEASE_SIGNING_KEY_HEX`
3. Value: the 64-hex-char private seed.
4. Save. The secret is write-only after this — retrieve it from your
   own copy if you need it elsewhere.

The release workflow (`.github/workflows/release.yml`) already reads
this secret in its "Sign SHA256SUMS.txt" step. No other workflow change
is needed.

## 3. Install the public key in the client

Edit `src/updater.rs`:

```rust
// Before:
const RELEASE_PUBLIC_KEY: Option<[u8; 32]> = None;

// After (paste the public-key bytes from step 1):
const RELEASE_PUBLIC_KEY: Option<[u8; 32]> = Some([
    0xXX, 0xXX, 0xXX, 0xXX, 0xXX, 0xXX, 0xXX, 0xXX,
    0xXX, 0xXX, 0xXX, 0xXX, 0xXX, 0xXX, 0xXX, 0xXX,
    0xXX, 0xXX, 0xXX, 0xXX, 0xXX, 0xXX, 0xXX, 0xXX,
    0xXX, 0xXX, 0xXX, 0xXX, 0xXX, 0xXX, 0xXX, 0xXX,
]);
```

Also update the unit test `verify_sums_signature_noop_when_key_absent`
in the same file — with the key set, the no-op assertion no longer
holds.

Commit and tag as usual. The next release built from that commit will
be the first one whose signature clients verify.

## 4. Offline key storage

Don't leave the private key on your laptop after step 2. Options:

- **Password manager** (Bitwarden, 1Password) in a vault that only you
  unlock — acceptable for small teams, terrible against endpoint
  compromise.
- **Encrypted file** on a USB key kept in a physical safe
  (`age`, `gpg --symmetric`, or `openssl enc -aes-256-cbc`).
- **HSM / YubiKey** — best, but requires a signing path that lets the
  device sign raw Ed25519 (the CI script would have to talk to it).
  Out of scope for this first iteration; revisit if the app's user
  base grows.

Whatever you pick, **document the recovery story**. A lost key means
every future release has to be signed with a new key, which breaks the
updater for every existing install.

## 5. Key rotation (when needed)

If the key is ever suspected compromised:

1. Generate a new keypair (step 1).
2. Ship a client update **signed with the old key** that embeds the
   new public key alongside the old one (add a second `Option<[u8;32]>`
   const and accept either sig during a transition window).
3. Rotate `RELEASE_SIGNING_KEY_HEX` in GitHub Actions.
4. After a release or two, remove the old public key from the client.

Until step 4 ships to every install, a holder of the old key can still
sign malicious updates. So key compromise is serious — treat it like a
credential breach.

## 6. Things the updater verifies

With `RELEASE_PUBLIC_KEY = Some(...)` the download flow in
`download_and_update` does:

1. Fetch `SHA256SUMS.txt.sig`, reject if missing or not exactly 64
   bytes.
2. `verify_strict` the signature over the raw bytes of
   `SHA256SUMS.txt` using the embedded public key. Reject on failure.
3. Look up the hash of the downloaded `.zip` in
   `SHA256SUMS.txt` and reject on mismatch.
4. Extract and stage the new `.exe`.

Step 2 is what makes this chain authenticity-checking rather than just
integrity-checking. Without it, a release-repo writer (leaked PAT, CI
compromise, malicious insider) can publish a coherent (zip, sums)
pair and every installed LockNote will accept it.
