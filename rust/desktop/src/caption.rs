//! Extend only the root window's client area into the native caption.
//! Windows still owns the resize frame, drag, system menu and maximize behavior.
use eframe::egui;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::cell::Cell;
use windows_sys::Win32::{
    Foundation::*,
    Graphics::Gdi::ScreenToClient,
    UI::{
        Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass},
        WindowsAndMessaging::*,
    },
};
thread_local! {static REGION:Cell<Option<[i32;4]>>=const{Cell::new(None)};static ACTIVE:Cell<bool>=const{Cell::new(false)};}
pub fn install(cc: &eframe::CreationContext<'_>) {
    let Ok(handle) = cc.window_handle() else {
        return;
    };
    let RawWindowHandle::Win32(raw) = handle.as_raw() else {
        return;
    };
    attach(raw.hwnd.get() as HWND);
}
fn attach(hwnd: HWND) {
    // No Rust pointers are retained by Windows. The subclass lives on the window thread.
    unsafe {
        if SetWindowSubclass(hwnd, Some(proc), 1, 0) != 0 {
            ACTIVE.set(true);
            SetWindowPos(
                hwnd,
                std::ptr::null_mut(),
                0,
                0,
                0,
                0,
                SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_NOZORDER,
            );
        }
    }
}
unsafe extern "system" fn proc(
    hwnd: HWND,
    msg: u32,
    w: WPARAM,
    l: LPARAM,
    _id: usize,
    _data: usize,
) -> LRESULT {
    // WM_NCCALCSIZE supplies either a RECT or NCCALCSIZE_PARAMS whose first member is a RECT.
    unsafe {
        if msg == WM_NCCALCSIZE && l != 0 {
            let r = l as *mut RECT;
            let old = *r;
            DefSubclassProc(hwnd, msg, w, l);
            (*r).top = old.top + ((*r).left - old.left).max(0);
            return 0;
        }
        if msg == WM_NCDESTROY {
            ACTIVE.set(false);
            REGION.set(None);
            RemoveWindowSubclass(hwnd, Some(proc), 1);
        }
        let hit = DefSubclassProc(hwnd, msg, w, l);
        if msg == WM_NCHITTEST
            && matches!(
                hit as u32,
                HTCLIENT | HTCAPTION | HTMAXBUTTON | HTMINBUTTON | HTCLOSE | HTSYSMENU
            )
        {
            let mut point = POINT {
                x: l as i16 as i32,
                y: (l >> 16) as i16 as i32,
            };
            if ScreenToClient(hwnd, &mut point) != 0
                && REGION.get().is_some_and(|[x, y, r, b]| {
                    point.x >= x && point.x < r && point.y >= y && point.y < b
                })
            {
                return HTCAPTION as isize;
            }
            return HTCLIENT as isize;
        }
        hit
    }
}
pub fn controls(ui: &mut egui::Ui) {
    if !ACTIVE.get() && !cfg!(test) {
        return;
    }
    let ctx = ui.ctx().clone();
    let w = (ui.available_width() - 3. * 44. - 3. * ui.spacing().item_spacing.x).max(0.);
    let (drag, _) = ui.allocate_exact_size(egui::vec2(w, 32.), egui::Sense::hover());
    let scale = ctx.pixels_per_point();
    REGION.set(ui.is_enabled().then(|| {
        [drag.left(), drag.top(), drag.right(), drag.bottom()].map(|x| (x * scale).round() as i32)
    }));
    let max = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
    for (i, key) in ["WindowMinimize", "WindowMaximizeRestore", "WindowClose"]
        .iter()
        .enumerate()
    {
        let label = crate::strings::text(key);
        let r = ui
            .add_sized([44., 32.], egui::Button::new("").frame_when_inactive(false))
            .on_hover_text(&label);
        r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), &label));
        let c = r.rect.center();
        let stroke = ui.style().interact(&r).fg_stroke;
        let line = |a, b| {
            ui.painter().line_segment([c + a, c + b], stroke);
        };
        let v = egui::vec2;
        match i {
            0 => line(v(-5., 3.), v(5., 3.)),
            1 => {
                if max {
                    ui.painter().rect_stroke(
                        egui::Rect::from_center_size(c + v(1., -1.), v(9., 9.)),
                        0.,
                        stroke,
                        egui::StrokeKind::Inside,
                    );
                }
                ui.painter().rect_stroke(
                    egui::Rect::from_center_size(c, v(10., 10.)),
                    0.,
                    stroke,
                    egui::StrokeKind::Inside,
                );
            }
            _ => {
                line(v(-5., -5.), v(5., 5.));
                line(v(-5., 5.), v(5., -5.));
            }
        }
        if r.clicked() {
            ctx.send_viewport_cmd(match i {
                0 => egui::ViewportCommand::Minimized(true),
                1 => egui::ViewportCommand::Maximized(!max),
                _ => egui::ViewportCommand::Close,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows_sys::Win32::Graphics::Gdi::ClientToScreen;
    #[test]
    fn hidden_window_extends_client_area_and_separates_drag_from_buttons() {
        // An invisible test window on this thread. No app launch, focus, pointer or desktop input.
        unsafe {
            let class: Vec<_> = "ShovelRustCaptionTest".encode_utf16().chain([0]).collect();
            let wc = WNDCLASSW {
                lpszClassName: class.as_ptr(),
                lpfnWndProc: Some(DefWindowProcW),
                ..Default::default()
            };
            assert_ne!(RegisterClassW(&wc), 0);
            struct Registered(Vec<u16>);
            impl Drop for Registered {
                fn drop(&mut self) {
                    unsafe {
                        UnregisterClassW(self.0.as_ptr(), std::ptr::null_mut());
                    }
                }
            }
            let _registered = Registered(class.clone());
            let hwnd = CreateWindowExW(
                0,
                class.as_ptr(),
                std::ptr::null(),
                WS_OVERLAPPEDWINDOW,
                100,
                100,
                900,
                620,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null(),
            );
            assert!(!hwnd.is_null());
            struct Owner(HWND);
            impl Drop for Owner {
                fn drop(&mut self) {
                    unsafe {
                        DestroyWindow(self.0);
                    }
                }
            }
            let _owner = Owner(hwnd);
            let mut before = RECT::default();
            GetClientRect(hwnd, &mut before);
            attach(hwnd);
            let mut after = RECT::default();
            GetClientRect(hwnd, &mut after);
            assert!(after.bottom > before.bottom && after.right == before.right);
            REGION.set(Some([240, 0, 700, 32]));
            for (x, expected) in [(350, HTCAPTION), (40, HTCLIENT), (800, HTCLIENT)] {
                let mut pos = POINT { x, y: 16 };
                assert_ne!(ClientToScreen(hwnd, &mut pos), 0);
                let packed = ((pos.y as u16 as usize) << 16) | (pos.x as u16 as usize);
                assert_eq!(
                    SendMessageW(hwnd, WM_NCHITTEST, 0, packed as isize),
                    expected as isize
                );
            }
            assert_eq!(IsWindowVisible(hwnd), 0);
        }
    }
}
