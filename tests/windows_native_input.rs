#![cfg(windows)]

use std::{
    fs,
    path::PathBuf,
    process::{Child, Command, Stdio},
    ptr, thread,
    time::{Duration, Instant},
};

use autoquill::{
    platform::{ForegroundBackend, NativeInputError},
    typing::{Instruction, PreviewOperation},
};
use windows_sys::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    BringWindowToTop, FindWindowW, GetForegroundWindow, GetWindowThreadProcessId, IsWindow,
    SetForegroundWindow,
};

struct ProbeProcess {
    child: Child,
    directory: PathBuf,
}

impl Drop for ProbeProcess {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = fs::remove_dir_all(&self.directory);
    }
}

/// This explicit test sends real input only to a temporary, externally hosted WinForms text box.
/// It is ignored by default so normal `cargo test` never injects keyboard input.
#[test]
#[ignore = "sends controlled real input to a temporary Windows text box"]
fn types_unicode_into_controlled_foreground_window() {
    let original_foreground = unsafe { GetForegroundWindow() };
    let directory = std::env::temp_dir().join(format!(
        "autoquill-native-input-probe-{}",
        std::process::id()
    ));
    fs::create_dir_all(&directory).expect("probe directory should be creatable");
    let output = directory.join("captured.txt");
    let title = "AutoQuill Native Input Probe";

    let script = r#"
$outputPath = $env:AUTOQUILL_PROBE_OUTPUT
Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing
$form = New-Object System.Windows.Forms.Form
$form.Text = 'AutoQuill Native Input Probe'
$form.Width = 640
$form.Height = 240
$form.StartPosition = 'CenterScreen'
$form.TopMost = $true
$textBox = New-Object System.Windows.Forms.TextBox
$textBox.Multiline = $true
$textBox.Dock = 'Fill'
$textBox.Font = New-Object System.Drawing.Font('Segoe UI', 16)
$utf8 = New-Object System.Text.UTF8Encoding($false)
$textBox.Add_TextChanged({ [System.IO.File]::WriteAllText($outputPath, $textBox.Text, $utf8) })
$form.Controls.Add($textBox)
$form.Add_Shown({ $form.Activate(); $textBox.Focus() })
[System.Windows.Forms.Application]::Run($form)
"#;

    let child = Command::new("powershell.exe")
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-STA",
            "-Command",
            script,
        ])
        .env("AUTOQUILL_PROBE_OUTPUT", &output)
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("controlled input probe should launch");
    let _probe = ProbeProcess { child, directory };

    let wide_title: Vec<u16> = title.encode_utf16().chain(Some(0)).collect();
    let deadline = Instant::now() + Duration::from_secs(10);
    let window = loop {
        let window = unsafe { FindWindowW(ptr::null(), wide_title.as_ptr()) };
        if !window.is_null() {
            break window;
        }
        assert!(Instant::now() < deadline, "probe window did not appear");
        thread::sleep(Duration::from_millis(50));
    };

    foreground_probe_window(window);
    let focus_deadline = Instant::now() + Duration::from_secs(5);
    while unsafe { GetForegroundWindow() } != window {
        foreground_probe_window(window);
        assert!(
            Instant::now() < focus_deadline,
            "probe window could not become foreground"
        );
        thread::sleep(Duration::from_millis(50));
    }
    thread::sleep(Duration::from_millis(150));

    let backend = ForegroundBackend;
    let target = backend
        .capture_foreground()
        .expect("probe should be a valid external foreground target");
    assert!(target.label().contains("AutoQuill Native Input Probe"));

    let expected = "AutoQuill 日本🙂";
    for character in expected.chars() {
        backend
            .emit(
                &target,
                &PreviewOperation::Intended(Instruction::Character(character)),
            )
            .expect("controlled Unicode input should succeed");
    }

    let output_deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if fs::read_to_string(&output).is_ok_and(|actual| actual == expected) {
            break;
        }
        assert!(
            Instant::now() < output_deadline,
            "probe did not receive the expected Unicode text"
        );
        thread::sleep(Duration::from_millis(50));
    }

    if !original_foreground.is_null()
        && original_foreground != window
        && unsafe { IsWindow(original_foreground) } != 0
    {
        foreground_probe_window(original_foreground);
        let restore_deadline = Instant::now() + Duration::from_secs(5);
        while unsafe { GetForegroundWindow() } != original_foreground {
            foreground_probe_window(original_foreground);
            assert!(
                Instant::now() < restore_deadline,
                "original foreground window could not be restored"
            );
            thread::sleep(Duration::from_millis(50));
        }
        assert_eq!(
            backend.validate(&target),
            Err(NativeInputError::TargetChanged),
            "a foreground change must invalidate the captured target"
        );
    }
}

fn foreground_probe_window(window: windows_sys::Win32::Foundation::HWND) {
    let current_foreground = unsafe { GetForegroundWindow() };
    let foreground_thread = if current_foreground.is_null() {
        0
    } else {
        unsafe { GetWindowThreadProcessId(current_foreground, ptr::null_mut()) }
    };
    let current_thread = unsafe { GetCurrentThreadId() };
    let attached = foreground_thread != 0
        && foreground_thread != current_thread
        && unsafe { AttachThreadInput(current_thread, foreground_thread, 1) } != 0;

    unsafe {
        BringWindowToTop(window);
        SetForegroundWindow(window);
    }

    if attached {
        unsafe { AttachThreadInput(current_thread, foreground_thread, 0) };
    }
}
