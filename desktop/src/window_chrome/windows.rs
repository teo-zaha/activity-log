use std::cell::Cell;

use ::windows::{
    Win32::{
        Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM},
        System::LibraryLoader::GetModuleHandleW,
        UI::{
            HiDpi::{GetDpiForWindow, GetSystemMetricsForDpi},
            Input::KeyboardAndMouse::{TME_LEAVE, TME_NONCLIENT, TRACKMOUSEEVENT, TrackMouseEvent},
            Shell::{DefSubclassProc, SetWindowSubclass},
            WindowsAndMessaging::{
                CREATESTRUCTW, CreateWindowExW, DefWindowProcW, FindWindowExW, GA_ROOT,
                GWLP_USERDATA, GetAncestor, GetClientRect, GetWindowLongPtrW, HTMAXBUTTON,
                HWND_TOP, IDC_ARROW, IsZoomed, LoadCursorW, RegisterClassExW, SM_CYFRAME,
                SW_MAXIMIZE, SW_RESTORE, SWP_NOACTIVATE, SWP_NOOWNERZORDER, SetWindowLongPtrW,
                SetWindowPos, ShowWindow, WINDOW_EX_STYLE, WM_NCCREATE, WM_NCDESTROY, WM_NCHITTEST,
                WM_NCLBUTTONDBLCLK, WM_NCLBUTTONDOWN, WM_NCLBUTTONUP, WM_NCMOUSELEAVE,
                WM_NCMOUSEMOVE, WM_NCRBUTTONDOWN, WM_NCRBUTTONUP, WM_SIZE, WNDCLASSEXW, WS_CHILD,
                WS_CLIPSIBLINGS, WS_VISIBLE,
            },
        },
    },
    core::{PCWSTR, w},
};
use serde_json::json;
use tauri::{Emitter, Runtime, WebviewWindow};

use super::{ButtonArea, placement};

const CLASS_NAME: PCWSTR = w!("ACTIVITY_LOG_MAXIMIZE_BUTTON");
const SUBCLASS_ID: usize = 0x6d61_7862;
/// The window over the button keeps the pointer from the page, so the page hears of it here.
const EVENT: &str = "maximize-button";

/// What the window over the button knows; it lives in the window's `GWLP_USERDATA`.
struct Overlay {
    area: Cell<ButtonArea>,
    hovered: Cell<bool>,
    pressed: Cell<bool>,
    notify: Box<dyn Fn(bool, bool)>,
}

impl Overlay {
    fn show(&self, hovered: bool, pressed: bool) {
        if (self.hovered.get(), self.pressed.get()) != (hovered, pressed) {
            self.hovered.set(hovered);
            self.pressed.set(pressed);
            (self.notify)(hovered, pressed);
        }
    }
}

/// Puts the window over the button, or moves it there when the page asks again (a reload).
pub fn attach<R: Runtime>(window: WebviewWindow<R>, area: ButtonArea) -> Result<(), String> {
    let target = window.clone();
    window
        .run_on_main_thread(move || {
            let Ok(parent) = target.hwnd() else {
                return;
            };
            let parent = HWND(parent.0);
            let notify = move |hovered: bool, pressed: bool| {
                let _ = target.emit(EVENT, json!({ "hovered": hovered, "pressed": pressed }));
            };
            unsafe { attach_to(parent, area, Box::new(notify)) };
        })
        .map_err(|err| err.to_string())
}

unsafe fn attach_to(parent: HWND, area: ButtonArea, notify: Box<dyn Fn(bool, bool)>) {
    unsafe {
        if let Ok(overlay) = FindWindowExW(Some(parent), None, CLASS_NAME, PCWSTR::null()) {
            if let Some(state) = overlay_state(overlay) {
                state.area.set(area);
            }
            place(overlay, parent);
            return;
        }

        let instance = GetModuleHandleW(PCWSTR::null())
            .ok()
            .map(|module| HINSTANCE(module.0));
        let class = WNDCLASSEXW {
            cbSize: size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(overlay_proc),
            hInstance: instance.unwrap_or_default(),
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            lpszClassName: CLASS_NAME,
            ..Default::default()
        };
        RegisterClassExW(&class);

        let state = Box::into_raw(Box::new(Overlay {
            area: Cell::new(area),
            hovered: Cell::new(false),
            pressed: Cell::new(false),
            notify,
        }));
        // Never painted, as Tauri's window over the resize border: the page shows through.
        let Ok(overlay) = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            CLASS_NAME,
            PCWSTR::null(),
            WS_CHILD | WS_VISIBLE | WS_CLIPSIBLINGS,
            0,
            0,
            0,
            0,
            Some(parent),
            None,
            instance,
            Some(state as _),
        ) else {
            return;
        };
        place(overlay, parent);
        let _ = SetWindowSubclass(parent, Some(follow_parent), SUBCLASS_ID, overlay.0 as usize);
    }
}

fn overlay_state<'a>(overlay: HWND) -> Option<&'a Overlay> {
    unsafe { (GetWindowLongPtrW(overlay, GWLP_USERDATA) as *const Overlay).as_ref() }
}

/// Over the button and above the page, below the border that resizes the window.
unsafe fn place(overlay: HWND, parent: HWND) {
    unsafe {
        let Some(state) = overlay_state(overlay) else {
            return;
        };
        let mut client = RECT::default();
        if GetClientRect(parent, &mut client).is_err() {
            return;
        }
        let dpi = GetDpiForWindow(parent);
        let (x, y, width, height) = placement(
            state.area.get(),
            client.right - client.left,
            f64::from(dpi) / 96.0,
            GetSystemMetricsForDpi(SM_CYFRAME, dpi),
        );
        let _ = SetWindowPos(
            overlay,
            Some(HWND_TOP),
            x,
            y,
            width,
            height,
            SWP_NOACTIVATE | SWP_NOOWNERZORDER,
        );
    }
}

/// Keeps the window over the button as the app's window resizes.
unsafe extern "system" fn follow_parent(
    parent: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _id: usize,
    overlay: usize,
) -> LRESULT {
    unsafe {
        let result = DefSubclassProc(parent, msg, wparam, lparam);
        if msg == WM_SIZE {
            place(HWND(overlay as _), parent);
        }
        result
    }
}

/// A caption button as Windows Terminal's drag bar handles one: the press and the release
/// happen over the button, and the release maximizes or restores the window.
unsafe extern "system" fn overlay_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe {
        match msg {
            WM_NCCREATE => {
                let create = &*(lparam.0 as *const CREATESTRUCTW);
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, create.lpCreateParams as isize);
            }
            WM_NCHITTEST => return LRESULT(HTMAXBUTTON as _),
            WM_NCMOUSEMOVE => {
                if let Some(state) = overlay_state(hwnd)
                    && !state.hovered.get()
                {
                    let mut track = TRACKMOUSEEVENT {
                        cbSize: size_of::<TRACKMOUSEEVENT>() as u32,
                        dwFlags: TME_LEAVE | TME_NONCLIENT,
                        hwndTrack: hwnd,
                        dwHoverTime: 0,
                    };
                    let _ = TrackMouseEvent(&mut track);
                    state.show(true, false);
                }
                return LRESULT(0);
            }
            WM_NCMOUSELEAVE => {
                if let Some(state) = overlay_state(hwnd) {
                    state.show(false, false);
                }
                return LRESULT(0);
            }
            WM_NCLBUTTONDOWN | WM_NCLBUTTONDBLCLK => {
                if let Some(state) = overlay_state(hwnd) {
                    state.show(true, true);
                }
                return LRESULT(0);
            }
            WM_NCLBUTTONUP => {
                if let Some(state) = overlay_state(hwnd)
                    && state.pressed.get()
                {
                    state.show(true, false);
                    let window = GetAncestor(hwnd, GA_ROOT);
                    let command = if IsZoomed(window).as_bool() {
                        SW_RESTORE
                    } else {
                        SW_MAXIMIZE
                    };
                    let _ = ShowWindow(window, command);
                }
                return LRESULT(0);
            }
            WM_NCRBUTTONDOWN | WM_NCRBUTTONUP => return LRESULT(0),
            WM_NCDESTROY => {
                let state = SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0) as *mut Overlay;
                if !state.is_null() {
                    drop(Box::from_raw(state));
                }
            }
            _ => {}
        }
        DefWindowProcW(hwnd, msg, wparam, lparam)
    }
}
