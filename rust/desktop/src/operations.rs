use crate::strings::{self, text};
use eframe::egui::{self, RichText};
use sha2::{Digest, Sha256};
use shovel_core::{
    authentication,
    git::{Git, Snapshot},
    network::{self, Action, PushTarget, Strategy, Target},
    process::Cancellation,
    scheduler::{Lane, Outcome, Scheduler},
    settings::Settings,
};
use std::{
    collections::VecDeque,
    path::{Path, PathBuf},
    sync::{
        Arc,
        mpsc::{self, Receiver, Sender},
    },
    time::Duration,
};
use zeroize::{Zeroize, Zeroizing};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Fetch,
    Pull,
    Push,
}
impl Kind {
    fn key(self) -> &'static str {
        match self {
            Self::Fetch => "Fetch",
            Self::Pull => "Pull",
            Self::Push => "Push",
        }
    }
    fn number(self) -> u64 {
        match self {
            Self::Fetch => 2,
            Self::Pull => 3,
            Self::Push => 4,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct Options {
    kind: Kind,
    source: String,
    remote: String,
    branch: String,
    strategy: Strategy,
    auto_stash: bool,
    all_remotes: bool,
    all_tags: bool,
    set_upstream: bool,
    explicit: bool,
}
impl Options {
    fn load(settings: &Settings, snapshot: &Snapshot, kind: Kind) -> Self {
        let branch = snapshot.branch.clone().unwrap_or_default();
        let upstream = snapshot
            .upstream
            .as_deref()
            .and_then(|u| u.strip_prefix("refs/remotes/"))
            .and_then(|u| {
                snapshot.remotes.iter().find_map(|r| {
                    u.strip_prefix(&format!("{r}/"))
                        .map(|b| (r.clone(), b.to_owned()))
                })
            });
        let (remote, destination) = upstream.unwrap_or_else(|| {
            (
                snapshot
                    .remotes
                    .iter()
                    .find(|r| *r == "origin")
                    .or(snapshot.remotes.first())
                    .cloned()
                    .unwrap_or_default(),
                branch.clone(),
            )
        });
        let mut options = Self {
            kind,
            source: branch.clone(),
            remote,
            branch: destination,
            strategy: Strategy::Rebase,
            auto_stash: false,
            all_remotes: false,
            all_tags: false,
            set_upstream: false,
            explicit: false,
        };
        let saved = settings
            .get("NetworkActions")
            .and_then(serde_json::Value::as_array)
            .and_then(|items| {
                items.iter().find(|v| {
                    v["Action"].as_u64() == Some(kind.number())
                        && v["Path"].as_str().is_some_and(|s| {
                            s.eq_ignore_ascii_case(&snapshot.worktree.to_string_lossy())
                        })
                })
            });
        if let Some(saved) = saved {
            options.strategy = if saved["Strategy"].as_u64() == Some(1) {
                Strategy::Merge
            } else {
                Strategy::Rebase
            };
            options.auto_stash = saved["AutoStash"].as_bool().unwrap_or(false);
            options.all_remotes = saved["AllRemotes"].as_bool().unwrap_or(false);
            options.all_tags = saved["AllTags"].as_bool().unwrap_or(false);
            options.set_upstream = saved["SetUpstream"].as_bool().unwrap_or(false);
            let valid = ["SourceBranch", "Remote", "RemoteBranch"].iter().all(|k| {
                saved[*k]
                    .as_str()
                    .is_some_and(|s| s.len() <= 1024 && !s.chars().any(char::is_control))
            });
            if valid
                && saved["ExplicitTarget"].as_bool() == Some(true)
                && (kind == Kind::Fetch || saved["Branch"].as_str() == Some(&branch))
            {
                options.source = saved["SourceBranch"].as_str().unwrap().into();
                options.remote = saved["Remote"].as_str().unwrap().into();
                options.branch = saved["RemoteBranch"].as_str().unwrap().into();
                options.explicit = true;
            }
        }
        options
    }
    fn save(&self, settings: &mut Settings, snapshot: &Snapshot) {
        let mut entries = settings
            .get("NetworkActions")
            .and_then(serde_json::Value::as_array)
            .cloned()
            .unwrap_or_default();
        let path = snapshot.worktree.to_string_lossy();
        entries.retain(|v| {
            !(v["Action"].as_u64() == Some(self.kind.number())
                && v["Path"]
                    .as_str()
                    .is_some_and(|p| p.eq_ignore_ascii_case(&path)))
        });
        entries.insert(0, serde_json::json!({ "Path": path, "Action": self.kind.number(), "Branch": snapshot.branch.as_deref().unwrap_or_default(), "SourceBranch": self.source, "Remote": self.remote, "RemoteBranch": self.branch, "Strategy": if self.strategy == Strategy::Merge { 1 } else { 0 }, "AutoStash": self.auto_stash, "AllRemotes": self.all_remotes, "AllTags": self.all_tags, "SetUpstream": self.set_upstream, "ExplicitTarget": self.explicit }));
        entries.truncate(60);
        settings.set("NetworkActions", serde_json::Value::Array(entries));
    }
    fn action(&self) -> Action {
        let target = Target {
            remote: self.remote.clone(),
            branch: self.branch.clone(),
        };
        match self.kind {
            Kind::Fetch => Action::Fetch {
                remote: self.explicit.then(|| self.remote.clone()),
                all: self.all_remotes,
            },
            Kind::Pull => Action::Pull {
                target: self.explicit.then_some(target),
                strategy: self.strategy,
                auto_stash: self.auto_stash,
            },
            Kind::Push => Action::Push {
                target: self.explicit.then(|| PushTarget {
                    source: self.source.clone(),
                    destination: target,
                }),
                all_tags: self.all_tags,
                set_upstream: self.set_upstream,
            },
        }
    }
}
enum LocalAction {
    Branch(shovel_core::branches::Action),
    History(shovel_core::history_actions::Action),
    Recovery(shovel_core::recovery::Action),
    Stash(shovel_core::stashes::Action),
    Remote(shovel_core::remotes::Action),
    Submodule(shovel_core::submodules::Action),
}
impl LocalAction {
    fn key(&self) -> String {
        use shovel_core::branches::Action;
        match self {
            Self::Branch(Action::Create { name, start }) => format!("create:{name}:{}", start.hash),
            Self::Branch(Action::Checkout {
                target, changes, ..
            }) => format!("checkout:{target:?}:{changes:?}"),
            Self::Branch(Action::Manage { branch, action }) => {
                format!("manage:{}:{action:?}", branch.name)
            }
            Self::History(action) => format!("history:{action:?}"),
            Self::Recovery(action) => format!("recovery:{action:?}"),
            Self::Stash(action) => format!("stash:{action:?}"),
            Self::Submodule(action) => format!("submodule:{action:?}"),
            Self::Remote(action) => format!(
                "remote:{:x}",
                Sha256::digest(format!("{action:?}").as_bytes())
            ),
        }
    }
}

enum Event {
    Stage(u64, network::Stage),
    BranchStage(u64, shovel_core::branches::Stage),
    Prompt(u64, authentication::Prompt),
    ExternalPrompt(u64, authentication::Prompt),
    Completed(u64, Box<Result<network::Completed, String>>),
    Removed(u64, Outcome),
    OptionsReady(u64, Box<ReadyOptions>),
    OptionsClosed(u64),
}
struct Run {
    sequence: u64,
    request: u64,
    path: PathBuf,
    label: &'static str,
    status: String,
    output: String,
    done: bool,
    dialog_open: bool,
    changed: bool,
    reopen_commit: bool,
    show_failure: bool,
}
struct Question {
    run: u64,
    prompt: authentication::Prompt,
    response: Zeroizing<String>,
    focus: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RequestKind {
    Network(Kind),
    Options(Kind),
    NewBranch,
}
impl RequestKind {
    fn label(self) -> &'static str {
        match self {
            Self::Network(kind) => kind.key(),
            Self::Options(Kind::Fetch) => "RustFetchOptions",
            Self::Options(Kind::Pull) => "PullOptions",
            Self::Options(Kind::Push) => "PushOptions",
            Self::NewBranch => "ActionTitleNewBranch",
        }
    }
    fn lane(self) -> Lane {
        match self {
            Self::Network(Kind::Fetch) | Self::Options(Kind::Fetch) => Lane::Fetch,
            _ => Lane::Exclusive,
        }
    }
}
pub struct DialogLease {
    request: u64,
    _release: Sender<()>,
}
impl DialogLease {
    pub fn continue_with(&self, scheduler: &Scheduler, command: Option<u64>) {
        if let Some(command) = command {
            scheduler.inherit_dialog_order(self.request, command);
        }
    }
}
struct ReadyOptions {
    options: Option<Options>,
    snapshot: Snapshot,
    lease: DialogLease,
}
struct PendingNetwork {
    sequence: u64,
    request: u64,
    path: PathBuf,
    action: RequestKind,
    settings: Settings,
}
pub struct Controller {
    pub divergence: crate::divergence::Display,
    pending: Vec<PendingNetwork>,
    related_changes: Vec<PathBuf>,
    options: Option<(Options, Snapshot)>,
    ready_options: VecDeque<ReadyOptions>,
    options_hold: Option<DialogLease>,
    next: u64,
    runs: Vec<Run>,
    active_failure: Option<u64>,
    prompts: VecDeque<Question>,
    sender: Sender<Event>,
    receiver: Receiver<Event>,
    helper: Option<PathBuf>,
}
impl Default for Controller {
    fn default() -> Self {
        let (sender, receiver) = mpsc::channel();
        Self {
            divergence: Default::default(),
            pending: vec![],
            related_changes: vec![],
            options: None,
            ready_options: VecDeque::new(),
            options_hold: None,
            next: 1,
            runs: vec![],
            active_failure: None,
            prompts: VecDeque::new(),
            sender,
            receiver,
            helper: None,
        }
    }
}
impl Controller {
    pub fn external_authentication(
        &self,
        request: u64,
        ctx: &egui::Context,
    ) -> Box<dyn FnOnce() -> Result<authentication::Session, shovel_core::operation::Error> + Send>
    {
        let helper = self.helper.clone();
        let sender = self.sender.clone();
        let ctx = ctx.clone();
        Box::new(move || {
            let handler = Arc::new(move |prompt| {
                let _ = sender.send(Event::ExternalPrompt(request, prompt));
                ctx.request_repaint();
            });
            let helper = match helper {
                Some(path) => path,
                None => std::env::current_exe()
                    .map_err(|_| shovel_core::operation::Error::Code("ActionAskPassMissing"))?
                    .with_file_name("ShovelAskPass.exe"),
            };
            authentication::Session::start(&helper, handler)
                .map_err(|_| shovel_core::operation::Error::Code("ActionAskPassMissing"))
        })
    }
    #[cfg(test)]
    pub(crate) fn set_authentication_helper(&mut self, helper: PathBuf) {
        self.helper = Some(helper);
    }
    pub fn active_modal_open(&self) -> bool {
        !self.prompts.is_empty() || self.options.is_some()
    }
    pub fn modal_open(&self) -> bool {
        self.active_modal_open() || !self.ready_options.is_empty()
    }
    pub fn open_ready_branch(&mut self, branches: &mut crate::branches::Dialogs) {
        if !self.active_modal_open()
            && !branches.is_open()
            && self
                .ready_options
                .front()
                .is_some_and(|ready| ready.options.is_none())
        {
            let ready = self.ready_options.pop_front().unwrap();
            branches.create_queued(&ready.snapshot, ready.lease);
        }
    }
    pub fn toolbar(
        &mut self,
        ui: &mut egui::Ui,
        settings: &mut Settings,
        path: Option<&PathBuf>,
        snapshot: Option<&Snapshot>,
        git: Option<&Arc<Git>>,
        scheduler: &Scheduler,
    ) -> bool {
        let changed = false;
        for kind in [Kind::Fetch, Kind::Pull, Kind::Push] {
            ui.push_id(kind.number(), |ui| {
                let available = path.is_some() && git.is_some();
                ui.spacing_mut().item_spacing.x = 0.;
                let count = match kind {
                    Kind::Pull => self
                        .divergence
                        .behind
                        .as_deref()
                        .map(|n| (n, "BehindCount")),
                    Kind::Push => self.divergence.ahead.as_deref().map(|n| (n, "AheadCount")),
                    Kind::Fetch => None,
                };
                let button =
                    crate::workspace_style::action_with_count(ui, kind.key(), available, count);
                if kind != Kind::Fetch {
                    button.clone().on_hover_text(&self.divergence.help);
                }
                if !available {
                    button
                        .clone()
                        .on_disabled_hover_text(text("ActionInspectionNeeded"));
                }
                if button.clicked()
                    && let (Some(path), Some(git)) = (path, git)
                {
                    if let Some(snapshot) = snapshot {
                        self.submit(
                            Options::load(settings, snapshot, kind),
                            snapshot,
                            git.clone(),
                            scheduler,
                            ui.ctx(),
                        );
                    } else {
                        self.defer_network(kind, path, settings, scheduler, ui.ctx());
                    }
                }
                if crate::workspace_style::options_button(
                    ui,
                    strings::format("RustNetworkOptions", &[&text(kind.key())]),
                    available,
                )
                .clicked()
                    && let Some(path) = path
                {
                    if let Some(snapshot) = snapshot {
                        self.options =
                            Some((Options::load(settings, snapshot, kind), snapshot.clone()));
                    } else {
                        self.defer(
                            RequestKind::Options(kind),
                            path,
                            settings,
                            scheduler,
                            ui.ctx(),
                        );
                    }
                }
            });
        }
        changed
    }
    pub fn options_dialog(
        &mut self,
        ctx: &egui::Context,
        settings: &mut Settings,
        git: Option<&Arc<Git>>,
        scheduler: &Scheduler,
    ) -> bool {
        if self.options.is_none()
            && self
                .ready_options
                .front()
                .is_some_and(|ready| ready.options.is_some())
        {
            let ready = self.ready_options.pop_front().unwrap();
            self.options = Some((ready.options.unwrap(), ready.snapshot));
            self.options_hold = Some(ready.lease);
        }
        let Some((mut options, snapshot)) = self.options.take() else {
            return false;
        };
        let before = options.clone();
        let kind = options.kind;
        let mut open = true;
        let mut close = false;
        egui::Window::new(strings::format("RustNetworkOptions", &[&text(kind.key())]))
            .id(egui::Id::unique("network-options"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(480.)
            .show(ctx, |ui| {
                ui.label(snapshot.worktree.display().to_string());
                ui.label(text("NetworkRememberOptions"));
                ui.label(text(if options.explicit {
                    "ActionCachedTargets"
                } else {
                    "RustNetworkConfigured"
                }));
                if kind == Kind::Fetch {
                    ui.checkbox(&mut options.all_remotes, text("ActionAllRemotes"));
                }
                if kind == Kind::Pull {
                    ui.radio_value(
                        &mut options.strategy,
                        Strategy::Rebase,
                        text("ActionPullRebase"),
                    );
                    ui.radio_value(
                        &mut options.strategy,
                        Strategy::Merge,
                        text("ActionPullMerge"),
                    );
                    ui.checkbox(&mut options.auto_stash, text("ActionAutoStash"));
                }
                if kind == Kind::Push {
                    ui.checkbox(&mut options.all_tags, text("ActionAllTags"));
                    ui.checkbox(&mut options.set_upstream, text("ActionSetUpstream"));
                    ui.label(text("ActionBranchLabel"));
                    let old = options.source.clone();
                    egui::ComboBox::from_id_salt("source")
                        .selected_text(&options.source)
                        .show_ui(ui, |ui| {
                            for reference in &snapshot.references {
                                if let Some(branch) = reference.name.strip_prefix("refs/heads/") {
                                    ui.selectable_value(
                                        &mut options.source,
                                        branch.to_owned(),
                                        branch,
                                    );
                                }
                            }
                        });
                    options.explicit |= options.source != old;
                }
                ui.label(text("ActionRemote"));
                let old = options.remote.clone();
                egui::ComboBox::from_id_salt("remote")
                    .selected_text(&options.remote)
                    .show_ui(ui, |ui| {
                        for remote in &snapshot.remotes {
                            ui.selectable_value(&mut options.remote, remote.clone(), remote);
                        }
                    });
                options.explicit |= old != options.remote;
                if kind != Kind::Fetch {
                    ui.label(text("ActionRemoteBranch"));
                    options.explicit |= ui
                        .add(egui::TextEdit::singleline(&mut options.branch).char_limit(1024))
                        .changed();
                }
                if options.explicit && ui.button(text("RustNetworkUseConfigured")).clicked() {
                    options.explicit = false;
                }

                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button(text("Close")).clicked() {
                        close = true;
                    }
                    if ui.button(text(kind.key())).clicked() {
                        if let Some(git) = git {
                            let command = self.submit(
                                options.clone(),
                                &snapshot,
                                git.clone(),
                                scheduler,
                                ctx,
                            );
                            if let Some(lease) = &self.options_hold {
                                lease.continue_with(scheduler, command);
                            }
                        }
                        close = true;
                    }
                });
            });
        let changed = options != before;
        if changed {
            options.save(settings, &snapshot);
        }
        if open && !close && !ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.options = Some((options, snapshot));
        } else {
            // Release the repository lane only after remembered choices are saved
            // and any explicit submission has entered the queue.
            self.options_hold.take();
        }
        changed
    }
    fn pending_owner(path: &std::path::Path) -> String {
        format!("initializing:{}", shovel_core::case::path_key(path))
    }

    fn reserve_run(
        &mut self,
        action: RequestKind,
        path: &Path,
        owner: (&str, &str),
        target_key: Option<&str>,
        scheduler: &Scheduler,
        ctx: &egui::Context,
    ) -> Option<(u64, u64)> {
        let sequence = self.next;
        self.next += 1;
        let sender = self.sender.clone();
        let ctx = ctx.clone();
        let (request, accepted) = scheduler.reserve(
            owner.0.into(),
            owner.1.into(),
            match (action, target_key) {
                (RequestKind::Network(kind), Some(key)) => format!("network:{}:{key}", kind.key()),
                (RequestKind::Network(kind), None) => format!("network:{}", kind.key()),
                _ => format!("dialog:{action:?}"),
            },
            action.lane(),
            move |outcome| {
                let _ = sender.send(Event::Removed(sequence, outcome));
                ctx.request_repaint();
            },
        );
        if !accepted {
            return None;
        }
        self.runs.push(Run {
            sequence,
            request,
            path: path.to_path_buf(),
            label: action.label(),
            status: text("OperationWaiting"),
            output: String::new(),
            done: false,
            dialog_open: false,
            changed: false,
            reopen_commit: false,
            show_failure: false,
        });
        Some((request, sequence))
    }

    pub fn defer_network(
        &mut self,
        kind: Kind,
        path: &Path,
        settings: &Settings,
        scheduler: &Scheduler,
        ctx: &egui::Context,
    ) {
        self.defer(RequestKind::Network(kind), path, settings, scheduler, ctx);
    }

    pub fn defer_new_branch(
        &mut self,
        path: &Path,
        settings: &Settings,
        scheduler: &Scheduler,
        ctx: &egui::Context,
    ) {
        self.defer(RequestKind::NewBranch, path, settings, scheduler, ctx);
    }

    fn defer(
        &mut self,
        action: RequestKind,
        path: &Path,
        settings: &Settings,
        scheduler: &Scheduler,
        ctx: &egui::Context,
    ) {
        let owner = Self::pending_owner(path);
        if let Some((request, sequence)) =
            self.reserve_run(action, path, (&owner, &owner), None, scheduler, ctx)
        {
            self.pending.push(PendingNetwork {
                request,
                sequence,
                path: path.to_path_buf(),
                action,
                settings: settings.clone(),
            });
        }
    }

    pub fn pending_paths(&self) -> Vec<PathBuf> {
        self.pending.iter().map(|p| p.path.clone()).collect()
    }

    pub fn has_pending(&self, path: &std::path::Path) -> bool {
        self.pending
            .iter()
            .any(|p| crate::Workspace::same_path(&p.path, path))
    }

    pub fn bind_initialization(
        &self,
        path: &std::path::Path,
        snapshot: &Snapshot,
        scheduler: &Scheduler,
    ) {
        scheduler.bind_repository(
            &Self::pending_owner(path),
            &snapshot.identity.fingerprint,
            &shovel_core::case::path_key(&snapshot.common_directory),
        );
    }

    pub fn resolve_initialization(
        &mut self,
        path: &std::path::Path,
        result: Result<&Snapshot, &str>,
        git: &Arc<Git>,
        scheduler: &Scheduler,
        ctx: &egui::Context,
    ) {
        let (pending, others): (Vec<_>, Vec<_>) = std::mem::take(&mut self.pending)
            .into_iter()
            .partition(|p| crate::Workspace::same_path(&p.path, path));
        self.pending = others;
        if pending.is_empty() {
            return;
        }
        match result {
            Ok(snapshot) => {
                self.bind_initialization(path, snapshot, scheduler);
                for request in pending {
                    if let RequestKind::Network(kind) = request.action {
                        self.activate_network(
                            (request.request, request.sequence),
                            Options::load(&request.settings, snapshot, kind),
                            snapshot,
                            git.clone(),
                            scheduler,
                            ctx,
                        );
                    } else {
                        if request.action == RequestKind::NewBranch
                            && !snapshot.operations.is_empty()
                        {
                            if let Some(run) = self
                                .runs
                                .iter_mut()
                                .find(|r| r.sequence == request.sequence)
                            {
                                run.output = text("ActionSequence");
                            }
                            scheduler.fail_reserved(request.request);
                            continue;
                        }
                        let sender = self.sender.clone();
                        let ctx = ctx.clone();
                        let snapshot = snapshot.clone();
                        scheduler.activate(request.request, move || {
                            let (release, held) = mpsc::channel();
                            let ready = ReadyOptions {
                                options: if let RequestKind::Options(kind) = request.action {
                                    Some(Options::load(&request.settings, &snapshot, kind))
                                } else {
                                    None
                                },
                                snapshot,
                                lease: DialogLease {
                                    request: request.request,
                                    _release: release,
                                },
                            };
                            let _ =
                                sender.send(Event::OptionsReady(request.sequence, Box::new(ready)));
                            ctx.request_repaint();
                            // No Git or preparation. The dialog owns this queue slot
                            // until dismissal, including while another dialog is shown.
                            let _ = held.recv();
                            let _ = sender.send(Event::OptionsClosed(request.sequence));
                            ctx.request_repaint();
                            Outcome::Succeeded
                        });
                    }
                }
            }
            Err(error) => {
                for request in pending {
                    if let Some(run) = self
                        .runs
                        .iter_mut()
                        .find(|r| r.sequence == request.sequence)
                    {
                        run.output = network::redact(error);
                    }
                    scheduler.fail_reserved(request.request);
                }
            }
        }
    }

    fn submit(
        &mut self,
        options: Options,
        snapshot: &Snapshot,
        git: Arc<Git>,
        scheduler: &Scheduler,
        ctx: &egui::Context,
    ) -> Option<u64> {
        let common = shovel_core::case::path_key(&snapshot.common_directory);
        if let Some(ticket) = self.reserve_run(
            RequestKind::Network(options.kind),
            &snapshot.worktree,
            (&snapshot.identity.fingerprint, &common),
            None,
            scheduler,
            ctx,
        ) {
            self.activate_network(ticket, options, snapshot, git, scheduler, ctx);
            Some(ticket.0)
        } else {
            None
        }
    }

    pub fn fetch_remote(
        &mut self,
        name: &str,
        snapshot: &Snapshot,
        git: Arc<Git>,
        scheduler: &Scheduler,
        ctx: &egui::Context,
    ) {
        // A sidebar action selects exactly this remote. It neither inherits
        // AllRemotes nor replaces the user's remembered toolbar preferences.
        let mut options = Options::load(&Settings::default(), snapshot, Kind::Fetch);
        options.remote = name.to_owned();
        options.explicit = true;
        options.all_remotes = false;
        let common = shovel_core::case::path_key(&snapshot.common_directory);
        if let Some(ticket) = self.reserve_run(
            RequestKind::Network(Kind::Fetch),
            &snapshot.worktree,
            (&snapshot.identity.fingerprint, &common),
            Some(&format!("remote:{name}")),
            scheduler,
            ctx,
        ) {
            self.activate_network(ticket, options, snapshot, git, scheduler, ctx);
        }
    }

    fn activate_network(
        &self,
        (request, sequence): (u64, u64),
        options: Options,
        snapshot: &Snapshot,
        git: Arc<Git>,
        scheduler: &Scheduler,
        ctx: &egui::Context,
    ) {
        let action = options.action();
        let captured = snapshot.clone();
        let helper = self.helper.clone();
        let sender = self.sender.clone();
        let ctx = ctx.clone();
        scheduler.activate(request, move || {
            let progress_sender = sender.clone();
            let progress_ctx = ctx.clone();
            let progress = Arc::new(move |stage| {
                let _ = progress_sender.send(Event::Stage(sequence, stage));
                progress_ctx.request_repaint();
            });
            let prompt_sender = sender.clone();
            let prompt_ctx = ctx.clone();
            let handler = Arc::new(move |prompt| {
                let _ = prompt_sender.send(Event::Prompt(sequence, prompt));
                prompt_ctx.request_repaint();
            });
            let result = (|| {
                progress(network::Stage::Validating);
                let helper = match helper {
                    Some(path) => path,
                    None => std::env::current_exe()
                        .map_err(|_| text("ActionAskPassMissing"))?
                        .with_file_name("ShovelAskPass.exe"),
                };
                let auth = authentication::Session::start(&helper, handler)
                    .map_err(|_| text("ActionAskPassMissing"))?;
                let review = git
                    .prepare_network(&captured.worktree, &Cancellation::default())
                    .map_err(|e| error_log(&e))?;
                if review.repository.identity != captured.identity
                    || review.repository.branch != captured.branch
                {
                    return Err(text("ActionChanged"));
                }
                git.execute_network(&review, &action, Some(&auth), Some(progress))
                    .map_err(|e| error_log(&e))
            })();
            let outcome = match &result {
                Ok(result) if result.succeeded() => Outcome::Succeeded,
                Ok(result)
                    if result.has_conflicts
                        || result
                            .updated
                            .as_ref()
                            .is_ok_and(|r| !r.operations.is_empty()) =>
                {
                    Outcome::Conflict
                }
                _ => Outcome::Failed,
            };
            let _ = sender.send(Event::Completed(sequence, Box::new(result)));
            ctx.request_repaint();
            outcome
        });
    }
    pub fn submit_branch(
        &mut self,
        label: &'static str,
        action: shovel_core::branches::Action,
        snapshot: &Snapshot,
        git: Arc<Git>,
        scheduler: &Scheduler,
        ctx: &egui::Context,
    ) -> Option<u64> {
        self.submit_local(
            label,
            LocalAction::Branch(action),
            snapshot,
            git,
            scheduler,
            ctx,
        )
    }
    pub fn submit_history(
        &mut self,
        label: &'static str,
        action: shovel_core::history_actions::Action,
        snapshot: &Snapshot,
        git: Arc<Git>,
        scheduler: &Scheduler,
        ctx: &egui::Context,
    ) {
        self.submit_local(
            label,
            LocalAction::History(action),
            snapshot,
            git,
            scheduler,
            ctx,
        );
    }
    pub fn submit_recovery(
        &mut self,
        label: &'static str,
        action: shovel_core::recovery::Action,
        snapshot: &Snapshot,
        git: Arc<Git>,
        scheduler: &Scheduler,
        ctx: &egui::Context,
    ) {
        self.submit_local(
            label,
            LocalAction::Recovery(action),
            snapshot,
            git,
            scheduler,
            ctx,
        );
    }
    pub fn submit_stash(
        &mut self,
        label: &'static str,
        action: shovel_core::stashes::Action,
        snapshot: &Snapshot,
        git: Arc<Git>,
        scheduler: &Scheduler,
        ctx: &egui::Context,
    ) -> Option<u64> {
        self.submit_local(
            label,
            LocalAction::Stash(action),
            snapshot,
            git,
            scheduler,
            ctx,
        )
    }
    pub fn submit_remote(
        &mut self,
        label: &'static str,
        action: shovel_core::remotes::Action,
        snapshot: &Snapshot,
        git: Arc<Git>,
        scheduler: &Scheduler,
        ctx: &egui::Context,
    ) {
        self.submit_local(
            label,
            LocalAction::Remote(action),
            snapshot,
            git,
            scheduler,
            ctx,
        );
    }
    pub fn submit_submodule(
        &mut self,
        action: shovel_core::submodules::Action,
        snapshot: &Snapshot,
        git: Arc<Git>,
        scheduler: &Scheduler,
        ctx: &egui::Context,
    ) {
        let label = match action.operation {
            shovel_core::submodules::Operation::Update => "ActionTitleSubmoduleUpdate",
            shovel_core::submodules::Operation::Sync => "ActionTitleSubmoduleSync",
        };
        self.submit_local(
            label,
            LocalAction::Submodule(action),
            snapshot,
            git,
            scheduler,
            ctx,
        );
    }
    fn submit_local(
        &mut self,
        label: &'static str,
        action: LocalAction,
        snapshot: &Snapshot,
        git: Arc<Git>,
        scheduler: &Scheduler,
        ctx: &egui::Context,
    ) -> Option<u64> {
        let sequence = self.next;
        self.next += 1;
        let captured = snapshot.clone();
        let helper = self.helper.clone();
        let sender = self.sender.clone();
        let discarded = sender.clone();
        let ctx = ctx.clone();
        let discarded_ctx = ctx.clone();
        // Keys remain in memory and omit working-copy review content.
        let key = if matches!(action, LocalAction::Recovery(_)) {
            format!(
                "{}:{}",
                action.key(),
                captured
                    .recovery
                    .as_ref()
                    .map(|r| r.stamp.as_str())
                    .unwrap_or_default()
            )
        } else {
            action.key()
        };
        let (request, accepted) = scheduler.submit(
            captured.identity.fingerprint.clone(),
            shovel_core::case::path_key(&captured.common_directory),
            key,
            Lane::Exclusive,
            move || {
                let progress_sender = sender.clone();
                let progress_ctx = ctx.clone();
                let progress = Arc::new(move |stage| {
                    let _ = progress_sender.send(Event::BranchStage(sequence, stage));
                    progress_ctx.request_repaint();
                });
                let result = (|| {
                    progress(shovel_core::branches::Stage::Validating);
                    let result = if let LocalAction::Recovery(action) = &action {
                        git.execute_recovery(&captured, *action, Some(progress))
                    } else if let LocalAction::Stash(action) = &action {
                        git.execute_stash(&captured, action, Some(progress))
                    } else if let LocalAction::Remote(action) = &action {
                        git.execute_remote(&captured, action, Some(progress))
                    } else if let LocalAction::Submodule(action) = &action {
                        let prepared = git
                            .prepare_local(&captured, true, &Cancellation::default())
                            .map_err(|e| error_log(&e))?;
                        let auth = if action.operation == shovel_core::submodules::Operation::Update
                        {
                            let helper = match &helper {
                                Some(path) => path.clone(),
                                None => std::env::current_exe()
                                    .map_err(|_| text("ActionAskPassMissing"))?
                                    .with_file_name("ShovelAskPass.exe"),
                            };
                            let prompt_sender = sender.clone();
                            let prompt_ctx = ctx.clone();
                            Some(
                                authentication::Session::start(
                                    &helper,
                                    Arc::new(move |prompt| {
                                        let _ = prompt_sender.send(Event::Prompt(sequence, prompt));
                                        prompt_ctx.request_repaint();
                                    }),
                                )
                                .map_err(|_| text("ActionAskPassMissing"))?,
                            )
                        } else {
                            None
                        };
                        git.execute_submodule(&prepared, action, auth.as_ref(), Some(progress))
                    } else {
                        let unchanged_refs = !matches!(
                            action,
                            LocalAction::Branch(shovel_core::branches::Action::Checkout { .. })
                        );
                        let prepared = git
                            .prepare_local(&captured, unchanged_refs, &Cancellation::default())
                            .map_err(|e| error_log(&e))?;
                        match &action {
                            LocalAction::Branch(action) => {
                                git.execute_branch(&prepared, action, Some(progress))
                            }
                            LocalAction::History(action) => {
                                git.execute_history(&prepared, action, Some(progress))
                            }
                            LocalAction::Recovery(_)
                            | LocalAction::Stash(_)
                            | LocalAction::Remote(_)
                            | LocalAction::Submodule(_) => unreachable!(),
                        }
                    };
                    result
                        .map(|mut result| {
                            if let LocalAction::History(action) = &action {
                                result.output = format!(
                                    "{}\n{}",
                                    strings::format(
                                        "HistoryActionTarget",
                                        &[
                                            action.target(),
                                            captured.head.as_deref().unwrap_or_default()
                                        ]
                                    ),
                                    result.output
                                );
                            }
                            result
                        })
                        .map_err(|e| error_log(&e))
                })();
                let outcome = match &result {
                    Ok(result) if result.succeeded() => Outcome::Succeeded,
                    Ok(result)
                        if result.has_conflicts
                            || result
                                .updated
                                .as_ref()
                                .is_ok_and(|s| !s.operations.is_empty()) =>
                    {
                        Outcome::Conflict
                    }
                    _ => Outcome::Failed,
                };
                let _ = sender.send(Event::Completed(sequence, Box::new(result)));
                ctx.request_repaint();
                outcome
            },
            move |outcome| {
                let _ = discarded.send(Event::Removed(sequence, outcome));
                discarded_ctx.request_repaint();
            },
        );
        if accepted {
            self.runs.push(Run {
                sequence,
                request,
                path: snapshot.worktree.clone(),
                label,
                status: text("OperationWaiting"),
                output: String::new(),
                done: false,
                dialog_open: false,
                changed: false,
                reopen_commit: false,
                show_failure: false,
            });
        }
        accepted.then_some(request)
    }
    pub fn poll(&mut self, ctx: &egui::Context) -> Vec<PathBuf> {
        while let Ok(event) = self.receiver.try_recv() {
            match event {
                Event::BranchStage(id, stage) => {
                    use shovel_core::branches::Stage;
                    if let Some(run) = self.runs.iter_mut().find(|r| r.sequence == id) {
                        run.status = text(match stage {
                            Stage::Validating => "ActionStageValidating",
                            Stage::Stashing => "ActionTitleStashCreate",
                            Stage::Discarding => "CheckoutDiscard",
                            Stage::Switching => "ActionTitleSwitchBranch",
                            Stage::Reapplying => "ActionTitleStashApply",
                            Stage::DroppingStash => "ActionTitleStashDrop",
                            Stage::UpdatingBranch => "ActionStageUpdatingBranch",
                            Stage::UpdatingRemote => "RemoteManageTitle",
                            Stage::UpdatingSubmodule => run.label,
                            Stage::Resetting => "ActionStageResetting",
                            Stage::Reverting => "ActionStageReverting",
                            Stage::Recovering => "ActionStageRecovering",
                            Stage::Reconciling => "ActionReinspecting",
                        });
                    }
                }
                Event::ExternalPrompt(request, prompt) => {
                    if let Some(run) = self.runs.iter_mut().find(|r| r.request == request) {
                        run.status = text("ActionWaitingShort");
                        self.prompts.push_back(Question {
                            run: run.sequence,
                            prompt,
                            response: Zeroizing::new(String::new()),
                            focus: true,
                        });
                    } else {
                        // An already-retired owner cannot leave a broker waiting.
                        let _ = prompt.answer.send(None);
                    }
                }
                Event::Prompt(id, prompt) => {
                    if let Some(run) = self.runs.iter_mut().find(|r| r.sequence == id) {
                        run.status = text("ActionWaitingShort");
                    }
                    self.prompts.push_back(Question {
                        run: id,
                        prompt,
                        response: Zeroizing::new(String::new()),
                        focus: true,
                    });
                }
                Event::Stage(id, stage) => {
                    if let Some(run) = self.runs.iter_mut().find(|r| r.sequence == id) {
                        run.status = text(match stage {
                            network::Stage::Validating => "ActionStageValidating",
                            network::Stage::Fetching => "ActionStageFetching",
                            network::Stage::CheckingFetchedTip => "ActionStageCheckingFetchedTip",
                            network::Stage::Rebasing => "ActionStageRebasing",
                            network::Stage::Merging => "ActionStageMerging",
                            network::Stage::Pushing => "ActionStagePushing",
                            network::Stage::Reconciling => "ActionReinspecting",
                        });
                    }
                }
                Event::Completed(id, result) => {
                    if let Some(run) = self.runs.iter_mut().find(|r| r.sequence == id) {
                        run.done = true;
                        run.changed = true;
                        match *result {
                            Ok(result) => {
                                self.related_changes.extend(result.related.iter().cloned());
                                let pending = result
                                    .updated
                                    .as_ref()
                                    .is_ok_and(|s| !s.operations.is_empty());
                                let commands_succeeded =
                                    result.commands.iter().all(network::Command::succeeded);
                                let awaiting_recovery = pending
                                    && !result.has_conflicts
                                    && result.message.is_none()
                                    && commands_succeeded;
                                let succeeded = result.succeeded();
                                run.show_failure = !succeeded && !awaiting_recovery;
                                run.status = text(if succeeded && result.warning.is_some() {
                                    "RustActionWarning"
                                } else if succeeded {
                                    "ActionCompletedShort"
                                } else if result.has_conflicts {
                                    "ActionConflictShort"
                                } else if awaiting_recovery {
                                    "RecoveryPending"
                                } else if result.updated.is_err()
                                    && result.message.is_none()
                                    && commands_succeeded
                                {
                                    "ActionRefreshFailedShort"
                                } else {
                                    "ActionFailedShort"
                                });
                                run.output = result.output;
                                for message in
                                    [result.message, result.warning].into_iter().flatten()
                                {
                                    run.output.push_str(&format!("\n{}", text(&message)));
                                }
                                if let Err(error) = result.updated {
                                    run.output.push('\n');
                                    run.output.push_str(&error_log(&error));
                                }
                                if pending {
                                    run.output
                                        .push_str(&format!("\n{}", text("RecoveryPending")));
                                }
                                if result.output_truncated {
                                    run.output
                                        .push_str(&format!("\n{}", text("ActivityOutputLimit")));
                                }
                                if let Some(failure) = result.authentication_failure {
                                    run.output.push_str(&format!(
                                        "\n{}",
                                        text(match failure {
                                            authentication::Failure::Unavailable =>
                                                "RustAuthUnavailable",
                                            authentication::Failure::TimedOut => "RustAuthTimedOut",
                                            authentication::Failure::PromptLimit =>
                                                "RustAuthPromptLimit",
                                        })
                                    ));
                                }
                            }
                            Err(error) => {
                                run.show_failure = true;
                                run.status = text("ActionFailedShort");
                                run.output = error;
                            }
                        }
                    }
                }
                Event::OptionsReady(id, ready) => {
                    if let Some(run) = self.runs.iter_mut().find(|r| r.sequence == id) {
                        run.dialog_open = true;
                        run.status = text("RustOptionsOpen");
                    }
                    self.ready_options.push_back(*ready);
                }
                Event::OptionsClosed(id) => {
                    if let Some(run) = self.runs.iter_mut().find(|r| r.sequence == id) {
                        run.done = true;
                        run.status = text("RustOptionsClosed");
                    }
                }
                Event::Removed(id, outcome) => {
                    self.pending.retain(|p| p.sequence != id);
                    if let Some(run) = self.runs.iter_mut().find(|r| r.sequence == id) {
                        run.done = true;
                        run.show_failure = outcome == Outcome::Failed;
                        run.status = text(match outcome {
                            Outcome::Removed => "QueuedRequestCancelled",
                            Outcome::Failed => "ActionFailedShort",
                            _ => "ActionStoppedShort",
                        });
                    }
                }
            }
        }
        self.prompts.retain(|q| !q.prompt.closed.is_cancelled());
        while self.runs.len() > 128 {
            if let Some(index) = self.runs.iter().position(|run| run.done) {
                self.runs.remove(index);
            } else {
                break;
            }
        }
        if !self.prompts.is_empty() {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
        let mut changed: Vec<_> = self
            .runs
            .iter_mut()
            .filter_map(|r| std::mem::take(&mut r.changed).then(|| r.path.clone()))
            .collect();
        changed.append(&mut self.related_changes);
        changed
    }
    pub fn record_owned(&mut self, path: PathBuf, activity: crate::commit::OwnedActivity) {
        if let Some(run) = self.runs.iter_mut().find(|r| r.request == activity.request) {
            run.show_failure |= !run.done && activity.done && activity.failed;
            run.status = activity.status;
            run.output = activity.output;
            run.done = activity.done;
            run.reopen_commit = activity.reopen_commit;
        } else {
            let sequence = self.next;
            self.next += 1;
            self.runs.push(Run {
                sequence,
                request: activity.request,
                path,
                label: activity.label,
                status: activity.status,
                output: activity.output,
                done: activity.done,
                dialog_open: false,
                changed: false,
                reopen_commit: activity.reopen_commit,
                show_failure: activity.done && activity.failed,
            });
        }
    }
    pub fn record_local(
        &mut self,
        path: PathBuf,
        status: String,
        output: String,
        failure: Option<&'static str>,
    ) {
        let sequence = self.next;
        self.next += 1;
        self.runs.push(Run {
            sequence,
            request: 0,
            path,
            label: failure.unwrap_or("Commit"),
            status,
            output: network::redact(&output).chars().take(32768).collect(),
            done: true,
            dialog_open: false,
            changed: false,
            reopen_commit: false,
            show_failure: failure.is_some(),
        });
    }
    pub fn summary(&self, path: &std::path::Path) -> Option<String> {
        let mut runs = self
            .runs
            .iter()
            .rev()
            .filter(|r| crate::Workspace::same_path(&r.path, path));
        runs.clone()
            .find(|r| !r.done)
            .or_else(|| runs.next())
            .map(|r| format!("{} · {}", text(r.label), r.status))
    }
    pub fn progress(&self, path: &Path, scheduler: &Scheduler) -> Option<String> {
        let runs = self.runs.iter().rev().filter(|run| {
            !run.done && !run.dialog_open && crate::Workspace::same_path(&run.path, path)
        });
        runs.clone().next()?;
        let records = scheduler.active_records();
        let mut running = None;
        let mut waiting = None;
        let mut queued = 0;
        for run in runs {
            if let Some(record) = records
                .iter()
                .find(|record| record.id == run.request)
            {
                if record.running {
                    running.get_or_insert(run);
                } else {
                    waiting.get_or_insert(run);
                    queued += 1;
                }
            }
        }
        let mut message = if let Some(run) = running {
            format!("{} · {}", text(run.label), run.status)
        } else {
            strings::format("ActionQueued", &[&text(waiting?.label)])
        };
        if queued > usize::from(running.is_none()) {
            message.push_str(" · ");
            message.push_str(&strings::format(
                "OperationQueuedCount",
                &[&queued.to_string()],
            ));
        }
        Some(message)
    }
    pub fn status(
        &self,
        ui: &mut egui::Ui,
        path: Option<&PathBuf>,
        scheduler: &Scheduler,
    ) -> Option<PathBuf> {
        let mut reopen = None;
        let records = scheduler.active_records();
        for run in self
            .runs
            .iter()
            .filter(|r| path.is_none_or(|p| &r.path == p))
            .rev()
        {
            ui.horizontal(|ui| {
                ui.label(format!(
                    "{} · {} · {}",
                    run.path.file_name().unwrap_or_default().to_string_lossy(),
                    text(run.label),
                    run.status
                ));
                if !run.done
                    && records
                        .iter()
                        .any(|r| r.id == run.request && !r.running)
                    && ui.button(text("CancelWaitingRequest")).clicked()
                {
                    scheduler.remove_waiting(run.request);
                }
                if run.reopen_commit && ui.button(text("ReopenCommit")).clicked() {
                    reopen = Some(run.path.clone());
                }
                if !run.output.is_empty() {
                    egui::CollapsingHeader::new(text("ActionOutput"))
                        .id_salt(run.sequence)
                        .show(ui, |ui| {
                            egui::ScrollArea::both().max_height(170.).show(ui, |ui| {
                                ui.add(
                                    egui::Label::new(RichText::new(&run.output).monospace())
                                        .selectable(true),
                                );
                            });
                        });
                }
            });
        }
        reopen
    }
    pub fn show_prompt(&mut self, ctx: &egui::Context) {
        let Some(question) = self.prompts.front_mut() else {
            return;
        };
        let mut reply = None;
        egui::Modal::new(egui::Id::unique("authentication")).show(ctx, |ui| {
            ui.set_max_width(600.);
            ui.heading(text("AuthTitle"));
            if let Some(run) = self.runs.iter().find(|r| r.sequence == question.run) {
                ui.label(strings::format(
                    "AuthContext",
                    &[&run.path.to_string_lossy(), &text(run.label)],
                ));
            }
            ui.label(text("AuthPromptSource"));
            egui::ScrollArea::vertical()
                .max_height(220.)
                .show(ui, |ui| {
                    ui.add(egui::Label::new(&question.prompt.message).selectable(true));
                });
            ui.label(text("RustAuthExplanation"));
            if question.prompt.kind == authentication::Kind::Secret {
                ui.label(text("AuthResponse"));
                let mut output = egui::TextEdit::singleline(&mut *question.response)
                    .password(true)
                    .char_limit(4096)
                    .show(ui);
                output.state.clear_undoer();
                output.state.store(ctx, output.response.id);
                if question.focus {
                    output.response.request_focus();
                    question.focus = false;
                }
                if ui.button(text("AuthSubmit")).clicked()
                    || ui.input(|i| i.key_pressed(egui::Key::Enter))
                {
                    reply = Some(Some(question.response.as_bytes().to_vec().into()));
                }
            } else if ui.button(text("AuthApprove")).clicked() {
                reply = Some(Some(
                    if question.prompt.kind == authentication::Kind::Confirmation {
                        b"yes".to_vec()
                    } else {
                        vec![]
                    }
                    .into(),
                ));
            }
            if ui.button(text("RustAuthDecline")).clicked()
                || ui.input(|i| i.key_pressed(egui::Key::Escape))
            {
                reply = Some(None);
            }
        });
        if let Some(reply) = reply {
            question.response.zeroize();
            let _ = question.prompt.answer.send(reply);
            self.prompts.pop_front();
        }
    }

    fn next_failure(&self) -> Option<usize> {
        if !self.prompts.is_empty() {
            return None;
        }
        self.runs
            .iter()
            .position(|r| r.show_failure && Some(r.sequence) == self.active_failure)
            .or_else(|| self.runs.iter().position(|r| r.show_failure))
    }

    pub fn failure_path(&self) -> Option<&Path> {
        self.next_failure().map(|i| self.runs[i].path.as_path())
    }

    pub fn show_failure(&mut self, ctx: &egui::Context) {
        let Some(index) = self.next_failure() else {
            return;
        };
        let run = &mut self.runs[index];
        self.active_failure = Some(run.sequence);
        let mut open = true;
        let mut dismiss = false;
        egui::Window::new(text("OperationAttentionTitle"))
            .id(egui::Id::unique("operation-failure"))
            .open(&mut open)
            .collapsible(false)
            .order(egui::Order::Foreground)
            .default_width(580.)
            .show(ctx, |ui| {
                ui.heading(text(run.label));
                ui.add(egui::Label::new(run.path.to_string_lossy()).selectable(true));
                ui.add(egui::Label::new(&run.status).selectable(true));
                let output = run.output.trim();
                if !output.is_empty() && output != run.status.trim() {
                    ui.label(text("OperationAttentionLogs"));
                    egui::ScrollArea::both()
                        .id_salt(run.sequence)
                        .max_height(280.)
                        .show(ui, |ui| {
                            ui.add(
                                egui::Label::new(RichText::new(output).monospace())
                                    .selectable(true),
                            );
                        });
                }
                dismiss = ui.button(text("OperationAttentionDismiss")).clicked();
            });
        run.show_failure = open && !dismiss;
        if !run.show_failure {
            self.active_failure = None;
            ctx.request_repaint();
        }
    }
}

pub(crate) fn error_log(error: &shovel_core::operation::Error) -> String {
    use shovel_core::{git, operation::Error};
    let status = text(&error.code());
    match error {
        Error::Git(git::Error::Command { output, .. })
        | Error::Working(shovel_core::working::Error::Git(git::Error::Command {
            output, ..
        })) => git_error_message(&status, output),
        _ => status,
    }
}

pub(crate) fn git_error_message(status: &str, output: &str) -> String {
    let output: String = network::redact(output).chars().take(32768).collect();
    if output.is_empty() {
        status.to_owned()
    } else {
        format!("{status}\n{output}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shovel_core::process::{self, Lifetime};
    use std::{fs, path::Path, thread, time::Instant};
    pub(super) fn command(git: &Git, path: &Path, args: &[&str]) -> String {
        let output = process::run(
            &git.request(path, args.iter().copied(), Lifetime::AcceptedCommand, 65536),
            &Cancellation::default(),
            None,
        )
        .unwrap();
        assert!(output.success(), "{output:?}");
        String::from_utf8(output.stdout)
            .unwrap()
            .trim_end()
            .to_owned()
    }
    pub(super) fn fixture() -> (tempfile::TempDir, Arc<Git>, Snapshot) {
        let temporary = tempfile::tempdir().unwrap();
        let path = temporary.path();
        let mut git = Git::discover(None, &Cancellation::default()).unwrap();
        let config = path.join("isolated.config");
        fs::write(&config, "[user]\nname=Fixture\nemail=fixture@example.invalid\n[commit]\ngpgsign=false\n[push]\ndefault=current\n").unwrap();
        git.environment
            .insert("GIT_CONFIG_GLOBAL".into(), Some(config.into_os_string()));
        git.environment
            .insert("GIT_CONFIG_NOSYSTEM".into(), Some("1".into()));
        command(&git, path, &["init", "--initial-branch=main"]);
        command(&git, path, &["commit", "--allow-empty", "-m", "base"]);
        let remote = path.join("remote.git");
        command(&git, path, &["init", "--bare", remote.to_str().unwrap()]);
        command(
            &git,
            path,
            &["remote", "add", "origin", remote.to_str().unwrap()],
        );
        let snapshot = git.inspect(path, &Cancellation::default()).unwrap();
        (temporary, Arc::new(git), snapshot)
    }
    pub(super) fn wait(mut condition: impl FnMut() -> bool) {
        let until = Instant::now() + Duration::from_secs(20);
        while !condition() {
            assert!(
                Instant::now() < until,
                "worker did not reach expected state"
            );
            thread::sleep(Duration::from_millis(20));
        }
    }

    #[test]
    fn network_options_are_modal_persist_on_close_and_do_not_execute() {
        let (_temp, git, snapshot) = fixture();
        let mut controller = Controller {
            options: Some((
                Options::load(&Settings::default(), &snapshot, Kind::Pull),
                snapshot.clone(),
            )),
            ..Default::default()
        };
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let mut settings = Settings::default();
        let scheduler = Scheduler::default();
        assert!(controller.modal_open());
        let render = |controller: &mut Controller, settings: &mut Settings, events| {
            ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(800., 600.),
                    )),
                    events,
                    ..Default::default()
                },
                |_| {
                    controller.options_dialog(&ctx, settings, Some(&git), &scheduler);
                },
            )
        };
        let mut nodes = std::collections::HashMap::new();
        for _ in 0..3 {
            let out = render(&mut controller, &mut settings, vec![]);
            if let Some(update) = &out.platform_output.accesskit_update {
                for (_, node) in &update.nodes {
                    if let (Some(label), Some(r)) = (node.label(), node.bounds()) {
                        nodes.insert(
                            label.to_owned(),
                            egui::pos2(((r.x0 + r.x1) / 2.) as f32, ((r.y0 + r.y1) / 2.) as f32),
                        );
                    }
                }
            }
            out.drop_without_applying_deltas();
        }
        for label in [text("ActionPullMerge"), text("Close")] {
            let point = nodes[&label];
            for events in [
                vec![egui::Event::PointerMoved(point)],
                vec![egui::Event::PointerButton {
                    pos: point,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                }],
                vec![egui::Event::PointerButton {
                    pos: point,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                }],
            ] {
                render(&mut controller, &mut settings, events).drop_without_applying_deltas();
            }
        }
        assert!(!controller.modal_open());
        assert_eq!(
            Options::load(&settings, &snapshot, Kind::Pull).strategy,
            Strategy::Merge
        );
        assert!(!scheduler.has_accepted_work());
        assert!(controller.runs.is_empty());
    }
    #[test]
    fn remembered_flags_persist_and_explicit_targets_are_branch_bound() {
        let (temporary, _, snapshot) = fixture();
        let mut settings = Settings::default();
        let mut options = Options::load(&settings, &snapshot, Kind::Pull);
        options.auto_stash = true;
        options.strategy = Strategy::Merge;
        options.save(&mut settings, &snapshot);
        assert!(matches!(
            Options::load(&settings, &snapshot, Kind::Pull).action(),
            Action::Pull {
                target: None,
                strategy: Strategy::Merge,
                auto_stash: true
            }
        ));
        options.remote = "chosen".into();
        options.branch = "destination".into();
        options.explicit = true;
        options.save(&mut settings, &snapshot);
        let directory = temporary.path().join("profile");
        let mut store = shovel_core::settings::SettingsStore::new(directory.clone());
        store.load().unwrap();
        store.save(&settings).unwrap();
        let restored = shovel_core::settings::SettingsStore::new(directory)
            .load()
            .unwrap();
        assert_eq!(Options::load(&restored, &snapshot, Kind::Pull), options);
        let mut other_branch = snapshot.clone();
        other_branch.branch = Some("next".into());
        let other = Options::load(&restored, &other_branch, Kind::Pull);
        assert!(!other.explicit);
        assert!(other.auto_stash);
        assert_eq!(other.strategy, Strategy::Merge);
        let mut fetch = Options::load(&settings, &snapshot, Kind::Fetch);
        fetch.remote = "chosen".into();
        fetch.all_remotes = true;
        fetch.explicit = true;
        fetch.save(&mut settings, &snapshot);
        assert_eq!(Options::load(&settings, &other_branch, Kind::Fetch), fetch);
    }
    #[test]
    fn toolbar_push_reaches_native_git_and_survives_leaving_its_view() {
        let (temporary, git, snapshot) = fixture();
        let path = temporary.path();
        let helper =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/debug/ShovelAskPass.exe");
        assert!(
            helper.is_file(),
            "build shovel-askpass before this focused check"
        );
        let mut controller = Controller {
            helper: Some(helper),
            ..Default::default()
        };
        let scheduler = Scheduler::default();
        let mut settings = Settings::default();
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000., 400.));
        let hook = path.join(".git/hooks/pre-push");
        fs::write(&hook, "#!/bin/sh\necho started > .git/entered\nwhile [ ! -f .git/release ]; do sleep 0.05; done\necho 'warning: fixture push succeeded' >&2\n").unwrap();
        struct Release(PathBuf);
        impl Drop for Release {
            fn drop(&mut self) {
                let _ = fs::write(&self.0, "done");
            }
        }
        let release = Release(path.join(".git/release"));
        let output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(screen),
                ..Default::default()
            },
            |ui| {
                ui.horizontal(|ui| {
                    controller.toolbar(
                        ui,
                        &mut settings,
                        Some(&snapshot.worktree),
                        Some(&snapshot),
                        Some(&git),
                        &scheduler,
                    );
                });
            },
        );
        let node = output
            .platform_output
            .accesskit_update
            .as_ref()
            .unwrap()
            .nodes
            .iter()
            .find(|(_, n)| n.label() == Some("Push"))
            .unwrap();
        let bounds = node.1.bounds().unwrap();
        let point = egui::pos2(
            (bounds.x0 + bounds.x1) as f32 / 2.,
            (bounds.y0 + bounds.y1) as f32 / 2.,
        );
        output.drop_without_applying_deltas();
        for events in [
            vec![egui::Event::PointerMoved(point)],
            vec![egui::Event::PointerButton {
                pos: point,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            }],
            vec![egui::Event::PointerButton {
                pos: point,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            }],
        ] {
            ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(screen),
                    events,
                    ..Default::default()
                },
                |ui| {
                    ui.horizontal(|ui| {
                        controller.toolbar(
                            ui,
                            &mut settings,
                            Some(&snapshot.worktree),
                            Some(&snapshot),
                            Some(&git),
                            &scheduler,
                        );
                    });
                },
            )
            .drop_without_applying_deltas();
        }
        assert_eq!(controller.runs.len(), 1);
        wait(|| {
            controller.poll(&ctx);
            path.join(".git/entered").is_file()
        });
        assert!(scheduler.has_accepted_work());
        assert_eq!(
            controller.progress(path, &scheduler).as_deref(),
            Some("Push · Pushing")
        );
        // No further view renders while the accepted native hook is held.
        thread::sleep(Duration::from_millis(80));
        assert!(scheduler.has_accepted_work());
        drop(release);
        wait(|| {
            controller.poll(&ctx);
            controller.runs[0].done && !scheduler.has_accepted_work()
        });
        assert_eq!(
            controller.runs[0].status,
            text("ActionCompletedShort"),
            "{}",
            controller.runs[0].output
        );
        assert!(controller.runs[0].output.contains("fixture push succeeded"));
        assert!(controller.progress(path, &scheduler).is_none());
        assert!(!controller.runs[0].show_failure);
        assert_eq!(
            command(&git, &path.join("remote.git"), &["rev-parse", "main"]),
            snapshot.head.unwrap()
        );
        command(&git, path, &["commit", "--allow-empty", "-m", "rejected"]);
        let snapshot = git.inspect(path, &Cancellation::default()).unwrap();
        fs::write(
            &hook,
            "#!/bin/sh\necho 'fixture push rejected' >&2\nexit 1\n",
        )
        .unwrap();
        controller.submit(
            Options::load(&settings, &snapshot, Kind::Push),
            &snapshot,
            git,
            &scheduler,
            &ctx,
        );
        wait(|| {
            controller.poll(&ctx);
            controller.runs.len() == 2 && controller.runs[1].done
        });
        assert!(controller.runs[1].show_failure);
        let frame = |controller: &mut Controller| {
            ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(screen),
                    ..Default::default()
                },
                |_| controller.show_failure(&ctx),
            )
        };
        frame(&mut controller).drop_without_applying_deltas();
        let output = frame(&mut controller);
        assert!(
            output
                .platform_output
                .accesskit_update
                .as_ref()
                .unwrap()
                .nodes
                .iter()
                .any(|(_, node)| {
                    node.label()
                        .or_else(|| node.value())
                        .is_some_and(|value| value.contains("fixture push rejected"))
                })
        );
        output.drop_without_applying_deltas();
    }
    #[test]
    fn password_prompt_masks_accessibility_and_closes_when_broker_finishes() {
        let mut controller = Controller::default();
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let (answer, _received) = mpsc::channel();
        let closed = Cancellation::default();
        controller.prompts.push_back(Question {
            run: 1,
            prompt: authentication::Prompt {
                kind: authentication::Kind::Secret,
                message: "Fixture password:".into(),
                answer,
                closed: closed.clone(),
            },
            response: Zeroizing::new("fixture-secret".into()),
            focus: true,
        });
        let output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(900., 620.),
                )),
                ..Default::default()
            },
            |_ui| controller.show_prompt(&ctx),
        );
        let update = output.platform_output.accesskit_update.as_ref().unwrap();
        assert!(!format!("{update:?}").contains("fixture-secret"));
        output.drop_without_applying_deltas();
        assert!(controller.modal_open());
        closed.cancel();
        controller.poll(&ctx);
        assert!(!controller.modal_open());
    }
}

#[cfg(test)]
mod activity_layout_test {
    use super::*;
    use crate::workspace_interaction_tests::{frame, workspace};
    #[test]
    fn retained_operation_output_does_not_push_down_repository_tabs() {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let mut app = workspace();
        let tab_y = |app: &mut crate::Workspace| {
            let output = frame(&ctx, app, vec![]);
            let y = output
                .platform_output
                .accesskit_update
                .as_ref()
                .unwrap()
                .nodes
                .iter()
                .find(|(_, n)| n.label() == Some("Bravo"))
                .unwrap()
                .1
                .bounds()
                .unwrap()
                .y0;
            output.drop_without_applying_deltas();
            y
        };
        let before = tab_y(&mut app);
        for seq in 1..=8 {
            app.operations.runs.push(Run {
                sequence: seq,
                request: seq,
                path: "Bravo".into(),
                label: "Fetch",
                status: "complete".into(),
                output: "command output\n".repeat(20),
                done: true,
                dialog_open: false,
                changed: false,
                reopen_commit: false,
                show_failure: false,
            });
        }
        let after = tab_y(&mut app);
        assert_eq!(before, after);
        app.activity_open = true;
        assert_eq!(tab_y(&mut app), after);
    }
}

#[cfg(test)]
#[path = "operation_queue_tests.rs"]
mod queue_tests;
