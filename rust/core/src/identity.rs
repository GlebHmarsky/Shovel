//! Local Windows directory/file identity, matching the baseline's replacement
//! checks. These checks are not a repository trust or permissions dialogue.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::Read,
    mem::{size_of, zeroed},
    os::windows::{
        ffi::{OsStrExt, OsStringExt},
        fs::MetadataExt,
        io::{AsRawHandle, FromRawHandle, OwnedHandle},
    },
    path::{Component, Path, PathBuf},
};
use windows_sys::Win32::{Foundation::INVALID_HANDLE_VALUE, Storage::FileSystem::*};

#[derive(Clone, Debug, Serialize, Deserialize, Eq)]
pub struct Identity {
    pub root: PathBuf,
    pub git: PathBuf,
    pub common: PathBuf,
    pub fingerprint: String,
    // Read-only fallback for a draft created through this spelling by an older
    // build. Only the live identity reader can supply an alternate draft key.
    #[serde(skip)]
    previous_fingerprint: Option<String>,
}
impl PartialEq for Identity {
    fn eq(&self, other: &Self) -> bool {
        // Git reports the on-disk capitalization, even when the user selected
        // another spelling. The fingerprint also checks file IDs and pointers.
        let same_path =
            |a: &Path, b: &Path| a == b || crate::case::path_key(a) == crate::case::path_key(b);
        self.fingerprint == other.fingerprint
            && same_path(&self.root, &other.root)
            && same_path(&self.git, &other.git)
            && same_path(&self.common, &other.common)
    }
}
impl Identity {
    pub fn previous_fingerprint(&self) -> Option<&str> {
        self.previous_fingerprint.as_deref()
    }
    pub fn matches_recorded_fingerprint(&self, value: &str) -> bool {
        self.fingerprint == value || self.previous_fingerprint() == Some(value)
    }
}
fn unavailable() -> std::io::Error {
    std::io::Error::other("repository identity unavailable")
}
fn canonical(path: &Path) -> std::io::Result<PathBuf> {
    if !path.is_absolute() {
        return Err(unavailable());
    }
    let absolute = std::path::absolute(path)?;
    let text = absolute
        .to_str()
        .ok_or_else(unavailable)?
        .replace('/', "\\");
    if text.starts_with("\\\\") {
        return Err(unavailable());
    }
    let root = absolute.components().take(2).collect::<PathBuf>();
    let drive: Vec<u16> = root.as_os_str().encode_wide().chain(Some(0)).collect();
    // DRIVE_FIXED = 3. Do not silently broaden the baseline to network/reparse roots.
    if unsafe { GetDriveTypeW(drive.as_ptr()) } != 3 {
        return Err(unavailable());
    }
    if absolute.parent().is_none() {
        Ok(absolute)
    } else {
        Ok(PathBuf::from(text.trim_end_matches('\\')))
    }
}
// Preserve the entered spelling when a parent cannot be enumerated. Such
// paths still go through the native file-ID and Git directory checks.
pub(crate) fn long_name(path: &Path) -> PathBuf {
    let source: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let mut buffer = vec![0; source.len().max(260)];
    loop {
        let length =
            unsafe { GetLongPathNameW(source.as_ptr(), buffer.as_mut_ptr(), buffer.len() as u32) }
                as usize;
        if length == 0 || length > 32768 {
            return path.to_owned();
        }
        if length < buffer.len() {
            return std::ffi::OsString::from_wide(&buffer[..length]).into();
        }
        buffer.resize(length + 1, 0);
    }
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
    let plain = marker.is_dir();
    let git = if plain {
        canonical(&marker)?
    } else {
        resolve(&marker, "gitdir: ", &root)?
    };
    reject_reparse(&git)?;
    let common_file = git.join("commondir");
    let linked = common_file.try_exists()?;
    let common = if linked {
        resolve(&common_file, "", &git)?
    } else {
        git.clone()
    };
    let long_root = long_name(&root);
    let long_marker = long_root.join(".git");
    let long_git = if plain {
        long_marker.clone()
    } else {
        long_name(&git)
    };
    let long_common = if linked {
        long_name(&common)
    } else {
        long_git.clone()
    };
    let mut previous =
        (root != long_root || git != long_git || common != long_common).then(Sha256::new);
    let mut digest = Sha256::new();
    for (path, expanded) in [
        (&root, &long_root),
        (&marker, &long_marker),
        (&git, &long_git),
        (&common, &long_common),
    ] {
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
        // Published Rust v1 fingerprint encoding: preserve it for existing
        // draft/profile identities. Native shared locks use case::invariant_upper;
        // this opaque, file-ID-backed digest is not a C# interchange field.
        let file_id = format!(
                "{:016X}:{high:016X}{low:016X}:{:08X}{:08X}",
                id.VolumeSerialNumber,
                info.ftCreationTime.dwHighDateTime,
                info.ftCreationTime.dwLowDateTime
        );
        append(&mut digest, &expanded.to_string_lossy().to_uppercase());
        append(&mut digest, &file_id);
        if let Some(hash) = &mut previous {
            append(hash, &path.to_string_lossy().to_uppercase());
            append(hash, &file_id);
        }
    }
    for path in [(!plain).then_some(&marker), linked.then_some(&common_file)]
        .into_iter()
        .flatten()
    {
        let bytes = pointer(path)?;
        append(&mut digest, &bytes);
        if let Some(hash) = &mut previous {
            append(hash, &bytes);
        }
    }
    let fingerprint = format!("{:X}", digest.finalize());
    let previous_fingerprint = previous
        .map(|hash| format!("{:X}", hash.finalize()))
        .filter(|old| *old != fingerprint);
    Ok(Identity {
        root: long_root,
        git: long_git,
        common: long_common,
        fingerprint,
        previous_fingerprint,
    })
}
