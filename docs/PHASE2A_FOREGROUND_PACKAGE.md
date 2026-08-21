# Phase 2A Windows foreground package verification

Date: 2026-08-21

## Delivered

- Simulation remains the safe default on every launch.
- Real Typing requires explicit confirmation each launch and a mandatory two-second countdown.
- A configurable bare F1–F12 global key starts and stops the session; conflicts are visible.
- The foreground target is captured only when the activation key is pressed outside AutoQuill.
- Window identity and process ownership are validated throughout countdown and before every input.
- Focus change, target closure, input failure, or a partial Windows input send stops the session.
- Unicode uses Windows UTF-16 input, including surrogate pairs; canonical special keys use native
  down/up pairs and extended-key flags where required.
- Documents cannot use the same F-key token that is reserved for Start/Stop.
- Sticky/background typing and native macOS/Linux input remain deliberately deferred.

## Verification

- `cargo fmt --check` passed.
- Strict Clippy passed across all targets with warnings denied.
- 56 library tests, 3 executable tests, and 2 real-event/accessibility UI tests passed.
- The default suite confirmed that real input is ignored unless explicitly requested.
- The explicit Windows probe typed `AutoQuill 日本🙂` into its own temporary external text box,
  verified the received text, changed foreground focus, and confirmed immediate target rejection.
- GitHub Actions run `32451530818` passed Windows format/lint/tests and optimized release builds on
  native Windows, macOS, and Ubuntu runners.
- The locked size-focused software-renderer build passed the hidden optimized smoke test in 1,606
  milliseconds.

## Windows artifact

- Source commit: `0f070be96090980baedc3a82c4684e117410d1ae`.
- Windows x64 executable: 9,677,824 bytes (about 9.23 MiB).
- SHA-256: `38AF002424C1C42E298943C6773396FF988B13AAF7FCD217EF0E3942BD182C1B`.
- Increase from the Phase 1 profile package: 101,376 bytes (about 0.10 MiB).
- Published at `dist/AutoQuill-windows-x64.exe` and preserved in the gitignored `local-builds`
  backup before cleanup.
- Cleanup removed 8,357 Cargo artifact files totaling 3.8 GiB; the verified executables were kept.
- The executable is portable and unsigned; code signing remains release-hardening work.
