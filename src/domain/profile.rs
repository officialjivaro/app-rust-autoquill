//! Portable profile values. File storage and migration are added in a later work package.

use super::TypingSettings;

/// Schema written by the Rust profile store once a user explicitly saves or upgrades a profile.
pub const CURRENT_PROFILE_SCHEMA_VERSION: u32 = 2;

/// Where an in-memory profile originated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProfileSource {
    Current,
    LegacyV013,
    Imported,
}

/// Metadata shown by the profile manager without coupling it to file paths.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileMetadata {
    pub name: String,
    pub schema_version: u32,
    pub source: ProfileSource,
    pub is_default: bool,
}

/// A complete portable profile held in memory.
#[derive(Debug, Clone, PartialEq)]
pub struct Profile {
    pub metadata: ProfileMetadata,
    pub settings: TypingSettings,
    pub typing_text: String,
}

impl Profile {
    /// Create a new schema-version-2 profile.
    #[must_use]
    pub fn new(
        name: impl Into<String>,
        settings: TypingSettings,
        typing_text: impl Into<String>,
    ) -> Self {
        Self {
            metadata: ProfileMetadata {
                name: name.into(),
                schema_version: CURRENT_PROFILE_SCHEMA_VERSION,
                source: ProfileSource::Current,
                is_default: false,
            },
            settings,
            typing_text: typing_text.into(),
        }
    }

    /// Mark profile data loaded non-destructively from an AutoQuill v0.13 JSON file.
    #[must_use]
    pub fn from_legacy(
        name: impl Into<String>,
        settings: TypingSettings,
        typing_text: impl Into<String>,
    ) -> Self {
        Self {
            metadata: ProfileMetadata {
                name: name.into(),
                schema_version: 1,
                source: ProfileSource::LegacyV013,
                is_default: false,
            },
            settings,
            typing_text: typing_text.into(),
        }
    }

    /// Return whether saving this profile requires the explicit legacy-upgrade flow.
    #[must_use]
    pub fn requires_upgrade(&self) -> bool {
        self.metadata.schema_version < CURRENT_PROFILE_SCHEMA_VERSION
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_profiles_use_the_current_schema() {
        let profile = Profile::new("Draft", TypingSettings::default(), "Hello");
        assert_eq!(
            profile.metadata.schema_version,
            CURRENT_PROFILE_SCHEMA_VERSION
        );
        assert_eq!(profile.metadata.source, ProfileSource::Current);
        assert!(!profile.requires_upgrade());
    }

    #[test]
    fn legacy_profiles_remain_marked_until_explicitly_upgraded() {
        let profile = Profile::from_legacy("Original", TypingSettings::default(), "Hello");
        assert_eq!(profile.metadata.source, ProfileSource::LegacyV013);
        assert!(profile.requires_upgrade());
    }
}
