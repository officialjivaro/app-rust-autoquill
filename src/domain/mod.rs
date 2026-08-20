//! Portable domain models shared by the UI, typing engine, persistence, and platform backends.

mod profile;
mod session;
mod settings;
mod shortcut;
mod warning;

pub use profile::{CURRENT_PROFILE_SCHEMA_VERSION, Profile, ProfileMetadata, ProfileSource};
pub use session::SessionState;
pub use settings::{
    BreakSettings, ErrorSettings, LoopSettings, PauseSettings, SettingsDraft, StopAfterSettings,
    TargetIntent, TypingSettings, Wpm,
};
pub use shortcut::{ModifierSet, Shortcut, ShortcutError, ShortcutKey};
pub use warning::{SettingsWarning, WarningCode, collect_warnings};
