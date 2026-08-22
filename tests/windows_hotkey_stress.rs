#![cfg(windows)]

use std::{
    mem::size_of,
    sync::mpsc::{Receiver, RecvTimeoutError},
    thread,
    time::{Duration, Instant},
};

use autoquill::platform::{HotkeyEvent, HotkeyService};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, SendInput, VK_F1,
};

const EVENT_TIMEOUT: Duration = Duration::from_secs(3);
const QUIET_WINDOW: Duration = Duration::from_millis(175);

/// Exercises real RegisterHotKey/WM_HOTKEY behavior without targeting an application or typing
/// document content. Ignored by default so ordinary test runs never synthesize key presses.
#[test]
#[ignore = "registers global F-keys and synthesizes controlled activation-key presses"]
fn rebind_conflict_repeat_and_stop_latency_stress() {
    let available = inventory_available_keys();
    assert!(
        available.len() >= 2,
        "the stress probe requires two available F-key registrations"
    );
    let primary_key = available[available.len() - 2];
    let conflict_key = available[available.len() - 1];

    let (primary, primary_events) = HotkeyService::start(primary_key)
        .unwrap_or_else(|error| panic!("primary F{primary_key} service should start: {error}"));
    expect_registered(&primary_events, primary_key);

    let (conflict, conflict_events) = HotkeyService::start(conflict_key).unwrap_or_else(|error| {
        panic!("conflicting F{conflict_key} service should start: {error}")
    });
    expect_registered(&conflict_events, conflict_key);

    primary
        .set_key(conflict_key)
        .expect("failed registration should still return its event");
    match recv_event(&primary_events) {
        HotkeyEvent::RegistrationFailed {
            key,
            retained_key,
            message,
        } => {
            assert_eq!(key, conflict_key);
            assert_eq!(retained_key, Some(primary_key));
            assert!(message.contains(&format!("F{primary_key} remains registered")));
        }
        event => panic!("expected transactional rebind failure, got {event:?}"),
    }

    let stop_started = Instant::now();
    tap_function_key(primary_key);
    assert_eq!(recv_event(&primary_events), HotkeyEvent::Pressed);
    let stop_latency = stop_started.elapsed();
    assert!(
        stop_latency < Duration::from_millis(250),
        "retained emergency stop event took {stop_latency:?}"
    );

    drop(conflict);
    primary
        .set_key(conflict_key)
        .unwrap_or_else(|error| panic!("F{conflict_key} should become available: {error}"));
    expect_registered(&primary_events, conflict_key);

    tap_function_key(primary_key);
    expect_quiet(&primary_events, QUIET_WINDOW);
    tap_function_key(conflict_key);
    assert_eq!(recv_event(&primary_events), HotkeyEvent::Pressed);

    repeat_function_key_down(conflict_key, 24);
    assert_eq!(recv_event(&primary_events), HotkeyEvent::Pressed);
    expect_quiet(&primary_events, QUIET_WINDOW);

    for cycle in 0..24 {
        let key = if cycle % 2 == 0 {
            primary_key
        } else {
            conflict_key
        };
        primary
            .set_key(key)
            .unwrap_or_else(|error| panic!("stress rebind F{key} should queue: {error}"));
        expect_registered(&primary_events, key);
        tap_function_key(key);
        assert_eq!(recv_event(&primary_events), HotkeyEvent::Pressed);
    }

    println!(
        "HOTKEY PASS: F{primary_key}/F{conflict_key} transactional conflicts, MOD_NOREPEAT, 24 rebind cycles, max measured stop path {stop_latency:?}"
    );
}

fn inventory_available_keys() -> Vec<u8> {
    let mut available = Vec::new();
    for key in 1..=12 {
        let (service, receiver) = HotkeyService::start(key)
            .unwrap_or_else(|error| panic!("F{key} inventory service should start: {error}"));
        match recv_event(&receiver) {
            HotkeyEvent::Registered(registered) => {
                assert_eq!(registered, key);
                available.push(key);
                println!("HOTKEY INVENTORY: F{key} available");
            }
            HotkeyEvent::RegistrationFailed {
                key: failed,
                retained_key,
                ..
            } => {
                assert_eq!(failed, key);
                assert_eq!(retained_key, None);
                println!("HOTKEY INVENTORY: F{key} already reserved externally");
            }
            event => panic!("unexpected F{key} inventory event: {event:?}"),
        }
        drop(service);
    }
    available
}

fn expect_registered(receiver: &Receiver<HotkeyEvent>, key: u8) {
    assert_eq!(recv_event(receiver), HotkeyEvent::Registered(key));
}

fn recv_event(receiver: &Receiver<HotkeyEvent>) -> HotkeyEvent {
    receiver
        .recv_timeout(EVENT_TIMEOUT)
        .expect("hotkey service should emit an event")
}

fn expect_quiet(receiver: &Receiver<HotkeyEvent>, duration: Duration) {
    match receiver.recv_timeout(duration) {
        Err(RecvTimeoutError::Timeout) => {}
        Err(RecvTimeoutError::Disconnected) => panic!("hotkey service disconnected unexpectedly"),
        Ok(event) => panic!("unexpected hotkey event: {event:?}"),
    }
}

fn tap_function_key(key: u8) {
    send_key_events(key, 1);
}

fn repeat_function_key_down(key: u8, repeats: usize) {
    let virtual_key = VK_F1 + u16::from(key - 1);
    let mut inputs = Vec::with_capacity(repeats + 1);
    inputs.extend(std::iter::repeat_n(keyboard_input(virtual_key, 0), repeats));
    inputs.push(keyboard_input(virtual_key, KEYEVENTF_KEYUP));
    send_inputs(&inputs);
    thread::sleep(Duration::from_millis(25));
}

fn send_key_events(key: u8, taps: usize) {
    let virtual_key = VK_F1 + u16::from(key - 1);
    let mut inputs = Vec::with_capacity(taps * 2);
    for _ in 0..taps {
        inputs.push(keyboard_input(virtual_key, 0));
        inputs.push(keyboard_input(virtual_key, KEYEVENTF_KEYUP));
    }
    send_inputs(&inputs);
}

fn send_inputs(inputs: &[INPUT]) {
    let sent = unsafe {
        SendInput(
            inputs.len() as u32,
            inputs.as_ptr(),
            size_of::<INPUT>() as i32,
        )
    };
    assert_eq!(
        sent,
        inputs.len() as u32,
        "all synthetic hotkey events should be accepted"
    );
}

fn keyboard_input(virtual_key: u16, flags: u32) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: virtual_key,
                wScan: 0,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}
