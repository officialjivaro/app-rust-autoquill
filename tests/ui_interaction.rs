use std::{cell::RefCell, rc::Rc, time::Duration};

use i_slint_backend_testing::{ElementHandle, init_no_event_loop, mock_elapsed_time};
use slint::ComponentHandle;
use slint::platform::{PointerEventButton, WindowEvent};

slint::include_modules!();

#[test]
fn editor_pointer_keyboard_and_token_action_are_interactive() {
    init_no_event_loop();
    let app = AppWindow::new().expect("testing backend should create the window");
    app.window().set_size(slint::PhysicalSize::new(1280, 720));
    app.show().expect("testing backend should show the window");
    mock_elapsed_time(Duration::ZERO);

    // The editor occupies the large left card below the token row.
    send_mouse_click(&app, 220.0, 390.0);
    send_keyboard_string_sequence(&app, "Hello");
    assert_eq!(app.get_draft_text().as_str(), "Hello");

    let inserted = Rc::new(RefCell::new(None));
    let inserted_from_callback = Rc::clone(&inserted);
    app.on_insert_token(move |token, cursor, anchor| {
        *inserted_from_callback.borrow_mut() = Some((token.to_string(), cursor, anchor));
    });

    ElementHandle::find_by_accessible_label(&app, "{CLIPBOARD}")
        .next()
        .expect("Clipboard token chip should be exposed to accessibility")
        .invoke_accessible_default_action();
    {
        let inserted = inserted.borrow();
        let (token, cursor, anchor) = inserted.as_ref().expect("token chip should be clickable");
        assert_eq!(token, "{CLIPBOARD}");
        assert_eq!((*cursor, *anchor), (5, 5));
    }

    let opened = Rc::new(RefCell::new(false));
    let opened_from_callback = Rc::clone(&opened);
    app.on_open_profile_manager(move || *opened_from_callback.borrow_mut() = true);
    ElementHandle::find_by_accessible_label(&app, "PROFILES · Unsaved")
        .next()
        .expect("profile selector should be exposed to accessibility")
        .invoke_accessible_default_action();
    assert!(*opened.borrow(), "profile button should be clickable");

    let new_profile = Rc::new(RefCell::new(false));
    let new_profile_from_callback = Rc::clone(&new_profile);
    app.on_profile_new(move || *new_profile_from_callback.borrow_mut() = true);
    app.set_profile_manager_open(true);
    mock_elapsed_time(Duration::from_millis(150));
    ElementHandle::find_by_accessible_label(&app, "NEW")
        .next()
        .expect("New profile button should be exposed to accessibility")
        .invoke_accessible_default_action();
    assert!(
        *new_profile.borrow(),
        "profile manager buttons should be clickable"
    );
}

#[test]
fn real_typing_mode_and_confirmation_accept_pointer_input() {
    init_no_event_loop();
    let app = AppWindow::new().expect("testing backend should create the window");
    app.window().set_size(slint::PhysicalSize::new(1280, 720));
    app.set_real_typing_available(true);
    app.show().expect("testing backend should show the window");
    mock_elapsed_time(Duration::ZERO);

    let requested = Rc::new(RefCell::new(false));
    let requested_from_callback = Rc::clone(&requested);
    app.on_request_real_typing(move || *requested_from_callback.borrow_mut() = true);
    scan_for_click(&app, &requested, 820..1260, 80..180);
    assert!(*requested.borrow(), "Real Typing mode should be clickable");

    let confirmed = Rc::new(RefCell::new(false));
    let confirmed_from_callback = Rc::clone(&confirmed);
    app.on_confirm_real_typing(move || *confirmed_from_callback.borrow_mut() = true);
    app.set_real_typing_confirm_open(true);
    mock_elapsed_time(Duration::from_millis(150));
    ElementHandle::find_by_accessible_label(&app, "I UNDERSTAND • ENABLE")
        .next()
        .expect("Real Typing confirmation should be exposed to accessibility")
        .invoke_accessible_default_action();
    assert!(
        *confirmed.borrow(),
        "Real Typing confirmation should require an explicit click"
    );
}

#[test]
fn unavailable_native_backend_opens_guidance_and_rechecks_without_enabling() {
    init_no_event_loop();
    let app = AppWindow::new().expect("testing backend should create the window");
    app.window().set_size(slint::PhysicalSize::new(1280, 720));
    app.set_real_typing_available(false);
    app.set_platform_status("LINUX WAYLAND • SIMULATION ONLY".into());
    app.set_platform_summary("Permission-mediated input is unavailable.".into());
    app.set_platform_guidance("Continue with Simulation.".into());
    app.show().expect("testing backend should show the window");
    mock_elapsed_time(Duration::ZERO);

    let requested = Rc::new(RefCell::new(false));
    let requested_from_callback = Rc::clone(&requested);
    app.on_request_real_typing(move || *requested_from_callback.borrow_mut() = true);
    ElementHandle::find_by_accessible_label(&app, "REAL TYPING • INFO")
        .next()
        .expect("unavailable Real Typing guidance should remain clickable")
        .invoke_accessible_default_action();
    assert!(*requested.borrow());

    let rechecked = Rc::new(RefCell::new(false));
    let rechecked_from_callback = Rc::clone(&rechecked);
    app.on_refresh_platform_capabilities(move || *rechecked_from_callback.borrow_mut() = true);
    app.set_real_typing_confirm_open(true);
    mock_elapsed_time(Duration::from_millis(150));
    ElementHandle::find_by_accessible_label(&app, "RE-CHECK")
        .next()
        .expect("permission guidance should expose a Re-check action")
        .invoke_accessible_default_action();
    assert!(*rechecked.borrow());
    assert!(!app.get_real_typing_selected());
}

#[test]
fn minimum_size_keeps_the_stacked_workspace_and_drawer_accessible() {
    init_no_event_loop();
    let app = AppWindow::new().expect("testing backend should create the window");
    app.window().set_size(slint::PhysicalSize::new(960, 600));
    app.show().expect("testing backend should show the window");
    mock_elapsed_time(Duration::ZERO);

    for label in ["Text to preview", "SETTINGS"] {
        assert!(
            ElementHandle::find_by_accessible_label(&app, label)
                .next()
                .is_some(),
            "{label} should remain accessible in the minimum-size layout"
        );
    }

    let reset_requested = Rc::new(RefCell::new(false));
    let callback_flag = Rc::clone(&reset_requested);
    app.on_reset_window(move || *callback_flag.borrow_mut() = true);
    app.set_advanced_open(true);
    mock_elapsed_time(Duration::from_millis(150));
    ElementHandle::find_by_accessible_label(&app, "RESET WINDOW")
        .next()
        .expect("window reset should be exposed in the settings drawer")
        .invoke_accessible_default_action();
    assert!(*reset_requested.borrow());
}

#[test]
fn shortcut_recorder_accepts_pointer_and_modifier_keyboard_input() {
    init_no_event_loop();
    let app = AppWindow::new().expect("testing backend should create the window");
    app.window().set_size(slint::PhysicalSize::new(1280, 720));
    app.set_advanced_open(true);
    app.show().expect("testing backend should show the window");
    mock_elapsed_time(Duration::from_millis(150));

    let recorded = Rc::new(RefCell::new(None));
    let callback_result = Rc::clone(&recorded);
    app.on_shortcut_recorded(move |text, control, alt, shift, meta| {
        *callback_result.borrow_mut() = Some((text.to_string(), control, alt, shift, meta));
    });

    ElementHandle::find_by_accessible_label(&app, "F1")
        .next()
        .expect("shortcut recorder should be exposed to accessibility")
        .invoke_accessible_default_action();
    assert!(app.get_shortcut_recording());

    let control: slint::SharedString = slint::platform::Key::Control.into();
    app.window().dispatch_event(WindowEvent::KeyPressed {
        text: control.clone(),
    });
    app.window()
        .dispatch_event(WindowEvent::KeyPressed { text: "q".into() });
    app.window()
        .dispatch_event(WindowEvent::KeyReleased { text: "q".into() });
    app.window()
        .dispatch_event(WindowEvent::KeyReleased { text: control });

    assert_eq!(
        recorded.borrow().as_ref(),
        Some(&("q".to_owned(), true, false, false, false))
    );
}

#[test]
fn appearance_and_privacy_diagnostic_controls_are_keyboard_accessible() {
    init_no_event_loop();
    let app = AppWindow::new().expect("testing backend should create the window");
    app.window().set_size(slint::PhysicalSize::new(960, 600));
    app.set_advanced_open(true);
    app.show().expect("testing backend should show the window");
    mock_elapsed_time(Duration::from_millis(150));

    ElementHandle::find_by_accessible_label(&app, "LIGHT")
        .next()
        .expect("Light theme should be exposed to accessibility")
        .invoke_accessible_default_action();
    assert_eq!(app.get_theme_mode().as_str(), "light");

    ElementHandle::find_by_accessible_label(&app, "Reduce interface motion")
        .next()
        .expect("Reduced motion should be exposed to accessibility")
        .invoke_accessible_default_action();
    assert!(app.get_reduce_motion());

    let copied = Rc::new(RefCell::new(false));
    let copied_from_callback = Rc::clone(&copied);
    app.on_copy_diagnostics(move || *copied_from_callback.borrow_mut() = true);
    ElementHandle::find_by_accessible_label(&app, "COPY DIAGNOSTICS")
        .next()
        .expect("Copy Diagnostics should be exposed to accessibility")
        .invoke_accessible_default_action();
    assert!(*copied.borrow());

    ElementHandle::find_by_accessible_label(&app, "Appearance and support")
        .next()
        .expect("Settings section headers should be exposed as buttons")
        .invoke_accessible_default_action();
    assert!(!app.get_appearance_settings_expanded());
}

#[test]
fn editable_text_and_numeric_controls_expose_complete_accessibility_contracts() {
    init_no_event_loop();
    let app = AppWindow::new().expect("testing backend should create the window");
    app.window().set_size(slint::PhysicalSize::new(1280, 720));
    app.show().expect("testing backend should show the window");
    mock_elapsed_time(Duration::ZERO);

    let editor = ElementHandle::find_by_accessible_label(&app, "Text to preview")
        .next()
        .expect("draft editor should be exposed as a named text input");
    editor.set_accessible_value("Set through accessibility");
    assert_eq!(app.get_draft_text().as_str(), "Set through accessibility");

    app.set_preview_text("Read only".into());
    let preview = ElementHandle::find_by_accessible_label(&app, "Simulation preview")
        .next()
        .expect("preview should be exposed as a named read-only text input");
    preview.set_accessible_value("Must not replace preview");
    assert_eq!(app.get_preview_text().as_str(), "Read only");

    assert_named_control(&app, "Words per minute");
    app.set_advanced_open(true);
    mock_elapsed_time(Duration::from_millis(150));
    assert_named_control(&app, "Active seconds");
    ElementHandle::find_by_accessible_label(&app, "Appearance and support")
        .next()
        .expect("Appearance section header should be visible")
        .invoke_accessible_default_action();
    mock_elapsed_time(Duration::from_millis(150));
    for label in ["Wait seconds minimum", "Wait seconds maximum"] {
        assert_named_control(&app, label);
    }
    ElementHandle::find_by_accessible_label(&app, "Timing and session settings")
        .next()
        .expect("Session section header should be visible")
        .invoke_accessible_default_action();
    mock_elapsed_time(Duration::from_millis(150));
    for label in [
        "Action interval minimum",
        "Action interval maximum",
        "Mistakes / event minimum",
        "Mistakes / event maximum",
        "Word interval minimum",
        "Word interval maximum",
        "Break ms minimum",
        "Break ms maximum",
        "Character interval minimum",
        "Character interval maximum",
        "Pause ms minimum",
        "Pause ms maximum",
    ] {
        assert_named_control(&app, label);
    }
}

#[test]
fn modal_surfaces_focus_the_first_action_and_profiles_explain_empty_results() {
    init_no_event_loop();
    let app = AppWindow::new().expect("testing backend should create the window");
    app.window().set_size(slint::PhysicalSize::new(1280, 720));
    app.show().expect("testing backend should show the window");
    mock_elapsed_time(Duration::ZERO);

    app.set_advanced_open(true);
    mock_elapsed_time(Duration::from_millis(150));
    send_keyboard_string_sequence(&app, " ");
    assert!(
        !app.get_advanced_open(),
        "Settings should focus its Done action when opened"
    );

    app.set_profile_manager_open(true);
    mock_elapsed_time(Duration::from_millis(150));
    assert_named_control(&app, "Search profiles");
    assert_named_control(&app, "NO PROFILES YET");
    send_keyboard_string_sequence(&app, "draft");
    assert_eq!(app.get_profile_search().as_str(), "draft");
    assert_named_control(&app, "NO MATCHING PROFILES");

    app.set_dialog_input_visible(true);
    let dialog_primary = Rc::new(RefCell::new(false));
    let dialog_primary_from_callback = Rc::clone(&dialog_primary);
    app.on_dialog_primary(move || *dialog_primary_from_callback.borrow_mut() = true);
    app.set_dialog_open(true);
    mock_elapsed_time(Duration::from_millis(150));
    assert_named_control(&app, "Profile name");
    send_keyboard_string_sequence(&app, "New name");
    assert_eq!(app.get_dialog_input().as_str(), "New name");
    assert!(
        !*dialog_primary.borrow(),
        "Naming a profile must not submit it"
    );
    assert_eq!(
        app.get_profile_search().as_str(),
        "draft",
        "Nested dialog owns typing"
    );
    app.set_dialog_error("That name is already in use.".into());
    mock_elapsed_time(Duration::ZERO);
    assert_named_control(&app, "That name is already in use.");
    app.set_dialog_open(false);
    mock_elapsed_time(Duration::ZERO);
    send_keyboard_string_sequence(&app, "s");
    assert_eq!(
        app.get_profile_search().as_str(),
        "drafts",
        "Cancel returns to Profiles"
    );
}

fn assert_named_control(app: &AppWindow, label: &str) {
    assert!(
        ElementHandle::find_by_accessible_label(app, label)
            .next()
            .is_some(),
        "{label} should be exposed with an accessibility label"
    );
}

fn scan_for_click(
    app: &AppWindow,
    result: &Rc<RefCell<bool>>,
    x: std::ops::Range<i32>,
    y: std::ops::Range<i32>,
) {
    for y in y.step_by(8) {
        for x in x.clone().step_by(8) {
            send_mouse_click(app, x as f32, y as f32);
            if *result.borrow() {
                return;
            }
        }
    }
}

fn send_mouse_click(app: &AppWindow, x: f32, y: f32) {
    let position = slint::LogicalPosition::new(x, y);
    let window = app.window();
    window.dispatch_event(WindowEvent::PointerMoved { position });
    window.dispatch_event(WindowEvent::PointerPressed {
        position,
        button: PointerEventButton::Left,
    });
    mock_elapsed_time(Duration::from_millis(50));
    window.dispatch_event(WindowEvent::PointerReleased {
        position,
        button: PointerEventButton::Left,
    });
}

fn send_keyboard_string_sequence(app: &AppWindow, sequence: &str) {
    for character in sequence.chars() {
        let text: slint::SharedString = character.into();
        app.window()
            .dispatch_event(WindowEvent::KeyPressed { text: text.clone() });
        app.window()
            .dispatch_event(WindowEvent::KeyReleased { text });
    }
}
