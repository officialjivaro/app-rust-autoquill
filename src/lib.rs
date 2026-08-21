//! Portable application metadata and future domain entry points.
//!
//! The crate is deliberately small in Phase 0. Typing behavior will be added behind portable
//! interfaces so that the UI and platform integrations remain separate.

pub mod domain;
pub mod persistence;
pub mod platform;
pub mod typing;

/// Name shown throughout the user interface.
pub const APP_DISPLAY_NAME: &str = "AutoQuill";

/// Stable reverse-domain identifier for packaging and platform integration.
pub const APP_ID: &str = "net.jivaro.autoquill";

/// Version compiled from Cargo package metadata.
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Enable lightweight development diagnostics without creating user data or release log files.
pub fn initialize_diagnostics() {
    #[cfg(debug_assertions)]
    eprintln!("{APP_DISPLAY_NAME} {APP_VERSION} development diagnostics enabled");
}

/// Return the title used for the primary application window.
#[must_use]
pub fn window_title() -> String {
    format!("{APP_DISPLAY_NAME} {APP_VERSION}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn application_id_is_stable_and_reverse_domain_shaped() {
        assert_eq!(APP_ID, "net.jivaro.autoquill");
        assert_eq!(APP_ID.split('.').count(), 3);
    }

    #[test]
    fn window_title_contains_name_and_version() {
        let title = window_title();
        assert!(title.contains(APP_DISPLAY_NAME));
        assert!(title.contains(APP_VERSION));
    }
}
