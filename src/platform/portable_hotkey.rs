//! macOS/X11 global-shortcut adapter. Wayland is rejected before manager initialization.

use std::{
    cell::RefCell,
    fmt,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, Sender},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use global_hotkey::{
    GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState,
    hotkey::{Code, HotKey, Modifiers},
};

use crate::domain::{ModifierSet, Shortcut, ShortcutKey};

use super::{HotkeyEvent, NativeInputError, current_capabilities};

#[derive(Debug, Default)]
struct HotkeyIds {
    primary: u32,
    escape: Option<u32>,
}

pub struct HotkeyService {
    manager: GlobalHotKeyManager,
    primary: RefCell<(Shortcut, HotKey)>,
    escape: RefCell<Option<HotKey>>,
    ids: Arc<Mutex<HotkeyIds>>,
    event_sender: Sender<HotkeyEvent>,
    stop: Arc<AtomicBool>,
    event_thread: Option<JoinHandle<()>>,
}

impl fmt::Debug for HotkeyService {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PortableHotkeyService")
            .field("shortcut", &self.primary.borrow().0)
            .field("escape_active", &self.escape.borrow().is_some())
            .finish_non_exhaustive()
    }
}

impl HotkeyService {
    pub fn start(shortcut: Shortcut) -> Result<(Self, Receiver<HotkeyEvent>), NativeInputError> {
        if !current_capabilities().global_shortcuts_available {
            return Err(NativeInputError::Unsupported(
                "Global shortcuts are unavailable in this desktop session. Simulation remains available from the window controls."
                    .into(),
            ));
        }

        let primary = native_hotkey(shortcut)?;
        let manager = GlobalHotKeyManager::new().map_err(|error| {
            NativeInputError::HotkeyUnavailable(format!(
                "The native global-shortcut manager could not start ({error})."
            ))
        })?;
        manager.register(primary).map_err(|error| {
            NativeInputError::HotkeyUnavailable(format!(
                "{shortcut} could not be registered by this desktop ({error})."
            ))
        })?;

        let (event_sender, event_receiver) = mpsc::channel();
        let _ = event_sender.send(HotkeyEvent::Registered(shortcut));
        let ids = Arc::new(Mutex::new(HotkeyIds {
            primary: primary.id(),
            escape: None,
        }));
        let stop = Arc::new(AtomicBool::new(false));
        let event_thread = Some(spawn_event_thread(
            Arc::clone(&ids),
            Arc::clone(&stop),
            event_sender.clone(),
        )?);

        Ok((
            Self {
                manager,
                primary: RefCell::new((shortcut, primary)),
                escape: RefCell::new(None),
                ids,
                event_sender,
                stop,
                event_thread,
            },
            event_receiver,
        ))
    }

    pub fn set_shortcut(&self, shortcut: Shortcut) -> Result<(), NativeInputError> {
        let replacement = native_hotkey(shortcut)?;
        let (retained_shortcut, retained) = *self.primary.borrow();
        if replacement.id() == retained.id() {
            let _ = self.event_sender.send(HotkeyEvent::Registered(shortcut));
            return Ok(());
        }

        let mut ids = self.ids.lock().map_err(|_| hotkey_state_error())?;

        if let Err(error) = self.manager.register(replacement) {
            let message = format!(
                "{shortcut} is unavailable on this desktop ({error}). {retained_shortcut} remains registered."
            );
            let _ = self.event_sender.send(HotkeyEvent::RegistrationFailed {
                shortcut,
                retained_shortcut: Some(retained_shortcut),
                message: message.clone(),
            });
            return Err(NativeInputError::HotkeyUnavailable(message));
        }

        if let Err(error) = self.manager.unregister(retained) {
            let _ = self.manager.unregister(replacement);
            let message = format!(
                "{shortcut} was not kept because {retained_shortcut} could not be released safely ({error})."
            );
            let _ = self.event_sender.send(HotkeyEvent::RegistrationFailed {
                shortcut,
                retained_shortcut: Some(retained_shortcut),
                message: message.clone(),
            });
            return Err(NativeInputError::HotkeyUnavailable(message));
        }

        *self.primary.borrow_mut() = (shortcut, replacement);
        ids.primary = replacement.id();
        let _ = self.event_sender.send(HotkeyEvent::Registered(shortcut));
        Ok(())
    }

    pub fn set_escape_active(&self, active: bool) -> Result<(), NativeInputError> {
        if active == self.escape.borrow().is_some() {
            return Ok(());
        }

        if active {
            let escape = HotKey::new(None, Code::Escape);
            let mut ids = self.ids.lock().map_err(|_| hotkey_state_error())?;
            if let Err(error) = self.manager.register(escape) {
                let message = format!(
                    "Emergency Escape could not be registered on this desktop ({error}). The primary Start/Stop shortcut remains available."
                );
                let _ = self
                    .event_sender
                    .send(HotkeyEvent::EscapeRegistrationFailed(message.clone()));
                return Err(NativeInputError::HotkeyUnavailable(message));
            }
            *self.escape.borrow_mut() = Some(escape);
            ids.escape = Some(escape.id());
        } else {
            let escape = *self.escape.borrow();
            if let Some(escape) = escape {
                let mut ids = self.ids.lock().map_err(|_| hotkey_state_error())?;
                self.manager.unregister(escape).map_err(|error| {
                    NativeInputError::HotkeyUnavailable(format!(
                        "Emergency Escape could not be released ({error})."
                    ))
                })?;
                *self.escape.borrow_mut() = None;
                ids.escape = None;
            }
        }
        Ok(())
    }
}

impl Drop for HotkeyService {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(thread) = self.event_thread.take() {
            let _ = thread.join();
        }
        if let Some(escape) = self.escape.get_mut().take() {
            let _ = self.manager.unregister(escape);
        }
        let (_, primary) = *self.primary.get_mut();
        let _ = self.manager.unregister(primary);
    }
}

fn spawn_event_thread(
    ids: Arc<Mutex<HotkeyIds>>,
    stop: Arc<AtomicBool>,
    sender: Sender<HotkeyEvent>,
) -> Result<JoinHandle<()>, NativeInputError> {
    let receiver = GlobalHotKeyEvent::receiver().clone();
    thread::Builder::new()
        .name("autoquill-portable-hotkey".into())
        .spawn(move || {
            while !stop.load(Ordering::Acquire) {
                let Ok(event) = receiver.recv_timeout(Duration::from_millis(50)) else {
                    continue;
                };
                if event.state != HotKeyState::Pressed {
                    continue;
                }
                let Ok(ids) = ids.lock() else {
                    break;
                };
                let translated = if event.id == ids.primary {
                    Some(HotkeyEvent::Pressed)
                } else if ids.escape == Some(event.id) {
                    Some(HotkeyEvent::EmergencyStop)
                } else {
                    None
                };
                drop(ids);
                if translated.is_some_and(|event| sender.send(event).is_err()) {
                    break;
                }
            }
        })
        .map_err(|error| {
            NativeInputError::HotkeyUnavailable(format!(
                "The global-shortcut event service could not start ({error})."
            ))
        })
}

fn hotkey_state_error() -> NativeInputError {
    NativeInputError::HotkeyUnavailable(
        "The global-shortcut state could not be updated safely.".into(),
    )
}

fn native_hotkey(shortcut: Shortcut) -> Result<HotKey, NativeInputError> {
    if matches!(shortcut.key, ShortcutKey::Character(_) | ShortcutKey::Space)
        && shortcut.modifiers.is_empty()
    {
        return Err(NativeInputError::HotkeyUnavailable(
            "Letter, symbol, and Space shortcuts require a modifier so normal typing remains usable."
                .into(),
        ));
    }

    let mut modifiers = native_modifiers(shortcut.modifiers);
    let (code, implicit_shift) = match shortcut.key {
        ShortcutKey::Function(number) => (function_code(number)?, false),
        ShortcutKey::Space => (Code::Space, false),
        ShortcutKey::Character(character) => character_code(character)?,
    };
    if implicit_shift {
        modifiers.insert(Modifiers::SHIFT);
    }
    let modifiers = (!modifiers.is_empty()).then_some(modifiers);
    Ok(HotKey::new(modifiers, code))
}

fn native_modifiers(modifiers: ModifierSet) -> Modifiers {
    let mut native = Modifiers::empty();
    if modifiers.control {
        native.insert(Modifiers::CONTROL);
    }
    if modifiers.alt {
        native.insert(Modifiers::ALT);
    }
    if modifiers.shift {
        native.insert(Modifiers::SHIFT);
    }
    if modifiers.meta {
        native.insert(Modifiers::SUPER);
    }
    native
}

fn function_code(number: u8) -> Result<Code, NativeInputError> {
    Ok(match number {
        1 => Code::F1,
        2 => Code::F2,
        3 => Code::F3,
        4 => Code::F4,
        5 => Code::F5,
        6 => Code::F6,
        7 => Code::F7,
        8 => Code::F8,
        9 => Code::F9,
        10 => Code::F10,
        11 => Code::F11,
        12 => Code::F12,
        _ => {
            return Err(NativeInputError::HotkeyUnavailable(
                "The activation function key must be between F1 and F12.".into(),
            ));
        }
    })
}

#[allow(clippy::too_many_lines)]
fn character_code(character: char) -> Result<(Code, bool), NativeInputError> {
    let mapping = match character.to_ascii_uppercase() {
        'A' => (Code::KeyA, false),
        'B' => (Code::KeyB, false),
        'C' => (Code::KeyC, false),
        'D' => (Code::KeyD, false),
        'E' => (Code::KeyE, false),
        'F' => (Code::KeyF, false),
        'G' => (Code::KeyG, false),
        'H' => (Code::KeyH, false),
        'I' => (Code::KeyI, false),
        'J' => (Code::KeyJ, false),
        'K' => (Code::KeyK, false),
        'L' => (Code::KeyL, false),
        'M' => (Code::KeyM, false),
        'N' => (Code::KeyN, false),
        'O' => (Code::KeyO, false),
        'P' => (Code::KeyP, false),
        'Q' => (Code::KeyQ, false),
        'R' => (Code::KeyR, false),
        'S' => (Code::KeyS, false),
        'T' => (Code::KeyT, false),
        'U' => (Code::KeyU, false),
        'V' => (Code::KeyV, false),
        'W' => (Code::KeyW, false),
        'X' => (Code::KeyX, false),
        'Y' => (Code::KeyY, false),
        'Z' => (Code::KeyZ, false),
        '0' => (Code::Digit0, false),
        '1' => (Code::Digit1, false),
        '2' => (Code::Digit2, false),
        '3' => (Code::Digit3, false),
        '4' => (Code::Digit4, false),
        '5' => (Code::Digit5, false),
        '6' => (Code::Digit6, false),
        '7' => (Code::Digit7, false),
        '8' => (Code::Digit8, false),
        '9' => (Code::Digit9, false),
        '`' => (Code::Backquote, false),
        '-' => (Code::Minus, false),
        '=' => (Code::Equal, false),
        '[' => (Code::BracketLeft, false),
        ']' => (Code::BracketRight, false),
        '\\' => (Code::Backslash, false),
        ';' => (Code::Semicolon, false),
        '\'' => (Code::Quote, false),
        ',' => (Code::Comma, false),
        '.' => (Code::Period, false),
        '/' => (Code::Slash, false),
        '~' => (Code::Backquote, true),
        '!' => (Code::Digit1, true),
        '@' => (Code::Digit2, true),
        '#' => (Code::Digit3, true),
        '$' => (Code::Digit4, true),
        '%' => (Code::Digit5, true),
        '^' => (Code::Digit6, true),
        '&' => (Code::Digit7, true),
        '*' => (Code::Digit8, true),
        '(' => (Code::Digit9, true),
        ')' => (Code::Digit0, true),
        '_' => (Code::Minus, true),
        '+' => (Code::Equal, true),
        '{' => (Code::BracketLeft, true),
        '}' => (Code::BracketRight, true),
        '|' => (Code::Backslash, true),
        ':' => (Code::Semicolon, true),
        '"' => (Code::Quote, true),
        '<' => (Code::Comma, true),
        '>' => (Code::Period, true),
        '?' => (Code::Slash, true),
        _ => {
            return Err(NativeInputError::HotkeyUnavailable(format!(
                "{character} is not available as a portable global shortcut."
            )));
        }
    };
    Ok(mapping)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn function_shortcuts_and_modified_characters_are_supported() {
        let function = Shortcut::new(ModifierSet::default(), ShortcutKey::Function(12)).unwrap();
        assert!(native_hotkey(function).is_ok());

        let modified = Shortcut::new(
            ModifierSet {
                control: true,
                shift: true,
                ..ModifierSet::default()
            },
            ShortcutKey::Character('Q'),
        )
        .unwrap();
        assert!(native_hotkey(modified).is_ok());
    }

    #[test]
    fn bare_typing_keys_and_non_portable_characters_are_refused() {
        let bare = Shortcut::new(ModifierSet::default(), ShortcutKey::Character('Q')).unwrap();
        assert!(native_hotkey(bare).is_err());

        let unicode = Shortcut::new(
            ModifierSet {
                control: true,
                ..ModifierSet::default()
            },
            ShortcutKey::Character('é'),
        );
        assert!(unicode.is_err());
    }
}
