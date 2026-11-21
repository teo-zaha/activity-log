use std::sync::{Arc, Mutex};

use activity_log_core::{
    ipc::DESKTOP_COMMANDS,
    menu::{INSTALL_MENU_COMMAND, UPDATE_MENU_COMMAND},
};
use tauri::{
    ipc::{CallbackFn, InvokeBody},
    test::{INVOKE_KEY, MockRuntime, get_ipc_response, mock_builder, mock_context, noop_assets},
    webview::InvokeRequest,
};

use crate::{app_menu, commands, deep_link, window_chrome};

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
