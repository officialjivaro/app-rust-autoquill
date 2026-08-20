# AutoQuill Rust Port — Execution Plan

Status: Ready for Phase 1 implementation
Last updated: 2026-08-20
Primary roadmap: [`../BUILD_PLAN.md`](../BUILD_PLAN.md)

This document converts the product roadmap into buildable work packages. It is the day-to-day
implementation sequence; `BUILD_PLAN.md` remains the source of truth for product behavior, UX, and
release acceptance.

## 1. Delivery rules

- Keep one Cargo package and one raw executable per operating system and architecture.
- Keep UI, portable domain logic, and operating-system integrations in separate modules.
- Never test real keyboard injection in the default automated test suite.
- Complete portable parity with a fake input backend before enabling native injection.
- Add dependencies only in the phase that needs them, disable unused default features, and record
  the release-size change.
- Treat Windows, macOS, X11, and Wayland as separate capability backends rather than pretending
  they behave identically.
- A work package is complete only when formatting, strict Clippy, unit tests, the optimized build,
  and the hidden smoke test pass.
- After each completed phase gate, refresh `dist/AutoQuill-windows-x64.exe`, update its SHA-256
  checksum and distribution note, commit the verified source and artifact to `main`, and push from
  the normal host user context.
- Do not publish a broken or partially verified executable merely to keep `/dist` current.

## 2. Planned source layout

```text
src/
├── main.rs
├── app.rs
├── domain/
│   ├── mod.rs
│   ├── settings.rs
│   ├── profile.rs
│   ├── session.rs
│   ├── shortcut.rs
│   └── warning.rs
├── typing/
│   ├── mod.rs
│   ├── instruction.rs
│   ├── templating.rs
│   ├── tokenizer.rs
│   ├── scheduler.rs
│   ├── timing.rs
│   ├── engine.rs
│   └── fake_backend.rs
├── platform/
│   ├── mod.rs
│   ├── capabilities.rs
│   ├── windows/
│   ├── macos/
│   └── linux/
├── persistence/
│   ├── mod.rs
│   ├── profile_store.rs
│   ├── preferences.rs
│   └── legacy_import.rs
├── updates/
└── diagnostics/

ui/
├── app-window.slint
├── theme.slint
├── components/
├── dialogs/
└── pages/

tests/
├── fixtures/
├── core_parity.rs
├── profile_migration.rs
└── session_scenarios.rs
```

Not every file must be created immediately. Add modules as their work package begins so the
project stays understandable throughout the port.

## 3. Core contracts to establish first

### Input backend

The portable engine receives an injected backend with operations equivalent to:

```text
capabilities() -> CapabilityReport
prepare_target(TargetRequest) -> PreparedTarget
validate_target(PreparedTarget) -> TargetState
emit_character(char) -> Result
emit_special_key(SpecialKey) -> Result
detach_target()
```

The fake backend records operations in memory. Native backends must return typed failures; they
must never silently report success after dropping input.

### Time and randomness

The engine receives abstractions for:

- Monotonic time.
- Interruptible waits.
- Current local date and time for runtime variables.
- Random integer, float, and typo-character selection.
- Clipboard text.

Production adapters use the operating system. Tests use manual time and seeded randomness, so no
test relies on wall-clock sleeps or nondeterministic output.

### Session commands and events

Commands:

```text
Start(snapshot)
Pause(session_id)
Resume(session_id)
Stop(session_id)
Reset
Shutdown
```

Events:

```text
Preparing
Countdown
Started
Progress
Paused
Resumed
TargetChanged
Warning
Completed
Stopped
Failed
```

Every event carries a session identifier. The UI ignores events belonging to an older session.

## 4. Phase 1 — Portable core parity

Goal: reproduce AutoQuill v0.13 behavior without Slint or real operating-system input.

### 1.1 Domain models and validation

Deliverables:

- Create typed settings, profile, target-mode, shortcut, warning, session-state, and error models.
- Preserve these v0.13 defaults:

| Setting | Default | Validation |
|---|---:|---|
| WPM | 60 | Clamp 1–200; five characters per word |
| Shortcut | F1 | Preserve F1–F12; model modifier combinations |
| Sticky typing | Off | Legacy sticky values normalize to portable target intent |
| Startup delay | Off | Existing enabled value equals two seconds |
| Stop after | Off / 60 s | Clamp enabled duration to 1–86,400 s |
| Loop wait | Off / 5–10 s | Normalize reversed range; clamp 1–86,400 s |
| Error interval | 15–40 tokens | Normalize reversed range; minimum 1 |
| Error count | 1–4 | Normalize reversed range; minimum 1 |
| Break interval | 18–42 word units | Normalize reversed range; cap 500 |
| Break duration | 2.0–5.0 s | Normalize reversed range; clamp 0–60 s |
| Short pause interval | 120–250 characters | Normalize reversed range; minimum 1 |
| Short pause duration | 0.6–1.8 s | Normalize reversed range; minimum 0 |

- Port the current warning rules for very high WPM, short stop-after values, reversed ranges,
  empty looping text, and sticky-target behavior.
- Ensure settings validation exists independently of UI controls.

Tests:

- Every default and boundary value.
- Invalid numeric input falls back safely.
- Reversed ranges normalize in one documented direction.
- Legacy target-mode values map to the intended current behavior.
- Warning output is stable and human-readable.

Gate: settings can round-trip through typed Rust values without a UI or JSON file.

### 1.2 Runtime variables and instruction compiler

Deliverables:

- Add `Instruction::Character(char)` and `Instruction::SpecialKey(SpecialKey)`.
- Port `{CLIPBOARD}`, `{DATE}`, and `{TIME}` with injectable providers.
- Preserve literal runtime-token escaping: `""{TOKEN}""` emits `{TOKEN}`.
- Port `[NAME]` and `[NAME*COUNT]` special-key parsing.
- Preserve literal special-key escaping: `""[NAME]""` emits `[NAME]` as text.
- Normalize CRLF, CR, and LF to a single Enter instruction.
- Support the existing key names and aliases:
  Enter, Tab, Backspace, Space/Spacebar, Esc/Escape, Ctrl, Shift, Alt, Caps Lock, Num Lock,
  Scroll Lock, Pause, Insert, Delete, Print Screen, Home, End, Page Up, Page Down, arrows,
  Windows keys, Apps, F1–F12, and numeric keypad keys.
- Keep unknown placeholders and key-looking text literal.
- Count progress using intended character instructions only, matching v0.13.

Tests:

- Empty, ASCII, Unicode, combining characters, non-Latin text, and emoji.
- Known and unknown runtime variables.
- Escaped variables and special keys.
- Special-key repetition, aliases, malformed counts, and newline normalization.
- Clipboard/date/time values supplied by fakes.

Gate: Python and Rust fixture cases compile to the same logical instruction stream.

### 1.3 Scheduling, timing, and humanization

Deliverables:

- Port WPM-to-delay calculation and 0.8–1.2 per-token variance.
- Port word-unit break scheduling and the current punctuation boundaries.
- Port short pauses after randomized character counts.
- Port simulated errors: randomized characters followed by matching Backspace instructions.
- Port break/pause compensation with its 5 ms minimum effective delay.
- Port loop wait selection and “loop wins” break-scheduler reset behavior.
- Represent all waits as interruptible scheduled actions rather than direct `sleep` calls.

Tests:

- Seeded sequences produce exact expected delays and typo operations.
- Breaks occur only on word boundaries; special keys count as documented units.
- Error characters do not advance intended progress.
- Loop wait resets progress and break counters.
- Compensation matches Python fixtures within a documented floating-point tolerance.

Gate: a deterministic simulator produces the expected operation timeline for representative runs.

### 1.4 Session state machine and fake backend

Deliverables:

- Implement `Idle -> Preparing -> Countdown -> Typing <-> Paused -> Stopping` and terminal states.
- Guarantee one active worker and reject a second Start command safely.
- Exclude paused time from stop-after accounting.
- Check cancellation during countdown, per-token delay, simulated pauses, breaks, errors, and loop waits.
- Target a Stop-to-no-more-output latency below 100 ms.
- Publish typed events for progress, ETA inputs, state, warnings, target failure, and completion.
- Add an in-memory backend that records every character and key operation.

Tests:

- Table-driven command/state transitions.
- Stop and pause at every wait boundary.
- Stop-after while typing, paused, and between loops.
- Stale session events cannot mutate the current session.
- Backend failure produces one Failed event and no later output.

Gate: full sessions run to completion under manual time without emitting real keystrokes.

### 1.5 Versioned profiles and legacy import

Deliverables:

- Define profile schema version 2 with all v0.13 fields plus metadata.
- Keep app preferences separate from typing profiles.
- Use the platform data directory for new files.
- Detect `~/Jivaro/AutoQuill/Data/Saves` on Windows and offer non-destructive import.
- Preserve unknown legacy fields in an extension map when possible.
- Reject empty names, invalid filename characters, separators, traversal, reserved names, and
  collisions according to an explicit overwrite policy.
- Save by writing a temporary sibling file, flushing it, and atomically replacing the destination.
- Support list, save, load, rename, duplicate, delete, import, export, search, and default profile.

Tests:

- Round-trip every settings field.
- Import representative valid, partial, malformed, and future-field legacy JSON fixtures.
- Atomic save failure leaves the previous profile readable.
- Profile names cannot escape the profile directory.
- Import never modifies or deletes the Python source profile.

Gate: fixture copies of v0.13 profiles import and re-open as schema version 2 without losing
supported behavior.

### 1.6 Phase 1 application integration

Deliverables:

- Connect the existing Slint shell to the portable session controller and fake backend.
- Make the editor, Start, Pause/Resume, Stop, Reset, progress, status, and error states functional.
- Display a visible “Simulation — no external keystrokes” capability label.
- Keep all advanced settings available through programmatic models even if the final controls are
  not built until Phase 2.
- Add Windows/macOS/Linux CI jobs for core tests and optimized compile checks.
- Record release size and warm smoke timing after the new dependencies are linked.

Gate: a user can run, pause, resume, stop, reset, and complete a simulated session in the app;
automated tests demonstrate v0.13 core parity.

## 5. Phase 2 — Functional parity UI

Goal: expose every portable feature before native input and final visual polish.

### Work packages

1. Replace static preview elements with a real multiline editor, counts, token menu, and clear flow.
2. Add WPM slider/numeric input, shortcut recorder, target intent, and startup delay to the main
   flow.
3. Add a persistent session bar with state text, progress, typed/total count, ETA, active target,
   Start, Pause/Resume, Stop, and Reset.
4. Add collapsible advanced controls for stop-after, loops, breaks, pauses, and errors; show
   dependent fields only when enabled.
5. Add contextual validation and warnings beside the affected controls.
6. Add profile quick switch and the complete profile manager.
7. Add capability and permission surfaces driven by the platform report.
8. Add non-blocking update-check state using a fake transport first.
9. Add accessible names, keyboard focus order, logical tab navigation, and visible focus styling as
   components are created.

Verification:

- Each setting can be entered, saved, loaded, and included in a Start snapshot.
- Invalid input is explained and cannot start an unsafe or ambiguous session.
- UI callbacks do not contain domain validation or platform-specific code.
- The fake backend still remains the default while testing this phase.
- Window remains usable at the minimum supported size and 100%, 125%, 150%, and 200% scaling.

Gate: the Rust UI exposes every v0.13 user-facing behavior, excluding real input and documented
platform limitations.

## 6. Phase 3 — Windows backend parity

Goal: make the Windows build safe for beta use and retire no Python behavior prematurely.

### 3.1 Native API foundation

- Use narrowly enabled Microsoft `windows-sys` bindings for Win32 APIs.
- Add RAII wrappers for HWND/process handles, attached thread input, and timer resolution.
- Keep Win32 constants and virtual-key mapping inside the Windows backend.
- Add compile-time `cfg(windows)` boundaries so no Windows dependency enters macOS/Linux builds.

### 3.2 Foreground input

- Port `SendInput` Unicode injection, including characters requiring UTF-16 surrogate pairs.
- Port special-key down/up pairs and extended-key flags.
- Return detailed OS errors and stop the active session after the first terminal failure.

### 3.3 Sticky target behavior

- Capture the focused child HWND and root window at Start.
- Record window title, class, process identity, and browser-like classification.
- Use `PostMessageW` for verified classic native controls.
- Promote browser/custom controls to foreground assist when background messages are unreliable.
- Stop when a browser-assist target loses focus; never repeatedly steal focus.
- Detect closed/replaced targets before every emitted operation.

### 3.4 Global shortcuts

- Implement modifier shortcut registration with native Win32 registration first.
- Preserve F1–F12 behavior.
- Evaluate an active-session-only keyboard hook for Escape or unsupported combinations; do not
  install a broad permanent hook by default.
- Report registration conflicts and provide a visible fallback.

### 3.5 Verification matrix

- Automated backend contract tests use recorded Win32 call adapters where practical.
- Real-input tests run only under an explicit ignored/manual test target.
- Manual targets: Notepad, WordPad-equivalent native control, VS Code, Chrome, Edge, Firefox, and
  common single-line/multiline web fields.
- Cover ASCII, non-Latin text, emoji, all special-key aliases, long text, focus loss, target close,
  pause/resume, Escape, loop, and Stop latency.
- Compare behavior side by side with Python v0.13.

Gate: the Windows parity checklist passes, release size remains under the agreed target, and the
updated portable executable replaces `/dist` only after smoke and clean-machine checks.

## 7. Phase 4 — macOS and Linux backends

This phase has three independent gates. A passing backend can ship while another remains marked
limited; the UI must never claim unavailable capabilities.

### 4A. macOS

- Build native Apple Silicon and Intel artifacts independently before attempting Universal 2.
- Implement foreground character/key events through supported Core Graphics APIs.
- Detect Accessibility/Input Monitoring permission state and provide request, instructions, and a
  re-check action.
- Implement signed-app-compatible global shortcuts.
- Investigate target-specific Accessibility delivery as a separate capability; do not block
  foreground typing on it.
- Test TextEdit, Notes, Safari, Chrome, Firefox, common form fields, non-Latin text, emoji,
  permission denial/revocation, sleep/wake, and shortcut collision.

Gate: both architectures complete a real foreground session and handle denied permissions without
false success.

### 4B. Linux X11

- Detect the display session at runtime.
- Prototype a maintained X11 input implementation behind the platform trait.
- Prototype X11 global shortcuts separately.
- Treat background targeting as experimental until application-specific tests pass.
- Test Ubuntu GNOME X11 and KDE Plasma X11 with native editors and Chromium/Firefox fields.

Gate: foreground typing and shortcuts work on both tested X11 desktops with useful dependency and
session diagnostics.

### 4C. Linux Wayland

- Use the desktop Global Shortcuts portal when the compositor provides it.
- Prototype Remote Desktop portal/libei input with explicit user consent.
- Keep experimental Wayland/libei support isolated behind Cargo features until the compositor
  matrix is reliable.
- Never promise arbitrary background targeting.
- Provide visible Start/Stop controls and copy-to-clipboard guidance when the required portal is
  unavailable.
- Test Ubuntu GNOME Wayland and KDE Plasma Wayland independently.

Gate: supported portal sessions type successfully after consent; unsupported environments explain
the limitation before Start.

### Dependency decision checkpoint

- `global-hotkey` is a candidate for macOS and X11, but not Wayland; its event-loop constraints
  must be proven with Slint before adoption.
- `enigo` is a prototype candidate for macOS/X11 and optional Linux implementations. Its Wayland
  and libei paths remain experimental, so the portable engine must not depend on it directly.
- `ashpd` is the preferred portal client candidate for Wayland Global Shortcuts and Remote Desktop.
- Windows keeps a native backend because AutoQuill requires behavior beyond a generic input crate.
- Pin exact selected versions and features only after each spike passes size, license, event-loop,
  and behavior tests.

Current primary references for the dependency spikes:

- [`global-hotkey` platform support and event-loop notes](https://docs.rs/global-hotkey/latest/global_hotkey/)
- [`enigo` platform features and experimental Wayland/libei notice](https://docs.rs/crate/enigo/latest)
- [`ashpd` Global Shortcuts portal API](https://docs.rs/ashpd/latest/ashpd/desktop/global_shortcuts/)
- [`ashpd` Remote Desktop portal API](https://docs.rs/ashpd/latest/ashpd/desktop/remote_desktop/struct.RemoteDesktop.html)
- [Microsoft `windows-rs` repository and binding guidance](https://github.com/microsoft/windows-rs)

## 8. Phase 5 — Final Jivaro UX

Goal: turn the parity UI into the polished everyday product without hiding power features.

### Work packages

1. Expand `theme.slint` into semantic Dark, Light, and System token sets.
2. Build reusable field, toggle, slider, button, card, chip, banner, tooltip, modal, menu, progress,
   and focus-ring components.
3. Implement the final editor-first main window and responsive narrow layout.
4. Add first-run onboarding: explanation, permission check, shortcut choice, and internal safe
   typing test.
5. Refine progressive disclosure for advanced settings and contextual warnings.
6. Finish profile search, dirty state, duplicate, rename, import/export, delete confirmation, and
   default selection.
7. Add tray controls where the platform reports reliable support.
8. Add reduced motion, keyboard-only flows, accessible labels, screen-reader smoke checks, and
   contrast verification.
9. Run task-based usability passes for first-time, occasional, and power users.

Required usability scenarios:

- First launch to successful internal test without documentation.
- Paste text, change speed, select a target, and start.
- Pause and stop during a long wait.
- Recover from missing permission, target loss, and shortcut conflict.
- Save a profile, switch profiles, edit it, and resolve dirty state.
- Find every advanced v0.13 option without cluttering the default workflow.

Gate: a first-time user completes a successful run without documentation, and all critical flows
work with keyboard-only navigation at supported scaling levels.

## 9. Phase 6 — Packaging and release hardening

### 6.1 Artifact production

- Windows: raw portable EXE in `/dist`, version metadata/icon, optional installer as a separate
  artifact.
- macOS: `.app`, DMG, Apple Silicon and Intel builds, then Universal 2 if validated.
- Linux: raw x64 executable and AppImage.
- Generate SHA-256 checksums, an artifact manifest, dependency license inventory, and release-size
  report.

### 6.2 Signing and trust

- Add Windows code signing when credentials are available.
- Add macOS Developer ID signing, hardened runtime, and notarization when credentials are
  available.
- Document unsigned development behavior without presenting it as a production release.
- Run clean-machine and antivirus false-positive checks before stable publication.

### 6.3 Updates and diagnostics

- Check GitHub Releases asynchronously after startup.
- Compare semantic versions and expose release notes.
- Require a user action to download; never silently replace the running executable.
- Add bounded, crash-safe local diagnostics with Copy and Export actions.
- Do not log typed text, clipboard contents, or injected characters.

### 6.4 CI release flow

1. Format, strict Clippy, unit tests, and core scenario tests.
2. Compile all platform/architecture targets on native runners.
3. Run platform smoke tests.
4. Build optimized raw artifacts with one renderer per artifact.
5. Measure size and compare against the recorded threshold.
6. Generate checksums, manifests, notices, and bundles.
7. Sign/notarize when credentials exist.
8. Publish an alpha/beta release only from a reviewed version tag.

Gate: all advertised artifacts pass clean-machine startup, permission, typing, migration, upgrade,
and uninstall checks.

## 10. Quality gates used for every work package

Run from `rust_autoquill`:

```text
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo build --profile release-size --locked
```

Additionally:

- Run the hidden `AUTOQUILL_SMOKE_TEST=1` executable check.
- Run `git diff --check` before every commit.
- Audit `cargo tree -e features` when dependencies change.
- Record raw executable bytes after each phase gate.
- Verify the `/dist` executable hash matches the just-built release artifact.
- Confirm `main` and `origin/main` point to the same commit after an authorized push.

## 11. Immediate implementation order

The next commits should be narrowly scoped in this order:

1. `Add typed settings and validation models`
2. `Port runtime variables and instruction tokenizer`
3. `Port deterministic scheduler and timing model`
4. `Add session state machine and fake input backend`
5. `Add versioned profile storage and legacy importer`
6. `Connect simulated sessions to the Slint shell`
7. `Complete Phase 1 verification and refresh dist`

Do not begin Windows injection until item 7 passes. This preserves a testable portable foundation
for macOS and Linux instead of baking Windows assumptions into the engine.

## 12. Plan maintenance

- Mark a work package complete only after its gate passes.
- Record design changes in the `BUILD_PLAN.md` decision log.
- If platform behavior differs, update the capability matrix and UI copy in the same change.
- If a dependency adds more than 1 MiB to the Windows raw executable, record why it is justified or
  replace it.
- Keep failed experiments out of the default feature set and `/dist` artifact.
