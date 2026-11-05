//! Exact selected-file actions. The caller freezes and confirms the paths before
//! submission; validation and native/file writes run under the common Git lock.
use crate::{
    diff,
    git::Git,
    operation::{Command, Error, Transcript},
    process::{Cancellation, Lifetime},
    working::{self, Change, Context, Group, MutationGate},
};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    ffi::OsString,
    os::windows::ffi::OsStrExt,
    path::{Path, PathBuf},
};
/// Match the baseline's Windows File.Delete behavior: a read-only attribute or
/// denied sharing mode is a real failure, not permission to clear attributes.
pub(crate) fn remove_file(path: &Path) -> std::io::Result<()> {
    if !path.is_absolute() {
        return Err(std::io::ErrorKind::InvalidInput.into());
    }
    let mut wide = path
        .as_os_str()
        .encode_wide()
        .map(|c| if c == b'/' as u16 { b'\\' as u16 } else { c })
        .collect::<Vec<_>>();
    if !wide.starts_with(&[92, 92, 63, 92]) {
        let mut extended = vec![92, 92, 63, 92];
        extended.extend(wide);
        wide = extended;
    }
    wide.push(0);
    // The validated absolute path is owned, UTF-16 and NUL-terminated for the
    // duration of this synchronous call; DeleteFileW never recurses.
    if unsafe { windows_sys::Win32::Storage::FileSystem::DeleteFileW(wide.as_ptr()) } == 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

pub struct Completed {
    pub commands: Vec<Command>,
    pub output: String,
    pub output_truncated: bool,
    pub message: Option<String>,
    pub warning: Option<&'static str>,
    pub saved_stash: Option<String>,
    /// Filesystem-only work is reported without inventing a Git command/exit.
    pub filesystem_changes: usize,
    pub updated: Result<Context, working::Error>,
}
impl Completed {
    pub fn succeeded(&self) -> bool {
        self.message.is_none() && self.commands.iter().all(Command::succeeded)
    }
}
pub fn paths(entries: &[Change]) -> Vec<String> {
    let mut paths = BTreeSet::new();
    for entry in entries {
        paths.insert(entry.path.clone());
        if entry.code == 'R'
            && let Some(old) = &entry.original
        {
            paths.insert(old.clone());
        }
    }
    paths.into_iter().collect()
}
pub fn supported(entries: &[Change]) -> bool {
    !entries.is_empty()
        && entries.iter().all(|entry| {
            !matches!(entry.group, Group::Conflicted | Group::Ignored)
                && entry.head_mode != "160000"
                && entry.index_mode != "160000"
        })
}
pub fn discard_key(context: &Context, entries: &[Change]) -> String {
    let mut digest = Sha256::new();
    for value in [
        context.index_stamp.as_str(),
        context.config_stamp.as_str(),
        context.repository.head.as_deref().unwrap_or_default(),
    ] {
        digest.update(value.as_bytes());
        digest.update([0]);
    }
    digest.update([u8::from(
        entries.first().is_some_and(|e| e.group == Group::Staged),
    )]);
    for path in paths(entries) {
        digest.update(path.as_bytes());
        digest.update([0]);
    }
    format!("discard:{:x}", digest.finalize())
}
pub fn stash_key(context: &Context, entries: &[Change], message: &str) -> String {
    let mut digest = Sha256::new();
    digest.update(discard_key(context, entries).as_bytes());
    digest.update([0]);
    digest.update(message.as_bytes());
    format!("stash-files:{:x}", digest.finalize())
}
pub(crate) fn regular(root: &Path, relative: &str) -> Result<PathBuf, Error> {
    let path = working::path(root, relative)?;
    if diff::has_reparse(root, &path)? {
        return Err(Error::Code("FileMenuLinkedPath"));
    }
    if path.is_dir() {
        return Err(Error::Code("FileActionUnsupported"));
    }
    Ok(path)
}
pub(crate) fn input(paths: &[String]) -> Result<Vec<u8>, Error> {
    let mut input = Vec::new();
    for path in paths {
        if path.len() + 1 > (1024 * 1024usize).saturating_sub(input.len()) {
            return Err(Error::Code("FileActionSelectionLimit"));
        }
        input.extend_from_slice(path.as_bytes());
        input.push(0);
    }
    Ok(input)
}
impl Git {
    fn discard_context(
        &self,
        previous: &Context,
        entries: &[Change],
        cancel: &Cancellation,
    ) -> Result<Context, working::Error> {
        let mut fresh = self.working_copy(&previous.repository.worktree, cancel)?;
        if fresh.repository.identity == previous.repository.identity
            && fresh.repository.head == previous.repository.head
            && fresh.repository.branch == previous.repository.branch
            && fresh.index_stamp == previous.index_stamp
            && fresh.config_stamp == previous.config_stamp
            && let Some(changed) = crate::lfs::discard_changes(self, &fresh, entries, cancel)?
        {
            let paths = entries
                .iter()
                .map(|e| e.path.as_str())
                .collect::<BTreeSet<_>>();
            fresh
                .entries
                .retain(|e| e.group != Group::Unstaged || !paths.contains(e.path.as_str()));
            fresh.entries.extend(changed);
            fresh
                .entries
                .sort_by(|a, b| (&a.group, &a.path).cmp(&(&b.group, &b.path)));
        }
        Ok(fresh)
    }
    pub fn discard_files(&self, context: &Context, entries: &[Change]) -> Result<Completed, Error> {
        if entries.is_empty() || entries.iter().any(|e| !context.entries.contains(e)) {
            return Err(Error::Code("CommitStale"));
        }
        if !supported(entries) {
            return Err(Error::Code("FileActionUnsupported"));
        }
        let staged = entries[0].group == Group::Staged;
        if entries.iter().any(|e| (e.group == Group::Staged) != staged) {
            return Err(Error::Code("CommitStale"));
        }
        let _gate = MutationGate::acquire(&context.repository.common_directory, false)?;
        let root = &context.repository.worktree;
        for path in paths(entries) {
            regular(root, &path)?;
        }
        let cancel = Cancellation::default();
        // The standard-LFS status reader compares local pointer hashes rather
        // than trusting a stat-only clean result or executing observation filters.
        let fresh = self.discard_context(context, entries, &cancel)?;
        if fresh.repository.identity != context.repository.identity
            || fresh.repository.head != context.repository.head
            || fresh.repository.branch != context.repository.branch
            || fresh.repository.operations != context.repository.operations
            || fresh.repository.recovery != context.repository.recovery
            || fresh.index_stamp != context.index_stamp
            || fresh.config_stamp != context.config_stamp
            || entries.iter().any(|e| !fresh.entries.contains(e))
        {
            return Err(Error::Code("CommitStale"));
        }
        let tracked = paths(
            &entries
                .iter()
                .filter(|e| e.group != Group::Untracked)
                .cloned()
                .collect::<Vec<_>>(),
        );
        let stdin = input(&tracked)?;
        let mut log = Transcript::default();
        let mut filesystem_changes = 0;
        let result = (|| -> Result<(), Error> {
            if !tracked.is_empty() {
                let args: Vec<OsString> = if !staged {
                    ["checkout-index", "--index", "--force", "--stdin", "-z"]
                        .map(Into::into)
                        .to_vec()
                } else if let Some(head) = &fresh.repository.head {
                    vec![
                        "restore".into(),
                        "--staged".into(),
                        "--worktree".into(),
                        "--no-recurse-submodules".into(),
                        format!("--source={head}").into(),
                        "--pathspec-from-file=-".into(),
                        "--pathspec-file-nul".into(),
                    ]
                } else {
                    [
                        "rm",
                        "--cached",
                        "--ignore-unmatch",
                        "-f",
                        "--pathspec-from-file=-",
                        "--pathspec-file-nul",
                    ]
                    .map(Into::into)
                    .to_vec()
                };
                let mut request = self.request(
                    root,
                    [
                        "--literal-pathspecs",
                        "-c",
                        "core.fsmonitor=false",
                        "-c",
                        "submodule.recurse=false",
                    ],
                    Lifetime::AcceptedCommand,
                    1024 * 1024,
                );
                request.arguments.extend(args);
                request.input = stdin;
                request
                    .environment
                    .insert("GIT_EDITOR".into(), Some("false".into()));
                if !log.run(&request)? {
                    return Ok(());
                }
            }
            for entry in entries.iter().filter(|e| {
                e.group == Group::Untracked || staged && fresh.repository.head.is_none()
            }) {
                let target = regular(root, &entry.path)?;
                match remove_file(&target) {
                    Ok(()) => filesystem_changes += 1,
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                    Err(_) => return Err(Error::Code("RustFileDeleteFailed")),
                }
            }
            Ok(())
        })();
        let (commands, output, output_truncated) = log.into_parts();
        let updated = self.discard_context(context, entries, &cancel);
        let warning = if result.is_ok()
            && commands.iter().all(Command::succeeded)
            && updated.as_ref().is_ok_and(|fresh| {
                let selected = paths(entries).into_iter().collect::<BTreeSet<_>>();
                fresh.entries.iter().any(|e| {
                    selected.contains(&e.path)
                        && (staged || e.group != Group::Staged || !context.entries.contains(e))
                })
            }) {
            Some("RustDiscardRemaining")
        } else {
            None
        };
        // A later failure is a partial outcome. Keep the command result and
        // actual files; never retry, remove a Git lock, clean or roll back.
        Ok(Completed {
            commands,
            output,
            output_truncated,
            message: result.err().map(|e| e.code()),
            warning,
            saved_stash: None,
            filesystem_changes,
            updated,
        })
    }
}
