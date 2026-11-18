//! Shared outcomes for native repository operations. Command exit and refresh
//! failure remain distinct; accepted process output stays bounded in memory.
use crate::{
    authentication,
    git::{self, Git, Snapshot},
    network::captured_output,
    process::{self, Cancellation, Lifetime},
    working,
};
use std::{ffi::OsString, path::Path, time::Duration};
const LIMIT: usize = 1024 * 1024;
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Code(&'static str),
    #[error("ActionReadFailed")]
    Git(#[from] git::Error),
    #[error("{0}")]
    Working(#[from] working::Error),
    #[error("ActionReadFailed")]
    Io(#[from] std::io::Error),
}
impl Error {
    pub fn code(&self) -> String {
        self.to_string()
    }
}
#[derive(Debug)]
pub struct Command {
    pub verb: Option<&'static str>,
    pub outcome: process::Outcome,
    pub exit_code: u32,
    pub elapsed: Duration,
}
impl Command {
    pub(crate) fn from_git(request: &process::Request, result: &process::Result) -> Self {
        Self {
            verb: git_verb(&request.arguments),
            outcome: result.outcome,
            exit_code: result.exit_code,
            elapsed: result.elapsed,
        }
    }
    pub fn succeeded(&self) -> bool {
        self.outcome.accepted_succeeded(self.exit_code)
    }
}
// Only retain a known Git command name. Configuration, paths, remotes, message
// arguments and the environment are not needed to identify which command exited.
fn git_verb(arguments: &[OsString]) -> Option<&'static str> {
    const VERBS: &[&str] = &[
        "add",
        "am",
        "apply",
        "branch",
        "checkout",
        "cherry-pick",
        "clean",
        "clone",
        "commit",
        "config",
        "fetch",
        "init",
        "merge",
        "mv",
        "pull",
        "push",
        "rebase",
        "remote",
        "reset",
        "restore",
        "revert",
        "rm",
        "stash",
        "submodule",
        "switch",
        "tag",
        "update-index",
        "update-ref",
        "worktree",
    ];
    let mut args = arguments.iter();
    while let Some(arg) = args.next() {
        match arg.to_str()? {
            "-c" | "-C" | "--git-dir" | "--work-tree" => {
                args.next()?;
            }
            "--no-pager" | "--no-lazy-fetch" | "--no-optional-locks" | "--literal-pathspecs" => {}
            verb => return VERBS.iter().copied().find(|item| *item == verb),
        }
    }
    None
}

pub struct Completed {
    pub commands: Vec<Command>,
    pub output: String,
    pub output_truncated: bool,
    pub message: Option<String>,
    pub warning: Option<String>,
    pub has_conflicts: bool,
    pub(crate) preserves_worktree: bool,
    pub(crate) unchanged: bool,
    pub updated: Result<Snapshot, Error>,
    pub related: Vec<std::path::PathBuf>,
    pub authentication_failure: Option<authentication::Failure>,
}
impl Completed {
    pub fn succeeded(&self) -> bool {
        (!self.commands.is_empty() || self.unchanged)
            && self.commands.iter().all(Command::succeeded)
            && self.message.is_none()
            && (!self.has_conflicts || self.preserves_worktree)
            && self
                .updated
                .as_ref()
                .is_ok_and(|s| self.preserves_worktree || s.operations.is_empty())
    }
}

#[derive(Default)]
pub(crate) struct Transcript {
    commands: Vec<Command>,
    output: String,
    truncated: bool,
}
impl Transcript {
    pub(crate) fn into_parts(self) -> (Vec<Command>, String, bool) {
        (self.commands, self.output, self.truncated)
    }
    pub(crate) fn write(
        &mut self,
        git: &Git,
        root: &Path,
        args: impl IntoIterator<Item = impl Into<OsString>>,
    ) -> Result<bool, Error> {
        self.write_with_editor(git, root, args, "false")
    }
    pub(crate) fn write_with_editor(
        &mut self,
        git: &Git,
        root: &Path,
        args: impl IntoIterator<Item = impl Into<OsString>>,
        editor: &str,
    ) -> Result<bool, Error> {
        let mut request = git.request(
            root,
            ["-c", "core.fsmonitor=false"],
            Lifetime::AcceptedCommand,
            LIMIT,
        );
        request.arguments.extend(args.into_iter().map(Into::into));
        request
            .environment
            .insert("GIT_EDITOR".into(), Some(editor.into()));
        if editor == "true" {
            request
                .environment
                .insert("GIT_SEQUENCE_EDITOR".into(), Some("false".into()));
        }
        request
            .environment
            .insert("GIT_MERGE_AUTOEDIT".into(), Some("no".into()));
        self.run(&request)
    }
    pub(crate) fn run(&mut self, request: &process::Request) -> Result<bool, Error> {
        self.run_capture(request)?;
        Ok(self.commands.last().is_some_and(Command::succeeded))
    }
    pub(crate) fn run_capture(
        &mut self,
        request: &process::Request,
    ) -> Result<process::Result, Error> {
        let result = process::run(request, &Cancellation::default(), None)?;
        let output = captured_output(&result);
        let mut available = LIMIT.saturating_sub(self.output.len()).min(output.len());
        while !output.is_char_boundary(available) {
            available -= 1;
        }
        self.output.push_str(&output[..available]);
        self.truncated |=
            available != output.len() || result.outcome == process::Outcome::OutputLimit;
        let command = Command::from_git(request, &result);
        self.commands.push(command);
        Ok(result)
    }
    pub(crate) fn finish(
        self,
        git: &Git,
        root: &Path,
        result: Result<Option<&'static str>, Error>,
    ) -> Result<Completed, Error> {
        let cancel = Cancellation::default();
        let (message, warning) = match result {
            Err(error) if self.commands.is_empty() => return Err(error),
            Err(error) => (Some(error.code()), None),
            Ok(warning) => (None, warning.map(str::to_owned)),
        };
        let updated = git.inspect(root, &cancel).map_err(Error::from);
        let unmerged = git.local_read(root, &["ls-files", "--unmerged", "-z"], &cancel);
        let message = message.or_else(|| unmerged.as_ref().err().map(Error::code));
        let has_conflicts = unmerged.is_ok_and(|s| !s.is_empty());
        Ok(Completed {
            commands: self.commands,
            output: self.output,
            output_truncated: self.truncated,
            message,
            warning,
            has_conflicts,
            preserves_worktree: false,
            unchanged: false,
            updated,
            related: Vec::new(),
            authentication_failure: None,
        })
    }
}

#[cfg(test)]
mod command_name_tests {
    use super::*;
    #[test]
    fn activity_commands_exclude_global_values_aliases_and_arguments() {
        let name = |args: &[&str]| git_verb(&args.iter().map(OsString::from).collect::<Vec<_>>());
        assert_eq!(
            name(&[
                "--no-pager",
                "-c",
                "credential.helper=sensitive",
                "-C",
                "push",
                "--literal-pathspecs",
                "checkout",
                "--",
                "private path"
            ]),
            Some("checkout")
        );
        assert_eq!(name(&["-c", "fetch"]), None);
        assert_eq!(name(&["external-alias", "push"]), None);
        assert_eq!(name(&["--unknown", "push"]), None);
    }
}
