//! Application contracts shared by the Rust desktop UI, helper and verifier.
//! This crate does not load or invoke the C# application.
#[cfg(windows)]
pub mod authentication;
#[cfg(windows)]
pub mod branches;
#[cfg(windows)]
pub mod diff;
#[cfg(windows)]
pub mod drafts;
#[cfg(windows)]
pub mod git;
pub mod graph;
#[cfg(windows)]
pub mod history_actions;
#[cfg(windows)]
pub mod identity;
#[cfg(windows)]
mod lfs;
#[cfg(windows)]
pub mod network;
#[cfg(windows)]
pub mod operation;
#[cfg(windows)]
pub mod process;
#[cfg(windows)]
pub mod recovery;
pub mod scheduler;
pub mod settings;
pub mod updates;
#[cfg(windows)]
pub mod working;
