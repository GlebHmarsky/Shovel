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
    pub entries: Vec<Change>,
    pub index_stamp: String,
    pub config_stamp: String,
    pub head_message: String,
    pub merge_message: String,
    pub(crate) filters: Vec<OsString>,
    pub(crate) standard_lfs: bool,
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
}
impl Completed {
    pub fn succeeded(&self) -> bool {
        command_succeeded(&self.command)
    }
}
fn command_succeeded(result: &process::Result) -> bool {
    result.exit_code == 0
        && matches!(
            result.outcome,
            process::Outcome::Exited | process::Outcome::OutputLimit
        )
}
pub struct MutationGate {
    handle: OwnedHandle,
}
impl MutationGate {
    pub fn acquire(common: &Path, fetch: bool) -> Result<Self, Error> {
        let key = format!(
            "{:X}",
            Sha256::digest(
                common
                    .to_string_lossy()
                    .replace('/', "\\")
                    .to_uppercase()
                    .as_bytes()
            )
        );
        // Share the baseline's native lock to coordinate C# and Rust profiles.
        let name = format!(
            "Local\\Shovel.{}.{key}",
            if fetch { "Fetch" } else { "GitAction" }
        );
        let wide = name.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
        let raw = unsafe { CreateMutexW(std::ptr::null(), 0, wide.as_ptr()) };
        if raw.is_null() {
            return Err(std::io::Error::last_os_error().into());
        }
        let handle = unsafe { OwnedHandle::from_raw_handle(raw) };
        match unsafe { WaitForSingleObject(handle.as_raw_handle(), 0) } {
            WAIT_OBJECT_0 | WAIT_ABANDONED => Ok(Self { handle }),
            WAIT_TIMEOUT => Err(Error::OtherInstance),
            _ => Err(std::io::Error::last_os_error().into()),
        }
    }
}
impl Drop for MutationGate {
    fn drop(&mut self) {
        unsafe { ReleaseMutex(self.handle.as_raw_handle()) };
    }
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

impl Git {
    pub fn semantic_index(
        &self,
        repository: &Snapshot,
        cancel: &Cancellation,
    ) -> Result<String, Error> {
        let output = self
            .read(
                &repository.worktree,
                ["ls-files", "--stage", "--debug", "-z"],
                cancel,
                64 * 1024 * 1024,
            )?
            .stdout;
        let mut digest = Sha256::new();
        let mut offset = 0;
        while offset < output.len() {
            let end = output[offset..]
                .iter()
                .position(|b| *b == 0)
                .ok_or(Error::Invalid)?
                + offset;
            digest.update(&output[offset..=end]);
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
            digest.update((flags & 0x60008000).to_le_bytes());
        }
        Ok(format!("{:X}", digest.finalize()))
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
        let mut args = Vec::new();
        let mut lfs_process = false;
        let mut lfs_clean = false;
        for item in std::str::from_utf8(&result.stdout)
            .map_err(|_| Error::Invalid)?
            .split('\0')
            .filter(|s| !s.is_empty())
        {
            let (key, value) = item.split_once('\n').ok_or(Error::Invalid)?;
            if !key.starts_with("filter.")
                || ![".clean", ".process", ".required"]
                    .iter()
                    .any(|suffix| key.ends_with(suffix))
            {
                return Err(Error::Invalid);
            }
            if key.eq_ignore_ascii_case("filter.lfs.process") {
                lfs_process = value == "git-lfs filter-process";
            }
            if key.eq_ignore_ascii_case("filter.lfs.clean") {
                lfs_clean = value == "git-lfs clean -- %f";
            }
            args.push("-c".into());
            args.push(
                format!(
                    "{key}={}",
                    if key.ends_with(".required") {
                        "false"
                    } else {
                        ""
                    }
                )
                .into(),
            );
        }
        Ok((args, lfs_process && lfs_clean))
    }
    pub fn config_stamp(&self, root: &Path, cancel: &Cancellation) -> Result<String, Error> {
        Ok(hash(
            &self
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
                .stdout,
        ))
    }
    pub fn working_copy(&self, root: &Path, cancel: &Cancellation) -> Result<Context, Error> {
        let repository = self.inspect(root, cancel)?;
        let index_stamp = self.semantic_index(&repository, cancel)?;
        let config_stamp = self.config_stamp(root, cancel)?;
        let (filters, standard_lfs) = self.working_filters(root, cancel)?;
        let mut args = filters.clone();
        args.extend(
            [
                "--literal-pathspecs",
                "-c",
                "status.submoduleSummary=false",
                "status",
                "--porcelain=v2",
                "-z",
                "--untracked-files=all",
                "--ignore-submodules=dirty",
                "--renames",
                "--ignored=no",
                "--",
            ]
            .map(OsString::from),
        );
        let mut entries = parse_status(&self.read(root, args, cancel, 4 * 1024 * 1024)?.stdout)?;
        if standard_lfs {
            crate::lfs::normalize(self, root, &mut entries, cancel)?;
        }
        let head_message = if let Some(head) = &repository.head {
            String::from_utf8_lossy(
                &self
                    .read(
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
                    )?
                    .stdout,
            )
            .trim_end_matches(['\r', '\n'])
            .to_owned()
        } else {
            String::new()
        };
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
    pub fn execute_working(
        &self,
        context: &Context,
        action: &Action,
        output: Option<OutputCallback>,
    ) -> Result<Completed, Error> {
        let _gate = MutationGate::acquire(&context.repository.common_directory, false)?;
        let cancel = Cancellation::default();
        let root = &context.repository.worktree;
        let fresh = self.inspect(root, &cancel)?;
        if fresh.identity != context.repository.identity
            || fresh.branch != context.repository.branch
            || fresh.head != context.repository.head
            || fresh.operations != context.repository.operations
            || self.semantic_index(&fresh, &cancel)? != context.index_stamp
            || self.config_stamp(root, &cancel)? != context.config_stamp
        {
            return Err(Error::Stale);
        }
        let mut input = Vec::new();
        let args: Vec<OsString> = match action {
            Action::Hunks { preview, ids } => {
                if preview.context.repository.identity != context.repository.identity
                    || preview.context.index_stamp != context.index_stamp
                    || preview.context.repository.head != context.repository.head
                    || !context.entries.contains(&preview.change)
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
                let checked = process::run(&request, &cancel, output.clone())?;
                if !checked.success() {
                    return Ok(Completed {
                        command: checked,
                        commit: None,
                        updated: self.working_copy(root, &cancel),
                    });
                }
                let latest = self.inspect(root, &cancel)?;
                if self.semantic_index(&latest, &cancel)? != context.index_stamp
                    || crate::diff::content_stamp(root, &preview.change.path, &cancel)?
                        != preview.content_stamp
                    || self.hunk_attributes(root, &preview.change.path, &cancel)?.1
                        != preview.attribute_stamp
                    || self.config_stamp(root, &cancel)? != context.config_stamp
                    || latest.identity != fresh.identity
                    || latest.head != fresh.head
                    || latest.branch != fresh.branch
                    || latest.operations != fresh.operations
                {
                    return Err(Error::HunkStale);
                }
                args.extend(["--", "-"].map(OsString::from));
                args
            }
            Action::Stage(entries) | Action::Unstage(entries) => {
                let unstage = matches!(action, Action::Unstage(_));
                if entries.is_empty()
                    || entries.iter().any(|e| {
                        !context.entries.contains(e)
                            || if unstage {
                                e.group != Group::Staged
                            } else {
                                matches!(e.group, Group::Staged | Group::Ignored)
                            }
                    })
                {
                    return Err(Error::Stale);
                }
                let mut targets = BTreeSet::new();
                for entry in entries {
                    path(root, &entry.path)?;
                    targets.insert(entry.path.clone());
                    if let Some(old) = &entry.original
                        && (unstage || entry.code == 'R')
                    {
                        path(root, old)?;
                        targets.insert(old.clone());
                    }
                }
                for target in targets {
                    input.extend_from_slice(target.as_bytes());
                    input.push(0);
                }
                let mut args = if !unstage {
                    vec!["add".into(), "--all".into()]
                } else if let Some(head) = &fresh.head {
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
                if !fresh.operations.is_empty() && fresh.operations != ["merge"]
                    || *amend && (fresh.head.is_none() || !fresh.operations.is_empty())
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
        let command = process::run(&request, &cancel, output)?;
        let commit = if command_succeeded(&command) && matches!(action, Action::Commit { .. }) {
            self.inspect(root, &cancel).ok().and_then(|r| r.head)
        } else {
            None
        };
        // Reconciliation failure never changes a successful command into retryable mutation.
        let updated = self.working_copy(root, &cancel);
        Ok(Completed {
            command,
            commit,
            updated,
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
