# AutoQuill for Linux

Experimental X11 beta for Ubuntu 22.04 or newer and compatible x86_64 distributions.
The AppImage is the recommended single-file download. The raw executable and tar archive
are alternatives for systems with the required libraries installed.

## Launch

In your file manager, open the AppImage's properties, allow executing it as a program,
then open it. From a terminal in the download folder:

```sh
chmod +x AutoQuill-0.19.0-beta.2-linux-x64.AppImage
./AutoQuill-0.19.0-beta.2-linux-x64.AppImage
```

If AppImage reports a missing FUSE runtime, use its extraction-and-run option:

```sh
APPIMAGE_EXTRACT_AND_RUN=1 ./AutoQuill-0.19.0-beta.2-linux-x64.AppImage
```

The AppImage needs a working desktop session. Profile file pickers require an XDG desktop
portal and the appropriate portal backend for your desktop. If import/export does not open
a picker, check those desktop packages. Your saves remain in
`~/Jivaro/AutoQuill/Data/Saves`; imports are always deliberate.

## Type into another application

1. Write or import a profile, then preview it in Simulation.
2. Check the session badge beside the mode selector or open Settings for platform guidance.
3. In an X11 session, choose Real Typing and accept the experimental beta confirmation.
4. Focus a disposable document in another application and press your recorded Start/Stop
   shortcut. The same shortcut stops typing; Escape is an additional emergency stop.

Typing stops if the active X11 window or focused X11 child control changes. Focus changes
inside a single application-managed surface (for example, between browser fields) may not
create a distinct X11 child, so keep the destination still during a run. Background typing
is unavailable. If a shortcut conflicts with your desktop, record a different combination.

Wayland supports Simulation only, including when XWayland exposes a DISPLAY variable.
For experimental Real Typing, save your work, sign out, and choose an X11 session such as
Ubuntu on Xorg if your desktop provides one. Otherwise continue with Simulation.

## Verification and closing

Builds receive automated native-runner and virtual X11 checks. Physical GNOME/KDE input,
desktop shortcut conflicts, suspend/resume, and distribution-specific integration remain
unverified. Use disposable documents during evaluation.

Closing the window may keep AutoQuill in a supported desktop tray; use the tray's Exit
action to quit. Desktops without a compatible tray host close normally.
