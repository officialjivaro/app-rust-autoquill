# Phase 2C Windows completion verification

Date: 2026-08-24
Version: `0.16.0-beta.1`

## Delivered

- Sticky Auto captures the foreground root and focused child controls with separate process and
  class identities.
- Background delivery is limited to verified Win32 `Edit`, RichEdit, and Windows Forms edit
  controls. It uses `PostMessageW` without activating or stealing focus.
- Browsers, Electron/custom controls, unknown classes, and documents containing background-unsafe
  special keys select **Foreground Protected** before typing starts and disclose that strategy.
- Every operation revalidates both captured controls, their process identities, classes, and root
  relationship. Protected delivery additionally requires the exact foreground/focused target.
- The shortcut recorder accepts F1–F12 plus modifier combinations such as `Ctrl+Shift+Space`.
  Letter, symbol, and Space shortcuts require a modifier so ordinary typing is never globally
  consumed by a bare key.
- Shortcut replacement remains transactional. Escape is registered as a second emergency Stop only
  while Real Typing is active and is released immediately afterward.
- The Windows executable contains the Jivaro icon and native product, company, description,
  filename, and `0.16.0-beta.1` version fields.

## Local verification

- `cargo fmt --all -- --check` passed.
- Strict workflow-equivalent Clippy passed with `-D warnings`.
- The normal locked test suite passed: 61 library, 8 application, and 4 UI interaction tests. The
  three real-input suites remained ignored during the normal run.
- The explicit native-input probe passed Unicode foreground input, strict focus-loss rejection,
  Sticky Background after foreground change, unsafe-document fallback, and target-closure failure.
- The explicit hotkey probe passed F1–F12 inventory, transactional conflicts, MOD_NOREPEAT,
  `Ctrl+Shift+Space`, emergency Escape, 24 rebind cycles, and a measured 816.6 microsecond Stop
  event path.
- The explicit compatibility matrix passed Notepad, Microsoft Edge, and Google Chrome using only
  temporary documents and isolated browser profiles.
- Firefox was not installed. VS Code's isolated clean-profile window did not appear; the harness
  reports this optional editor limitation without weakening Notepad or browser gates.
- The locked size-focused software-renderer build passed its hidden self-closing smoke test with
  exit code 0 in 1,649 milliseconds.

## Release evidence

- Optimized Windows x64 executable: 9,839,616 bytes (about 9.38 MiB).
- Size change from `0.15.0-beta.1`: +80,896 bytes (+0.83%), below the 1 MiB dependency review
  threshold.
- SHA-256: `4B39718B8F6EE96F862A208C9D76994297D64DA1687FE14B8C5DF4910FB4AC2B`.
- Windows Explorer metadata reports `AutoQuill`, `Jivaro LLC`, `AutoQuill automatic typing
  assistant`, original filename `AutoQuill.exe`, and version `0.16.0-beta.1`.
- The verified executable is published at `dist/AutoQuill-windows-x64.exe` and preserved in the
  gitignored `local-builds` backup with the same hash.

## Deferred deliberately

- Native Real Typing and permission onboarding on macOS and Linux.
- Windows tray controls, installer, code signing, antivirus reputation, and clean-machine trust
  checks. These remain release-hardening work and are not claimed by this unsigned beta.
