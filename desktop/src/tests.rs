use std::sync::{Arc, Mutex};

use activity_log_core::{
    ipc::DESKTOP_COMMANDS,
    menu::{INSTALL_MENU_COMMAND, UPDATE_MENU_COMMAND},
};
use tauri::{
    Manager, WindowEvent,
    ipc::{CallbackFn, InvokeBody},
    test::{INVOKE_KEY, MockRuntime, get_ipc_response, mock_builder, mock_context, noop_assets},
    webview::InvokeRequest,
};

use crate::{
    app_menu,
    collection_notice::WindowFocus,
    commands, deep_link,
    deep_link::{DeepLinkInbox, Delivery},
    page_started, window_chrome, window_event,
};

/// Held by every test that logs, reads or clears the process-wide diagnostics log, which tests
/// on other threads share: the registration test below runs `clear_logs` for real.
pub(crate) static DIAGNOSTICS: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// The app's own handler, on the mock runtime.
fn handler() -> impl Fn(tauri::ipc::Invoke<MockRuntime>) -> bool + Send + Sync + 'static {
    app_commands!()
}

/// Asks `webview` to run `name` without arguments; the answer does not matter here.
fn call(webview: &tauri::WebviewWindow<MockRuntime>, name: &str) {
    let _ = get_ipc_response(
        webview,
        InvokeRequest {
            cmd: name.into(),
            callback: CallbackFn(0),
            error: CallbackFn(1),
            url: "tauri://localhost".parse().unwrap(),
            body: InvokeBody::Json(serde_json::json!({})),
            headers: Default::default(),
            invoke_key: INVOKE_KEY.to_string(),
        },
    );
}

/// The names the dashboard calls commands by (`DESKTOP_COMMANDS`, `MENU_COMMANDS`) are the
/// names Tauri registered them under: the app's own handler takes each one.
#[test]
fn every_command_the_dashboard_names_is_registered() {
    let _log = DIAGNOSTICS.blocking_lock();
    let taken = Arc::new(Mutex::new(Vec::new()));
    let handler = handler();
    let record = taken.clone();
    let app = mock_builder()
        .invoke_handler(move |invoke| {
            let name = invoke.message.command().to_string();
            let handled = handler(invoke);
            record.lock().unwrap().push((name, handled));
            handled
        })
        .build(mock_context(noop_assets()))
        .unwrap();
    let webview =
        tauri::WebviewWindowBuilder::<MockRuntime, _>::new(&app, "main", Default::default())
            .build()
            .unwrap();
    let names = DESKTOP_COMMANDS
        .iter()
        .map(|(_, name)| *name)
        .chain([INSTALL_MENU_COMMAND, UPDATE_MENU_COMMAND])
        .collect::<Vec<_>>();

    for name in &names {
        // Without its arguments or state each command refuses; only whether it was taken counts.
        call(&webview, name);
    }

    let taken = taken.lock().unwrap().clone();
    let expected = names
        .iter()
        .map(|name| (name.to_string(), true))
        .collect::<Vec<_>>();
    assert_eq!(taken, expected);
}

/// A name the handler does not have is not taken, so the test above can fail.
#[test]
fn a_name_the_app_does_not_register_is_not_taken() {
    let handler = handler();
    let taken = Arc::new(Mutex::new(None));
    let record = taken.clone();
    let app = mock_builder()
        .invoke_handler(move |invoke| {
            let handled = handler(invoke);
            *record.lock().unwrap() = Some(handled);
            handled
        })
        .build(mock_context(noop_assets()))
        .unwrap();
    let webview =
        tauri::WebviewWindowBuilder::<MockRuntime, _>::new(&app, "main", Default::default())
            .build()
            .unwrap();
    call(&webview, "get_runtime_stat");
    assert_eq!(*taken.lock().unwrap(), Some(false));
}

/// B58: a page that starts loading again in the main window holds the links that come until
/// it asks; another window's page does not.
#[test]
fn a_reloading_main_page_gets_the_links_that_come_meanwhile() {
    let app = mock_builder()
        .manage(DeepLinkInbox::default())
        .build(mock_context(noop_assets()))
        .unwrap();
    let inbox = app.state::<DeepLinkInbox>();
    let link = || "activity-log://timeline".to_string();
    inbox.take();

    page_started("other", app.handle());
    assert_eq!(inbox.receive(link()), Delivery::Forward(link()));

    page_started(deep_link::MAIN_WINDOW, app.handle());
    assert_eq!(inbox.receive(link()), Delivery::Held);
    assert_eq!(inbox.take(), Some(link()));
}

/// The main window's focus events reach `WindowFocus`; another window's do not.
#[test]
fn the_main_window_s_focus_is_followed() {
    let app = mock_builder()
        .manage(WindowFocus::default())
        .build(mock_context(noop_assets()))
        .unwrap();
    let window = |label: &str| {
        tauri::WebviewWindowBuilder::<MockRuntime, _>::new(&app, label, Default::default())
            .build()
            .unwrap()
    };
    let (main, other) = (window(deep_link::MAIN_WINDOW), window("other"));
    let focus = app.state::<WindowFocus>();

    window_event(&main.as_ref().window(), &WindowEvent::Focused(true));
    assert!(focus.get());
    window_event(&other.as_ref().window(), &WindowEvent::Focused(false));
    assert!(focus.get(), "another window's focus says nothing");
    window_event(&main.as_ref().window(), &WindowEvent::Focused(false));
    assert!(!focus.get());
}
