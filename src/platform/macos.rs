//! Compile-gated macOS foreground backend. Native behavior still requires Mac hardware testing.

use std::fmt;

use objc2_app_kit::NSWorkspace;

use crate::{
    domain::TargetIntent,
    typing::{Instruction, PreviewOperation},
};

use super::{ForegroundTarget, NativeInputError, portable_input::NativeKeyboard};

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn AXIsProcessTrusted() -> bool;
}

#[must_use]
pub(super) fn accessibility_permission_granted() -> bool {
    // SAFETY: AXIsProcessTrusted takes no pointers and only queries the current process's TCC state.
    unsafe { AXIsProcessTrusted() }
}

#[derive(Default)]
pub struct ForegroundBackend {
    keyboard: NativeKeyboard,
}

impl fmt::Debug for ForegroundBackend {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("MacOsForegroundBackend")
            .field("keyboard", &self.keyboard)
            .finish()
    }
}

impl ForegroundBackend {
    #[must_use]
    pub fn available() -> bool {
        accessibility_permission_granted()
    }

    pub fn capture_foreground(&self) -> Result<ForegroundTarget, NativeInputError> {
        self.capture(TargetIntent::Foreground, &[])
    }

    pub fn capture(
        &self,
        _intent: TargetIntent,
        _instructions: &[Instruction],
    ) -> Result<ForegroundTarget, NativeInputError> {
        require_accessibility_permission()?;
        self.keyboard.ensure_ready()?;
        let (process_id, label) = frontmost_application()?;
        if process_id == std::process::id() {
            return Err(NativeInputError::OwnWindow);
        }
        Ok(ForegroundTarget::platform_foreground(
            process_id as usize,
            process_id,
            label,
        ))
    }

    pub fn validate(&self, target: &ForegroundTarget) -> Result<(), NativeInputError> {
        require_accessibility_permission()?;
        let (process_id, _) = frontmost_application()?;
        if process_id != target.platform_process_id() {
            return Err(NativeInputError::TargetChanged);
        }
        Ok(())
    }

    pub fn emit(
        &self,
        target: &ForegroundTarget,
        operation: &PreviewOperation,
    ) -> Result<(), NativeInputError> {
        self.validate(target)?;
        self.keyboard.emit(operation)
    }
}

fn require_accessibility_permission() -> Result<(), NativeInputError> {
    if accessibility_permission_granted() {
        Ok(())
    } else {
        Err(NativeInputError::PermissionRequired(
            "macOS Accessibility permission is not granted. Enable AutoQuill in System Settings → Privacy & Security → Accessibility, then press Re-check."
                .into(),
        ))
    }
}

fn frontmost_application() -> Result<(u32, String), NativeInputError> {
    let workspace = NSWorkspace::sharedWorkspace();
    let application = workspace
        .frontmostApplication()
        .ok_or(NativeInputError::NoTarget)?;
    let process_id = application.processIdentifier();
    let process_id = u32::try_from(process_id).map_err(|_| NativeInputError::NoTarget)?;
    let label = application
        .localizedName()
        .map(|name| name.to_string())
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| format!("macOS application {process_id}"));
    Ok((process_id, label))
}
