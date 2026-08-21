//! Recoverable profile-store operations built on validated names and atomic writes.

use std::{
    collections::HashSet,
    error::Error,
    fmt, fs, io,
    path::{Path, PathBuf},
};

use crate::domain::{CURRENT_PROFILE_SCHEMA_VERSION, Profile, ProfileSource};

use super::{
    atomic::write_atomic,
    format::{Preferences, ProfileStatus, inspect_profile, parse_profile, serialize_profile},
    paths::DataPaths,
};

#[derive(Debug)]
pub enum StoreError {
    Io(io::Error),
    InvalidName(String),
    InvalidProfile(String),
    NotFound(String),
    UnsupportedSchema(u32),
    Conflict(String),
}

impl fmt::Display for StoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "{error}"),
            Self::InvalidName(message) => write!(formatter, "invalid profile name: {message}"),
            Self::InvalidProfile(message) => write!(formatter, "invalid profile: {message}"),
            Self::NotFound(name) => write!(formatter, "profile '{name}' was not found"),
            Self::UnsupportedSchema(version) => write!(
                formatter,
                "profile schema {version} is newer than supported schema {CURRENT_PROFILE_SCHEMA_VERSION}"
            ),
            Self::Conflict(name) => write!(formatter, "profile '{name}' already exists"),
        }
    }
}

impl Error for StoreError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}

impl From<io::Error> for StoreError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileSummary {
    pub name: String,
    pub status: ProfileStatus,
    pub is_default: bool,
    pub is_imported: bool,
    pub bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportCandidate {
    pub source: PathBuf,
    pub suggested_name: String,
    pub status: ProfileStatus,
    pub conflicts: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportConflictPolicy {
    KeepBoth,
    Overwrite,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ImportReport {
    pub imported: Vec<String>,
    pub skipped: Vec<String>,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ProfileStore {
    paths: DataPaths,
}

impl ProfileStore {
    pub fn discover() -> Result<Self, StoreError> {
        Self::new(DataPaths::discover()?)
    }

    pub fn new(paths: DataPaths) -> Result<Self, StoreError> {
        paths.ensure_directories()?;
        Ok(Self { paths })
    }

    #[must_use]
    pub fn paths(&self) -> &DataPaths {
        &self.paths
    }

    pub fn list(&self) -> Result<Vec<ProfileSummary>, StoreError> {
        let preferences = self.load_preferences()?;
        let imported: HashSet<&str> = preferences
            .imported_profiles
            .iter()
            .map(String::as_str)
            .collect();
        let mut profiles = Vec::new();
        for entry in fs::read_dir(&self.paths.saves)? {
            let entry = entry?;
            let path = entry.path();
            let metadata = fs::symlink_metadata(&path)?;
            if metadata.file_type().is_symlink() || !metadata.is_file() || !is_json(&path) {
                continue;
            }
            let Some(name) = profile_name_from_path(&path) else {
                continue;
            };
            let bytes = fs::read(&path)?;
            profiles.push(ProfileSummary {
                is_default: preferences.default_profile.as_deref() == Some(&name),
                is_imported: imported.contains(name.as_str()),
                name,
                status: inspect_profile(&bytes),
                bytes: metadata.len(),
            });
        }
        profiles.sort_by_key(|profile| profile.name.to_lowercase());
        Ok(profiles)
    }

    pub fn load(&self, name: &str) -> Result<Profile, StoreError> {
        let name = normalize_profile_name(name)?;
        let path = self.profile_path(&name);
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Err(StoreError::NotFound(name));
            }
            Err(error) => return Err(error.into()),
        };
        if let ProfileStatus::Newer(version) = inspect_profile(&bytes) {
            return Err(StoreError::UnsupportedSchema(version));
        }
        let mut profile = parse_profile(&name, &bytes).map_err(StoreError::InvalidProfile)?;
        let preferences = self.load_preferences()?;
        profile.metadata.is_default = preferences.default_profile.as_deref() == Some(&name);
        if profile.metadata.source == ProfileSource::Current
            && preferences
                .imported_profiles
                .iter()
                .any(|item| item == &name)
        {
            profile.metadata.source = ProfileSource::Imported;
        }
        Ok(profile)
    }

    pub fn save(&self, name: &str, profile: &Profile) -> Result<String, StoreError> {
        if profile.requires_upgrade() {
            return Err(StoreError::InvalidProfile(
                "legacy profiles must use the explicit upgrade operation".to_owned(),
            ));
        }
        let name = normalize_profile_name(name)?;
        let bytes = serialize_profile(profile).map_err(StoreError::InvalidProfile)?;
        write_atomic(&self.profile_path(&name), &bytes)?;
        self.remember_loaded(&name)?;
        Ok(name)
    }

    pub fn upgrade_legacy(&self, name: &str, profile: &Profile) -> Result<PathBuf, StoreError> {
        let name = normalize_profile_name(name)?;
        let source = self.profile_path(&name);
        let original = fs::read(&source).map_err(|error| {
            if error.kind() == io::ErrorKind::NotFound {
                StoreError::NotFound(name.clone())
            } else {
                StoreError::Io(error)
            }
        })?;
        if inspect_profile(&original) != ProfileStatus::Legacy {
            return Err(StoreError::InvalidProfile(
                "only legacy profiles can be upgraded".to_owned(),
            ));
        }
        let backup = unique_path(&self.paths.backups, &name, "legacy-backup", "json");
        write_atomic(&backup, &original)?;
        let mut upgraded = profile.clone();
        upgraded.metadata.schema_version = CURRENT_PROFILE_SCHEMA_VERSION;
        upgraded.metadata.source = ProfileSource::Current;
        let bytes = serialize_profile(&upgraded).map_err(StoreError::InvalidProfile)?;
        write_atomic(&source, &bytes)?;
        self.remember_loaded(&name)?;
        Ok(backup)
    }

    pub fn duplicate(&self, source: &str, destination: &str) -> Result<String, StoreError> {
        let source = normalize_profile_name(source)?;
        let destination = normalize_profile_name(destination)?;
        let destination_path = self.profile_path(&destination);
        if destination_path.exists() {
            return Err(StoreError::Conflict(destination));
        }
        let bytes = fs::read(self.profile_path(&source)).map_err(|error| {
            if error.kind() == io::ErrorKind::NotFound {
                StoreError::NotFound(source.clone())
            } else {
                StoreError::Io(error)
            }
        })?;
        write_atomic(&destination_path, &bytes)?;
        let mut preferences = self.load_preferences()?;
        if preferences
            .imported_profiles
            .iter()
            .any(|item| item == &source)
        {
            preferences.imported_profiles.push(destination.clone());
            sort_deduplicate(&mut preferences.imported_profiles);
            self.save_preferences(&preferences)?;
        }
        Ok(destination)
    }

    pub fn rename(&self, source: &str, destination: &str) -> Result<String, StoreError> {
        let source = normalize_profile_name(source)?;
        let destination = normalize_profile_name(destination)?;
        if source == destination {
            return Ok(destination);
        }
        let source_path = self.profile_path(&source);
        let destination_path = self.profile_path(&destination);
        if destination_path.exists() {
            return Err(StoreError::Conflict(destination));
        }
        if !source_path.exists() {
            return Err(StoreError::NotFound(source));
        }
        fs::rename(source_path, destination_path)?;
        let mut preferences = self.load_preferences()?;
        replace_name(&mut preferences.last_profile, &source, &destination);
        replace_name(&mut preferences.default_profile, &source, &destination);
        for name in &mut preferences.imported_profiles {
            if name == &source {
                *name = destination.clone();
            }
        }
        sort_deduplicate(&mut preferences.imported_profiles);
        self.save_preferences(&preferences)?;
        Ok(destination)
    }

    pub fn move_to_trash(&self, name: &str) -> Result<PathBuf, StoreError> {
        let name = normalize_profile_name(name)?;
        let source = self.profile_path(&name);
        if !source.exists() {
            return Err(StoreError::NotFound(name));
        }
        let destination = unique_path(&self.paths.trash, &name, "deleted", "json");
        fs::rename(source, &destination)?;
        let mut preferences = self.load_preferences()?;
        clear_name(&mut preferences.last_profile, &name);
        clear_name(&mut preferences.default_profile, &name);
        preferences.imported_profiles.retain(|item| item != &name);
        self.save_preferences(&preferences)?;
        Ok(destination)
    }

    pub fn set_default(&self, name: Option<&str>) -> Result<(), StoreError> {
        let normalized = name.map(normalize_profile_name).transpose()?;
        if let Some(name) = &normalized
            && !self.profile_path(name).exists()
        {
            return Err(StoreError::NotFound(name.clone()));
        }
        let mut preferences = self.load_preferences()?;
        preferences.default_profile = normalized;
        self.save_preferences(&preferences)
    }

    pub fn remember_loaded(&self, name: &str) -> Result<(), StoreError> {
        let name = normalize_profile_name(name)?;
        let mut preferences = self.load_preferences()?;
        preferences.last_profile = Some(name);
        self.save_preferences(&preferences)
    }

    pub fn startup_profile(&self) -> Result<Option<Profile>, StoreError> {
        let preferences = self.load_preferences()?;
        for name in [preferences.last_profile, preferences.default_profile]
            .into_iter()
            .flatten()
        {
            if let Ok(profile) = self.load(&name) {
                return Ok(Some(profile));
            }
        }
        Ok(None)
    }

    pub fn load_preferences(&self) -> Result<Preferences, StoreError> {
        match fs::read(&self.paths.preferences) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map_err(|error| StoreError::InvalidProfile(error.to_string())),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Preferences::default()),
            Err(error) => Err(error.into()),
        }
    }

    pub fn save_preferences(&self, preferences: &Preferences) -> Result<(), StoreError> {
        let bytes = serde_json::to_vec_pretty(preferences)
            .map_err(|error| StoreError::InvalidProfile(error.to_string()))?;
        write_atomic(&self.paths.preferences, &bytes)?;
        Ok(())
    }

    pub fn preview_import(&self, paths: &[PathBuf]) -> Result<Vec<ImportCandidate>, StoreError> {
        let mut files = Vec::new();
        for path in paths {
            if path.is_dir() {
                for entry in fs::read_dir(path)? {
                    let entry = entry?;
                    let candidate = entry.path();
                    let metadata = fs::symlink_metadata(&candidate)?;
                    if metadata.is_file()
                        && !metadata.file_type().is_symlink()
                        && is_json(&candidate)
                    {
                        files.push(candidate);
                    }
                }
            } else if is_json(path) {
                files.push(path.clone());
            }
        }
        files.sort();
        files.dedup();
        Ok(files
            .into_iter()
            .filter_map(|source| {
                let raw_name = profile_name_from_path(&source)?;
                let suggested_name = normalize_profile_name(&raw_name).ok()?;
                let status = match fs::read(&source) {
                    Ok(bytes) => inspect_profile(&bytes),
                    Err(error) => ProfileStatus::Invalid(error.to_string()),
                };
                let conflicts = self.profile_path(&suggested_name).exists();
                Some(ImportCandidate {
                    source,
                    suggested_name,
                    status,
                    conflicts,
                })
            })
            .collect())
    }

    pub fn import(
        &self,
        candidates: &[ImportCandidate],
        policy: ImportConflictPolicy,
    ) -> Result<ImportReport, StoreError> {
        let mut report = ImportReport::default();
        let mut preferences = self.load_preferences()?;
        for candidate in candidates {
            if matches!(
                candidate.status,
                ProfileStatus::Invalid(_) | ProfileStatus::Newer(_)
            ) {
                report.skipped.push(candidate.source.display().to_string());
                continue;
            }
            let bytes = match fs::read(&candidate.source) {
                Ok(bytes) => bytes,
                Err(error) => {
                    report
                        .errors
                        .push(format!("{}: {error}", candidate.source.display()));
                    continue;
                }
            };
            let name = match policy {
                ImportConflictPolicy::Overwrite => candidate.suggested_name.clone(),
                ImportConflictPolicy::KeepBoth => {
                    self.available_name(&candidate.suggested_name, &report.imported)
                }
            };
            if let Err(error) = write_atomic(&self.profile_path(&name), &bytes) {
                report
                    .errors
                    .push(format!("{}: {error}", candidate.source.display()));
                continue;
            }
            preferences.imported_profiles.push(name.clone());
            report.imported.push(name);
        }
        sort_deduplicate(&mut preferences.imported_profiles);
        self.save_preferences(&preferences)?;
        Ok(report)
    }

    pub fn export(&self, name: &str, destination: &Path) -> Result<(), StoreError> {
        let name = normalize_profile_name(name)?;
        let bytes = fs::read(self.profile_path(&name)).map_err(|error| {
            if error.kind() == io::ErrorKind::NotFound {
                StoreError::NotFound(name.clone())
            } else {
                StoreError::Io(error)
            }
        })?;
        write_atomic(destination, &bytes)?;
        Ok(())
    }

    fn profile_path(&self, name: &str) -> PathBuf {
        self.paths.saves.join(format!("{name}.json"))
    }

    fn available_name(&self, base: &str, reserved: &[String]) -> String {
        if !self.profile_path(base).exists() && !reserved.iter().any(|name| name == base) {
            return base.to_owned();
        }
        for suffix in 2..=10_000 {
            let candidate = format!("{base} ({suffix})");
            if !self.profile_path(&candidate).exists()
                && !reserved.iter().any(|name| name == &candidate)
            {
                return candidate;
            }
        }
        format!("{base} (copy)")
    }
}

pub fn normalize_profile_name(raw: &str) -> Result<String, StoreError> {
    let name = raw.trim();
    if name.is_empty() {
        return Err(StoreError::InvalidName("enter a name".to_owned()));
    }
    if name.chars().count() > 80 {
        return Err(StoreError::InvalidName(
            "use 80 characters or fewer".to_owned(),
        ));
    }
    if name == "." || name == ".." || name.ends_with(['.', ' ']) {
        return Err(StoreError::InvalidName(
            "names cannot be '.', '..', or end with a dot or space".to_owned(),
        ));
    }
    if name
        .chars()
        .any(|character| character.is_control() || r#"<>:"/\|?*"#.contains(character))
    {
        return Err(StoreError::InvalidName(
            "remove control characters and < > : \" / \\ | ? *".to_owned(),
        ));
    }
    let stem = name.split('.').next().unwrap_or(name).to_ascii_uppercase();
    let reserved = matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || stem
            .strip_prefix("COM")
            .or_else(|| stem.strip_prefix("LPT"))
            .and_then(|number| number.parse::<u8>().ok())
            .is_some_and(|number| (1..=9).contains(&number));
    if reserved {
        return Err(StoreError::InvalidName(
            "that name is reserved by Windows".to_owned(),
        ));
    }
    Ok(name.to_owned())
}

fn profile_name_from_path(path: &Path) -> Option<String> {
    path.file_stem()?.to_str().map(str::to_owned)
}

fn is_json(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("json"))
}

fn unique_path(directory: &Path, name: &str, label: &str, extension: &str) -> PathBuf {
    let base = format!("{name}.{label}");
    let first = directory.join(format!("{base}.{extension}"));
    if !first.exists() {
        return first;
    }
    for suffix in 2..=10_000 {
        let candidate = directory.join(format!("{base}.{suffix}.{extension}"));
        if !candidate.exists() {
            return candidate;
        }
    }
    directory.join(format!("{base}.{}.{}", std::process::id(), extension))
}

fn replace_name(value: &mut Option<String>, old: &str, new: &str) {
    if value.as_deref() == Some(old) {
        *value = Some(new.to_owned());
    }
}

fn clear_name(value: &mut Option<String>, name: &str) {
    if value.as_deref() == Some(name) {
        *value = None;
    }
}

fn sort_deduplicate(values: &mut Vec<String>) {
    values.sort_by_key(|value| value.to_lowercase());
    values.dedup();
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};

    use crate::domain::{ProfileSource, SettingsDraft};

    use super::*;

    static TEST_COUNTER: AtomicU64 = AtomicU64::new(1);

    struct TestStore {
        store: ProfileStore,
        root: PathBuf,
    }

    impl TestStore {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!(
                "autoquill-store-{}-{}",
                std::process::id(),
                TEST_COUNTER.fetch_add(1, Ordering::Relaxed)
            ));
            let store = ProfileStore::new(DataPaths::from_root(&root)).unwrap();
            Self { store, root }
        }
    }

    impl Drop for TestStore {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn names_reject_traversal_and_windows_devices() {
        for name in ["", "../escape", "A/B", "CON", "nul.txt", "LPT9"] {
            assert!(normalize_profile_name(name).is_err(), "{name} should fail");
        }
        assert_eq!(
            normalize_profile_name("  My Profile  ").unwrap(),
            "My Profile"
        );
    }

    #[test]
    fn save_load_and_startup_preferences_round_trip() {
        let test = TestStore::new();
        let profile = Profile::new("Daily", SettingsDraft::default().normalize(), "Hi 🌍");
        test.store.save("Daily", &profile).unwrap();
        test.store.set_default(Some("Daily")).unwrap();
        let loaded = test.store.startup_profile().unwrap().unwrap();
        assert_eq!(loaded.typing_text, "Hi 🌍");
        assert!(loaded.metadata.is_default);
        let preferences = test.store.load_preferences().unwrap();
        assert_eq!(preferences.last_profile.as_deref(), Some("Daily"));
        assert_eq!(preferences.default_profile.as_deref(), Some("Daily"));
    }

    #[test]
    fn import_keep_both_preserves_source_bytes_and_does_not_auto_load() {
        let test = TestStore::new();
        let outside = test.root.join("outside");
        fs::create_dir_all(&outside).unwrap();
        let source = outside.join("Draft.json");
        let source_bytes = br#"{"typing_text":"legacy","function_key":"F2","wpm":70}"#;
        fs::write(&source, source_bytes).unwrap();
        fs::write(test.store.profile_path("Draft"), source_bytes).unwrap();
        let candidates = test
            .store
            .preview_import(std::slice::from_ref(&source))
            .unwrap();
        let report = test
            .store
            .import(&candidates, ImportConflictPolicy::KeepBoth)
            .unwrap();
        assert_eq!(report.imported, ["Draft (2)"]);
        assert_eq!(fs::read(&source).unwrap(), source_bytes);
        assert_eq!(
            fs::read(test.store.profile_path("Draft (2)")).unwrap(),
            source_bytes
        );
        assert_eq!(test.store.load_preferences().unwrap().last_profile, None);
        assert_eq!(
            test.store.load("Draft (2)").unwrap().metadata.source,
            ProfileSource::LegacyV013
        );
    }

    #[test]
    fn legacy_upgrade_keeps_an_exact_backup() {
        let test = TestStore::new();
        let original = br#"{"typing_text":"old","function_key":"F3","wpm":42}"#;
        fs::write(test.store.profile_path("Old"), original).unwrap();
        let loaded = test.store.load("Old").unwrap();
        let backup = test.store.upgrade_legacy("Old", &loaded).unwrap();
        assert_eq!(fs::read(backup).unwrap(), original);
        assert_eq!(
            inspect_profile(&fs::read(test.store.profile_path("Old")).unwrap()),
            ProfileStatus::Current
        );
    }

    #[test]
    fn deletion_moves_data_to_trash_and_updates_preferences() {
        let test = TestStore::new();
        let profile = Profile::new("Daily", SettingsDraft::default().normalize(), "recover me");
        test.store.save("Daily", &profile).unwrap();
        test.store.set_default(Some("Daily")).unwrap();
        let trashed = test.store.move_to_trash("Daily").unwrap();
        assert!(fs::read_to_string(trashed).unwrap().contains("recover me"));
        assert!(matches!(
            test.store.load("Daily"),
            Err(StoreError::NotFound(_))
        ));
        let preferences = test.store.load_preferences().unwrap();
        assert_eq!(preferences.last_profile, None);
        assert_eq!(preferences.default_profile, None);
    }
}
