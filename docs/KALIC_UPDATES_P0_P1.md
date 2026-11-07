# kalic-updates — P0/P1 implementation notes

This branch applies the safety and release priorities selected after the
SF Companion/SF Commander comparison. It does not guess undocumented game
protocol or system-mail codes.

## P0 safety boundary

- `src/safety.rs` is the only application path allowed to send a mutating game
  command. It checks a process-wide fail-closed Kill Switch immediately before
  the send.
- The Kill Switch starts active. Manual actions and **Do it now** do not bypass
  it.
- Every write gets a durable JSONL audit entry before execution. A write is
  classified as blocked, verified failure, inconclusive, or verified success;
  payloads and credentials are never recorded.
- A transport failure is ambiguous because the server may have applied the
  command. It activates the Kill Switch and is not automatically replayed.
- Feature handlers validate the returned state before recording success for
  direct arena, underworld and fortress actions. Ordinary typed commands require
  a valid server response and successful `GameState` update.
- Remembered `PWHash` values are stored in Windows Credential Manager. Legacy
  values are migrated from `helper.toml`; new TOML serialization omits them.

The update handover remains a separate, short-lived boundary. It necessarily
contains live session exports so a running login can survive an update. The file
uses a random application-owned name, is accepted only beside the executable,
and is removed by the receiving process or by the sender on timeout.

## P1 operations and evidence

- The Activity panel exports the redacted durable action journal as CSV.
- `.github/workflows/windows-ci.yml` builds on Windows, runs tests and Clippy,
  creates the portable executable and NSIS installer, silently installs the
  package into an isolated directory, and runs an offline packaged smoke test.
- `scripts/check-release-version.ps1` prevents a mismatched `vX.Y.Z` tag from
  producing artifacts.
- Existing crash-safe settings writes and `.bak` recovery stay in place and are
  covered by the Windows test job.
- Guild duties no longer manufacture absences. The current diary is explicitly
  labelled a local scheduling record, not proof of combat participation.

## Evidence-gated item

Authoritative attack, defense, portal and hydra attendance cannot be implemented
from screenshots or assumptions. It requires sanitized raw system-mail header
and detail fixtures for every relevant message type. Until those fixtures are
available, the UI must show that participation evidence as unknown rather than
claiming that a signup proves combat participation.

## Windows verification

Local Rust linking requires MSVC Build Tools, which are intentionally not
installed on the current machine. Syntax/metadata checks can run locally; the
branch's GitHub Windows workflow is the authoritative compile, test, package and
installer smoke-test environment. After it passes, install the generated setup
artifact and verify login migration, Kill Switch behavior, one confirmed manual
write, one simulated/disconnected inconclusive write, CSV export and restart.
