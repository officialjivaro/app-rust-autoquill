# AutoQuill 0.18 cross-platform release-candidate package

Status: implemented source package; final artifact hashes are recorded after the opt-in native
runner completes

## What this package adds

- Modern quill-and-caret icon assets for Windows, macOS, Linux, the window, and the tray.
- Dark, Light, and System themes with persisted reduced-motion behavior.
- Best-effort Show, Start/Stop, and Exit tray actions. Closing the window exits normally if the
  desktop cannot host a tray icon.
- Privacy-safe Copy/Export Diagnostics. The report never reads typed text, clipboard contents,
  profile names, profile bodies, or emitted input.
- Windows x64 EXE packaging, unsigned Universal 2 macOS app/DMG packaging, and Linux x64 raw plus
  AppImage packaging.
- Per-platform checksums, manifests, architecture/dependency inspection, and a three-version local
  build archive.
- Release builds remap runner home paths and packaging refuses binaries that still expose one.

## Verification boundary

Windows retains its locally exercised Real Typing beta boundary. macOS and Linux remain
**unverified previews** even when all automated checks pass:

| Platform | Automated evidence | Evidence that is unavailable |
|---|---|---|
| macOS Universal 2 | Rust tests, ARM/Intel builds, `lipo`, `otool`, plist/icon checks, simulation smoke, archive and DMG verification | Physical Mac Accessibility prompts, TCC persistence, TextEdit/Safari input, native shortcuts, sleep/wake, Gatekeeper user flow |
| Linux X11 x64 | Rust tests, native build, Xvfb/openbox simulation smoke, ELF/`ldd` checks, AppImage extraction and smoke | Physical GNOME/KDE input, editor/browser delivery, global shortcut conflicts, sleep/wake |
| Linux Wayland | Capability tests proving Real Typing and shortcuts remain disabled | Portal/libei input is not implemented or advertised |

Logical QC reduces implementation and packaging risk. It cannot establish that another desktop's
permissions, compositor, global shortcut service, or real keyboard target behaves correctly.

## Artifact policy

The source push runs one opt-in packaging matrix when the final commit contains `[platform ci]`.
Artifacts remain downloadable from that Actions run for 14 days and are copied into the local
`dist/platform-builds` directory. No public GitHub Release is created by this phase.

The local `dist/builds/<version>` archive retains the newest three completed versions. Older
versions keep only the platforms that were actually built at the time; no absent platform package
is fabricated retroactively.

## Reusable website warning

> macOS and Linux downloads are unverified experimental previews produced by native CI runners,
> not tested on physical user hardware. The macOS build is unsigned and unnotarized and may be
> blocked by Gatekeeper. Linux Real Typing is experimental on X11; Wayland supports Simulation
> only. Test with a disposable document and never use sensitive fields or important data.
