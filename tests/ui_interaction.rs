use std::{cell::RefCell, rc::Rc, time::Duration};

use i_slint_backend_testing::{init_no_event_loop, mock_elapsed_time};
use slint::ComponentHandle;
use slint::platform::{PointerEventButton, WindowEvent};

slint::include_modules!();

#[test]
fn editor_and_token_chip_accept_real_pointer_and_keyboard_events() {
    init_no_event_loop();
    let app = AppWindow::new().expect("testing backend should create the window");
    app.window().set_size(slint::PhysicalSize::new(960, 680));
    app.show().expect("testing backend should show the window");
    mock_elapsed_time(Duration::ZERO);

    // The editor occupies the large left card below the token row.
    send_mouse_click(&app, 180.0, 300.0);
    send_keyboard_string_sequence(&app, "Hello");
    assert_eq!(app.get_draft_text().as_str(), "Hello");

    let inserted = Rc::new(RefCell::new(None));
    let inserted_from_callback = Rc::clone(&inserted);
    app.on_insert_token(move |token, cursor, anchor| {
        *inserted_from_callback.borrow_mut() = Some((token.to_string(), cursor, anchor));
    });

    // The first chip in the row is the Clipboard runtime token.
    send_mouse_click(&app, 150.0, 180.0);
    let inserted = inserted.borrow();
    let (token, cursor, anchor) = inserted.as_ref().expect("token chip should be clickable");
    assert_eq!(token, "{CLIPBOARD}");
    assert_eq!((*cursor, *anchor), (5, 5));

    let opened = Rc::new(RefCell::new(false));
    let opened_from_callback = Rc::clone(&opened);
    app.on_open_profile_manager(move || *opened_from_callback.borrow_mut() = true);
    scan_for_click(&app, &opened, 520..740, 34..72);
    assert!(*opened.borrow(), "profile button should be clickable");

    let new_profile = Rc::new(RefCell::new(false));
    let new_profile_from_callback = Rc::clone(&new_profile);
    app.on_profile_new(move || *new_profile_from_callback.borrow_mut() = true);
    app.set_profile_manager_open(true);
    mock_elapsed_time(Duration::from_millis(150));
    scan_for_click(&app, &new_profile, 500..690, 118..160);
    assert!(
        *new_profile.borrow(),
        "profile manager buttons should be clickable"
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
