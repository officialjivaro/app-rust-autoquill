//! Bounded, privacy-safe diagnostics for user-initiated copy and export actions.

use crate::{APP_DISPLAY_NAME, APP_VERSION, platform::CapabilityReport};

/// Build a small support report without reading profiles, clipboard data, typed text, or logs.
#[must_use]
pub fn privacy_safe_report(
    capabilities: CapabilityReport,
    theme: &str,
    reduce_motion: bool,
) -> String {
    format!(
        "{APP_DISPLAY_NAME} diagnostics\n\
         Version: {APP_VERSION}\n\
         Operating system: {}\n\
         Architecture: {}\n\
         Detected platform: {:?}\n\
         Capability state: {:?}\n\
         Verification level: {:?}\n\
         Real Typing available: {}\n\
         Global shortcuts available: {}\n\
         Sticky Background available: {}\n\
         Theme preference: {theme}\n\
         Reduced motion: {reduce_motion}\n\
         Data root: ~/Jivaro/AutoQuill\n\
         Privacy: typed text, clipboard contents, profile names, and injected characters are not included.\n",
        std::env::consts::OS,
        std::env::consts::ARCH,
        capabilities.platform,
        capabilities.state,
        capabilities.verification,
        capabilities.real_typing_available,
        capabilities.global_shortcuts_available,
        capabilities.sticky_background_available,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::{PlatformKind, report_for};

    #[test]
    fn report_is_bounded_and_contains_no_user_content_fields() {
        let report =
            privacy_safe_report(report_for(PlatformKind::LinuxWayland, true), "system", true);
        assert!(report.len() < 2_048);
        assert!(report.contains("LINUX") || report.contains("Linux"));
        assert!(report.contains("typed text, clipboard contents"));
        assert!(!report.contains("USERPROFILE"));
        assert!(!report.contains("HOME="));
    }
}
