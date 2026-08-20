# AutoQuill Rust Port — Build Plan

Status: Approved for implementation planning
Source application: `python_autoquill` v0.13
Target application: `rust_autoquill`
Primary stack: Rust + Slint

This document is the implementation contract for porting AutoQuill. Update the checkboxes and decision log as work proceeds, but do not weaken an acceptance criterion without recording why.

The implementation sequence, module contracts, per-phase work packages, and recurring quality
gates are defined in [`docs/EXECUTION_PLAN.md`](docs/EXECUTION_PLAN.md).

## 1. Confirmed product decisions

- Use Rust for application logic and platform integration.
- Use Slint for the UI.
- Preserve current behavior before replacing the UI with the final design.
- Ship a separate native build for each supported platform.
- Initial architectures:
  - Windows x64.
  - macOS Apple Silicon and Intel. Prefer one Universal 2 app when practical; otherwise publish both builds.
  - Linux x64.
- Distribution:
  - Windows portable `.exe`; installer can be added without replacing the portable build.
  - macOS signed/notarized `.app` distributed in a DMG when signing credentials are available.
  - Linux raw executable plus AppImage. The raw executable is the size-focused build; AppImage is the portability-focused build.
- Match the visual identity of jivaro.net while making the desktop interface calmer, clearer, accessible, and task-focused.
- Best-effort platform parity is acceptable when an operating system prohibits a Windows feature. The UI must disclose capability differences before a run starts.
- Include permission onboarding, visible session controls, expanded hotkey support, organized advanced settings, improved profiles, legacy-profile import, update checks, system tray controls, accessibility, and scalable layouts.

## 2. Outcome and success criteria

The port is complete when:

- No Python installation, Python runtime, Qt installation, WebView, or external asset folder is required to run the raw application binary.
- All resources required by the raw application are compiled into its native executable.
- The Windows raw executable is materially smaller than the current 49.4 MB PyInstaller executable. Stretch goal: 20 MB or less after release optimization; record actual measurements rather than compromising behavior to hit the stretch goal.
- The application opens quickly, remains responsive during typing, and stops promptly after Stop or Escape.
- Existing AutoQuill v0.13 profiles can be imported without losing supported settings.
- The portable typing engine has deterministic automated tests.
- Each platform backend reports what it can actually do: foreground typing, target capture, background typing, global shortcuts, and required permissions.
- Windows passes complete parity testing before the Python application is retired.
- macOS and Linux pass the platform matrices in this document.
- Packaging, hashes, release notes, and update metadata are produced by repeatable CI jobs.

## 3. Existing behavior to preserve

### Editor and tokens

- Plain-text editor.
- Runtime tokens: `{CLIPBOARD}`, `{DATE}`, and `{TIME}`.
- Preserve the current literal escape form for tokens wrapped as `""{TOKEN}""`.
- Special-key instruction parsing, including Enter, Tab, Backspace, navigation keys, function keys, and the currently supported aliases.
- Full Unicode text, including non-Latin scripts and emoji where the platform injection API supports them.

### Typing session

- Configurable WPM, clamped to the current 1–200 range unless a later product decision changes it.
- Optional startup delay.
- Start, pause, resume, stop, and reset.
- Configurable stop-after duration using active typing time, excluding paused time.
- Looping with configurable delay range.
- Simulated human errors with configurable interval and count ranges.
- Simulated breaks based on word counts and duration ranges.
- Simulated pauses based on character counts and duration ranges.
- Progress, typed-character count, session state, and ETA.
- Responsive cancellation during sleeps, pauses, loop waits, and error simulation.
- Preserve the current pause/break compensation behavior, but cover it with tests before modifying it.

### Hotkeys and target modes

- Global shortcut starts a run when idle and toggles pause/resume during a run.
- Escape stops an active run.
- Keep F1–F12 compatibility while allowing modifier combinations such as `Ctrl+Shift+Space`.
- Foreground typing.
- Windows native background targeting for compatible classic controls.
- Windows browser-safe foreground assist and focus-loss protection.
- Target validation and useful failure messages instead of silently dropping characters.

### Profiles, links, and updates

- Save, load, rename, duplicate, delete, search, and set a default profile.
- Preserve all existing profile fields and add a versioned schema.
- Import legacy JSON profiles from the Python application.
- Open Jivaro, support, community, and other maintained links through the default browser.
- Check GitHub Releases asynchronously after startup; failures remain non-blocking and do not show an alarming modal.
- Show an update banner with release notes and a user-initiated download action. Do not silently replace the executable.

## 4. Product and UX design

### Core interaction model

The easiest path for a first-time user should be:

1. Paste or type text.
2. Choose typing speed.
3. Confirm the target mode and any required permission.
4. Press the visible Start button or the displayed hotkey.
5. See immediate countdown/session feedback.
6. Pause or stop from either the UI, hotkey, or tray.

The app must not require opening a settings dialog to complete that basic flow.

### Main window

- Compact header:
  - AutoQuill identity and version.
  - Current profile selector.
  - Permission/capability indicator.
  - Settings and overflow menu.
- Primary editor card:
  - Large text editor.
  - Character/word count.
  - Token insertion menu with descriptions.
  - Clear action that requires confirmation only when the editor is non-empty.
- Quick controls:
  - WPM slider plus numeric field.
  - Global shortcut recorder.
  - Target mode selector using plain language.
  - Startup delay.
- Persistent session bar:
  - Start as the dominant action while idle.
  - Pause/Resume and Stop while running.
  - Status, progress, ETA, and active target.
  - Do not rely on color alone to communicate state.
- Advanced options in a collapsible section:
  - Stop-after.
  - Looping.
  - Break simulation.
  - Pause simulation.
  - Error simulation.
  - Reveal dependent fields only when their feature is enabled.
- Contextual warnings appear beside the relevant control, not in a detached warnings box.

### Secondary surfaces

- First-run onboarding:
  - One short product explanation.
  - Platform permission checks.
  - Hotkey selection.
  - Safe typing test field contained inside AutoQuill.
- Profiles manager:
  - Search, create, duplicate, rename, import, export, and delete.
  - Dirty-state indicator and explicit save behavior.
- Settings:
  - Appearance: Dark, Light, and System. Dark is the Jivaro-first default.
  - Startup/update preferences.
  - Storage location and profile import/export.
  - Permissions and diagnostics.
  - About, license attribution, links, and version information.
- System tray:
  - Show/Hide, Start/Pause/Resume, Stop, current profile, and Quit.
  - Tray support may be disabled on desktops that do not provide a reliable tray protocol; expose the capability honestly.

### Accessibility requirements

- Complete keyboard navigation with logical focus order.
- Visible focus rings with at least 3:1 contrast against adjacent colors.
- Text and essential controls target WCAG AA contrast.
- Minimum normal text size equivalent to 14 px at 100% scaling.
- Controls remain usable at 125%, 150%, and 200% scaling.
- Minimum pointer target of approximately 36 logical pixels for compact desktop controls and 44 for primary actions where layout permits.
- Icons are accompanied by text or accessible labels.
- Respect reduced-motion settings where the platform exposes them.
- Never communicate Idle, Typing, Paused, Error, or Permission Needed through color alone.

## 5. Jivaro design system

The following tokens were derived from the current jivaro.net homepage and are the initial source of truth.

### Base palette

| Token | Value | Use |
|---|---:|---|
| `bg` | `#0F1113` | Main application background |
| `surface` | `#171A1D` | Cards and panels |
| `surface-raised` | `#202428` | Menus, active controls, raised panels |
| `text-primary` | `#F2F3F1` | Primary text |
| `text-secondary` | `#C8CBCB` | Secondary text |
| `text-muted` | `#A7ADB3` | Hints and metadata |
| `text-disabled` | `#7A8289` | Disabled text |
| `accent` | `#D87341` | Primary Jivaro orange |
| `accent-hover` | `#E08251` | Hover state |
| `accent-pressed` | `#B95E34` | Pressed state |
| `border-subtle` | `rgba(242, 243, 241, 0.12)` | Default dividers and card borders |
| `border-strong` | `rgba(242, 243, 241, 0.18)` | Focus-adjacent and active borders |

Add semantic success, warning, danger, and info colors only after contrast testing them against the base surfaces. The orange accent should remain special; do not turn every control orange.

### Visual language

- Inter for interface text, falling back to the system sans-serif stack.
- A restrained monospace face for hotkeys, tokens, timing values, and small uppercase metadata labels.
- Near-black layered surfaces, thin translucent borders, and subtle shadows.
- Corners generally 10–16 logical pixels; primary buttons can be slightly less rounded than cards.
- Use orange for the primary action, active progress, focus accents, and selected states.
- A subtle orange radial glow is permitted for onboarding or an empty state, never behind dense settings.
- Avoid excessive gradients, glass effects, glowing text, oversized marketing typography, and decorative animation.
- Light theme must preserve the warm Jivaro character without simply inverting colors.

## 6. Technical architecture

Use one Cargo package that produces one executable. Source files remain modular; “single executable” refers to the release artifact, not a single source file.

Suggested structure:

```text
rust_autoquill/
├── Cargo.toml
├── Cargo.lock
├── build.rs
├── assets/
├── packaging/
│   ├── windows/
│   ├── macos/
│   └── linux/
├── ui/
│   ├── app-window.slint
│   ├── components/
│   └── theme.slint
├── src/
│   ├── main.rs
│   ├── app.rs
│   ├── domain/
│   │   ├── settings.rs
│   │   ├── profile.rs
│   │   └── session.rs
│   ├── typing/
│   │   ├── engine.rs
│   │   ├── tokenizer.rs
│   │   ├── scheduler.rs
│   │   └── timing.rs
│   ├── platform/
│   │   ├── mod.rs
│   │   ├── windows.rs
│   │   ├── macos.rs
│   │   └── linux.rs
│   ├── persistence/
│   ├── updates/
│   └── diagnostics/
└── tests/
```

### Boundaries

- The typing engine must not import Slint or OS-specific APIs.
- The UI communicates with the engine through commands and typed events.
- All OS-specific code sits behind traits and `cfg`-gated modules.
- Time, randomness, clipboard content, and input injection are injectable dependencies for deterministic testing.
- Settings and profiles are validated in the domain layer, not only in UI controls.

### Platform capability interface

The platform layer should expose a capability report similar to:

```text
foreground_input
background_targeting
target_capture
global_shortcuts
tray
permission_state
wayland_session
```

Core interfaces should cover:

- Request/check permissions.
- Register/unregister a global shortcut.
- Capture, validate, describe, and detach a target.
- Inject Unicode text and supported special keys.
- Report why an attempted operation is unavailable.
- Subscribe to focus/target invalidation where possible.

### Session state machine

Use explicit states:

```text
Idle -> Preparing -> Countdown -> Typing <-> Paused -> Stopping -> Idle
                                      \-> Completed -> Idle
                                      \-> Failed -> Idle
```

- One worker owns a run.
- Commands are delivered through channels; cancellation must be checked between characters and during interruptible waits.
- UI updates are posted back onto the Slint event loop.
- Do not update Slint objects directly from a worker thread.
- Prevent concurrent runs and stale events from an earlier run with a session identifier.

### Dependency policy

- Prefer small, maintained crates with permissive licenses.
- Disable unused default features.
- Put native dependencies in target-specific Cargo sections so one platform does not carry another platform’s code.
- Likely foundations: `slint`, `serde`, `serde_json`, `thiserror`, `semver`, `directories`, `tracing`, and a small HTTP client with TLS.
- Evaluate rather than blindly commit to `global-hotkey` and `enigo`; AutoQuill will still need native backends for full Windows behavior and Wayland permission flows.
- Avoid an always-on async runtime unless a backend requires it. If Wayland portal support needs async execution, contain it within the Linux backend.

## 7. Platform implementation strategy

### Windows x64

- Port Unicode foreground injection through `SendInput`.
- Port classic-control background typing through `PostMessageW`.
- Preserve target HWND capture, root-window tracking, browser detection, focus assist, and focus-loss cancellation.
- Use a native global-shortcut API for registered combinations. Use a hook only when a desired shortcut cannot be registered safely.
- Wrap Windows timer-resolution changes in an RAII guard so they are always restored.
- Preserve the current application identity and migrate to a stable bundle identifier such as `net.jivaro.autoquill` after checking update implications.
- Acceptance: all current Windows typing modes and profiles pass side-by-side comparison with v0.13.

### macOS Apple Silicon and Intel

- Use `CGEvent` for foreground Unicode/key events.
- Investigate Accessibility APIs for target-specific delivery; expose it only where tests show reliable behavior.
- Provide an in-app Accessibility/Input Monitoring permission guide with a re-check button.
- Use the most appropriate system global-shortcut mechanism that works in a signed app.
- Never repeatedly steal focus from another application.
- Package an `.app`; use DMG distribution. Add Developer ID signing and notarization when credentials are supplied.
- Prefer Universal 2 packaging after both architecture builds pass independently.
- Acceptance: permissions survive restart, foreground typing works in common native and browser fields, shortcuts work, and denied permissions produce a helpful state.

### Linux x64

- Detect X11 versus Wayland at runtime.
- X11:
  - Implement foreground injection and global shortcuts using maintained native Rust bindings.
  - Evaluate background targeting separately; ship only combinations verified in common applications.
- Wayland:
  - Use supported portal/libei flows with explicit user consent.
  - Do not promise arbitrary background targeting.
  - If a compositor cannot provide the required portal, show a concise limitation and offer copy-to-clipboard/manual alternatives.
  - Investigate the Global Shortcuts portal; otherwise fall back to visible controls and document compositor limitations.
- Test at minimum on current Ubuntu GNOME Wayland, Ubuntu X11, and KDE Plasma Wayland/X11 when runners or test machines are available.
- Publish a raw executable built against the documented baseline and an AppImage for broader portability.

## 8. Persistence and migration

- Create a versioned profile schema, beginning with `schema_version: 2`.
- Keep profile data separate from app preferences.
- Use platform-appropriate user data directories for new installations.
- On first run, look for the legacy Windows directory `~/Jivaro/AutoQuill/Data/Saves` and offer a one-click import when files are present.
- Never modify or delete the Python profiles during import.
- Validate names and prevent path traversal.
- Save through a temporary file followed by an atomic replace where supported.
- Preserve unknown legacy fields during import when practical so downgrading does not unnecessarily destroy data.
- Provide explicit JSON import/export for portability between platforms.

## 9. Delivery phases and gates

### Phase 0 — Foundation and size spike

- [x] Initialize Cargo application in `rust_autoquill`.
- [x] Add Slint license attribution to the About design from the beginning.
- [x] Create a minimal embedded-resource Slint window.
- [x] Compare Slint renderer/backend combinations for release size and visual quality.
- [x] Configure formatting, Clippy, unit tests, release profiles, lightweight development diagnostics, and CI skeleton.
- [x] Record baseline binary size and startup time for each locally available platform.

Phase 0 completed on 2026-08-20. The Windows x64 baseline selects Slint's software renderer:
8.25 MiB and 666–1,007 ms across final warm smoke runs. FemtoVG remains available as a feature for
future visual-performance comparisons. macOS and Linux measurements will be recorded by their
native CI/release runners when those artifacts are introduced.

Gate: a blank branded application builds as one raw executable on Windows and CI can begin platform builds.

### Phase 1 — Portable core parity

- [ ] Port settings models and WPM conversion.
- [ ] Port runtime variables and token escaping.
- [ ] Port special-key tokenizer.
- [ ] Port scheduler, pauses, looping, stop-after behavior, and simulated errors.
- [ ] Implement session state machine and fake input backend.
- [ ] Port versioned persistence and legacy importer.
- [ ] Add deterministic unit and state-machine tests.

Gate: core tests reproduce v0.13 behavior without a real keyboard or UI.

### Phase 2 — Functional parity UI

- [ ] Build an intentionally plain Slint UI exposing every current setting.
- [ ] Connect editor, profiles, session commands, progress, ETA, warnings, and update check.
- [ ] Add hotkey recorder and capability model.
- [ ] Ensure all features can be exercised before visual redesign.

Gate: no current user-facing feature is missing from the Rust application, excluding documented platform limitations.

### Phase 3 — Windows backend parity

- [ ] Port foreground injection.
- [ ] Port background target capture and message injection.
- [ ] Port browser focus assist and failure reporting.
- [ ] Register global shortcuts and Escape behavior.
- [ ] Test Unicode, special keys, stop/pause responsiveness, focus loss, and closed targets.
- [ ] Measure raw executable size against the Python release.

Gate: Windows passes the complete parity checklist and is safe for beta use.

### Phase 4 — macOS and Linux backends

- [ ] Implement macOS permissions, shortcuts, foreground typing, and verified target assistance.
- [ ] Build and test Apple Silicon and Intel releases.
- [ ] Implement Linux X11 input and shortcuts.
- [ ] Implement Wayland portal/libei flow and capability messaging.
- [ ] Complete the Linux compositor test matrix.

Gate: each platform completes onboarding and a real typing session, with unavailable capabilities explained before Start.

### Phase 5 — Jivaro UI redesign and additions

- [ ] Implement the design tokens in `theme.slint`.
- [ ] Replace the parity UI with the main-window interaction model in this plan.
- [ ] Add first-run onboarding and internal typing test.
- [ ] Add collapsible/contextual advanced controls.
- [ ] Build improved profile management.
- [ ] Add Dark, Light, and System themes.
- [ ] Add system tray controls where supported.
- [ ] Complete keyboard navigation, scaling, contrast, and reduced-motion review.
- [ ] Run usability passes for new, occasional, and power users.

Gate: a first-time user can complete a successful run without documentation, and advanced users can access every existing setting without cluttering the basic flow.

### Phase 6 — Packaging, updates, and release hardening

- [ ] Embed icons, metadata, licenses, and version information.
- [ ] Produce Windows portable EXE and optional installer.
- [ ] Produce macOS `.app`/DMG; add signing and notarization when credentials are available.
- [ ] Produce Linux raw executable and AppImage.
- [ ] Publish checksums and an artifact manifest.
- [ ] Connect asynchronous GitHub Release checks.
- [ ] Add crash-safe local diagnostics with a user-controlled copy/export action.
- [ ] Run antivirus false-positive, clean-machine, and upgrade/migration checks.
- [ ] Write installation, permission, troubleshooting, and uninstall documentation.

Gate: reproducible release artifacts pass clean-machine smoke tests on all target platforms.

## 10. Verification strategy

### Automated core tests

- Token expansion and literal escaping.
- Special-key parsing and aliases.
- WPM clamping and delay calculation.
- Pause/resume timing and stop-after active-time accounting.
- Loop behavior and progress reset.
- Break/pause compensation.
- Deterministic simulated errors and deletions.
- Cancellation at every wait boundary.
- Profile validation, round trip, atomic save, and v0.13 import.
- Update-version parsing and comparison.

### Backend contract tests

- Correct down/up event pairs for keys.
- Unicode scalar handling, including surrogate-requiring Windows characters.
- Target invalidation produces one failure and stops the run.
- Permission denial never results in a false “Typing” status.
- Hotkey registration conflicts are reported and recoverable.
- No input is emitted after a run has entered Stopping.

### Manual application matrix

- Text editors: Notepad or equivalent, VS Code, native macOS editor, and common Linux editors.
- Browsers: current Chrome/Chromium, Firefox, Edge on Windows, and Safari on macOS.
- Common form fields, multiline editors, non-Latin input, emoji, special keys, and long text.
- Pause, resume, stop, target closure, focus change, sleep/wake, permission revocation, and shortcut collision.
- Display scaling, light/dark system themes, narrow window, keyboard-only navigation, and screen-reader smoke checks where supported.

## 11. Binary-size controls

- Configure release builds with LTO, one codegen unit, symbol stripping, and `panic = "abort"` after confirming crash diagnostics remain useful.
- Build only one Slint renderer/backend combination per artifact.
- Compile resources instead of shipping loose files.
- Keep debug symbols as separate CI artifacts when possible.
- Audit feature trees with `cargo tree -e features` before each release.
- Track raw executable size in CI and flag significant regressions.
- Exclude the Linux AppImage wrapper from comparisons with the raw executable because it intentionally bundles compatibility dependencies.

## 12. Known risks and responses

| Risk | Response |
|---|---|
| Wayland blocks unrestricted synthetic input | Use permission-mediated portals/libei, report capabilities, and keep X11 support |
| macOS requires user permissions and release signing | Build guided onboarding; support unsigned development builds and signed/notarized releases when credentials exist |
| Windows background injection is application-dependent | Preserve browser detection/focus assist and maintain a tested compatibility matrix |
| Cross-platform shortcut support differs | Record conflicts, support visible controls, and expose compositor limitations |
| Slint ecosystem/API changes | Pin compatible versions in `Cargo.lock`, review releases deliberately, and preserve UI/domain separation |
| Single-file size grows through dependencies | Use target-specific dependencies, disable unused features, and enforce size tracking |
| Automated input tests can affect the user’s machine | Use a fake backend by default and run real injection tests only in explicit manual/integration modes |

## 13. Definition of the first implementation milestone

The first milestone after this plan is approved is not a full rewrite. It is a Windows development build containing:

- A branded Slint shell using the Jivaro tokens.
- The portable settings/profile models.
- The tokenizer and token expansion with tests.
- A fake typing backend that drives visible progress without emitting real keystrokes.
- Release-size measurement.

That slice validates architecture, UI integration, testing, and artifact size before the native input code is ported.

## 14. Decision log

- 2026-08-20: Rust + Slint selected.
- 2026-08-20: Feature parity will precede final UI redesign.
- 2026-08-20: Windows x64, macOS Apple Silicon/Intel, and Linux x64 selected.
- 2026-08-20: Windows portable EXE, macOS app/DMG, and Linux raw binary/AppImage selected.
- 2026-08-20: Jivaro.net palette and visual identity selected, with usability and accessibility taking priority over exact website imitation.
- 2026-08-20: Best-effort, capability-aware behavior approved for platform security restrictions.
