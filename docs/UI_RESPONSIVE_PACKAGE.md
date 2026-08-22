# Responsive interface package

Date: 2026-08-22
Version: `0.15.0-beta.1`

## Delivered

- A 1280×720 default window with a 960×600 minimum and no locked aspect ratio.
- An editor-first 65/35 workspace at wide sizes and a scrollable stacked workspace below 1100
  logical pixels.
- Simulation and Real Typing mode selection inside the run panel instead of a separate banner.
- A single primary action that changes from Start to Stop; Pause/Resume remains secondary.
- An always-available validation preview in both modes, with target and foreground guidance in
  Real Typing mode.
- A right-side settings drawer with collapsible Timing + Session and Natural Variation groups.
- Escape-key dismissal for the topmost drawer, profile manager, confirmation, or dialog.
- DPI-independent window-size persistence, saved position recovery, Windows virtual-screen
  validation, and a Reset Window action.
- A refined Jivaro charcoal and warm-orange theme using only embedded native Slint resources.

## Compatibility and safety

- Simulation remains selected on every launch and emits no native input.
- Windows Real Typing remains opt-in, explicitly confirmed, foreground-only, and protected by the
  existing focus-loss and activation-key safeguards.
- Existing profiles and schema-1 preference files remain readable. Older preferences gain safe
  1280×720 window defaults without rewriting profile data.
- On Wayland, window managers retain control of placement; AutoQuill still restores the usable
  logical size. Windows validates a saved position against the current virtual desktop before
  applying it.

## Verification

- `cargo fmt --all -- --check` passed.
- `cargo clippy --all-targets --all-features -- -D warnings` passed.
- `cargo test --locked` passed: 57 library, 6 application, and 3 UI interaction tests; the three
  real-input suites remained ignored in the ordinary run.
- The explicit controlled Windows native-input probe passed against its temporary text box.
- The self-closing real-window smoke test passed using the default software renderer.

## Release evidence

- Verified source commit: `ea506a383fb08a84ab2cfd4232bbbb4a5ec5def6`.
- Optimized Windows x64 executable: 9,758,720 bytes (about 9.31 MiB), an increase of 80,384 bytes
  from the Phase 2B build.
- SHA-256: `F8ED1F0B0920E4A77A6EEC92C32282692485CA70773E1EA454C656AD75F4AFB6`.
- The optimized self-closing smoke test passed before publication.
- The verified executable is published to `dist/AutoQuill-windows-x64.exe` and preserved in the
  gitignored `local-builds` backup with a matching hash.
