//! Local Windows directory/file identity, matching the baseline's replacement
//! checks. These checks are not a repository trust or permissions dialogue.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::Read,
    mem::{size_of, zeroed},
    os::windows::{
        ffi::OsStrExt,
        fs::MetadataExt,
        io::{AsRawHandle, FromRawHandle, OwnedHandle},
    },
    path::{Component, Path, PathBuf},
};
use windows_sys::Win32::{Foundation::INVALID_HANDLE_VALUE, Storage::FileSystem::*};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Identity {
    pub root: PathBuf,
    pub git: PathBuf,
    pub common: PathBuf,
    pub fingerprint: String,
}
fn unavailable() -> std::io::Error {
    std::io::Error::other("repository identity unavailable")
}
fn canonical(path: &Path) -> std::io::Result<PathBuf> {
    if !path.is_absolute() {
        return Err(unavailable());
    }
    let absolute = std::path::absolute(path)?;
    let text = absolute.to_string_lossy().replace('/', "\\");
    if text.starts_with("\\\\") {
        return Err(unavailable());
    }
    let root = absolute.components().take(2).collect::<PathBuf>();
    let drive: Vec<u16> = root.as_os_str().encode_wide().chain(Some(0)).collect();
    // DRIVE_FIXED = 3. Do not silently broaden the baseline to network/reparse roots.
    if unsafe { GetDriveTypeW(drive.as_ptr()) } != 3 {
        return Err(unavailable());
    }
    Ok(PathBuf::from(text.trim_end_matches('\\')))
}
fn reject_reparse(path: &Path) -> std::io::Result<()> {
    let mut cursor = PathBuf::new();
    for component in path.components() {
        cursor.push(component.as_os_str());
        if matches!(component, Component::Prefix(_)) {
            continue;
        }
        if fs::symlink_metadata(&cursor)?.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Err(unavailable());
        }
    }
    Ok(())
}
fn pointer(path: &Path) -> std::io::Result<String> {
    reject_reparse(path)?;
    let mut bytes = Vec::new();
    File::open(path)?.take(8193).read_to_end(&mut bytes)?;
    if bytes.len() > 8192 {
        return Err(unavailable());
    }
    String::from_utf8(bytes).map_err(|_| unavailable())
}
fn resolve(path: &Path, prefix: &str, parent: &Path) -> std::io::Result<PathBuf> {
    let text = pointer(path)?;
    let text = text.trim_end_matches(['\r', '\n']);
    let value = text
        .strip_prefix(prefix)
        .filter(|v| !v.is_empty() && !v.contains(['\n', '\0']))
        .ok_or_else(unavailable)?;
    canonical(&parent.join(value))
}
fn append(hash: &mut Sha256, text: &str) {
    hash.update((text.len() as u32).to_le_bytes());
    hash.update(text.as_bytes());
}
pub fn read(path: &Path) -> std::io::Result<Identity> {
    let root = canonical(path)?;
    reject_reparse(&root)?;
    let marker = root.join(".git");
    reject_reparse(&marker)?;
    let git = if marker.is_dir() {
        canonical(&marker)?
    } else {
        resolve(&marker, "gitdir: ", &root)?
    };
    reject_reparse(&git)?;
    let common_file = git.join("commondir");
    let common = if common_file.exists() {
        resolve(&common_file, "", &git)?
    } else {
        git.clone()
    };
    let mut digest = Sha256::new();
    for path in [&root, &marker, &git, &common] {
        reject_reparse(path)?;
        let path = canonical(path)?;
        let wide = path
            .as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect::<Vec<u16>>();
        let handle = unsafe {
            CreateFileW(
                wide.as_ptr(),
                0,
                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                std::ptr::null(),
                OPEN_EXISTING,
                FILE_FLAG_BACKUP_SEMANTICS,
                std::ptr::null_mut(),
            )
        };
        if handle == INVALID_HANDLE_VALUE {
            return Err(std::io::Error::last_os_error());
        }
        let handle = unsafe { OwnedHandle::from_raw_handle(handle) };
        let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { zeroed() };
        let mut id: FILE_ID_INFO = unsafe { zeroed() };
        if unsafe { GetFileInformationByHandle(handle.as_raw_handle(), &mut info) } == 0
            || unsafe {
                GetFileInformationByHandleEx(
                    handle.as_raw_handle(),
                    FileIdInfo,
                    (&mut id as *mut FILE_ID_INFO).cast(),
                    size_of::<FILE_ID_INFO>() as u32,
                )
            } == 0
        {
            return Err(std::io::Error::last_os_error());
        }
        let low = u64::from_le_bytes(
            id.FileId.Identifier[..8]
                .try_into()
                .map_err(|_| unavailable())?,
        );
        let high = u64::from_le_bytes(
            id.FileId.Identifier[8..]
                .try_into()
                .map_err(|_| unavailable())?,
        );
        if low | high == 0 {
            return Err(unavailable());
        }
        append(&mut digest, &path.to_string_lossy().to_uppercase());
        append(
            &mut digest,
            &format!(
                "{:016X}:{high:016X}{low:016X}:{:08X}{:08X}",
                id.VolumeSerialNumber,
                info.ftCreationTime.dwHighDateTime,
                info.ftCreationTime.dwLowDateTime
            ),
        );
    }
    if !marker.is_dir() {
        append(&mut digest, &pointer(&marker)?);
    }
    if common_file.exists() {
        append(&mut digest, &pointer(&common_file)?);
    }
    Ok(Identity {
        root,
        git,
        common,
        fingerprint: format!("{:X}", digest.finalize()),
    })
}
