use crate::strings::{self, text};
use eframe::egui::{self, RichText};
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
    path::PathBuf,
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
enum Event {
    Stage(u64, network::Stage),
    BranchStage(u64, shovel_core::branches::Stage),
    Prompt(u64, authentication::Prompt),
    Completed(u64, Box<Result<network::Completed, String>>),
    Removed(u64, Outcome),
}
struct Run {
    sequence: u64,
    request: u64,
    path: PathBuf,
    label: &'static str,
    status: String,
    output: String,
    done: bool,
    changed: bool,
}
struct Question {
    run: u64,
    prompt: authentication::Prompt,
    response: Zeroizing<String>,
    focus: bool,
}
pub struct Controller {
    next: u64,
    runs: Vec<Run>,
    prompts: VecDeque<Question>,
    sender: Sender<Event>,
    receiver: Receiver<Event>,
    helper: Option<PathBuf>,
}
impl Default for Controller {
    fn default() -> Self {
        let (sender, receiver) = mpsc::channel();
        Self {
            next: 1,
            runs: vec![],
            prompts: VecDeque::new(),
            sender,
            receiver,
            helper: None,
        }
    }
}
impl Controller {
    pub fn modal_open(&self) -> bool {
        !self.prompts.is_empty()
    }
    pub fn toolbar(
        &mut self,
        ui: &mut egui::Ui,
        settings: &mut Settings,
        snapshot: Option<&Snapshot>,
        git: Option<&Arc<Git>>,
        scheduler: &Scheduler,
    ) -> bool {
        let mut changed = false;
        for kind in [Kind::Fetch, Kind::Pull, Kind::Push] {
            ui.push_id(kind.number(), |ui| {
                let available = snapshot.is_some() && git.is_some();
                ui.horizontal(|ui| {
                    let button = ui.add_enabled(available, egui::Button::new(text(kind.key())));
                    if !available {
                        button
                            .clone()
                            .on_disabled_hover_text(text("ActionInspectionNeeded"));
                    }
                    if button.clicked()
                        && let (Some(snapshot), Some(git)) = (snapshot, git)
                    {
                        self.submit(
                            Options::load(settings, snapshot, kind),
                            snapshot,
                            git.clone(),
                            scheduler,
                            ui.ctx(),
                        );
                    }
                    ui.add_enabled_ui(available, |ui| {
                        ui.menu_button("▾", |ui| {
                            let Some(snapshot) = snapshot else {
                                return;
                            };
                            let mut options = Options::load(settings, snapshot, kind);
                            let before = options.clone();
                            ui.set_max_width(480.);
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
                                            if let Some(branch) =
                                                reference.name.strip_prefix("refs/heads/")
                                            {
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
                                        ui.selectable_value(
                                            &mut options.remote,
                                            remote.clone(),
                                            remote,
                                        );
                                    }
                                });
                            options.explicit |= old != options.remote;
                            if kind != Kind::Fetch {
                                ui.label(text("ActionRemoteBranch"));
                                options.explicit |= ui
                                    .add(
                                        egui::TextEdit::singleline(&mut options.branch)
                                            .char_limit(1024),
                                    )
                                    .changed();
                            }
                            if options.explicit
                                && ui.button(text("RustNetworkUseConfigured")).clicked()
                            {
                                options.explicit = false;
                            }
                            if options != before {
                                options.save(settings, snapshot);
                                changed = true;
                            }
                            if ui.button(text(kind.key())).clicked() {
                                if let Some(git) = git {
                                    self.submit(
                                        options,
                                        snapshot,
                                        git.clone(),
                                        scheduler,
                                        ui.ctx(),
                                    );
                                }
                                ui.close();
                            }
                        })
                        .response
                        .on_hover_text(strings::format("RustNetworkOptions", &[&text(kind.key())]));
                    });
                });
            });
        }
        changed
    }
    fn submit(
        &mut self,
        options: Options,
        snapshot: &Snapshot,
        git: Arc<Git>,
        scheduler: &Scheduler,
        ctx: &egui::Context,
    ) {
        let sequence = self.next;
        self.next += 1;
        let kind = options.kind;
        let action = options.action();
        let captured = snapshot.clone();
        let helper = self.helper.clone();
        let sender = self.sender.clone();
        let discarded = sender.clone();
        let ctx = ctx.clone();
        let discarded_ctx = ctx.clone();
        let key = format!("network:{action:?}");
        let (request, accepted) = scheduler.submit(
            snapshot.identity.fingerprint.clone(),
            snapshot.common_directory.to_string_lossy().to_uppercase(),
            key,
            if kind == Kind::Fetch {
                Lane::Fetch
            } else {
                Lane::Exclusive
            },
            move || {
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
                            .with_file_name("ShovelRustAskPass.exe"),
                    };
                    let auth = authentication::Session::start(&helper, handler)
                        .map_err(|_| text("ActionAskPassMissing"))?;
                    let review = git
                        .prepare_network(&captured.worktree, &Cancellation::default())
                        .map_err(|e| text(&e.code()))?;
                    if review.repository.identity != captured.identity
                        || review.repository.branch != captured.branch
                        || review.repository.head != captured.head
                    {
                        return Err(text("ActionChanged"));
                    }
                    git.execute_network(&review, &action, Some(&auth), Some(progress))
                        .map_err(|e| text(&e.code()))
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
                label: kind.key(),
                status: text("OperationWaiting"),
                output: String::new(),
                done: false,
                changed: false,
            });
            while self.runs.len() > 128 {
                if let Some(index) = self.runs.iter().position(|r| r.done) {
                    self.runs.remove(index);
                } else {
                    break;
                }
            }
        }
    }
    pub fn submit_branch(
        &mut self,
        label: &'static str,
        action: shovel_core::branches::Action,
        snapshot: &Snapshot,
        git: Arc<Git>,
        scheduler: &Scheduler,
        ctx: &egui::Context,
    ) {
        let sequence = self.next;
        self.next += 1;
        let captured = snapshot.clone();
        let sender = self.sender.clone();
        let discarded = sender.clone();
        let ctx = ctx.clone();
        let discarded_ctx = ctx.clone();
        // Do not format the working-copy review or repository data into a key.
        let key = match &action {
            shovel_core::branches::Action::Create { name, start } => {
                format!("create:{name}:{}", start.hash)
            }
            shovel_core::branches::Action::Checkout {
                target, changes, ..
            } => format!("checkout:{target:?}:{changes:?}"),
            shovel_core::branches::Action::Manage { branch, action } => {
                format!("manage:{}:{action:?}", branch.name)
            }
        };
        let (request, accepted) = scheduler.submit(
            captured.identity.fingerprint.clone(),
            captured.common_directory.to_string_lossy().to_uppercase(),
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
                    let unchanged_refs =
                        !matches!(action, shovel_core::branches::Action::Checkout { .. });
                    let prepared = git
                        .prepare_local(&captured, unchanged_refs, &Cancellation::default())
                        .map_err(|e| text(&e.code()))?;
                    git.execute_branch(&prepared, &action, Some(progress))
                        .map_err(|e| text(&e.code()))
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
                changed: false,
            });
        }
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
                            Stage::Reconciling => "ActionReinspecting",
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
                        run.changed = true;
                        match *result {
                            Ok(result) => {
                                run.status =
                                    text(if result.succeeded() && result.warning.is_some() {
                                        "RustActionWarning"
                                    } else if result.succeeded() {
                                        "ActionCompletedShort"
                                    } else if result.has_conflicts {
                                        "ActionConflictShort"
                                    } else if result.updated.is_err()
                                        && result.message.is_none()
                                        && result.commands.iter().all(network::Command::succeeded)
                                    {
                                        "ActionRefreshFailedShort"
                                    } else {
                                        "ActionFailedShort"
                                    });
                                run.output = result.output;
                                for message in [
                                    result.message,
                                    result.warning,
                                    result.updated.err().map(|e| e.code()),
                                ]
                                .into_iter()
                                .flatten()
                                {
                                    run.output.push_str(&format!("\n{}", text(&message)));
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
                                run.status = text("ActionFailedShort");
                                run.output = error;
                            }
                        }
                    }
                }
                Event::Removed(id, outcome) => {
                    if let Some(run) = self.runs.iter_mut().find(|r| r.sequence == id) {
                        run.done = true;
                        run.status = text(if outcome == Outcome::Removed {
                            "QueuedRequestCancelled"
                        } else {
                            "ActionStoppedShort"
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
        self.runs
            .iter_mut()
            .filter_map(|r| std::mem::take(&mut r.changed).then(|| r.path.clone()))
            .collect()
    }
    pub fn status(&self, ui: &mut egui::Ui, path: &PathBuf, scheduler: &Scheduler) {
        let records = scheduler.records();
        for run in self.runs.iter().filter(|r| &r.path == path).rev().take(8) {
            ui.horizontal(|ui| {
                ui.label(format!("{} · {}", text(run.label), run.status));
                if !run.done
                    && records
                        .iter()
                        .any(|r| r.id == run.request && !r.running && r.outcome.is_none())
                    && ui.button(text("CancelWaitingRequest")).clicked()
                {
                    scheduler.remove_waiting(run.request);
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use shovel_core::process::{self, Lifetime};
    use std::{fs, path::Path, thread, time::Instant};
    fn command(git: &Git, path: &Path, args: &[&str]) -> String {
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
    fn fixture() -> (tempfile::TempDir, Arc<Git>, Snapshot) {
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
    fn wait(mut condition: impl FnMut() -> bool) {
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
        let helper = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/debug/ShovelRustAskPass.exe");
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
        fs::write(path.join(".git/hooks/pre-push"), "#!/bin/sh\necho started > .git/entered\nwhile [ ! -f .git/release ]; do sleep 0.05; done\n").unwrap();
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
                    controller.toolbar(ui, &mut settings, Some(&snapshot), Some(&git), &scheduler);
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
        assert_eq!(
            command(&git, &path.join("remote.git"), &["rev-parse", "main"]),
            snapshot.head.unwrap()
        );
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
