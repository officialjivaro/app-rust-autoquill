#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::time::Duration;

use autoquill::{APP_VERSION, initialize_diagnostics, window_title};

slint::include_modules!();

fn main() -> Result<(), slint::PlatformError> {
    initialize_diagnostics();

    let app = AppWindow::new()?;
    app.set_app_version(APP_VERSION.into());
    app.set_window_title(window_title().into());

    if std::env::var_os("AUTOQUILL_SMOKE_TEST").is_some() {
        app.show()?;
        slint::Timer::single_shot(Duration::from_millis(100), || {
            let _ = slint::quit_event_loop();
        });
        slint::run_event_loop()
    } else {
        app.run()
    }
}
