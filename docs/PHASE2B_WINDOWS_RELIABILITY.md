# Phase 2B Windows foreground reliability verification

Date: 2026-08-22
Version: `0.14.0-beta.1`

## Delivered

- Hotkey rebinding is transactional. A failed new registration keeps the old F-key registered as
  an emergency Stop until another key is available.
- A retained fallback key cannot start an idle session when the selected key is unavailable.
- Queued or repeated activation events cannot restart a session for 300 milliseconds after Stop.
- Stale registration success/failure events cannot overwrite the status of a newer selected key.
- Focus change and target closure prevent every later native emission; the engine clears pending
  operations on Stop or backend failure.
- Simulation remains selected by default. Real Typing remains opt-in, per-launch confirmed,
  foreground-only, and protected by the mandatory two-second countdown.

## Local quality and safety gates

- `cargo fmt --all -- --check` passed.
- Strict all-target, all-feature Clippy passed with warnings denied.
- The ordinary suite passed with every real-input probe ignored by default.
- The controlled WinForms probe received `AutoQuill 日本🙂`, rejected emission after focus loss,
  and reported the closed target before any further input.
- The F-key inventory found F1–F11 available and F12 reserved by another installed program. The
  stress probe passed transactional conflict fallback, `MOD_NOREPEAT`, 24 rebind cycles, and a
  measured emergency-Stop event latency of 63.4 milliseconds.

## Installed-application matrix

Every probe uses a unique temporary document or isolated temporary profile. No existing user file,
browser profile, or VS Code profile is opened. The qualification workload runs at AutoQuill's
default 60 WPM and includes ASCII, Japanese, emoji, Enter, simulated typo correction/backspaces,
Tab, and sustained text.

| Application | Result | Evidence |
|---|---|---|
| Windows Notepad | Pass, release-gating | Saved temporary file matched the expected receiver content exactly. |
| Microsoft Edge | Pass | Isolated app-mode textarea reported the expected UTF-8 length and FNV-1a content hash. |
| Google Chrome | Pass | Isolated app-mode textarea reported the expected UTF-8 length and FNV-1a content hash. |
| Mozilla Firefox | Skipped | Firefox is not installed on the verification host; nothing was installed for the test. |
| Visual Studio Code | Documented harness limitation | Top-level foreground safety remained valid, but isolated clean-profile child-editor focus was inconsistent. This is non-gating and does not authorize background targeting. |

VS Code did receive the exact workload in one diagnostic run (with its normal Tab-to-indentation
semantics), but repeated isolated-profile launches did not focus the child editor deterministically.
The result is therefore recorded as a limitation rather than presented as a stable pass.

## Platform and feature boundary

- Sticky/background targeting remains deferred to the next Windows package.
- Native input and global shortcuts on macOS and Linux remain unimplemented and unverified.
- The executable remains portable and unsigned; code signing is future release-hardening work.

## Release evidence

- Source commit: pending.
- GitHub Actions: pending.
- Windows x64 executable size: pending.
- SHA-256: pending.
- Cleanup result: pending.
