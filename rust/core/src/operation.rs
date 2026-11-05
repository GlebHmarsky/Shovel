//! Shared outcomes for native repository operations. Command exit and refresh
//! failure remain distinct; accepted process output stays bounded in memory.
use crate::{
    authentication,
    git::{self, Snapshot},
    process, working,
};
use std::time::Duration;
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
    pub updated: Result<Snapshot, Error>,
    pub authentication_failure: Option<authentication::Failure>,
}
impl Completed {
    pub fn succeeded(&self) -> bool {
        !self.commands.is_empty()
            && self.commands.iter().all(Command::succeeded)
            && self.message.is_none()
            && !self.has_conflicts
            && self.updated.as_ref().is_ok_and(|s| s.operations.is_empty())
    }
}
