//! Repository menu commands use the same captured worktree and mutation gate as branch actions.
use crate::{
    authentication,
    branches::Stage,
    git::{Git, Snapshot},
    identity,
    operation::{Completed, Error, Transcript},
    process::{Cancellation, Lifetime},
    working::MutationGate,
};
use std::{ffi::OsString, path::PathBuf, sync::Arc};

#[derive(Debug)]
pub enum Action {
    Tag {
        name: String,
        target: String,
        message: String,
    },
    Worktree {
        path: PathBuf,
        branch: String,
        target: String,
    },
    Patch(PathBuf),
    BisectStart {
        good: String,
        bad: String,
    },
    Bisect(Bisect),
    FlowInit {
        production: String,
        development: String,
    },
    Feature {
        name: String,
        finish: bool,
    },
    LfsInstall,
    LfsFetch,
    LfsPull,
    LfsPattern {
        pattern: String,
        track: bool,
    },
}

#[derive(Clone, Copy, Debug)]
pub enum Bisect {
    Good,
    Bad,
    Skip,
    Reset,
}

impl Action {
    pub fn needs_authentication(&self) -> bool {
        matches!(self, Self::LfsFetch | Self::LfsPull)
    }
}

impl Git {
    pub fn execute_repository_action(
        &self,
        reviewed: &Snapshot,
        action: &Action,
        auth: Option<&authentication::Session>,
        progress: Option<Arc<dyn Fn(Stage) + Send + Sync>>,
    ) -> Result<Completed, Error> {
        let _gate = MutationGate::acquire(&reviewed.common_directory, false)?;
        let cancel = Cancellation::default();
        let root = &reviewed.worktree;
        if matches!(action, Action::Bisect(_)) {
            let actual = self.inspect(root, &cancel)?;
            if actual.identity != reviewed.identity
                || actual.head != reviewed.head
                || actual.branch != reviewed.branch
                || actual.operations != ["bisect"]
            {
                return Err(Error::Code("BranchManageChanged"));
            }
        } else {
            self.prepare_local(reviewed, false, &cancel)?;
        }
        let revision = |value: &str| -> Result<String, Error> {
            let value = format!("{value}^{{commit}}");
            let hash = self
                .local_read(
                    root,
                    &["rev-parse", "--verify", "--end-of-options", &value],
                    &cancel,
                )
                .map_err(|_| Error::Code("RepositoryRevisionInvalid"))?;
            Ok(hash.trim().to_owned())
        };
        let name = |value: &str, prefix: &str| -> Result<(), Error> {
            if value.is_empty() || value.starts_with('-') || value.chars().any(char::is_control) {
                return Err(Error::Code("RepositoryRefInvalid"));
            }
            self.local_read(
                root,
                &["check-ref-format", &format!("refs/{prefix}/{value}")],
                &cancel,
            )
            .map_err(|_| Error::Code("RepositoryRefInvalid"))?;
            Ok(())
        };
        let mut transcript = Transcript::default();
        let mut steps: Vec<Vec<OsString>> = Vec::new();
        let args: Vec<OsString> = match action {
            Action::Tag {
                name: tag,
                target,
                message,
            } => {
                name(tag, "tags")?;
                let mut args: Vec<OsString> = vec!["tag".into()];
                if !message.is_empty() {
                    args.extend(["-a", "-m", message].map(OsString::from));
                }
                args.extend(["--", tag, &revision(target)?].map(OsString::from));
                args
            }
            Action::Worktree {
                path,
                branch,
                target,
            } => {
                name(branch, "heads")?;
                if !path.is_absolute() {
                    return Err(Error::Code("RepositoryAbsolutePath"));
                }
                let mut args = ["worktree", "add", "-b", branch, "--"]
                    .map(OsString::from)
                    .to_vec();
                args.extend([path.as_os_str().to_owned(), revision(target)?.into()]);
                args
            }
            Action::Patch(path) => {
                if !path.is_absolute() || !path.is_file() {
                    return Err(Error::Code("RepositoryPatchMissing"));
                }
                vec!["apply".into(), "--".into(), path.as_os_str().to_owned()]
            }
            Action::BisectStart { good, bad } => {
                let good = revision(good)?;
                let bad = revision(bad)?;
                if good == bad {
                    return Err(Error::Code("RepositoryBisectSameRevision"));
                }
                ["bisect", "start", &bad, &good]
                    .map(OsString::from)
                    .to_vec()
            }
            Action::Bisect(step) => [
                "bisect",
                match step {
                    Bisect::Good => "good",
                    Bisect::Bad => "bad",
                    Bisect::Skip => "skip",
                    Bisect::Reset => "reset",
                },
            ]
            .map(OsString::from)
            .to_vec(),
            Action::FlowInit {
                production,
                development,
            } => {
                name(production, "heads")?;
                name(development, "heads")?;
                if production == development {
                    return Err(Error::Code("RepositoryFlowBranches"));
                }
                let production_ref = format!("refs/heads/{production}");
                let development_ref = format!("refs/heads/{development}");
                let target = revision(&production_ref)?;
                if !reviewed
                    .references
                    .iter()
                    .any(|reference| reference.name == development_ref)
                {
                    steps.push(
                        ["branch", "--", development, &target]
                            .map(OsString::from)
                            .to_vec(),
                    );
                }
                for (key, value) in [
                    ("gitflow.branch.master", production.as_str()),
                    ("gitflow.branch.develop", development),
                ] {
                    steps.push(
                        ["config", "--local", key, value]
                            .map(OsString::from)
                            .to_vec(),
                    );
                }
                ["config", "--local", "gitflow.prefix.feature", "feature/"]
                    .map(OsString::from)
                    .to_vec()
            }
            Action::Feature {
                name: feature,
                finish,
            } => {
                name(feature, "heads")?;
                if self.has_local_changes(root, Some(&reviewed.identity.fingerprint), &cancel)? {
                    return Err(Error::Code("RepositoryFlowDirty"));
                }
                let config = |key: &str| {
                    self.local_read(root, &["config", "--local", "--get", key], &cancel)
                        .map(|value| value.trim_end_matches(['\r', '\n']).to_owned())
                        .map_err(|_| Error::Code("RepositoryFlowUnconfigured"))
                };
                let development = config("gitflow.branch.develop")?;
                let branch = format!("{}{feature}", config("gitflow.prefix.feature")?);
                name(&development, "heads")?;
                name(&branch, "heads")?;
                let develop = format!("refs/heads/{development}");
                if *finish {
                    let tip = revision(&format!("refs/heads/{branch}"))?;
                    revision(&develop)?;
                    steps.push(["switch", "--", &development].map(OsString::from).to_vec());
                    ["merge", "--no-ff", "--no-edit", "--", &tip]
                        .map(OsString::from)
                        .to_vec()
                } else {
                    ["switch", "-c", &branch, "--", &revision(&develop)?]
                        .map(OsString::from)
                        .to_vec()
                }
            }
            Action::LfsInstall => ["lfs", "install", "--local"].map(OsString::from).to_vec(),
            Action::LfsFetch => ["lfs", "fetch"].map(OsString::from).to_vec(),
            Action::LfsPull => ["lfs", "pull"].map(OsString::from).to_vec(),
            Action::LfsPattern { pattern, track } => {
                if pattern.is_empty()
                    || pattern.starts_with('-')
                    || pattern.chars().any(char::is_control)
                {
                    return Err(Error::Code("RepositoryPatternInvalid"));
                }
                [
                    "lfs",
                    if *track { "track" } else { "untrack" },
                    "--",
                    pattern,
                ]
                .map(OsString::from)
                .to_vec()
            }
        };
        if identity::read(root)? != reviewed.identity {
            return Err(Error::Code("ActionIdentityChanged"));
        }
        if let Some(progress) = progress {
            progress(Stage::RepositoryCommand);
        }
        for step in steps {
            match transcript.write(self, root, step) {
                Ok(true) => {}
                result => return transcript.finish(self, root, result.map(|_| None)),
            }
        }
        let result = if action.needs_authentication() {
            let mut request = self.request(root, args, Lifetime::AcceptedCommand, 1024 * 1024);
            request
                .environment
                .extend(self.network_environment(root, auth, &cancel)?);
            transcript.run(&request)
        } else {
            transcript.write(self, root, args)
        };
        let mut completed = transcript.finish(self, root, result.map(|_| None))?;
        completed.authentication_failure = auth.and_then(authentication::Session::failure);
        if completed.succeeded()
            && let Action::Worktree { path, .. } = action
        {
            completed.related.push(path.clone());
        }
        Ok(completed)
    }
}
