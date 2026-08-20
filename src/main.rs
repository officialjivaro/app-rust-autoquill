#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::{
    cell::RefCell,
    rc::Rc,
    time::{Duration, Instant},
};

use autoquill::{
    APP_VERSION,
    domain::{SessionState, SettingsDraft},
    initialize_diagnostics,
    typing::{
        CompileWarning, CompletionReason, EngineUpdate, PreviewOperation, SeededRandom,
        SessionEngine, SessionPlan, SystemRuntimeValues, compile,
    },
    window_title,
};

slint::include_modules!();

const UI_TICK: Duration = Duration::from_millis(16);

fn main() -> Result<(), slint::PlatformError> {
    initialize_diagnostics();

    let app = AppWindow::new()?;
    app.set_app_version(APP_VERSION.into());
    app.set_window_title(window_title().into());
    connect_interactions(&app);

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

    let weak_app = app.as_weak();
    app.on_draft_edited(move |text| {
        if let Some(app) = weak_app.upgrade() {
            update_document_summary(&app, text.as_str());
            if !app.get_session_active() {
                app.set_notice_is_error(false);
                app.set_notice_text(
                    "Simulation mode is safe: no keystrokes leave this window.".into(),
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
            app.invoke_place_editor_caret(caret);
        }
    });

    let weak_app = app.as_weak();
    let start_engine = Rc::clone(&engine);
    let start_timer = Rc::clone(&timer);
    let start_last_tick = Rc::clone(&last_tick);
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
        let plan = SessionPlan {
            instructions: compilation.instructions,
            settings: settings_from_ui(&app),
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
                    "Safe simulation is running. No native input is being sent.".into()
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
        let weak_timer = Rc::downgrade(&start_timer);
        start_timer.start(slint::TimerMode::Repeated, UI_TICK, move || {
            let Some(app) = tick_app.upgrade() else {
                if let Some(timer) = weak_timer.upgrade() {
                    timer.stop();
                }
                return;
            };

            let now = Instant::now();
            let elapsed = now.saturating_duration_since(*tick_last_tick.borrow());
            *tick_last_tick.borrow_mut() = now;
            let update = tick_engine.borrow_mut().advance(elapsed);
            apply_engine_update(&app, &update);
            if !update.snapshot.state.is_active() {
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
            app.set_notice_text("Safe simulation resumed.".into());
        }
    });

    let weak_app = app.as_weak();
    let stop_engine = Rc::clone(&engine);
    let stop_timer = Rc::clone(&timer);
    app.on_stop_simulation(move || {
        stop_timer.stop();
        if let Some(app) = weak_app.upgrade() {
            let update = stop_engine.borrow_mut().stop();
            apply_engine_update(&app, &update);
            app.set_notice_is_error(false);
            app.set_notice_text("Simulation stopped safely. You can edit and restart it.".into());
        }
    });

    let weak_app = app.as_weak();
    let reset_engine = Rc::clone(&engine);
    let reset_timer = Rc::clone(&timer);
    app.on_reset_simulation(move || {
        reset_timer.stop();
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
}

fn settings_from_ui(app: &AppWindow) -> autoquill::domain::TypingSettings {
    SettingsDraft {
        wpm: Some(i64::from(app.get_wpm())),
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
        break_min_seconds: Some(f64::from(app.get_break_min_seconds())),
        break_max_seconds: Some(f64::from(app.get_break_max_seconds())),
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
    app.set_notice_text(
        match reason {
            Some(CompletionReason::StopAfterReached) => {
                "Active-time limit reached. The simulation stopped safely."
            }
            Some(CompletionReason::UserStopped) => {
                "Simulation stopped safely. You can edit and restart it."
            }
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
