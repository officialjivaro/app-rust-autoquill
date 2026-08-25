//! Safe portable profile persistence and explicit legacy import.

mod atomic;
mod format;
mod paths;
mod store;

pub use format::{
    AppearancePreferences, Preferences, ProfileStatus, ThemePreference, WindowPreferences,
};
pub use paths::DataPaths;
pub use store::{
    ImportCandidate, ImportConflictPolicy, ImportReport, ProfileStore, ProfileSummary, StoreError,
    normalize_profile_name,
};
