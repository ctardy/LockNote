# Releasing LockNote

A release is a `vX.Y.Z` tag pushed to `ctardy/LockNote`. The workflow
`.github/workflows/release.yml` builds, runs the tests, signs the exe
(Authenticode, Azure Trusted Signing), writes `SHA256SUMS.txt` with its
Ed25519 signature and publishes the GitHub release with
`LockNote-windows-x64.zip`. Each run costs build minutes and a signature.

## Before the tag

1. `main` is up to date and its last CI run is green.
2. Version: `Cargo.toml` `version` (the exe version, the About box and the
   updater all read it); `Cargo.lock` follows at the next build. Semver: a
   new feature bumps the minor version, fixes only bump the patch.
3. `CHANGELOG.md`: rename `## [Unreleased]` to `## [X.Y.Z] - YYYY-MM-DD` and
   start a new empty `## [Unreleased]`.
4. Version named in `docs/en/user-guide.md` and
   `docs/fr/guide-utilisateur.md` (title and introduction).
5. Full test suite green locally with `LOCALAPPDATA`, `TEMP` and `TMP`
   redirected (`scripts\test.cmd`).
6. Running copies: as soon as the release is "Latest", every open LockNote
   offers it. Save before accepting the update.

## Tag

```cmd
git tag -a vX.Y.Z -m "LockNote X.Y.Z"
git push origin vX.Y.Z
```

## After

- `gh run watch` on the release run, then `gh release view vX.Y.Z`: the
  three assets `LockNote-windows-x64.zip`, `SHA256SUMS.txt`,
  `SHA256SUMS.txt.sig` are there. A tag without a release means the run
  failed.
- Check the downloaded `SHA256SUMS.txt.sig` against `RELEASE_PUBLIC_KEY` and
  the Authenticode signature of `LockNote.exe` (`Get-AuthenticodeSignature`).
- A failed run: fix, then `gh run rerun <id> --failed` (the tag stays).
  A run failing in seconds with "payments have failed" is GitHub billing.

## Rolling back

The updater only offers the latest release. If a published release is bad,
mark it as a pre-release (`gh release edit vX.Y.Z --prerelease`) so the
previous one becomes "Latest" again, then publish a fixed `X.Y.Z+1`. Never
move or reuse a published tag.
