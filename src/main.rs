#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::{
    cell::RefCell,
    fs,
    path::PathBuf,
    rc::Rc,
    time::{Duration, Instant},
};

use autoquill::{
    APP_VERSION,
    diagnostics::privacy_safe_report,
    domain::{
        ModifierSet, Profile, SessionState, SettingsDraft, Shortcut, ShortcutKey, TargetIntent,
        TypingSettings,
    },
    initialize_diagnostics,
    persistence::{
        AppearancePreferences, ImportCandidate, ImportConflictPolicy, ProfileStatus, ProfileStore,
        ProfileSummary, ThemePreference, WindowPreferences,
    },
    platform::{
        CapabilityReport, DeliveryStrategy, ForegroundBackend, ForegroundTarget, HotkeyEvent,
        HotkeyService, VerificationLevel, current_capabilities, document_uses_activation_shortcut,
    },
    typing::{
        CompileWarning, CompletionReason, EngineUpdate, PreviewOperation, SeededRandom,
        SessionEngine, SessionPlan, SystemRuntimeValues, compile,
    },
    window_title,
};
use copypasta::{ClipboardContext, ClipboardProvider};
use slint::{
    CloseRequestResponse, ComponentHandle, LogicalSize, ModelRc, PhysicalPosition, PhysicalSize,
    VecModel, platform::Key,
};

slint::include_modules!();

const UI_TICK: Duration = Duration::from_millis(16);
const HOTKEY_POLL: Duration = Duration::from_millis(25);
const HOTKEY_RESTART_SUPPRESSION: Duration = Duration::from_millis(300);
const DEFAULT_WINDOW_WIDTH: u32 = 1280;
const DEFAULT_WINDOW_HEIGHT: u32 = 720;
const MIN_WINDOW_WIDTH: u32 = 960;
const MIN_WINDOW_HEIGHT: u32 = 600;
const MAX_WINDOW_WIDTH: u32 = 7680;
const MAX_WINDOW_HEIGHT: u32 = 4320;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ActivationAction {
    Start,
    Stop,
    Ignore,
}

#[derive(Debug, Default)]
struct ActivationGate {
    suppress_starts_until: Option<Instant>,
}

impl ActivationGate {
    fn decide(
        &mut self,
        now: Instant,
        session_active: bool,
        start_permitted: bool,
    ) -> ActivationAction {
        if session_active {
            self.suppress_starts_until = Some(now + HOTKEY_RESTART_SUPPRESSION);
            return ActivationAction::Stop;
        }

        if self
            .suppress_starts_until
            .is_some_and(|deadline| now < deadline)
        {
            return ActivationAction::Ignore;
        }
        self.suppress_starts_until = None;

        if start_permitted {
            ActivationAction::Start
        } else {
            ActivationAction::Ignore
        }
    }
}

#[derive(Debug, Default)]
struct NativeRunState {
    backend: ForegroundBackend,
    target: Option<ForegroundTarget>,
}

fn main() -> Result<(), slint::PlatformError> {
    initialize_diagnostics();

    let app = AppWindow::new()?;
    app.set_app_version(APP_VERSION.into());
    app.set_window_title(window_title().into());
    restore_window_preferences(&app);
    connect_interactions(&app);
    connect_profiles(&app);
    connect_appearance_and_diagnostics(&app);

    let smoke_test = std::env::var_os("AUTOQUILL_SMOKE_TEST").is_some();
    let tray = (!smoke_test).then(|| connect_tray(&app)).flatten();

    if smoke_test {
        let smoke_text = "Hi[ENTER]";
        app.set_draft_text(smoke_text.into());
        app.set_wpm(200);
        app.invoke_draft_edited(smoke_text.into());
        app.invoke_start_simulation();
        app.show()?;
        slint::Timer::single_shot(Duration::from_millis(450), || {
            let _ = slint::quit_event_loop();
        });
        slint::run_event_loop()?;
        assert_eq!(app.get_session_status().as_str(), "COMPLETE");
        assert_eq!(app.get_preview_text().as_str(), "Hi‹ENTER›");
        Ok(())
    } else {
        app.show()?;
        if let Some(tray_instance) = &tray
            && let Err(error) = tray_instance.show()
        {
            let _ = tray_instance.hide();
            app.set_notice_is_error(true);
            app.set_notice_text(
                format!("The system tray is unavailable ({error}). Closing the window will exit AutoQuill.")
                    .into(),
            );
        }
        slint::run_event_loop()
    }
}

fn restore_window_preferences(app: &AppWindow) {
    let Ok(store) = ProfileStore::discover() else {
        return;
    };
    let Ok(preferences) = store.load_preferences() else {
        return;
    };
    app.set_theme_mode(preferences.appearance.theme.as_str().into());
    app.set_reduce_motion(preferences.appearance.reduce_motion);
    let width = preferences
        .window
        .width
        .clamp(MIN_WINDOW_WIDTH, MAX_WINDOW_WIDTH);
    let height = preferences
        .window
        .height
        .clamp(MIN_WINDOW_HEIGHT, MAX_WINDOW_HEIGHT);
    app.window()
        .set_size(LogicalSize::new(width as f32, height as f32));

    if let (Some(x), Some(y)) = (preferences.window.x, preferences.window.y) {
        let position = PhysicalPosition::new(x, y);
        let scale_factor = app.window().scale_factor();
        let physical_size =
            LogicalSize::new(width as f32, height as f32).to_physical(scale_factor.max(1.0));
        if window_position_is_visible(position, physical_size) {
            app.window().set_position(position);
        }
    }
}

fn save_window_preferences(app: &AppWindow, store: &ProfileStore) {
    let Ok(mut preferences) = store.load_preferences() else {
        return;
    };
    let window = app.window();
    let scale_factor = window.scale_factor().max(1.0);
    let logical_size = window.size().to_logical(scale_factor);
    let width = logical_size.width.round() as u32;
    let height = logical_size.height.round() as u32;
    preferences.schema_version = 3;
    preferences.window = WindowPreferences {
        width: width.clamp(MIN_WINDOW_WIDTH, MAX_WINDOW_WIDTH),
        height: height.clamp(MIN_WINDOW_HEIGHT, MAX_WINDOW_HEIGHT),
        x: None,
        y: None,
    };

    let position = window.position();
    if window_position_is_visible(position, window.size()) {
        preferences.window.x = Some(position.x);
        preferences.window.y = Some(position.y);
    }
    let _ = store.save_preferences(&preferences);
}

fn connect_appearance_and_diagnostics(app: &AppWindow) {
    let weak_app = app.as_weak();
    app.on_appearance_edited(move |theme, reduce_motion| {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        let Ok(store) = ProfileStore::discover() else {
            show_error(
                &app,
                "Appearance preferences could not locate the AutoQuill data folder.",
            );
            return;
        };
        let Ok(mut preferences) = store.load_preferences() else {
            show_error(&app, "Appearance preferences could not be loaded.");
            return;
        };
        preferences.schema_version = 3;
        preferences.appearance = AppearancePreferences {
            theme: theme_preference(theme.as_str()),
            reduce_motion,
        };
        if let Err(error) = store.save_preferences(&preferences) {
            show_error(
                &app,
                &format!("Appearance preferences could not be saved: {error}"),
            );
        }
    });

    let weak_app = app.as_weak();
    app.on_copy_diagnostics(move || {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        let report = privacy_safe_report(
            current_capabilities(),
            app.get_theme_mode().as_str(),
            app.get_reduce_motion(),
        );
        match ClipboardContext::new().and_then(|mut clipboard| clipboard.set_contents(report)) {
            Ok(()) => {
                app.set_notice_is_error(false);
                app.set_notice_text(
                    "Privacy-safe diagnostics copied. Typed text and clipboard contents were excluded."
                        .into(),
                );
            }
            Err(error) => show_error(&app, &format!("Diagnostics could not be copied: {error}")),
        }
    });

    let weak_app = app.as_weak();
    app.on_export_diagnostics(move || {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        let Some(destination) = rfd::FileDialog::new()
            .set_title("Export privacy-safe AutoQuill diagnostics")
            .set_file_name(format!("AutoQuill-{APP_VERSION}-diagnostics.txt"))
            .save_file()
        else {
            return;
        };
        let report = privacy_safe_report(
            current_capabilities(),
            app.get_theme_mode().as_str(),
            app.get_reduce_motion(),
        );
        match fs::write(destination, report) {
            Ok(()) => {
                app.set_notice_is_error(false);
                app.set_notice_text(
                    "Privacy-safe diagnostics exported without typed text or clipboard contents."
                        .into(),
                );
            }
            Err(error) => show_error(&app, &format!("Diagnostics could not be exported: {error}")),
        }
    });
}

fn theme_preference(value: &str) -> ThemePreference {
    match value {
        "light" => ThemePreference::Light,
        "dark" => ThemePreference::Dark,
        _ => ThemePreference::System,
    }
}

fn connect_tray(app: &AppWindow) -> Option<AutoQuillTray> {
    let tray = match AutoQuillTray::new() {
        Ok(tray) => tray,
        Err(error) => {
            app.set_notice_is_error(true);
            app.set_notice_text(
                format!("The system tray could not start ({error}). AutoQuill remains fully usable from this window.")
                    .into(),
            );
            return None;
        }
    };

    let weak_app = app.as_weak();
    tray.on_show_window(move || {
        if let Some(app) = weak_app.upgrade() {
            let _ = app.show();
        }
    });

    let weak_app = app.as_weak();
    tray.on_toggle_session(move || {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_session_active() {
            app.invoke_stop_simulation();
        } else if app.get_real_typing_selected() {
            let _ = app.show();
            app.invoke_real_start_help();
        } else if app.get_can_start() {
            app.invoke_start_simulation();
        } else {
            let _ = app.show();
            show_error(
                &app,
                "Add valid text before starting AutoQuill from the tray.",
            );
        }
    });

    let weak_app = app.as_weak();
    let weak_tray = tray.as_weak();
    tray.on_quit_application(move || {
        if let Some(app) = weak_app.upgrade() {
            if app.get_session_active() {
                app.invoke_stop_simulation();
            }
            if let Ok(store) = ProfileStore::discover() {
                save_window_preferences(&app, &store);
            }
        }
        if let Some(tray) = weak_tray.upgrade() {
            let _ = tray.hide();
        }
        let _ = slint::quit_event_loop();
    });

    Some(tray)
}

fn reset_window(app: &AppWindow, store: &ProfileStore) {
    app.window().set_size(LogicalSize::new(
        DEFAULT_WINDOW_WIDTH as f32,
        DEFAULT_WINDOW_HEIGHT as f32,
    ));
    center_window_on_primary(app);
    save_window_preferences(app, store);
    app.set_notice_is_error(false);
    app.set_notice_text("Window restored to 1280 × 720 and centered safely.".into());
}

#[cfg(windows)]
fn window_position_is_visible(position: PhysicalPosition, size: PhysicalSize) -> bool {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetSystemMetrics, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN,
        SM_YVIRTUALSCREEN,
    };

    let left = unsafe { GetSystemMetrics(SM_XVIRTUALSCREEN) } as i64;
    let top = unsafe { GetSystemMetrics(SM_YVIRTUALSCREEN) } as i64;
    let right = left + i64::from(unsafe { GetSystemMetrics(SM_CXVIRTUALSCREEN) });
    let bottom = top + i64::from(unsafe { GetSystemMetrics(SM_CYVIRTUALSCREEN) });
    let x = i64::from(position.x);
    let y = i64::from(position.y);
    let window_right = x + i64::from(size.width);
    let window_bottom = y + i64::from(size.height);
    const MIN_VISIBLE: i64 = 64;

    window_right >= left + MIN_VISIBLE
        && x <= right - MIN_VISIBLE
        && window_bottom >= top + MIN_VISIBLE
        && y <= bottom - MIN_VISIBLE
}

#[cfg(not(windows))]
fn window_position_is_visible(position: PhysicalPosition, _size: PhysicalSize) -> bool {
    position.x.abs() <= 32_768 && position.y.abs() <= 32_768
}

#[cfg(windows)]
fn center_window_on_primary(app: &AppWindow) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN};

    let screen_width = unsafe { GetSystemMetrics(SM_CXSCREEN) };
    let screen_height = unsafe { GetSystemMetrics(SM_CYSCREEN) };
    let scale_factor = app.window().scale_factor().max(1.0);
    let desired = LogicalSize::new(DEFAULT_WINDOW_WIDTH as f32, DEFAULT_WINDOW_HEIGHT as f32)
        .to_physical(scale_factor);
    let x = (screen_width - i32::try_from(desired.width).unwrap_or(screen_width)).max(0) / 2;
    let y = (screen_height - i32::try_from(desired.height).unwrap_or(screen_height)).max(0) / 2;
    app.window().set_position(PhysicalPosition::new(x, y));
}

#[cfg(not(windows))]
fn center_window_on_primary(_app: &AppWindow) {
    // Wayland does not permit applications to set their own position. Other window managers can
    // choose the safest placement after the size is reset.
}

fn connect_interactions(app: &AppWindow) {
    let engine = Rc::new(RefCell::new(
        SessionEngine::<SeededRandom>::from_system_time(),
    ));
    let timer = Rc::new(slint::Timer::default());
    let last_tick = Rc::new(RefCell::new(Instant::now()));
    let native = Rc::new(RefCell::new(NativeRunState::default()));

    apply_platform_capabilities(app, current_capabilities());
    app.set_real_typing_selected(false);
    app.set_real_typing_confirmed(false);
    app.set_target_name("No target captured".into());
    app.set_target_delivery("NOT CAPTURED".into());

    let weak_app = app.as_weak();
    app.on_choose_simulation(move || {
        if let Some(app) = weak_app.upgrade() {
            app.set_real_typing_selected(false);
            app.set_real_typing_confirm_open(false);
            app.set_notice_is_error(false);
            app.set_notice_text("Simulation mode is safe: no keystrokes leave this window.".into());
        }
    });

    let weak_app = app.as_weak();
    app.on_request_real_typing(move || {
        if let Some(app) = weak_app.upgrade() {
            let report = current_capabilities();
            apply_platform_capabilities(&app, report);
            app.set_real_typing_confirm_open(true);
            app.set_notice_is_error(!report.real_typing_available);
            if report.real_typing_available {
                app.set_notice_text(
                    "Review the platform capability notice before enabling Real Typing.".into(),
                );
            } else {
                app.set_notice_text(report.guidance.into());
            }
        }
    });

    let weak_app = app.as_weak();
    app.on_refresh_platform_capabilities(move || {
        if let Some(app) = weak_app.upgrade() {
            let report = current_capabilities();
            apply_platform_capabilities(&app, report);
            app.set_real_typing_confirm_open(true);
            app.set_notice_is_error(!report.real_typing_available);
            app.set_notice_text(
                if report.real_typing_available {
                    "Platform capability re-checked. Review the notice, then enable Real Typing only if you accept its verification level."
                } else {
                    report.guidance
                }
                .into(),
            );
        }
    });

    let weak_app = app.as_weak();
    app.on_confirm_real_typing(move || {
        if let Some(app) = weak_app.upgrade() {
            let report = current_capabilities();
            apply_platform_capabilities(&app, report);
            if !report.real_typing_available {
                app.set_real_typing_confirm_open(true);
                show_error(&app, report.guidance);
                return;
            }
            app.set_real_typing_confirm_open(false);
            app.set_real_typing_confirmed(true);
            app.set_real_typing_selected(true);
            app.set_notice_is_error(false);
            app.set_notice_text(
                format!(
                    "Real Typing is armed for {}. Focus the destination app and press {} to start; press it again or Escape to stop.",
                    report.badge,
                    app.get_shortcut_text()
                )
                .into(),
            );
        }
    });

    let weak_app = app.as_weak();
    app.on_cancel_real_typing(move || {
        if let Some(app) = weak_app.upgrade() {
            app.set_real_typing_confirm_open(false);
            app.set_real_typing_selected(false);
        }
    });

    let weak_app = app.as_weak();
    app.on_real_start_help(move || {
        if let Some(app) = weak_app.upgrade() {
            app.set_notice_is_error(false);
            app.set_notice_text(
                format!(
                    "Focus the exact destination application, then press {}. AutoQuill will capture and validate it before a 2-second countdown.",
                    app.get_shortcut_text()
                )
                .into(),
            );
        }
    });

    let weak_app = app.as_weak();
    app.on_draft_edited(move |text| {
        if let Some(app) = weak_app.upgrade() {
            update_document_summary(&app, text.as_str());
            app.set_profile_modified(true);
            if !app.get_session_active() {
                app.set_notice_is_error(false);
                app.set_notice_text(
                    if app.get_real_typing_selected() {
                        format!(
                            "Draft updated. Focus the destination app and press {} when ready.",
                            app.get_shortcut_text()
                        )
                    } else {
                        "Simulation mode is safe: no keystrokes leave this window.".into()
                    }
                    .into(),
                );
                app.set_session_status("READY".into());
            }
        }
    });

    let weak_app = app.as_weak();
    app.on_insert_token(move |token, cursor, anchor| {
        if let Some(app) = weak_app.upgrade() {
            let current = app.get_draft_text().to_string();
            let (updated, caret) = insert_at_selection(&current, token.as_str(), cursor, anchor);
            app.set_draft_text(updated.clone().into());
            update_document_summary(&app, &updated);
            app.set_profile_modified(true);
            app.invoke_place_editor_caret(caret);
        }
    });

    let weak_app = app.as_weak();
    let start_engine = Rc::clone(&engine);
    let start_timer = Rc::clone(&timer);
    let start_last_tick = Rc::clone(&last_tick);
    let start_native = Rc::clone(&native);
    app.on_start_simulation(move || {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        let source = app.get_draft_text().to_string();
        if source.trim().is_empty() {
            show_error(
                &app,
                "Enter or paste some text before starting the simulation.",
            );
            return;
        }

        let compilation = match compile(&source, &SystemRuntimeValues) {
            Ok(compilation) => compilation,
            Err(error) => {
                show_error(&app, &error.to_string());
                return;
            }
        };
        let warning_notice = summarize_warnings(&compilation.warnings);
        let real_typing = app.get_real_typing_selected();
        let mut settings = settings_from_ui(&app);
        let shortcut = settings.shortcut;
        let mut target_notice = None;

        if real_typing {
            if !app.get_real_typing_confirmed() {
                show_error(&app, "Confirm Real Typing before arming native input.");
                return;
            }
            if !app.get_hotkey_ready() {
                show_error(
                    &app,
                    "The selected activation shortcut is unavailable. Record another shortcut in Advanced Settings.",
                );
                return;
            }
            if document_uses_activation_shortcut(&compilation.instructions, shortcut) {
                show_error(
                    &app,
                    &format!(
                        "This document contains [{shortcut}], which is reserved as the Start/Stop shortcut. Remove that token or choose another activation shortcut."
                    ),
                );
                return;
            }

            let target = match start_native
                .borrow()
                .backend
                .capture(settings.target, &compilation.instructions)
            {
                Ok(target) => target,
                Err(error) => {
                    show_error(&app, &error.to_string());
                    return;
                }
            };
            app.set_target_name(target.label().into());
            app.set_target_delivery(target.delivery_label().into());
            target_notice = Some(match target.delivery() {
                DeliveryStrategy::NativeBackground => format!(
                    "Sticky Background is active for {}. You may change foreground apps; {} or Escape stops immediately.",
                    target.label(), shortcut
                ),
                DeliveryStrategy::ForegroundProtected if settings.target == TargetIntent::StickyAuto => format!(
                    "Sticky Auto selected Foreground Protected for {} because this target or document is not background-safe. Keep it foreground; {} or Escape stops immediately.",
                    target.label(), shortcut
                ),
                DeliveryStrategy::ForegroundProtected => format!(
                    "Foreground Protected: {}. Keep it foreground; {} or Escape stops immediately.",
                    target.label(), shortcut
                ),
            });
            start_native.borrow_mut().target = Some(target);
        } else {
            start_native.borrow_mut().target = None;
            app.set_target_name("No target captured".into());
            app.set_target_delivery("NOT CAPTURED".into());
        }

        if real_typing {
            settings.startup_delay_enabled = true;
            settings.startup_delay_seconds = 2;
        }
        let plan = SessionPlan {
            instructions: compilation.instructions,
            settings,
        };

        start_timer.stop();
        app.set_preview_text("".into());
        let update = match start_engine.borrow_mut().start(plan) {
            Ok(update) => update,
            Err(error) => {
                show_error(&app, &error.to_string());
                return;
            }
        };
        apply_engine_update(&app, &update);
        app.set_notice_is_error(false);
        app.set_notice_text(
            match (target_notice, warning_notice) {
                (Some(target), Some(warning)) => format!("{target} {warning}"),
                (Some(target), None) => target,
                (None, Some(warning)) => warning,
                (None, None) => {
                    "Safe simulation is running. No native input is being sent.".into()
                }
            }
                .into(),
        );

        if !update.snapshot.state.is_active() {
            apply_completion_notice(&app, update.snapshot.completion_reason);
            return;
        }

        *start_last_tick.borrow_mut() = Instant::now();
        let tick_app = app.as_weak();
        let tick_engine = Rc::clone(&start_engine);
        let tick_last_tick = Rc::clone(&start_last_tick);
        let tick_native = Rc::clone(&start_native);
        let weak_timer = Rc::downgrade(&start_timer);
        start_timer.start(slint::TimerMode::Repeated, UI_TICK, move || {
            let Some(app) = tick_app.upgrade() else {
                if let Some(timer) = weak_timer.upgrade() {
                    timer.stop();
                }
                return;
            };

            let real_typing = app.get_real_typing_selected();
            if real_typing {
                let validation = {
                    let native = tick_native.borrow();
                    native
                        .target
                        .as_ref()
                        .ok_or(autoquill::platform::NativeInputError::NoTarget)
                        .and_then(|target| native.backend.validate(target))
                };
                if let Err(error) = validation {
                    let update = tick_engine.borrow_mut().fail(error.to_string());
                    tick_native.borrow_mut().target = None;
                    app.set_target_name("Target lost".into());
                    app.set_target_delivery("STOPPED".into());
                    apply_engine_update(&app, &update);
                    show_error(&app, &error.to_string());
                    if let Some(timer) = weak_timer.upgrade() {
                        timer.stop();
                    }
                    return;
                }
            }

            let now = Instant::now();
            let elapsed = now.saturating_duration_since(*tick_last_tick.borrow());
            *tick_last_tick.borrow_mut() = now;
            let update = tick_engine.borrow_mut().advance(elapsed);

            if real_typing {
                let emission = {
                    let native = tick_native.borrow();
                    let target = native
                        .target
                        .as_ref()
                        .ok_or(autoquill::platform::NativeInputError::NoTarget);
                    target.and_then(|target| {
                        update
                            .operations
                            .iter()
                            .try_for_each(|operation| native.backend.emit(target, operation))
                    })
                };
                if let Err(error) = emission {
                    let failed = tick_engine.borrow_mut().fail(error.to_string());
                    tick_native.borrow_mut().target = None;
                    app.set_target_name("Input stopped".into());
                    app.set_target_delivery("STOPPED".into());
                    apply_engine_update(&app, &failed);
                    show_error(&app, &error.to_string());
                    if let Some(timer) = weak_timer.upgrade() {
                        timer.stop();
                    }
                    return;
                }
            }

            apply_engine_update(&app, &update);
            if !update.snapshot.state.is_active() {
                tick_native.borrow_mut().target = None;
                app.set_target_delivery("COMPLETE".into());
                apply_completion_notice(&app, update.snapshot.completion_reason);
                if let Some(timer) = weak_timer.upgrade() {
                    timer.stop();
                }
            }
        });
    });

    let weak_app = app.as_weak();
    let pause_engine = Rc::clone(&engine);
    app.on_pause_simulation(move || {
        if let Some(app) = weak_app.upgrade() {
            let update = pause_engine.borrow_mut().pause();
            apply_engine_update(&app, &update);
            app.set_notice_is_error(false);
            app.set_notice_text(
                "Paused safely. Paused time does not count toward the active-time limit.".into(),
            );
        }
    });

    let weak_app = app.as_weak();
    let resume_engine = Rc::clone(&engine);
    let resume_last_tick = Rc::clone(&last_tick);
    app.on_resume_simulation(move || {
        if let Some(app) = weak_app.upgrade() {
            *resume_last_tick.borrow_mut() = Instant::now();
            let update = resume_engine.borrow_mut().resume();
            apply_engine_update(&app, &update);
            app.set_notice_is_error(false);
            app.set_notice_text(
                if app.get_real_typing_selected() {
                    "Real Typing resumed with the captured target and selected safety strategy."
                } else {
                    "Safe simulation resumed."
                }
                .into(),
            );
        }
    });

    let weak_app = app.as_weak();
    let stop_engine = Rc::clone(&engine);
    let stop_timer = Rc::clone(&timer);
    let stop_native = Rc::clone(&native);
    app.on_stop_simulation(move || {
        stop_timer.stop();
        stop_native.borrow_mut().target = None;
        if let Some(app) = weak_app.upgrade() {
            let was_real_typing = app.get_real_typing_selected();
            let update = stop_engine.borrow_mut().stop();
            apply_engine_update(&app, &update);
            app.set_target_delivery("STOPPED".into());
            app.set_notice_is_error(false);
            app.set_notice_text(
                if was_real_typing {
                    "Real Typing stopped immediately. Refocus the destination and press the activation key to start again."
                } else {
                    "Simulation stopped safely. You can edit and restart it."
                }
                .into(),
            );
        }
    });

    let weak_app = app.as_weak();
    let reset_engine = Rc::clone(&engine);
    let reset_timer = Rc::clone(&timer);
    let reset_native = Rc::clone(&native);
    app.on_reset_simulation(move || {
        reset_timer.stop();
        reset_native.borrow_mut().target = None;
        if let Some(app) = weak_app.upgrade() {
            let update = reset_engine.borrow_mut().reset();
            app.set_preview_text("".into());
            apply_engine_update(&app, &update);
            app.set_target_name("No target captured".into());
            app.set_target_delivery("NOT CAPTURED".into());
            app.set_notice_is_error(false);
            app.set_notice_text(
                "Preview reset. Your draft and advanced settings were kept.".into(),
            );
        }
    });

    connect_hotkey(app);
}

fn connect_hotkey(app: &AppWindow) {
    if !current_capabilities().global_shortcuts_available {
        app.set_hotkey_ready(false);
        app.set_hotkey_status("GLOBAL KEY UNAVAILABLE".into());
        app.on_shortcut_changed(|_| {});
        app.on_shortcut_recorded(|_, _, _, _, _| {});
        return;
    }

    match HotkeyService::start(shortcut_from_ui(app)) {
        Ok((service, receiver)) => {
            let service = Rc::new(service);
            let receiver = Rc::new(RefCell::new(receiver));
            let hotkey_timer = Rc::new(slint::Timer::default());
            let activation_gate = Rc::new(RefCell::new(ActivationGate::default()));
            let escape_active = Rc::new(RefCell::new(false));

            let weak_app = app.as_weak();
            let key_service = Rc::clone(&service);
            let key_timer = Rc::clone(&hotkey_timer);
            app.on_shortcut_changed(move |text| {
                let _timer = &key_timer;
                if let Some(app) = weak_app.upgrade() {
                    let Ok(shortcut) = text.as_str().parse::<Shortcut>() else {
                        app.set_hotkey_ready(false);
                        app.set_hotkey_status("INVALID SHORTCUT".into());
                        show_error(&app, "Record a supported activation shortcut.");
                        return;
                    };
                    app.set_hotkey_ready(false);
                    app.set_hotkey_status(format!("REGISTERING {shortcut}").into());
                    if let Err(error) = key_service.set_shortcut(shortcut) {
                        app.set_hotkey_status("SHORTCUT ERROR".into());
                        show_error(&app, &error.to_string());
                    }
                }
            });

            let weak_app = app.as_weak();
            app.on_shortcut_recorded(move |text, control, alt, shift, meta| {
                let Some(app) = weak_app.upgrade() else {
                    return;
                };
                match shortcut_from_key_event(text.as_str(), control, alt, shift, meta) {
                    Ok(Some(shortcut)) => {
                        app.set_shortcut_recording(false);
                        app.set_shortcut_text(shortcut.to_string().into());
                        show_notice(
                            &app,
                            &format!("Activation shortcut recorded: {shortcut}."),
                        );
                    }
                    Ok(None) => {
                        app.set_notice_is_error(false);
                        app.set_notice_text(
                            "Keep holding the modifier, then press F1–F12, Space, or a printable key."
                                .into(),
                        );
                    }
                    Err(message) => {
                        app.set_notice_is_error(true);
                        app.set_notice_text(message.into());
                    }
                }
            });

            let weak_app = app.as_weak();
            let event_receiver = Rc::clone(&receiver);
            let event_gate = Rc::clone(&activation_gate);
            let keep_service_alive = Rc::clone(&service);
            let event_escape_active = Rc::clone(&escape_active);
            let weak_timer = Rc::downgrade(&hotkey_timer);
            hotkey_timer.start(slint::TimerMode::Repeated, HOTKEY_POLL, move || {
                let Some(app) = weak_app.upgrade() else {
                    if let Some(timer) = weak_timer.upgrade() {
                        timer.stop();
                    }
                    return;
                };

                let should_capture_escape =
                    app.get_session_active() && app.get_real_typing_selected();
                if should_capture_escape != *event_escape_active.borrow() {
                    if let Err(error) = keep_service_alive.set_escape_active(should_capture_escape)
                    {
                        app.set_notice_is_error(true);
                        app.set_notice_text(error.to_string().into());
                    }
                    *event_escape_active.borrow_mut() = should_capture_escape;
                }

                while let Ok(event) = event_receiver.borrow().try_recv() {
                    match event {
                        HotkeyEvent::Pressed => {
                            let start_permitted = app.get_hotkey_ready()
                                && app.get_can_start()
                                && (!app.get_real_typing_selected()
                                    || app.get_real_typing_confirmed());
                            match event_gate.borrow_mut().decide(
                                Instant::now(),
                                app.get_session_active(),
                                start_permitted,
                            ) {
                                ActivationAction::Start => app.invoke_start_simulation(),
                                ActivationAction::Stop => app.invoke_stop_simulation(),
                                ActivationAction::Ignore => {}
                            }
                        }
                        HotkeyEvent::EmergencyStop => {
                            if app.get_session_active() {
                                app.invoke_stop_simulation();
                            }
                        }
                        HotkeyEvent::Registered(shortcut) => {
                            if shortcut == shortcut_from_ui(&app) {
                                app.set_hotkey_ready(true);
                                app.set_hotkey_status(
                                    format!("{shortcut} READY • START / STOP").into(),
                                );
                            }
                        }
                        HotkeyEvent::RegistrationFailed {
                            shortcut, message, ..
                        } => {
                            if shortcut == shortcut_from_ui(&app) {
                                app.set_hotkey_ready(false);
                                app.set_hotkey_status("SHORTCUT UNAVAILABLE".into());
                                show_error(&app, &message);
                            }
                        }
                        HotkeyEvent::EscapeRegistrationFailed(message) => {
                            app.set_notice_is_error(true);
                            app.set_notice_text(message.into());
                        }
                    }
                }
            });
        }
        Err(error) => {
            app.set_hotkey_ready(false);
            app.set_hotkey_status("GLOBAL KEY UNAVAILABLE".into());
            if current_capabilities().global_shortcuts_available {
                show_error(app, &error.to_string());
            }
            app.on_shortcut_changed(|_| {});
            app.on_shortcut_recorded(|_, _, _, _, _| {});
        }
    }
}

fn apply_platform_capabilities(app: &AppWindow, report: CapabilityReport) {
    app.set_real_typing_available(report.real_typing_available);
    app.set_platform_status(report.badge.into());
    app.set_platform_summary(report.summary.into());
    app.set_platform_guidance(report.guidance.into());
    app.set_platform_footer(report.footer.into());
    app.set_native_behavior_verified(report.verification == VerificationLevel::VerifiedBeta);
    app.set_sticky_typing_supported(report.sticky_background_available);
    if !report.sticky_background_available {
        app.set_sticky_typing(false);
    }
    if !report.real_typing_available {
        app.set_real_typing_selected(false);
        app.set_real_typing_confirmed(false);
    }
}

fn shortcut_from_ui(app: &AppWindow) -> Shortcut {
    app.get_shortcut_text().as_str().parse().unwrap_or_default()
}

fn shortcut_from_key_event(
    text: &str,
    control: bool,
    alt: bool,
    shift: bool,
    meta: bool,
) -> Result<Option<Shortcut>, String> {
    let Some(character) = text.chars().next() else {
        return Ok(None);
    };
    if text.chars().count() != 1 {
        return Err("That key is not supported. Use F1–F12, Space, or one printable key.".into());
    }

    let modifier_keys = [
        Key::Control,
        Key::ControlR,
        Key::Alt,
        Key::AltGr,
        Key::Shift,
        Key::ShiftR,
        Key::Meta,
        Key::MetaR,
    ];
    if modifier_keys
        .into_iter()
        .any(|key| character == char::from(key))
    {
        return Ok(None);
    }

    let key = [
        Key::F1,
        Key::F2,
        Key::F3,
        Key::F4,
        Key::F5,
        Key::F6,
        Key::F7,
        Key::F8,
        Key::F9,
        Key::F10,
        Key::F11,
        Key::F12,
    ]
    .into_iter()
    .position(|key| character == char::from(key))
    .map_or_else(
        || {
            if character == char::from(Key::Space) {
                Ok(ShortcutKey::Space)
            } else if character.is_ascii_graphic() {
                Ok(ShortcutKey::Character(character))
            } else {
                Err("That key is not supported. Use F1–F12, Space, or one printable key.")
            }
        },
        |index| Ok(ShortcutKey::Function((index + 1) as u8)),
    )?;

    let modifiers = ModifierSet {
        control,
        alt,
        shift,
        meta,
    };
    if matches!(key, ShortcutKey::Character(_) | ShortcutKey::Space) && modifiers.is_empty() {
        return Err(
            "Letters, symbols, and Space need Ctrl, Alt, Shift, or Win so normal typing remains usable. Keep recording and try a modifier combination."
                .into(),
        );
    }

    Shortcut::new(modifiers, key)
        .map(Some)
        .map_err(|error| format!("That shortcut is not supported: {error}."))
}

#[derive(Debug, Clone, PartialEq)]
struct ProfileSnapshot {
    text: String,
    settings: TypingSettings,
}

impl ProfileSnapshot {
    fn from_ui(app: &AppWindow) -> Self {
        Self {
            text: app.get_draft_text().to_string(),
            settings: settings_from_ui(app),
        }
    }

    fn into_profile(self, name: &str) -> Profile {
        Profile::new(name, self.settings, self.text)
    }
}

#[derive(Debug, Clone)]
enum PendingAction {
    Load(String),
    New,
    Close,
}

#[derive(Debug, Clone)]
enum DialogAction {
    SaveAs(Option<PendingAction>),
    Rename(String),
    Duplicate(String),
    Delete(String),
    Upgrade(Option<PendingAction>),
    Dirty(PendingAction),
    Import(Vec<ImportCandidate>),
}

struct ProfileController {
    store: ProfileStore,
    visible: Vec<ProfileSummary>,
    selected_name: Option<String>,
    current_name: Option<String>,
    current_requires_upgrade: bool,
    baseline: ProfileSnapshot,
    dialog: Option<DialogAction>,
}

fn connect_profiles(app: &AppWindow) {
    let store = match ProfileStore::discover() {
        Ok(store) => store,
        Err(error) => {
            show_error(app, &format!("Profiles are unavailable: {error}"));
            return;
        }
    };
    let controller = Rc::new(RefCell::new(ProfileController {
        store,
        visible: Vec::new(),
        selected_name: None,
        current_name: None,
        current_requires_upgrade: false,
        baseline: ProfileSnapshot::from_ui(app),
        dialog: None,
    }));

    let weak_app = app.as_weak();
    let state = Rc::clone(&controller);
    app.on_reset_window(move || {
        if let Some(app) = weak_app.upgrade() {
            reset_window(&app, &state.borrow().store);
        }
    });

    let startup = controller.borrow().store.startup_profile();
    match startup {
        Ok(Some(profile)) => apply_loaded_profile(app, &controller, profile),
        Ok(None) => {
            controller.borrow_mut().baseline = ProfileSnapshot::from_ui(app);
            app.set_profile_modified(false);
        }
        Err(error) => show_error(
            app,
            &format!("The last profile could not be reopened: {error}"),
        ),
    }
    refresh_profile_list(app, &controller);

    let weak_app = app.as_weak();
    let state = Rc::clone(&controller);
    app.on_settings_edited(move || {
        if let Some(app) = weak_app.upgrade() {
            update_dirty_state(&app, &state);
        }
    });

    let weak_app = app.as_weak();
    let state = Rc::clone(&controller);
    app.on_open_profile_manager(move || {
        if let Some(app) = weak_app.upgrade() {
            update_dirty_state(&app, &state);
            refresh_profile_list(&app, &state);
            app.set_advanced_open(false);
            app.set_profile_manager_open(true);
        }
    });

    let weak_app = app.as_weak();
    let state = Rc::clone(&controller);
    app.on_profile_refresh(move || {
        if let Some(app) = weak_app.upgrade() {
            refresh_profile_list(&app, &state);
        }
    });

    let weak_app = app.as_weak();
    let state = Rc::clone(&controller);
    app.on_profile_search_edited(move |_| {
        if let Some(app) = weak_app.upgrade() {
            state.borrow_mut().selected_name = None;
            refresh_profile_list(&app, &state);
        }
    });

    let weak_app = app.as_weak();
    let state = Rc::clone(&controller);
    app.on_profile_selected(move |index| {
        if let Some(app) = weak_app.upgrade() {
            let selected = usize::try_from(index).ok().and_then(|index| {
                state
                    .borrow()
                    .visible
                    .get(index)
                    .map(|item| item.name.clone())
            });
            state.borrow_mut().selected_name = selected;
            app.set_selected_profile_index(index);
        }
    });

    let weak_app = app.as_weak();
    let state = Rc::clone(&controller);
    app.on_profile_load(move || {
        if let Some(app) = weak_app.upgrade()
            && let Some(name) = state.borrow().selected_name.clone()
        {
            request_pending(&app, &state, PendingAction::Load(name));
        }
    });

    let weak_app = app.as_weak();
    let state = Rc::clone(&controller);
    app.on_profile_new(move || {
        if let Some(app) = weak_app.upgrade() {
            request_pending(&app, &state, PendingAction::New);
        }
    });

    let weak_app = app.as_weak();
    let state = Rc::clone(&controller);
    app.on_profile_save(move || {
        if let Some(app) = weak_app.upgrade() {
            begin_save(&app, &state, None);
        }
    });

    let weak_app = app.as_weak();
    let state = Rc::clone(&controller);
    app.on_profile_save_as(move || {
        if let Some(app) = weak_app.upgrade() {
            prompt_save_as(&app, &state, None);
        }
    });

    let weak_app = app.as_weak();
    let state = Rc::clone(&controller);
    app.on_profile_rename(move || {
        if let Some(app) = weak_app.upgrade()
            && let Some(name) = state.borrow().selected_name.clone()
        {
            show_input_dialog(
                &app,
                &state,
                DialogAction::Rename(name.clone()),
                "Rename profile",
                "Choose a new name. The profile contents are not rewritten.",
                &name,
                "RENAME",
            );
        }
    });

    let weak_app = app.as_weak();
    let state = Rc::clone(&controller);
    app.on_profile_duplicate(move || {
        if let Some(app) = weak_app.upgrade()
            && let Some(name) = state.borrow().selected_name.clone()
        {
            show_input_dialog(
                &app,
                &state,
                DialogAction::Duplicate(name.clone()),
                "Duplicate profile",
                "Create an independent copy with a new name.",
                &format!("{name} Copy"),
                "DUPLICATE",
            );
        }
    });

    let weak_app = app.as_weak();
    let state = Rc::clone(&controller);
    app.on_profile_delete(move || {
        if let Some(app) = weak_app.upgrade()
            && let Some(name) = state.borrow().selected_name.clone()
        {
            show_dialog(
                &app,
                &state,
                DialogAction::Delete(name.clone()),
                "Move profile to Trash?",
                &format!(
                    "'{name}' will be moved to Jivaro/AutoQuill/Data/Trash so it remains recoverable."
                ),
                "MOVE TO TRASH",
                "CANCEL",
                "",
            );
        }
    });

    let weak_app = app.as_weak();
    let state = Rc::clone(&controller);
    app.on_profile_default(move || {
        if let Some(app) = weak_app.upgrade()
            && let Some(name) = state.borrow().selected_name.clone()
        {
            let result = state.borrow().store.set_default(Some(&name));
            match result {
                Ok(()) => {
                    show_notice(&app, &format!("'{name}' is now the default profile."));
                    refresh_profile_list(&app, &state);
                }
                Err(error) => show_error(&app, &error.to_string()),
            }
        }
    });

    connect_import_callbacks(app, &controller);

    let weak_app = app.as_weak();
    let state = Rc::clone(&controller);
    app.on_profile_export(move || {
        if let Some(app) = weak_app.upgrade()
            && let Some(name) = state.borrow().selected_name.clone()
            && let Some(destination) = rfd::FileDialog::new()
                .add_filter("AutoQuill profile", &["json"])
                .set_file_name(format!("{name}.json"))
                .save_file()
        {
            let result = state.borrow().store.export(&name, &destination);
            match result {
                Ok(()) => show_notice(&app, &format!("Exported '{name}' without changing it.")),
                Err(error) => show_error(&app, &error.to_string()),
            }
        }
    });

    connect_dialog_callbacks(app, &controller);

    let weak_app = app.as_weak();
    let state = Rc::clone(&controller);
    app.window().on_close_requested(move || {
        let Some(app) = weak_app.upgrade() else {
            return CloseRequestResponse::HideWindow;
        };
        update_dirty_state(&app, &state);
        if app.get_profile_modified() {
            request_pending(&app, &state, PendingAction::Close);
            CloseRequestResponse::KeepWindowShown
        } else {
            save_window_preferences(&app, &state.borrow().store);
            CloseRequestResponse::HideWindow
        }
    });
}

fn connect_import_callbacks(app: &AppWindow, controller: &Rc<RefCell<ProfileController>>) {
    let weak_app = app.as_weak();
    let state = Rc::clone(controller);
    app.on_profile_import_files(move || {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if let Some(paths) = rfd::FileDialog::new()
            .add_filter("AutoQuill profiles", &["json"])
            .pick_files()
        {
            preview_import(&app, &state, paths);
        }
    });

    let weak_app = app.as_weak();
    let state = Rc::clone(controller);
    app.on_profile_import_folder(move || {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if let Some(path) = rfd::FileDialog::new().pick_folder() {
            preview_import(&app, &state, vec![path]);
        }
    });
}

fn preview_import(
    app: &AppWindow,
    controller: &Rc<RefCell<ProfileController>>,
    paths: Vec<PathBuf>,
) {
    let result = controller.borrow().store.preview_import(&paths);
    match result {
        Ok(candidates) if candidates.is_empty() => {
            show_error(app, "No JSON profiles were found in that selection.");
        }
        Ok(candidates) => {
            let supported = candidates
                .iter()
                .filter(|candidate| {
                    matches!(
                        candidate.status,
                        ProfileStatus::Current | ProfileStatus::Legacy
                    )
                })
                .count();
            let conflicts = candidates
                .iter()
                .filter(|candidate| candidate.conflicts)
                .count();
            show_dialog(
                app,
                controller,
                DialogAction::Import(candidates),
                "Import profiles?",
                &format!(
                    "{supported} supported profile(s) are ready with {conflicts} name conflict(s). Keep Both is safest; Overwrite replaces only matching names. Nothing opens automatically."
                ),
                "KEEP BOTH",
                "OVERWRITE",
                "CANCEL",
            );
        }
        Err(error) => show_error(app, &error.to_string()),
    }
}

fn connect_dialog_callbacks(app: &AppWindow, controller: &Rc<RefCell<ProfileController>>) {
    let weak_app = app.as_weak();
    let state = Rc::clone(controller);
    app.on_dialog_primary(move || {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        let action = state.borrow_mut().dialog.take();
        match action {
            Some(DialogAction::SaveAs(after)) => {
                let name = app.get_dialog_input().to_string();
                if save_as(&app, &state, &name) {
                    dismiss_dialog(&app);
                    if let Some(after) = after {
                        execute_pending(&app, &state, after);
                    }
                } else {
                    state.borrow_mut().dialog = Some(DialogAction::SaveAs(after));
                }
            }
            Some(DialogAction::Rename(source)) => {
                let destination = app.get_dialog_input().to_string();
                let result = state.borrow().store.rename(&source, &destination);
                match result {
                    Ok(destination) => {
                        if state.borrow().current_name.as_deref() == Some(&source) {
                            state.borrow_mut().current_name = Some(destination.clone());
                            app.set_current_profile_name(destination.clone().into());
                        }
                        state.borrow_mut().selected_name = Some(destination);
                        dismiss_dialog(&app);
                        refresh_profile_list(&app, &state);
                    }
                    Err(error) => {
                        state.borrow_mut().dialog = Some(DialogAction::Rename(source));
                        show_error(&app, &error.to_string());
                    }
                }
            }
            Some(DialogAction::Duplicate(source)) => {
                let destination = app.get_dialog_input().to_string();
                let result = state.borrow().store.duplicate(&source, &destination);
                match result {
                    Ok(destination) => {
                        state.borrow_mut().selected_name = Some(destination);
                        dismiss_dialog(&app);
                        refresh_profile_list(&app, &state);
                    }
                    Err(error) => {
                        state.borrow_mut().dialog = Some(DialogAction::Duplicate(source));
                        show_error(&app, &error.to_string());
                    }
                }
            }
            Some(DialogAction::Delete(name)) => {
                let result = state.borrow().store.move_to_trash(&name);
                match result {
                    Ok(path) => {
                        if state.borrow().current_name.as_deref() == Some(&name) {
                            execute_pending(&app, &state, PendingAction::New);
                        }
                        state.borrow_mut().selected_name = None;
                        dismiss_dialog(&app);
                        refresh_profile_list(&app, &state);
                        show_notice(&app, &format!("Moved '{name}' to {}.", path.display()));
                    }
                    Err(error) => show_error(&app, &error.to_string()),
                }
            }
            Some(DialogAction::Upgrade(after)) => {
                if save_current(&app, &state, true) {
                    dismiss_dialog(&app);
                    if let Some(after) = after {
                        execute_pending(&app, &state, after);
                    }
                }
            }
            Some(DialogAction::Dirty(pending)) => {
                begin_save(&app, &state, Some(pending));
            }
            Some(DialogAction::Import(candidates)) => {
                perform_import(&app, &state, &candidates, ImportConflictPolicy::KeepBoth);
            }
            None => dismiss_dialog(&app),
        }
    });

    let weak_app = app.as_weak();
    let state = Rc::clone(controller);
    app.on_dialog_secondary(move || {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        let action = state.borrow_mut().dialog.take();
        dismiss_dialog(&app);
        match action {
            Some(DialogAction::Dirty(pending)) => execute_pending(&app, &state, pending),
            Some(DialogAction::Import(candidates)) => {
                perform_import(&app, &state, &candidates, ImportConflictPolicy::Overwrite);
            }
            _ => {}
        }
    });

    let weak_app = app.as_weak();
    let state = Rc::clone(controller);
    app.on_dialog_tertiary(move || {
        if let Some(app) = weak_app.upgrade() {
            state.borrow_mut().dialog = None;
            dismiss_dialog(&app);
        }
    });
}

fn perform_import(
    app: &AppWindow,
    controller: &Rc<RefCell<ProfileController>>,
    candidates: &[ImportCandidate],
    policy: ImportConflictPolicy,
) {
    let result = controller.borrow().store.import(candidates, policy);
    match result {
        Ok(report) => {
            dismiss_dialog(app);
            refresh_profile_list(app, controller);
            show_notice(
                app,
                &format!(
                    "Imported {} profile(s); skipped {}; {} error(s).",
                    report.imported.len(),
                    report.skipped.len(),
                    report.errors.len()
                ),
            );
        }
        Err(error) => show_error(app, &error.to_string()),
    }
}

fn request_pending(
    app: &AppWindow,
    controller: &Rc<RefCell<ProfileController>>,
    pending: PendingAction,
) {
    update_dirty_state(app, controller);
    if app.get_profile_modified() {
        show_dialog(
            app,
            controller,
            DialogAction::Dirty(pending),
            "Save your changes?",
            "This profile has unsaved text or settings. Save it, discard the changes, or cancel.",
            "SAVE",
            "DISCARD",
            "CANCEL",
        );
    } else {
        execute_pending(app, controller, pending);
    }
}

fn execute_pending(
    app: &AppWindow,
    controller: &Rc<RefCell<ProfileController>>,
    pending: PendingAction,
) {
    match pending {
        PendingAction::Load(name) => {
            let result = controller.borrow().store.load(&name);
            match result {
                Ok(profile) => {
                    if let Err(error) = controller.borrow().store.remember_loaded(&name) {
                        show_error(app, &error.to_string());
                        return;
                    }
                    apply_loaded_profile(app, controller, profile);
                    app.set_profile_manager_open(false);
                    show_notice(app, &format!("Loaded '{name}'."));
                }
                Err(error) => show_error(app, &error.to_string()),
            }
        }
        PendingAction::New => {
            apply_settings_to_ui(app, &TypingSettings::default());
            app.set_draft_text("".into());
            update_document_summary(app, "");
            app.set_current_profile_name("Unsaved".into());
            app.set_profile_modified(false);
            app.set_profile_manager_open(false);
            let mut state = controller.borrow_mut();
            state.current_name = None;
            state.current_requires_upgrade = false;
            state.baseline = ProfileSnapshot::from_ui(app);
        }
        PendingAction::Close => {
            save_window_preferences(app, &controller.borrow().store);
            let _ = app.hide();
        }
    }
}

fn begin_save(
    app: &AppWindow,
    controller: &Rc<RefCell<ProfileController>>,
    after: Option<PendingAction>,
) {
    let (name, requires_upgrade) = {
        let state = controller.borrow();
        (state.current_name.clone(), state.current_requires_upgrade)
    };
    match name {
        None => prompt_save_as(app, controller, after),
        Some(_) if requires_upgrade => show_dialog(
            app,
            controller,
            DialogAction::Upgrade(after),
            "Upgrade legacy profile?",
            "AutoQuill will first create an exact backup in Data/Backups, then save this profile using the current portable format.",
            "BACK UP & UPGRADE",
            "CANCEL",
            "",
        ),
        Some(_) => {
            if save_current(app, controller, false)
                && let Some(after) = after
            {
                execute_pending(app, controller, after);
            }
        }
    }
}

fn save_current(
    app: &AppWindow,
    controller: &Rc<RefCell<ProfileController>>,
    upgrade: bool,
) -> bool {
    let Some(name) = controller.borrow().current_name.clone() else {
        return false;
    };
    let snapshot = ProfileSnapshot::from_ui(app);
    let profile = if upgrade {
        Profile::from_legacy(&name, snapshot.settings.clone(), snapshot.text.clone())
    } else {
        snapshot.clone().into_profile(&name)
    };
    let result = if upgrade {
        controller
            .borrow()
            .store
            .upgrade_legacy(&name, &profile)
            .map(|_| name.clone())
    } else {
        controller.borrow().store.save(&name, &profile)
    };
    match result {
        Ok(_) => {
            let mut state = controller.borrow_mut();
            state.baseline = snapshot;
            state.current_requires_upgrade = false;
            drop(state);
            app.set_profile_modified(false);
            app.set_current_profile_name(name.clone().into());
            refresh_profile_list(app, controller);
            show_notice(app, &format!("Saved '{name}'."));
            true
        }
        Err(error) => {
            show_error(app, &error.to_string());
            false
        }
    }
}

fn save_as(app: &AppWindow, controller: &Rc<RefCell<ProfileController>>, name: &str) -> bool {
    let snapshot = ProfileSnapshot::from_ui(app);
    let profile = snapshot.clone().into_profile(name);
    let result = controller.borrow().store.save(name, &profile);
    match result {
        Ok(name) => {
            let mut state = controller.borrow_mut();
            state.current_name = Some(name.clone());
            state.current_requires_upgrade = false;
            state.baseline = snapshot;
            state.selected_name = Some(name.clone());
            drop(state);
            app.set_current_profile_name(name.clone().into());
            app.set_profile_modified(false);
            refresh_profile_list(app, controller);
            show_notice(app, &format!("Saved '{name}'."));
            true
        }
        Err(error) => {
            show_error(app, &error.to_string());
            false
        }
    }
}

fn prompt_save_as(
    app: &AppWindow,
    controller: &Rc<RefCell<ProfileController>>,
    after: Option<PendingAction>,
) {
    let suggested = controller
        .borrow()
        .current_name
        .clone()
        .unwrap_or_else(|| "My Profile".to_owned());
    show_input_dialog(
        app,
        controller,
        DialogAction::SaveAs(after),
        "Save profile as",
        "Save the text and every current setting as one portable JSON profile.",
        &suggested,
        "SAVE",
    );
}

fn apply_loaded_profile(
    app: &AppWindow,
    controller: &Rc<RefCell<ProfileController>>,
    profile: Profile,
) {
    let requires_upgrade = profile.requires_upgrade();
    apply_settings_to_ui(app, &profile.settings);
    app.set_draft_text(profile.typing_text.clone().into());
    update_document_summary(app, &profile.typing_text);
    app.set_current_profile_name(profile.metadata.name.clone().into());
    app.set_profile_modified(false);
    let mut state = controller.borrow_mut();
    state.current_name = Some(profile.metadata.name.clone());
    state.selected_name = Some(profile.metadata.name);
    state.current_requires_upgrade = requires_upgrade;
    state.baseline = ProfileSnapshot::from_ui(app);
}

fn apply_settings_to_ui(app: &AppWindow, settings: &TypingSettings) {
    app.set_shortcut_text(settings.shortcut.to_string().into());
    app.set_sticky_typing(
        app.get_sticky_typing_supported() && settings.target == TargetIntent::StickyAuto,
    );
    app.set_wpm(i32::from(settings.wpm.get()));
    app.set_startup_delay_enabled(settings.startup_delay_enabled);
    app.set_stop_after_enabled(settings.stop_after.enabled);
    app.set_stop_after_seconds(i32::try_from(settings.stop_after.seconds).unwrap_or(i32::MAX));
    app.set_loop_enabled(settings.looping.enabled);
    app.set_loop_min_seconds(i32::try_from(settings.looping.min_seconds).unwrap_or(i32::MAX));
    app.set_loop_max_seconds(i32::try_from(settings.looping.max_seconds).unwrap_or(i32::MAX));
    app.set_errors_enabled(settings.errors.enabled);
    app.set_error_min_interval(i32::try_from(settings.errors.min_interval).unwrap_or(i32::MAX));
    app.set_error_max_interval(i32::try_from(settings.errors.max_interval).unwrap_or(i32::MAX));
    app.set_error_min_count(i32::try_from(settings.errors.min_errors).unwrap_or(i32::MAX));
    app.set_error_max_count(i32::try_from(settings.errors.max_errors).unwrap_or(i32::MAX));
    app.set_breaks_enabled(settings.breaks.enabled);
    app.set_break_min_words(i32::try_from(settings.breaks.min_words).unwrap_or(i32::MAX));
    app.set_break_max_words(i32::try_from(settings.breaks.max_words).unwrap_or(i32::MAX));
    app.set_break_min_milliseconds((settings.breaks.min_seconds * 1_000.0).round() as i32);
    app.set_break_max_milliseconds((settings.breaks.max_seconds * 1_000.0).round() as i32);
    app.set_pauses_enabled(settings.pauses.enabled);
    app.set_pause_min_characters(i32::try_from(settings.pauses.min_characters).unwrap_or(i32::MAX));
    app.set_pause_max_characters(i32::try_from(settings.pauses.max_characters).unwrap_or(i32::MAX));
    app.set_pause_min_milliseconds((settings.pauses.min_seconds * 1_000.0).round() as i32);
    app.set_pause_max_milliseconds((settings.pauses.max_seconds * 1_000.0).round() as i32);
}

fn update_dirty_state(app: &AppWindow, controller: &Rc<RefCell<ProfileController>>) {
    app.set_profile_modified(ProfileSnapshot::from_ui(app) != controller.borrow().baseline);
}

fn refresh_profile_list(app: &AppWindow, controller: &Rc<RefCell<ProfileController>>) {
    let search = app.get_profile_search().trim().to_lowercase();
    let result = controller.borrow().store.list();
    let all = match result {
        Ok(profiles) => profiles,
        Err(error) => {
            show_error(app, &error.to_string());
            return;
        }
    };
    let selected_name = controller.borrow().selected_name.clone();
    let visible: Vec<_> = all
        .into_iter()
        .filter(|profile| search.is_empty() || profile.name.to_lowercase().contains(&search))
        .collect();
    let selected_index = selected_name
        .as_ref()
        .and_then(|selected| visible.iter().position(|profile| &profile.name == selected))
        .and_then(|index| i32::try_from(index).ok())
        .unwrap_or(-1);
    let rows: Vec<ProfileListItem> = visible
        .iter()
        .map(|profile| {
            let mut labels = Vec::new();
            if profile.is_default {
                labels.push("default");
            }
            if profile.is_imported {
                labels.push("imported");
            }
            labels.push(if profile.bytes < 1_024 {
                "under 1 KB"
            } else {
                "portable JSON"
            });
            ProfileListItem {
                name: profile.name.clone().into(),
                badge: profile.status.badge().into(),
                detail: labels.join(" • ").into(),
            }
        })
        .collect();
    controller.borrow_mut().visible = visible;
    app.set_selected_profile_index(selected_index);
    app.set_profile_items(ModelRc::new(VecModel::from(rows)));
}

fn show_input_dialog(
    app: &AppWindow,
    controller: &Rc<RefCell<ProfileController>>,
    action: DialogAction,
    title: &str,
    message: &str,
    input: &str,
    primary: &str,
) {
    app.set_dialog_input(input.into());
    app.set_dialog_input_visible(true);
    show_dialog(
        app, controller, action, title, message, primary, "CANCEL", "",
    );
}

#[allow(clippy::too_many_arguments)]
fn show_dialog(
    app: &AppWindow,
    controller: &Rc<RefCell<ProfileController>>,
    action: DialogAction,
    title: &str,
    message: &str,
    primary: &str,
    secondary: &str,
    tertiary: &str,
) {
    controller.borrow_mut().dialog = Some(action);
    app.set_dialog_title(title.into());
    app.set_dialog_message(message.into());
    app.set_dialog_primary_label(primary.into());
    app.set_dialog_secondary_label(secondary.into());
    app.set_dialog_tertiary_label(tertiary.into());
    if app.get_dialog_input().is_empty() {
        app.set_dialog_input_visible(false);
    }
    app.set_dialog_open(true);
}

fn dismiss_dialog(app: &AppWindow) {
    app.set_dialog_open(false);
    app.set_dialog_input_visible(false);
    app.set_dialog_input("".into());
}

fn show_notice(app: &AppWindow, message: &str) {
    app.set_notice_is_error(false);
    app.set_notice_text(message.into());
}

fn settings_from_ui(app: &AppWindow) -> autoquill::domain::TypingSettings {
    SettingsDraft {
        shortcut: shortcut_from_ui(app),
        wpm: Some(i64::from(app.get_wpm())),
        sticky_typing: app.get_sticky_typing(),
        startup_delay_enabled: app.get_startup_delay_enabled(),
        stop_after_enabled: app.get_stop_after_enabled(),
        stop_after_seconds: Some(i64::from(app.get_stop_after_seconds())),
        loop_enabled: app.get_loop_enabled(),
        loop_min_seconds: Some(i64::from(app.get_loop_min_seconds())),
        loop_max_seconds: Some(i64::from(app.get_loop_max_seconds())),
        errors_enabled: app.get_errors_enabled(),
        error_min_interval: Some(i64::from(app.get_error_min_interval())),
        error_max_interval: Some(i64::from(app.get_error_max_interval())),
        error_min_count: Some(i64::from(app.get_error_min_count())),
        error_max_count: Some(i64::from(app.get_error_max_count())),
        breaks_enabled: app.get_breaks_enabled(),
        break_min_words: Some(i64::from(app.get_break_min_words())),
        break_max_words: Some(i64::from(app.get_break_max_words())),
        break_min_seconds: Some(f64::from(app.get_break_min_milliseconds()) / 1_000.0),
        break_max_seconds: Some(f64::from(app.get_break_max_milliseconds()) / 1_000.0),
        pauses_enabled: app.get_pauses_enabled(),
        pause_min_characters: Some(i64::from(app.get_pause_min_characters())),
        pause_max_characters: Some(i64::from(app.get_pause_max_characters())),
        pause_min_seconds: Some(f64::from(app.get_pause_min_milliseconds()) / 1_000.0),
        pause_max_seconds: Some(f64::from(app.get_pause_max_milliseconds()) / 1_000.0),
        ..SettingsDraft::default()
    }
    .normalize()
}

fn update_document_summary(app: &AppWindow, text: &str) {
    let (characters, words) = document_counts(text);
    app.set_character_count(characters);
    app.set_word_count(words);
    app.set_can_start(!app.get_session_active() && !text.trim().is_empty());
}

fn document_counts(text: &str) -> (i32, i32) {
    let characters = i32::try_from(text.chars().count()).unwrap_or(i32::MAX);
    let words = i32::try_from(text.split_whitespace().count()).unwrap_or(i32::MAX);
    (characters, words)
}

fn insert_at_selection(text: &str, token: &str, cursor: i32, anchor: i32) -> (String, i32) {
    let cursor = byte_boundary(text, cursor);
    let anchor = if anchor < 0 {
        cursor
    } else {
        byte_boundary(text, anchor)
    };
    let start = cursor.min(anchor);
    let end = cursor.max(anchor);
    let mut updated = String::with_capacity(text.len() - (end - start) + token.len());
    updated.push_str(&text[..start]);
    updated.push_str(token);
    updated.push_str(&text[end..]);
    let caret = i32::try_from(start + token.len()).unwrap_or(i32::MAX);
    (updated, caret)
}

fn byte_boundary(text: &str, raw: i32) -> usize {
    if raw < 0 {
        return text.len();
    }
    let mut index = usize::try_from(raw).unwrap_or(text.len()).min(text.len());
    while !text.is_char_boundary(index) {
        index -= 1;
    }
    index
}

fn summarize_warnings(warnings: &[CompileWarning]) -> Option<String> {
    let first = warnings.first()?;
    let remaining = warnings.len() - 1;
    Some(if remaining == 0 {
        format!("{} {}", first.token, first.message)
    } else {
        format!(
            "{} {} (+{remaining} more warning{})",
            first.token,
            first.message,
            if remaining == 1 { "" } else { "s" }
        )
    })
}

fn apply_engine_update(app: &AppWindow, update: &EngineUpdate) {
    if !update.operations.is_empty() {
        let mut preview = app.get_preview_text().to_string();
        for operation in &update.operations {
            match operation {
                PreviewOperation::Intended(instruction) => {
                    preview.push_str(&instruction.preview_fragment());
                }
                PreviewOperation::TypoCharacter(character) => preview.push(*character),
                PreviewOperation::CorrectionBackspace => {
                    preview.pop();
                }
            }
        }
        app.set_preview_text(preview.into());
    }

    let snapshot = &update.snapshot;
    let is_active = snapshot.state.is_active();
    app.set_session_active(is_active);
    app.set_session_paused(snapshot.state == SessionState::Paused);
    app.set_can_start(!is_active && !app.get_draft_text().trim().is_empty());
    app.set_simulation_progress(snapshot.progress);
    app.set_current_action(snapshot.current_action.clone().into());
    app.set_instruction_progress(
        format!(
            "{} / {} ACTIONS",
            snapshot.completed_instructions, snapshot.total_instructions
        )
        .into(),
    );
    app.set_pass_number(i32::try_from(snapshot.pass).unwrap_or(i32::MAX));
    app.set_elapsed_text(format!("{:.1}S ACTIVE", snapshot.active_elapsed.as_secs_f64()).into());
    app.set_session_status(
        match snapshot.state {
            SessionState::Countdown => "COUNTDOWN",
            SessionState::Typing if app.get_real_typing_selected() => "TYPING",
            SessionState::Typing => "SIMULATING",
            SessionState::LoopWait => "LOOP WAIT",
            SessionState::Paused => "PAUSED",
            SessionState::Completed
                if snapshot.completion_reason == Some(CompletionReason::StopAfterReached) =>
            {
                "TIME LIMIT"
            }
            SessionState::Completed => "COMPLETE",
            SessionState::Failed => "ERROR",
            SessionState::Idle
                if snapshot.completion_reason == Some(CompletionReason::UserStopped) =>
            {
                "STOPPED"
            }
            _ => "READY",
        }
        .into(),
    );
}

fn apply_completion_notice(app: &AppWindow, reason: Option<CompletionReason>) {
    app.set_notice_is_error(false);
    let real_typing = app.get_real_typing_selected();
    app.set_notice_text(
        match reason {
            Some(CompletionReason::StopAfterReached) if real_typing => {
                "Active-time limit reached. Real Typing stopped safely."
            }
            Some(CompletionReason::StopAfterReached) => {
                "Active-time limit reached. The simulation stopped safely."
            }
            Some(CompletionReason::UserStopped) if real_typing => {
                "Real Typing stopped immediately. Refocus the destination to start again."
            }
            Some(CompletionReason::UserStopped) => {
                "Simulation stopped safely. You can edit and restart it."
            }
            _ if real_typing => "Real Typing complete. The captured target was released.",
            _ => "Simulation complete — no keystrokes were sent.",
        }
        .into(),
    );
}

fn show_error(app: &AppWindow, message: &str) {
    app.set_session_active(false);
    app.set_session_paused(false);
    app.set_can_start(!app.get_draft_text().trim().is_empty());
    app.set_session_status("NEEDS ATTENTION".into());
    app.set_notice_is_error(true);
    app.set_notice_text(message.into());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_unicode_characters_and_whitespace_words() {
        assert_eq!(document_counts("Hello 日本🙂"), (9, 2));
    }

    #[test]
    fn token_insertion_replaces_the_selected_utf8_range() {
        let text = "A🙂B";
        let (updated, caret) = insert_at_selection(text, "{DATE}", 1, 5);
        assert_eq!(updated, "A{DATE}B");
        assert_eq!(caret, 7);
    }

    #[test]
    fn missing_cursor_appends_a_token() {
        let (updated, caret) = insert_at_selection("Hello", "{TIME}", -1, -1);
        assert_eq!(updated, "Hello{TIME}");
        assert_eq!(caret, 11);
    }

    #[test]
    fn queued_hotkey_press_cannot_restart_a_just_stopped_session() {
        let now = Instant::now();
        let mut gate = ActivationGate::default();

        assert_eq!(gate.decide(now, true, true), ActivationAction::Stop);
        assert_eq!(
            gate.decide(now + Duration::from_millis(25), false, true),
            ActivationAction::Ignore
        );
        assert_eq!(
            gate.decide(now + HOTKEY_RESTART_SUPPRESSION, false, true),
            ActivationAction::Start
        );
    }

    #[test]
    fn active_session_can_always_be_stopped_by_retained_hotkey() {
        let mut gate = ActivationGate::default();
        assert_eq!(
            gate.decide(Instant::now(), true, false),
            ActivationAction::Stop
        );
    }

    #[test]
    fn unavailable_hotkey_cannot_start_an_idle_session() {
        let mut gate = ActivationGate::default();
        assert_eq!(
            gate.decide(Instant::now(), false, false),
            ActivationAction::Ignore
        );
    }

    #[test]
    fn shortcut_recorder_accepts_function_and_modified_character_keys() {
        let f12 = shortcut_from_key_event(
            &char::from(Key::F12).to_string(),
            false,
            false,
            false,
            false,
        )
        .unwrap()
        .unwrap();
        assert_eq!(f12.to_string(), "F12");

        let modified = shortcut_from_key_event("q", true, false, true, false)
            .unwrap()
            .unwrap();
        assert_eq!(modified.to_string(), "Ctrl+Shift+Q");
    }

    #[test]
    fn shortcut_recorder_waits_for_non_modifier_and_rejects_bare_characters() {
        assert_eq!(
            shortcut_from_key_event(
                &char::from(Key::Control).to_string(),
                true,
                false,
                false,
                false,
            )
            .unwrap(),
            None
        );
        assert!(shortcut_from_key_event("Q", false, false, false, false).is_err());
    }
}
