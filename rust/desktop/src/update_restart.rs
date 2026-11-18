use crate::strings::{format, text};
use shovel_core::{git::Git, identity::Identity, process::Cancellation};
use std::path::PathBuf;

pub(super) struct Repository {
    pub path: PathBuf,
    pub observed: Option<Identity>,
    pub identity_blocked: bool,
}

/// Reinspect each open worktree on the restart worker, including unvisited tabs.
/// Index conflicts from a stash or a patch may have no active Git sequence.
pub(super) fn check_repositories(
    git: Option<&Git>,
    repositories: &[Repository],
) -> Result<(), String> {
    let cancel = Cancellation::default();
    for repository in repositories {
        let blocked = || {
            format(
                "RustUpdateWorktreeBlocked",
                &[&repository.path.to_string_lossy()],
            )
        };
        if repository.identity_blocked {
            return Err(blocked());
        }
        let git = git.ok_or_else(blocked)?;
        let snapshot = git
            .inspect(&repository.path, &cancel)
            .map_err(|_| blocked())?;
        if repository
            .observed
            .as_ref()
            .is_some_and(|expected| *expected != snapshot.identity)
            || !snapshot.operations.is_empty()
        {
            return Err(blocked());
        }
        let unmerged = git
            .read(
                &repository.path,
                ["ls-files", "--unmerged", "-z"],
                &cancel,
                256 * 1024,
            )
            .map_err(|_| blocked())?;
        if !unmerged.stdout.is_empty()
            || shovel_core::identity::read(&repository.path).map_err(|_| blocked())?
                != snapshot.identity
        {
            return Err(blocked());
        }
    }
    Ok(())
}

// Check all Windows sessions. The C# client may have the same executable name
// in a separate installation.
pub(super) fn other_instances() -> Result<(), String> {
    let current = std::env::current_exe().map_err(|_| text("RustUpdateInstances"))?;
    let directory = current
        .parent()
        .ok_or_else(|| text("RustUpdateInstances"))?;
    check_installation_processes(directory, "Shovel.exe", "ShovelRust.exe")
}

fn check_installation_processes(
    directory: &std::path::Path,
    executable_name: &str,
    legacy_name: &str,
) -> Result<(), String> {
    use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
    use windows_sys::Win32::{
        Foundation::{ERROR_NO_MORE_FILES, GetLastError, INVALID_HANDLE_VALUE},
        System::Diagnostics::ToolHelp::{
            CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
            TH32CS_SNAPPROCESS,
        },
    };
    let blocked = || text("RustUpdateInstances");
    // ToolHelp requires dwSize before its first call. OwnedHandle closes the snapshot on all exits.
    unsafe {
        let raw = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if raw == INVALID_HANDLE_VALUE {
            return Err(blocked());
        }
        let snapshot = OwnedHandle::from_raw_handle(raw);
        let mut entry: PROCESSENTRY32W = std::mem::zeroed();
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        if Process32FirstW(snapshot.as_raw_handle(), &mut entry) == 0 {
            return Err(blocked());
        }
        loop {
            let end = entry
                .szExeFile
                .iter()
                .position(|ch| *ch == 0)
                .unwrap_or(entry.szExeFile.len());
            let name = String::from_utf16_lossy(&entry.szExeFile[..end]);
            if entry.th32ProcessID != std::process::id()
                && (name.eq_ignore_ascii_case(legacy_name)
                    || name.eq_ignore_ascii_case(executable_name)
                        && process_uses_installation(entry.th32ProcessID, directory)
                            .map_err(|_| blocked())?)
            {
                return Err(blocked());
            }
            if Process32NextW(snapshot.as_raw_handle(), &mut entry) == 0 {
                return if GetLastError() == ERROR_NO_MORE_FILES {
                    Ok(())
                } else {
                    Err(blocked())
                };
            }
        }
    }
}

fn process_uses_installation(pid: u32, directory: &std::path::Path) -> std::io::Result<bool> {
    use std::os::windows::{
        ffi::OsStringExt,
        io::{AsRawHandle, FromRawHandle, OwnedHandle},
    };
    use windows_sys::Win32::{
        Foundation::{ERROR_INVALID_PARAMETER, GetLastError, STILL_ACTIVE},
        System::Threading::{
            GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
            QueryFullProcessImageNameW,
        },
    };
    let mut image = vec![0u16; 32768];
    let mut length = image.len() as u32;
    unsafe {
        let raw = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if raw.is_null() {
            let error = GetLastError();
            return if error == ERROR_INVALID_PARAMETER {
                Ok(false)
            } else {
                Err(std::io::Error::from_raw_os_error(error as i32))
            };
        }
        let process = OwnedHandle::from_raw_handle(raw);
        let image_error = (QueryFullProcessImageNameW(
            process.as_raw_handle(),
            0,
            image.as_mut_ptr(),
            &mut length,
        ) == 0)
            .then(std::io::Error::last_os_error);
        // A process may exit after the snapshot. Windows can still return its image
        // while another owner holds its handle, even though it no longer blocks restart.
        let mut exit_code = 0;
        if GetExitCodeProcess(process.as_raw_handle(), &mut exit_code) != 0
            && exit_code != STILL_ACTIVE as u32
        {
            return Ok(false);
        }
        if let Some(error) = image_error {
            return Err(error);
        }
    }
    let path = PathBuf::from(std::ffi::OsString::from_wide(&image[..length as usize]));
    let parent = path
        .parent()
        .ok_or_else(|| std::io::Error::other("process image has no directory"))?;
    same_file::is_same_file(parent, directory)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn restart_guard_allows_another_install_and_rechecks_after_a_competing_process_exits() {
        use std::{
            os::windows::process::CommandExt,
            process::{Child, Command, Stdio},
        };
        use windows_sys::Win32::System::Threading::CREATE_NO_WINDOW;

        struct Fixture(Child);
        impl Drop for Fixture {
            fn drop(&mut self) {
                drop(self.0.stdin.take());
                let _ = self.0.wait();
            }
        }
        let temporary = tempfile::tempdir().unwrap();
        let installation = temporary.path().join("Rust Beta");
        let other = temporary.path().join("another install");
        let executable = format!("shovel-restart-{}.exe", std::process::id());
        let legacy = format!("shovel-legacy-{}.exe", std::process::id());
        let source =
            PathBuf::from(std::env::var_os("SystemRoot").unwrap()).join("System32/findstr.exe");
        let start = |directory: &std::path::Path, name: &str| {
            std::fs::create_dir_all(directory).unwrap();
            let image = directory.join(name);
            std::fs::copy(&source, &image).unwrap();
            let mut child = Command::new(image)
                .arg("fixture")
                .creation_flags(CREATE_NO_WINDOW)
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap();
            assert!(child.try_wait().unwrap().is_none());
            Fixture(child)
        };
        std::fs::create_dir_all(&installation).unwrap();
        let check = || check_installation_processes(&installation, &executable, &legacy);
        let _unrelated = start(&other, &executable);
        check().unwrap();
        let mut competing = start(&installation, &executable);
        assert_eq!(check().unwrap_err(), text("RustUpdateInstances"));
        drop(competing.0.stdin.take());
        competing.0.wait().unwrap();
        assert!(!process_uses_installation(competing.0.id(), &installation).unwrap());
        drop(competing);
        check().unwrap();
        let old_rust = start(&other, &legacy);
        assert_eq!(check().unwrap_err(), text("RustUpdateInstances"));
        drop(old_rust);
        check().unwrap();
    }

    #[test]
    fn process_image_matches_its_physical_directory_and_distinguishes_other_installs() {
        let exe = std::env::current_exe().unwrap();
        let installation = exe.parent().unwrap();
        let canonical = installation.canonicalize().unwrap();
        let alias = PathBuf::from(installation.to_string_lossy().to_uppercase());
        for directory in [installation, canonical.as_path(), alias.as_path()] {
            assert!(process_uses_installation(std::process::id(), directory).unwrap());
        }
        let other = tempfile::tempdir().unwrap();
        assert!(!process_uses_installation(std::process::id(), other.path()).unwrap());
        assert!(
            process_uses_installation(std::process::id(), &other.path().join("missing")).is_err()
        );
    }
}
