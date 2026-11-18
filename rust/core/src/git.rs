//! Native Git reads and immutable history snapshots. UI owners schedule these
//! blocking calls on workers and discard obsolete generations.
use crate::graph;
use crate::process::{self, Cancellation, Lifetime, Request};
use chrono::{DateTime, FixedOffset};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashSet},
    ffi::{OsStr, OsString},
    fs,
    io::Read,
    path::{Path, PathBuf},
};

const MAX_HISTORY: usize = 4 * 1024 * 1024;
pub const PAGE_SIZE: usize = 200;
const FORMAT: &str = "--format=tformat:%H%x00%P%x00%an%x00%ae%x00%aI%x00%cn%x00%ce%x00%cI%x00%B";
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Git for Windows 2.47 or newer is required")]
    MissingGit,
    #[error("Git could not read the repository")]
    Command {
        outcome: process::Outcome,
        code: u32,
        output: String,
    },
    #[error("Git returned unsupported or malformed data")]
    Invalid,
    #[error("the selected directory is not a working-tree root")]
    NotWorktree,
    #[error("repository state changed during the read; refresh to retry")]
    Changed,
    #[error("repository data exceeds the supported read limit")]
    Limit,
    #[error("repository data could not be read")]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Graph(#[from] graph::GraphError),
}
#[derive(Clone, Debug)]
pub struct Git {
    pub executable: PathBuf,
    pub version: String,
    pub environment: BTreeMap<OsString, Option<OsString>>,
    foreground_reads: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Reference {
    pub name: String,
    pub hash: String,
    pub upstream: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Snapshot {
    pub identity: crate::identity::Identity,
    pub worktree: PathBuf,
    pub git_directory: PathBuf,
    pub common_directory: PathBuf,
    pub branch: Option<String>,
    pub head: Option<String>,
    pub upstream: Option<String>,
    #[serde(default)]
    pub upstream_target: Option<crate::network::Target>,
    pub remotes: Vec<String>,
    pub operations: Vec<String>,
    #[serde(default)]
    pub recovery: Option<crate::recovery::State>,
    pub shallow: bool,
    pub fingerprint: String,
    pub references: Vec<Reference>,
}
impl Snapshot {
    fn with_directories(
        identity: crate::identity::Identity,
        worktree: PathBuf,
        git_directory: PathBuf,
        common_directory: PathBuf,
    ) -> Self {
        Self {
            identity,
            worktree,
            git_directory,
            common_directory,
            branch: None,
            head: None,
            upstream: None,
            upstream_target: None,
            remotes: vec![],
            operations: vec![],
            recovery: None,
            shallow: false,
            fingerprint: String::new(),
            references: vec![],
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Commit {
    pub hash: String,
    pub parents: Vec<String>,
    pub author: String,
    pub email: String,
    pub authored: DateTime<FixedOffset>,
    pub committer: String,
    pub committer_email: String,
    pub committed: DateTime<FixedOffset>,
    pub message: String,
}
impl Commit {
    pub fn subject(&self) -> &str {
        self.message.lines().next().unwrap_or("")
    }
}
#[derive(Clone, Debug)]
pub struct History {
    pub snapshot: Snapshot,
    pub stashes: Vec<Commit>,
    hidden: HashSet<String>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Cursor {
    pub offset: usize,
    pub raw_offset: usize,
    pub inserted_offset: usize,
    pub expected: Option<String>,
    pub graph: graph::Cursor,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Entry {
    pub commit: Commit,
    pub graph: graph::Row,
    pub references: Vec<Reference>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Page {
    pub entries: Vec<Entry>,
    pub cursor: Cursor,
    pub next: Option<Cursor>,
}

pub enum HistoryRead {
    Unchanged(Box<Snapshot>),
    Page(Box<History>, Box<Page>),
}

impl HistoryRead {
    pub fn snapshot(&self) -> &Snapshot {
        match self {
            Self::Unchanged(snapshot) => snapshot,
            Self::Page(history, _) => &history.snapshot,
        }
    }

    fn apply_directories(&mut self, native: Snapshot) {
        let snapshot = match self {
            Self::Unchanged(snapshot) => snapshot.as_mut(),
            Self::Page(history, _) => &mut history.snapshot,
        };
        snapshot.worktree = native.worktree;
        snapshot.git_directory = native.git_directory;
        snapshot.common_directory = native.common_directory;
        snapshot.remotes = native.remotes;
    }
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}
fn oid(value: &str) -> bool {
    (4..=128).contains(&value.len()) && value.bytes().all(|b| b.is_ascii_hexdigit())
}
fn lines(bytes: &[u8]) -> Vec<String> {
    text(bytes).lines().map(str::to_owned).collect()
}

impl Git {
    pub fn discover(explicit: Option<&Path>, cancel: &Cancellation) -> Result<Self, Error> {
        let mut candidates = Vec::new();
        if let Some(path) = explicit {
            candidates.push(path.to_path_buf());
        } else {
            for root in [
                std::env::var_os("ProgramFiles").map(|p| PathBuf::from(p).join("Git")),
                std::env::var_os("LOCALAPPDATA").map(|p| PathBuf::from(p).join("Programs/Git")),
            ]
            .into_iter()
            .flatten()
            {
                for relative in [
                    "ucrt64/bin/git.exe",
                    "mingw64/bin/git.exe",
                    "mingw32/bin/git.exe",
                    "cmd/git.exe",
                ] {
                    candidates.push(root.join(relative));
                }
            }
        }
        Self::discover_candidates(candidates, cancel)
    }

    fn discover_candidates(
        candidates: impl IntoIterator<Item = PathBuf>,
        cancel: &Cancellation,
    ) -> Result<Self, Error> {
        let mut last_error = Error::MissingGit;
        for path in candidates {
            if !path.is_absolute()
                || !path
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("exe"))
                || !path.is_file()
            {
                continue;
            }
            match Self::probe_executable(path, cancel) {
                Ok(git) => return Ok(git),
                Err(error) if cancel.is_cancelled() => return Err(error),
                Err(error) => last_error = error,
            }
        }
        Err(last_error)
    }

    fn probe_executable(executable: PathBuf, cancel: &Cancellation) -> Result<Self, Error> {
        let mut git = Self {
            executable,
            version: String::new(),
            environment: BTreeMap::new(),
            foreground_reads: false,
        };
        let directory = git.executable.parent().ok_or(Error::MissingGit)?;
        let output = git.read(directory, ["--version"], cancel, 4096)?;
        let version = text(&output.stdout)
            .trim()
            .strip_prefix("git version ")
            .ok_or(Error::MissingGit)?
            .to_owned();
        let parts = version
            .split('.')
            .take(3)
            .map(str::parse::<u32>)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| Error::MissingGit)?;
        if parts.len() != 3 || parts.as_slice() < [2, 47, 0].as_slice() {
            return Err(Error::MissingGit);
        }
        git.version = version;
        Ok(git)
    }

    /// Give reads for the current interaction first access to free read slots.
    pub fn with_foreground_reads(mut self, foreground: bool) -> Self {
        self.foreground_reads = foreground;
        self
    }

    pub fn request(
        &self,
        root: &Path,
        args: impl IntoIterator<Item = impl Into<OsString>>,
        lifetime: Lifetime,
        max: usize,
    ) -> Request {
        let mut request = Request::new(
            self.executable.clone(),
            root.to_path_buf(),
            [
                "--no-pager",
                "--no-lazy-fetch",
                "-c",
                "gc.auto=0",
                "-c",
                "maintenance.auto=false",
            ],
        );
        if lifetime == Lifetime::Read {
            request
                .arguments
                .extend(["--no-optional-locks", "-c", "core.fsmonitor=false"].map(OsString::from));
        }
        request.arguments.extend(args.into_iter().map(Into::into));
        request.capture_limit = max;
        request.lifetime = lifetime;
        request.foreground_read = self.foreground_reads;
        for (name, _) in std::env::vars_os() {
            let key = name.to_string_lossy().to_uppercase();
            if (key.starts_with("GIT_")
                && !matches!(
                    key.as_str(),
                    "GIT_CONFIG_GLOBAL"
                        | "GIT_CONFIG_SYSTEM"
                        | "GIT_CONFIG_NOSYSTEM"
                        | "GIT_SSH"
                        | "GIT_SSH_COMMAND"
                        | "GIT_SSH_VARIANT"
                ))
                || key.starts_with("GCM_TRACE")
                || key.starts_with("SHOVEL_ASKPASS_")
                || key.starts_with("SHOVEL_RUST_ASKPASS_")
            {
                request.environment.insert(name, None);
            }
        }
        request
            .environment
            .insert("GIT_NO_LAZY_FETCH".into(), Some("1".into()));
        for key in ["GIT_ASKPASS", "SSH_ASKPASS"] {
            request.environment.insert(key.into(), Some("".into()));
        }
        request
            .environment
            .insert("SSH_ASKPASS_REQUIRE".into(), Some("never".into()));
        request
            .environment
            .insert("SSH_ASKPASS_PROMPT".into(), None);
        for key in [
            "GCM_DEBUG",
            "GCM_TRACE",
            "GCM_TRACE_SECRETS",
            "GCM_TRACE_MSAUTH",
        ] {
            request.environment.insert(key.into(), Some("0".into()));
        }
        {
            request
                .environment
                .insert("GIT_TERMINAL_PROMPT".into(), Some("0".into()));
            request
                .environment
                .insert("GCM_INTERACTIVE".into(), Some("never".into()));
        }
        if lifetime == Lifetime::Read {
            request
                .environment
                .insert("GIT_OPTIONAL_LOCKS".into(), Some("0".into()));
        }
        request.environment.extend(self.environment.iter().map(|(name, value)| {
            let name = if name.eq_ignore_ascii_case(OsStr::new("GCM_INTERACTIVE")) {
                "GCM_INTERACTIVE".into()
            } else {
                name.clone()
            };
            (name, value.clone())
        }));
        request
    }
    pub fn read(
        &self,
        root: &Path,
        args: impl IntoIterator<Item = impl Into<OsString>>,
        cancel: &Cancellation,
        max: usize,
    ) -> Result<process::Result, Error> {
        let result = process::run(&self.request(root, args, Lifetime::Read, max), cancel, None)?;
        require(result)
    }
    pub(crate) fn inspect_directories(
        &self,
        root: &Path,
        cancel: &Cancellation,
    ) -> Result<Snapshot, Error> {
        let identity = crate::identity::read(root)?;
        let output = self.read(
            root,
            [
                "rev-parse",
                "--path-format=absolute",
                "--is-inside-work-tree",
                "--show-toplevel",
                "--absolute-git-dir",
                "--git-common-dir",
            ],
            cancel,
            32768,
        )?;
        let dirs = lines(&output.stdout);
        if dirs.len() != 4
            || dirs[0] != "true"
            || dirs[1..].iter().any(|p| !Path::new(p).is_absolute())
        {
            return Err(Error::NotWorktree);
        }
        let normalize = |p: &Path| crate::case::path_key(p).trim_end_matches('\\').to_owned();
        if normalize(&identity.root) != normalize(Path::new(&dirs[1]))
            || normalize(&identity.git) != normalize(Path::new(&dirs[2]))
            || normalize(&identity.common) != normalize(Path::new(&dirs[3]))
        {
            return Err(Error::NotWorktree);
        }
        Ok(Snapshot::with_directories(
            identity,
            dirs[1].clone().into(),
            dirs[2].clone().into(),
            dirs[3].clone().into(),
        ))
    }
    pub fn inspect(&self, root: &Path, cancel: &Cancellation) -> Result<Snapshot, Error> {
        self.inspect_metadata(root, self.inspect_directories(root, cancel)?, cancel)
    }
    pub(crate) fn inspect_metadata(
        &self,
        root: &Path,
        mut snapshot: Snapshot,
        cancel: &Cancellation,
    ) -> Result<Snapshot, Error> {
        self.refresh_refs(&mut snapshot, cancel)?;
        self.inspect_remotes(root, snapshot, cancel)
    }

    fn inspect_remotes(
        &self,
        root: &Path,
        mut snapshot: Snapshot,
        cancel: &Cancellation,
    ) -> Result<Snapshot, Error> {
        snapshot.remotes = lines(&self.read(root, ["remote"], cancel, 256 * 1024)?.stdout);
        if crate::identity::read(root)? != snapshot.identity {
            return Err(Error::Changed);
        }
        Ok(snapshot)
    }

    /// Confirm the worktree and expose an optional first page for reading while
    /// the refs and remotes are still being checked. Only the returned history
    /// includes a validated snapshot for repository actions.
    pub fn read_history(
        &self,
        root: &Path,
        identity: &crate::identity::Identity,
        previous_fingerprint: Option<&str>,
        initial_page: Option<&dyn Fn(&Page)>,
        cancel: &Cancellation,
    ) -> Result<HistoryRead, Error> {
        let mut snapshot = Snapshot::with_directories(
            identity.clone(),
            identity.root.clone(),
            identity.git.clone(),
            identity.common.clone(),
        );
        std::thread::scope(|scope| {
            let (confirmed, directory_ready) = std::sync::mpsc::channel();
            let show_initial = initial_page.is_some();
            let directories = std::thread::Builder::new().spawn_scoped(scope, move || {
                let native = self.inspect_directories(root, cancel)?;
                if native.identity != *identity {
                    return Err(Error::Changed);
                }
                if show_initial {
                    let _ = confirmed.send(());
                }
                self.inspect_remotes(root, native, cancel)
            })?;
            let rows = (|| {
                self.refresh_refs(&mut snapshot, cancel)?;
                if previous_fingerprint == Some(snapshot.fingerprint.as_str()) {
                    return Ok(HistoryRead::Unchanged(Box::new(snapshot)));
                }
                let history = self.history(snapshot, cancel)?;
                let verify_worktree = |page: &Page| {
                    if let Some(publish) = initial_page
                        && !page.entries.is_empty()
                        && directory_ready.recv().is_ok()
                        && !cancel.is_cancelled()
                        && crate::identity::read(root).ok().as_ref() == Some(identity)
                    {
                        publish(page);
                    }
                };
                let page = self.page_from_observed_refs(
                    &history,
                    &Cursor::default(),
                    history.snapshot.clone(),
                    Some(&verify_worktree),
                    cancel,
                )?;
                Ok::<_, Error>(HistoryRead::Page(Box::new(history), Box::new(page)))
            })();
            let native = directories
                .join()
                .map_err(|_| std::io::Error::other("repository directory worker stopped"))??;
            let mut read = rows?;
            if crate::identity::read(root)? != *identity {
                return Err(Error::Changed);
            }
            read.apply_directories(native);
            Ok(read)
        })
    }
    /// Check the worktree for app restart without loading refs, stashes or remotes.
    pub fn worktree_allows_restart(
        &self,
        root: &Path,
        expected: Option<&crate::identity::Identity>,
        cancel: &Cancellation,
    ) -> Result<bool, Error> {
        let snapshot = self.inspect_directories(root, cancel)?;
        if expected.is_some_and(|identity| *identity != snapshot.identity)
            || worktree_has_unfinished_operation(&snapshot.git_directory)?
        {
            return Ok(false);
        }
        let unmerged = self.read(root, ["ls-files", "--unmerged", "-z"], cancel, 256 * 1024)?;
        Ok(unmerged.stdout.is_empty()
            && crate::identity::read(root)? == snapshot.identity
            && !worktree_has_unfinished_operation(&snapshot.git_directory)?)
    }

    pub(crate) fn current_index_target(
        &self,
        expected: &Snapshot,
        cancel: &Cancellation,
    ) -> Result<bool, Error> {
        let root = &expected.worktree;
        let current = self.inspect_directories(root, cancel)?;
        if current.identity != expected.identity
            || operation_markers(&current.git_directory) != expected.operations
        {
            return Ok(false);
        }
        // An index action needs HEAD and its symbolic target. Enumerating tags,
        // stash history and remote refs here makes unrelated history part of a write.
        let head = self.read(
            root,
            [
                "for-each-ref",
                "--include-root-refs",
                "--format=%(objectname)%00%(symref)%00%(objecttype)",
                "HEAD",
            ],
            cancel,
            8192,
        )?;
        let bytes = std::str::from_utf8(&head.stdout)
            .map_err(|_| Error::Invalid)?
            .trim_end_matches(['\r', '\n']);
        let matches = if bytes.is_empty() {
            let branch = self.read(
                root,
                ["symbolic-ref", "--quiet", "--short", "HEAD"],
                cancel,
                4096,
            )?;
            expected.head.is_none()
                && Some(text(&branch.stdout).trim_end_matches(['\r', '\n']))
                    == expected.branch.as_deref()
        } else {
            let fields: Vec<_> = bytes.split('\0').collect();
            if fields.len() != 3 || !oid(fields[0]) || fields[2] != "commit" {
                return Err(Error::Invalid);
            }
            let branch = (!fields[1].is_empty())
                .then(|| fields[1].strip_prefix("refs/heads/").unwrap_or(fields[1]));
            Some(fields[0]) == expected.head.as_deref() && branch == expected.branch.as_deref()
        };
        Ok(matches && crate::identity::read(root)? == expected.identity)
    }
    fn refresh_refs(&self, snapshot: &mut Snapshot, cancel: &Cancellation) -> Result<(), Error> {
        if crate::identity::read(&snapshot.worktree)? != snapshot.identity {
            return Err(Error::Changed);
        }
        let raw = self.read(
            &snapshot.worktree,
            [
                "for-each-ref", "--include-root-refs", "--sort=refname",
                "--format=%(refname)%00%(objectname)%00%(objecttype)%00%(*objectname)%00%(*objecttype)%00%(symref)%00%(upstream)%00%(upstream:remotename)%00%(upstream:remoteref)",
                "HEAD", "refs/heads/", "refs/remotes/", "refs/tags/", "refs/replace/", "refs/stash",
            ],
            cancel,
            MAX_HISTORY,
        )?;
        let mut refs = text(&raw.stdout);
        if refs.lines().any(|l| l.starts_with("refs/stash\0")) {
            let stashes = lines(
                &self
                    .read(
                        &snapshot.worktree,
                        [
                            "reflog",
                            "show",
                            "--format=%H",
                            "--max-count=257",
                            "refs/stash",
                            "--",
                        ],
                        cancel,
                        MAX_HISTORY,
                    )?
                    .stdout,
            );
            if stashes.len() > 256 {
                return Err(Error::Limit);
            }
            for (i, hash) in stashes.iter().enumerate() {
                if !oid(hash) {
                    return Err(Error::Invalid);
                }
                refs.push_str(&format!("stash@{{{i}}}\0{hash}\0commit\0\0\0\0\0\0\n"));
            }
        }
        let mut references = Vec::new();
        let mut branch = None;
        let mut head = None;
        let mut target = None;
        let mut upstream_target = None;
        for line in refs.lines() {
            let fields = line.split('\0').collect::<Vec<_>>();
            if fields.len() != 9 || !oid(fields[1]) {
                return Err(Error::Invalid);
            }
            if fields[0] == "HEAD" {
                if head.is_some() || fields[2] != "commit" {
                    return Err(Error::Invalid);
                }
                head = Some(fields[1].to_owned());
                target = (!fields[5].is_empty()).then(|| fields[5].to_owned());
                branch = target
                    .as_ref()
                    .map(|t| t.strip_prefix("refs/heads/").unwrap_or(t).to_owned());
                references.push(Reference {
                    name: "HEAD".into(),
                    hash: fields[1].into(),
                    upstream: None,
                });
                continue;
            }
            if target.as_deref() == Some(fields[0])
                && !matches!(fields[7], "" | ".")
                && let Some(branch) = fields[8].strip_prefix("refs/heads/")
            {
                upstream_target = Some(crate::network::Target {
                    remote: fields[7].into(),
                    branch: branch.into(),
                });
            }
            // A remote symbolic ref is an alias (for example origin/HEAD), not a branch tip.
            if fields[0].starts_with("refs/remotes/") && !fields[5].is_empty() {
                continue;
            }
            if !(fields[0].starts_with("refs/heads/")
                || fields[0].starts_with("refs/remotes/")
                || fields[0].starts_with("refs/tags/")
                || fields[0].starts_with("stash@{"))
            {
                continue;
            }
            let hash = if fields[2] == "commit" {
                Some(fields[1].to_owned())
            } else if fields[4] == "commit" {
                Some(fields[3].to_owned())
            } else if fields[4] == "tag" {
                let result = process::run(
                    &self.request(
                        &snapshot.worktree,
                        [
                            "rev-parse",
                            "--verify",
                            "--quiet",
                            &format!("{}^{{commit}}", fields[1]),
                        ],
                        Lifetime::Read,
                        4096,
                    ),
                    cancel,
                    None,
                )?;
                if result.success() {
                    Some(text(&result.stdout).trim().to_owned())
                } else if result.outcome == process::Outcome::Exited && result.exit_code == 1 {
                    None
                } else {
                    require(result)?;
                    None
                }
            } else {
                None
            };
            if let Some(hash) = hash {
                if !oid(&hash) {
                    return Err(Error::Invalid);
                }
                references.push(Reference {
                    name: fields[0].into(),
                    hash,
                    upstream: (!fields[6].is_empty()).then(|| fields[6].into()),
                });
            }
        }
        if head.is_none() {
            branch = Some(
                text(
                    &self
                        .read(
                            &snapshot.worktree,
                            ["symbolic-ref", "--quiet", "--short", "HEAD"],
                            cancel,
                            4096,
                        )?
                        .stdout,
                )
                .trim()
                .to_owned(),
            );
        }
        let operations = operation_markers(&snapshot.git_directory);
        let shallow = bounded_file(&snapshot.common_directory.join("shallow"), MAX_HISTORY)?;
        let grafts = bounded_file(&snapshot.common_directory.join("info/grafts"), MAX_HISTORY)?;
        let mut digest = Sha256::new();
        for part in [
            refs.as_bytes(),
            shallow.as_slice(),
            grafts.as_slice(),
            branch.as_deref().unwrap_or("").as_bytes(),
            operations.join("\n").as_bytes(),
        ] {
            digest.update((part.len() as u64).to_le_bytes());
            digest.update(part);
        }
        snapshot.fingerprint = format!("{:x}", digest.finalize());
        snapshot.shallow = !shallow.is_empty();
        snapshot.head = head;
        snapshot.branch = branch;
        snapshot.upstream_target = upstream_target;
        snapshot.upstream = references
            .iter()
            .find(|r| Some(&r.name) == target.as_ref())
            .and_then(|r| r.upstream.clone());
        snapshot.references = references;
        snapshot.recovery = crate::recovery::read(&snapshot.git_directory, &operations)?;
        snapshot.operations = operations;
        Ok(())
    }
    pub fn history(&self, snapshot: Snapshot, cancel: &Cancellation) -> Result<History, Error> {
        let tips = snapshot
            .references
            .iter()
            .filter(|r| r.name.starts_with("stash@{"))
            .map(|r| r.hash.clone())
            .collect::<HashSet<_>>();
        let mut stashes = Vec::new();
        let tips = tips.into_iter().collect::<Vec<_>>();
        for chunk in tips.chunks(64) {
            let mut args = vec!["log".into(), "--no-walk=unsorted".into()];
            args.extend(log_options());
            args.extend(chunk.iter().cloned());
            args.push("--".into());
            stashes.extend(
                parse_commits(
                    &self
                        .read(&snapshot.worktree, args, cancel, MAX_HISTORY)?
                        .stdout,
                    chunk.len(),
                )?
                .into_iter()
                .filter(|c| (2..=3).contains(&c.parents.len())),
            );
        }
        // Preserve stash order from reflog, not hash-set iteration order.
        stashes.sort_by_key(|c| {
            snapshot
                .references
                .iter()
                .position(|r| r.name.starts_with("stash@{") && r.hash == c.hash)
                .unwrap_or(usize::MAX)
        });
        let candidates = stashes
            .iter()
            .flat_map(|c| c.parents.iter().skip(1).cloned())
            .collect::<HashSet<_>>();
        let candidate_list = candidates.iter().cloned().collect::<Vec<_>>();
        let mut hidden = HashSet::new();
        for chunk in candidate_list.chunks(64) {
            let mut args = vec!["rev-list".into(), "--max-count=513".into()];
            args.extend(chunk.iter().cloned());
            args.extend(["--not", "--branches", "--remotes", "--tags"].map(str::to_owned));
            args.extend(snapshot.head.iter().cloned());
            args.extend(stashes.iter().map(|c| c.parents[0].clone()));
            args.push("--".into());
            let hashes = lines(
                &self
                    .read(&snapshot.worktree, args, cancel, MAX_HISTORY)?
                    .stdout,
            );
            if hashes.len() > 512 {
                return Err(Error::Limit);
            }
            for hash in hashes {
                if !oid(&hash) {
                    return Err(Error::Invalid);
                }
                if candidates.contains(&hash) {
                    hidden.insert(hash);
                }
            }
        }
        for tip in tips {
            hidden.remove(&tip);
        }
        hidden.extend(stashes.iter().map(|c| c.hash.clone()));
        Ok(History {
            snapshot,
            stashes,
            hidden,
        })
    }
    pub fn page(
        &self,
        history: &History,
        cursor: &Cursor,
        cancel: &Cancellation,
    ) -> Result<Page, Error> {
        let mut current = history.snapshot.clone();
        self.refresh_refs(&mut current, cancel)?;
        if current.fingerprint != history.snapshot.fingerprint {
            return Err(Error::Changed);
        }
        self.page_from_observed_refs(history, cursor, current, None, cancel)
    }

    /// Start the projection from a freshly inspected snapshot. The page read
    /// checks the refs and directory identity again before it can publish.
    pub fn history_and_first_page(
        &self,
        snapshot: Snapshot,
        cancel: &Cancellation,
    ) -> Result<(History, Page), Error> {
        let history = self.history(snapshot, cancel)?;
        let current = history.snapshot.clone();
        let page = self.page_from_observed_refs(&history, &Cursor::default(), current, None, cancel)?;
        Ok((history, page))
    }

    fn page_from_observed_refs(
        &self,
        history: &History,
        cursor: &Cursor,
        mut current: Snapshot,
        initial_page: Option<&dyn Fn(&Page)>,
        cancel: &Cancellation,
    ) -> Result<Page, Error> {
        let mut args = vec!["log".into(), "--topo-order".into()];
        args.extend(log_options());
        args.push(format!("--skip={}", cursor.raw_offset));
        args.push(format!(
            "--max-count={}",
            PAGE_SIZE + 1 + history.hidden.len()
        ));
        args.extend(["--branches", "--remotes", "--tags"].map(str::to_owned));
        args.extend(history.snapshot.head.iter().cloned());
        args.extend(history.stashes.iter().map(|c| c.parents[0].clone()));
        args.extend(
            history
                .snapshot
                .references
                .iter()
                .filter(|r| {
                    r.name.starts_with("stash@{")
                        && !history.stashes.iter().any(|c| c.hash == r.hash)
                })
                .map(|r| r.hash.clone()),
        );
        args.push("--".into());
        let raw = if history.snapshot.references.is_empty() {
            Vec::new()
        } else {
            parse_commits(
                &self
                    .read(&history.snapshot.worktree, args, cancel, MAX_HISTORY)?
                    .stdout,
                PAGE_SIZE + 1 + history.hidden.len(),
            )?
        };
        let page = Self::project_page(history, cursor, &raw)?;
        if let Some(publish) = initial_page {
            publish(&page);
        }
        self.refresh_refs(&mut current, cancel)?;
        if current.fingerprint != history.snapshot.fingerprint {
            return Err(Error::Changed);
        }
        Ok(page)
    }

    fn project_page(history: &History, cursor: &Cursor, raw: &[Commit]) -> Result<Page, Error> {
        let mut projected = Vec::new();
        for (raw_index, commit) in raw.iter().enumerate() {
            if history.hidden.contains(&commit.hash) {
                continue;
            }
            let block = history
                .stashes
                .iter()
                .filter(|s| s.parents[0] == commit.hash)
                .chain(std::iter::once(commit));
            for (inserted_index, item) in block.enumerate() {
                if raw_index == 0 && inserted_index < cursor.inserted_offset {
                    continue;
                }
                projected.push((item.clone(), raw_index, inserted_index));
            }
        }
        if cursor.expected.as_ref().is_some_and(|expected| {
            projected
                .first()
                .is_none_or(|(c, _, _)| c.hash != *expected)
        }) {
            return Err(Error::Changed);
        }
        let shown = projected
            .iter()
            .take(PAGE_SIZE)
            .map(|(c, _, _)| c.clone())
            .collect::<Vec<_>>();
        let graph_commits = projected.iter().take(PAGE_SIZE).map(|(c, raw_index, _)| {
            let stash = history.stashes.iter().any(|s| s.hash == c.hash);
            let base = &raw[*raw_index];
            let stash_base_parent = (stash
                && !history.snapshot.references.iter().any(|r| r.hash == base.hash))
                .then(|| base.parents.first().map(String::as_str))
                .flatten();
            (
                c.hash.as_str(),
                if stash {
                    &c.parents[..1]
                } else {
                    c.parents.as_slice()
                },
                stash,
                stash_base_parent,
            )
        });
        let (graph, next_graph) = graph::build(graph_commits, &cursor.graph)?;
        let next = projected
            .get(PAGE_SIZE)
            .map(|(commit, raw, inserted)| Cursor {
                offset: cursor.offset + shown.len(),
                raw_offset: cursor.raw_offset + raw,
                inserted_offset: *inserted,
                expected: Some(commit.hash.clone()),
                graph: next_graph,
            });
        let entries = shown
            .into_iter()
            .zip(graph)
            .map(|(commit, graph)| {
                let references = history
                    .snapshot
                    .references
                    .iter()
                    .filter(|r| r.hash == commit.hash)
                    .cloned()
                    .collect();
                Entry {
                    commit,
                    graph,
                    references,
                }
            })
            .collect();
        Ok(Page {
            entries,
            cursor: cursor.clone(),
            next,
        })
    }
}

pub fn require(result: process::Result) -> Result<process::Result, Error> {
    if result.success() {
        Ok(result)
    } else {
        Err(command_error(&result))
    }
}

pub(crate) fn command_error(result: &process::Result) -> Error {
    Error::Command {
        outcome: result.outcome,
        code: result.exit_code,
        output: crate::network::captured_stream(&result.stderr, result.outcome)
            .chars()
            .take(32768)
            .collect(),
    }
}
pub(crate) fn log_options() -> Vec<String> {
    [
        "--no-color",
        "--no-decorate",
        "--no-show-signature",
        "--no-notes",
        "--no-use-mailmap",
        "--no-patch",
        "--no-ext-diff",
        "--no-textconv",
        "--encoding=UTF-8",
        "-z",
        FORMAT,
    ]
    .map(str::to_owned)
    .to_vec()
}
fn bounded_file(path: &Path, limit: usize) -> Result<Vec<u8>, Error> {
    let file = match fs::File::open(path) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
        Err(e) => return Err(e.into()),
    };
    let mut bytes = Vec::new();
    file.take((limit + 1) as u64).read_to_end(&mut bytes)?;
    if bytes.len() > limit {
        return Err(Error::Limit);
    }
    Ok(bytes)
}
const OPERATION_MARKERS: [(&str, &str); 6] = [
    ("MERGE_HEAD", "merge"),
    ("rebase-merge", "rebase"),
    ("rebase-apply", "rebase-apply"),
    ("CHERRY_PICK_HEAD", "cherry-pick"),
    ("REVERT_HEAD", "revert"),
    ("sequencer", "sequencer"),
];

fn worktree_has_unfinished_operation(root: &Path) -> std::io::Result<bool> {
    for name in OPERATION_MARKERS.map(|(path, _)| path).into_iter().chain(["BISECT_START"]) {
        match fs::symlink_metadata(root.join(name)) {
            Ok(_) => return Ok(true),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
    }
    Ok(false)
}

pub(crate) fn operation_markers(root: &Path) -> Vec<String> {
    OPERATION_MARKERS
    .into_iter()
    .filter(|(path, _)| root.join(path).exists())
    .map(|(_, name)| name.to_owned())
    .collect()
}
pub fn parse_commits(bytes: &[u8], maximum: usize) -> Result<Vec<Commit>, Error> {
    if bytes.len() > MAX_HISTORY {
        return Err(Error::Limit);
    }
    if bytes.is_empty() {
        return Ok(vec![]);
    }
    let data = text(bytes);
    let fields = data.split('\0').collect::<Vec<_>>();
    if fields.last() != Some(&"") || (fields.len() - 1) % 9 != 0 || (fields.len() - 1) / 9 > maximum
    {
        return Err(Error::Invalid);
    }
    let mut commits = Vec::new();
    for fields in fields[..fields.len() - 1].as_chunks::<9>().0 {
        let parents = fields[1]
            .split_whitespace()
            .map(str::to_owned)
            .collect::<Vec<_>>();
        if !oid(fields[0]) || parents.iter().any(|p| !oid(p)) {
            return Err(Error::Invalid);
        }
        commits.push(Commit {
            hash: fields[0].into(),
            parents,
            author: fields[2].into(),
            email: fields[3].into(),
            authored: DateTime::parse_from_rfc3339(fields[4]).map_err(|_| Error::Invalid)?,
            committer: fields[5].into(),
            committer_email: fields[6].into(),
            committed: DateTime::parse_from_rfc3339(fields[7]).map_err(|_| Error::Invalid)?,
            message: fields[8].into(),
        });
    }
    Ok(commits)
}

#[cfg(test)]
mod discovery_tests {
    use super::*;

    #[test]
    fn damaged_preferred_git_does_not_hide_a_usable_install_but_explicit_choice_is_exact() {
        let cancel = Cancellation::default();
        let installed = Git::discover(None, &cancel).unwrap();
        let temp = tempfile::tempdir().unwrap();
        let damaged = temp.path().join("git.exe");
        fs::write(&damaged, b"incomplete installation fixture").unwrap();
        let wrong_type = temp.path().join("git.cmd");
        fs::write(&wrong_type, b"fixture").unwrap();
        let fallback = Git::discover_candidates(
            [
                PathBuf::from("git.exe"),
                wrong_type,
                damaged.clone(),
                installed.executable.clone(),
            ],
            &cancel,
        )
        .unwrap();
        assert_eq!(fallback.executable, installed.executable);
        assert_eq!(fallback.version, installed.version);
        assert!(Git::discover(Some(&damaged), &cancel).is_err());
        assert!(Git::discover(Some(Path::new("git.exe")), &cancel).is_err());
        let cancelled = Cancellation::default();
        cancelled.cancel();
        assert!(Git::discover_candidates([installed.executable], &cancelled).is_err());
    }
}
