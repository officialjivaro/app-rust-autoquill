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
            KEYEVENTF_UNICODE, MOD_NOREPEAT, RegisterHotKey, SendInput, UnregisterHotKey, VK_ADD,
            VK_APPS, VK_BACK, VK_CAPITAL, VK_CONTROL, VK_DECIMAL, VK_DELETE, VK_DIVIDE, VK_DOWN,
            VK_END, VK_ESCAPE, VK_F1, VK_HOME, VK_INSERT, VK_LEFT, VK_LWIN, VK_MENU, VK_MULTIPLY,
            VK_NEXT, VK_NUMLOCK, VK_NUMPAD0, VK_PAUSE, VK_PRIOR, VK_RETURN, VK_RIGHT, VK_RWIN,
            VK_SCROLL, VK_SHIFT, VK_SNAPSHOT, VK_SPACE, VK_SUBTRACT, VK_TAB, VK_UP,
        },
        WindowsAndMessaging::{
            GetForegroundWindow, GetMessageW, GetWindowTextW, GetWindowThreadProcessId, IsWindow,
            MSG, PM_NOREMOVE, PeekMessageW, PostThreadMessageW, WM_APP, WM_HOTKEY,
        },
    },
};

use crate::typing::{Instruction, PreviewOperation, SpecialKey};

use super::{ForegroundTarget, HotkeyEvent, NativeInputError};

const HOTKEY_ID_PRIMARY: i32 = 1;
const HOTKEY_ID_SECONDARY: i32 = 2;
const COMMAND_MESSAGE: u32 = WM_APP + 17;

#[derive(Debug, Default)]
pub struct ForegroundBackend;

impl ForegroundBackend {
    #[must_use]
    pub const fn available() -> bool {
        true
    }

    pub fn capture_foreground(&self) -> Result<ForegroundTarget, NativeInputError> {
        let window = unsafe { GetForegroundWindow() };
        if window.is_null() {
            return Err(NativeInputError::NoTarget);
        }

        let mut process_id = 0;
        unsafe { GetWindowThreadProcessId(window, &mut process_id) };
        if process_id == std::process::id() {
            return Err(NativeInputError::OwnWindow);
        }

        let mut buffer = [0u16; 512];
        let length = unsafe { GetWindowTextW(window, buffer.as_mut_ptr(), buffer.len() as i32) };
        let label = if length > 0 {
            String::from_utf16_lossy(&buffer[..length as usize])
        } else {
            "Untitled window".to_owned()
        };

        Ok(ForegroundTarget {
            raw_handle: window as usize,
            process_id,
            label,
        })
    }

    pub fn validate(&self, target: &ForegroundTarget) -> Result<(), NativeInputError> {
        let window = target.raw_handle as HWND;
        if unsafe { IsWindow(window) } == 0 {
            return Err(NativeInputError::TargetClosed);
        }
        if unsafe { GetForegroundWindow() } != window {
            return Err(NativeInputError::TargetChanged);
        }

        let mut current_process_id = 0;
        unsafe { GetWindowThreadProcessId(window, &mut current_process_id) };
        if current_process_id != target.process_id {
            return Err(NativeInputError::TargetClosed);
        }
        Ok(())
    }

    pub fn emit(
        &self,
        target: &ForegroundTarget,
        operation: &PreviewOperation,
    ) -> Result<(), NativeInputError> {
        self.validate(target)?;
        match operation {
            PreviewOperation::Intended(Instruction::Character(character))
            | PreviewOperation::TypoCharacter(character) => send_unicode(*character),
            PreviewOperation::Intended(Instruction::SpecialKey(key)) => send_special(*key),
            PreviewOperation::CorrectionBackspace => send_special(SpecialKey::Backspace),
        }
    }
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
    // AutoQuill submits ordered down/up pairs. If Windows accepted an odd prefix, release the
    // unmatched final key before surfacing the terminal error so a virtual key cannot stay held.
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
    Set(u8),
    Shutdown,
}

#[derive(Debug)]
pub struct HotkeyService {
    command_sender: Sender<HotkeyCommand>,
    thread_id: u32,
    thread: Option<JoinHandle<()>>,
}

impl HotkeyService {
    pub fn start(function_key: u8) -> Result<(Self, Receiver<HotkeyEvent>), NativeInputError> {
        validate_function_key(function_key)?;
        let (event_sender, event_receiver) = mpsc::channel();
        let (command_sender, command_receiver) = mpsc::channel();
        let (ready_sender, ready_receiver) = mpsc::sync_channel(1);

        let thread = thread::Builder::new()
            .name("autoquill-hotkey".into())
            .spawn(move || {
                hotkey_thread(function_key, event_sender, command_receiver, ready_sender)
            })
            .map_err(|error| NativeInputError::HotkeyUnavailable(error.to_string()))?;

        let thread_id = ready_receiver.recv().map_err(|_| {
            NativeInputError::HotkeyUnavailable(
                "The activation-key service could not initialize.".into(),
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

    pub fn set_key(&self, function_key: u8) -> Result<(), NativeInputError> {
        validate_function_key(function_key)?;
        self.command_sender
            .send(HotkeyCommand::Set(function_key))
            .map_err(|_| {
                NativeInputError::HotkeyUnavailable(
                    "The activation-key service is no longer running.".into(),
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

fn validate_function_key(function_key: u8) -> Result<(), NativeInputError> {
    if (1..=12).contains(&function_key) {
        Ok(())
    } else {
        Err(NativeInputError::HotkeyUnavailable(
            "The activation key must be between F1 and F12.".into(),
        ))
    }
}

fn post_command(thread_id: u32) -> Result<(), NativeInputError> {
    if unsafe { PostThreadMessageW(thread_id, COMMAND_MESSAGE, 0 as WPARAM, 0 as LPARAM) } != 0 {
        Ok(())
    } else {
        Err(NativeInputError::HotkeyUnavailable(format!(
            "The activation-key service could not be updated ({}).",
            io::Error::last_os_error()
        )))
    }
}

fn hotkey_thread(
    initial_key: u8,
    event_sender: Sender<HotkeyEvent>,
    command_receiver: Receiver<HotkeyCommand>,
    ready_sender: mpsc::SyncSender<u32>,
) {
    let mut message = unsafe { std::mem::zeroed::<MSG>() };
    unsafe { PeekMessageW(&mut message, ptr::null_mut(), 0, 0, PM_NOREMOVE) };
    let thread_id = unsafe { GetCurrentThreadId() };
    let _ = ready_sender.send(thread_id);

    let mut active_registration =
        try_register_key(HOTKEY_ID_PRIMARY, initial_key, &event_sender, None)
            .then_some((HOTKEY_ID_PRIMARY, initial_key));

    loop {
        let result = unsafe { GetMessageW(&mut message, ptr::null_mut(), 0, 0) };
        if result <= 0 {
            break;
        }
        if message.message == WM_HOTKEY
            && active_registration.is_some_and(|(id, _)| message.wParam == id as WPARAM)
        {
            let _ = event_sender.send(HotkeyEvent::Pressed);
        } else if message.message == COMMAND_MESSAGE {
            while let Ok(command) = command_receiver.try_recv() {
                match command {
                    HotkeyCommand::Set(function_key) => {
                        if active_registration.is_some_and(|(_, key)| key == function_key) {
                            let _ = event_sender.send(HotkeyEvent::Registered(function_key));
                            continue;
                        }

                        let candidate_id = match active_registration {
                            Some((HOTKEY_ID_PRIMARY, _)) => HOTKEY_ID_SECONDARY,
                            _ => HOTKEY_ID_PRIMARY,
                        };
                        let retained_key = active_registration.map(|(_, key)| key);
                        if try_register_key(candidate_id, function_key, &event_sender, retained_key)
                        {
                            if let Some((old_id, _)) = active_registration {
                                unsafe { UnregisterHotKey(ptr::null_mut(), old_id) };
                            }
                            active_registration = Some((candidate_id, function_key));
                        }
                    }
                    HotkeyCommand::Shutdown => {
                        if let Some((id, _)) = active_registration {
                            unsafe { UnregisterHotKey(ptr::null_mut(), id) };
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
}

fn try_register_key(
    id: i32,
    function_key: u8,
    event_sender: &Sender<HotkeyEvent>,
    retained_key: Option<u8>,
) -> bool {
    let virtual_key = u32::from(VK_F1) + u32::from(function_key - 1);
    if unsafe { RegisterHotKey(ptr::null_mut(), id, MOD_NOREPEAT, virtual_key) } != 0 {
        let _ = event_sender.send(HotkeyEvent::Registered(function_key));
        true
    } else {
        let error = io::Error::last_os_error();
        let retained = retained_key.map_or_else(String::new, |key| {
            format!(" F{key} remains registered until another key is available.")
        });
        let _ = event_sender.send(HotkeyEvent::RegistrationFailed {
            key: function_key,
            retained_key,
            message: format!(
                "F{function_key} is already in use or unavailable ({error}).{retained} Choose another F-key."
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
}
