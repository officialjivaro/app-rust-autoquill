//! Pure platform capability classification used by the UI and native backends.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlatformKind {
    Windows,
    MacOs,
    LinuxX11,
    LinuxWayland,
    LinuxUnknown,
    Unsupported,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapabilityState {
    Ready,
    PermissionRequired,
    Unverified,
    Unsupported,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerificationLevel {
    VerifiedBeta,
    LogicalOnly,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CapabilityReport {
    pub platform: PlatformKind,
    pub state: CapabilityState,
    pub verification: VerificationLevel,
    pub real_typing_available: bool,
    pub global_shortcuts_available: bool,
    pub sticky_background_available: bool,
    pub badge: &'static str,
    pub summary: &'static str,
    pub guidance: &'static str,
    pub footer: &'static str,
}

#[must_use]
pub const fn report_for(platform: PlatformKind, permission_granted: bool) -> CapabilityReport {
    match platform {
        PlatformKind::Windows => CapabilityReport {
            platform,
            state: CapabilityState::Ready,
            verification: VerificationLevel::VerifiedBeta,
            real_typing_available: true,
            global_shortcuts_available: true,
            sticky_background_available: true,
            badge: "WINDOWS • VERIFIED BETA",
            summary: "Foreground Protected and safe native-control Sticky Background are available.",
            guidance: "AutoQuill captures the exact external Windows target when the activation shortcut is pressed, waits two seconds, and validates it before every operation.",
            footer: "Windows Real Typing beta • target safety checks remain active.",
        },
        PlatformKind::MacOs if permission_granted => CapabilityReport {
            platform,
            state: CapabilityState::Unverified,
            verification: VerificationLevel::LogicalOnly,
            real_typing_available: true,
            global_shortcuts_available: true,
            sticky_background_available: false,
            badge: "macOS • UNVERIFIED PREVIEW",
            summary: "Accessibility is granted. Foreground typing passed logical and native-runner checks but has not been tested on Mac hardware.",
            guidance: "Use only in a disposable document. AutoQuill validates the foreground application and stops if it changes. Sticky Background is unavailable on macOS.",
            footer: "macOS package is unsigned and not hardware-verified.",
        },
        PlatformKind::MacOs => CapabilityReport {
            platform,
            state: CapabilityState::PermissionRequired,
            verification: VerificationLevel::LogicalOnly,
            real_typing_available: false,
            global_shortcuts_available: true,
            sticky_background_available: false,
            badge: "macOS • ACCESSIBILITY REQUIRED",
            summary: "Real Typing needs Accessibility permission before native input can be armed.",
            guidance: "Open System Settings → Privacy & Security → Accessibility, enable AutoQuill, return here, and press Re-check. This unsigned preview still requires native Mac hardware testing.",
            footer: "Simulation remains available • macOS permission and input are not hardware-verified.",
        },
        PlatformKind::LinuxX11 => CapabilityReport {
            platform,
            state: CapabilityState::Unverified,
            verification: VerificationLevel::LogicalOnly,
            real_typing_available: true,
            global_shortcuts_available: true,
            sticky_background_available: false,
            badge: "LINUX X11 • EXPERIMENTAL BETA",
            summary: "Real Typing and Start/Stop shortcuts are available on X11. Physical Linux desktop testing is still pending.",
            guidance: "Focus a disposable document and press your Start/Stop shortcut. Typing stops if the active window or focused X11 control changes. Background typing is unavailable. AppImage targets Ubuntu 22.04 or newer and compatible x64 systems.",
            footer: "Linux X11 package is experimental and not desktop-verified.",
        },
        PlatformKind::LinuxWayland => CapabilityReport {
            platform,
            state: CapabilityState::Unsupported,
            verification: VerificationLevel::Unavailable,
            real_typing_available: false,
            global_shortcuts_available: false,
            sticky_background_available: false,
            badge: "LINUX WAYLAND • SIMULATION ONLY",
            summary: "Simulation works here. Real Typing and global shortcuts are unavailable in a Wayland session.",
            guidance: "For experimental Real Typing, save your work, sign out, and choose an X11 session (such as Ubuntu on Xorg) if your desktop offers one. Reopen AutoQuill there. Otherwise continue using Simulation.",
            footer: "Wayland detected • no unrestricted native input is attempted.",
        },
        PlatformKind::LinuxUnknown => CapabilityReport {
            platform,
            state: CapabilityState::Unsupported,
            verification: VerificationLevel::Unavailable,
            real_typing_available: false,
            global_shortcuts_available: false,
            sticky_background_available: false,
            badge: "LINUX SESSION • NOT DETECTED",
            summary: "AutoQuill could not confirm an X11 desktop session.",
            guidance: "Continue with Simulation. Start AutoQuill inside a native X11 desktop session to expose the unverified X11 preview.",
            footer: "Linux display session unknown • Simulation remains safe.",
        },
        PlatformKind::Unsupported => CapabilityReport {
            platform,
            state: CapabilityState::Unsupported,
            verification: VerificationLevel::Unavailable,
            real_typing_available: false,
            global_shortcuts_available: false,
            sticky_background_available: false,
            badge: "SIMULATION ONLY",
            summary: "Real Typing is unavailable on this operating system.",
            guidance: "Continue with Simulation. No native input will be attempted.",
            footer: "Simulation is the default every launch • no native input.",
        },
    }
}

#[must_use]
pub fn classify_linux_session(
    session_type: Option<&str>,
    display: Option<&str>,
    wayland_display: Option<&str>,
) -> PlatformKind {
    let session_type = session_type.unwrap_or_default().trim().to_ascii_lowercase();
    if session_type == "wayland" || wayland_display.is_some_and(|value| !value.trim().is_empty()) {
        PlatformKind::LinuxWayland
    } else if display.is_some_and(|value| !value.trim().is_empty())
        && (session_type.is_empty() || session_type == "x11")
    {
        PlatformKind::LinuxX11
    } else {
        PlatformKind::LinuxUnknown
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linux_session_detection_prioritizes_wayland_over_xwayland_display() {
        assert_eq!(
            classify_linux_session(Some("wayland"), Some(":0"), Some("wayland-0")),
            PlatformKind::LinuxWayland
        );
        assert_eq!(
            classify_linux_session(Some("x11"), Some(":0"), None),
            PlatformKind::LinuxX11
        );
        assert_eq!(
            classify_linux_session(None, None, None),
            PlatformKind::LinuxUnknown
        );
    }

    #[test]
    fn only_hardware_verified_windows_report_is_marked_verified() {
        let windows = report_for(PlatformKind::Windows, true);
        let mac = report_for(PlatformKind::MacOs, true);
        let x11 = report_for(PlatformKind::LinuxX11, true);

        assert_eq!(windows.verification, VerificationLevel::VerifiedBeta);
        assert_eq!(mac.verification, VerificationLevel::LogicalOnly);
        assert_eq!(x11.verification, VerificationLevel::LogicalOnly);
        assert!(!mac.sticky_background_available);
        assert!(!x11.sticky_background_available);
    }

    #[test]
    fn missing_macos_permission_never_reports_real_typing_available() {
        let report = report_for(PlatformKind::MacOs, false);
        assert_eq!(report.state, CapabilityState::PermissionRequired);
        assert!(!report.real_typing_available);
        assert!(report.global_shortcuts_available);
    }

    #[test]
    fn wayland_is_explicitly_simulation_only() {
        let report = report_for(PlatformKind::LinuxWayland, true);
        assert_eq!(report.state, CapabilityState::Unsupported);
        assert_eq!(report.verification, VerificationLevel::Unavailable);
        assert!(!report.real_typing_available);
        assert!(!report.global_shortcuts_available);
    }

    #[test]
    fn missing_display_and_non_desktop_sessions_do_not_offer_x11_input() {
        assert_eq!(
            classify_linux_session(Some("x11"), None, None),
            PlatformKind::LinuxUnknown
        );
        assert_eq!(
            classify_linux_session(Some("tty"), Some(":0"), None),
            PlatformKind::LinuxUnknown
        );
        assert_eq!(
            classify_linux_session(None, Some(":0"), None),
            PlatformKind::LinuxX11
        );
        assert_eq!(
            classify_linux_session(Some("x11"), Some(":0"), Some("wayland-0")),
            PlatformKind::LinuxWayland
        );
    }
}
