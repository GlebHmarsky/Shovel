//! Native cloning, with accepted-command lifetime and retained partial results.
use crate::{
    authentication,
    git::Git,
    initialization::{self, Completed},
    network::redact,
    operation::Error,
    process::{self, Cancellation, Lifetime, OutputCallback},
    working::MutationGate,
};
use std::{
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
};
#[derive(Clone, Debug)]
pub struct Options {
    pub source: String,
    pub directory: PathBuf,
    pub branch: String,
}
impl Options {
    pub fn valid(&self) -> bool {
        valid_source(&self.source)
            && initialization::destination(&self.directory).is_ok()
            && !self.branch.starts_with('-')
            && !self.branch.chars().any(char::is_control)
    }
}
fn valid_source(source: &str) -> bool {
    if source.is_empty()
        || source.encode_utf16().count() > 32767
        || source.chars().any(char::is_control)
    {
        return false;
    }
    if Path::new(source).is_absolute() {
        return true;
    }
    if let Ok(url) = url::Url::parse(source) {
        if matches!(url.scheme(), "http" | "https") {
            return url.password().is_none() && url.query().is_none();
        }
        if matches!(url.scheme(), "ssh" | "git" | "file") {
            return true;
        }
    }
    // Git's scp-style user@host:path form; never accept a drive-relative path.
    let Some((host, path)) = source.split_once(':') else {
        return false;
    };
    if path.is_empty() || host.len() == 1 && host.as_bytes()[0].is_ascii_alphabetic() {
        return false;
    }
    let mut pieces = host.split('@');
    let first = pieces.next().unwrap();
    let valid = |part: &str| {
        !part.is_empty()
            && !part
                .chars()
                .any(|c| c.is_whitespace() || matches!(c, '@' | '/' | ':'))
    };
    valid(first) && pieces.next().is_none_or(valid) && pieces.next().is_none()
}
impl Git {
    pub fn clone_repository(
        &self,
        options: &Options,
        auth: Option<&authentication::Session>,
        progress: Option<OutputCallback>,
    ) -> Result<Completed, Error> {
        if !options.valid() {
            return Err(Error::Code("CloneInvalid"));
        }
        let target = initialization::destination(&options.directory)?;
        let _gate = MutationGate::acquire_named(&target, "Init")?;
        let parent = initialization::validate(&target)?;
        if target.is_dir() && fs::read_dir(&target)?.next().transpose()?.is_some() {
            return Err(Error::Code("CloneNotEmpty"));
        }
        let cancel = Cancellation::default();
        let environment = self.network_environment(&parent, auth, &cancel)?;
        if !initialization::same(&parent, &initialization::validate(&target)?) {
            return Err(Error::Code("InitDestinationChanged"));
        }
        let mut args: Vec<OsString> = [
            "-c",
            "core.fsmonitor=false",
            "-c",
            "protocol.allow=never",
            "-c",
            "protocol.file.allow=always",
            "-c",
            "protocol.https.allow=always",
            "-c",
            "protocol.http.allow=always",
            "-c",
            "protocol.ssh.allow=always",
            "-c",
            "protocol.git.allow=always",
            "clone",
            "--progress",
            "--origin=origin",
        ]
        .into_iter()
        .map(Into::into)
        .collect();
        if !options.branch.is_empty() {
            args.push(format!("--branch={}", options.branch).into());
        }
        args.push("--".into());
        args.push(options.source.clone().into());
        args.push(target.as_os_str().to_owned());
        let mut request = self.request(&parent, args, Lifetime::AcceptedCommand, 1024 * 1024);
        request.environment.extend(environment);
        let result = process::run(&request, &cancel, progress)?;
        // A bounded final chunk can end in the middle of a credential-bearing
        // URL. Publish only complete lines when native output was truncated.
        let captured = |bytes: &[u8]| {
            let bytes = if result.outcome == process::Outcome::OutputLimit {
                &bytes[..bytes
                    .iter()
                    .rposition(|b| matches!(b, b'\r' | b'\n'))
                    .map_or(0, |n| n + 1)]
            } else {
                bytes
            };
            redact(&String::from_utf8_lossy(bytes))
        };
        let mut completed = Completed {
            repository: None,
            succeeded: result.success(),
            message: if result.outcome == process::Outcome::OutputLimit {
                "CloneOutputLimit"
            } else if result.success() {
                "CloneSucceeded"
            } else {
                "CloneIncomplete"
            },
            output: format!("{}{}", captured(&result.stdout), captured(&result.stderr)),
        };
        if target.is_dir()
            && initialization::validate(&target).is_ok()
            && let Ok(repository) = self.inspect(&target, &cancel)
            && initialization::same(&repository.worktree, &target)
        {
            completed.repository = Some(repository);
        }
        if completed.succeeded && completed.repository.is_none() {
            completed.message = "CloneUnverified";
        }
        if let Some(failure) = auth.and_then(authentication::Session::failure) {
            completed.succeeded = false;
            completed.message = match failure {
                authentication::Failure::Unavailable => "RustAuthUnavailable",
                authentication::Failure::TimedOut => "RustAuthTimedOut",
                authentication::Failure::PromptLimit => "RustAuthPromptLimit",
            };
        }
        Ok(completed)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn options_follow_literal_baseline_source_and_branch_rules_without_preparation() {
        for source in [
            "https://example.invalid/repo.git",
            "ssh://git@example.invalid/repo",
            "git@example.invalid:path.git",
            "host:path",
            r"C:\absolute path\repo",
            "file:///C:/repo",
        ] {
            assert!(valid_source(source), "{source}");
        }
        for (branch, expected) in [
            ("", true),
            ("feature/topic", true),
            ("v1", true),
            ("--help", false),
            ("line\nfeed", false),
        ] {
            assert_eq!(
                Options {
                    source: "https://example.invalid/repo.git".into(),
                    directory: PathBuf::from(r"C:\fixtures\not-created"),
                    branch: branch.into(),
                }
                .valid(),
                expected
            );
        }
        for source in [
            "",
            "relative/path",
            "C:relative",
            "--upload-pack=bad",
            "https://user:secret@example.invalid/repo",
            "https://example.invalid/repo?token=x",
            "host:\npath",
            "@host:path",
            "user@@host:path",
        ] {
            assert!(!valid_source(source), "{source}");
        }
    }
}
