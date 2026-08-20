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
