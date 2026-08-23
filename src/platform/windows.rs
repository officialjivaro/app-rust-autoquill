use std::{
    io,
    mem::size_of,
    ptr,
    sync::mpsc::{self, Receiver, Sender},
    thread::{self, JoinHandle},
};

use windows_sys::Win32::{
    Foundation::{HWND, LPARAM, WPARAM},
    System::Threading::GetCurrentThreadId,
    UI::{
        Input::KeyboardAndMouse::{
            INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP,
            KEYEVENTF_UNICODE, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, MOD_WIN,
            RegisterHotKey, SendInput, UnregisterHotKey, VK_ADD, VK_APPS, VK_BACK, VK_CAPITAL,
            VK_CONTROL, VK_DECIMAL, VK_DELETE, VK_DIVIDE, VK_DOWN, VK_END, VK_ESCAPE, VK_F1,
            VK_HOME, VK_INSERT, VK_LEFT, VK_LWIN, VK_MENU, VK_MULTIPLY, VK_NEXT, VK_NUMLOCK,
            VK_NUMPAD0, VK_PAUSE, VK_PRIOR, VK_RETURN, VK_RIGHT, VK_RWIN, VK_SCROLL, VK_SHIFT,
            VK_SNAPSHOT, VK_SPACE, VK_SUBTRACT, VK_TAB, VK_UP, VkKeyScanW,
        },
        WindowsAndMessaging::{
            GA_ROOT, GUITHREADINFO, GetAncestor, GetClassNameW, GetForegroundWindow,
            GetGUIThreadInfo, GetMessageW, GetWindowTextW, GetWindowThreadProcessId, IsWindow, MSG,
            PM_NOREMOVE, PeekMessageW, PostMessageW, PostThreadMessageW, WM_APP, WM_CHAR,
            WM_HOTKEY, WM_KEYDOWN, WM_KEYUP,
        },
    },
};

use crate::{
    domain::{ModifierSet, Shortcut, ShortcutKey, TargetIntent},
    typing::{Instruction, PreviewOperation, SpecialKey},
};

use super::{
    DeliveryStrategy, ForegroundTarget, HotkeyEvent, NativeInputError,
    instructions_support_background_delivery,
};

const HOTKEY_ID_PRIMARY: i32 = 1;
const HOTKEY_ID_SECONDARY: i32 = 2;
const HOTKEY_ID_ESCAPE: i32 = 3;
const COMMAND_MESSAGE: u32 = WM_APP + 17;

#[derive(Debug, Default)]
pub struct ForegroundBackend;

impl ForegroundBackend {
    #[must_use]
    pub const fn available() -> bool {
        true
    }

    pub fn capture_foreground(&self) -> Result<ForegroundTarget, NativeInputError> {
        self.capture(TargetIntent::Foreground, &[])
    }

    pub fn capture(
        &self,
        intent: TargetIntent,
        instructions: &[Instruction],
    ) -> Result<ForegroundTarget, NativeInputError> {
        let root = unsafe { GetForegroundWindow() };
        if root.is_null() {
            return Err(NativeInputError::NoTarget);
        }

        let (root_thread_id, process_id) = window_identity(root);
        if process_id == std::process::id() {
            return Err(NativeInputError::OwnWindow);
        }

        let input = focused_input_for(root, root_thread_id).unwrap_or(root);
        let (_, input_process_id) = window_identity(input);

        let root_class = window_class(root);
        let input_class = window_class(input);
        let delivery = if intent == TargetIntent::StickyAuto
            && is_background_capable_class(&input_class)
            && instructions_support_background_delivery(instructions)
        {
            DeliveryStrategy::NativeBackground
        } else {
            DeliveryStrategy::ForegroundProtected
        };

        Ok(ForegroundTarget {
            root_handle: root as usize,
            input_handle: input as usize,
            root_process_id: process_id,
            input_process_id,
            root_class,
            input_class,
            label: window_label(root),
            delivery,
        })
    }

    pub fn validate(&self, target: &ForegroundTarget) -> Result<(), NativeInputError> {
        let root = target.root_handle as HWND;
        let input = target.input_handle as HWND;
        validate_identity(root, target.root_process_id, &target.root_class)?;
        validate_identity(input, target.input_process_id, &target.input_class)?;

        if input != root && unsafe { GetAncestor(input, GA_ROOT) } != root {
            return Err(NativeInputError::TargetChanged);
        }

        if target.delivery == DeliveryStrategy::ForegroundProtected {
            if unsafe { GetForegroundWindow() } != root {
                return Err(NativeInputError::TargetChanged);
            }
            if input != root {
                let (thread_id, _) = window_identity(root);
                if focused_input_for(root, thread_id) != Some(input) {
                    return Err(NativeInputError::TargetChanged);
                }
            }
        }
        Ok(())
    }

    pub fn emit(
        &self,
        target: &ForegroundTarget,
        operation: &PreviewOperation,
    ) -> Result<(), NativeInputError> {
        self.validate(target)?;
        match target.delivery {
            DeliveryStrategy::ForegroundProtected => match operation {
                PreviewOperation::Intended(Instruction::Character(character))
                | PreviewOperation::TypoCharacter(character) => send_unicode(*character),
                PreviewOperation::Intended(Instruction::SpecialKey(key)) => send_special(*key),
                PreviewOperation::CorrectionBackspace => send_special(SpecialKey::Backspace),
            },
            DeliveryStrategy::NativeBackground => match operation {
                PreviewOperation::Intended(Instruction::Character(character))
                | PreviewOperation::TypoCharacter(character) => {
                    post_unicode(target.input_handle as HWND, *character)
                }
                PreviewOperation::Intended(Instruction::SpecialKey(key)) => {
                    post_background_special(target.input_handle as HWND, *key)
                }
                PreviewOperation::CorrectionBackspace => {
                    post_background_special(target.input_handle as HWND, SpecialKey::Backspace)
                }
            },
        }
    }
}

fn window_identity(window: HWND) -> (u32, u32) {
    let mut process_id = 0;
    let thread_id = unsafe { GetWindowThreadProcessId(window, &mut process_id) };
    (thread_id, process_id)
}

fn focused_input_for(root: HWND, thread_id: u32) -> Option<HWND> {
    if thread_id == 0 {
        return None;
    }
    let mut info = unsafe { std::mem::zeroed::<GUITHREADINFO>() };
    info.cbSize = size_of::<GUITHREADINFO>() as u32;
    if unsafe { GetGUIThreadInfo(thread_id, &mut info) } == 0 || info.hwndFocus.is_null() {
        return None;
    }
    let focused_root = unsafe { GetAncestor(info.hwndFocus, GA_ROOT) };
    (focused_root == root || info.hwndFocus == root).then_some(info.hwndFocus)
}

fn window_label(window: HWND) -> String {
    let mut buffer = [0u16; 512];
    let length = unsafe { GetWindowTextW(window, buffer.as_mut_ptr(), buffer.len() as i32) };
    if length > 0 {
        String::from_utf16_lossy(&buffer[..length as usize])
    } else {
        "Untitled window".to_owned()
    }
}

fn window_class(window: HWND) -> String {
    let mut buffer = [0u16; 256];
    let length = unsafe { GetClassNameW(window, buffer.as_mut_ptr(), buffer.len() as i32) };
    if length > 0 {
        String::from_utf16_lossy(&buffer[..length as usize])
    } else {
        String::new()
    }
}

fn validate_identity(
    window: HWND,
    expected_process_id: u32,
    expected_class: &str,
) -> Result<(), NativeInputError> {
    if unsafe { IsWindow(window) } == 0 {
        return Err(NativeInputError::TargetClosed);
    }
    let (_, process_id) = window_identity(window);
    if process_id != expected_process_id {
        return Err(NativeInputError::TargetClosed);
    }
    if window_class(window) != expected_class {
        return Err(NativeInputError::TargetChanged);
    }
    Ok(())
}

fn is_background_capable_class(class_name: &str) -> bool {
    let class_name = class_name.to_ascii_lowercase();
    class_name == "edit"
        || class_name.starts_with("richedit")
        || class_name.starts_with("windowsforms10.edit")
}

fn send_unicode(character: char) -> Result<(), NativeInputError> {
    let mut encoded = [0u16; 2];
    let units = character.encode_utf16(&mut encoded);
    let mut inputs = Vec::with_capacity(units.len() * 2);
    for &unit in units.iter() {
        inputs.push(keyboard_input(0, unit, KEYEVENTF_UNICODE));
        inputs.push(keyboard_input(0, unit, KEYEVENTF_UNICODE | KEYEVENTF_KEYUP));
    }
    send_inputs(&inputs)
}

fn send_special(key: SpecialKey) -> Result<(), NativeInputError> {
    let (virtual_key, extended) = virtual_key(key);
    let base_flags = if extended { KEYEVENTF_EXTENDEDKEY } else { 0 };
    send_inputs(&[
        keyboard_input(virtual_key, 0, base_flags),
        keyboard_input(virtual_key, 0, base_flags | KEYEVENTF_KEYUP),
    ])
}

fn keyboard_input(virtual_key: u16, scan_code: u16, flags: u32) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: virtual_key,
                wScan: scan_code,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

fn send_inputs(inputs: &[INPUT]) -> Result<(), NativeInputError> {
    let sent = unsafe {
        SendInput(
            inputs.len() as u32,
            inputs.as_ptr(),
            size_of::<INPUT>() as i32,
        )
    };
    if sent == inputs.len() as u32 {
        return Ok(());
    }
    best_effort_release_after_partial_send(inputs, sent as usize);
    let error = io::Error::last_os_error();
    Err(NativeInputError::InputFailed(format!(
        "Windows accepted {sent} of {} input events ({error}). Real Typing stopped immediately.",
        inputs.len()
    )))
}

fn best_effort_release_after_partial_send(inputs: &[INPUT], sent: usize) {
    if sent == 0 || sent > inputs.len() || sent.is_multiple_of(2) {
        return;
    }
    let keyboard = unsafe { inputs[sent - 1].Anonymous.ki };
    let release = keyboard_input(
        keyboard.wVk,
        keyboard.wScan,
        keyboard.dwFlags | KEYEVENTF_KEYUP,
    );
    unsafe {
        SendInput(1, &release, size_of::<INPUT>() as i32);
    }
}

fn post_unicode(window: HWND, character: char) -> Result<(), NativeInputError> {
    let mut encoded = [0u16; 2];
    for &unit in character.encode_utf16(&mut encoded).iter() {
        post_window_message(window, WM_CHAR, usize::from(unit), 1)?;
    }
    Ok(())
}

fn post_background_special(window: HWND, key: SpecialKey) -> Result<(), NativeInputError> {
    match key {
        SpecialKey::Enter => post_window_message(window, WM_CHAR, usize::from(VK_RETURN), 1),
        SpecialKey::Backspace => post_window_message(window, WM_CHAR, usize::from(VK_BACK), 1),
        SpecialKey::Space => post_window_message(window, WM_CHAR, usize::from(VK_SPACE), 1),
        SpecialKey::Delete
        | SpecialKey::Home
        | SpecialKey::End
        | SpecialKey::PageUp
        | SpecialKey::PageDown
        | SpecialKey::Up
        | SpecialKey::Down
        | SpecialKey::Left
        | SpecialKey::Right => {
            let (virtual_key, _) = virtual_key(key);
            post_window_message(window, WM_KEYDOWN, usize::from(virtual_key), 1)?;
            post_window_message(
                window,
                WM_KEYUP,
                usize::from(virtual_key),
                1 | (1_isize << 30) | (1_isize << 31),
            )
        }
        _ => Err(NativeInputError::InputFailed(
            "This document contains a key that cannot be delivered safely in the background. Real Typing stopped immediately."
                .into(),
        )),
    }
}

fn post_window_message(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> Result<(), NativeInputError> {
    if unsafe { PostMessageW(window, message, wparam, lparam) } != 0 {
        Ok(())
    } else {
        Err(NativeInputError::InputFailed(format!(
            "Windows could not deliver input to the selected target ({}). Real Typing stopped immediately.",
            io::Error::last_os_error()
        )))
    }
}

fn virtual_key(key: SpecialKey) -> (u16, bool) {
    match key {
        SpecialKey::Enter => (VK_RETURN, false),
        SpecialKey::Tab => (VK_TAB, false),
        SpecialKey::Backspace => (VK_BACK, false),
        SpecialKey::Space => (VK_SPACE, false),
        SpecialKey::Escape => (VK_ESCAPE, false),
        SpecialKey::Control => (VK_CONTROL, false),
        SpecialKey::Shift => (VK_SHIFT, false),
        SpecialKey::Alt => (VK_MENU, false),
        SpecialKey::CapsLock => (VK_CAPITAL, false),
        SpecialKey::NumLock => (VK_NUMLOCK, true),
        SpecialKey::ScrollLock => (VK_SCROLL, false),
        SpecialKey::Pause => (VK_PAUSE, false),
        SpecialKey::Insert => (VK_INSERT, true),
        SpecialKey::Delete => (VK_DELETE, true),
        SpecialKey::PrintScreen => (VK_SNAPSHOT, true),
        SpecialKey::Home => (VK_HOME, true),
        SpecialKey::End => (VK_END, true),
        SpecialKey::PageUp => (VK_PRIOR, true),
        SpecialKey::PageDown => (VK_NEXT, true),
        SpecialKey::Up => (VK_UP, true),
        SpecialKey::Down => (VK_DOWN, true),
        SpecialKey::Left => (VK_LEFT, true),
        SpecialKey::Right => (VK_RIGHT, true),
        SpecialKey::LeftWindows => (VK_LWIN, true),
        SpecialKey::RightWindows => (VK_RWIN, true),
        SpecialKey::Applications => (VK_APPS, true),
        SpecialKey::Function(number) => (VK_F1 + u16::from(number.saturating_sub(1)), false),
        SpecialKey::NumpadDigit(number) => (VK_NUMPAD0 + u16::from(number), false),
        SpecialKey::NumpadMultiply => (VK_MULTIPLY, false),
        SpecialKey::NumpadAdd => (VK_ADD, false),
        SpecialKey::NumpadSubtract => (VK_SUBTRACT, false),
        SpecialKey::NumpadDecimal => (VK_DECIMAL, false),
        SpecialKey::NumpadDivide => (VK_DIVIDE, true),
    }
}

enum HotkeyCommand {
    Set(Shortcut),
    SetEscape(bool),
    Shutdown,
}

#[derive(Debug)]
pub struct HotkeyService {
    command_sender: Sender<HotkeyCommand>,
    thread_id: u32,
    thread: Option<JoinHandle<()>>,
}

impl HotkeyService {
    pub fn start(shortcut: Shortcut) -> Result<(Self, Receiver<HotkeyEvent>), NativeInputError> {
        native_hotkey(shortcut)?;
        let (event_sender, event_receiver) = mpsc::channel();
        let (command_sender, command_receiver) = mpsc::channel();
        let (ready_sender, ready_receiver) = mpsc::sync_channel(1);

        let thread = thread::Builder::new()
            .name("autoquill-hotkey".into())
            .spawn(move || hotkey_thread(shortcut, event_sender, command_receiver, ready_sender))
            .map_err(|error| NativeInputError::HotkeyUnavailable(error.to_string()))?;

        let thread_id = ready_receiver.recv().map_err(|_| {
            NativeInputError::HotkeyUnavailable(
                "The activation-shortcut service could not initialize.".into(),
            )
        })?;

        Ok((
            Self {
                command_sender,
                thread_id,
                thread: Some(thread),
            },
            event_receiver,
        ))
    }

    pub fn set_shortcut(&self, shortcut: Shortcut) -> Result<(), NativeInputError> {
        native_hotkey(shortcut)?;
        self.send_command(HotkeyCommand::Set(shortcut))
    }

    pub fn set_escape_active(&self, active: bool) -> Result<(), NativeInputError> {
        self.send_command(HotkeyCommand::SetEscape(active))
    }

    fn send_command(&self, command: HotkeyCommand) -> Result<(), NativeInputError> {
        self.command_sender.send(command).map_err(|_| {
            NativeInputError::HotkeyUnavailable(
                "The activation-shortcut service is no longer running.".into(),
            )
        })?;
        post_command(self.thread_id)
    }
}

impl Drop for HotkeyService {
    fn drop(&mut self) {
        let _ = self.command_sender.send(HotkeyCommand::Shutdown);
        let _ = post_command(self.thread_id);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn native_hotkey(shortcut: Shortcut) -> Result<(u32, u32), NativeInputError> {
    if matches!(shortcut.key, ShortcutKey::Character(_) | ShortcutKey::Space)
        && shortcut.modifiers.is_empty()
    {
        return Err(NativeInputError::HotkeyUnavailable(
            "Letter, symbol, and Space shortcuts require Ctrl, Alt, Shift, or Win so normal typing remains usable."
                .into(),
        ));
    }

    let mut modifiers = native_modifiers(shortcut.modifiers) | MOD_NOREPEAT;
    let virtual_key = match shortcut.key {
        ShortcutKey::Function(number @ 1..=12) => u32::from(VK_F1) + u32::from(number - 1),
        ShortcutKey::Function(_) => {
            return Err(NativeInputError::HotkeyUnavailable(
                "The activation function key must be between F1 and F12.".into(),
            ));
        }
        ShortcutKey::Space => u32::from(VK_SPACE),
        ShortcutKey::Character(character) => {
            let mapping = unsafe { VkKeyScanW(character as u16) };
            if mapping == -1 {
                return Err(NativeInputError::HotkeyUnavailable(format!(
                    "{character} is not available on the current Windows keyboard layout."
                )));
            }
            let mapping = mapping as u16;
            let implicit = (mapping >> 8) as u8;
            if implicit & 1 != 0 {
                modifiers |= MOD_SHIFT;
            }
            if implicit & 2 != 0 {
                modifiers |= MOD_CONTROL;
            }
            if implicit & 4 != 0 {
                modifiers |= MOD_ALT;
            }
            u32::from(mapping & 0xff)
        }
    };
    Ok((modifiers, virtual_key))
}

fn native_modifiers(modifiers: ModifierSet) -> u32 {
    let mut native = 0;
    if modifiers.control {
        native |= MOD_CONTROL;
    }
    if modifiers.alt {
        native |= MOD_ALT;
    }
    if modifiers.shift {
        native |= MOD_SHIFT;
    }
    if modifiers.meta {
        native |= MOD_WIN;
    }
    native
}

fn post_command(thread_id: u32) -> Result<(), NativeInputError> {
    if unsafe { PostThreadMessageW(thread_id, COMMAND_MESSAGE, 0, 0) } != 0 {
        Ok(())
    } else {
        Err(NativeInputError::HotkeyUnavailable(format!(
            "The activation-shortcut service could not be updated ({}).",
            io::Error::last_os_error()
        )))
    }
}

fn hotkey_thread(
    initial_shortcut: Shortcut,
    event_sender: Sender<HotkeyEvent>,
    command_receiver: Receiver<HotkeyCommand>,
    ready_sender: mpsc::SyncSender<u32>,
) {
    let mut message = unsafe { std::mem::zeroed::<MSG>() };
    unsafe { PeekMessageW(&mut message, ptr::null_mut(), 0, 0, PM_NOREMOVE) };
    let thread_id = unsafe { GetCurrentThreadId() };
    let _ = ready_sender.send(thread_id);

    let mut active_registration =
        try_register_shortcut(HOTKEY_ID_PRIMARY, initial_shortcut, &event_sender, None)
            .then_some((HOTKEY_ID_PRIMARY, initial_shortcut));
    let mut escape_registered = false;

    loop {
        let result = unsafe { GetMessageW(&mut message, ptr::null_mut(), 0, 0) };
        if result <= 0 {
            break;
        }
        if message.message == WM_HOTKEY
            && active_registration.is_some_and(|(id, _)| message.wParam == id as WPARAM)
        {
            let _ = event_sender.send(HotkeyEvent::Pressed);
        } else if message.message == WM_HOTKEY
            && escape_registered
            && message.wParam == HOTKEY_ID_ESCAPE as WPARAM
        {
            let _ = event_sender.send(HotkeyEvent::EmergencyStop);
        } else if message.message == COMMAND_MESSAGE {
            while let Ok(command) = command_receiver.try_recv() {
                match command {
                    HotkeyCommand::Set(shortcut) => {
                        if active_registration.is_some_and(|(_, current)| current == shortcut) {
                            let _ = event_sender.send(HotkeyEvent::Registered(shortcut));
                            continue;
                        }

                        let candidate_id = match active_registration {
                            Some((HOTKEY_ID_PRIMARY, _)) => HOTKEY_ID_SECONDARY,
                            _ => HOTKEY_ID_PRIMARY,
                        };
                        let retained_shortcut = active_registration.map(|(_, current)| current);
                        if try_register_shortcut(
                            candidate_id,
                            shortcut,
                            &event_sender,
                            retained_shortcut,
                        ) {
                            if let Some((old_id, _)) = active_registration {
                                unsafe { UnregisterHotKey(ptr::null_mut(), old_id) };
                            }
                            active_registration = Some((candidate_id, shortcut));
                        }
                    }
                    HotkeyCommand::SetEscape(active) => {
                        if active == escape_registered {
                            continue;
                        }
                        if active {
                            if unsafe {
                                RegisterHotKey(
                                    ptr::null_mut(),
                                    HOTKEY_ID_ESCAPE,
                                    MOD_NOREPEAT,
                                    u32::from(VK_ESCAPE),
                                )
                            } != 0
                            {
                                escape_registered = true;
                            } else {
                                let _ = event_sender.send(
                                    HotkeyEvent::EscapeRegistrationFailed(format!(
                                        "Emergency Escape could not be registered ({}). Use the activation shortcut to stop.",
                                        io::Error::last_os_error()
                                    )),
                                );
                            }
                        } else {
                            unsafe { UnregisterHotKey(ptr::null_mut(), HOTKEY_ID_ESCAPE) };
                            escape_registered = false;
                        }
                    }
                    HotkeyCommand::Shutdown => {
                        if let Some((id, _)) = active_registration {
                            unsafe { UnregisterHotKey(ptr::null_mut(), id) };
                        }
                        if escape_registered {
                            unsafe { UnregisterHotKey(ptr::null_mut(), HOTKEY_ID_ESCAPE) };
                        }
                        return;
                    }
                }
            }
        }
    }

    if let Some((id, _)) = active_registration {
        unsafe { UnregisterHotKey(ptr::null_mut(), id) };
    }
    if escape_registered {
        unsafe { UnregisterHotKey(ptr::null_mut(), HOTKEY_ID_ESCAPE) };
    }
}

fn try_register_shortcut(
    id: i32,
    shortcut: Shortcut,
    event_sender: &Sender<HotkeyEvent>,
    retained_shortcut: Option<Shortcut>,
) -> bool {
    let Ok((modifiers, virtual_key)) = native_hotkey(shortcut) else {
        return false;
    };
    if unsafe { RegisterHotKey(ptr::null_mut(), id, modifiers, virtual_key) } != 0 {
        let _ = event_sender.send(HotkeyEvent::Registered(shortcut));
        true
    } else {
        let error = io::Error::last_os_error();
        let retained = retained_shortcut.map_or_else(String::new, |current| {
            format!(" {current} remains active until another shortcut is available.")
        });
        let _ = event_sender.send(HotkeyEvent::RegistrationFailed {
            shortcut,
            retained_shortcut,
            message: format!(
                "{shortcut} is already in use or unavailable ({error}).{retained} Choose another shortcut."
            ),
        });
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_navigation_and_function_keys() {
        assert_eq!(virtual_key(SpecialKey::Left), (VK_LEFT, true));
        assert_eq!(virtual_key(SpecialKey::Function(12)), (VK_F1 + 11, false));
    }

    #[test]
    fn unicode_input_uses_surrogate_pairs_when_needed() {
        let mut encoded = [0u16; 2];
        assert_eq!('A'.encode_utf16(&mut encoded), &[0x0041]);
        assert_eq!('🙂'.encode_utf16(&mut encoded), &[0xD83D, 0xDE42]);
    }

    #[test]
    fn sticky_background_allowlist_is_narrow() {
        assert!(is_background_capable_class("Edit"));
        assert!(is_background_capable_class("RichEditD2DPT"));
        assert!(is_background_capable_class(
            "WindowsForms10.EDIT.app.0.2bf8098_r8_ad1"
        ));
        assert!(!is_background_capable_class("Chrome_RenderWidgetHostHWND"));
        assert!(!is_background_capable_class("MozillaWindowClass"));
    }

    #[test]
    fn character_shortcuts_require_a_modifier() {
        let bare = Shortcut::new(ModifierSet::default(), ShortcutKey::Character('Q')).unwrap();
        assert!(native_hotkey(bare).is_err());

        let modified = Shortcut::new(
            ModifierSet {
                control: true,
                ..ModifierSet::default()
            },
            ShortcutKey::Character('Q'),
        )
        .unwrap();
        assert!(native_hotkey(modified).is_ok());
    }
}
