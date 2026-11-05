//! Shared outcomes for native repository operations. Command exit and refresh
//! failure remain distinct; accepted process output stays bounded in memory.
use crate::{
    authentication,
    git::{self, Git, Snapshot},
    network::redact,
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
    pub outcome: process::Outcome,
    pub exit_code: u32,
    pub elapsed: Duration,
}
impl Command {
    pub fn succeeded(&self) -> bool {
        self.exit_code == 0
            && matches!(
                self.outcome,
                process::Outcome::Exited | process::Outcome::OutputLimit
            )
    }
}
pub struct Completed {
    pub commands: Vec<Command>,
    pub output: String,
    pub output_truncated: bool,
    pub message: Option<String>,
    pub warning: Option<String>,
    pub has_conflicts: bool,
    pub(crate) preserves_worktree: bool,
    pub updated: Result<Snapshot, Error>,
    pub authentication_failure: Option<authentication::Failure>,
}
impl Completed {
    pub fn succeeded(&self) -> bool {
        !self.commands.is_empty()
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
        let result = process::run(&request, &Cancellation::default(), None)?;
        let output = redact(&format!(
            "{}{}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        ));
        let mut available = LIMIT.saturating_sub(self.output.len()).min(output.len());
        while !output.is_char_boundary(available) {
            available -= 1;
        }
        self.output.push_str(&output[..available]);
        self.truncated |=
            available != output.len() || result.outcome == process::Outcome::OutputLimit;
        let command = Command {
            outcome: result.outcome,
            exit_code: result.exit_code,
            elapsed: result.elapsed,
        };
        let succeeded = command.succeeded();
        self.commands.push(command);
        Ok(succeeded)
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
            updated,
            authentication_failure: None,
        })
    }
}
