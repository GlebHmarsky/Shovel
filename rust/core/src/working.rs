//! Working-copy reads and native index/commit mutations with captured-state
//! validation. All filesystem/Git calls belong on a repository worker.
use crate::{
    git::{self, Git, Snapshot},
    identity,
    process::{self, Cancellation, Lifetime, OutputCallback},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    ffi::OsString,
    fs,
    io::Read,
    os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
use windows_sys::Win32::{Foundation::*, System::Threading::*};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Group {
    Conflicted,
    Staged,
    Unstaged,
    Untracked,
    Ignored,
}
impl Group {
    pub fn can_stage(self) -> bool {
        matches!(self, Self::Unstaged | Self::Untracked)
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Change {
    pub group: Group,
    pub path: String,
    pub original: Option<String>,
    pub code: char,
    pub submodule: String,
    pub head_object: String,
    pub index_object: String,
    pub head_mode: String,
    pub index_mode: String,
    pub worktree_mode: String,
}
#[derive(Clone, Debug)]
pub struct Context {
    pub repository: Snapshot,
    /// Working-copy status and targeted reconciliation both order rows by (group, path).
    pub entries: Vec<Change>,
    pub index_stamp: String,
    pub config_stamp: String,
    pub head_message: String,
    pub merge_message: String,
    pub(crate) filters: Vec<OsString>,
    pub(crate) standard_lfs: bool,
}
impl Context {
    pub fn group(&self, group: Group) -> &[Change] {
        let start = self.entries.partition_point(|entry| entry.group < group);
        let rest = &self.entries[start..];
        &rest[..rest.partition_point(|entry| entry.group == group)]
    }

    pub fn change(&self, group: Group, path: &str) -> Option<&Change> {
        let rows = self.group(group);
        rows.binary_search_by(|entry| entry.path.as_str().cmp(path))
            .ok()
            .map(|index| &rows[index])
    }
}
#[derive(Clone, Debug)]
pub enum Action {
    Stage(Vec<Change>),
    Unstage(Vec<Change>),
    Commit {
        message: String,
        amend: bool,
    },
    Hunks {
        preview: std::sync::Arc<crate::diff::HunkPreview>,
        ids: Vec<usize>,
    },
}
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("CommitStale")]
    Stale,
    #[error("CommitSelectionGone")]
    SelectionGone,
    #[error("CommitInvalidPath")]
    InvalidPath,
    #[error("CommitMessageRequired")]
    Message,
    #[error("CommitUnresolved")]
    Unresolved,
    #[error("ActionSequence")]
    Sequence,
    #[error("ActionOtherInstance")]
    OtherInstance,
    #[error("GitErrorInvalidOutput")]
    Invalid,
    #[error("GitErrorOutputLimit")]
    Limit,
    #[error("HunkStale")]
    HunkStale,
    #[error("HunkSelectChange")]
    HunkSelection,
    #[error("HunkUnsupportedPath")]
    HunkPath,
    #[error("ActionReadFailed")]
    Io(#[from] std::io::Error),
    #[error("ActionReadFailed")]
    Git(#[from] git::Error),
}
pub struct Completed {
    pub command: process::Result,
    pub commit: Option<String>,
    pub updated: Result<Context, Error>,
    /// A verified path-limited result is usable while the editor refreshes other rows.
    pub needs_full_scan: bool,
    pub validation_elapsed: Duration,
    pub reconciliation_elapsed: Duration,
}
impl Completed {
    pub fn succeeded(&self) -> bool {
        self.command.command_succeeded()
    }
}
fn ordinary_modification(entry: &Change) -> bool {
    entry.code == 'M'
        && entry.original.is_none()
        && entry.submodule == "N..."
        && matches!(entry.group, Group::Staged | Group::Unstaged)
}
fn index_targets<'a>(context: &Context, action: &'a Action) -> BTreeSet<&'a str> {
    let (Action::Stage(entries) | Action::Unstage(entries)) = action else {
        return BTreeSet::new();
    };
    // status takes pathspecs on the Windows command line. Wide selections and
    // rename/copy partners use the complete native status/reconciliation path.
    if entries.iter().any(|entry| !ordinary_modification(entry))
        || entries.iter().map(|e| e.path.len() + 3).sum::<usize>() > 12000
    {
        return BTreeSet::new();
    }
    let selected: BTreeSet<_> = entries.iter().map(|e| e.path.as_str()).collect();
    if context.entries.iter().any(|entry| {
        entry
            .original
            .as_deref()
            .is_some_and(|path| selected.contains(path))
            || selected.contains(entry.path.as_str()) && !ordinary_modification(entry)
    }) {
        BTreeSet::new()
    } else {
        selected
    }
}

// Git prints the object it created after running post-commit. HEAD may already
// point elsewhere by then. An incomplete or unrecognized summary has no safe ID.
fn created_commit(command: &process::Result) -> Option<String> {
    if !command.success() {
        return None;
    }
    let summary = command
        .stdout
        .split(|&b| b == b'\n')
        .rev()
        .find(|line| line.starts_with(b"["))?;
    let object = summary
        .split(|b| b.is_ascii_whitespace())
        .skip(1)
        .find_map(|word| word.strip_suffix(b"]"))?;
    (matches!(object.len(), 40 | 64) && object.iter().all(u8::is_ascii_hexdigit))
        .then(|| String::from_utf8_lossy(object).into_owned())
}
/// A Windows mutex must be released by the thread that acquired it.
/// Keep the guard on that worker through validation and mutation.
///
/// ```compile_fail
/// use shovel_core::working::MutationGate;
/// let gate = MutationGate::acquire(std::path::Path::new(r"C:\fixture\.git"), false).unwrap();
/// std::thread::spawn(move || drop(gate));
/// ```
pub struct MutationGate {
    _global: HeldMutex,
    _session: HeldMutex,
    _thread: std::marker::PhantomData<std::rc::Rc<()>>,
}
impl MutationGate {
    pub fn acquire(common: &Path, fetch: bool) -> Result<Self, Error> {
        Self::acquire_named(common, if fetch { "Fetch" } else { "GitAction" })
    }
    pub(crate) fn acquire_named(common: &Path, scope: &str) -> Result<Self, Error> {
        let key = format!(
            "{:X}",
            Sha256::digest(
                crate::case::invariant_upper(&common.to_string_lossy().replace('/', "\\"))?
                    .as_bytes()
            )
        );
        let name = format!("Shovel.{scope}.{key}");
        // Keep the C# lock in the current session. Rust instances also use
        // the system-wide namespace to coordinate across Windows sessions.
        let session = HeldMutex::acquire(&format!("Local\\{name}"))?;
        let global = HeldMutex::acquire(&format!("Global\\{name}"))?;
        Ok(Self {
            _global: global,
            _session: session,
            _thread: std::marker::PhantomData,
        })
    }
}

struct HeldMutex(OwnedHandle);
impl HeldMutex {
    fn acquire(name: &str) -> Result<Self, Error> {
        let wide = name.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
        let raw = unsafe {
            CreateMutexExW(
                std::ptr::null(),
                wide.as_ptr(),
                0,
                SYNCHRONIZATION_SYNCHRONIZE | MUTEX_MODIFY_STATE,
            )
        };
        if raw.is_null() {
            return Err(std::io::Error::last_os_error().into());
        }
        let handle = unsafe { OwnedHandle::from_raw_handle(raw) };
        match unsafe { WaitForSingleObject(handle.as_raw_handle(), 0) } {
            WAIT_OBJECT_0 | WAIT_ABANDONED => Ok(Self(handle)),
            WAIT_TIMEOUT => Err(Error::OtherInstance),
            _ => Err(std::io::Error::last_os_error().into()),
        }
    }
}
impl Drop for HeldMutex {
    fn drop(&mut self) {
        unsafe { ReleaseMutex(self.0.as_raw_handle()) };
    }
}
/// Each record from Git's scoped, NUL-delimited config list contains scope,
/// origin, and key/value. Callers that need only values can skip the first two.
pub(crate) fn config_values(raw: &[u8]) -> Result<impl Iterator<Item = &[u8]>, Error> {
    let fields = if raw.is_empty() {
        Vec::new()
    } else {
        raw.strip_suffix(b"\0")
            .ok_or(Error::Invalid)?
            .split(|byte| *byte == 0)
            .collect::<Vec<_>>()
    };
    if !fields.len().is_multiple_of(3) {
        return Err(Error::Invalid);
    }
    Ok(fields.into_iter().skip(2).step_by(3))
}

fn hash(bytes: &[u8]) -> String {
    format!("{:X}", Sha256::digest(bytes))
}
pub fn path(root: &Path, relative: &str) -> Result<PathBuf, Error> {
    if relative.is_empty()
        || relative.contains('\0')
        || Path::new(relative).is_absolute()
        || relative.contains(':')
        || relative.starts_with(['\\', '/'])
        || relative
            .split(['\\', '/'])
            .any(|p| p == ".." || p.eq_ignore_ascii_case(".git"))
    {
        return Err(Error::InvalidPath);
    }
    Ok(root.join(relative))
}
pub fn parse_status(bytes: &[u8]) -> Result<Vec<Change>, Error> {
    if bytes.len() > 4 * 1024 * 1024 {
        return Err(Error::Limit);
    }
    let text = std::str::from_utf8(bytes).map_err(|_| Error::Invalid)?;
    if !text.is_empty() && !text.ends_with('\0') {
        return Err(Error::Invalid);
    }
    let records = text.split('\0').collect::<Vec<_>>();
    let mut entries = Vec::new();
    let mut i = 0;
    while i + 1 < records.len() {
        let record = records[i];
        i += 1;
        if record.starts_with("? ") || record.starts_with("! ") {
            entries.push(Change {
                group: if record.starts_with('?') {
                    Group::Untracked
                } else {
                    Group::Ignored
                },
                path: record[2..].into(),
                original: None,
                code: record.chars().next().unwrap(),
                submodule: "N...".into(),
                head_object: String::new(),
                index_object: String::new(),
                head_mode: String::new(),
                index_mode: String::new(),
                worktree_mode: String::new(),
            });
        } else {
            let count = if record.starts_with("1 ") {
                9
            } else if record.starts_with("2 ") {
                10
            } else if record.starts_with("u ") {
                11
            } else {
                return Err(Error::Invalid);
            };
            let fields = record.splitn(count, ' ').collect::<Vec<_>>();
            if fields.len() != count
                || fields[1].len() != 2
                || fields[2].len() != 4
                || !fields[1].chars().all(|c| ".MADRCUT".contains(c))
            {
                return Err(Error::Invalid);
            }
            let original = if count == 10 {
                if i + 1 >= records.len() || records[i].is_empty() {
                    return Err(Error::Invalid);
                }
                let value = Some(records[i].into());
                i += 1;
                value
            } else {
                None
            };
            let entry = |group, code| Change {
                group,
                path: fields[count - 1].into(),
                original: original.clone(),
                code,
                submodule: fields[2].into(),
                head_object: fields[6].into(),
                index_object: fields[7].into(),
                head_mode: fields[3].into(),
                index_mode: fields[4].into(),
                worktree_mode: fields[5].into(),
            };
            if count == 11 {
                let mut conflict = entry(Group::Conflicted, 'U');
                conflict.head_object.clear();
                conflict.index_object.clear();
                conflict.head_mode.clear();
                conflict.index_mode.clear();
                conflict.worktree_mode.clear();
                entries.push(conflict);
            } else {
                let xy = fields[1].as_bytes();
                if xy[0] != b'.' {
                    entries.push(entry(Group::Staged, xy[0] as char));
                }
                if xy[1] != b'.' {
                    entries.push(entry(Group::Unstaged, xy[1] as char));
                }
            }
        }
        if entries.len() > 20000 {
            return Err(Error::Limit);
        }
        if entries.last().is_some_and(|e| e.path.is_empty()) {
            return Err(Error::Invalid);
        }
    }
    entries.sort_by(|a, b| a.group.cmp(&b.group).then(a.path.cmp(&b.path)));
    Ok(entries)
}

struct BranchStatus<'a> {
    head: &'a [u8],
    branch: &'a [u8],
    rows: &'a [u8],
}

fn status_branch(mut bytes: &[u8]) -> Result<BranchStatus<'_>, Error> {
    let mut head = None;
    let mut branch = None;
    while bytes.starts_with(b"# ") {
        let end = bytes
            .iter()
            .position(|byte| *byte == 0)
            .ok_or(Error::Invalid)?;
        let header = &bytes[..end];
        if let Some(value) = header.strip_prefix(b"# branch.oid ") {
            head = Some(value);
        } else if let Some(value) = header.strip_prefix(b"# branch.head ") {
            branch = Some(value);
        }
        bytes = &bytes[end + 1..];
    }
    Ok(BranchStatus {
        head: head.ok_or(Error::Invalid)?,
        branch: branch.ok_or(Error::Invalid)?,
        rows: bytes,
    })
}

fn filter_overrides<'a>(
    records: impl Iterator<Item = &'a [u8]>,
) -> Result<(Vec<OsString>, bool), Error> {
    let mut args = Vec::new();
    let mut lfs_process = false;
    let mut lfs_clean = false;
    let mut size = 0;
    for record in records {
        let key = record.split(|byte| *byte == b'\n').next().unwrap();
        let Some(driver_key) = key.strip_prefix(b"filter.") else {
            continue;
        };
        if ![b".clean".as_slice(), b".process", b".required"]
            .iter()
            .any(|suffix| driver_key.ends_with(suffix))
        {
            continue;
        }
        size += record.len() + 1;
        if size > 256 * 1024 {
            return Err(Error::Limit);
        }
        let item = std::str::from_utf8(record).map_err(|_| Error::Invalid)?;
        let (key, value) = item.split_once('\n').unwrap_or((item, "true"));
        if key.eq_ignore_ascii_case("filter.lfs.process") {
            lfs_process = value == "git-lfs filter-process";
        }
        if key.eq_ignore_ascii_case("filter.lfs.clean") {
            lfs_clean = value == "git-lfs clean -- %f";
        }
        args.push("-c".into());
        let value = if key.ends_with(".required") {
            "false"
        } else {
            ""
        };
        args.push(format!("{key}={value}").into());
    }
    Ok((args, lfs_process && lfs_clean))
}

impl Git {
    pub fn semantic_index(
        &self,
        repository: &Snapshot,
        cancel: &Cancellation,
    ) -> Result<String, Error> {
        Ok(self.index_stamps(repository, &BTreeSet::new(), cancel)?.0)
    }
    fn index_stamps(
        &self,
        repository: &Snapshot,
        targets: &BTreeSet<&str>,
        cancel: &Cancellation,
    ) -> Result<(String, Option<String>), Error> {
        let output = self
            .read(
                &repository.worktree,
                ["ls-files", "--stage", "--debug", "-z"],
                cancel,
                64 * 1024 * 1024,
            )?
            .stdout;
        let mut digest = Sha256::new();
        let mut outside = (!targets.is_empty()).then(Sha256::new);
        let mut offset = 0;
        while offset < output.len() {
            let end = output[offset..]
                .iter()
                .position(|b| *b == 0)
                .ok_or(Error::Invalid)?
                + offset;
            let record = &output[offset..=end];
            digest.update(record);
            offset = end + 1;
            let mut last = "";
            for prefix in ["  ctime: ", "  mtime: ", "  dev: ", "  uid: ", "  size: "] {
                let end = output[offset..]
                    .iter()
                    .position(|b| *b == b'\n')
                    .ok_or(Error::Invalid)?
                    + offset;
                last = std::str::from_utf8(&output[offset..end])
                    .map_err(|_| Error::Invalid)?
                    .trim_end_matches('\r');
                offset = end + 1;
                if !last.starts_with(prefix) {
                    return Err(Error::Invalid);
                }
            }
            let flags =
                u32::from_str_radix(last.split_once("flags: ").ok_or(Error::Invalid)?.1, 16)
                    .map_err(|_| Error::Invalid)?;
            let semantic_flags = (flags & 0x60008000).to_le_bytes();
            digest.update(semantic_flags);
            if let Some(outside) = &mut outside {
                let tab = record
                    .iter()
                    .position(|b| *b == b'\t')
                    .ok_or(Error::Invalid)?;
                let path = std::str::from_utf8(&record[tab + 1..record.len() - 1]);
                if !path.is_ok_and(|p| targets.contains(p)) {
                    outside.update(record);
                    outside.update(semantic_flags);
                }
            }
        }
        Ok((
            format!("{:X}", digest.finalize()),
            outside.map(|hash| format!("{:X}", hash.finalize())),
        ))
    }
    fn working_filters(
        &self,
        root: &Path,
        cancel: &Cancellation,
    ) -> Result<(Vec<OsString>, bool), Error> {
        let result = process::run(
            &self.request(
                root,
                [
                    "config",
                    "--null",
                    "--get-regexp",
                    r"^filter\..*\.(clean|process|required)$",
                ],
                Lifetime::Read,
                256 * 1024,
            ),
            cancel,
            None,
        )?;
        if result.outcome != process::Outcome::Exited || result.exit_code > 1 {
            git::require(result)?;
            return Err(Error::Invalid);
        }
        filter_overrides(result.stdout.split(|byte| *byte == 0))
    }
    pub fn config_stamp(&self, root: &Path, cancel: &Cancellation) -> Result<String, Error> {
        let (stamp, _) = self.stamped_config(root, cancel)?;
        Ok(stamp)
    }
    pub(crate) fn stamped_config(
        &self,
        root: &Path,
        cancel: &Cancellation,
    ) -> Result<(String, Vec<u8>), Error> {
        let raw = self
            .read(
                root,
                [
                    "config",
                    "--null",
                    "--list",
                    "--show-origin",
                    "--show-scope",
                ],
                cancel,
                4 * 1024 * 1024,
            )?
            .stdout;
        Ok((hash(&raw), raw))
    }
    fn working_configuration(
        &self,
        root: &Path,
        cancel: &Cancellation,
    ) -> Result<(String, Vec<OsString>, bool), Error> {
        let (stamp, raw) = self.stamped_config(root, cancel)?;
        let (filters, standard_lfs) = filter_overrides(config_values(&raw)?)?;
        Ok((stamp, filters, standard_lfs))
    }
    fn read_worktree_status(
        &self,
        root: &Path,
        filters: &[OsString],
        full: bool,
        paths: &BTreeSet<&str>,
        cancel: &Cancellation,
    ) -> Result<Vec<Change>, Error> {
        parse_status(&self.read_status_bytes(root, filters, full, paths, false, cancel)?)
    }
    fn read_status_bytes(
        &self,
        root: &Path,
        filters: &[OsString],
        full: bool,
        paths: &BTreeSet<&str>,
        branch_header: bool,
        cancel: &Cancellation,
    ) -> Result<Vec<u8>, Error> {
        let mut args = filters.to_vec();
        args.extend(
            [
                "--literal-pathspecs",
                "-c",
                "status.submoduleSummary=false",
                "status",
                "--porcelain=v2",
                "-z",
                if full {
                    "--untracked-files=all"
                } else {
                    "--untracked-files=normal"
                },
                "--ignore-submodules=dirty",
                if full { "--renames" } else { "--no-renames" },
                "--ignored=no",
            ]
            .map(OsString::from),
        );
        if branch_header {
            args.extend(["--branch", "--no-ahead-behind"].map(OsString::from));
        }
        args.push("--".into());
        args.extend(paths.iter().map(OsString::from));
        Ok(self.read(root, args, cancel, 4 * 1024 * 1024)?.stdout)
    }

    pub fn has_local_changes(
        &self,
        root: &Path,
        expected: Option<&str>,
        cancel: &Cancellation,
    ) -> Result<bool, Error> {
        Ok(!self
            .local_changes(root, expected, false, cancel)?
            .is_empty())
    }

    pub fn local_change_rows(
        &self,
        root: &Path,
        expected: &str,
        cancel: &Cancellation,
    ) -> Result<Vec<Change>, Error> {
        self.local_changes(root, Some(expected), true, cancel)
    }

    fn local_changes(
        &self,
        root: &Path,
        expected: Option<&str>,
        full: bool,
        cancel: &Cancellation,
    ) -> Result<Vec<Change>, Error> {
        let before = identity::read(root)?;
        if expected.is_some_and(|id| id != before.fingerprint) {
            return Err(Error::Stale);
        }
        let (filters, standard_lfs) = self.working_filters(root, cancel)?;
        let mut entries =
            self.read_worktree_status(root, &filters, full, &BTreeSet::new(), cancel)?;
        if standard_lfs
            && (full
                || entries
                    .iter()
                    .all(|entry| entry.group == Group::Unstaged && entry.code == 'M'))
        {
            crate::lfs::normalize(self, root, &mut entries, cancel)?;
        }
        if identity::read(root)? != before || cancel.is_cancelled() {
            return Err(Error::Stale);
        }
        Ok(entries)
    }

    pub fn working_copy(&self, root: &Path, cancel: &Cancellation) -> Result<Context, Error> {
        let directories = self.inspect_directories(root, cancel)?;
        // Both halves use the bounded read lane. Keep repository discovery
        // ahead of the scan, and only combine status with the branch it saw.
        let (
            (repository, head_message),
            (status_head, status_branch, entries, index_stamp, config_stamp, filters, standard_lfs),
        ) = std::thread::scope(|scope| {
            let metadata = std::thread::Builder::new().spawn_scoped(scope, || {
                let repository = self.inspect_metadata(root, directories.clone(), cancel)?;
                let head_message = self.working_head_message(root, &repository, cancel)?;
                Ok::<_, Error>((repository, head_message))
            })?;
            let working = (|| {
                let index_stamp = self.semantic_index(&directories, cancel)?;
                let (config_stamp, filters, standard_lfs) =
                    self.working_configuration(root, cancel)?;
                if identity::read(root)? != directories.identity {
                    return Err(Error::Stale);
                }
                let status =
                    self.read_status_bytes(root, &filters, true, &BTreeSet::new(), true, cancel)?;
                let status = status_branch(&status)?;
                let mut entries = parse_status(status.rows)?;
                if standard_lfs {
                    crate::lfs::normalize(self, root, &mut entries, cancel)?;
                }
                Ok((
                    status.head.to_vec(),
                    status.branch.to_vec(),
                    entries,
                    index_stamp,
                    config_stamp,
                    filters,
                    standard_lfs,
                ))
            })();
            let metadata = metadata
                .join()
                .map_err(|_| std::io::Error::other("repository metadata worker stopped"));
            Ok::<_, Error>((metadata??, working?))
        })?;
        if status_head != repository.head.as_deref().unwrap_or("(initial)").as_bytes()
            || status_branch
                != repository
                    .branch
                    .as_deref()
                    .unwrap_or("(detached)")
                    .as_bytes()
        {
            return Err(Error::Stale);
        }
        let merge_message = if repository.operations == ["merge"] {
            let file = repository.git_directory.join("MERGE_MSG");
            match fs::File::open(file) {
                Ok(file) => {
                    let mut bytes = Vec::new();
                    file.take(65537).read_to_end(&mut bytes)?;
                    if bytes.len() > 65536 {
                        return Err(Error::Limit);
                    }
                    String::from_utf8(bytes).map_err(|_| Error::Invalid)?
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
                Err(e) => return Err(e.into()),
            }
        } else {
            String::new()
        };
        if index_stamp != self.semantic_index(&repository, cancel)?
            || identity::read(root)? != repository.identity
            || git::operation_markers(&repository.git_directory) != repository.operations
        {
            return Err(Error::Stale);
        }
        Ok(Context {
            repository,
            entries,
            index_stamp,
            config_stamp,
            head_message,
            merge_message,
            filters,
            standard_lfs,
        })
    }
    fn working_head_message(
        &self,
        root: &Path,
        repository: &Snapshot,
        cancel: &Cancellation,
    ) -> Result<String, Error> {
        let Some(head) = &repository.head else {
            return Ok(String::new());
        };
        let output = self.read(
            root,
            [
                "log",
                "-1",
                "--no-show-signature",
                "--no-notes",
                "--format=%B",
                head,
                "--",
            ],
            cancel,
            65536,
        )?;
        Ok(String::from_utf8_lossy(&output.stdout)
            .trim_end_matches(['\r', '\n'])
            .to_owned())
    }
    pub fn execute_working(
        &self,
        context: &Context,
        action: &Action,
        output: Option<OutputCallback>,
    ) -> Result<Completed, Error> {
        self.execute_working_inner(context, action, output, false)
    }
    pub fn refresh_working_copy(
        &self,
        reviewed: &Snapshot,
        cancel: &Cancellation,
    ) -> Result<Context, Error> {
        let updated = self.working_copy(&reviewed.worktree, cancel)?;
        if updated.repository.identity != reviewed.identity {
            return Err(Error::Stale);
        }
        Ok(updated)
    }
    pub fn execute_working_for_editor(
        &self,
        context: &Context,
        action: &Action,
        output: Option<OutputCallback>,
    ) -> Result<Completed, Error> {
        let result = self.execute_working_inner(context, action, output.clone(), true);
        let (Action::Stage(entries) | Action::Unstage(entries)) = action else {
            return result;
        };
        if !matches!(result, Err(Error::Stale)) {
            return result;
        }
        let fresh = self.refresh_working_copy(&context.repository, &Cancellation::default())?;
        if fresh.repository.head != context.repository.head
            || fresh.repository.branch != context.repository.branch
            || fresh.repository.operations != context.repository.operations
            || fresh.config_stamp != context.config_stamp
        {
            return result;
        }
        // Carry only the original reviewed rows forward. A fresh scan may also
        // contain new work, a newly conflicted path, or a changed staged blob.
        let same = entries.iter().filter(|entry| fresh.change(entry.group, &entry.path) == Some(entry)).cloned().collect::<Vec<_>>();
        if same.is_empty() {
            return Err(Error::SelectionGone);
        }
        let refreshed = if matches!(action, Action::Stage(_)) { Action::Stage(same) } else { Action::Unstage(same) };
        self.execute_working_inner(&fresh, &refreshed, output, true)
    }
    fn execute_working_inner(
        &self,
        context: &Context,
        action: &Action,
        output: Option<OutputCallback>,
        targeted: bool,
    ) -> Result<Completed, Error> {
        let started = Instant::now();
        let _gate = MutationGate::acquire(&context.repository.common_directory, false)?;
        let cancel = Cancellation::default();
        let root = &context.repository.worktree;
        let reviewed = &context.repository;
        let targets = if targeted {
            index_targets(context, action)
        } else {
            BTreeSet::new()
        };
        let (index_stamp, outside) = self.validated_index(context, &targets, &cancel)?;
        if index_stamp != context.index_stamp {
            return Err(Error::Stale);
        }
        let mut input = Vec::new();
        let args: Vec<OsString> = match action {
            Action::Hunks { preview, ids } => {
                if preview.context.repository.identity != context.repository.identity
                    || preview.context.index_stamp != context.index_stamp
                    || preview.context.repository.head != context.repository.head
                    || context.change(preview.change.group, &preview.change.path)
                        != Some(&preview.change)
                {
                    return Err(Error::HunkStale);
                }
                let current = self.file_diff(
                    context,
                    &preview.change,
                    crate::diff::Options::default(),
                    &cancel,
                )?;
                let verified = current.hunks.ok_or(Error::HunkStale)?;
                if verified.patch != preview.patch
                    || verified.content_stamp != preview.content_stamp
                    || verified.attribute_stamp != preview.attribute_stamp
                {
                    return Err(Error::HunkStale);
                }
                input = verified.select(ids)?;
                let mut args = [
                    "-c",
                    "apply.ignoreWhitespace=false",
                    "apply",
                    "--cached",
                    "--whitespace=nowarn",
                ]
                .map(OsString::from)
                .to_vec();
                if preview.staged() {
                    args.push("--reverse".into());
                }
                let mut check = args.clone();
                check.extend(["--check", "--", "-"].map(OsString::from));
                let mut request = self.request(root, check, Lifetime::Read, 1024 * 1024);
                request.input = input.clone();
                let validation_elapsed = started.elapsed();
                let checked = process::run(&request, &cancel, output.clone())?;
                if !checked.success() {
                    let reconciling = Instant::now();
                    let updated = self.refresh_working_copy(reviewed, &cancel);
                    return Ok(Completed {
                        command: checked,
                        commit: None,
                        updated,
                        needs_full_scan: false,
                        validation_elapsed,
                        reconciliation_elapsed: reconciling.elapsed(),
                    });
                }
                if !self.current_index_target(reviewed, &cancel)?
                    || self.semantic_index(reviewed, &cancel)? != context.index_stamp
                    || crate::diff::content_stamp(root, &preview.change.path, &cancel)?
                        != preview.content_stamp
                    || self.hunk_attributes(root, &preview.change.path, &cancel)?.1
                        != preview.attribute_stamp
                    || self.config_stamp(root, &cancel)? != context.config_stamp
                {
                    return Err(Error::HunkStale);
                }
                args.extend(["--", "-"].map(OsString::from));
                args
            }
            Action::Stage(entries) | Action::Unstage(entries) => {
                let unstage = matches!(action, Action::Unstage(_));
                if !unstage && entries.iter().any(|entry| {
                    entry.group == Group::Conflicted
                        || context.change(Group::Conflicted, &entry.path).is_some()
                        || (entry.code == 'R'
                            && entry.original.as_deref().is_some_and(|path| {
                                context.change(Group::Conflicted, path).is_some()
                            }))
                }) {
                    return Err(Error::Unresolved);
                }
                if entries.is_empty()
                    || entries.iter().any(|e| {
                        context.change(e.group, &e.path) != Some(e)
                            || if unstage {
                                e.group != Group::Staged
                            } else {
                                !e.group.can_stage()
                            }
                    })
                {
                    return Err(Error::Stale);
                }
                let mut targets = BTreeSet::new();
                for entry in entries {
                    let file = path(root, &entry.path)?;
                    if targeted && !unstage && entry.code != 'D'
                        && fs::symlink_metadata(file).is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound)
                    {
                        continue;
                    }
                    targets.insert(entry.path.clone());
                    if let Some(old) = &entry.original
                        && (unstage || entry.code == 'R')
                    {
                        path(root, old)?;
                        targets.insert(old.clone());
                    }
                }
                if targets.is_empty() {
                    return Err(Error::SelectionGone);
                }
                for target in targets {
                    input.extend_from_slice(target.as_bytes());
                    input.push(0);
                }
                let mut args = if !unstage {
                    vec!["add".into(), "--all".into()]
                } else if let Some(head) = &reviewed.head {
                    vec![
                        "reset".into(),
                        "--quiet".into(),
                        "--no-refresh".into(),
                        head.into(),
                    ]
                } else {
                    ["rm", "--cached", "--ignore-unmatch", "-r", "-f"]
                        .map(OsString::from)
                        .to_vec()
                };
                args.extend(["--pathspec-from-file=-", "--pathspec-file-nul"].map(OsString::from));
                args
            }
            Action::Commit { message, amend } => {
                if message.trim().is_empty() || message.len() > 65536 || message.contains('\0') {
                    return Err(Error::Message);
                }
                if !reviewed.operations.is_empty() && reviewed.operations != ["merge"]
                    || *amend && (reviewed.head.is_none() || !reviewed.operations.is_empty())
                {
                    return Err(Error::Sequence);
                }
                if !self
                    .read(
                        root,
                        ["ls-files", "--unmerged", "-z"],
                        &cancel,
                        4 * 1024 * 1024,
                    )?
                    .stdout
                    .is_empty()
                {
                    return Err(Error::Unresolved);
                }
                input = message.as_bytes().to_vec();
                let mut args = [
                    "-c",
                    "i18n.commitEncoding=UTF-8",
                    "-c",
                    "core.abbrev=no",
                    "-c",
                    "color.ui=false",
                    "commit",
                    "--cleanup=strip",
                    "--file=-",
                ]
                .map(OsString::from)
                .to_vec();
                if *amend {
                    args.push("--amend".into());
                }
                args
            }
        };
        let mut args_with_literal = vec!["--literal-pathspecs".into()];
        args_with_literal.extend(args);
        let mut request = self.request(
            root,
            args_with_literal,
            Lifetime::AcceptedCommand,
            1024 * 1024,
        );
        request.input = input;
        request
            .environment
            .insert("GIT_EDITOR".into(), Some("false".into()));
        let validation_elapsed = started.elapsed();
        let command = process::run(&request, &cancel, output)?;
        let reconciling = Instant::now();
        let commit = if matches!(action, Action::Commit { .. }) {
            created_commit(&command)
        } else {
            None
        };
        // Reconciliation failure never changes a successful command into retryable mutation.
        let partial = if let Some(outside) = outside
            && command.command_succeeded()
        {
            self.reconcile_index_paths(context, &targets, &outside, &cancel)
                .ok()
                .flatten()
        } else {
            None
        };
        let needs_full_scan = partial.is_some();
        let updated = match partial {
            Some(partial) => Ok(partial),
            None => self.refresh_working_copy(reviewed, &cancel),
        };
        Ok(Completed {
            command,
            commit,
            updated,
            needs_full_scan,
            validation_elapsed,
            reconciliation_elapsed: reconciling.elapsed(),
        })
    }
    fn reconcile_index_paths(
        &self,
        context: &Context,
        targets: &BTreeSet<&str>,
        prior_outside: &str,
        cancel: &Cancellation,
    ) -> Result<Option<Context>, Error> {
        let reviewed = &context.repository;
        let root = &reviewed.worktree;
        let (after, outside) = self.index_stamps(reviewed, targets, cancel)?;
        // A hook can stage another file. Only a full read may adopt that index.
        if outside.as_deref() != Some(prior_outside) {
            return Ok(None);
        }
        let mut actual =
            self.read_worktree_status(root, &context.filters, true, targets, cancel)?;
        if context.standard_lfs {
            crate::lfs::normalize(self, root, &mut actual, cancel)?;
        }
        if actual
            .iter()
            .any(|entry| !ordinary_modification(entry) || !targets.contains(entry.path.as_str()))
            || self.validated_index(context, &BTreeSet::new(), cancel)?.0 != after
        {
            return Ok(None);
        }
        let mut updated = context.clone();
        updated
            .entries
            .retain(|entry| !targets.contains(entry.path.as_str()));
        updated.entries.extend(actual);
        updated
            .entries
            .sort_by(|a, b| a.group.cmp(&b.group).then(a.path.cmp(&b.path)));
        updated.index_stamp = after;
        Ok(Some(updated))
    }
    fn validated_index(
        &self,
        context: &Context,
        targets: &BTreeSet<&str>,
        cancel: &Cancellation,
    ) -> Result<(String, Option<String>), Error> {
        let reviewed = &context.repository;
        let root = &reviewed.worktree;
        if identity::read(root)? != reviewed.identity {
            return Err(Error::Stale);
        }
        // These reads are independent. Both use normal process admission, and
        // the mutation gate stays on the owning thread until they have joined.
        std::thread::scope(|scope| {
            let head = std::thread::Builder::new()
                .spawn_scoped(scope, || self.current_index_target(reviewed, cancel))?;
            let index = self
                .index_stamps(reviewed, targets, cancel)
                .and_then(|index| {
                    if self.config_stamp(root, cancel)? == context.config_stamp {
                        Ok(index)
                    } else {
                        Err(Error::Stale)
                    }
                });
            let matches = head
                .join()
                .map_err(|_| std::io::Error::other("repository validation worker stopped"))??;
            if !matches || identity::read(root)? != reviewed.identity {
                return Err(Error::Stale);
            }
            index
        })
    }
    pub fn working_diff(
        &self,
        context: &Context,
        entry: &Change,
        cancel: &Cancellation,
    ) -> Result<String, Error> {
        Ok(self
            .file_diff(context, entry, crate::diff::Options::default(), cancel)?
            .patch)
    }
}
