# AutoQuill Rust Port — Consolidated Execution Plan

Status: Phase 2B Windows foreground reliability verified; Sticky Auto remains a separate package
Last updated: 2026-08-22
Primary roadmap: [`../BUILD_PLAN.md`](../BUILD_PLAN.md)

This document is the day-to-day build sequence for AutoQuill. It consolidates the six remaining
roadmap phases into four delivery phases so working product slices arrive sooner without removing
verification gates.

## 1. Faster delivery strategy

The original plan separated portable logic, a temporary parity UI, Windows integration,
cross-platform integration, final UI redesign, and packaging. That was safe but repeated too much
UI work. The revised sequence is:

| Phase | Outcome | Consolidates |
|---|---|---|
| 1. Portable product alpha | Tested core, profiles, and modern Jivaro UI using safe simulation | Old Phases 1, 2, and most of 5 |
| 2. Windows beta | Real Windows input, shortcuts, sticky targeting, and portable EXE | Old Phase 3 plus Windows product polish |
| 3. macOS and Linux expansion | Native platform backends, permissions, and artifacts | Old Phase 4 plus platform onboarding |
| 4. Release hardening | Final accessibility, packaging, updates, diagnostics, and release automation | Remaining old Phases 5 and 6 |

This removes the throwaway plain UI. The final Jivaro components are built incrementally alongside
working features. Internal work packages remain small enough to test and commit independently.

## 2. Delivery rules

- Keep one Cargo package and one raw executable per operating system and architecture.
- Keep UI, portable domain logic, and operating-system integrations in separate modules.
- Never test real keyboard injection in the default automated test suite.
- Complete portable parity with a fake input backend before enabling native injection.
- Add dependencies only when needed, disable unused default features, and measure size changes.
- Treat Windows, macOS, X11, and Wayland as separate capability backends.
- A work package is complete only when its focused tests and the repository quality checks pass.
- Build directly toward the final Jivaro interaction model; do not create a second disposable UI.
- Keep `~/Jivaro/AutoQuill` as the canonical user-data root and `Data/Saves` as the profile folder
  on Windows, macOS, and Linux.
- Show legacy profile files in the profile manager without automatically converting, moving, or
  rewriting them. Loading and upgrading always require explicit user actions.
- After each completed phase gate, refresh `dist/AutoQuill-windows-x64.exe`, update its SHA-256
  checksum and note, commit to `main`, and push from the normal host user context.
- Do not publish a broken or partially verified executable merely to keep `/dist` current.

## 3. Planned source layout

```text
src/
├── main.rs
├── app.rs
├── domain/
│   ├── settings.rs
│   ├── profile.rs
│   ├── session.rs
│   ├── shortcut.rs
│   └── warning.rs
├── typing/
│   ├── instruction.rs
│   ├── templating.rs
│   ├── tokenizer.rs
│   ├── scheduler.rs
│   ├── timing.rs
│   ├── engine.rs
│   └── fake_backend.rs
├── platform/
│   ├── capabilities.rs
│   ├── windows/
│   ├── macos/
│   └── linux/
├── persistence/
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

Create modules only as their work package starts. “Single executable” applies to the artifact, not
to the number of source files.

## 4. Core contracts

### Input backend

The portable engine owns no Slint or operating-system objects. It receives a backend with logical
operations equivalent to:

```text
capabilities() -> CapabilityReport
prepare_target(TargetRequest) -> PreparedTarget
validate_target(PreparedTarget) -> TargetState
emit_character(char) -> Result
emit_special_key(SpecialKey) -> Result
detach_target()
```

The Phase 1 backend records operations in memory and drives an internal simulation preview. Native
backends return typed failures and never silently claim success after dropping input.

### Time, randomness, and runtime data

Inject these dependencies:

- Monotonic time and interruptible waits.
- Local date and time.
- Clipboard text.
- Random integer, float, and typo-character selection.

Production adapters use platform services. Tests use manual time and seeded randomness, so core
tests never wait on wall-clock time and never produce nondeterministic output.

### Session model

Commands:

```text
Start(snapshot)
Pause(session_id)
Resume(session_id)
Stop(session_id)
Reset
Shutdown
```

States:

```text
Idle -> Preparing -> Countdown -> Typing <-> Paused -> Stopping -> Idle
                                       \-> Completed -> Idle
                                       \-> Failed -> Idle
```

Events cover state, progress, target, warning, completion, and failure. Every event carries a
session identifier so stale worker output cannot mutate a newer session.

## 5. Phase 1 — Portable product alpha

Goal: deliver a polished, safe, useful application that exercises all portable behavior using a
clearly labelled internal simulation instead of external keystrokes.

### 1.1 Typed settings and validation

Status: Completed on 2026-08-21

Deliverables:

- Add typed settings, profile, target-mode, shortcut, warning, session-state, and error models.
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

- Port warnings for high WPM, very short stop-after, reversed ranges, empty loop text, and sticky
  targeting.
- Keep validation out of Slint callbacks.

Tests:

- Defaults, minimums, maximums, invalid input, and reversed ranges.
- WPM-to-delay conversion.
- Legacy target values and warning rules.

Gate: settings round-trip through typed Rust values without a UI or JSON file.

### 1.2 Runtime variables and instruction compiler

Status: Implemented with the first interactive safe-simulation UI on 2026-08-21. The broader
Python fixture suite remains part of the Phase 1 verification gate.

Deliverables:

- Add `Instruction::Character(char)` and `Instruction::SpecialKey(SpecialKey)`.
- Port `{CLIPBOARD}`, `{DATE}`, and `{TIME}` with injectable providers.
- Preserve `""{TOKEN}""` and `""[NAME]""` literal escapes.
- Port `[NAME]` and `[NAME*COUNT]`.
- Normalize CRLF, CR, and LF to Enter.
- Preserve existing key names and aliases: Enter, Tab, Backspace, Space, Escape, modifiers,
  locks, navigation, arrows, Windows keys, Apps, F1–F12, and keypad keys.
- Keep unknown placeholders and key-like text literal.
- Count intended character instructions for progress, excluding special keys and simulated errors.

Tests:

- ASCII, Unicode, combining characters, non-Latin text, and emoji.
- Known, unknown, malformed, repeated, and escaped tokens.
- Clipboard/date/time values supplied by fakes.
- Fixture parity with the Python compiler.

Gate: Rust and Python fixtures produce the same logical instruction stream.

### 1.3 Deterministic scheduling and session engine

Status: Completed on 2026-08-21, including the advanced safe-simulation controls. Native input
remains intentionally disabled until Phase 2.

Deliverables:

- Port WPM delay and 0.8–1.2 token variance.
- Port word-unit breaks, short pauses, simulated errors, and matching Backspaces.
- Port break/pause compensation with its 5 ms minimum delay.
- Port loops, loop waits, and “loop wins” scheduler reset behavior.
- Implement the explicit session state machine and one-worker rule.
- Exclude paused time from stop-after accounting.
- Check cancellation during every countdown, delay, pause, break, error, and loop wait.
- Target less than 100 ms from Stop to no more output.
- Add an in-memory backend and manual clock.

Implementation note: the engine emits an in-memory `PreviewOperation` stream. The Slint adapter
renders that stream without exposing any operating-system input API, and tests advance it with
manual elapsed durations and seeded randomness.

Tests:

- Seeded operation timelines.
- Break boundary and special-key unit behavior.
- Pause, resume, stop-after, loop reset, and cancellation at every wait boundary.
- No progress increase for simulated typo characters.
- One terminal failure event and no later output after backend failure.

Gate: full sessions complete deterministically under manual time without external keystrokes.

### 1.4 Profiles and easy manual import

Status: Completed on 2026-08-21. Profiles use schema version 2, retain legacy files until an
explicit backed-up upgrade, and support multi-file or non-recursive folder import.

Deliverables:

- Define profile schema version 2 with all v0.13 fields and metadata.
- Keep app preferences separate from profiles.
- Use `~/Jivaro/AutoQuill` as the data root on every platform. On Windows this resolves to
  `C:\Users\<username>\Jivaro\AutoQuill`.
- Continue using `~/Jivaro/AutoQuill/Data/Saves` for profiles.
- Discover existing JSON files in place and show them in the normal profile list with a Legacy
  badge when they lack the current schema version.
- Load a legacy profile only when the user selects it and presses Load.
- Prompt before upgrading a legacy profile on Save, then retain a backup of its original bytes.
- Provide Import for selecting additional profile files from elsewhere, with conflict preview and
  overwrite/keep-both choices.
- Never auto-convert, modify, move, or delete Python profiles.
- Support list, save, load, rename, duplicate, delete, search, default, import, and export.
- Validate names and prevent traversal or reserved-name problems.
- Save atomically through a temporary sibling file and replace.

Tests:

- Schema round-trip and representative v0.13 fixtures.
- Partial, malformed, conflicting, and future-field files.
- Atomic-save failure preserves the previous file.
- Profile paths cannot escape the profile directory.
- Import leaves original Python files byte-for-byte unchanged.

Gate: users can deliberately import legacy profiles in a few clear steps without automatic data
changes.

### 1.5 Modern Jivaro application UI

Status: Completed for the portable simulation product on 2026-08-21. Platform-specific native
typing, global shortcut registration, and permission onboarding remain in Phases 2 and 3.

Build the actual product interface now instead of a temporary parity screen:

- Real multiline editor, character/word counts, token insertion menu, and clear action.
- WPM slider and numeric input.
- Shortcut recorder model and displayed shortcut; native registration waits for Phase 2.
- Plain-language target intent and startup delay.
- Persistent session bar with Start, Pause/Resume, Stop, Reset, state, progress, and ETA.
- Collapsible advanced controls for stop-after, loops, breaks, pauses, and errors.
- Contextual validation beside the relevant control.
- Profile quick switch and complete profile manager.
- Visible “Simulation mode — no external keystrokes” status.
- Internal output preview showing what the fake backend would type.
- Responsive minimum width, logical focus order, accessible names, and visible focus states.

UI boundaries:

- Slint sends typed commands and renders typed events.
- UI callbacks contain no parser, scheduler, persistence, or platform logic.
- Worker events return through the Slint event loop and never mutate UI objects directly.

Gate: every portable v0.13 feature can be configured, saved, simulated, paused, resumed, stopped,
reset, and inspected through the modern UI.

### 1.6 Phase 1 verification and artifact

Status: Automated Phase 1 gate completed on 2026-08-21. Local Windows smoke/size checks and native
Windows, macOS, and Ubuntu CI passed; `/dist` was refreshed. Manual visual scaling/usability passes
remain tracked for release hardening.

- Run core parity, migration, session scenario, UI-state, and smoke tests.
- Compile on native Windows, macOS, and Linux CI runners.
- Test the window at 100%, 125%, 150%, and 200% scaling.
- Record Windows raw size and warm startup time.
- Refresh `/dist` only after the optimized Windows simulation build passes.

Phase 1 gate: a first-time user can paste text, choose speed, simulate a full run, manage profiles,
and understand that no external input has occurred.

## 6. Phase 2 — Windows feature-complete beta

Goal: replace simulation with verified Windows input while keeping Simulation available as a safe
test mode.

### 2.1 Native foundation

Status: Implemented on 2026-08-21 for strict Windows foreground typing.

- Use narrowly enabled Microsoft `windows-sys` bindings.
- Add RAII wrappers for HWND/process handles, attached thread input, and timer resolution.
- Keep Win32 constants and virtual keys inside `cfg(windows)` modules.

### 2.2 Foreground input

Status: Implemented on 2026-08-21. Simulation remains the default every launch; Real Typing needs
explicit per-launch confirmation and a mandatory two-second countdown.

- Port `SendInput` Unicode, including UTF-16 surrogate pairs.
- Port special-key down/up pairs and extended-key flags.
- Return detailed errors and stop after the first terminal input failure.

### 2.3 Sticky Auto targeting

Status: Deferred to the next package after the foreground reliability matrix.

- Capture focused child HWND and root window at Start.
- Record title, class, process, and browser-like classification.
- Use `PostMessageW` only for verified classic controls.
- Promote browser/custom controls to foreground assist.
- Stop when browser focus is lost; never repeatedly steal focus.
- Validate targets before every operation and explain closure/replacement.

### 2.4 Global shortcuts and Windows product integration

Status: Bare F1–F12 Start/Stop registration, transactional conflict fallback, repeat/restart
suppression, and unsigned portable packaging are implemented. Modifier shortcuts, tray integration,
and signing remain deferred.

- Register modifier shortcuts through native Win32 APIs first.
- Preserve F1–F12 behavior.
- Evaluate an active-session-only hook for Escape or unsupported shortcuts.
- Report shortcut conflicts and keep visible UI controls available.
- Add Windows permission/capability messaging, tray actions where reliable, icon, version metadata,
  and portable artifact naming.

### 2.5 Windows matrix

Status: Foreground reliability package completed on 2026-08-22. Notepad, Edge, Chrome, controlled
Unicode input, focus loss, target closure, F1–F12 registration inventory, repeat suppression,
transactional rebind conflicts, and Stop latency passed. Firefox was not installed. VS Code's
isolated clean-profile top-level safety passed, but its child-editor focus automation remains
non-gating and documented in `PHASE2B_WINDOWS_RELIABILITY.md`.

- Real-input tests are explicit/manual and never part of default `cargo test`.
- Test Notepad, VS Code, Chrome, Edge, Firefox, and common form fields.
- Cover ASCII, non-Latin text, emoji, special keys, long text, focus loss, closed targets,
  pause/resume, Escape, loops, and Stop latency.
- Compare against Python v0.13 before declaring parity.

Phase 2 gate: Windows passes the parity matrix, the raw executable remains within the size target,
and the verified beta replaces `/dist`.

## 7. Phase 3 — macOS and Linux expansion

Each backend has an independent gate. The UI advertises only proven capabilities.

### 3A. macOS

- Build Apple Silicon and Intel separately before Universal 2.
- Implement foreground characters and keys through supported Core Graphics APIs.
- Detect Accessibility/Input Monitoring permission state with instructions and Re-check.
- Implement signed-app-compatible shortcuts.
- Investigate Accessibility target delivery as optional; do not block foreground typing on it.
- Test TextEdit, Notes, Safari, Chrome, Firefox, Unicode, emoji, permission denial/revocation,
  sleep/wake, and shortcut conflicts.

Gate: both architectures complete real sessions and permission failures never show false success.

### 3B. Linux X11

- Detect the display session at runtime.
- Prototype maintained X11 input and global shortcuts behind platform traits.
- Keep background targeting experimental until application-specific tests pass.
- Test Ubuntu GNOME X11 and KDE Plasma X11 with native editors and browsers.

Gate: foreground typing and shortcuts work across the tested X11 desktops with useful diagnostics.

### 3C. Linux Wayland

- Use the desktop Global Shortcuts portal when available.
- Prototype Remote Desktop portal/libei input with explicit consent.
- Isolate experimental Wayland/libei support behind Cargo features until reliable.
- Never promise arbitrary background targeting.
- Provide visible controls and copy-to-clipboard guidance if required portals are unavailable.
- Test Ubuntu GNOME Wayland and KDE Plasma Wayland independently.

Gate: supported portals type after consent; unsupported environments explain limitations before
Start.

### Dependency checkpoint

- `global-hotkey` is a macOS/X11 candidate, not a Wayland solution. Prove its event-loop behavior
  with Slint before adoption.
- `enigo` is a prototype candidate for macOS/X11. Its Wayland/libei paths remain experimental, so
  the portable engine never depends on it directly.
- `ashpd` is the preferred portal-client candidate for Wayland Global Shortcuts and Remote Desktop.
- Windows remains native because AutoQuill needs behavior beyond generic input libraries.
- Pin selected versions and features only after size, license, event-loop, and behavior spikes.

Primary references:

- [`global-hotkey` platform and event-loop notes](https://docs.rs/global-hotkey/latest/global_hotkey/)
- [`enigo` platforms and experimental Wayland/libei notice](https://docs.rs/crate/enigo/latest)
- [`ashpd` Global Shortcuts](https://docs.rs/ashpd/latest/ashpd/desktop/global_shortcuts/)
- [`ashpd` Remote Desktop](https://docs.rs/ashpd/latest/ashpd/desktop/remote_desktop/struct.RemoteDesktop.html)
- [Microsoft `windows-rs`](https://github.com/microsoft/windows-rs)

Phase 3 gate: every published platform completes onboarding and a real session, with unsupported
capabilities stated before Start.

## 8. Phase 4 — Release hardening

Goal: turn proven platform builds into repeatable, trustworthy releases.

### Final product pass

- Finish Dark, Light, and System themes.
- Complete keyboard-only navigation, screen-reader smoke tests, contrast, scaling, and reduced
  motion.
- Finish tray behavior per platform.
- Run task-based usability passes for first-time, occasional, and power users.
- Resolve permission, target-loss, shortcut-conflict, profile-conflict, and dirty-state recovery.

### Artifacts

- Windows raw portable EXE plus optional installer.
- macOS `.app` and DMG; Apple Silicon/Intel and Universal 2 when proven.
- Linux raw x64 executable and AppImage.
- Generate SHA-256 checksums, artifact manifest, dependency licenses, and size report.

### Signing and trust

- Add Windows code signing when credentials exist.
- Add macOS Developer ID signing, hardened runtime, and notarization when credentials exist.
- Document unsigned development builds without presenting them as production releases.
- Run clean-machine and antivirus false-positive checks.

### Updates and diagnostics

- Check GitHub Releases asynchronously.
- Compare semantic versions and show release notes.
- Require user action to download; never silently replace the executable.
- Add bounded local diagnostics with Copy and Export.
- Never log typed text, clipboard data, or injected characters.

### Release automation

1. Format, strict Clippy, unit tests, scenario tests, and platform contract tests.
2. Compile platform/architecture targets on native runners.
3. Run platform smoke tests.
4. Build one optimized renderer/backend combination per artifact.
5. Measure size and compare to thresholds.
6. Generate checksums, manifests, notices, and bundles.
7. Sign/notarize when credentials exist.
8. Publish alpha/beta/stable only from a reviewed version tag.

Phase 4 gate: every advertised artifact passes clean-machine startup, permissions, typing,
migration, upgrade, and uninstall checks.

## 9. Quality gates

Run for each work package:

```text
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo build --profile release-size --locked
```

Also:

- Run the hidden `AUTOQUILL_SMOKE_TEST=1` check.
- Run `git diff --check` before committing.
- Audit `cargo tree -e features` when dependencies change.
- Record executable bytes at each phase gate.
- Verify `/dist` hashes against the just-built artifact.
- Confirm local and remote `main` match after an authorized push.
- If a dependency adds more than 1 MiB to the Windows raw executable, justify or replace it.

## 10. Immediate implementation order

The next commits remain small even though the delivery phases are consolidated:

- [x] `Add typed settings and validation models`
- [x] `Port runtime variables and instruction tokenizer`
- [x] `Port deterministic scheduler and timing model`
- [x] `Add session state machine and fake input backend`
- [x] `Add profile storage and deliberate legacy import`
- [x] `Build reusable Jivaro controls and functional editor`
- [x] `Connect settings, profiles, and simulation to the UI`
- [x] `Complete Phase 1 verification and refresh dist`
- [x] `Add strict Windows foreground capture and Unicode native input`
- [x] `Add bare F1-F12 global Start/Stop with conflict handling`
- [x] `Add Simulation/Real Typing mode, per-launch consent, and 2-second countdown`
- [x] `Publish the verified Phase 2A Windows foreground beta to dist`
- [x] `Complete the Windows foreground reliability/input matrix`
- [x] `Publish the verified Phase 2B Windows reliability beta to dist`
- [ ] `Implement and verify Sticky Auto targeting`
- [ ] `Complete the Phase 2 Windows gate and refresh dist`

Real Windows injection began only after the Phase 1 gate passed. Sticky Auto remains required before
Phase 2 can be declared feature-complete.

## 11. Plan maintenance

- Mark work packages complete only after their gate passes.
- Record product or architecture changes in the `BUILD_PLAN.md` decision log.
- Update capability copy and tests in the same change when a platform differs.
- Keep failed experiments out of default features and `/dist`.
- Push coherent, verified commits to `main` from the normal host user context.
