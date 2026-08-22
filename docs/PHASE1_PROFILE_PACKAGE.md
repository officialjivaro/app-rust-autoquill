# Phase 1 profile package verification

Date: 2026-08-21

## Delivered

- Schema-version-2 profiles containing draft text and every portable setting.
- Separate last-used/default/import preferences.
- Discovery of existing files in `~/Jivaro/AutoQuill/Data/Saves`.
- Explicit legacy loading and backed-up upgrade; no automatic conversion.
- Multi-file and non-recursive folder import with Keep Both or Overwrite choices.
- Exact-byte export, atomic save, validated names, duplicate, rename, default, search, and
  recoverable deletion to `Data/Trash`.
- Modern Jivaro profile manager, immediate modified state, and Save/Discard/Cancel guards.
- Native Windows/macOS file dialogs and a portal-first Linux dialog backend.

## Windows local gate

```text
cargo fmt --check
cargo clippy --locked --all-targets --no-default-features --features renderer-software -- -D warnings
cargo test --locked --no-default-features --features renderer-software
cargo build --profile release-size --locked --no-default-features --features renderer-software
AUTOQUILL_SMOKE_TEST=1 target/release-size/autoquill.exe
```

Results:

- 52 library tests, 3 executable tests, 1 real-event UI test, and doc tests passed.
- Hidden optimized-build smoke test exited successfully.
- Windows x64 executable: 9,576,448 bytes (about 9.13 MiB).
- SHA-256: `5CEF576A56FAB5E4EFEA03031E2F990C9B9146F6219E6083C44F8550C28E9423`.
- Increase from the previous verified local build: 585,728 bytes (about 0.56 MiB).
- Preserved at `local-builds/AutoQuill-windows-x64.exe` (gitignored).
- Cleanup removed 8,342 Cargo artifact files totaling 3.8 GiB.

The pre-public GitHub Actions verification passed the native optimized Windows, macOS, and Ubuntu
builds plus the Windows format/lint/test job. The verified Windows executable and checksum were
then published to `dist`.
