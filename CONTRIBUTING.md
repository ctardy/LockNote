# Contributing to LockNote

Thank you for your interest. LockNote holds people's secrets: changes are
reviewed for correctness first, and a change that could lose or expose notes
is not merged without tests that prove it does not.

## Before you start

- Bugs and ideas: open an issue (templates in `.github/ISSUE_TEMPLATE`).
- Security issues: **never** in a public issue, see [SECURITY.md](SECURITY.md).
- Contributions are accepted under the project license, GPL-3.0-or-later.

## Build and test

See the README: `scripts\build.cmd` and `scripts\test.cmd`. `test.cmd`
redirects `LOCALAPPDATA`, `TEMP` and `TMP` to a throwaway folder; if you run
`cargo test` by hand, do the same. Never test a build on an exe that holds
real notes.

## Rules for changes

- **Test first.** A bug fix comes with a test that fails without it; a new
  feature with tests written from its specification.
- **No silent fallback.** Do not catch an error to return an empty value or a
  default: fail explicitly, with a message that names the file or the field.
- **Data format.** The payload of the exe and the `.lnbak` full backup must
  stay readable forever. Any change to what is stored goes through
  `src/features/backup` with its tests (`roundtrip_all_blocks`,
  `every_settings_field_has_a_decision`, ...).
- **Frozen fixtures** (`src/features/backup/fixtures/`) are written once and never
  regenerated or edited. If a frozen test fails, fix the reader.
- Keep the style of the surrounding code; comments say why, not what.
- Update `docs/SPEC.md` and `CHANGELOG.md` with the behaviour you change.

## Commits and pull requests

- One topic per pull request, with a description of what changes for the
  user and how it was tested.
- Commit messages follow Conventional Commits (`feat:`, `fix:`, `docs:`,
  `refactor:`, `test:`, `ci:`).
- CI (`.github/workflows/ci.yml`) must be green.
