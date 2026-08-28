# AutoQuill 0.19 UI Reliability Package

Status: Implemented; release verification pending
Version: `0.19.0-beta.1`
Date: 2026-08-29

## Purpose

This package applies the findings from the hands-on Windows review that complemented the user's
normal usage pass. It is a focused reliability and accessibility correction, not a wholesale UI
redesign.

## Corrections included

- Clip the responsive stacked workspace to its scroll viewport so content cannot paint over the
  fixed header at the 960 x 600 minimum and high display scaling.
- Treat Settings, Profiles, Real Typing confirmation, and profile dialogs as modal surfaces:
  disable the main workspace, hide lower overlays, and place initial keyboard focus inside the
  active surface.
- Give the WPM control, profile search/name inputs, and every advanced numeric value/range a stable
  accessibility name.
- Implement the UI Automation ValuePattern setter for the editable document while keeping the
  simulation preview read-only.
- Use dark text on the warm accent button in Dark/System-dark mode to meet normal-text contrast;
  retain warm-white text in Light mode.
- Replace the blank Profiles result area with guided empty and no-match states without importing
  anything automatically.
- Show a persisted one-time notice explaining that closing the window keeps AutoQuill in the tray
  and that the tray Exit action quits the process.

## Verification contract

The Rust UI regression tests cover the ValuePattern behavior, accessible names, modal initial
focus, and profile empty/no-match guidance. `scripts/windows-ui-acceptance.ps1` runs a second layer
against a built Windows executable through Microsoft UI Automation. It verifies the editor value
pattern, labeled spinners, disabled modal background, contained Settings tab traversal, profile
search focus, and the no-match state.

The normal local gate remains:

```text
cargo fmt --check
cargo clippy --locked --all-targets --no-default-features --features renderer-software -- -D warnings
cargo test --locked --no-default-features --features renderer-software
cargo build --profile release-size --locked --no-default-features --features renderer-software
powershell -ExecutionPolicy Bypass -File scripts/package-windows.ps1 -Version 0.19.0-beta.1 -OutputDirectory artifacts/windows
powershell -ExecutionPolicy Bypass -File scripts/windows-ui-acceptance.ps1 -Executable artifacts/windows/AutoQuill-0.19.0-beta.1-windows-x64.exe
```

## Platform boundary and remaining work

Windows is the only platform available for interactive UI and native-input verification. The
macOS Universal 2 and Linux x64 packages continue to receive source review, native-runner tests,
packaging checks, manifests, and smoke checks, but remain explicitly unverified on end-user
hardware. After 0.19, the open release-hardening work is asynchronous update checks with a
user-initiated download, signing/notarization when credentials exist, and clean-machine,
antivirus, migration, permission, and uninstall checks.
