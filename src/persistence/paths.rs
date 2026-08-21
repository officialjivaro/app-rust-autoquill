//! Canonical AutoQuill user-data paths.

use std::{env, io, path::PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataPaths {
    pub root: PathBuf,
    pub data: PathBuf,
    pub saves: PathBuf,
    pub backups: PathBuf,
    pub trash: PathBuf,
    pub preferences: PathBuf,
}

impl DataPaths {
    #[must_use]
    pub fn from_home(home: impl Into<PathBuf>) -> Self {
        Self::from_root(home.into().join("Jivaro").join("AutoQuill"))
    }

    #[must_use]
    pub fn from_root(root: impl Into<PathBuf>) -> Self {
        let root = root.into();
        let data = root.join("Data");
        Self {
            saves: data.join("Saves"),
            backups: data.join("Backups"),
            trash: data.join("Trash"),
            preferences: data.join("preferences.json"),
            data,
            root,
        }
    }

    pub fn discover() -> io::Result<Self> {
        let variable = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
        let home = env::var_os(variable)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::NotFound,
                    format!("Could not locate the user home directory from {variable}."),
                )
            })?;
        Ok(Self::from_home(PathBuf::from(home)))
    }

    pub fn ensure_directories(&self) -> io::Result<()> {
        std::fs::create_dir_all(&self.saves)?;
        std::fs::create_dir_all(&self.backups)?;
        std::fs::create_dir_all(&self.trash)
    }
}
