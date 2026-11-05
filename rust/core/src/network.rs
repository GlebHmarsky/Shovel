//! Explicit native Git network actions. Preparation and execution belong to a
//! repository worker; menus use cached snapshots and never call these methods.
use crate::{
    authentication,
    git::{Git, Snapshot},
    identity,
    process::{self, Cancellation, Lifetime},
    working::MutationGate,
};
use std::{
    collections::{BTreeMap, HashSet},
    ffi::OsString,
    path::Path,
    sync::Arc,
};

const LIMIT: usize = 1024 * 1024;
#[derive(Clone, Debug)]
pub struct Remote {
    pub name: String,
    pub fetch_address: String,
    pub push_address: String,
}
#[derive(Clone, Debug)]
pub struct Context {
    pub repository: Snapshot,
    pub remotes: Vec<Remote>,
    pub config_stamp: String,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Strategy {
    Rebase,
    Merge,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Target {
    pub remote: String,
    pub branch: String,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PushTarget {
    pub source: String,
    pub destination: Target,
}
#[derive(Clone, Debug)]
pub enum Action {
    Fetch {
        remote: Option<String>,
        all: bool,
    },
    Pull {
        target: Option<Target>,
        strategy: Strategy,
        auto_stash: bool,
    },
    Push {
        target: Option<PushTarget>,
        all_tags: bool,
        set_upstream: bool,
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    Validating,
    Fetching,
    CheckingFetchedTip,
    Rebasing,
    Merging,
    Pushing,
    Reconciling,
}
pub type Progress = Arc<dyn Fn(Stage) + Send + Sync>;
pub use crate::operation::{Command, Completed, Error};
fn decode(bytes: &[u8]) -> Result<String, Error> {
    String::from_utf8(bytes.to_vec()).map_err(|_| Error::Code("GitErrorInvalidOutput"))
}
fn trim(value: &str) -> &str {
    value.trim_end_matches(['\r', '\n'])
}
fn oid(value: &str) -> bool {
    matches!(value.len(), 40 | 64) && value.bytes().all(|b| b.is_ascii_hexdigit())
}
// Redact complete captured output before it is exposed. Streaming chunks are
// deliberately not forwarded: credentials can straddle arbitrary pipe chunks.
pub fn redact(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let lower = text.to_ascii_lowercase();
    let mut remaining = text;
    loop {
        let offset = text.len() - remaining.len();
        let start = [
            lower[offset..].find("https://"),
            lower[offset..].find("http://"),
        ]
        .into_iter()
        .flatten()
        .min();
        let Some(start) = start else {
            result.push_str(remaining);
            break;
        };
        result.push_str(&remaining[..start]);
        remaining = &remaining[start..];
        let end = remaining
            .find(char::is_whitespace)
            .unwrap_or(remaining.len());
        let url = &remaining[..end];
        let scheme = url.find("://").unwrap() + 3;
        result.push_str(&url[..scheme]);
        let tail = &url[scheme..];
        let auth_end = tail.find(['/', '?', '#']).unwrap_or(tail.len());
        let tail = if let Some(at) = tail[..auth_end].rfind('@') {
            result.push_str("[redacted]@");
            &tail[at + 1..]
        } else {
            tail
        };
        if let Some(query) = tail.find(['?', '#']) {
            result.push_str(&tail[..query]);
            result.push_str("?[redacted]");
        } else {
            result.push_str(tail);
        }
        remaining = &remaining[end..];
    }
    result
}
fn append(output: &mut String, value: &str, truncated: &mut bool) {
    let mut available = LIMIT.saturating_sub(output.len()).min(value.len());
    while !value.is_char_boundary(available) {
        available -= 1;
    }
    output.push_str(&value[..available]);
    *truncated |= available < value.len();
}
fn overlap(existing: &[String], incoming: &[String]) -> bool {
    let existing = existing
        .iter()
        .map(|p| p.trim_end_matches('/').to_uppercase())
        .collect::<HashSet<_>>();
    let mut directories = HashSet::new();
    for path in &existing {
        for (index, _) in path.match_indices('/') {
            directories.insert(&path[..index]);
        }
    }
    incoming.iter().any(|path| {
        let path = path.to_uppercase();
        existing.contains(&path)
            || directories.contains(path.as_str())
            || path
                .match_indices('/')
                .any(|(index, _)| existing.contains(&path[..index]))
    })
}

impl Git {
    fn network_read(
        &self,
        root: &Path,
        args: &[&str],
        cancel: &Cancellation,
    ) -> Result<String, Error> {
        decode(&self.read(root, args.iter().copied(), cancel, LIMIT)?.stdout)
    }
    fn network_config(
        &self,
        root: &Path,
        key: &str,
        cancel: &Cancellation,
    ) -> Result<Option<String>, Error> {
        let result = process::run(
            &self.request(root, ["config", "--get-all", key], Lifetime::Read, LIMIT),
            cancel,
            None,
        )?;
        if result.outcome != process::Outcome::Exited || result.exit_code > 1 {
            return Err(Error::Code("ActionReadFailed"));
        }
        if result.exit_code == 1 {
            return Ok(None);
        }
        Ok(Some(trim(&decode(&result.stdout)?).to_owned()))
    }
    fn network_branch(&self, root: &Path, name: &str, cancel: &Cancellation) -> Result<(), Error> {
        if name.trim().is_empty()
            || name.starts_with('-')
            || name == "HEAD"
            || name.contains('@')
            || name.chars().any(char::is_control)
        {
            return Err(Error::Code("ActionBranchName"));
        }
        self.network_read(
            root,
            &["check-ref-format", &format!("refs/heads/{name}")],
            cancel,
        )
        .map_err(|_| Error::Code("ActionBranchName"))?;
        Ok(())
    }
    pub fn prepare_network(&self, root: &Path, cancel: &Cancellation) -> Result<Context, Error> {
        let stamp = self.config_stamp(root, cancel)?;
        let repository = self.inspect(root, cancel)?;
        let mut remotes = Vec::new();
        for name in &repository.remotes {
            if remotes.len() >= 32
                || name.is_empty()
                || name.starts_with('-')
                || name.chars().any(char::is_control)
            {
                return Err(Error::Code("ActionUnsupportedRemote"));
            }
            self.network_read(
                root,
                &["check-ref-format", &format!("refs/remotes/{name}/check")],
                cancel,
            )?;
            let fetch = self.network_read(root, &["remote", "get-url", "--all", name], cancel)?;
            let push = self.network_read(
                root,
                &["remote", "get-url", "--push", "--all", name],
                cancel,
            )?;
            let fetch = trim(&fetch);
            let push = trim(&push);
            if fetch.is_empty()
                || push.is_empty()
                || fetch.contains(['\n', '\r'])
                || push.contains(['\n', '\r'])
            {
                return Err(Error::Code("ActionUnsupportedRemote"));
            }
            remotes.push(Remote {
                name: name.clone(),
                fetch_address: redact(fetch),
                push_address: redact(push),
            });
        }
        if self.config_stamp(root, cancel)? != stamp || identity::read(root)? != repository.identity
        {
            return Err(Error::Code("ActionChanged"));
        }
        Ok(Context {
            repository,
            remotes,
            config_stamp: stamp,
        })
    }
    fn validate_network(
        &self,
        context: &Context,
        refs: bool,
        cancel: &Cancellation,
    ) -> Result<Snapshot, Error> {
        let root = &context.repository.worktree;
        if identity::read(root)? != context.repository.identity {
            return Err(Error::Code("ActionIdentityChanged"));
        }
        let fresh = self.inspect(root, cancel)?;
        if fresh.branch != context.repository.branch
            || fresh.head != context.repository.head
            || refs && fresh.fingerprint != context.repository.fingerprint
            || self.config_stamp(root, cancel)? != context.config_stamp
        {
            return Err(Error::Code("ActionChanged"));
        }
        if !fresh.operations.is_empty() {
            return Err(Error::Code("ActionSequence"));
        }
        Ok(fresh)
    }
    fn require_remote(
        &self,
        context: &Context,
        name: &str,
        code: &'static str,
    ) -> Result<(), Error> {
        if context.remotes.iter().any(|r| r.name == name) {
            Ok(())
        } else {
            Err(Error::Code(code))
        }
    }
    pub fn default_fetch(&self, context: &Context, cancel: &Cancellation) -> Result<String, Error> {
        let configured = match &context.repository.branch {
            Some(branch) => self.network_config(
                &context.repository.worktree,
                &format!("branch.{branch}.remote"),
                cancel,
            )?,
            None => None,
        };
        let name = configured.unwrap_or_else(|| {
            if context.remotes.iter().any(|r| r.name == "origin") {
                "origin".into()
            } else if context.remotes.len() == 1 {
                context.remotes[0].name.clone()
            } else {
                String::new()
            }
        });
        self.require_remote(context, &name, "DefaultFetchNeedsTarget")?;
        Ok(name)
    }
    pub fn default_pull(&self, context: &Context, cancel: &Cancellation) -> Result<Target, Error> {
        let branch = context
            .repository
            .branch
            .as_ref()
            .ok_or(Error::Code("ActionPullNeedsBranch"))?;
        let local = format!("refs/heads/{branch}");
        let output = self.network_read(
            &context.repository.worktree,
            &[
                "for-each-ref",
                "--count=1",
                "--format=%(refname)%00%(upstream:remotename)%00%(upstream:remoteref)",
                "--",
                &local,
            ],
            cancel,
        )?;
        let fields = trim(&output).split('\0').collect::<Vec<_>>();
        if fields.len() != 3 || fields[0] != local {
            return Err(Error::Code("PullDefaultTargetChanged"));
        }
        self.require_remote(context, fields[1], "PullDefaultTargetChanged")?;
        let branch = fields[2]
            .strip_prefix("refs/heads/")
            .ok_or(Error::Code("PullDefaultTargetChanged"))?;
        Ok(Target {
            remote: fields[1].into(),
            branch: branch.into(),
        })
    }
    pub fn default_push(
        &self,
        context: &Context,
        cancel: &Cancellation,
    ) -> Result<PushTarget, Error> {
        let root = &context.repository.worktree;
        let branch = context
            .repository
            .branch
            .as_ref()
            .ok_or(Error::Code("DefaultPushNeedsTarget"))?;
        let local = format!("refs/heads/{branch}");
        let output = self.network_read(root, &["for-each-ref", "--count=1", "--format=%(refname)%00%(push:remotename)%00%(upstream:remotename)%00%(upstream:remoteref)", "--", &local], cancel)?;
        let fields = trim(&output).split('\0').collect::<Vec<_>>();
        if fields.len() != 4 || fields[0] != local {
            return Err(Error::Code("DefaultPushNeedsTarget"));
        }
        let mut remote = fields[1].to_owned();
        if remote.is_empty() {
            for key in [
                format!("branch.{branch}.pushRemote"),
                "remote.pushDefault".into(),
                format!("branch.{branch}.remote"),
            ] {
                if let Some(value) = self.network_config(root, &key, cancel)? {
                    remote = value;
                    break;
                }
            }
            if remote.is_empty() {
                remote = "origin".into();
            }
        }
        self.require_remote(context, &remote, "DefaultPushNeedsTarget")?;
        if self
            .network_config(root, &format!("remote.{remote}.push"), cancel)?
            .is_some()
        {
            return Err(Error::Code("DefaultPushNeedsTarget"));
        }
        let mode = self
            .network_config(root, "push.default", cancel)?
            .unwrap_or("simple".into());
        let destination = match mode.as_str() {
            "current" => &local,
            "upstream" | "tracking" if remote == fields[2] => fields[3],
            "simple" if !fields[2].is_empty() && remote != fields[2] => &local,
            "simple" if remote == fields[2] && fields[3] == local => &local,
            _ => "",
        };
        let destination = destination
            .strip_prefix("refs/heads/")
            .ok_or(Error::Code("DefaultPushNeedsTarget"))?;
        Ok(PushTarget {
            source: branch.clone(),
            destination: Target {
                remote,
                branch: destination.into(),
            },
        })
    }
    pub fn network_environment(
        &self,
        root: &Path,
        auth: Option<&authentication::Session>,
        cancel: &Cancellation,
    ) -> Result<BTreeMap<OsString, Option<OsString>>, Error> {
        if self
            .network_config(root, "core.sshCommand", cancel)?
            .is_some()
        {
            return Err(Error::Code("ActionCustomSsh"));
        }
        let mut install = self
            .executable
            .parent()
            .and_then(Path::parent)
            .ok_or(Error::Code("ActionSshMissing"))?;
        if install.file_name().is_some_and(|name| {
            ["ucrt64", "mingw64", "mingw32"]
                .iter()
                .any(|n| name.eq_ignore_ascii_case(n))
        }) {
            install = install.parent().ok_or(Error::Code("ActionSshMissing"))?;
        }
        let ssh = install.join("usr/bin/ssh.exe");
        if !ssh.is_file() {
            return Err(Error::Code("ActionSshMissing"));
        }
        let mut environment = auth.map(|a| a.environment.clone()).unwrap_or_default();
        let path = ssh
            .to_string_lossy()
            .replace('\\', "/")
            .replace('\'', "'\\''");
        let options = if auth.is_some() {
            "-oBatchMode=no -oStrictHostKeyChecking=ask"
        } else {
            "-oBatchMode=yes -oStrictHostKeyChecking=yes"
        };
        environment.insert(
            "GIT_SSH_COMMAND".into(),
            Some(format!("'{path}' {options}").into()),
        );
        environment.insert("GIT_SSH_VARIANT".into(), Some("ssh".into()));
        environment.insert("GIT_MERGE_AUTOEDIT".into(), Some("no".into()));
        environment.insert("GIT_EDITOR".into(), Some("false".into()));
        Ok(environment)
    }
    pub fn execute_network(
        &self,
        context: &Context,
        action: &Action,
        auth: Option<&authentication::Session>,
        progress: Option<Progress>,
    ) -> Result<Completed, Error> {
        let cancel = Cancellation::default(); // Repository-owned accepted work.
        let stage = |value| {
            if let Some(callback) = &progress {
                callback(value);
            }
        };
        stage(Stage::Validating);
        let _gate = MutationGate::acquire(
            &context.repository.common_directory,
            matches!(action, Action::Fetch { .. }),
        )?;
        let _fetch_gate = if matches!(action, Action::Pull { .. }) {
            Some(MutationGate::acquire(
                &context.repository.common_directory,
                true,
            )?)
        } else {
            None
        };
        self.validate_network(context, true, &cancel)?;
        let root = &context.repository.worktree;
        let environment = self.network_environment(root, auth, &cancel)?;
        let mut commands = Vec::new();
        let mut output = String::new();
        let mut output_truncated = false;
        let mut write = |args: Vec<String>| -> Result<bool, Error> {
            let mut request = self.request(
                root,
                [
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
                ],
                Lifetime::AcceptedCommand,
                LIMIT,
            );
            request
                .arguments
                .extend(args.into_iter().map(OsString::from));
            request.environment.extend(environment.clone());
            let result = process::run(&request, &cancel, None)?;
            let command = Command {
                outcome: result.outcome,
                exit_code: result.exit_code,
                elapsed: result.elapsed,
            };
            let succeeded = command.succeeded();
            output_truncated |= result.outcome == process::Outcome::OutputLimit;
            append(
                &mut output,
                &redact(&format!(
                    "{}{}",
                    String::from_utf8_lossy(&result.stdout),
                    String::from_utf8_lossy(&result.stderr)
                )),
                &mut output_truncated,
            );
            commands.push(command);
            Ok(succeeded)
        };
        let fetch = |remote: &str, reference: String| {
            [
                "fetch",
                "--progress",
                "--no-recurse-submodules",
                "--no-prune",
                "--no-prune-tags",
                "--no-auto-maintenance",
                "--refmap=",
                "--",
                remote,
                &reference,
            ]
            .map(str::to_owned)
            .to_vec()
        };
        let execution = (|| -> Result<(), Error> {
            match action {
                Action::Fetch { remote, all } => {
                    let selected = remote
                        .clone()
                        .map(Ok)
                        .unwrap_or_else(|| self.default_fetch(context, &cancel))?;
                    self.require_remote(context, &selected, "ActionNoRemote")?;
                    self.validate_network(context, true, &cancel)?;
                    stage(Stage::Fetching);
                    for remote in context
                        .remotes
                        .iter()
                        .filter(|r| *all || r.name == selected)
                    {
                        if !write(fetch(
                            &remote.name,
                            format!("+refs/heads/*:refs/remotes/{}/*", remote.name),
                        ))? {
                            break;
                        }
                    }
                }
                Action::Push {
                    target,
                    all_tags,
                    set_upstream,
                } => {
                    let target = target
                        .clone()
                        .map(Ok)
                        .unwrap_or_else(|| self.default_push(context, &cancel))?;
                    self.require_remote(context, &target.destination.remote, "ActionNoRemote")?;
                    self.network_branch(root, &target.source, &cancel)?;
                    self.network_branch(root, &target.destination.branch, &cancel)?;
                    if !context
                        .repository
                        .references
                        .iter()
                        .any(|r| r.name == format!("refs/heads/{}", target.source))
                    {
                        return Err(Error::Code("ActionChanged"));
                    }
                    let mut args = vec![
                        "-c".into(),
                        format!("remote.{}.mirror=false", target.destination.remote),
                    ];
                    args.extend(
                        [
                            "push",
                            "--porcelain",
                            "--progress",
                            "--no-force",
                            "--no-follow-tags",
                            "--recurse-submodules=no",
                        ]
                        .map(str::to_owned),
                    );
                    if *all_tags {
                        args.push("--tags".into());
                    }
                    if *set_upstream {
                        args.push("--set-upstream".into());
                    }
                    args.extend([
                        "--".into(),
                        target.destination.remote,
                        format!(
                            "refs/heads/{}:refs/heads/{}",
                            target.source, target.destination.branch
                        ),
                    ]);
                    self.validate_network(context, true, &cancel)?;
                    stage(Stage::Pushing);
                    write(args)?;
                }
                Action::Pull {
                    target,
                    strategy,
                    auto_stash,
                } => {
                    let target = target
                        .clone()
                        .map(Ok)
                        .unwrap_or_else(|| self.default_pull(context, &cancel))?;
                    self.require_remote(context, &target.remote, "ActionNoRemote")?;
                    self.network_branch(root, &target.branch, &cancel)?;
                    let branch = context
                        .repository
                        .branch
                        .as_ref()
                        .ok_or(Error::Code("ActionAttachedRequired"))?;
                    let head = context
                        .repository
                        .head
                        .as_ref()
                        .ok_or(Error::Code("ActionAttachedRequired"))?;
                    self.validate_network(context, true, &cancel)?;
                    stage(Stage::Fetching);
                    if !write(fetch(
                        &target.remote,
                        format!(
                            "+refs/heads/{}:refs/remotes/{}/{}",
                            target.branch, target.remote, target.branch
                        ),
                    ))? {
                        return Ok(());
                    }
                    stage(Stage::CheckingFetchedTip);
                    self.validate_network(context, false, &cancel)
                        .map_err(|_| Error::Code("ActionChangedAfterFetch"))?;
                    let tip = self.network_read(
                        root,
                        &["rev-parse", "--verify", "FETCH_HEAD^{commit}"],
                        &cancel,
                    )?;
                    let tip = trim(&tip);
                    let tracking = self.network_read(
                        root,
                        &[
                            "rev-parse",
                            "--verify",
                            &format!(
                                "refs/remotes/{}/{}^{{commit}}",
                                target.remote, target.branch
                            ),
                        ],
                        &cancel,
                    )?;
                    if !oid(tip) || tip != trim(&tracking) {
                        return Err(Error::Code("ActionChangedAfterFetch"));
                    }
                    let stash = if *auto_stash {
                        "--autostash"
                    } else {
                        "--no-autostash"
                    };
                    if *strategy == Strategy::Rebase {
                        let additions = self.network_read(
                            root,
                            &[
                                "diff",
                                "--no-ext-diff",
                                "--no-textconv",
                                "--no-renames",
                                "--name-only",
                                "--diff-filter=A",
                                "-z",
                                head,
                                tip,
                                "--",
                            ],
                            &cancel,
                        )?;
                        let additions = additions
                            .split('\0')
                            .filter(|s| !s.is_empty())
                            .map(str::to_owned)
                            .collect::<Vec<_>>();
                        if !additions.is_empty() {
                            let ignored = self.network_read(
                                root,
                                &[
                                    "ls-files",
                                    "--others",
                                    "--ignored",
                                    "--exclude-standard",
                                    "--directory",
                                    "--no-empty-directory",
                                    "-z",
                                ],
                                &cancel,
                            )?;
                            let ignored = ignored
                                .split('\0')
                                .filter(|s| !s.is_empty())
                                .map(str::to_owned)
                                .collect::<Vec<_>>();
                            if overlap(&ignored, &additions) {
                                return Err(Error::Code("ActionIgnoredCollision"));
                            }
                        }
                        stage(Stage::Rebasing);
                        write(
                            [
                                "-c",
                                "submodule.recurse=false",
                                "rebase",
                                "--merge",
                                "--no-fork-point",
                                "--no-update-refs",
                                "--no-rebase-merges",
                                stash,
                                tip,
                            ]
                            .map(str::to_owned)
                            .to_vec(),
                        )?;
                    } else {
                        stage(Stage::Merging);
                        write(
                            [
                                "-c",
                                "submodule.recurse=false",
                                "-c",
                                &format!("branch.{branch}.mergeOptions="),
                                "merge",
                                "--ff",
                                "--no-squash",
                                "--commit",
                                "--no-overwrite-ignore",
                                stash,
                                "--no-edit",
                                tip,
                            ]
                            .map(str::to_owned)
                            .to_vec(),
                        )?;
                    }
                }
            }
            Ok(())
        })();
        let message = match execution {
            Err(error) if commands.is_empty() => return Err(error),
            Err(error) => Some(error.code()),
            Ok(()) => None,
        };
        stage(Stage::Reconciling);
        let updated = self.inspect(root, &cancel).map_err(Error::from);
        let unmerged = self.network_read(root, &["ls-files", "--unmerged", "-z"], &cancel);
        let message = message.or_else(|| unmerged.as_ref().err().map(Error::code));
        let has_conflicts = unmerged.is_ok_and(|data| !data.is_empty());
        Ok(Completed {
            commands,
            output,
            output_truncated,
            message,
            warning: None,
            has_conflicts,
            preserves_worktree: false,
            updated,
            authentication_failure: auth.and_then(authentication::Session::failure),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn credentials_queries_and_path_collisions() {
        assert_eq!(
            redact("fatal https://u:p@host/a?token=secret\nHTTP://u@other/x#secret\n"),
            "fatal https://[redacted]@host/a?[redacted]\nHTTP://[redacted]@other/x?[redacted]\n"
        );
        assert!(overlap(&["Build/".into()], &["build/a".into()]));
        assert!(overlap(&["build/a".into()], &["Build".into()]));
        assert!(!overlap(&["builder/a".into()], &["Build".into()]));
    }
}
