use crate::{
    path_text::{self, PathText},
    strings::{self, text},
};
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
    remember: bool,
}
impl Options {
    fn saved<'a>(settings: &'a Settings, path: &Path, kind: Kind) -> Option<&'a serde_json::Value> {
        settings
            .get("NetworkActions")?
            .as_array()?
            .iter()
            .find(|entry| {
                entry["Action"].as_u64() == Some(kind.number())
                    && entry["Path"]
                        .as_str()
                        .is_some_and(|p| crate::Workspace::same_path(Path::new(p), path))
            })
    }
    fn confirmed(settings: &Settings, path: &Path, kind: Kind) -> bool {
        Self::saved(settings, path, kind)
            .is_some_and(|entry| entry["Confirmed"].as_bool() == Some(true))
    }
    fn queue_key(&self, snapshot: &Snapshot) -> String {
        use sha2::{Digest, Sha256};
        let bytes = serde_json::to_vec(&(&snapshot.branch, self.action()))
            .expect("network action contains only strings and flags");
        format!("network:{}:{:x}", self.kind.key(), Sha256::digest(bytes))
    }
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
        let (remote, destination) = snapshot
            .upstream_target
            .as_ref()
            .map(|target| (target.remote.clone(), target.branch.clone()))
            .or(upstream)
            .unwrap_or_else(|| {
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
            remember: true,
        };
        let saved = Self::saved(settings, &snapshot.worktree, kind);
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
    fn save(&self, settings: &mut Settings, snapshot: &Snapshot, confirmed_now: bool) {
        let confirmed = confirmed_now || Self::confirmed(settings, &snapshot.worktree, self.kind);
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
                    .is_some_and(|p| crate::Workspace::same_path(Path::new(p), &snapshot.worktree)))
        });
        entries.insert(
            0,
            serde_json::json!({
                "Path": path,
                "Action": self.kind.number(),
                "Branch": snapshot.branch.as_deref().unwrap_or_default(),
                "SourceBranch": self.source,
                "Remote": self.remote,
                "RemoteBranch": self.branch,
                "Strategy": if self.strategy == Strategy::Merge { 1 } else { 0 },
                "AutoStash": self.auto_stash,
                "AllRemotes": self.all_remotes,
                "AllTags": self.all_tags,
                "SetUpstream": self.set_upstream,
                "ExplicitTarget": self.explicit,
                "Confirmed": confirmed,
            }),
        );
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
                    commit: None,
                    destination: target,
                }),
                all_tags: self.all_tags,
                set_upstream: self.set_upstream,
            },
        }
    }

    fn show_destinations(&mut self, ui: &mut egui::Ui, snapshot: &Snapshot, field_width: f32) {
        let remote_label = ui.label(text("ActionRemote"));
        let old = self.remote.clone();
        ui.add_enabled_ui(self.kind != Kind::Fetch || !self.all_remotes, |ui| {
            egui::ComboBox::from_id_salt("remote")
                .width(field_width)
                .selected_text(display_network_target(&self.remote))
                .show_ui(ui, |ui| {
                    for remote in &snapshot.remotes {
                        ui.selectable_value(&mut self.remote, remote.clone(), remote);
                    }
                })
                .response
                .labelled_by(remote_label.id);
        });
        self.explicit |= self.remote != old;
        ui.end_row();

        if self.kind != Kind::Fetch {
            let branch_label = ui.label(text("NetworkBranch"));
            let branch = if self.remote.is_empty() || self.branch.is_empty() {
                display_network_target(&self.branch)
            } else {
                format!("{}/{}", self.remote, self.branch)
            };
            egui::ComboBox::from_id_salt("remote-branch")
                .width(field_width)
                .selected_text(branch)
                .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
                .show_ui(ui, |ui| {
                    let name = text("NetworkBranchName");
                    let entry = ui.add_sized(
                        egui::vec2(ui.available_width(), 26.),
                        egui::TextEdit::singleline(&mut self.branch)
                            .char_limit(1024)
                            .hint_text(&name),
                    );
                    self.explicit |= entry.changed();
                    if entry.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter)) {
                        ui.close();
                    }
                    let branches = cached_remote_branches(snapshot, &self.remote);
                    if !branches.is_empty() {
                        ui.separator();
                    }
                    for name in branches {
                        if ui.selectable_label(self.branch == name, name).clicked() {
                            self.explicit |= self.branch != name;
                            self.branch = name.into();
                            ui.close();
                        }
                    }
                })
                .response
                .labelled_by(branch_label.id);
            ui.end_row();
        }

        match self.kind {
            Kind::Fetch => {}
            Kind::Pull => {
                ui.label(text("NetworkInto"));
                let branch = snapshot.branch.clone().unwrap_or_else(|| {
                    snapshot.head.as_deref().map_or_else(
                        || text("NetworkNoTarget"),
                        |head| strings::format("DetachedHead", &[head.get(..7).unwrap_or(head)]),
                    )
                });
                ui.monospace(branch);
                ui.end_row();
            }
            Kind::Push => {
                let source = ui.label(text("ActionBranchLabel"));
                if self.remember {
                    let old = self.source.clone();
                    egui::ComboBox::from_id_salt("source")
                        .width(field_width)
                        .selected_text(display_network_target(&self.source))
                        .show_ui(ui, |ui| {
                            for reference in &snapshot.references {
                                if let Some(branch) = reference.name.strip_prefix("refs/heads/") {
                                    ui.selectable_value(
                                        &mut self.source,
                                        branch.to_owned(),
                                        branch,
                                    );
                                }
                            }
                        })
                        .response
                        .labelled_by(source.id);
                    self.explicit |= self.source != old;
                } else {
                    ui.monospace(display_network_target(&self.source));
                }
                ui.end_row();
            }
        }
    }

    fn show_flags(&mut self, ui: &mut egui::Ui) {
        use crate::workspace_style::dialog_checkbox as checkbox;
        match self.kind {
            Kind::Fetch => {
                checkbox(ui, &mut self.all_remotes, text("ActionAllRemotes"));
            }
            Kind::Pull => {
                let mut rebase = self.strategy == Strategy::Rebase;
                if checkbox(ui, &mut rebase, text("NetworkRebase"))
                    .on_hover_text(text("ActionPullStrategyHelp"))
                    .changed()
                {
                    self.strategy = if rebase {
                        Strategy::Rebase
                    } else {
                        Strategy::Merge
                    };
                }
                checkbox(ui, &mut self.auto_stash, text("ActionAutoStash"))
                    .on_hover_text(text("ActionAutoStashHelp"));
            }
            Kind::Push => {
                checkbox(ui, &mut self.all_tags, text("ActionAllTags"));
                checkbox(ui, &mut self.set_upstream, text("ActionSetUpstream"));
            }
        }
    }
}

fn display_network_target(value: &str) -> String {
    if value.is_empty() {
        text("NetworkNoTarget")
    } else {
        value.to_owned()
    }
}

fn cached_remote_branches<'a>(snapshot: &'a Snapshot, remote: &str) -> Vec<&'a str> {
    snapshot
        .references
        .iter()
        .filter_map(|reference| {
            let path = reference.name.strip_prefix("refs/remotes/")?;
            let (owner, branch) = snapshot
                .remotes
                .iter()
                .filter_map(|owner| {
                    let branch = path.strip_prefix(owner)?.strip_prefix('/')?;
                    Some((owner, branch))
                })
                .max_by_key(|(owner, _)| owner.len())?;
            (owner == remote && branch != "HEAD").then_some(branch)
        })
        .collect()
}
enum LocalAction {
    Branch(shovel_core::branches::Action),
    History(shovel_core::history_actions::Action),
    Recovery(shovel_core::recovery::Action),
    Stash(shovel_core::stashes::Action),
    Remote(shovel_core::remotes::Action),
    Submodule(shovel_core::submodules::Action),
    Repository(shovel_core::repository_actions::Action),
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
            Self::Repository(action) => format!("repository:{:x}", Sha256::digest(format!("{action:?}"))),
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
    ExternalPrompt(u64, PathBuf, &'static str, authentication::Prompt),
    Completed(u64, Box<Result<network::Completed, String>>),
    StashConflict(PathBuf, Option<Arc<shovel_core::stashes::ConflictOrigin>>),
    Removed(u64, Outcome, Option<&'static str>),
    OptionsReady(u64, Box<ReadyOptions>),
    OptionsClosed(u64),
}
impl Event {
    fn completed(sequence: u64, result: Result<network::Completed, String>) -> Self {
        let result = result.map(|mut result| {
            crate::activity_output::prepare_git(&mut result);
            result
        });
        Self::Completed(sequence, Box::new(result))
    }
}

#[derive(Default, PartialEq, Eq, Clone, Copy)]
pub enum ActivityFilter {
    #[default]
    All,
    User,
    Background,
}

struct Run {
    background: bool,
    sequence: u64,
    request: u64,
    path: PathBuf,
    label: &'static str,
    status: String,
    output: String,
    done: bool,
    dialog_open: bool,
    reopen_commit: Option<Arc<crate::commit::FailedCommit>>,
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
    Toolbar(Kind),
    Network(Kind),
    Options(Kind),
    NewBranch,
}
impl RequestKind {
    fn for_worktree(self, settings: &Settings, worktree: &Path) -> Self {
        match self {
            Self::Toolbar(kind) if Options::confirmed(settings, worktree, kind) => {
                Self::Network(kind)
            }
            Self::Toolbar(kind) => Self::Options(kind),
            action => action,
        }
    }
    fn label(self) -> &'static str {
        match self {
            Self::Toolbar(kind) | Self::Network(kind) => kind.key(),
            Self::Options(Kind::Fetch) => "RustFetchOptions",
            Self::Options(Kind::Pull) => "PullOptions",
            Self::Options(Kind::Push) => "PushOptions",
            Self::NewBranch => "ActionTitleNewBranch",
        }
    }
    fn lane(self) -> Lane {
        match self {
            Self::Toolbar(Kind::Fetch)
            | Self::Network(Kind::Fetch)
            | Self::Options(Kind::Fetch) => Lane::Fetch,
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
    bound: Option<(String, Option<String>)>,
}
pub struct Controller {
    pub filter: ActivityFilter,
    pub divergence: crate::divergence::Display,
    pending: Vec<PendingNetwork>,
    refresh_paths: Vec<PathBuf>,
    stash_conflicts: VecDeque<Arc<shovel_core::stashes::ConflictOrigin>>,
    options: Option<(Options, Snapshot)>,
    options_opened: bool,
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
            filter: ActivityFilter::default(),
            divergence: Default::default(),
            pending: vec![],
            refresh_paths: vec![],
            stash_conflicts: VecDeque::new(),
            options: None,
            options_opened: false,
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
#[derive(Clone)]
pub(crate) struct ExternalAuthentication {
    sender: Sender<Event>,
    helper: Option<PathBuf>,
}
impl ExternalAuthentication {
    pub(crate) fn start(
        self,
        request: u64,
        path: PathBuf,
        label: &'static str,
        ctx: &egui::Context,
    ) -> Result<authentication::Session, shovel_core::operation::Error> {
        let ctx = ctx.clone();
        let handler = Arc::new(move |prompt| {
            let _ = self
                .sender
                .send(Event::ExternalPrompt(request, path.clone(), label, prompt));
            ctx.request_repaint();
        });
        let helper = match self.helper {
            Some(path) => path,
            None => std::env::current_exe()
                .map_err(|_| shovel_core::operation::Error::Code("ActionAskPassMissing"))?
                .with_file_name("ShovelAskPass.exe"),
        };
        authentication::Session::start(&helper, handler)
            .map_err(|_| shovel_core::operation::Error::Code("ActionAskPassMissing"))
    }
}
impl Controller {
    pub(crate) fn external_authentication(&self) -> ExternalAuthentication {
        ExternalAuthentication {
            sender: self.sender.clone(),
            helper: self.helper.clone(),
        }
    }
    #[cfg(test)]
    pub(crate) fn set_authentication_helper(&mut self, helper: PathBuf) {
        self.helper = Some(helper);
    }
    pub fn active_modal_open(&self) -> bool {
        self.authentication_open() || self.options.is_some()
    }
    pub fn authentication_open(&self) -> bool {
        !self.prompts.is_empty()
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
        settings: &Settings,
        path: Option<&PathBuf>,
        snapshot: Option<&Snapshot>,
        git: Option<&Arc<Git>>,
        scheduler: &Scheduler,
    ) {
        let available = path.is_some() && git.is_some() && !self.active_modal_open();
        for kind in [Kind::Fetch, Kind::Pull, Kind::Push] {
            ui.push_id(kind.number(), |ui| {
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
                if path.is_none() || git.is_none() {
                    button
                        .clone()
                        .on_disabled_hover_text(text("ActionInspectionNeeded"));
                }
                let main_clicked = button.clicked();
                let options_clicked = crate::workspace_style::options_button(
                    ui,
                    strings::format("RustNetworkOptions", &[&text(kind.key())]),
                    available,
                )
                .clicked();
                if (main_clicked || options_clicked)
                    && let (Some(path), Some(git)) = (path, git)
                {
                    if options_clicked {
                        self.open_network_options(
                            kind,
                            path,
                            snapshot,
                            settings,
                            scheduler,
                            ui.ctx(),
                        );
                    } else if let Some(snapshot) = snapshot {
                        let options = Options::load(settings, snapshot, kind);
                        if Options::confirmed(settings, &snapshot.worktree, kind) {
                            self.submit(options, snapshot, git.clone(), scheduler, ui.ctx());
                        } else {
                            self.options = Some((options, snapshot.clone()));
                        }
                    } else {
                        self.defer(
                            RequestKind::Toolbar(kind),
                            path,
                            settings,
                            scheduler,
                            ui.ctx(),
                        );
                    }
                }
            });
        }
    }
    pub fn open_network_options(
        &mut self,
        kind: Kind,
        path: &Path,
        snapshot: Option<&Snapshot>,
        settings: &Settings,
        scheduler: &Scheduler,
        ctx: &egui::Context,
    ) {
        if self.active_modal_open() {
            return;
        }
        if let Some(snapshot) = snapshot {
            self.options = Some((Options::load(settings, snapshot, kind), snapshot.clone()));
        } else {
            self.defer(RequestKind::Options(kind), path, settings, scheduler, ctx);
        }
    }
    pub fn open_branch_push(
        &mut self,
        snapshot: &Snapshot,
        reference: &shovel_core::git::Reference,
    ) {
        if self.active_modal_open() {
            return;
        }
        let Some(branch) = reference.name.strip_prefix("refs/heads/") else {
            return;
        };
        let mut options = Options::load(&Settings::default(), snapshot, Kind::Push);
        if snapshot.branch.as_deref() != Some(branch) {
            options.branch = branch.to_owned();
            options.remote = snapshot
                .remotes
                .iter()
                .find(|remote| *remote == "origin")
                .or(snapshot.remotes.first())
                .cloned()
                .unwrap_or_default();
        }
        options.source = branch.to_owned();
        if let Some(upstream) = reference
            .upstream
            .as_deref()
            .and_then(|name| name.strip_prefix("refs/remotes/"))
            && let Some((remote, destination)) = snapshot
                .remotes
                .iter()
                .filter_map(|remote| {
                    upstream
                        .strip_prefix(&format!("{remote}/"))
                        .map(|name| (remote, name))
                })
                .max_by_key(|(remote, _)| remote.len())
        {
            options.remote.clone_from(remote);
            options.branch = destination.to_owned();
        }
        options.explicit = true;
        options.remember = false;
        self.options = Some((options, snapshot.clone()));
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
        let already_confirmed = Options::confirmed(settings, &snapshot.worktree, kind);
        let mut confirmed_now = false;
        let mut open = true;
        let mut close = false;
        let popup_was_open = egui::Popup::is_any_open(ctx);
        let width = (ctx.content_rect().width() - 46.).clamp(236., 442.);
        let title = strings::format("RustNetworkOptions", &[&text(kind.key())]);
        let window = egui::Window::new(&title)
            .id(egui::Id::unique("network-options"))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .default_width(width)
            .max_width(width);
        crate::workspace_style::show_closable_window(ctx, window, &title, &mut open, |ui| {
            use crate::workspace_style::Palette;
            ui.set_width(width);
            ui.spacing_mut().item_spacing.y = 6.;
            ui.spacing_mut().interact_size.y = 26.;
            let palette = Palette::current(ui);
            egui::ScrollArea::vertical()
                .max_height((ctx.content_rect().height() - 128.).max(80.))
                .show(ui, |ui| {
                    let path = snapshot.worktree.to_string_lossy();
                    let repository = PathText::new(ctx, &path);
                    ui.add(
                        egui::Label::new(RichText::new(repository.visual()).strong()).truncate(),
                    )
                    .on_hover_text(repository.spoken());
                    if options.remember && !already_confirmed {
                        ui.label(RichText::new(text("NetworkConfirmOptions")).color(palette.muted));
                    }
                    ui.separator();
                    let label_width = 97.;
                    let gap = 12.;
                    egui::Grid::new("network-destinations")
                        .num_columns(2)
                        .min_col_width(label_width)
                        .spacing([gap, 7.])
                        .show(ui, |ui| {
                            options.show_destinations(ui, &snapshot, width - label_width - gap);
                        });
                    ui.add_space(4.);
                    ui.horizontal_top(|ui| {
                        ui.add_space(label_width + gap - ui.spacing().item_spacing.x);
                        ui.vertical(|ui| options.show_flags(ui));
                    });
                    ui.label(
                        RichText::new(text(if options.explicit {
                            "ActionCachedTargets"
                        } else {
                            "RustNetworkConfigured"
                        }))
                        .small()
                        .color(palette.muted),
                    );
                    if options.remember
                        && options.explicit
                        && ui.button(text("RustNetworkUseConfigured")).clicked()
                    {
                        options.explicit = false;
                    }
                });
            ui.separator();
            ui.horizontal(|ui| {
                let button_size = egui::vec2(85., 29.);
                ui.add_space(
                    ui.available_width() - button_size.x * 2. - ui.spacing().item_spacing.x,
                );
                let mut primary = egui::Button::new(RichText::new(text(kind.key())).strong())
                    .min_size(button_size);
                if git.is_some() {
                    let (fill, foreground) = if palette.high_contrast {
                        (palette.active_selection, palette.selection_text)
                    } else if ui.visuals().dark_mode {
                        (palette.accent, palette.chrome)
                    } else {
                        (palette.accent, palette.selection_text)
                    };
                    primary = egui::Button::new(
                        RichText::new(text(kind.key())).strong().color(foreground),
                    )
                    .min_size(button_size)
                    .fill(fill);
                }
                let primary = ui.add_enabled(git.is_some(), primary);
                let cancel =
                    ui.add(egui::Button::new(text("CancelOperation")).min_size(button_size));
                if !self.options_opened && !ui.is_sizing_pass() {
                    if primary.enabled() {
                        primary.request_focus();
                    } else {
                        cancel.request_focus();
                    }
                    self.options_opened = true;
                }
                if primary.has_focus() {
                    ui.painter().rect_stroke(
                        primary.rect.expand(2.),
                        4.,
                        egui::Stroke::new(1.5, palette.focus),
                        egui::StrokeKind::Outside,
                    );
                }
                if primary.clicked()
                    && let Some(git) = git
                {
                    let command =
                        self.submit(options.clone(), &snapshot, git.clone(), scheduler, ctx);
                    if let Some(lease) = &self.options_hold {
                        lease.continue_with(scheduler, command);
                    }
                    // A matching command can already be queued for this worktree.
                    confirmed_now = true;
                    close = true;
                } else if cancel.clicked() {
                    close = true;
                }
            });
        });
        let changed =
            options.remember && (options != before || (confirmed_now && !already_confirmed));
        if changed {
            options.save(settings, &snapshot, confirmed_now);
        }
        let escape = !popup_was_open && ctx.input(|input| input.key_pressed(egui::Key::Escape));
        if open && !close && !escape {
            self.options = Some((options, snapshot));
        } else {
            // Release the repository lane only after remembered choices are saved
            // and any explicit submission has entered the queue.
            self.options_hold.take();
            self.options_opened = false;
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
        key: String,
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
            key,
            action.lane(),
            move |outcome| {
                let _ = sender.send(Event::Removed(sequence, outcome, None));
                ctx.request_repaint();
            },
        );
        if !accepted {
            return None;
        }
        self.runs.push(Run {
            background: false,
            sequence,
            request,
            path: path.to_path_buf(),
            label: action.label(),
            status: text("OperationWaiting"),
            output: String::new(),
            done: false,
            dialog_open: false,
            reopen_commit: None,
            show_failure: false,
        });
        self.trim_finished();
        Some((request, sequence))
    }

    #[cfg(test)]
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
        let key = match action {
            RequestKind::Toolbar(kind) | RequestKind::Network(kind) => {
                use sha2::{Digest, Sha256};
                let saved = serde_json::to_vec(&Options::saved(settings, path, kind))
                    .expect("saved network preferences are JSON");
                format!("{action:?}:pending:{:x}", Sha256::digest(saved))
            }
            _ => format!("dialog:{action:?}"),
        };
        if let Some((request, sequence)) =
            self.reserve_run(action, path, (&owner, &owner), key, scheduler, ctx)
        {
            self.pending.push(PendingNetwork {
                request,
                sequence,
                path: path.to_path_buf(),
                action,
                settings: settings.clone(),
                bound: None,
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
        &mut self,
        path: &std::path::Path,
        snapshot: &Snapshot,
        scheduler: &Scheduler,
    ) {
        let bindings: Vec<_> = self
            .pending
            .iter_mut()
            .filter(|p| {
                p.bound.is_none()
                    && (crate::Workspace::same_path(&p.path, path)
                        || crate::Workspace::same_path(&p.path, &snapshot.worktree))
            })
            .map(|p| {
                let key = match p.action.for_worktree(&p.settings, &snapshot.worktree) {
                    RequestKind::Network(kind) => {
                        Options::load(&p.settings, snapshot, kind).queue_key(snapshot)
                    }
                    action => format!("dialog:{action:?}"),
                };
                p.bound = Some((
                    snapshot.identity.fingerprint.clone(),
                    snapshot.branch.clone(),
                ));
                (p.request, key)
            })
            .collect();
        if bindings.is_empty() {
            return;
        }
        scheduler.bind_reserved(
            &bindings,
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
        if let Ok(snapshot) = result {
            self.bind_initialization(path, snapshot, scheduler);
        }
        let (pending, others): (Vec<_>, Vec<_>) = std::mem::take(&mut self.pending)
            .into_iter()
            .partition(|p| {
                crate::Workspace::same_path(&p.path, path)
                    || result.is_ok_and(|s| crate::Workspace::same_path(&p.path, &s.worktree))
            });
        self.pending = others;
        if pending.is_empty() {
            return;
        }
        match result {
            Ok(snapshot) => {
                for request in pending {
                    let preparation_error = if request.bound.as_ref().is_some_and(|(owner, branch)| {
                        owner != &snapshot.identity.fingerprint || branch != &snapshot.branch
                    }) {
                        Some("ActionChanged")
                    } else if request.action == RequestKind::NewBranch
                        && !snapshot.operations.is_empty()
                    {
                        Some("ActionSequence")
                    } else {
                        None
                    };
                    if let Some(error) = preparation_error {
                        // Report a rejected action in its reserved slot so it cannot
                        // stop earlier conflicting work that has not yet started.
                        let sender = self.sender.clone();
                        let ctx = ctx.clone();
                        scheduler.activate(request.request, move || {
                            let _ = sender.send(Event::Removed(
                                request.sequence,
                                Outcome::Failed,
                                Some(error),
                            ));
                            ctx.request_repaint();
                            Outcome::Failed
                        });
                        continue;
                    }
                    let action = request
                        .action
                        .for_worktree(&request.settings, &snapshot.worktree);
                    if let RequestKind::Network(kind) = action {
                        self.activate_network(
                            (request.request, request.sequence),
                            Options::load(&request.settings, snapshot, kind),
                            snapshot,
                            git.clone(),
                            scheduler,
                            ctx,
                        );
                    } else {
                        let sender = self.sender.clone();
                        let ctx = ctx.clone();
                        let snapshot = snapshot.clone();
                        scheduler.activate(request.request, move || {
                            let (release, held) = mpsc::channel();
                            let ready = ReadyOptions {
                                options: if let RequestKind::Options(kind) = action {
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
            options.queue_key(snapshot),
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
        self.submit(options, snapshot, git, scheduler, ctx);
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
            let _ = sender.send(Event::completed(sequence, result));
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
    pub(super) fn submit_repository_action(
        &mut self, label: &'static str, action: shovel_core::repository_actions::Action,
        snapshot: &Snapshot, git: Arc<Git>, scheduler: &Scheduler, ctx: &egui::Context,
    ) {
        self.submit_local(label, LocalAction::Repository(action), snapshot, git, scheduler, ctx);
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
                let start_authentication = || {
                    let helper = match &helper {
                        Some(path) => path.clone(),
                        None => std::env::current_exe()
                            .map_err(|_| text("ActionAskPassMissing"))?
                            .with_file_name("ShovelAskPass.exe"),
                    };
                    let prompt_sender = sender.clone();
                    let prompt_ctx = ctx.clone();
                    authentication::Session::start(
                        &helper,
                        Arc::new(move |prompt| {
                            let _ = prompt_sender.send(Event::Prompt(sequence, prompt));
                            prompt_ctx.request_repaint();
                        }),
                    ).map_err(|_| text("ActionAskPassMissing"))
                };
                let result = (|| {
                    progress(shovel_core::branches::Stage::Validating);
                    let result = if let LocalAction::Recovery(action) = &action {
                        git.execute_recovery(&captured, *action, Some(progress))
                    } else if let LocalAction::Stash(action) = &action {
                        git.execute_stash_with_conflict(&captured, action, Some(progress))
                            .map(|(completed, origin)| {
                                if matches!(action, shovel_core::stashes::Action::Apply { .. }) {
                                    let _ = sender.send(Event::StashConflict(
                                        captured.worktree.clone(),
                                        origin.map(Arc::new),
                                    ));
                                }
                                completed
                            })
                    } else if let LocalAction::Remote(action) = &action {
                        git.execute_remote(&captured, action, Some(progress))
                    } else if let LocalAction::Submodule(action) = &action {
                        let prepared = git
                            .prepare_local(&captured, true, &Cancellation::default())
                            .map_err(|e| error_log(&e))?;
                        let auth = (action.operation == shovel_core::submodules::Operation::Update)
                            .then(start_authentication).transpose()?;
                        git.execute_submodule(&prepared, action, auth.as_ref(), Some(progress))
                    } else if let LocalAction::Repository(action) = &action {
                        let auth = action.needs_authentication().then(start_authentication).transpose()?;
                        git.execute_repository_action(&captured, action, auth.as_ref(), Some(progress))
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
                            | LocalAction::Submodule(_)
                            | LocalAction::Repository(_) => unreachable!(),
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
                let _ = sender.send(Event::completed(sequence, result));
                ctx.request_repaint();
                outcome
            },
            move |outcome| {
                let _ = discarded.send(Event::Removed(sequence, outcome, None));
                discarded_ctx.request_repaint();
            },
        );
        if accepted {
            self.runs.push(Run {
                background: false,
                sequence,
                request,
                path: snapshot.worktree.clone(),
                label,
                status: text("OperationWaiting"),
                output: String::new(),
                done: false,
                dialog_open: false,
                reopen_commit: None,
                show_failure: false,
            });
        }
        self.trim_finished();
        accepted.then_some(request)
    }
    pub fn poll(&mut self, ctx: &egui::Context) -> Vec<PathBuf> {
        while let Ok(event) = self.receiver.try_recv() {
            match event {
                Event::StashConflict(path, origin) => {
                    self.forget_stash_conflict(&path);
                    if let Some(origin) = origin {
                        self.stash_conflicts.push_back(origin);
                        if self.stash_conflicts.len() > 16 {
                            self.stash_conflicts.pop_front();
                        }
                    }
                }
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
                            Stage::UpdatingSubmodule | Stage::RepositoryCommand => run.label,
                            Stage::Resetting => "ActionStageResetting",
                            Stage::Reverting => "ActionStageReverting",
                            Stage::Recovering => "ActionStageRecovering",
                            Stage::Reconciling => "ActionReinspecting",
                        });
                    }
                }
                Event::ExternalPrompt(request, path, label, prompt) => {
                    if prompt.closed.is_cancelled() {
                        continue;
                    }
                    // The worker can reach Git authentication before the editor
                    // has published its first Activity entry to this controller.
                    if !self.runs.iter().any(|run| run.request == request) {
                        self.record_owned(
                            path,
                            crate::commit::OwnedActivity {
                                request,
                                label,
                                status: text("ActionWaitingShort"),
                                output: String::new(),
                                done: false,
                                failed: false,
                                reopen_commit: None,
                            },
                        );
                    }
                    if let Some(run) = self.runs.iter_mut().find(|r| r.request == request) {
                        if run.done {
                            let _ = prompt.answer.send(None);
                            continue;
                        }
                        run.status = text("ActionWaitingShort");
                        self.prompts.push_back(Question {
                            run: run.sequence,
                            prompt,
                            response: Zeroizing::new(String::new()),
                            focus: true,
                        });
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
                        self.refresh_paths.push(run.path.clone());
                        match *result {
                            Ok(result) => {
                                self.refresh_paths.extend(result.related.iter().cloned());
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
                Event::Removed(id, outcome, reason) => {
                    self.pending.retain(|p| p.sequence != id);
                    if let Some(run) = self.runs.iter_mut().find(|r| r.sequence == id && !r.done) {
                        run.done = true;
                        run.show_failure = matches!(outcome, Outcome::Failed | Outcome::WorkerFailed);
                        if outcome == Outcome::WorkerFailed {
                            self.refresh_paths.push(run.path.clone());
                        }
                        if let Some(reason) = reason {
                            run.output = text(reason);
                        }
                        run.status = text(match outcome {
                            Outcome::Removed => "QueuedRequestCancelled",
                            Outcome::Coalesced => "RustRequestCoalesced",
                            Outcome::Failed => "ActionFailedShort",
                            Outcome::WorkerFailed => "ActionWorkerFailed",
                            _ => "ActionStoppedShort",
                        });
                    }
                }
            }
        }
        self.prompts.retain(|q| !q.prompt.closed.is_cancelled());
        self.trim_finished();
        if !self.prompts.is_empty() {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
        std::mem::take(&mut self.refresh_paths)
    }

    fn trim_finished(&mut self) {
        while self.runs.len() > 128 {
            if let Some(index) = self.runs.iter().position(|run| run.done) {
                self.runs.remove(index);
            } else {
                break;
            }
        }
    }

    pub fn stash_conflict(
        &self,
        worktree: &std::path::Path,
    ) -> Option<Arc<shovel_core::stashes::ConflictOrigin>> {
        self.stash_conflicts
            .iter()
            .find(|origin| crate::Workspace::same_path(&origin.worktree, worktree))
            .cloned()
    }

    pub fn forget_stash_conflict(&mut self, worktree: &Path) {
        self.stash_conflicts
            .retain(|origin| !crate::Workspace::same_path(&origin.worktree, worktree));
    }

    pub fn record_owned(&mut self, path: PathBuf, activity: crate::commit::OwnedActivity) {
        if let Some(run) = self.runs.iter_mut().find(|r| r.request == activity.request) {
            run.show_failure |= !run.done && activity.done && activity.failed;
            run.status = activity.status;
            run.output = crate::activity_output::bounded(activity.output);
            run.done = activity.done;
            run.reopen_commit = activity.reopen_commit;
        } else {
            let sequence = self.next;
            self.next += 1;
            self.runs.push(Run {
                background: false,
                sequence,
                request: activity.request,
                path,
                label: activity.label,
                status: activity.status,
                output: crate::activity_output::bounded(activity.output),
                done: activity.done,
                dialog_open: false,
                reopen_commit: activity.reopen_commit,
                show_failure: activity.done && activity.failed,
            });
        }
        self.trim_finished();
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
            background: false,
            sequence,
            request: 0,
            path,
            label: failure.unwrap_or("Commit"),
            status,
            output: crate::activity_output::bounded(network::redact(&output)),
            done: true,
            dialog_open: false,
            reopen_commit: None,
            show_failure: failure.is_some(),
        });
        self.trim_finished();
    }
    pub fn record_background(&mut self, result: crate::auto_fetch::Finished) {
        let sequence = self.next;
        self.next += 1;
        let status = result.status;
        self.runs.push(Run {
            background: true,
            sequence,
            request: 0,
            path: result.path,
            label: "RustAutomaticFetch",
            status: text(status),
            output: crate::activity_output::bounded(network::redact(&result.output)),
            done: true,
            dialog_open: false,
            reopen_commit: None,
            show_failure: false,
        });
        self.trim_finished();
    }

    #[cfg(test)]
    pub(crate) fn background_count(&self) -> usize {
        self.runs.iter().filter(|run| run.background).count()
    }

    pub fn summary(&self, path: &std::path::Path) -> Option<String> {
        let mut runs = self
            .runs
            .iter()
            .rev()
            .filter(|r| !r.background && crate::Workspace::same_path(&r.path, path));
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
    fn activity_runs<'a>(&'a self, path: Option<&'a PathBuf>) -> impl Iterator<Item = &'a Run> {
        self.runs.iter().rev().filter(move |run| {
            path.is_none_or(|path| crate::Workspace::same_path(&run.path, path))
                && match self.filter {
                    ActivityFilter::All => true,
                    ActivityFilter::User => !run.background,
                    ActivityFilter::Background => run.background,
                }
        })
    }

    pub fn activity(
        &self,
        ui: &mut egui::Ui,
        path: Option<&PathBuf>,
        scheduler: &Scheduler,
    ) -> Option<(PathBuf, String)> {
        let key = ui.scope_id().with("selected-run");
        let runs: Vec<_> = self.activity_runs(path).collect();
        let remembered = ui.ctx().data(|data| data.get_temp::<u64>(key));
        let Some(mut selected) = runs
            .iter()
            .copied()
            .find(|run| Some(run.sequence) == remembered)
            .or_else(|| runs.first().copied())
        else {
            ui.label(text("ActivityEmpty"));
            return None;
        };
        let mut reopen = None;
        let width = ui.available_width();
        egui::Panel::left("activity-list")
            .exact_size((width * 0.38).clamp(140., 360.))
            .resizable(false)
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        for run in &runs {
                            let root = run.path.to_string_lossy();
                            let root = PathText::new(ui.ctx(), &root);
                            let name = run
                                .path
                                .file_name()
                                .unwrap_or(run.path.as_os_str())
                                .to_string_lossy();
                            let name = PathText::new(ui.ctx(), &name);
                            let label = text(run.label);
                            let summary = |target| {
                                strings::format("ActivityRunSummary", &[target, &label, &run.status])
                            };
                            let visual = summary(name.visual());
                            let response = ui.add_sized(
                                [ui.available_width(), 24.],
                                egui::Button::selectable(
                                    run.sequence == selected.sequence,
                                    (&visual, egui::Atom::grow()),
                                )
                                .truncate(),
                            );
                            ui.ctx().accesskit_node_builder(response.id, |node| {
                                node.set_value(summary(root.spoken()))
                            });
                            if response
                                .on_hover_text(format!("{visual}\n{}", root.visual()))
                                .clicked()
                            {
                                selected = run;
                                ui.ctx()
                                    .data_mut(|data| data.insert_temp(key, run.sequence));
                            }
                        }
                    });
            });
        egui::CentralPanel::default().show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.strong(text(selected.label));
                if !selected.done
                    && scheduler
                        .active_records()
                        .iter()
                        .any(|record| record.id == selected.request && !record.running)
                    && ui.button(text("CancelWaitingRequest")).clicked()
                {
                    scheduler.remove_waiting(selected.request);
                }
                if let Some(failed) = &selected.reopen_commit {
                    if ui.button(text("ReopenCommit")).clicked() {
                        reopen = Some((selected.path.clone(), failed.owner.clone()));
                    }
                    if ui.button(text("CopySubmittedMessage")).clicked() {
                        ui.ctx().copy_text(failed.message.clone());
                    }
                }
            });
            ui.separator();
            egui::ScrollArea::both()
                .id_salt(selected.sequence)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.label(&selected.status);
                    if !selected.output.is_empty() {
                        ui.add(
                            egui::Label::new(RichText::new(&selected.output).monospace())
                                .selectable(true),
                        );
                    }
                });
        });
        reopen
    }

    pub fn status(
        &self,
        ui: &mut egui::Ui,
        path: Option<&PathBuf>,
        scheduler: &Scheduler,
    ) -> Option<(PathBuf, String)> {
        let mut reopen = None;
        let records = scheduler.active_records();
        for run in self.activity_runs(path) {
            ui.horizontal(|ui| {
                let path = run.path.to_string_lossy();
                let name = run
                    .path
                    .file_name()
                    .unwrap_or(run.path.as_os_str())
                    .to_string_lossy();
                let path = PathText::new(ui.ctx(), &path);
                let name = PathText::new(ui.ctx(), &name);
                let label = text(run.label);
                let summary = |target| {
                    strings::format("ActivityRunSummary", &[target, &label, &run.status])
                };
                let row = ui
                    .add(egui::Label::new(summary(name.visual())).selectable(!name.reordered()))
                    .on_hover_text(path.visual());
                ui.ctx()
                    .accesskit_node_builder(row.id, |node| node.set_value(summary(path.spoken())));
                if !run.done
                    && records
                        .iter()
                        .any(|r| r.id == run.request && !r.running)
                    && ui.button(text("CancelWaitingRequest")).clicked()
                {
                    scheduler.remove_waiting(run.request);
                }
                if let Some(failed) = &run.reopen_commit {
                    if ui.button(text("ReopenCommit")).clicked() {
                        reopen = Some((run.path.clone(), failed.owner.clone()));
                    }
                    if ui.button(text("CopySubmittedMessage")).clicked() {
                        ui.ctx().copy_text(failed.message.clone());
                    }
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
                let path = run.path.to_string_lossy();
                let path = PathText::new(ctx, &path);
                let label = text(run.label);
                let context = |target| strings::format("AuthContext", &[target, &label]);
                path_text::label(
                    ui,
                    context(path.visual()),
                    path.reordered().then(|| context(path.spoken())),
                );
            }
            ui.label(text("AuthPromptSource"));
            egui::ScrollArea::vertical()
                .max_height(220.)
                .show(ui, |ui| {
                    ui.add(egui::Label::new(&question.prompt.message).selectable(true));
                });
            ui.label(text("RustAuthExplanation"));
            if question.prompt.kind == authentication::Kind::Secret {
                let label = ui.label(text("AuthResponse"));
                let field = ui.make_persistent_id("askpass_response");
                if question.focus && ui.is_visible() && !ui.is_sizing_pass() {
                    ui.memory_mut(|memory| memory.request_focus(field));
                    question.focus = false;
                }
                let mut output = egui::TextEdit::singleline(&mut *question.response)
                    .id(field)
                    .password(true)
                    .char_limit(4096)
                    .show(ui);
                output.state.clear_undoer();
                output.state.store(ctx, output.response.id);
                output.response.response.labelled_by(label.id);
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
        if self.active_failure != Some(run.sequence) {
            let inactive = ctx.input(|i| {
                !i.raw.focused
                    && i.raw
                        .viewports
                        .iter()
                        .all(|(id, info)| *id == i.raw.viewport_id || info.focused == Some(false))
            });
            if inactive {
                ctx.send_viewport_cmd_to(
                    egui::ViewportId::ROOT,
                    egui::ViewportCommand::RequestUserAttention(
                        egui::UserAttentionType::Informational,
                    ),
                );
            }
            self.active_failure = Some(run.sequence);
        }
        let mut open = true;
        let mut dismiss = false;
        let title = text("OperationAttentionTitle");
        let window = egui::Window::new(&title)
            .id(egui::Id::unique("operation-failure"))
            .collapsible(false)
            .order(egui::Order::Foreground)
            .default_width(580.);
        crate::workspace_style::show_closable_window(ctx, window, &title, &mut open, |ui| {
                ui.heading(text(run.label));
                let path = run.path.to_string_lossy();
                let display = PathText::new(ctx, &path);
                if display.reordered() {
                    path_text::label(ui, display.visual().into(), Some(display.spoken().into()));
                    if ui.button(text("FileMenuCopyFull")).clicked() {
                        ctx.copy_text(path.into_owned());
                    }
                } else {
                    ui.add(egui::Label::new(display.visual()).selectable(true));
                }
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

pub(crate) fn authentication_error(failure: authentication::Failure) -> &'static str {
    match failure {
        authentication::Failure::Unavailable => "RustAuthUnavailable",
        authentication::Failure::TimedOut => "RustAuthTimedOut",
        authentication::Failure::PromptLimit => "RustAuthPromptLimit",
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
        for label in [text("NetworkRebase"), text("CancelOperation")] {
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
        assert!(!Options::confirmed(
            &settings,
            &snapshot.worktree,
            Kind::Pull
        ));
        assert_eq!(
            Options::load(&settings, &snapshot, Kind::Pull).strategy,
            Strategy::Merge
        );
        assert!(!scheduler.has_accepted_work());
        assert!(controller.runs.is_empty());
    }
    #[test]
    fn network_form_aligns_fields_filters_branch_choices_and_activates_only_focused_controls() {
        let (_temp, git, mut snapshot) = fixture();
        snapshot.remotes.extend(["team".into(), "team/docs".into()]);
        for name in [
            "origin/review",
            "team/main",
            "team/docs/handbook",
            "origin/HEAD",
        ] {
            snapshot.references.push(shovel_core::git::Reference {
                name: format!("refs/remotes/{name}"),
                hash: snapshot.head.clone().unwrap(),
                upstream: None,
            });
        }
        assert_eq!(cached_remote_branches(&snapshot, "team"), ["main"]);
        assert_eq!(cached_remote_branches(&snapshot, "team/docs"), ["handbook"]);
        let mut settings = Settings::default();
        let mut controller = Controller::default();
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let scheduler = Scheduler::default();
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800., 600.));
        let frame = |controller: &mut Controller, settings: &mut Settings, events| {
            ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(screen),
                    events,
                    ..Default::default()
                },
                |_| {
                    controller.options_dialog(&ctx, settings, Some(&git), &scheduler);
                },
            )
        };
        let open = |controller: &mut Controller, settings: &Settings, kind| {
            controller.open_network_options(
                kind,
                &snapshot.worktree,
                Some(&snapshot),
                settings,
                &scheduler,
                &ctx,
            );
        };
        let tap = |controller: &mut Controller, settings: &mut Settings, key| {
            for pressed in [true, false] {
                frame(
                    controller,
                    settings,
                    vec![egui::Event::Key {
                        key,
                        physical_key: Some(key),
                        pressed,
                        repeat: false,
                        modifiers: egui::Modifiers::NONE,
                    }],
                )
                .drop_without_applying_deltas();
            }
        };
        let click = |controller: &mut Controller, settings: &mut Settings, point| {
            frame(controller, settings, vec![egui::Event::PointerMoved(point)])
                .drop_without_applying_deltas();
            for pressed in [true, false] {
                frame(
                    controller,
                    settings,
                    vec![egui::Event::PointerButton {
                        pos: point,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    }],
                )
                .drop_without_applying_deltas();
            }
        };
        let rect = |output: &egui::FullOutput, key, by_value: bool| {
            let label = text(key);
            let bounds = output
                .platform_output
                .accesskit_update
                .as_ref()
                .unwrap()
                .nodes
                .iter()
                .find_map(|(_, node)| {
                    let matched =
                        node.value() == Some(&label) || !by_value && node.label() == Some(&label);
                    matched.then(|| node.bounds()).flatten()
                })
                .unwrap_or_else(|| panic!("Missing {label}"));
            egui::Rect::from_min_max(
                egui::pos2(bounds.x0 as f32, bounds.y0 as f32),
                egui::pos2(bounds.x1 as f32, bounds.y1 as f32),
            )
        };
        open(&mut controller, &settings, Kind::Pull);
        frame(&mut controller, &mut settings, vec![]).drop_without_applying_deltas();
        let output = frame(&mut controller, &mut settings, vec![]);
        let remote = rect(&output, "origin", true);
        let branch = rect(&output, "origin/main", true);
        let into = rect(&output, "NetworkInto", false);
        let rebase = rect(&output, "NetworkRebase", false);
        let stash = rect(&output, "ActionAutoStash", false);
        let primary = rect(&output, "Pull", false);
        let cancel = rect(&output, "CancelOperation", false);
        assert!(remote.top() < branch.top() && branch.bottom() <= into.top());
        assert!(into.bottom() <= rebase.top() && rebase.bottom() <= stash.top());
        assert!((remote.left() - branch.left()).abs() < 1.);
        assert!(primary.right() <= cancel.left() && primary.left() > screen.center().x);
        let tree = output.platform_output.accesskit_update.as_ref().unwrap();
        assert!(
            tree.nodes
                .iter()
                .any(|(id, node)| *id == tree.focus && node.label() == Some("Pull"))
        );
        output.drop_without_applying_deltas();

        click(&mut controller, &mut settings, rebase.center());
        assert_eq!(
            controller.options.as_ref().unwrap().0.strategy,
            Strategy::Merge
        );
        tap(&mut controller, &mut settings, egui::Key::Enter);
        assert_eq!(
            controller.options.as_ref().unwrap().0.strategy,
            Strategy::Rebase
        );
        assert!(controller.runs.is_empty());

        click(&mut controller, &mut settings, branch.center());
        let output = frame(&mut controller, &mut settings, vec![]);
        assert!(rect(&output, "review", false).is_positive());
        let tree = output.platform_output.accesskit_update.as_ref().unwrap();
        assert!(
            !tree
                .nodes
                .iter()
                .any(|(_, node)| node.label() == Some("handbook") || node.label() == Some("HEAD"))
        );
        output.drop_without_applying_deltas();
        tap(&mut controller, &mut settings, egui::Key::Escape);
        assert!(controller.modal_open() && !egui::Popup::is_any_open(&ctx));
        tap(&mut controller, &mut settings, egui::Key::Escape);
        assert!(!controller.modal_open() && controller.runs.is_empty());

        // Submission uses a live, disposable snapshot with one local bare remote.
        command(&git, &snapshot.worktree, &["push", "origin", "main"]);
        let snapshot = git
            .inspect(&snapshot.worktree, &Cancellation::default())
            .unwrap();
        controller.open_network_options(
            Kind::Fetch,
            &snapshot.worktree,
            Some(&snapshot),
            &settings,
            &scheduler,
            &ctx,
        );
        frame(&mut controller, &mut settings, vec![]).drop_without_applying_deltas();
        tap(&mut controller, &mut settings, egui::Key::Enter);
        assert!(!controller.modal_open());
        assert!(Options::confirmed(
            &settings,
            &snapshot.worktree,
            Kind::Fetch
        ));
        wait(|| {
            controller.poll(&ctx);
            controller.runs.last().is_some_and(|run| run.done)
        });
        assert_eq!(
            controller.runs[0].status,
            text("ActionCompletedShort"),
            "{}",
            controller.runs[0].output
        );
    }

    #[test]
    fn repository_options_keep_their_owner_during_tab_switch_and_close() {
        use crate::workspace_interaction_tests::{frame, workspace};

        let (_first, git, owner) = fixture();
        let (_second, _, other) = fixture();
        let ctx = egui::Context::default();
        let mut app = workspace();
        app.tabs.clear();
        app.git = Some(git);
        app.open(owner.worktree.clone(), &ctx);
        crate::history_view::tests::wait(&mut app, &ctx);
        let owner_id = app.tabs[0].id;
        app.ensure_tab(other.worktree.clone());
        app.operations.options = Some((
            Options::load(&app.settings, &owner, Kind::Pull),
            owner.clone(),
        ));
        frame(&ctx, &mut app, vec![]).drop_without_applying_deltas();

        let press = |app: &mut crate::Workspace, key| {
            let modifiers = egui::Modifiers {
                ctrl: true,
                ..Default::default()
            };
            for pressed in [true, false] {
                frame(
                    &ctx,
                    app,
                    vec![
                        egui::Event::ModifiersChanged(modifiers),
                        egui::Event::Key {
                            key,
                            physical_key: None,
                            pressed,
                            repeat: false,
                            modifiers,
                        },
                    ],
                )
                .drop_without_applying_deltas();
            }
        };
        press(&mut app, egui::Key::Tab);
        assert_eq!(app.tabs[app.selected].path, other.worktree);
        press(&mut app, egui::Key::Tab);
        assert_eq!(app.tabs[app.selected].id, owner_id);
        press(&mut app, egui::Key::W);
        assert_eq!(app.tabs[app.selected].path, other.worktree);
        assert!(app.retained_tabs.iter().any(|tab| tab.id == owner_id));

        let (options, captured) = app.operations.options.as_ref().unwrap();
        assert_eq!(options.kind, Kind::Pull);
        assert_eq!(captured.identity, owner.identity);
        assert_eq!(captured.worktree, owner.worktree);
        assert!(app.operations.runs.is_empty());
        crate::history_view::tests::wait(&mut app, &ctx);
    }

    #[test]
    fn remembered_flags_persist_and_explicit_targets_are_branch_bound() {
        let (temporary, _, snapshot) = fixture();
        let mut settings = Settings::default();
        let mut options = Options::load(&settings, &snapshot, Kind::Pull);
        options.auto_stash = true;
        options.strategy = Strategy::Merge;
        options.save(&mut settings, &snapshot, true);
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
        options.save(&mut settings, &snapshot, false);
        let directory = temporary.path().join("profile");
        let mut store = shovel_core::settings::SettingsStore::new(directory.clone());
        store.load().unwrap();
        store.save(&settings).unwrap();
        let restored = shovel_core::settings::SettingsStore::new(directory)
            .load()
            .unwrap();
        assert_eq!(Options::load(&restored, &snapshot, Kind::Pull), options);
        assert!(Options::confirmed(
            &restored,
            &snapshot.worktree,
            Kind::Pull
        ));
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
        fetch.save(&mut settings, &snapshot, false);
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
        Options::load(&settings, &snapshot, Kind::Push).save(&mut settings, &snapshot, true);
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
                        &settings,
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
                            &settings,
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
            text("RustActionWarning"),
            "{}",
            controller.runs[0].output
        );
        assert!(controller.runs[0].output.contains("fixture push succeeded"));
        assert!(controller.runs[0].output.contains("git push · exit 0"));
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
            "#!/bin/sh\necho 'fixture push rejected' >&2\nprintf '%045000d' 0 >&2\nexit 1\n",
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
        assert!(controller.runs[1].output.contains("git push · exit 1"));
        assert!(
            controller.runs[1]
                .output
                .ends_with(&text("ActivityOutputLimit"))
        );
        assert!(controller.runs[1].output.len() <= 32768);
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
    fn a_failure_requests_taskbar_attention_once_only_if_every_window_is_inactive() {
        let draw = |controller: &mut Controller, current: bool, child: Option<bool>| {
            let ctx = egui::Context::default();
            let mut raw = egui::RawInput {
                focused: current,
                ..Default::default()
            };
            raw.viewports
                .get_mut(&egui::ViewportId::ROOT)
                .unwrap()
                .focused = Some(current);
            let id = egui::ViewportId::from_hash_of("commit-editor");
            raw.viewports.insert(
                id,
                egui::ViewportInfo {
                    focused: child,
                    ..Default::default()
                },
            );
            let output = ctx.run_ui(raw, |_| controller.show_failure(&ctx));
            let commands = &output.viewport_output[&egui::ViewportId::ROOT].commands;
            assert!(
                !commands
                    .iter()
                    .any(|cmd| matches!(cmd, egui::ViewportCommand::Focus))
            );
            let requests = commands
                .iter()
                .filter(|cmd| {
                    matches!(
                        cmd,
                        egui::ViewportCommand::RequestUserAttention(
                            egui::UserAttentionType::Informational
                        )
                    )
                })
                .count();
            output.drop_without_applying_deltas();
            requests
        };
        for (root, child, expected) in [
            (false, Some(false), 1),
            (true, Some(false), 0),
            (false, Some(true), 0),
            (false, None, 0),
        ] {
            let mut controller = Controller::default();
            controller.record_local(
                "repo".into(),
                "ok".into(),
                "remote: warning: retained".into(),
                None,
            );
            assert_eq!(draw(&mut controller, false, Some(false)), 0);
            controller.record_local(
                "repo".into(),
                "failed".into(),
                "original output".into(),
                Some("Push"),
            );
            assert_eq!(draw(&mut controller, root, child), expected);
            assert_eq!(draw(&mut controller, false, Some(false)), 0);
        }
        assert!(crate::activity_output::has_git_warning(
            "To origin
  REMOTE: Warning: message"
        ));
        assert!(!crate::activity_output::has_git_warning(
            "commit subject contains warning: inside"
        ));
    }

    #[test]
    fn completed_activity_eviction_preserves_every_refresh_and_every_active_request() {
        let mut controller = Controller::default();
        let ctx = egui::Context::default();
        for id in 1..=132 {
            controller.record_owned(
                PathBuf::from(format!("repo-{id}")),
                crate::commit::OwnedActivity {
                    request: id,
                    label: "Push",
                    status: "running".into(),
                    output: String::new(),
                    done: false,
                    failed: false,
                    reopen_commit: None,
                },
            );
        }
        assert_eq!(controller.runs.len(), 132);
        for id in 1..=6 {
            controller
                .sender
                .send(Event::Completed(id, Box::new(Err("fixture".into()))))
                .unwrap();
        }
        let changed = controller.poll(&ctx);
        assert_eq!(
            changed,
            (1..=6)
                .map(|id| PathBuf::from(format!("repo-{id}")))
                .collect::<Vec<_>>()
        );
        assert!(controller.poll(&ctx).is_empty());
        assert_eq!(controller.runs.len(), 128);
        assert_eq!(controller.runs.iter().filter(|r| !r.done).count(), 126);
        for _ in 0..150 {
            controller.record_local("newest".into(), "done".into(), String::new(), None);
        }
        assert_eq!(controller.runs.len(), 128);
        assert_eq!(controller.runs.iter().filter(|r| !r.done).count(), 126);
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
        let mut frame = |events| {
            ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(900., 620.),
                    )),
                    events,
                    ..Default::default()
                },
                |_ui| controller.show_prompt(&ctx),
            )
        };
        frame(vec![]).drop_without_applying_deltas();
        let output = frame(vec![egui::Event::Text("x".into())]);
        let update = output.platform_output.accesskit_update.as_ref().unwrap();
        assert!(!format!("{update:?}").contains("fixture-secret"));
        let (input, node) = update.nodes.iter().find(|(_, node)| {
            node.role() == egui::Role::PasswordInput
        }).expect("password input");
        assert_eq!(update.focus, *input);
        assert!(update.nodes.iter().any(|(id, label)| {
            node.labelled_by().contains(id) && label.value() == Some(text("AuthResponse").as_str())
        }));
        assert_eq!(controller.prompts.front().unwrap().response.len(), "fixture-secret".len() + 1);
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
    fn activity_and_failure_show_bidi_paths_and_copy_the_exact_target() {
        use crate::workspace_interaction_tests::sidebar_point;
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let mut app = workspace();
        app.activity_open = true;
        app.tabs.clear();
        let root = r"C:\Git\לקוח\שלום";
        let second = r"C:\Git\צוות\שלום";
        for path in [root, second] {
            app.operations
                .record_local(path.into(), "fixture".into(), String::new(), None);
        }
        frame(&ctx, &mut app, vec![]).drop_without_applying_deltas();
        let out = frame(&ctx, &mut app, vec![]);
        let painted = |label: &str| {
            out.shapes.iter().any(|shape| {
                matches!(&shape.shape, egui::Shape::Text(t) if t.galley.text() == label)
            })
        };
        assert!(painted("םולש · Commit · fixture"));
        let nodes = &out.platform_output.accesskit_update.as_ref().unwrap().nodes;
        for path in [root, second] {
            let expected = format!("\u{2066}{path}\u{2069} · Commit · fixture");
            assert!(
                nodes.iter().any(|(_, node)| node.value() == Some(expected.as_str()))
            );
        }
        out.drop_without_applying_deltas();

        app.operations.record_local(
            root.into(),
            text("ActionFailedShort"),
            String::new(),
            Some("Commit"),
        );
        frame(&ctx, &mut app, vec![]).drop_without_applying_deltas();
        let (pos, disabled) = sidebar_point(&ctx, &mut app, &text("FileMenuCopyFull")).unwrap();
        assert!(!disabled);
        frame(&ctx, &mut app, vec![egui::Event::PointerMoved(pos)]).drop_without_applying_deltas();
        let click = |pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        frame(&ctx, &mut app, vec![click(true)]).drop_without_applying_deltas();
        let out = frame(&ctx, &mut app, vec![click(false)]);
        assert!(out.platform_output.commands.iter().any(|cmd| {
            matches!(cmd, egui::OutputCommand::CopyText(path) if path == root)
        }));
        out.drop_without_applying_deltas();
    }

    #[test]
    fn activity_filters_user_background_and_repository_independently() {
        use crate::workspace_interaction_tests::{frame, workspace};
        let ctx = egui::Context::default();
        let mut app = workspace();
        app.activity_open = true;
        for (path, message) in [("Alpha", "alpha run"), ("Bravo", "bravo run")] {
            app.operations
                .record_local(path.into(), message.into(), String::new(), None);
        }
        for path in ["Alpha", "brAVo"] {
            app.operations
                .record_background(crate::auto_fetch::Finished {
                    path: path.into(),
                    succeeded: false,
                    status: "RustAutoFetchFailed",
                    output: "fixture fetch https://user:private@work.example/repo".into(),
                });
        }
        assert_eq!(app.operations.background_count(), 2);
        assert!(app.operations.failure_path().is_none());
        assert!(
            app.operations
                .summary(Path::new("Bravo"))
                .unwrap()
                .contains("bravo run")
        );
        assert!(
            app.operations
                .runs
                .last()
                .unwrap()
                .output
                .contains("work.example")
        );
        assert!(
            !app.operations
                .runs
                .last()
                .unwrap()
                .output
                .contains("private")
        );
        let rows = |app: &mut crate::Workspace| {
            frame(&ctx, app, vec![]).drop_without_applying_deltas();
            let frame = frame(&ctx, app, vec![]);
            let drawn = frame
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) => Some(text.galley.text().to_owned()),
                    _ => None,
                })
                .collect::<Vec<_>>();
            frame.drop_without_applying_deltas();
            drawn
        };
        let click = |app: &mut crate::Workspace, label: &str| {
            let output = frame(&ctx, app, vec![]);
            let point = output
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Text(tag) if tag.galley.text() == label => {
                        Some(tag.visual_bounding_rect().center())
                    }
                    _ => None,
                })
                .unwrap_or_else(|| panic!("control {label}"));
            output.drop_without_applying_deltas();
            for pressed in [true, false] {
                frame(
                    &ctx,
                    app,
                    vec![
                        egui::Event::PointerMoved(point),
                        egui::Event::PointerButton {
                            pos: point,
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers: egui::Modifiers::NONE,
                        },
                    ],
                )
                .drop_without_applying_deltas();
            }
        };
        let visible =
            |rows: &[String], prefix: &str| rows.iter().any(|line| line.starts_with(prefix));
        let selected = rows(&mut app);
        assert!(visible(&selected, "Bravo · Commit · bravo run"));
        assert!(visible(&selected, "brAVo · Automatic fetch"));
        assert!(!visible(&selected, "Alpha · Automatic fetch"));
        click(&mut app, &text("ActivityBackground"));
        let background = rows(&mut app);
        assert!(visible(&background, "brAVo · Automatic fetch"));
        assert!(!visible(&background, "Bravo · Commit · bravo run"));
        click(&mut app, &text("ActivityAllRepositories"));
        let all_repos = rows(&mut app);
        assert!(visible(&all_repos, "Alpha · Automatic fetch"));
        click(&mut app, &text("ActivityUser"));
        let user = rows(&mut app);
        assert!(visible(&user, "Alpha · Commit · alpha run"));
        assert!(visible(&user, "Bravo · Commit · bravo run"));
        assert!(!visible(&user, "Alpha · Automatic fetch"));
        for _ in 0..130 {
            app.operations
                .record_background(crate::auto_fetch::Finished {
                    path: "Alpha".into(),
                    succeeded: true,
                    status: "RustAutoFetchDone",
                    output: String::new(),
                });
        }
        assert_eq!(app.operations.runs.len(), 128);
        assert_eq!(app.operations.background_count(), 128);
    }

    #[test]
    fn activity_opens_below_header_then_moves_resizes_and_closes() {
        use crate::workspace_interaction_tests::{frame, sidebar_click, sidebar_point, workspace};
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let mut app = workspace();
        app.operations.record_local(
            "Bravo".into(),
            "Warning from the selected run".into(),
            "Full warning output".into(),
            None,
        );
        let render = |app: &mut crate::Workspace, events| {
            frame(&ctx, app, events).drop_without_applying_deltas();
        };
        render(&mut app, vec![]);
        let (header, _) = sidebar_point(&ctx, &mut app, &text("ActivityTitle")).unwrap();
        sidebar_click(
            &ctx,
            &mut app,
            &text("ActivityTitle"),
            egui::PointerButton::Primary,
        );
        render(&mut app, vec![]);
        let area = || {
            ctx.memory(|memory| memory.area_rect(egui::Id::unique("workspace-activity")))
                .unwrap()
        };
        let original = area();
        assert!(original.top() > header.y && original.x_range().contains(header.x));
        let frame_with_output = frame(&ctx, &mut app, vec![]);
        assert!(
            frame_with_output
                .shapes
                .iter()
                .any(|shape| matches!(&shape.shape,
            egui::Shape::Text(label) if label.galley.text().contains("Full warning output")))
        );
        frame_with_output.drop_without_applying_deltas();
        let drag = |app: &mut crate::Workspace, from: egui::Pos2, to: egui::Pos2| {
            let pointer = |pos, pressed| egui::Event::PointerButton {
                pos,
                pressed,
                button: egui::PointerButton::Primary,
                modifiers: egui::Modifiers::NONE,
            };
            render(app, vec![egui::Event::PointerMoved(from)]);
            render(app, vec![pointer(from, true)]);
            render(app, vec![egui::Event::PointerMoved(to)]);
            render(app, vec![pointer(to, false)]);
            render(app, vec![]);
        };
        let title = original.center_top() + egui::vec2(0., 14.);
        drag(&mut app, title, title + egui::vec2(18., 28.));
        let moved = area();
        assert!(
            moved.top() > original.top() + 15.,
            "{original:?} -> {moved:?}"
        );
        let grip = moved.right_bottom() - egui::vec2(2., 2.);
        drag(&mut app, grip, grip + egui::vec2(30., 45.));
        let resized = area();
        assert!(
            resized.height() > moved.height() + 16.,
            "{moved:?} -> {resized:?}"
        );
        sidebar_click(&ctx, &mut app, "Close window", egui::PointerButton::Primary);
        assert!(!app.activity_open);
    }

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
        app.tabs[app.selected].loading = true;
        assert_eq!(before, tab_y(&mut app));
        app.tabs[app.selected].loading = false;
        for seq in 1..=8 {
            app.operations.runs.push(Run {
                background: false,
                sequence: seq,
                request: seq,
                path: "Bravo".into(),
                label: "Fetch",
                status: "complete".into(),
                output: "command output\n".repeat(20),
                done: true,
                dialog_open: false,
                reopen_commit: None,
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
