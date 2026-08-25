# AutoQuill

AutoQuill is being ported from Python/PySide6 to a compact Rust + Slint desktop application.
The current source includes a branded, interactive editor, a safe simulation preview, and an
explicitly armed Windows Real Typing beta backed by the same portable compiler and session engine.
Logically validated macOS and Linux X11 preview backends now share that engine while reporting
their unverified status and operating-system limitations directly in the UI.
Sticky Auto safely selects native background delivery or protected foreground delivery before a
run begins. The responsive interface opens at 1280×720, scales freely down to a usable 960×600
minimum, and converts from an editor-first two-column workspace to a scrollable stacked layout at
narrow widths. Dark, Light, and System themes, reduced motion, privacy-safe diagnostics, a modern
cross-platform icon, and best-effort tray controls are included. The Windows reliability matrix
covers Notepad, Edge, Chrome, target loss, and
global-shortcut stress. Start/Stop, Pause/Resume, Reset, startup delay, active-time limits, loops,
breaks, short pauses, and visible simulated corrections are functional. Profiles keep the draft
and every setting together, including deliberate legacy import and recoverable upgrades.

The product roadmap is in [BUILD_PLAN.md](BUILD_PLAN.md), with the work-package sequence in
[docs/EXECUTION_PLAN.md](docs/EXECUTION_PLAN.md).

## Windows Real Typing beta

The latest verified Windows x64 executable is available at
[`dist/AutoQuill-windows-x64.exe`](dist/AutoQuill-windows-x64.exe). `/dist` is refreshed only after
the corresponding source gate passes. This unsigned beta includes both safe Simulation and the
opt-in Windows Real Typing flow. The same verified build is preserved at
`local-builds/AutoQuill-windows-x64.exe` (gitignored); build current source to recreate it on a new
machine.

## Current safety boundary

Simulation is selected on every launch and never emits external keystrokes. On Windows, choosing
**Real Typing** requires an explicit confirmation each launch. Focus the exact destination window
and press the recorded shortcut to capture it; F1–F12 and modifier combinations such as
`Ctrl+Shift+Space` are supported. The same shortcut stops the session, and Escape is registered as
an additional emergency Stop only while Real Typing is active. Real Typing always waits two
seconds and validates the exact root and focused child controls before every operation.

With **Sticky Auto** enabled, verified native `Edit`, RichEdit, and Windows Forms edit controls can
receive a conservative set of text/navigation messages after another app comes foreground.
Browsers, Electron/custom controls, unknown classes, and documents containing background-unsafe
special keys are disclosed as **Foreground Protected** before typing begins. Protected sessions
stop immediately if the required focus changes. AutoQuill never steals focus, refuses to target
itself, and blocks only an unmodified F-key document token that exactly conflicts with the active
shortcut. Shortcut rebinding remains transactional, so a failed replacement does not remove the
working Stop shortcut. Queued/repeated events cannot restart a session immediately after Stop.

Platform support is deliberately capability-based:

| Platform | Real Typing | Global shortcut | Sticky Auto | Verification |
|---|---|---|---|---|
| Windows x64 | Available | Available | Compatible controls | Verified beta |
| macOS Universal 2 | Accessibility permission required | Unverified preview | Unavailable | Logical/native-runner QC only |
| Linux X11 x64 | Unverified preview | Unverified preview | Unavailable | Logical/Xvfb/native-runner QC only |
| Linux Wayland | Simulation only | Unavailable | Unavailable | Portal work deferred |

The macOS and Linux packages are produced as explicitly unverified previews. The macOS bundle is
unsigned and unnotarized, so Gatekeeper may require a deliberate user override. macOS re-checks
Accessibility permission before enabling Real Typing. Linux detects X11 versus Wayland at runtime;
Wayland refuses unrestricted synthetic input and keeps Simulation available rather than claiming
support that the compositor may prohibit. See
[docs/PHASE4_RELEASE_CANDIDATE.md](docs/PHASE4_RELEASE_CANDIDATE.md) for the exact logical-QC and
packaging boundary.

## Profiles

Open the profile manager from the profile button in the header. It supports search, load, save,
Save As, rename, duplicate, default selection, export, and recoverable deletion. **Import Files**
accepts multiple JSON profiles; **Import Folder** scans one selected folder without recursing.
Imports are previewed and never opened automatically. Name conflicts are kept as separate copies.

AutoQuill uses the same cross-platform location as the original app:

```text
~/Jivaro/AutoQuill/Data/Saves
```

On Windows, `~` is `C:\Users\<username>`. Preferences are separate at
`Data/preferences.json`; they also remember the appearance, reduced-motion choice, and last usable
window size and position. Use **Reset Window** in the settings drawer to return to a centered
1280×720 window. Legacy profiles stay
byte-for-byte unchanged until you explicitly save and approve an upgrade; the original bytes are
backed up under `Data/Backups`. Deleted profiles move to `Data/Trash`.

## Responsive interface

- The editor remains the dominant workspace and the validation preview remains visible in both
  Simulation and Real Typing modes.
- The primary Start action becomes Stop while a session is active; Pause/Resume remains secondary.
- Common WPM and token controls stay beside the editor. Optional session and natural-variation
  controls live in a collapsible right-side drawer.
- Narrow windows stack the editor above the run panel and provide vertical scrolling instead of
  clipping controls.
- Escape closes the topmost settings drawer, profile manager, confirmation, or dialog.
- The tray provides Show, Start/Stop, and Exit when the desktop exposes a compatible tray host.
- Copy/Export Diagnostics omits typed text, clipboard contents, profile names, and injected keys.

## Prerequisites

- Rust 1.97.1 (managed automatically through `rust-toolchain.toml` when using rustup)
- Native build prerequisites required by Slint for the target operating system

## Development

```text
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo run
```

All Windows native-input probes are excluded from normal tests. Run them only on an interactive
desktop where temporary documents, isolated browser profiles, and controlled shortcuts may be used:

```text
cargo test --test windows_native_input -- --ignored
cargo test --test windows_hotkey_stress -- --ignored --nocapture --test-threads=1
cargo test --test windows_compatibility_matrix -- --ignored --nocapture --test-threads=1
```

See [docs/PHASE2C_WINDOWS_COMPLETION.md](docs/PHASE2C_WINDOWS_COMPLETION.md) for Sticky Auto,
shortcut, Escape, metadata, matrix, and release evidence.

Development and test profiles disable debug-symbol and incremental caches to keep storage usage
manageable. After preserving a verified release executable, remove Cargo artifacts with:

```text
powershell -ExecutionPolicy Bypass -File scripts/clean-local.ps1
```

## Release builds

The software renderer is the default. It produced much faster startup on the Phase 0 Windows
baseline while keeping the raw executable close to the FemtoVG build in size:

```text
cargo build --profile release-size --locked
```

For the FemtoVG comparison build:

```text
cargo build --profile release-size --locked --no-default-features --features renderer-femtovg
```

The packaging workflow produces a Windows x64 EXE, an unsigned Universal 2 macOS app/DMG, and a
Linux x64 raw executable/AppImage. It runs only for a manual request or a commit containing
`[platform ci]`; ordinary pushes retain the smaller Windows quality job. Packages, checksums, and
manifests are retained as workflow artifacts for 14 days and downloaded locally to
`dist/platform-builds` after a completed package run. This process does not create a public GitHub
Release.

Completed local build sets are archived under `dist/builds/<version>`. The archive script preserves
the newest three versions without committing the binaries to Git:

```text
powershell -ExecutionPolicy Bypass -File scripts/archive-builds.ps1 -Version 0.18.0-beta.1 -SourcePath dist/platform-builds
```

See [docs/DEPENDENCY_SAFETY.md](docs/DEPENDENCY_SAFETY.md) for the locked dependency safeguard
added after an unexpected build-script download attempt was observed during a clean rebuild.

## Licensing notice

The application displays Slint's `AboutSlint` attribution widget under the Slint Royalty-Free
Desktop, Mobile, and Web Applications License. See [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
