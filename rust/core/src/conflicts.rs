//! Reviewed per-file conflict resolution. Native side checkout/tools preserve
//! filters; edited results retain encoding and survive a later staging failure.
use crate::{
    conflict_text::{self, Content, Encoding, MAX_BYTES},
    diff,
    git::{self, Commit, Git, Snapshot},
    identity,
    operation::{self, Error, Transcript},
    process::{self, Cancellation, Lifetime},
    recovery::Kind,
    stashes::ConflictOrigin,
    working::{self, Change, Context, Group, MutationGate},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    ffi::OsString,
    fs::File,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::Arc,
};

#[derive(Clone, Debug)]
pub struct Side {
    pub stage: usize,
    pub hash: String,
    pub mode: String,
    pub content: Option<Content>,
    pub bytes: u64,
    pub lfs: bool,
}
#[derive(Clone, Debug)]
pub struct Source {
    pub reference: Option<String>,
    pub revision: Option<String>,
    pub commit: Option<Commit>,
}

#[derive(Clone, Debug)]
pub struct Preview {
    pub context: Arc<Context>,
    pub change: Change,
    pub sides: [Option<Side>; 3],
    pub sources: [Source; 2],
    pub from_stash: bool,
    pub worktree_stamp: String,
    pub working: Option<Content>,
    pub can_edit: bool,
    pub attributes: Vec<u8>,
    pub tools: Vec<String>,
    pub configured_tool: Option<String>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct ToolSettings {
    pub mode: u64,
    pub tool_name: String,
    pub executable_path: String,
    pub command: String,
}
impl ToolSettings {
    pub fn validate(&self) -> Result<(), Error> {
        match self.mode {
            0 => Ok(()),
            1 if !valid_tool_name(&self.tool_name) => Err(Error::Code("ConflictToolNameInvalid")),
            1 if !self.executable_path.is_empty()
                && (self.executable_path.encode_utf16().count() > 32767
                    || self
                        .executable_path
                        .chars()
                        .any(|c| c.is_control() || "\"<>|".contains(c))
                    || !Path::new(&self.executable_path).is_absolute()
                    || !Path::new(&self.executable_path)
                        .extension()
                        .is_some_and(|s| s.eq_ignore_ascii_case("exe"))) =>
            {
                Err(Error::Code("ConflictToolPathInvalid"))
            }
            1 => Ok(()),
            2 if self.command.trim().is_empty()
                || self.command.encode_utf16().count() > 8192
                || self.command.contains('\0') =>
            {
                Err(Error::Code("ConflictToolCommandInvalid"))
            }
            2 => Ok(()),
            _ => Err(Error::Code("ConflictToolSettingsInvalid")),
        }
    }
}
pub fn valid_tool_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && !name.starts_with('-')
        && name
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"-_.".contains(&c))
}
#[derive(Clone, Debug)]
pub enum Tool {
    Configured,
    Named(String),
    Saved(ToolSettings),
}
#[derive(Clone, Debug)]
pub enum Choice {
    Base,
    Current,
    Incoming,
    Edited(String),
    External(Tool),
}
pub struct Completed {
    pub operation: operation::Completed,
    pub resolved: bool,
    pub updated: Result<Context, working::Error>,
}
fn oid(s: &str) -> bool {
    matches!(s.len(), 40 | 64) && s.bytes().all(|b| b.is_ascii_hexdigit())
}
fn regular(root: &Path, relative: &str) -> Result<PathBuf, Error> {
    let file = working::path(root, relative)?;
    if diff::has_reparse(root, &file)? || file.is_dir() {
        return Err(Error::Code("ConflictUnsupported"));
    }
    Ok(file)
}
fn text(bytes: &[u8]) -> Result<&str, Error> {
    std::str::from_utf8(bytes).map_err(|_| Error::Code("GitErrorInvalidOutput"))
}
fn working_content(file: &Path) -> Result<Option<Content>, Error> {
    let file = match File::open(file) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    let mut bytes = Vec::new();
    file.take(MAX_BYTES as u64 + 1).read_to_end(&mut bytes)?;
    Ok(conflict_text::decode(&bytes))
}
impl Git {
    fn configured_merge_tool(
        &self,
        root: &Path,
        cancel: &Cancellation,
    ) -> Result<Option<String>, Error> {
        for key in ["merge.guitool", "merge.tool"] {
            let result = process::run(
                &self.request(root, ["config", "--get", key], Lifetime::Read, 65536),
                cancel,
                None,
            )?;
            if result.outcome != process::Outcome::Exited || result.exit_code > 1 {
                return Err(Error::Code("ActionReadFailed"));
            }
            if result.exit_code == 0 {
                let name = text(&result.stdout)?.trim();
                return Ok((!name.is_empty() && !name.chars().any(char::is_control))
                    .then(|| name.to_owned()));
            }
        }
        Ok(None)
    }

    fn conflict_source(
        &self,
        root: &Path,
        reference: Option<String>,
        revision: Option<String>,
        find_ref: bool,
        cancel: &Cancellation,
    ) -> Source {
        let reference = reference.or_else(|| {
            let revision = revision.as_ref().filter(|_| find_ref)?;
            let names = self
                .local_read(
                    root,
                    &[
                        "for-each-ref",
                        &format!("--points-at={revision}"),
                        "--format=%(refname:short)",
                        "refs/heads",
                        "refs/remotes",
                    ],
                    cancel,
                )
                .ok()?;
            let mut names = names.lines().filter(|name| {
                !name.is_empty() && !name.ends_with("/HEAD") && !name.chars().any(char::is_control)
            });
            let name = names.next()?;
            // Multiple branch aliases at the same object cannot identify the source ref.
            names.next().is_none().then(|| name.to_owned())
        });
        let commit = revision.as_deref().and_then(|revision| {
            let mut args = vec!["log".into(), "--no-walk=unsorted".into()];
            args.extend(git::log_options());
            args.extend([revision.into(), "--".into()]);
            let raw = self.read(root, args, cancel, 1024 * 1024).ok()?;
            let commit = git::parse_commits(&raw.stdout, 1).ok()?.pop()?;
            (commit.hash == revision).then_some(commit)
        });
        Source {
            reference,
            revision,
            commit,
        }
    }

    fn conflict_sources(
        &self,
        snapshot: &Snapshot,
        path: &str,
        stages: &[Option<Side>; 3],
        stash: Option<&ConflictOrigin>,
        cancel: &Cancellation,
    ) -> [Source; 2] {
        let root = &snapshot.worktree;
        let recovery = snapshot.recovery.as_ref();
        let rebase = recovery.is_some_and(|r| r.kind == Kind::Rebase);
        let current = snapshot
            .head
            .as_ref()
            .filter(|head| {
                if recovery.is_some() {
                    return true;
                }
                // Stash and externally created conflicts can use a staged version for stage 2.
                let expected = stages[1].as_ref().map_or_else(String::new, |side| {
                    format!("{} blob {}\t{path}\0", side.mode, side.hash)
                });
                self.read(
                    root,
                    ["--literal-pathspecs", "ls-tree", "-z", head, "--", path],
                    cancel,
                    65536,
                )
                .is_ok_and(|output| output.stdout == expected.as_bytes())
            })
            .cloned();
        let (incoming_ref, incoming, find_ref) = if let Some(stash) = stash {
            (
                Some(stash.target.selector.clone()),
                Some(stash.target.hash.clone()),
                false,
            )
        } else if let Some(recovery) = recovery {
            match recovery.kind {
                Kind::Rebase => (
                    (!recovery.branch.is_empty()).then(|| recovery.branch.clone()),
                    oid(&recovery.current).then(|| recovery.current.clone()),
                    false,
                ),
                Kind::CherryPick => (
                    None,
                    oid(&recovery.current).then(|| recovery.current.clone()),
                    true,
                ),
                Kind::Merge => {
                    let revision = File::open(snapshot.git_directory.join("MERGE_HEAD"))
                        .ok()
                        .and_then(|file| {
                            let mut bytes = Vec::new();
                            file.take(4097).read_to_end(&mut bytes).ok()?;
                            let head = (bytes.len() <= 4096)
                                .then(|| text(&bytes).ok())
                                .flatten()?
                                .trim();
                            oid(head).then(|| head.to_owned())
                        });
                    (None, revision, true)
                }
                // A revert applies an inverse patch. REVERT_HEAD is the target,
                // which is not necessarily the tree supplying stage 3.
                Kind::Revert => (None, None, false),
            }
        } else {
            (None, None, false)
        };
        [
            self.conflict_source(root, snapshot.branch.clone(), current, rebase, cancel),
            self.conflict_source(root, incoming_ref, incoming, find_ref, cancel),
        ]
    }

    fn conflict_attributes(
        &self,
        root: &Path,
        path: &str,
        cancel: &Cancellation,
    ) -> Result<Vec<u8>, Error> {
        Ok(self
            .read(
                root,
                [
                    "--literal-pathspecs",
                    "check-attr",
                    "-z",
                    "filter",
                    "working-tree-encoding",
                    "--",
                    path,
                ],
                cancel,
                65536,
            )?
            .stdout)
    }
    pub fn read_conflict(
        &self,
        context: Arc<Context>,
        change: &Change,
        cancel: &Cancellation,
    ) -> Result<Preview, Error> {
        self.read_conflict_with_stash(context, change, None, cancel)
    }

    pub fn read_conflict_with_stash(
        &self,
        context: Arc<Context>,
        change: &Change,
        stash: Option<&ConflictOrigin>,
        cancel: &Cancellation,
    ) -> Result<Preview, Error> {
        if change.group != Group::Conflicted || !context.entries.contains(change) {
            return Err(Error::Code("CommitStale"));
        }
        let snapshot = &context.repository;
        let root = &snapshot.worktree;
        let file = regular(root, &change.path)?;
        if identity::read(root)? != snapshot.identity
            || self.semantic_index(snapshot, cancel)? != context.index_stamp
        {
            return Err(Error::Code("CommitStale"));
        }
        let worktree_stamp = diff::content_stamp(root, &change.path, cancel)?;
        let raw = self
            .read(
                root,
                [
                    "--literal-pathspecs",
                    "ls-files",
                    "--unmerged",
                    "-z",
                    "--",
                    &change.path,
                ],
                cancel,
                1024 * 1024,
            )?
            .stdout;
        if !raw.is_empty() && !raw.ends_with(&[0]) {
            return Err(Error::Code("CommitStale"));
        }
        let mut sides: [Option<Side>; 3] = [None, None, None];
        for record in text(&raw)?.split_terminator('\0') {
            let (metadata, path) = record.split_once('\t').ok_or(Error::Code("CommitStale"))?;
            let fields = metadata.split(' ').collect::<Vec<_>>();
            if fields.len() != 3 || path != change.path || !oid(fields[1]) {
                return Err(Error::Code("CommitStale"));
            }
            let stage = fields[2]
                .parse::<usize>()
                .ok()
                .filter(|s| (1..=3).contains(s))
                .ok_or(Error::Code("CommitStale"))?;
            if sides[stage - 1].is_some() {
                return Err(Error::Code("CommitStale"));
            }
            if !matches!(fields[0], "100644" | "100755") {
                return Err(Error::Code("ConflictUnsupported"));
            }
            let size = self
                .local_read(root, &["cat-file", "-s", fields[1]], cancel)?
                .trim()
                .parse::<u64>()
                .map_err(|_| Error::Code("GitErrorInvalidOutput"))?;
            let (content, lfs_bytes) = if size <= MAX_BYTES as u64 {
                let blob = self.read(root, ["cat-file", "blob", fields[1]], cancel, MAX_BYTES)?;
                let lfs_bytes = diff::lfs_size(&blob.stdout);
                let content = if lfs_bytes.is_none() {
                    conflict_text::decode(&blob.stdout)
                } else {
                    None
                };
                (content, lfs_bytes)
            } else {
                (None, None)
            };
            sides[stage - 1] = Some(Side {
                stage,
                hash: fields[1].into(),
                mode: fields[0].into(),
                content,
                bytes: lfs_bytes.unwrap_or(size),
                lfs: lfs_bytes.is_some(),
            });
        }
        if sides.iter().all(Option::is_none) {
            return Err(Error::Code("CommitStale"));
        }
        let working = working_content(&file)?;
        let attributes = self.conflict_attributes(root, &change.path, cancel)?;
        let values = text(&attributes)?
            .split_terminator('\0')
            .collect::<Vec<_>>();
        if values.len() != 6
            || values[0] != change.path
            || values[3] != change.path
            || values[1] != "filter"
            || values[4] != "working-tree-encoding"
        {
            return Err(Error::Code("GitErrorInvalidOutput"));
        }
        let can_edit = working.is_some()
            && sides.iter().flatten().all(|s| s.content.is_some())
            && [values[2], values[5]]
                .iter()
                .all(|s| matches!(*s, "unset" | "unspecified"));
        let stash = stash.filter(|stash| stash.matches(snapshot, &change.path, &raw));
        let sources = self.conflict_sources(snapshot, &change.path, &sides, stash, cancel);
        if self.semantic_index(snapshot, cancel)? != context.index_stamp
            || diff::content_stamp(root, &change.path, cancel)? != worktree_stamp
            || identity::read(root)? != snapshot.identity
        {
            return Err(Error::Code("CommitStale"));
        }
        let mut tools = [
            "bc",
            "kdiff3",
            "meld",
            "p4merge",
            "tortoisemerge",
            "vscode",
            "winmerge",
        ]
        .map(str::to_owned)
        .into_iter()
        .collect::<BTreeSet<_>>();
        let result = process::run(
            &self.request(
                root,
                [
                    "config",
                    "--name-only",
                    "--get-regexp",
                    r"^mergetool\..*\.(cmd|path)$",
                ],
                Lifetime::Read,
                1024 * 1024,
            ),
            cancel,
            None,
        )?;
        if result.outcome != process::Outcome::Exited || result.exit_code > 1 {
            return Err(Error::Code("ActionReadFailed"));
        }
        for key in text(&result.stdout)?.lines() {
            if let Some(rest) = key.strip_prefix("mergetool.")
                && let Some((name, _)) = rest.rsplit_once('.')
                && valid_tool_name(name)
            {
                tools.insert(name.into());
            }
        }
        let configured_tool = self.configured_merge_tool(root, cancel).ok().flatten();
        Ok(Preview {
            context,
            change: change.clone(),
            sides,
            sources,
            from_stash: stash.is_some(),
            worktree_stamp,
            working,
            can_edit,
            attributes,
            tools: tools.into_iter().take(64).collect(),
            configured_tool,
        })
    }
    fn validate_conflict(
        &self,
        preview: &Preview,
        cancel: &Cancellation,
    ) -> Result<Snapshot, Error> {
        let context = &preview.context;
        let root = &context.repository.worktree;
        regular(root, &preview.change.path)?;
        let fresh = self.inspect(root, cancel)?;
        if fresh.identity != context.repository.identity
            || fresh.head != context.repository.head
            || fresh.branch != context.repository.branch
            || fresh.operations != context.repository.operations
            || fresh.recovery != context.repository.recovery
            || self.semantic_index(&fresh, cancel)? != context.index_stamp
            || self.config_stamp(root, cancel)? != context.config_stamp
            || self.conflict_attributes(root, &preview.change.path, cancel)? != preview.attributes
        {
            return Err(Error::Code("CommitStale"));
        }
        if diff::content_stamp(root, &preview.change.path, cancel)? != preview.worktree_stamp {
            return Err(Error::Code("ConflictEditedOutside"));
        }
        Ok(fresh)
    }
    fn external_merge(
        &self,
        preview: &Preview,
        tool: &Tool,
        cancel: &Cancellation,
    ) -> Result<(Vec<OsString>, Option<String>), Error> {
        let root = &preview.context.repository.worktree;
        let mut name = String::new();
        let mut args: Vec<OsString> = vec![];
        let mut custom = None;
        match tool {
            Tool::Named(value) => {
                if !valid_tool_name(value) || !preview.tools.contains(value) {
                    return Err(Error::Code("ConflictToolInvalid"));
                }
                name = value.clone();
            }
            Tool::Saved(settings) => {
                settings.validate()?;
                match settings.mode {
                    1 => {
                        name = settings.tool_name.clone();
                        if !settings.executable_path.is_empty() {
                            if !Path::new(&settings.executable_path).is_file() {
                                return Err(Error::Code("ConflictToolExecutableMissing"));
                            }
                            args.extend([
                                "-c".into(),
                                format!("mergetool.{name}.path={}", settings.executable_path)
                                    .into(),
                            ]);
                        }
                    }
                    2 => {
                        name = "shovel-custom".into();
                        custom = Some(settings.command.replace("\r\n", "\n"));
                        args.extend(["-c", "mergetool.shovel-custom.cmd=eval \"$SHOVEL_EXTERNAL_MERGE_COMMAND\"", "-c", "mergetool.shovel-custom.trustExitCode=true", "-c", "mergetool.shovel-custom.hideResolved=false"].map(OsString::from));
                    }
                    _ => {}
                }
            }
            Tool::Configured => {}
        }
        if name.is_empty() {
            name = self
                .configured_merge_tool(root, cancel)?
                .ok_or(Error::Code("ConflictToolMissing"))?;
        }
        if name.trim().is_empty() || name.chars().any(char::is_control) {
            return Err(Error::Code("ConflictToolMissing"));
        }
        args.extend([
            "mergetool".into(),
            "--no-prompt".into(),
            format!("--tool={name}").into(),
            "--".into(),
            preview.change.path.clone().into(),
        ]);
        Ok((args, custom))
    }
    pub fn resolve_conflict(&self, preview: &Preview, choice: &Choice) -> Result<Completed, Error> {
        let cancel = Cancellation::default();
        let context = &preview.context;
        let root = &context.repository.worktree;
        let _gate = MutationGate::acquire(&context.repository.common_directory, false)?;
        self.validate_conflict(preview, &cancel)?;
        let file = regular(root, &preview.change.path)?;
        let mut log = Transcript::default();
        let mut write = |args: Vec<OsString>, custom: Option<String>| -> Result<bool, Error> {
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
            request
                .environment
                .insert("GIT_EDITOR".into(), Some("false".into()));
            if let Some(custom) = custom {
                request
                    .environment
                    .insert("SHOVEL_EXTERNAL_MERGE_COMMAND".into(), Some(custom.into()));
            }
            log.run(&request)
        };
        let result = (|| -> Result<Option<&'static str>, Error> {
            if let Choice::External(tool) = choice {
                let (args, custom) = self.external_merge(preview, tool, &cancel)?;
                write(args, custom)?;
            } else {
                if let Choice::Edited(value) = choice {
                    let encoding: Encoding = preview
                        .working
                        .as_ref()
                        .filter(|_| preview.can_edit)
                        .ok_or(Error::Code("ConflictTextLimit"))?
                        .encoding;
                    let encoded = conflict_text::encode(value, encoding)?;
                    if conflict_text::has_markers(value) {
                        return Err(Error::Code("ConflictMarkersRemain"));
                    }
                    let parent = file.parent().ok_or(Error::Code("ConflictUnsupported"))?;
                    let mut temporary = tempfile::Builder::new()
                        .prefix(".shovel-conflict-")
                        .suffix(".tmp")
                        .tempfile_in(parent)?;
                    temporary.write_all(&encoded)?;
                    temporary.as_file().sync_all()?;
                    regular(root, &preview.change.path)?;
                    if diff::content_stamp(root, &preview.change.path, &cancel)?
                        != preview.worktree_stamp
                    {
                        return Err(Error::Code("ConflictEditedOutside"));
                    }
                    temporary
                        .into_temp_path()
                        .persist(&file)
                        .map_err(|e| e.error)?;
                } else {
                    let (index, args): (usize, &[&str]) = match choice {
                        Choice::Base => (0, &["checkout-index", "--force", "--stage=1", "--"]),
                        Choice::Current => (1, &["restore", "--worktree", "--no-recurse-submodules", "--ours", "--"]),
                        Choice::Incoming => (2, &["restore", "--worktree", "--no-recurse-submodules", "--theirs", "--"]),
                        Choice::Edited(_) | Choice::External(_) => unreachable!(),
                    };
                    if preview.sides[index].is_none() {
                        match crate::files::remove_file(&file) {
                            Ok(()) => {}
                            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                            Err(_) => return Err(Error::Code("RustFileDeleteFailed")),
                        }
                    } else if !write(
                        args.iter().copied().chain([preview.change.path.as_str()]).map(OsString::from).collect(),
                        None,
                    )? {
                        return Ok(None);
                    }
                }
                write(
                    ["add", "--all", "--", &preview.change.path]
                        .map(OsString::from)
                        .to_vec(),
                    None,
                )?;
            }
            Ok(None)
        })();
        let mut operation = log.finish(self, root, result)?;
        let unresolved = self.local_read(
            root,
            &[
                "--literal-pathspecs",
                "ls-files",
                "--unmerged",
                "-z",
                "--",
                &preview.change.path,
            ],
            &cancel,
        );
        let resolved = operation.message.is_none()
            && operation.commands.iter().all(operation::Command::succeeded)
            && unresolved.as_ref().is_ok_and(|s| s.is_empty());
        if !resolved
            && operation.message.is_none()
            && operation.commands.iter().all(operation::Command::succeeded)
        {
            operation.message = Some(
                if unresolved.is_err() {
                    "ActionReadFailed"
                } else {
                    "ConflictStillUnmerged"
                }
                .into(),
            );
        }
        Ok(Completed {
            operation,
            resolved,
            updated: self.working_copy(root, &cancel),
        })
    }
}
