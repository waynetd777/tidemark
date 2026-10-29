// Copyright (c) 2026 Wayne Davies
// SPDX-License-Identifier: GPL-3.0-or-later (see LICENSE at the repository root)

//! Windows 11's snap layouts for the page's own maximise button (src/WindowControls.tsx). The
//! flyout appears when the window under the pointer answers `WM_NCHITTEST` with `HTMAXBUTTON`,
//! and the page can't: the pointer over it is WebView2's, whose window belongs to another
//! process, so the main window is never asked. The way round is Tauri's own for resize borders
//! (tauri-runtime-wry's `undecorated_resizing`): a child window of this process over the button,
//! drawn by nothing (the page shows through), that answers the hit-test itself. Clicks on it are
//! then non-client clicks that never reach the page, so it maximises or restores the main window
//! on their behalf, and tells the page when the pointer is over it (`window-controls-hover`) so
//! the button can look hovered. The page places it with `window_max_button`, in physical pixels.

use tauri::Emitter;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Graphics::Gdi::{BeginPaint, EndPaint, PAINTSTRUCT};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    TrackMouseEvent, TME_LEAVE, TME_NONCLIENT, TRACKMOUSEEVENT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, FindWindowExW, GetParent, GetWindowLongPtrW, IsZoomed,
    PostMessageW, RegisterClassExW, SetWindowLongPtrW, SetWindowPos, CREATESTRUCTW, CW_USEDEFAULT,
    GWLP_USERDATA, HTMAXBUTTON, HWND_TOP, SC_MAXIMIZE, SC_RESTORE, SWP_HIDEWINDOW, SWP_NOACTIVATE,
    SWP_NOOWNERZORDER, SWP_SHOWWINDOW, WINDOW_EX_STYLE, WM_CREATE, WM_ERASEBKGND, WM_NCDESTROY,
    WM_NCHITTEST, WM_NCLBUTTONDOWN, WM_NCLBUTTONUP, WM_NCMOUSELEAVE, WM_NCMOUSEMOVE, WM_PAINT,
    WM_SYSCOMMAND, WNDCLASSEXW, WS_CHILD, WS_CLIPSIBLINGS,
};

const CLASS: PCWSTR = w!("TIDEMARK_MAXBUTTON");
const NAME: PCWSTR = w!("TIDEMARK_MAXBUTTON_WINDOW");

/// What the child window knows: the page to tell about hovers, and whether a leave is being
/// tracked (`TrackMouseEvent` is armed once per entry).
struct Data {
    page: tauri::WebviewWindow,
    tracking: bool,
}

/// Puts the hit-test window over the maximise button at `rect` (x, y, width, height in physical
/// pixels of the main window's client area), or hides it with `None`. On the main thread.
pub fn set_max_button(w: &tauri::WebviewWindow, rect: Option<(i32, i32, i32, i32)>) {
    // As a number: a window handle isn't Send, and the closure runs on the main thread.
    let Ok(parent) = w.hwnd().map(|h| h.0 as isize) else {
        return;
    };
    let page = w.clone();
    let _ = w.run_on_main_thread(move || unsafe {
        let parent = HWND(parent as _);
        let child = match FindWindowExW(Some(parent), None, CLASS, NAME) {
            Ok(h) if !h.is_invalid() => h,
            _ => match create(parent, page) {
                Some(h) => h,
                None => return,
            },
        };
        let flags = SWP_NOACTIVATE | SWP_NOOWNERZORDER;
        match rect {
            Some((x, y, cx, cy)) => {
                let _ = SetWindowPos(child, Some(HWND_TOP), x, y, cx, cy, flags | SWP_SHOWWINDOW);
            }
            None => {
                let _ = SetWindowPos(child, None, 0, 0, 0, 0, flags | SWP_HIDEWINDOW);
            }
        }
    });
}

unsafe fn create(parent: HWND, page: tauri::WebviewWindow) -> Option<HWND> {
    let instance = GetModuleHandleW(None).ok()?;
    let class = WNDCLASSEXW {
        cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
        lpfnWndProc: Some(wndproc),
        hInstance: instance.into(),
        lpszClassName: CLASS,
        ..Default::default()
    };
    // Already registered by an earlier window: the atom is the same class, so the error is fine.
    RegisterClassExW(&class);
    let data = Box::into_raw(Box::new(Data {
        page,
        tracking: false,
    }));
    let made = CreateWindowExW(
        WINDOW_EX_STYLE::default(),
        CLASS,
        NAME,
        WS_CHILD | WS_CLIPSIBLINGS,
        CW_USEDEFAULT,
        CW_USEDEFAULT,
        0,
        0,
        Some(parent),
        None,
        Some(HINSTANCE::from(instance)),
        Some(data as _),
    );
    if made.is_err() {
        // WM_NCDESTROY never ran to take it back.
        drop(Box::from_raw(data));
    }
    made.ok()
}

unsafe fn data<'a>(hwnd: HWND) -> Option<&'a mut Data> {
    let p = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut Data;
    (!p.is_null()).then(|| &mut *p)
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_CREATE => {
            let cs = lparam.0 as *const CREATESTRUCTW;
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, (*cs).lpCreateParams as isize);
            LRESULT(0)
        }
        // The window is the maximise button, wherever on it the pointer is.
        WM_NCHITTEST => LRESULT(HTMAXBUTTON as isize),
        // Nothing of its own to paint: the page shows through.
        WM_ERASEBKGND => LRESULT(1),
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let _ = BeginPaint(hwnd, &mut ps);
            let _ = EndPaint(hwnd, &ps);
            LRESULT(0)
        }
        WM_NCMOUSEMOVE => {
            if let Some(d) = data(hwnd) {
                if !d.tracking {
                    let mut t = TRACKMOUSEEVENT {
                        cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                        dwFlags: TME_LEAVE | TME_NONCLIENT,
                        hwndTrack: hwnd,
                        dwHoverTime: 0,
                    };
                    d.tracking = TrackMouseEvent(&mut t).is_ok();
                    let _ = d.page.emit("window-controls-hover", true);
                }
            }
            // The system's own handling of a hover on a maximise button (the flyout) runs there.
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        WM_NCMOUSELEAVE => {
            if let Some(d) = data(hwnd) {
                d.tracking = false;
                let _ = d.page.emit("window-controls-hover", false);
            }
            LRESULT(0)
        }
        // DefWindowProc would run the caption button's own press-and-release on this window and
        // maximise it, not the main window: the press is swallowed and the release does the work.
        WM_NCLBUTTONDOWN => LRESULT(0),
        WM_NCLBUTTONUP => {
            if let Ok(parent) = GetParent(hwnd) {
                let cmd = if IsZoomed(parent).as_bool() {
                    SC_RESTORE
                } else {
                    SC_MAXIMIZE
                };
                let _ = PostMessageW(Some(parent), WM_SYSCOMMAND, WPARAM(cmd as usize), LPARAM(0));
            }
            LRESULT(0)
        }
        WM_NCDESTROY => {
            let p = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut Data;
            if !p.is_null() {
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                drop(Box::from_raw(p));
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}
