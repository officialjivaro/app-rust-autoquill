#![cfg(windows)]

use std::{
    ffi::OsString,
    fs,
    mem::size_of,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    ptr, thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use autoquill::{
    platform::ForegroundBackend,
    typing::{Instruction, PreviewOperation, SpecialKey},
};
use windows_sys::Win32::{
    Foundation::{HWND, LPARAM, POINT, RECT},
    System::Threading::{AttachThreadInput, GetCurrentThreadId},
    UI::{
        Input::KeyboardAndMouse::{
            INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBDINPUT, KEYEVENTF_KEYUP,
            MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, MOUSEINPUT, SendInput, VK_1, VK_CONTROL,
            VK_RETURN, VK_S,
        },
        WindowsAndMessaging::{
            BringWindowToTop, EnumWindows, GetCursorPos, GetForegroundWindow, GetWindowRect,
            GetWindowTextW, GetWindowThreadProcessId, IsWindow, IsWindowVisible, PostMessageW,
            SetCursorPos, SetForegroundWindow, WM_CLOSE,
        },
    },
};
use windows_sys::core::BOOL;

const WINDOW_TIMEOUT: Duration = Duration::from_secs(45);
const RECEIVE_TIMEOUT: Duration = Duration::from_secs(10);

struct ScratchDirectory(PathBuf);

impl ScratchDirectory {
    fn new() -> Self {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after the Unix epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "autoquill-windows-matrix-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir_all(&path).expect("matrix scratch directory should be creatable");
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

struct CursorPosition(POINT);

impl CursorPosition {
    fn capture() -> Self {
        let mut point = POINT { x: 0, y: 0 };
        assert_ne!(
            unsafe { GetCursorPos(&mut point) },
            0,
            "cursor position should be readable"
        );
        Self(point)
    }
}

impl Drop for CursorPosition {
    fn drop(&mut self) {
        unsafe { SetCursorPos(self.0.x, self.0.y) };
    }
}

impl Drop for ScratchDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

struct AppWindowGuard {
    child: Child,
    window: HWND,
}

impl AppWindowGuard {
    fn close(mut self) {
        close_window(self.window);
        let _ = self.child.kill();
        let _ = self.child.wait();
        self.window = ptr::null_mut();
    }
}

impl Drop for AppWindowGuard {
    fn drop(&mut self) {
        if !self.window.is_null() {
            close_window(self.window);
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Sends controlled native input only to fresh temporary documents and isolated browser profiles.
/// This test is ignored by default so ordinary `cargo test` never injects keyboard input.
#[test]
#[ignore = "types into temporary documents in installed Windows applications"]
fn installed_application_compatibility_matrix() {
    let original_foreground = unsafe { GetForegroundWindow() };
    let _original_cursor = CursorPosition::capture();
    let scratch = ScratchDirectory::new();
    let run_marker = scratch
        .path()
        .file_name()
        .expect("matrix scratch directory should have a name")
        .to_string_lossy();
    let (operations, expected) = compatibility_document();
    let browser_only = std::env::var_os("AUTOQUILL_MATRIX_BROWSER_ONLY").is_some();

    if !browser_only {
        let notepad = installed_executable("WINDIR", r"System32\notepad.exe")
            .expect("Notepad is required for the Windows matrix");
        run_editor_probe(
            "Notepad",
            &notepad,
            vec![],
            scratch
                .path()
                .join(format!("autoquill-notepad-{run_marker}.txt")),
            &operations,
            &expected,
            true,
        );
    }

    let browser_page = scratch.path().join("autoquill-browser-probe.html");
    let browser_marker = format!("AutoQuill Browser Probe {}", std::process::id());
    write_browser_probe(&browser_page, &browser_marker);

    let edge = installed_executable(
        "ProgramFiles(x86)",
        r"Microsoft\Edge\Application\msedge.exe",
    );
    if let Some(edge) = edge {
        run_browser_probe(
            "Microsoft Edge",
            &edge,
            scratch.path().join("edge-profile"),
            &browser_page,
            &browser_marker,
            &operations,
            &expected,
        );
    } else {
        println!("MATRIX SKIP: Microsoft Edge is not installed");
    }

    let chrome = installed_executable("ProgramFiles", r"Google\Chrome\Application\chrome.exe");
    if let Some(chrome) = chrome {
        run_browser_probe(
            "Google Chrome",
            &chrome,
            scratch.path().join("chrome-profile"),
            &browser_page,
            &browser_marker,
            &operations,
            &expected,
        );
    } else {
        println!("MATRIX SKIP: Google Chrome is not installed");
    }

    let firefox = installed_executable("ProgramFiles", r"Mozilla Firefox\firefox.exe");
    if let Some(firefox) = firefox {
        run_browser_probe(
            "Mozilla Firefox",
            &firefox,
            scratch.path().join("firefox-profile"),
            &browser_page,
            &browser_marker,
            &operations,
            &expected,
        );
    } else {
        println!("MATRIX SKIP: Mozilla Firefox is not installed");
    }

    if !browser_only {
        let vscode = installed_executable("LOCALAPPDATA", r"Programs\Microsoft VS Code\Code.exe");
        if let Some(vscode) = vscode {
            let profile = scratch.path().join("vscode-profile");
            let extensions = scratch.path().join("vscode-extensions");
            run_editor_probe(
                "Visual Studio Code",
                &vscode,
                vec![
                    "--disable-extensions".into(),
                    "--new-window".into(),
                    "--user-data-dir".into(),
                    profile.into_os_string(),
                    "--extensions-dir".into(),
                    extensions.into_os_string(),
                ],
                scratch
                    .path()
                    .join(format!("autoquill-vscode-{run_marker}.txt")),
                &operations,
                &expected,
                false,
            );
        } else {
            println!("MATRIX SKIP: Visual Studio Code is not installed");
        }
    }

    restore_foreground(original_foreground);
}

fn installed_executable(root_variable: &str, relative_path: &str) -> Option<PathBuf> {
    std::env::var_os(root_variable)
        .map(PathBuf::from)
        .map(|root| root.join(relative_path))
        .filter(|path| path.is_file())
}

fn run_editor_probe(
    name: &'static str,
    executable: &Path,
    mut arguments: Vec<OsString>,
    document: PathBuf,
    operations: &[PreviewOperation],
    expected: &str,
    release_gating: bool,
) {
    fs::write(&document, []).expect("temporary editor document should be creatable");
    let marker = document
        .file_name()
        .expect("temporary document should have a file name")
        .to_string_lossy()
        .into_owned();
    arguments.push(document.as_os_str().to_owned());

    let app = match launch_and_focus(name, executable, &arguments, &marker) {
        Ok(app) => app,
        Err(message) if !release_gating => {
            println!(
                "MATRIX LIMITATION: {name} clean-profile launch/focus automation was unavailable: {message}. This optional editor is non-gating."
            );
            return;
        }
        Err(message) => panic!("{name} launch/focus probe failed: {message}"),
    };
    let backend = ForegroundBackend;
    let target = backend
        .capture_foreground()
        .unwrap_or_else(|error| panic!("{name} should be capturable: {error}"));
    assert!(target.label().contains(&marker));

    emit_operations(&backend, &target, operations);
    thread::sleep(Duration::from_secs(1));
    send_control_s();
    let expected_received = if name == "Visual Studio Code" {
        // VS Code converts Tab to indentation up to the next tab stop in plain-text editors.
        expected.replace('\t', " ")
    } else {
        expected.to_owned()
    };
    let receiver_result = wait_for_editor_content(name, &document, &expected_received);
    backend
        .validate(&target)
        .unwrap_or_else(|error| panic!("{name} target changed unexpectedly: {error}"));
    app.close();

    match receiver_result {
        Ok(()) => println!(
            "MATRIX PASS: {name} received {} Unicode/special-key characters in a temporary file",
            expected.chars().count()
        ),
        Err(message) if release_gating => panic!("{message}"),
        Err(message) => println!(
            "MATRIX LIMITATION: {message}. The top-level target stayed safe, but clean-profile editor focus is not release-gating."
        ),
    }
}

fn run_browser_probe(
    name: &'static str,
    executable: &Path,
    profile: PathBuf,
    page: &Path,
    marker: &str,
    operations: &[PreviewOperation],
    expected: &str,
) {
    fs::create_dir_all(&profile).expect("isolated browser profile should be creatable");
    let page_url = format!("file:///{}", page.to_string_lossy().replace('\\', "/"));
    let arguments = if name == "Mozilla Firefox" {
        vec![
            OsString::from("-new-instance"),
            OsString::from("-profile"),
            profile.into_os_string(),
            OsString::from(page_url),
        ]
    } else {
        vec![
            OsString::from("--no-first-run"),
            OsString::from("--no-default-browser-check"),
            OsString::from("--disable-default-apps"),
            OsString::from("--disable-extensions"),
            OsString::from("--disable-background-networking"),
            OsString::from("--disable-component-update"),
            OsString::from(format!("--user-data-dir={}", profile.display())),
            OsString::from(format!("--app={page_url}")),
        ]
    };

    let app = launch_and_focus(name, executable, &arguments, marker)
        .unwrap_or_else(|message| panic!("{name} launch/focus probe failed: {message}"));
    let backend = ForegroundBackend;
    let target = backend
        .capture_foreground()
        .unwrap_or_else(|error| panic!("{name} should be capturable: {error}"));
    assert!(target.label().contains(marker));

    emit_operations(&backend, &target, operations);
    let expected_title = browser_result_title(marker, expected);
    wait_for_browser_title(name, app.window, &expected_title);
    backend
        .validate(&target)
        .unwrap_or_else(|error| panic!("{name} target changed unexpectedly: {error}"));

    println!(
        "MATRIX PASS: {name} received {} Unicode/special-key characters in an isolated profile",
        expected.chars().count()
    );
    app.close();
}

fn launch_and_focus(
    name: &'static str,
    executable: &Path,
    arguments: &[OsString],
    marker: &str,
) -> Result<AppWindowGuard, String> {
    let child = Command::new(executable)
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| format!("application did not launch: {error}"))?;
    let mut app = AppWindowGuard {
        child,
        window: ptr::null_mut(),
    };
    let mut window = wait_for_window(marker, WINDOW_TIMEOUT)?;
    app.window = window;
    foreground_window(window);
    wait_until(Duration::from_secs(5), || unsafe {
        GetForegroundWindow() == window
    })
    .then_some(())
    .ok_or_else(|| format!("window containing {marker:?} could not become foreground"))?;
    thread::sleep(if name == "Visual Studio Code" {
        Duration::from_secs(10)
    } else {
        Duration::from_secs(5)
    });
    if let Some(final_window) = find_window(marker) {
        window = final_window;
        app.window = final_window;
        foreground_window(final_window);
    }
    focus_editor_content(window);
    if name == "Visual Studio Code" {
        send_control_key(VK_1);
        thread::sleep(Duration::from_millis(500));
    }
    foreground_window(window);
    wait_until(Duration::from_secs(5), || unsafe {
        GetForegroundWindow() == window
    })
    .then_some(())
    .ok_or_else(|| format!("window containing {marker:?} lost foreground focus"))?;
    thread::sleep(Duration::from_millis(200));
    Ok(app)
}

fn focus_editor_content(window: HWND) {
    let mut bounds = RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    assert_ne!(
        unsafe { GetWindowRect(window, &mut bounds) },
        0,
        "temporary app bounds should be readable"
    );
    let x = bounds.left + (bounds.right - bounds.left) * 2 / 3;
    let y = bounds.top + (bounds.bottom - bounds.top) * 3 / 5;
    assert_ne!(
        unsafe { SetCursorPos(x, y) },
        0,
        "cursor should move over the temporary editor"
    );
    let clicks = [
        mouse_input(MOUSEEVENTF_LEFTDOWN),
        mouse_input(MOUSEEVENTF_LEFTUP),
    ];
    let sent = unsafe {
        SendInput(
            clicks.len() as u32,
            clicks.as_ptr(),
            size_of::<INPUT>() as i32,
        )
    };
    assert_eq!(sent, clicks.len() as u32, "editor focus click should land");
}

fn compatibility_document() -> (Vec<PreviewOperation>, String) {
    let prefix = "AutoQuill compatibility 日本🙂";
    let typo = "mistkae";
    let correction = "ake";
    let suffix = "0123456789abcdef".repeat(2);
    let expected = format!("{prefix}\nmistake\t{suffix}");

    let mut operations = prefix
        .chars()
        .map(|character| PreviewOperation::Intended(Instruction::Character(character)))
        .collect::<Vec<_>>();
    operations.push(PreviewOperation::Intended(Instruction::SpecialKey(
        SpecialKey::Enter,
    )));
    operations.extend(typo.chars().map(PreviewOperation::TypoCharacter));
    operations.extend(std::iter::repeat_n(
        PreviewOperation::CorrectionBackspace,
        3,
    ));
    operations.extend(
        correction
            .chars()
            .map(|character| PreviewOperation::Intended(Instruction::Character(character))),
    );
    operations.push(PreviewOperation::Intended(Instruction::SpecialKey(
        SpecialKey::Tab,
    )));
    operations.extend(
        suffix
            .chars()
            .map(|character| PreviewOperation::Intended(Instruction::Character(character))),
    );
    (operations, expected)
}

fn emit_operations(
    backend: &ForegroundBackend,
    target: &autoquill::platform::ForegroundTarget,
    operations: &[PreviewOperation],
) {
    for (index, operation) in operations.iter().enumerate() {
        backend
            .emit(target, operation)
            .unwrap_or_else(|error| panic!("operation {index} should be delivered: {error}"));
        // Qualify the broad app matrix at AutoQuill's default 60 WPM. Higher-rate behavior is
        // separately warned in the UI and covered by controlled-backend stress tests.
        thread::sleep(Duration::from_millis(200));
    }
}

fn normalize_editor_text(text: &str) -> String {
    text.trim_start_matches('\u{feff}').replace("\r\n", "\n")
}

fn wait_for_editor_content(name: &str, document: &Path, expected: &str) -> Result<(), String> {
    let deadline = Instant::now() + RECEIVE_TIMEOUT;
    loop {
        let actual = fs::read_to_string(document).unwrap_or_default();
        if normalize_editor_text(&actual) == expected {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "{name} saved unexpected content: expected {expected:?}, received {:?}",
                normalize_editor_text(&actual)
            ));
        }
        thread::sleep(Duration::from_millis(50));
    }
}

fn wait_for_browser_title(name: &str, window: HWND, expected: &str) {
    let deadline = Instant::now() + RECEIVE_TIMEOUT;
    loop {
        let actual = window_title(window);
        if actual == expected {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "{name} reported unexpected probe result: expected {expected:?}, received {actual:?}"
        );
        thread::sleep(Duration::from_millis(50));
    }
}

fn write_browser_probe(path: &Path, marker: &str) {
    let html = format!(
        r#"<!doctype html>
<meta charset="utf-8">
<title>{marker}</title>
<textarea id="probe" autofocus style="width:95vw;height:85vh;font:18px monospace"></textarea>
<script>
const marker = {marker:?};
const probe = document.getElementById('probe');
function update() {{
  const bytes = new TextEncoder().encode(probe.value);
  let hash = 0x811c9dc5;
  for (const byte of bytes) hash = Math.imul(hash ^ byte, 0x01000193) >>> 0;
  document.title = `${{marker}}|${{bytes.length}}|${{hash.toString(16).padStart(8, '0')}}`;
}}
probe.addEventListener('keydown', event => {{
  if (event.key === 'Tab') {{
    event.preventDefault();
    const start = probe.selectionStart;
    probe.setRangeText('\t', start, probe.selectionEnd, 'end');
    probe.dispatchEvent(new Event('input'));
  }}
}});
probe.addEventListener('input', update);
window.addEventListener('load', () => probe.focus());
</script>"#
    );
    fs::write(path, html).expect("browser probe page should be writable");
}

fn browser_result_title(marker: &str, expected: &str) -> String {
    let hash = expected
        .as_bytes()
        .iter()
        .fold(0x811c9dc5_u32, |hash, byte| {
            (hash ^ u32::from(*byte)).wrapping_mul(0x01000193)
        });
    format!("{marker}|{}|{hash:08x}", expected.len())
}

fn send_control_s() {
    send_control_key(VK_S);
}

fn send_control_key(key: u16) {
    let inputs = [
        keyboard_input(VK_CONTROL, 0),
        keyboard_input(key, 0),
        keyboard_input(key, KEYEVENTF_KEYUP),
        keyboard_input(VK_CONTROL, KEYEVENTF_KEYUP),
    ];
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
        "Ctrl shortcut should be delivered"
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

fn mouse_input(flags: u32) -> INPUT {
    INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx: 0,
                dy: 0,
                mouseData: 0,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

struct WindowSearch<'a> {
    marker: &'a str,
    found: HWND,
}

unsafe extern "system" fn find_window_callback(window: HWND, parameter: LPARAM) -> BOOL {
    if unsafe { IsWindowVisible(window) } == 0 {
        return 1;
    }
    let search = unsafe { &mut *(parameter as *mut WindowSearch<'_>) };
    if window_title(window).contains(search.marker) {
        search.found = window;
        return 0;
    }
    1
}

fn find_window(marker: &str) -> Option<HWND> {
    let mut search = WindowSearch {
        marker,
        found: ptr::null_mut(),
    };
    unsafe {
        EnumWindows(
            Some(find_window_callback),
            &mut search as *mut WindowSearch<'_> as LPARAM,
        );
    }
    (!search.found.is_null()).then_some(search.found)
}

fn wait_for_window(marker: &str, timeout: Duration) -> Result<HWND, String> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(window) = find_window(marker) {
            return Ok(window);
        }
        if Instant::now() >= deadline {
            return Err(format!("window containing {marker:?} did not appear"));
        }
        thread::sleep(Duration::from_millis(50));
    }
}

fn window_title(window: HWND) -> String {
    let mut buffer = [0u16; 512];
    let length = unsafe { GetWindowTextW(window, buffer.as_mut_ptr(), buffer.len() as i32) };
    String::from_utf16_lossy(&buffer[..usize::try_from(length).unwrap_or(0)])
}

fn foreground_window(window: HWND) {
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

fn close_window(window: HWND) {
    if window.is_null() || unsafe { IsWindow(window) } == 0 {
        return;
    }
    unsafe { PostMessageW(window, WM_CLOSE, 0, 0) };
    let prompt_deadline = Instant::now() + Duration::from_secs(1);
    while unsafe { IsWindow(window) } != 0 && Instant::now() < prompt_deadline {
        thread::sleep(Duration::from_millis(50));
    }
    if unsafe { IsWindow(window) } != 0 {
        foreground_window(window);
        let enter = [
            keyboard_input(VK_RETURN, 0),
            keyboard_input(VK_RETURN, KEYEVENTF_KEYUP),
        ];
        let sent = unsafe {
            SendInput(
                enter.len() as u32,
                enter.as_ptr(),
                size_of::<INPUT>() as i32,
            )
        };
        assert_eq!(
            sent,
            enter.len() as u32,
            "temporary save prompt should close"
        );
        let close_deadline = Instant::now() + Duration::from_secs(4);
        while unsafe { IsWindow(window) } != 0 && Instant::now() < close_deadline {
            thread::sleep(Duration::from_millis(50));
        }
    }
}

fn restore_foreground(window: HWND) {
    if !window.is_null() && unsafe { IsWindow(window) } != 0 {
        foreground_window(window);
    }
}

fn wait_until(timeout: Duration, mut condition: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + timeout;
    while !condition() {
        if Instant::now() >= deadline {
            return false;
        }
        thread::sleep(Duration::from_millis(50));
    }
    true
}
