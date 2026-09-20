# Distribution artifacts

`AutoQuill-windows-x64.exe` is the unsigned portable Windows x64 release-candidate beta
(`0.19.0-beta.2`).

This build includes safe Simulation, explicitly confirmed Real Typing, Sticky Auto for verified
native edit controls, automatic Foreground Protected fallback for browsers/unknown controls and
background-unsafe documents, recorded F1–F12 or modifier shortcuts, active-session emergency
Escape, transactional conflict fallback, strict target validation, the modern Jivaro icon,
Dark/Light/System themes, reduced motion, privacy-safe diagnostics, tray controls, embedded Windows
metadata, Unicode and special-key input, schema-v2 profiles, and the responsive 1280×720 interface.
Simulation is selected every launch.

The 0.19 UI reliability pass also includes clipped narrow-layout scrolling, modal focus containment,
complete numeric/input accessibility names, a writable editor UI Automation ValuePattern, corrected
dark-theme primary-button contrast, guided empty profile results, and one-time close-to-tray guidance.

Beta.2 adds compact naming/confirmation dialogs, visible validation feedback, preserved parent
overlay state, clearer Profiles and Settings navigation, reliable save/retry flows, and an
unsaved-changes confirmation on tray Exit. Linux X11 also gains focused-child validation and a
packaged launch guide; Wayland remains simulation-only.

The opt-in native-runner workflow also produces an unsigned Universal 2 macOS app/DMG and Linux
x64 raw/AppImage previews. After the run, those unverified packages are downloaded into the local
`dist/platform-builds` folder rather than committed to Git. Verify this Windows executable against
`SHA256SUMS.txt`.
