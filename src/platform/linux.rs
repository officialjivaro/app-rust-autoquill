//! Compile-gated Linux X11 foreground backend with explicit Wayland refusal.

use std::{cell::RefCell, fmt};

use x11rb::{
    connection::Connection,
    protocol::xproto::{Atom, AtomEnum, ConnectionExt, Window},
    rust_connection::RustConnection,
};

use crate::{
    domain::TargetIntent,
    typing::{Instruction, PreviewOperation},
};

use super::{
    ForegroundTarget, NativeInputError, PlatformKind, classify_linux_session,
    portable_input::NativeKeyboard,
};

#[must_use]
pub(super) fn session_kind() -> PlatformKind {
    let session_type = std::env::var("XDG_SESSION_TYPE").ok();
    let display = std::env::var("DISPLAY").ok();
    let wayland_display = std::env::var("WAYLAND_DISPLAY").ok();
    classify_linux_session(
        session_type.as_deref(),
        display.as_deref(),
        wayland_display.as_deref(),
    )
}

#[derive(Default)]
pub struct ForegroundBackend {
    keyboard: NativeKeyboard,
    target_reader: RefCell<Option<X11TargetReader>>,
}

impl fmt::Debug for ForegroundBackend {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("LinuxX11ForegroundBackend")
            .field("keyboard", &self.keyboard)
            .field(
                "target_reader_initialized",
                &self.target_reader.borrow().is_some(),
            )
            .finish()
    }
}

impl ForegroundBackend {
    #[must_use]
    pub fn available() -> bool {
        session_kind() == PlatformKind::LinuxX11
    }

    pub fn capture_foreground(&self) -> Result<ForegroundTarget, NativeInputError> {
        self.capture(TargetIntent::Foreground, &[])
    }

    pub fn capture(
        &self,
        _intent: TargetIntent,
        _instructions: &[Instruction],
    ) -> Result<ForegroundTarget, NativeInputError> {
        require_x11()?;
        self.keyboard.ensure_ready()?;
        let target = self.with_reader(X11TargetReader::active_target)?;
        if target.process_id == std::process::id() {
            return Err(NativeInputError::OwnWindow);
        }
        Ok(ForegroundTarget::platform_foreground(
            target.window as usize,
            target.process_id,
            target.label,
        ))
    }

    pub fn validate(&self, target: &ForegroundTarget) -> Result<(), NativeInputError> {
        require_x11()?;
        let active = self
            .with_reader(X11TargetReader::active_target)
            .map_err(|_| NativeInputError::TargetClosed)?;
        if active.window as usize != target.platform_handle()
            || active.process_id != target.platform_process_id()
        {
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

    fn with_reader<T>(
        &self,
        operation: impl FnOnce(&X11TargetReader) -> Result<T, NativeInputError>,
    ) -> Result<T, NativeInputError> {
        if self.target_reader.borrow().is_none() {
            *self.target_reader.borrow_mut() = Some(X11TargetReader::connect()?);
        }
        let reader = self.target_reader.borrow();
        operation(reader.as_ref().ok_or_else(|| {
            NativeInputError::InputFailed("The X11 target reader could not initialize.".into())
        })?)
    }
}

fn require_x11() -> Result<(), NativeInputError> {
    match session_kind() {
        PlatformKind::LinuxX11 => Ok(()),
        PlatformKind::LinuxWayland => Err(NativeInputError::Unsupported(
            "Wayland Real Typing is disabled until its permission-mediated portal path can be tested on native Linux desktops. Simulation remains available."
                .into(),
        )),
        _ => Err(NativeInputError::Unsupported(
            "AutoQuill could not confirm a native X11 session. Simulation remains available."
                .into(),
        )),
    }
}

struct X11TargetReader {
    connection: RustConnection,
    root: Window,
    active_window_atom: Atom,
    process_id_atom: Atom,
    window_name_atom: Atom,
    utf8_string_atom: Atom,
}

impl X11TargetReader {
    fn connect() -> Result<Self, NativeInputError> {
        let (connection, screen_index) = x11rb::connect(None).map_err(x11_error)?;
        let root = connection
            .setup()
            .roots
            .get(screen_index)
            .ok_or_else(|| {
                NativeInputError::InputFailed("The X11 screen could not be selected.".into())
            })?
            .root;
        let active_window_atom = intern(&connection, b"_NET_ACTIVE_WINDOW")?;
        let process_id_atom = intern(&connection, b"_NET_WM_PID")?;
        let window_name_atom = intern(&connection, b"_NET_WM_NAME")?;
        let utf8_string_atom = intern(&connection, b"UTF8_STRING")?;
        Ok(Self {
            connection,
            root,
            active_window_atom,
            process_id_atom,
            window_name_atom,
            utf8_string_atom,
        })
    }

    fn active_target(&self) -> Result<X11Target, NativeInputError> {
        let active_reply = self
            .connection
            .get_property(
                false,
                self.root,
                self.active_window_atom,
                AtomEnum::WINDOW,
                0,
                1,
            )
            .map_err(x11_error)?
            .reply()
            .map_err(x11_error)?;
        let window = active_reply
            .value32()
            .and_then(|mut values| values.next())
            .filter(|window| *window != 0)
            .ok_or(NativeInputError::NoTarget)?;

        let process_reply = self
            .connection
            .get_property(
                false,
                window,
                self.process_id_atom,
                AtomEnum::CARDINAL,
                0,
                1,
            )
            .map_err(x11_error)?
            .reply()
            .map_err(x11_error)?;
        let process_id = process_reply
            .value32()
            .and_then(|mut values| values.next())
            .filter(|process_id| *process_id != 0)
            .ok_or_else(|| {
                NativeInputError::InputFailed(
                    "The active X11 window did not expose a stable process identity, so Real Typing was refused."
                        .into(),
                )
            })?;

        let label = self
            .window_label(window)
            .unwrap_or_else(|| format!("X11 window {window}"));
        Ok(X11Target {
            window,
            process_id,
            label,
        })
    }

    fn window_label(&self, window: Window) -> Option<String> {
        let reply = self
            .connection
            .get_property(
                false,
                window,
                self.window_name_atom,
                self.utf8_string_atom,
                0,
                512,
            )
            .ok()?
            .reply()
            .ok()?;
        let label = String::from_utf8_lossy(&reply.value).trim().to_owned();
        (!label.is_empty()).then_some(label)
    }
}

struct X11Target {
    window: Window,
    process_id: u32,
    label: String,
}

fn intern(connection: &RustConnection, name: &[u8]) -> Result<Atom, NativeInputError> {
    connection
        .intern_atom(false, name)
        .map_err(x11_error)?
        .reply()
        .map(|reply| reply.atom)
        .map_err(x11_error)
}

fn x11_error(error: impl fmt::Display) -> NativeInputError {
    NativeInputError::InputFailed(format!(
        "The X11 capability check failed ({error}). Simulation remains available."
    ))
}
