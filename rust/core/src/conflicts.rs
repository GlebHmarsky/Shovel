//! Reviewed per-file conflict resolution. Native side checkout/tools preserve
//! filters; edited results retain encoding and survive a later staging failure.
use crate::{
    conflict_text::{self, Content, Encoding, MAX_BYTES},
    diff,
    git::{Git, Snapshot},
    identity,
    operation::{self, Error, Transcript},
    process::{self, Cancellation, Lifetime},
    recovery::Kind,
    working::{self, Change, Context, Group, MutationGate},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    ffi::OsString,
    fs::{self, File},
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
}
#[derive(Clone, Debug)]
pub struct Preview {
    pub context: Arc<Context>,
    pub change: Change,
    pub sides: [Option<Side>; 3],
    pub current_source: String,
    pub incoming_source: String,
    pub worktree_stamp: String,
    pub working: Option<Content>,
    pub can_edit: bool,
    pub attributes: Vec<u8>,
    pub tools: Vec<String>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
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
            let content = if size <= MAX_BYTES as u64 {
                conflict_text::decode(
                    &self
                        .read(root, ["cat-file", "blob", fields[1]], cancel, MAX_BYTES)?
                        .stdout,
                )
            } else {
                None
            };
            sides[stage - 1] = Some(Side {
                stage,
                hash: fields[1].into(),
                mode: fields[0].into(),
                content,
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
        let mut current_source = snapshot
            .branch
            .clone()
            .or(snapshot.head.clone())
            .unwrap_or_default();
        let mut incoming_source = String::new();
        if let Some(recovery) = &snapshot.recovery {
            if recovery.kind == Kind::Rebase {
                current_source = snapshot.head.clone().unwrap_or_default();
                incoming_source = format!("{} · {}", recovery.branch, recovery.current);
            } else {
                let marker = match recovery.kind {
                    Kind::Merge => "MERGE_HEAD",
                    Kind::CherryPick => "CHERRY_PICK_HEAD",
                    Kind::Revert => "REVERT_HEAD",
                    _ => unreachable!(),
                };
                if let Ok(file) = File::open(snapshot.git_directory.join(marker)) {
                    let mut bytes = Vec::new();
                    file.take(4096).read_to_end(&mut bytes)?;
                    if bytes.len() < 4096 {
                        incoming_source = text(&bytes)?
                            .lines()
                            .map(str::trim)
                            .filter(|s| oid(s))
                            .collect::<Vec<_>>()
                            .join(", ");
                    }
                }
                if recovery.kind == Kind::Merge && oid(&incoming_source) {
                    let names = self.local_read(
                        root,
                        &[
                            "for-each-ref",
                            &format!("--points-at={incoming_source}"),
                            "--format=%(refname:short)",
                            "refs/heads",
                            "refs/remotes",
                        ],
                        cancel,
                    )?;
                    let names = names.lines().take(3).collect::<Vec<_>>().join(", ");
                    if !names.is_empty() {
                        incoming_source = format!("{names} · {incoming_source}");
                    }
                }
            }
        }
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
        Ok(Preview {
            context,
            change: change.clone(),
            sides,
            current_source,
            incoming_source,
            worktree_stamp,
            working,
            can_edit,
            attributes,
            tools: tools.into_iter().take(64).collect(),
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
                    name = text(&result.stdout)?.trim().into();
                    break;
                }
            }
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
                    let index = if matches!(choice, Choice::Current) {
                        1
                    } else {
                        2
                    };
                    if preview.sides[index].is_none() {
                        match fs::remove_file(&file) {
                            Ok(()) => {}
                            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                            Err(e) => return Err(e.into()),
                        }
                    } else if !write(
                        [
                            "restore",
                            "--worktree",
                            "--no-recurse-submodules",
                            if index == 1 { "--ours" } else { "--theirs" },
                            "--",
                            &preview.change.path,
                        ]
                        .map(OsString::from)
                        .to_vec(),
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
