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

## Rules for the editor window (src/ui/editor)

These come from bugs found in 2.1 (issues #22 to #31); the tests enforce most
of them.

- **Lock rule** (`lock_flow.rs`, SPEC "Auto-lock"): a due lock hides the note
  at once under a curtain that only the password lifts; open dialogs get
  their harmless answer; the wipe happens only with no window open and after
  a successful save. Do not add a path that wipes, or acts on a dialog
  answer, outside this flow.
- **Questions and messages** of the main window go through `confirm()` and
  `inform()`, never `nwg::message` / `modal_*_message` / `MessageBoxW`
  directly.
- **A dialog result** that changes the note is applied only if
  `still_valid(epoch)` is true (no lock happened while the dialog was open).
- **Never hold a `RefCell` borrow** of the password, tabs, attachments,
  settings or panels across a dialog: copy what you need (`Zeroizing` for
  secrets) and drop the borrow. A dialog runs its own message loop, and a
  lock or the end of the Windows session can run inside it.
- **Release builds use `panic = "abort"`**: a panic kills LockNote with the
  unsaved text. No `unwrap` on user input; slice strings on bytes or with
  `get(..)`.
- **Tests that read the code as text** use `crate::read_source`, which turns
  CRLF into LF: the CI runner checks out with `core.autocrlf=true`. Keep
  them for wiring only (the line of the real window that feeds a flow or a
  decision, which a test with a fake cannot see) and for rules over the
  whole editor; what a flow does once called is tested through its trait.
  Before removing one, check that the test replacing it fails on the
  mutant the text check caught.
- **Test the sequence, not only the decision**: a pure decision function can
  be right while the window chains it wrongly. Flows with several steps get a
  trait (`LockSteps`, `CloseSteps`, ...) and sequence tests with a fake.

## Commits and pull requests

- One topic per pull request, with a description of what changes for the
  user and how it was tested.
- Commit messages follow Conventional Commits (`feat:`, `fix:`, `docs:`,
  `refactor:`, `test:`, `ci:`).
- CI (`.github/workflows/ci.yml`) must be green.
