# Local build archive

`scripts/archive-builds.ps1` stores completed, versioned build sets here and retains the newest
three. The binaries are intentionally ignored by Git so old installers and executables remain
available on the development machine without permanently inflating repository history.

Versions before cross-platform packaging contain only the Windows executable that existed at the
time. Starting with `0.18.0-beta.1`, a completed build set may also contain the produced macOS and
Linux packages and their manifests.
