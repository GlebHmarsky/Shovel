//! Standard LFS status uses local pointer hashes; reads never invoke filters or
//! hydrate assets. Unsupported pointers and uncertain file reads stay visible.
use crate::{
    git::{self, Git},
    process::{self, Cancellation, Lifetime},
    working::{Change, Error, Group, path},
};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::OsString,
    fs::{self, OpenOptions},
    io::Read,
    os::windows::fs::{MetadataExt, OpenOptionsExt},
    path::Path,
};
struct Pointer {
    oid: String,
    size: u64,
}
fn pointer(bytes: &[u8]) -> Option<Pointer> {
    let text = std::str::from_utf8(bytes).ok()?;
    let lines = text.split('\n').collect::<Vec<_>>();
    if lines.len() != 4
        || lines[0] != "version https://git-lfs.github.com/spec/v1"
        || !lines[3].is_empty()
    {
        return None;
    }
    let oid = lines[1].strip_prefix("oid sha256:")?;
    if oid.len() != 64
        || !oid
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return None;
    }
    let size = lines[2].strip_prefix("size ")?;
    if size.is_empty() || !size.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    Some(Pointer {
        oid: oid.into(),
        size: size.parse().ok()?,
    })
}
pub(crate) fn normalize(
    git: &Git,
    root: &Path,
    entries: &mut Vec<Change>,
    cancel: &Cancellation,
) -> Result<(), Error> {
    let candidates = entries
        .iter()
        .filter(|e| {
            e.group == Group::Unstaged
                && e.code == 'M'
                && e.original.is_none()
                && matches!(e.index_mode.as_str(), "100644" | "100755")
                && e.index_mode == e.worktree_mode
                && matches!(e.index_object.len(), 40 | 64)
                && e.index_object.bytes().all(|b| b.is_ascii_hexdigit())
        })
        .collect::<Vec<_>>();
    let mut clean = BTreeSet::new();
    let mut pointers = BTreeMap::new();
    for batch in candidates.chunks(64) {
        if batch
            .iter()
            .map(|e| e.path.encode_utf16().count() + 3)
            .sum::<usize>()
            > 16000
        {
            continue;
        }
        let mut args = ["--literal-pathspecs", "check-attr", "-z", "filter", "--"]
            .map(OsString::from)
            .to_vec();
        args.extend(batch.iter().map(|e| OsString::from(&e.path)));
        let attributes = git.read(root, args, cancel, 256 * 1024)?.stdout;
        let fields = std::str::from_utf8(&attributes)
            .map_err(|_| Error::Invalid)?
            .split_terminator('\0')
            .collect::<Vec<_>>();
        if fields.len() % 3 != 0 {
            return Err(Error::Invalid);
        }
        let lfs = fields
            .as_chunks::<3>()
            .0
            .iter()
            .filter(|f| f[1] == "filter" && f[2] == "lfs")
            .map(|f| f[0])
            .collect::<BTreeSet<_>>();
        let selected = batch
            .iter()
            .filter(|e| lfs.contains(e.path.as_str()))
            .collect::<Vec<_>>();
        let hashes = selected
            .iter()
            .map(|e| e.index_object.clone())
            .filter(|h| !pointers.contains_key(h))
            .collect::<BTreeSet<_>>();
        read_pointers(git, root, &hashes, &mut pointers, cancel)?;
        for entry in selected {
            if cancel.is_cancelled() {
                return Err(Error::Io(std::io::ErrorKind::Interrupted.into()));
            }
            if let Some(Some(pointer)) = pointers.get(&entry.index_object)
                && matches_content(root, &entry.path, pointer, cancel) == Some(true)
            {
                clean.insert(entry.path.clone());
            }
        }
    }
    entries.retain(|e| e.group != Group::Unstaged || !clean.contains(&e.path));
    Ok(())
}
fn read_pointers(
    git: &Git,
    root: &Path,
    hashes: &BTreeSet<String>,
    pointers: &mut BTreeMap<String, Option<Pointer>>,
    cancel: &Cancellation,
) -> Result<(), Error> {
    if !hashes.is_empty() {
        let mut request = git.request(
            root,
            [
                "cat-file",
                "--batch-check=%(objectname) %(objecttype) %(objectsize)",
            ],
            Lifetime::Read,
            65536,
        );
        request.input = hashes
            .iter()
            .map(|h| format!("{h}\n"))
            .collect::<String>()
            .into_bytes();
        let sizes = git::require(process::run(&request, cancel, None)?)?.stdout;
        let mut small = Vec::new();
        for line in std::str::from_utf8(&sizes)
            .map_err(|_| Error::Invalid)?
            .lines()
        {
            let fields = line.split(' ').collect::<Vec<_>>();
            if fields.len() != 3 || !hashes.contains(fields[0]) {
                return Err(Error::Invalid);
            }
            let size = fields[2].parse::<usize>().map_err(|_| Error::Invalid)?;
            if fields[1] == "blob" && size > 0 && size < 1024 {
                small.push((fields[0].to_owned(), size));
            } else {
                pointers.insert(fields[0].to_owned(), None);
            }
        }
        if !small.is_empty() {
            let mut request =
                git.request(root, ["cat-file", "--batch"], Lifetime::Read, 128 * 1024);
            request.input = small
                .iter()
                .map(|(h, _)| format!("{h}\n"))
                .collect::<String>()
                .into_bytes();
            let bytes = git::require(process::run(&request, cancel, None)?)?.stdout;
            let mut offset = 0;
            for (hash, size) in small {
                let end = bytes
                    .get(offset..)
                    .ok_or(Error::Invalid)?
                    .iter()
                    .position(|b| *b == b'\n')
                    .ok_or(Error::Invalid)?
                    + offset;
                if bytes.get(offset..end) != Some(format!("{hash} blob {size}").as_bytes())
                    || bytes.get(end + size + 1) != Some(&b'\n')
                {
                    return Err(Error::Invalid);
                }
                pointers.insert(hash, pointer(&bytes[end + 1..end + size + 1]));
                offset = end + size + 2;
            }
            if offset != bytes.len() {
                return Err(Error::Invalid);
            }
        }
    }
    Ok(())
}

/// Recheck the reviewed standard-LFS subset independently of Git's stat cache.
/// Unsupported attributes/index flags/objects fall back to normal status.
pub(crate) fn discard_changes(
    git: &Git,
    context: &crate::working::Context,
    selected: &[Change],
    cancel: &Cancellation,
) -> Result<Option<Vec<Change>>, Error> {
    if !context.standard_lfs
        || selected.is_empty()
        || selected.len() > 128
        || selected.iter().any(|e| {
            e.group != Group::Unstaged
                || e.code != 'M'
                || e.original.is_some()
                || e.submodule != "N..."
                || e.index_mode != "100644"
                || e.worktree_mode != e.index_mode
        })
    {
        return Ok(None);
    }
    let paths = selected
        .iter()
        .map(|e| e.path.as_str())
        .collect::<BTreeSet<_>>();
    if paths.len() != selected.len()
        || paths
            .iter()
            .map(|p| p.encode_utf16().count() + 3)
            .sum::<usize>()
            >= 16000
    {
        return Ok(None);
    }
    let root = &context.repository.worktree;
    let mut args = vec![
        "--literal-pathspecs",
        "ls-files",
        "--stage",
        "-v",
        "-z",
        "--",
    ];
    args.extend(paths.iter().copied());
    let index = git.read(root, args, cancel, 256 * 1024)?.stdout;
    let actual = std::str::from_utf8(&index)
        .map_err(|_| Error::Invalid)?
        .split_terminator('\0')
        .map(str::to_owned)
        .collect::<BTreeSet<_>>();
    let expected = selected
        .iter()
        .map(|e| format!("H {} {} 0\t{}", e.index_mode, e.index_object, e.path))
        .collect();
    if index.last() != Some(&0) || actual != expected {
        return Ok(None);
    }
    let mut args = vec![
        "--literal-pathspecs",
        "check-attr",
        "-z",
        "filter",
        "text",
        "working-tree-encoding",
        "--",
    ];
    args.extend(paths.iter().copied());
    let attributes = git.read(root, args, cancel, 256 * 1024)?.stdout;
    let fields = std::str::from_utf8(&attributes)
        .map_err(|_| Error::Invalid)?
        .split_terminator('\0')
        .collect::<Vec<_>>();
    if attributes.last() != Some(&0) || fields.len() != paths.len() * 9 {
        return Ok(None);
    }
    for (path, fields) in paths.iter().zip(fields.as_chunks::<9>().0) {
        if *fields
            != [
                *path,
                "filter",
                "lfs",
                *path,
                "text",
                "unset",
                *path,
                "working-tree-encoding",
                "unspecified",
            ]
        {
            return Ok(None);
        }
    }
    let mut pointers = BTreeMap::new();
    for batch in selected.chunks(64) {
        read_pointers(
            git,
            root,
            &batch.iter().map(|e| e.index_object.clone()).collect(),
            &mut pointers,
            cancel,
        )?;
    }
    let mut changed = Vec::new();
    for entry in selected {
        let Some(Some(pointer)) = pointers.get(&entry.index_object) else {
            return Ok(None);
        };
        if pointer.size < 1024 {
            return Ok(None);
        }
        match matches_content(root, &entry.path, pointer, cancel) {
            Some(true) => {}
            Some(false) => changed.push(entry.clone()),
            None => return Ok(None),
        }
    }
    Ok(Some(changed))
}
fn matches_content(
    root: &Path,
    relative: &str,
    pointer: &Pointer,
    cancel: &Cancellation,
) -> Option<bool> {
    fn read(
        root: &Path,
        relative: &str,
        pointer: &Pointer,
        cancel: &Cancellation,
    ) -> std::io::Result<bool> {
        let file = path(root, relative).map_err(std::io::Error::other)?;
        let mut ancestor = file.as_path();
        while ancestor != root {
            if fs::symlink_metadata(ancestor)?.file_attributes() & 0x400 != 0 {
                return Err(std::io::ErrorKind::InvalidData.into());
            }
            ancestor = ancestor.parent().ok_or(std::io::ErrorKind::InvalidInput)?;
        }
        let mut stream = OpenOptions::new().read(true).share_mode(1).open(&file)?;
        let before = stream.metadata()?;
        if !before.is_file() || before.len() != pointer.size {
            return Err(std::io::ErrorKind::InvalidData.into());
        }
        let mut digest = Sha256::new();
        let mut buffer = [0u8; 65536];
        loop {
            if cancel.is_cancelled() {
                return Ok(false);
            }
            let count = stream.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            digest.update(&buffer[..count]);
        }
        let after = stream.metadata()?;
        Ok(before.len() == after.len()
            && before.last_write_time() == after.last_write_time()
            && format!("{:x}", digest.finalize()) == pointer.oid)
    }
    read(root, relative, pointer, cancel).ok()
}
