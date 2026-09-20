# AutoQuill 0.19 UI Reliability Package

## 0.19.0-beta.2 navigation and Linux follow-up (2026-09-20)

The approved follow-up preserves the single workspace, settings drawer, profile manager,
safe launch mode, and existing data folders.

Confirmed issues and corrections:

- Profile actions held a controller borrow across nested mutable callbacks. Selected names are
  now copied before opening dialogs or loading profiles.
- Naming dialogs initially focused the primary action; empty naming requests could hide their
  input. They now consistently show and focus the name field.
- Native Windows inspection showed that naming and typing-confirmation panels stretched to
  the full window height. Explicit panel heights now keep these dialogs compact and centered.
- Validation and operation feedback could be hidden behind overlays. Dialog errors and
  profile/settings results are visible in the owning surface.
- Failed dialog actions could lose their retry state, and Save-before-New could leave the
  old confirmation open. Retry actions are retained and successful saves dismiss the dialog.
- Tray Exit bypassed the unsaved-changes confirmation. It now uses the same Save/Discard/Cancel
  flow as closing the workspace, then quits after the user's choice.
- Destroying a parent overlay for a nested dialog reset its search cursor and scroll state.
  Parents now stay mounted and disabled while the child owns focus.
- The header did not identify its profile action, Settings opened under a different label,
  and the empty-profile guidance incorrectly described New as saving. Labels and guidance
  now match their actions; successful Load/New returns focus to the editor.
- Linux validated only the active top-level X11 window. It now captures and checks focused
  X11 child identity and ancestry before delivery. Missing DISPLAY and non-desktop sessions
  cannot advertise X11 input. Wayland remains simulation-only.

Verification for this version uses portable capability/UI tests, an isolated profile-controller
regression, local Windows UI inspection, and the existing native packaging matrix. The Linux
package additionally runs a focused Xvfb regression for child-focus changes, outside-window
focus, target loss, and refusal to emit after a target change. Physical Linux desktop behavior
is still unverified; the beta label does not imply hardware validation.

The Linux launch guide is shipped with the package and documents the Ubuntu 22.04+ x64
baseline, AppImage launch/FUSE fallback, desktop portals, shortcuts, and the Wayland boundary.
The former linuxdeploy continuous URL had changed bytes and failed its checksum preflight.
Packaging now pins release 1-alpha-20251107-1 and its GitHub-published SHA-256 digest.

## Original 0.19.0-beta.1 package

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
