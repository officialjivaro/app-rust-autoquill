# Phase 3 compile-only platform package

Date: 2026-08-24
Version: `0.17.0-beta.1`
Published artifact: Windows x64 only

## What this package adds

- A shared runtime capability report for Windows, macOS, Linux X11, Linux Wayland, and unsupported
  systems.
- macOS foreground-process capture, per-operation validation, Accessibility permission checks,
  foreground input, global shortcuts, and an in-app Re-check path.
- Linux runtime session detection, X11 active-window/process capture, per-operation validation,
  foreground input, and global shortcuts.
- Explicit Wayland refusal for unrestricted synthetic input. Simulation remains available and the
  UI explains that consent-based portal/libei support is not implemented yet.
- Platform-specific UI status, verification labels, permission guidance, and Sticky Auto
  availability. Unsupported or unverified capabilities never silently fall back to external input.

## Verification boundary

Windows formatting, strict Clippy, automated tests, controlled native-input probes, release build,
smoke startup, metadata, and artifact hashes are verified locally. Apple Silicon and Intel macOS
targets are type-checked from Windows. Native Ubuntu CI is the Linux compile authority because the
Windows host cannot provide a Linux fontconfig sysroot.

The privacy-remapped Windows executable is 9,844,224 bytes (9.39 MiB), reports
`0.17.0-beta.1` in both file-version fields, and has SHA-256
`1464527C2FF388A835256934B76C17CA0DD25CFE245026E62684BBA981CC2692`.

The macOS and Linux backends are **compile-only experiments**, not published product support. They
must not be relabelled as verified until the native matrices below pass on real hardware.

## macOS handoff matrix

1. Build and launch on current Apple Silicon and Intel macOS versions.
2. Verify denied, granted, revoked, and re-granted Accessibility permission states and the Re-check
   action without false success.
3. Test TextEdit, Notes, Safari, Chrome, and Firefox with ASCII, non-Latin text, emoji, navigation,
   Backspace/Delete, Enter, Tab, and function keys.
4. Test activation-key conflicts, transactional rebinding, active-session Escape, Stop latency,
   focus changes, target closure, sleep/wake, and application restart.
5. Confirm Sticky Auto stays unavailable and Simulation stays the default every launch.

## Linux X11 handoff matrix

1. Build and launch on Ubuntu GNOME X11 and KDE Plasma X11.
2. Test native editors, Chrome/Chromium, and Firefox with the same text and key cases as macOS.
3. Test X11 active-window changes, closed targets, application restart, shortcut conflicts,
   transactional rebinding, active-session Escape, and Stop latency.
4. Confirm the app refuses to target itself, stops on target changes, and never advertises Sticky
   Auto.

## Linux Wayland handoff matrix

Current expected behavior is Simulation-only with visible guidance. On Ubuntu GNOME Wayland and KDE
Plasma Wayland, confirm Real Typing cannot be armed and no unrestricted input or shortcut attempt is
made. A later package may add XDG Global Shortcuts and Remote Desktop/libei consent flows after they
can be implemented and tested on representative compositors.
