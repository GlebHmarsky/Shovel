//! Local-only remote configuration. Reviewed values are never silently flattened.
use crate::{
    branches::{Progress, Stage},
    git::{Git, Snapshot},
    identity,
    network::redact,
    operation::{Completed, Error, Transcript},
    process::{self, Cancellation, Lifetime},
    working::MutationGate,
};
use std::{collections::BTreeMap, path::Path};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Remote {
    pub name: String,
    pub location: String,
    pub push_location: String,
    pub can_edit: bool,
}
#[derive(Clone, Debug)]
pub struct Configuration {
    pub remotes: Vec<Remote>,
    pub stamp: String,
}
#[derive(Clone, Debug)]
pub enum Action {
    Add {
        name: String,
        location: String,
    },
    Change {
        reviewed: Configuration,
        name: String,
        location: String,
        push_location: String,
        remove: bool,
    },
}
pub fn valid(name: &str, location: &str) -> bool {
    if !(1..=255).contains(&name.encode_utf16().count())
        || name.starts_with('-')
        || name.chars().any(|c| c.is_whitespace() || c.is_control())
        || !(1..=32767).contains(&location.encode_utf16().count())
        || location.chars().any(char::is_control)
    {
        return false;
    }
    if let Ok(url) = url::Url::parse(location)
        && matches!(url.scheme(), "http" | "https")
        && (url.password().is_some() || url.query().is_some())
    {
        return false;
    }
    true
}
type Values = BTreeMap<String, (Vec<String>, Vec<String>)>;
fn parse(value: &str) -> Result<Values, Error> {
    let mut values = Values::new();
    for record in value.split('\0').filter(|s| !s.is_empty()) {
        let (key, value) = record
            .split_once('\n')
            .ok_or(Error::Code("ActionUnsupportedRemote"))?;
        let (name, field) = key
            .strip_prefix("remote.")
            .and_then(|s| s.rsplit_once('.'))
            .filter(|(name, field)| !name.is_empty() && matches!(*field, "url" | "pushurl"))
            .ok_or(Error::Code("ActionUnsupportedRemote"))?;
        let lists = values.entry(name.to_owned()).or_default();
        if field == "url" {
            &mut lists.0
        } else {
            &mut lists.1
        }
        .push(value.to_owned());
    }
    Ok(values)
}
impl Git {
    fn remote_values(
        &self,
        path: &Path,
        local: bool,
        cancel: &Cancellation,
    ) -> Result<Values, Error> {
        let mut args = vec!["config"];
        if local {
            args.push("--local");
        }
        args.extend(["--null", "--get-regexp", r"^remote\..*\.(url|pushurl)$"]);
        let result = process::run(
            &self.request(path, args, Lifetime::Read, 1024 * 1024),
            cancel,
            None,
        )?;
        if result.outcome != process::Outcome::Exited || !matches!(result.exit_code, 0 | 1) {
            return Err(Error::Code("ActionReadFailed"));
        }
        parse(
            std::str::from_utf8(&result.stdout)
                .map_err(|_| Error::Code("ActionUnsupportedRemote"))?,
        )
    }
    pub fn remote_configuration(
        &self,
        expected: &Snapshot,
        cancel: &Cancellation,
    ) -> Result<Configuration, Error> {
        let path = &expected.worktree;
        if identity::read(path)? != expected.identity {
            return Err(Error::Code("ActionIdentityChanged"));
        }
        let stamp = self.config_stamp(path, cancel)?;
        let local = self.remote_values(path, true, cancel)?;
        let effective = self.remote_values(path, false, cancel)?;
        if local.len() > 32 {
            return Err(Error::Code("ActionUnsupportedRemote"));
        }
        let mut remotes: Vec<_> = local
            .into_iter()
            .map(|(name, (fetch, push))| {
                let location = fetch.join("\n");
                let push_location = push.join("\n");
                let clean = redact(&location);
                let clean_push = redact(&push_location);
                let can_edit = fetch.len() == 1
                    && push.len() <= 1
                    && clean == location
                    && clean_push == push_location
                    && effective
                        .get(&name)
                        .is_some_and(|v| v.0.len() == fetch.len() && v.1.len() == push.len())
                    && valid(&name, &location)
                    && (push_location.is_empty() || valid(&name, &push_location));
                Remote {
                    name,
                    location: clean,
                    push_location: clean_push,
                    can_edit,
                }
            })
            .collect();
        // StringComparer.Ordinal in the pinned C# form compares UTF-16 code units.
        remotes.sort_by(|a, b| a.name.encode_utf16().cmp(b.name.encode_utf16()));
        if stamp != self.config_stamp(path, cancel)? {
            return Err(Error::Code("RemoteManageChanged"));
        }
        if identity::read(path)? != expected.identity {
            return Err(Error::Code("ActionIdentityChanged"));
        }
        Ok(Configuration { remotes, stamp })
    }
    pub fn execute_remote(
        &self,
        expected: &Snapshot,
        action: &Action,
        progress: Option<Progress>,
    ) -> Result<Completed, Error> {
        let path = &expected.worktree;
        let cancel = Cancellation::default();
        // Add has no pre-open I/O: prepare its digest only after submission. Edit
        // and remove keep the digest of the cached values the user reviewed.
        if identity::read(path)? != expected.identity {
            return Err(Error::Code("ActionIdentityChanged"));
        }
        let stamp = match action {
            Action::Add { .. } => self.config_stamp(path, &cancel)?,
            Action::Change { reviewed, .. } => reviewed.stamp.clone(),
        };
        let _gate = MutationGate::acquire(&expected.common_directory, false)?;
        if identity::read(path)? != expected.identity {
            return Err(Error::Code("ActionIdentityChanged"));
        }
        let mut commands: Vec<Vec<String>> = Vec::new();
        match action {
            Action::Add { name, location } => {
                if !valid(name, location) {
                    return Err(Error::Code("RemoteAddInvalid"));
                }
                self.local_read(
                    path,
                    &["check-ref-format", &format!("refs/remotes/{name}/probe")],
                    &cancel,
                )?;
                commands.push(
                    ["remote", "add", "--", name, location]
                        .map(str::to_owned)
                        .to_vec(),
                );
            }
            Action::Change {
                reviewed,
                name,
                location,
                push_location,
                remove,
            } => {
                let remote = reviewed
                    .remotes
                    .iter()
                    .find(|r| r.name == *name)
                    .ok_or(Error::Code("RemoteManageChanged"))?;
                if !valid(name, "placeholder") {
                    return Err(Error::Code("RemoteAddInvalid"));
                }
                if *remove {
                    commands.push(["remote", "remove", "--", name].map(str::to_owned).to_vec());
                } else {
                    if !remote.can_edit
                        || !valid(name, location)
                        || !push_location.is_empty() && !valid(name, push_location)
                    {
                        return Err(Error::Code("RemoteManageUnsupported"));
                    }
                    if location != &remote.location {
                        commands.push(
                            ["remote", "set-url", "--", name, location]
                                .map(str::to_owned)
                                .to_vec(),
                        );
                    }
                    if push_location != &remote.push_location {
                        commands.push(if push_location.is_empty() {
                            [
                                "config",
                                "--local",
                                "--unset-all",
                                "--",
                                &format!("remote.{name}.pushurl"),
                            ]
                            .map(str::to_owned)
                            .to_vec()
                        } else {
                            ["remote", "set-url", "--push", "--", name, push_location]
                                .map(str::to_owned)
                                .to_vec()
                        });
                    }
                }
            }
        }
        if stamp != self.config_stamp(path, &cancel)? {
            return Err(Error::Code(if matches!(action, Action::Add { .. }) {
                "BranchManageChanged"
            } else {
                "RemoteManageChanged"
            }));
        }
        let unchanged = commands.is_empty();
        let mut transcript = Transcript::default();
        let result = (|| {
            for command in commands {
                if let Some(progress) = &progress {
                    progress(Stage::UpdatingRemote);
                }
                if !transcript.write(self, path, command)? {
                    break;
                }
            }
            Ok(None)
        })();
        if let Some(progress) = &progress {
            progress(Stage::Reconciling);
        }
        let mut completed = transcript.finish(self, path, result)?;
        completed.preserves_worktree = true;
        completed.unchanged = unchanged;
        Ok(completed)
    }
}
