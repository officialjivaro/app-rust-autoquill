# Dependency safety note

## 2026-08-21 clean-build incident

During a clean Windows build, the build script reached through `arrayref 0.3.10` to a package named
`proc-macro1 1.0.107` and attempted to download executable content from a raw IP address. The
request failed, the build was stopped, and no downloaded payload was run.

AutoQuill now pins `arrayref` exactly to `0.3.9`. The lockfile was regenerated so `proc-macro1` and
its downloader-related dependency path are absent. A new locked build then completed successfully,
including all tests.

This note records the behavior directly observed in the build output; it does not depend on a
public advisory or make a broader claim about crates that were not present in that dependency path.

## Verification before changing the pin

1. Inspect the candidate package's published build script and dependency diff.
2. Run `cargo tree -i arrayref` and confirm the complete reverse dependency path.
3. Build with `--locked` in an isolated clean environment.
4. Reject any unexplained network access or executable download during compilation.
5. Commit `Cargo.toml` and `Cargo.lock` together.

## 2026-08-21 profile file dialogs

Profile import, folder selection, and export use exactly pinned `rfd 0.17.2`. Unused default
features are disabled. Windows and macOS use their native dialogs; Linux enables only the XDG
desktop-portal backend. The cross-platform lockfile adds `rfd` and the Linux-only `pollster` helper.
`rfd` has a small build script that validates the selected Linux backend and requests AppKit
linking on macOS; inspection confirmed that it performs no network or executable-download work.
Re-run the dependency-tree and locked cross-platform CI checks before changing this pin.

## 2026-08-24 compile-only platform input dependencies

The macOS and Linux X11 source backends pin `enigo 0.6.1` and `global-hotkey 0.8.0` in target-specific
dependency sections, so the verified Windows dependency graph and executable do not include them.
Linux enables only Enigo's X11 backend; no Wayland/libei feature is enabled. The Linux target also
pins `x11rb 0.13.2`, while macOS pins only the `objc2-app-kit 0.3.2` features needed to identify the
frontmost application. Default features remain disabled where the selected crate permits it.

The selected input and shortcut crates are compile-only experiments until native hardware tests
cover permissions, event-loop behavior, target loss, Unicode, special keys, and shortcut conflicts.
Re-run locked native platform builds and review feature trees before changing these pins.
