# AutoQuill

AutoQuill is being ported from Python/PySide6 to a compact Rust + Slint desktop application.
The current source includes a branded, interactive editor, a safe simulation preview, and an
explicitly armed Windows foreground-typing beta backed by the same portable compiler and session
engine. The Windows reliability matrix covers Notepad, Edge, Chrome, target loss, and global-key
stress. Start, Pause/Resume, Stop, Reset, startup delay, active-time limits, loops, breaks, short
pauses, and visible simulated corrections are functional. Profiles keep the draft and every
setting together, including deliberate legacy import and recoverable upgrades.

![AutoQuill Phase 0 shell](docs/phase0-shell.png)

The product roadmap is in [BUILD_PLAN.md](BUILD_PLAN.md), with the work-package sequence in
[docs/EXECUTION_PLAN.md](docs/EXECUTION_PLAN.md).

## Windows foreground-typing beta

The latest verified Windows x64 executable is available at
[`dist/AutoQuill-windows-x64.exe`](dist/AutoQuill-windows-x64.exe). `/dist` is refreshed only after
the corresponding source gate passes. This unsigned beta includes both safe Simulation and the
opt-in Windows Real Typing flow. The same verified build is preserved at
`local-builds/AutoQuill-windows-x64.exe` (gitignored); build current source to recreate it on a new
machine.

## Current safety boundary

Simulation is selected on every launch and never emits external keystrokes. On Windows, choosing
**Real Typing** requires an explicit confirmation each launch. Focus the exact destination window
and press the configured bare F1–F12 key to capture it; the same key stops the session. Real Typing
always waits two seconds, validates the foreground window before every operation, and stops
immediately if that window changes or closes. AutoQuill refuses to target itself and blocks a run
when its activation F-key also appears in the document. Activation-key rebinding is transactional:
if the new key is occupied, the old key stays available as an emergency Stop but cannot start a new
run. Queued/repeated key events cannot restart a session immediately after Stop.

Real Typing is not yet enabled on macOS or Linux. Sticky/background targeting is also deferred;
this beta deliberately supports strict foreground typing only.

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
`Data/preferences.json`. Legacy profiles stay byte-for-byte unchanged until you explicitly save
and approve an upgrade; the original bytes are backed up under `Data/Backups`. Deleted profiles
move to `Data/Trash`.

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
desktop where temporary documents, isolated browser profiles, and controlled F-keys may be used:

```text
cargo test --test windows_native_input -- --ignored
cargo test --test windows_hotkey_stress -- --ignored --nocapture --test-threads=1
cargo test --test windows_compatibility_matrix -- --ignored --nocapture --test-threads=1
```

See [docs/PHASE2B_WINDOWS_RELIABILITY.md](docs/PHASE2B_WINDOWS_RELIABILITY.md) for the exact matrix,
known VS Code harness limitation, and release evidence.

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

The packaged raw executable embeds the Slint markup and image resources. Platform-specific bundles
and installers will be added during the packaging phase.

See [docs/DEPENDENCY_SAFETY.md](docs/DEPENDENCY_SAFETY.md) for the locked dependency safeguard
added after an unexpected build-script download attempt was observed during a clean rebuild.

## Licensing notice

The application displays Slint's `AboutSlint` attribution widget under the Slint Royalty-Free
Desktop, Mobile, and Web Applications License. See [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
