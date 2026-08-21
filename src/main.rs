#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::{
    cell::RefCell,
    path::PathBuf,
    rc::Rc,
    time::{Duration, Instant},
};

use autoquill::{
    APP_VERSION,
    domain::{
        ModifierSet, Profile, SessionState, SettingsDraft, Shortcut, ShortcutKey, TargetIntent,
        TypingSettings,
    },
    initialize_diagnostics,
    persistence::{
        ImportCandidate, ImportConflictPolicy, ProfileStatus, ProfileStore, ProfileSummary,
    },
    platform::{
        ForegroundBackend, ForegroundTarget, HotkeyEvent, HotkeyService,
        document_uses_activation_key,
    },
    typing::{
        CompileWarning, CompletionReason, EngineUpdate, PreviewOperation, SeededRandom,
        SessionEngine, SessionPlan, SystemRuntimeValues, compile,
    },
    window_title,
};
use slint::{CloseRequestResponse, ComponentHandle, ModelRc, VecModel};

slint::include_modules!();

const UI_TICK: Duration = Duration::from_millis(16);
const HOTKEY_POLL: Duration = Duration::from_millis(25);

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
    connect_interactions(&app);
    connect_profiles(&app);

    if std::env::var_os("AUTOQUILL_SMOKE_TEST").is_some() {
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
        app.run()
    }
}

fn connect_interactions(app: &AppWindow) {
    let engine = Rc::new(RefCell::new(
        SessionEngine::<SeededRandom>::from_system_time(),
    ));
    let timer = Rc::new(slint::Timer::default());
    let last_tick = Rc::new(RefCell::new(Instant::now()));
    let native = Rc::new(RefCell::new(NativeRunState::default()));

    app.set_real_typing_available(ForegroundBackend::available());
    app.set_real_typing_selected(false);
    app.set_real_typing_confirmed(false);
    app.set_target_name("No target captured".into());

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
            if app.get_real_typing_available() {
                app.set_real_typing_confirm_open(true);
            } else {
                show_error(
                    &app,
                    "Real Typing is currently available on Windows only. Simulation remains available.",
                );
            }
        }
    });

    let weak_app = app.as_weak();
    app.on_confirm_real_typing(move || {
        if let Some(app) = weak_app.upgrade() {
            app.set_real_typing_confirm_open(false);
            app.set_real_typing_confirmed(true);
            app.set_real_typing_selected(true);
            app.set_notice_is_error(false);
            app.set_notice_text(
                format!(
                    "Real Typing is armed. Focus the destination app and press F{} to start; press it again to stop.",
                    app.get_shortcut_number()
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
                    "Focus the exact destination window, then press F{}. AutoQuill will capture it and begin after a 2-second countdown.",
                    app.get_shortcut_number()
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
                            "Draft updated. Focus the destination app and press F{} when ready.",
                            app.get_shortcut_number()
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
        let function_key = u8::try_from(app.get_shortcut_number()).unwrap_or(1);

        if real_typing {
            if !app.get_real_typing_confirmed() {
                show_error(&app, "Confirm Real Typing before arming native input.");
                return;
            }
            if !app.get_hotkey_ready() {
                show_error(
                    &app,
                    "The selected activation key is unavailable. Choose another F-key in Advanced Settings.",
                );
                return;
            }
            if document_uses_activation_key(&compilation.instructions, function_key) {
                show_error(
                    &app,
                    &format!(
                        "This document contains [F{function_key}], which is reserved as the Start/Stop key. Remove that token or choose another activation key."
                    ),
                );
                return;
            }

            let target = match start_native.borrow().backend.capture_foreground() {
                Ok(target) => target,
                Err(error) => {
                    show_error(&app, &error.to_string());
                    return;
                }
            };
            app.set_target_name(target.label().into());
            start_native.borrow_mut().target = Some(target);
        } else {
            start_native.borrow_mut().target = None;
            app.set_target_name("No target captured".into());
        }

        let mut settings = settings_from_ui(&app);
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
            warning_notice
                .unwrap_or_else(|| {
                    if real_typing {
                        format!(
                            "Target locked: {}. Keep it foreground; F{} stops immediately.",
                            app.get_target_name(),
                            app.get_shortcut_number()
                        )
                    } else {
                        "Safe simulation is running. No native input is being sent.".into()
                    }
                })
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
                    "Real Typing resumed. The captured target must remain foreground."
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
            app.set_notice_is_error(false);
            app.set_notice_text(
                "Preview reset. Your draft and advanced settings were kept.".into(),
            );
        }
    });

    connect_hotkey(app);
}

fn connect_hotkey(app: &AppWindow) {
    match HotkeyService::start(u8::try_from(app.get_shortcut_number()).unwrap_or(1)) {
        Ok((service, receiver)) => {
            let service = Rc::new(service);
            let receiver = Rc::new(RefCell::new(receiver));
            let hotkey_timer = Rc::new(slint::Timer::default());

            let weak_app = app.as_weak();
            let key_service = Rc::clone(&service);
            let key_timer = Rc::clone(&hotkey_timer);
            app.on_shortcut_changed(move |number| {
                let _timer = &key_timer;
                if let Some(app) = weak_app.upgrade() {
                    let key = u8::try_from(number).unwrap_or(1);
                    app.set_hotkey_ready(false);
                    app.set_hotkey_status(format!("REGISTERING F{key}").into());
                    if let Err(error) = key_service.set_key(key) {
                        app.set_hotkey_status("ACTIVATION KEY ERROR".into());
                        show_error(&app, &error.to_string());
                    }
                }
            });

            let weak_app = app.as_weak();
            let event_receiver = Rc::clone(&receiver);
            let keep_service_alive = Rc::clone(&service);
            let weak_timer = Rc::downgrade(&hotkey_timer);
            hotkey_timer.start(slint::TimerMode::Repeated, HOTKEY_POLL, move || {
                let _service = &keep_service_alive;
                let Some(app) = weak_app.upgrade() else {
                    if let Some(timer) = weak_timer.upgrade() {
                        timer.stop();
                    }
                    return;
                };
                while let Ok(event) = event_receiver.borrow().try_recv() {
                    match event {
                        HotkeyEvent::Pressed => {
                            if app.get_session_active() {
                                app.invoke_stop_simulation();
                            } else if app.get_can_start()
                                && (!app.get_real_typing_selected()
                                    || app.get_real_typing_confirmed())
                            {
                                app.invoke_start_simulation();
                            }
                        }
                        HotkeyEvent::Registered(key) => {
                            app.set_hotkey_ready(true);
                            app.set_hotkey_status(format!("F{key} READY • START / STOP").into());
                        }
                        HotkeyEvent::RegistrationFailed { key: _, message } => {
                            app.set_hotkey_ready(false);
                            app.set_hotkey_status("ACTIVATION KEY UNAVAILABLE".into());
                            show_error(&app, &message);
                        }
                    }
                }
            });
        }
        Err(error) => {
            app.set_hotkey_ready(false);
            app.set_hotkey_status("GLOBAL KEY UNAVAILABLE".into());
            if ForegroundBackend::available() {
                show_error(app, &error.to_string());
            }
            app.on_shortcut_changed(|_| {});
        }
    }
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
    let shortcut_number = match settings.shortcut.key {
        ShortcutKey::Function(number) => i32::from(number),
        _ => 1,
    };
    app.set_shortcut_number(shortcut_number);
    app.set_sticky_typing(settings.target == TargetIntent::StickyAuto);
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
        shortcut: Shortcut::new(
            ModifierSet::default(),
            ShortcutKey::Function(u8::try_from(app.get_shortcut_number()).unwrap_or(1)),
        )
        .unwrap_or_default(),
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
}
