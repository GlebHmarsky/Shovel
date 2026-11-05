//! Construct the picker on the UI thread, then show it on its native STA worker.
//! The editor keeps its owning window alive while a picker is outstanding.
use crate::strings::text;
use raw_window_handle::{
    DisplayHandle, HandleError, HasDisplayHandle, HasWindowHandle, RawWindowHandle,
    Win32WindowHandle, WindowHandle,
};
use std::num::NonZeroIsize;
struct Owner(Win32WindowHandle);
impl HasWindowHandle for Owner {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        // GetActiveWindow supplies a live window of this UI thread. The editor
        // refuses owner closure until the picker response has been received.
        unsafe { Ok(WindowHandle::borrow_raw(RawWindowHandle::Win32(self.0))) }
    }
}
impl HasDisplayHandle for Owner {
    fn display_handle(&self) -> Result<DisplayHandle<'_>, HandleError> {
        Ok(DisplayHandle::windows())
    }
}
pub fn patch() -> rfd::FileDialog {
    let mut dialog = rfd::FileDialog::new()
        .set_title(text("FilePatchTitle"))
        .add_filter(text("RustPatchFilter"), &["patch"])
        .add_filter(text("RustAllFilesFilter"), &["*"]);
    // Read only our thread's active window; never attach to another app's
    // foreground window. No filesystem, Git or tool discovery is done here.
    let active = unsafe { windows_sys::Win32::UI::Input::KeyboardAndMouse::GetActiveWindow() };
    if let Some(handle) = NonZeroIsize::new(active as isize) {
        dialog = dialog.set_parent(&Owner(Win32WindowHandle::new(handle)));
    }
    dialog
}
