//! Explicit Windows handoff. Preparation runs after submission, without Git;
//! native shell work is bounded to one worker and never owns the opened app.
use shovel_core::{git::Snapshot, identity, working};
use std::{
    fs,
    os::windows::{ffi::OsStrExt, fs::MetadataExt},
    path::{Path, PathBuf},
    ptr,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
use windows_sys::Win32::{
    System::Com::{
        COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE, CoInitializeEx, CoTaskMemFree,
        CoUninitialize,
    },
    UI::{
        Shell::{
            SEE_MASK_FLAG_NO_UI, SEE_MASK_NOASYNC, SHELLEXECUTEINFOW, SHOpenFolderAndSelectItems,
            SHParseDisplayName, ShellExecuteExW,
        },
        WindowsAndMessaging::SW_SHOWNORMAL,
    },
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Target {
    pub path: PathBuf,
    pub select_in_explorer: bool,
    pub missing_file_folder: bool,
}
pub type Launcher = Arc<dyn Fn(&Target) -> Result<(), &'static str> + Send + Sync>;
static ACTIVE: AtomicBool = AtomicBool::new(false);
pub struct Guard;
impl Guard {
    pub fn acquire() -> Result<Self, &'static str> {
        ACTIVE
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map(|_| Self)
            .map_err(|_| "FileMenuShellBusy")
    }
}
impl Drop for Guard {
    fn drop(&mut self) {
        ACTIVE.store(false, Ordering::Release);
    }
}
fn metadata(path: &Path) -> Result<Option<fs::Metadata>, &'static str> {
    match fs::symlink_metadata(path) {
        Ok(m) if m.file_attributes() & 0x400 != 0 => Err("FileMenuLinkedPath"),
        Ok(m) => Ok(Some(m)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(_) => Err("FileMenuOpenFailed"),
    }
}
pub fn prepare(
    repository: &Snapshot,
    relative: &str,
    explorer: bool,
) -> Result<Target, &'static str> {
    let root = &repository.worktree;
    if identity::read(root).map_err(|_| "FileMenuStale")? != repository.identity {
        return Err("FileMenuStale");
    }
    let file = working::path(root, relative).map_err(|_| "FileMenuMissingFile")?;
    let mut part = file.as_path();
    loop {
        metadata(part)?;
        if part == root {
            break;
        }
        part = part.parent().ok_or("FileMenuMissingFile")?;
    }
    if metadata(&file)?.is_some_and(|m| m.is_file()) {
        return Ok(Target {
            path: file,
            select_in_explorer: explorer,
            missing_file_folder: false,
        });
    }
    if !explorer {
        return Err("FileMenuMissingFile");
    }
    let mut target = file.as_path();
    loop {
        if metadata(target)?.is_some_and(|m| m.is_dir()) {
            return Ok(Target {
                path: target.to_owned(),
                select_in_explorer: false,
                missing_file_folder: true,
            });
        }
        if target == root {
            return Err("FileMenuMissingFile");
        }
        target = target.parent().ok_or("FileMenuMissingFile")?;
    }
}
struct Apartment;
impl Drop for Apartment {
    fn drop(&mut self) {
        unsafe {
            CoUninitialize();
        }
    }
}
fn succeeded(result: i32) -> Result<(), &'static str> {
    if result < 0 {
        Err("FileMenuOpenFailed")
    } else {
        Ok(())
    }
}
pub fn launch(target: &Target) -> Result<(), &'static str> {
    let path = target
        .path
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    // Fresh scheduler worker; COM is initialized and released on this thread.
    unsafe {
        succeeded(CoInitializeEx(
            ptr::null(),
            (COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE) as u32,
        ))?;
    }
    let _apartment = Apartment;
    if target.select_in_explorer {
        let mut pidl = ptr::null_mut();
        let mut attributes = 0;
        // Windows allocates the absolute item ID list. Zero children means
        // select this item in its parent. Free even after a failed open.
        let parsed = unsafe {
            SHParseDisplayName(
                path.as_ptr(),
                ptr::null_mut(),
                &mut pidl,
                0,
                &mut attributes,
            )
        };
        if parsed >= 0 && pidl.is_null() {
            return Err("FileMenuOpenFailed");
        }
        let result = if parsed < 0 {
            parsed
        } else {
            unsafe { SHOpenFolderAndSelectItems(pidl, 0, ptr::null(), 0) }
        };
        if !pidl.is_null() {
            unsafe {
                CoTaskMemFree(pidl.cast());
            }
        }
        succeeded(result)
    } else {
        let verb = [b'o' as u16, b'p' as u16, b'e' as u16, b'n' as u16, 0];
        let mut info = SHELLEXECUTEINFOW {
            cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
            fMask: SEE_MASK_NOASYNC | SEE_MASK_FLAG_NO_UI,
            lpVerb: verb.as_ptr(),
            lpFile: path.as_ptr(),
            nShow: SW_SHOWNORMAL,
            ..Default::default()
        };
        // No command-line concatenation, environment expansion, process/job
        // ownership, wait-for-exit or cancellation of the handed-off app.
        if unsafe { ShellExecuteExW(&mut info) } == 0 {
            Err("FileMenuOpenFailed")
        } else {
            Ok(())
        }
    }
}
