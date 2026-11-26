//! Give secondary eframe viewports native ownership and owner-monitor placement.
use eframe::egui;
use std::{cell::Cell, ptr::null_mut};
use windows_sys::Win32::{
    Foundation::{HWND, LPARAM, RECT},
    Graphics::Gdi::{GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow},
    System::Threading::GetCurrentThreadId,
    UI::{
        HiDpi::{AdjustWindowRectExForDpi, GetDpiForWindow},
        Input::KeyboardAndMouse::{
            EnableWindow, GetActiveWindow, IsWindowEnabled, SetActiveWindow,
        },
        WindowsAndMessaging::{
            EnumThreadWindows, GWL_EXSTYLE, GWL_STYLE, GWLP_HWNDPARENT, GetWindowLongW,
            GetWindowRect, GetWindowTextLengthW, GetWindowTextW, IsWindow, SWP_NOACTIVATE,
            SWP_NOZORDER, SetWindowLongPtrW, SetWindowPos,
        },
    },
};

thread_local! { static ROOT_DISABLED: Cell<bool> = const { Cell::new(false) }; }

pub fn block_root(block: bool) {
    let root = crate::caption::root();
    if !root.is_null() && ROOT_DISABLED.replace(block) != block {
        unsafe {
            let active = GetActiveWindow();
            EnableWindow(root, i32::from(!block));
            if !block && !active.is_null() {
                SetActiveWindow(root);
            }
        }
    }
}

fn state_id(id: egui::ViewportId) -> egui::Id {
    egui::Id::unique(("native-window", id))
}

pub fn handle(ctx: &egui::Context, id: egui::ViewportId) -> HWND {
    let raw = ctx
        .data(|data| data.get_temp::<isize>(state_id(id)))
        .unwrap_or_default() as HWND;
    if !raw.is_null() && unsafe { IsWindow(raw) } != 0 {
        raw
    } else {
        null_mut()
    }
}

pub fn show(
    ctx: &egui::Context,
    id: egui::ViewportId,
    mut builder: egui::ViewportBuilder,
    launched_from: HWND,
    interactive: bool,
    mut contents: impl FnMut(&mut egui::Ui, egui::ViewportClass),
) {
    let owner = crate::caption::root();
    if owner.is_null() || ctx.embed_viewports() {
        ctx.show_viewport_immediate(id, builder, contents);
        return;
    }

    let placement_owner = if launched_from.is_null() {
        owner
    } else {
        launched_from
    };
    let existing = handle(ctx, id);
    let creating = existing.is_null();
    let title = builder.title.clone().unwrap_or_default();
    // eframe's callback runs after the HWND is created, before its first paint.
    let before = if creating {
        windows_named(&title)
    } else {
        Vec::new()
    };
    let maximized = builder.maximized.unwrap_or_default();
    let zoom = ctx.zoom_factor();
    let size = builder.inner_size.unwrap_or(egui::vec2(900., 620.));
    if let Some(area) = Placement::read(placement_owner) {
        let capacity = area.client_capacity(zoom);
        builder = builder.with_inner_size(size.min(capacity));
        if let Some(minimum) = builder.min_inner_size {
            builder = builder.with_min_inner_size(minimum.min(capacity));
        }
    }
    // Keep these builder flags stable. Reveal after the first painted frame.
    builder = builder
        .with_visible(false)
        .with_taskbar(false)
        .with_maximized(false);
    ctx.show_viewport_immediate(id, builder, |ui, class| {
        if class == egui::ViewportClass::EmbeddedWindow {
            contents(ui, class);
            return;
        }
        let window = if creating {
            windows_named(&title)
                .into_iter()
                .find(|window| !before.contains(window))
                .unwrap_or(null_mut())
        } else {
            existing
        };
        if creating && !window.is_null() {
            ui.ctx()
                .data_mut(|data| data.insert_temp(state_id(id), window as isize));
            attach_and_place(window, owner, placement_owner, size, zoom);
        }
        if !window.is_null() && unsafe { IsWindowEnabled(window) } != i32::from(interactive) {
            unsafe {
                EnableWindow(window, i32::from(interactive));
            }
        }
        if creating {
            if maximized {
                ui.ctx()
                    .send_viewport_cmd(egui::ViewportCommand::Maximized(true));
            }
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::Visible(true));
            if interactive {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Focus);
            }
            ui.ctx().request_repaint();
        }
        contents(ui, class);
    });
}

fn windows_named(title: &str) -> Vec<HWND> {
    struct Matches {
        title: Vec<u16>,
        matches: Vec<HWND>,
        buffer: Vec<u16>,
    }
    unsafe extern "system" fn collect(window: HWND, context: LPARAM) -> windows_sys::core::BOOL {
        // EnumThreadWindows calls synchronously with this function's live context.
        let search = unsafe { &mut *(context as *mut Matches) };
        let length = unsafe { GetWindowTextLengthW(window) };
        if length as usize == search.title.len()
            && unsafe { GetWindowTextW(window, search.buffer.as_mut_ptr(), length + 1) } == length
            && search.buffer[..length as usize] == search.title
        {
            search.matches.push(window);
        }
        1
    }
    let title: Vec<_> = title.encode_utf16().collect();
    let mut search = Matches {
        buffer: vec![0; title.len() + 1],
        title,
        matches: Vec::new(),
    };
    unsafe {
        EnumThreadWindows(
            GetCurrentThreadId(),
            Some(collect),
            (&mut search as *mut Matches) as LPARAM,
        );
    }
    search.matches
}

struct Placement {
    owner: RECT,
    work: RECT,
    dpi: u32,
}

impl Placement {
    fn read(window: HWND) -> Option<Self> {
        let mut owner = RECT::default();
        let mut monitor = MONITORINFO {
            cbSize: size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        unsafe {
            let display = MonitorFromWindow(window, MONITOR_DEFAULTTONEAREST);
            let dpi = GetDpiForWindow(window);
            if GetWindowRect(window, &mut owner) == 0
                || GetMonitorInfoW(display, &mut monitor) == 0
                || dpi == 0
            {
                return None;
            }
            Some(Self {
                owner,
                work: monitor.rcWork,
                dpi,
            })
        }
    }

    fn scale(&self, zoom: f32) -> f32 {
        self.dpi as f32 / 96. * zoom
    }

    fn client_capacity(&self, zoom: f32) -> egui::Vec2 {
        // Leave room for the native frame while sizing the winit constraints.
        let scale = self.scale(1.);
        ((egui::vec2(
            (self.work.right - self.work.left) as f32,
            (self.work.bottom - self.work.top) as f32,
        ) - egui::vec2(20., 50.) * scale)
            / self.scale(zoom))
        .max(egui::Vec2::splat(1.))
    }

    fn fitted(&self, width: i32, height: i32) -> (i32, i32, i32, i32) {
        let width = width.clamp(1, (self.work.right - self.work.left).max(1));
        let height = height.clamp(1, (self.work.bottom - self.work.top).max(1));
        let center_x = self.owner.left + (self.owner.right - self.owner.left) / 2;
        let center_y = self.owner.top + (self.owner.bottom - self.owner.top) / 2;
        let x = (center_x - width / 2).clamp(self.work.left, self.work.right - width);
        let y = (center_y - height / 2).clamp(self.work.top, self.work.bottom - height);
        (x, y, width, height)
    }
}

fn attach_and_place(window: HWND, owner: HWND, placement_owner: HWND, size: egui::Vec2, zoom: f32) {
    unsafe {
        // GWLP_HWNDPARENT assigns the owner of a top-level window without
        // converting the viewport into a child or changing its native caption.
        SetWindowLongPtrW(window, GWLP_HWNDPARENT, owner as isize);
        if let Some(area) = Placement::read(placement_owner) {
            let size = size.min(area.client_capacity(zoom)) * area.scale(zoom);
            let mut frame = RECT {
                right: size.x.round() as i32,
                bottom: size.y.round() as i32,
                ..Default::default()
            };
            AdjustWindowRectExForDpi(
                &mut frame,
                GetWindowLongW(window, GWL_STYLE) as u32,
                0,
                GetWindowLongW(window, GWL_EXSTYLE) as u32,
                area.dpi,
            );
            let (x, y, width, height) =
                area.fitted(frame.right - frame.left, frame.bottom - frame.top);
            // Crossing monitors can let WM_DPICHANGED resize the hidden target.
            // Apply the same bounds after that synchronous transition.
            for _ in 0..2 {
                SetWindowPos(
                    window,
                    null_mut(),
                    x,
                    y,
                    width,
                    height,
                    SWP_NOACTIVATE | SWP_NOZORDER,
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_placement_uses_owner_monitor_work_bounds_and_dpi() {
        let placement = Placement {
            owner: RECT {
                left: -1720,
                top: 245,
                right: -520,
                bottom: 1045,
            },
            work: RECT {
                left: -1920,
                top: 220,
                right: 0,
                bottom: 1260,
            },
            dpi: 144,
        };
        assert_eq!(placement.fitted(900, 500), (-1570, 395, 900, 500));
        assert_eq!(placement.fitted(2200, 1300), (-1920, 220, 1920, 1040));
        let native = placement.client_capacity(0.8) * placement.scale(0.8);
        assert!((native - egui::vec2(1890., 965.)).length() < 0.01);
        let native = placement.client_capacity(1.8) * placement.scale(1.8);
        assert!((native - egui::vec2(1890., 965.)).length() < 0.01);
    }

    #[test]
    fn hidden_native_dialog_attaches_to_the_owner_and_uses_its_work_area() {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            CreateWindowExW, DestroyWindow, GW_OWNER, GetWindow, IsWindowVisible,
            WS_OVERLAPPEDWINDOW,
        };
        struct Window(HWND);
        impl Drop for Window {
            fn drop(&mut self) {
                unsafe {
                    DestroyWindow(self.0);
                }
            }
        }
        fn hidden(title: &str) -> Window {
            let class: Vec<u16> = "STATIC\0".encode_utf16().collect();
            let title: Vec<u16> = title.encode_utf16().chain([0]).collect();
            let hwnd = unsafe {
                CreateWindowExW(
                    0,
                    class.as_ptr(),
                    title.as_ptr(),
                    WS_OVERLAPPEDWINDOW,
                    40,
                    60,
                    800,
                    620,
                    null_mut(),
                    null_mut(),
                    null_mut(),
                    null_mut(),
                )
            };
            assert!(!hwnd.is_null());
            Window(hwnd)
        }
        let owner = hidden("Shovel placement fixture owner");
        let title = "Shovel placement fixture child";
        assert!(windows_named(title).is_empty());
        let child = hidden(title);
        assert_eq!(windows_named(title), [child.0]);
        attach_and_place(child.0, owner.0, owner.0, egui::vec2(420., 230.), 1.);
        unsafe {
            assert_eq!(GetWindow(child.0, GW_OWNER), owner.0);
            assert_eq!(
                MonitorFromWindow(child.0, MONITOR_DEFAULTTONEAREST),
                MonitorFromWindow(owner.0, MONITOR_DEFAULTTONEAREST)
            );
            let area = Placement::read(owner.0).unwrap();
            let mut rect = RECT::default();
            assert_ne!(GetWindowRect(child.0, &mut rect), 0);
            assert_eq!(
                area.fitted(rect.right - rect.left, rect.bottom - rect.top),
                (
                    rect.left,
                    rect.top,
                    rect.right - rect.left,
                    rect.bottom - rect.top
                )
            );
            assert_eq!(IsWindowVisible(child.0), 0);
            assert_eq!(IsWindowVisible(owner.0), 0);
        }
    }
}
