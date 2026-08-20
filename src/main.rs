#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::{cell::RefCell, rc::Rc, time::Duration};

use autoquill::{
    APP_VERSION,
    domain::{SessionState, Wpm},
    initialize_diagnostics,
    simulation::{SimulationController, SimulationUpdate},
    typing::{CompileWarning, SystemRuntimeValues, compile},
    window_title,
};

slint::include_modules!();

fn main() -> Result<(), slint::PlatformError> {
    initialize_diagnostics();

    let app = AppWindow::new()?;
    app.set_app_version(APP_VERSION.into());
    app.set_window_title(window_title().into());
    connect_interactions(&app);

    if std::env::var_os("AUTOQUILL_SMOKE_TEST").is_some() {
        let smoke_text = "Hi[ENTER]";
        app.set_draft_text(smoke_text.into());
        app.invoke_draft_edited(smoke_text.into());
        app.invoke_start_simulation(smoke_text.into(), 200);
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
    let controller = Rc::new(RefCell::new(SimulationController::default()));
    let timer = Rc::new(slint::Timer::default());

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
    let start_controller = Rc::clone(&controller);
    let start_timer = Rc::clone(&timer);
    app.on_start_simulation(move |text, raw_wpm| {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        let source = text.to_string();
        if source.trim().is_empty() {
            show_error(
                &app,
                "Enter or paste some text before starting the simulation.",
            );
            return;
        }

        start_timer.stop();
        let compilation = match compile(&source, &SystemRuntimeValues) {
            Ok(compilation) => compilation,
            Err(error) => {
                show_error(&app, &error.to_string());
                return;
            }
        };
        let warning_notice = summarize_warnings(&compilation.warnings);
        let update = start_controller.borrow_mut().start(compilation);
        apply_simulation_update(&app, &update);
        app.set_notice_is_error(false);
        app.set_notice_text(
            warning_notice
                .unwrap_or_else(|| {
                    "Safe simulation is running. No native input is being sent.".into()
                })
                .into(),
        );

        if !start_controller.borrow().is_running() {
            app.set_notice_text("Simulation complete — no keystrokes were sent.".into());
            return;
        }

        let interval =
            Duration::from_secs_f64(Wpm::new(i64::from(raw_wpm)).character_delay_seconds());
        let tick_app = app.as_weak();
        let tick_controller = Rc::clone(&start_controller);
        let weak_timer = Rc::downgrade(&start_timer);
        start_timer.start(slint::TimerMode::Repeated, interval, move || {
            let Some(app) = tick_app.upgrade() else {
                if let Some(timer) = weak_timer.upgrade() {
                    timer.stop();
                }
                return;
            };

            let update = tick_controller.borrow_mut().tick();
            let is_complete = update.state == SessionState::Completed;
            apply_simulation_update(&app, &update);
            if is_complete {
                app.set_notice_is_error(false);
                app.set_notice_text("Simulation complete — no keystrokes were sent.".into());
                if let Some(timer) = weak_timer.upgrade() {
                    timer.stop();
                }
            }
        });
    });

    let weak_app = app.as_weak();
    let stop_controller = Rc::clone(&controller);
    let stop_timer = Rc::clone(&timer);
    app.on_stop_simulation(move || {
        stop_timer.stop();
        if let Some(app) = weak_app.upgrade() {
            let update = stop_controller.borrow_mut().stop();
            apply_simulation_update(&app, &update);
            app.set_session_status("STOPPED".into());
            app.set_notice_is_error(false);
            app.set_notice_text("Simulation stopped safely. You can edit and restart it.".into());
        }
    });
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

fn apply_simulation_update(app: &AppWindow, update: &SimulationUpdate) {
    let is_active = update.state == SessionState::Typing;
    app.set_session_active(is_active);
    app.set_can_start(!is_active && !app.get_draft_text().trim().is_empty());
    app.set_simulation_progress(update.progress);
    app.set_preview_text(update.preview_text.clone().into());
    app.set_current_action(update.current_action.clone().into());
    app.set_instruction_progress(
        format!(
            "{} / {} INSTRUCTIONS",
            update.completed_instructions, update.total_instructions
        )
        .into(),
    );
    app.set_session_status(
        match update.state {
            SessionState::Typing => "SIMULATING",
            SessionState::Completed => "COMPLETE",
            SessionState::Failed => "ERROR",
            _ => "READY",
        }
        .into(),
    );
}

fn show_error(app: &AppWindow, message: &str) {
    app.set_session_active(false);
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
