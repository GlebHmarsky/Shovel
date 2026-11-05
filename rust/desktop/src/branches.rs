use crate::{
    operations,
    strings::{self, text},
};
use eframe::egui::{self, Color32};
use shovel_core::{
    branches::{Action, Changes, Checkout, Manage},
    git::{Git, Reference, Snapshot},
    process::Cancellation,
    scheduler::Scheduler,
    working,
};
use std::{
    collections::BTreeSet,
    sync::{
        Arc,
        mpsc::{self, Receiver},
    },
    thread,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Management {
    SetUpstream,
    UnsetUpstream,
}
impl Management {
    fn title(self) -> &'static str {
        match self {
            Self::SetUpstream => "BranchManageTitleSetUpstream",
            Self::UnsetUpstream => "BranchManageTitleUnsetUpstream",
        }
    }
    fn explanation(self) -> &'static str {
        match self {
            Self::SetUpstream => "BranchManageExplainSetUpstream",
            Self::UnsetUpstream => "BranchManageExplainUnsetUpstream",
        }
    }
    fn submit(self) -> &'static str {
        match self {
            Self::SetUpstream => "BranchManageSubmitSetUpstream",
            Self::UnsetUpstream => "BranchManageSubmitUnsetUpstream",
        }
    }
}
struct Review {
    target: Checkout,
    changes: Changes,
    context: Option<Arc<working::Context>>,
    receiver: Option<Receiver<Result<working::Context, String>>>,
    cancel: Cancellation,
    started: bool,
    status: String,
}
impl Review {
    fn new(target: Checkout) -> Self {
        Self {
            target,
            changes: Changes::Keep,
            context: None,
            receiver: None,
            cancel: Cancellation::default(),
            started: false,
            status: text("CheckoutReading"),
        }
    }
    fn title(&self) -> &'static str {
        if matches!(self.target, Checkout::Detached(_)) {
            "HistoryMenuCheckoutCommit"
        } else {
            "ActionTitleSwitchBranch"
        }
    }
    fn target_text(&self) -> String {
        match &self.target {
            Checkout::Branch(reference) => format!(
                "{} · {}",
                reference
                    .name
                    .strip_prefix("refs/heads/")
                    .unwrap_or(&reference.name),
                reference.hash
            ),
            Checkout::New { name, start } => format!("{name} · {} · {}", start.name, start.hash),
            Checkout::Detached(hash) => hash.clone(),
        }
    }
    fn start(&mut self, git: Arc<Git>, snapshot: &Snapshot, ctx: &egui::Context) {
        self.started = true;
        let (sender, receiver) = mpsc::channel();
        self.receiver = Some(receiver);
        let expected = snapshot.clone();
        let cancel = self.cancel.clone();
        let ctx = ctx.clone();
        thread::spawn(move || {
            let result = git
                .working_copy(&expected.worktree, &cancel)
                .map_err(|_| text("CheckoutReadFailed"))
                .and_then(|context| {
                    if context.repository.identity != expected.identity
                        || context.repository.branch != expected.branch
                        || context.repository.head != expected.head
                    {
                        Err(text("CheckoutChanged"))
                    } else {
                        Ok(context)
                    }
                });
            let _ = sender.send(result);
            ctx.request_repaint();
        });
    }
    fn poll(&mut self) {
        if let Some(receiver) = &self.receiver
            && let Ok(result) = receiver.try_recv()
        {
            self.receiver = None;
            match result {
                Ok(context) => {
                    self.status = text(if context.entries.is_empty() {
                        "CheckoutClean"
                    } else {
                        "CheckoutDirtyHelp"
                    });
                    self.context = Some(Arc::new(context));
                }
                Err(error) => self.status = error,
            }
        }
    }
}
impl Drop for Review {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}
enum Form {
    New {
        name: String,
        start: String,
        checkout: bool,
    },
    Manage {
        action: Management,
        branch: String,
        upstream: String,
    },
    // A fixed, immutable branch/tip confirmation, not an immediately executing menu.
    ConfirmDelete {
        branch: Reference,
    },
    Checkout(Review),
}
struct Dialog {
    snapshot: Snapshot,
    form: Form,
    viewport: egui::ViewportId,
}
#[derive(Default)]
pub struct Dialogs {
    active: Option<Dialog>,
}
impl Dialogs {
    pub fn is_open(&self) -> bool {
        self.active.is_some()
    }
    pub fn create(&mut self, snapshot: &Snapshot) {
        let start = snapshot
            .references
            .iter()
            .find(|r| r.name == "HEAD")
            .or(snapshot.references.first())
            .map(|r| r.name.clone())
            .unwrap_or_default();
        self.set(
            snapshot,
            Form::New {
                name: String::new(),
                start,
                checkout: true,
            },
        );
    }
    pub fn checkout(&mut self, snapshot: &Snapshot, target: Checkout) {
        self.set(snapshot, Form::Checkout(Review::new(target)));
    }
    pub fn confirm_delete(&mut self, snapshot: &Snapshot, branch: &Reference) {
        self.set(
            snapshot,
            Form::ConfirmDelete {
                branch: branch.clone(),
            },
        );
    }
    pub fn manage(
        &mut self,
        snapshot: &Snapshot,
        action: Management,
        selected: Option<&Reference>,
    ) {
        let current = format!(
            "refs/heads/{}",
            snapshot.branch.as_deref().unwrap_or_default()
        );
        let branches = snapshot
            .references
            .iter()
            .filter(|r| r.name.starts_with("refs/heads/"))
            .collect::<Vec<_>>();
        let branch = selected
            .or_else(|| branches.iter().copied().find(|r| r.name == current))
            .or(branches.first().copied());
        let upstream = snapshot
            .references
            .iter()
            .filter(|r| r.name.starts_with("refs/remotes/") && !r.name.ends_with("/HEAD"))
            .find(|r| Some(&r.name) == branch.and_then(|b| b.upstream.as_ref()))
            .or_else(|| {
                snapshot
                    .references
                    .iter()
                    .find(|r| r.name.starts_with("refs/remotes/") && !r.name.ends_with("/HEAD"))
            });
        self.set(
            snapshot,
            Form::Manage {
                action,
                branch: branch.map(|r| r.name.clone()).unwrap_or_default(),
                upstream: upstream.map(|r| r.name.clone()).unwrap_or_default(),
            },
        );
    }
    fn set(&mut self, snapshot: &Snapshot, form: Form) {
        self.active = Some(Dialog {
            snapshot: snapshot.clone(),
            form,
            viewport: egui::ViewportId::from_hash_of(("branch-options", uuid::Uuid::new_v4())),
        });
    }
    pub fn toolbar(&mut self, ui: &mut egui::Ui, snapshot: Option<&Snapshot>) {
        ui.add_enabled_ui(snapshot.is_some(), |ui| {
            ui.menu_button(text("BranchToolbar"), |ui| {
                let Some(snapshot) = snapshot else {
                    return;
                };
                if ui.button(text("ActionTitleNewBranch")).clicked() {
                    self.create(snapshot);
                    ui.close();
                }
                ui.separator();
                for reference in snapshot
                    .references
                    .iter()
                    .filter(|r| r.name.starts_with("refs/heads/"))
                {
                    if ui
                        .button(reference.name.trim_start_matches("refs/heads/"))
                        .clicked()
                    {
                        self.checkout(snapshot, Checkout::Branch(reference.clone()));
                        ui.close();
                    }
                }
                ui.separator();
                ui.menu_button(text("BranchManageTitleDeleteBranch"), |ui| {
                    for reference in snapshot
                        .references
                        .iter()
                        .filter(|r| r.name.starts_with("refs/heads/"))
                    {
                        if ui
                            .button(reference.name.trim_start_matches("refs/heads/"))
                            .clicked()
                        {
                            self.confirm_delete(snapshot, reference);
                            ui.close();
                        }
                    }
                });
                for action in [Management::SetUpstream, Management::UnsetUpstream] {
                    if ui.button(text(action.title())).clicked() {
                        self.manage(snapshot, action, None);
                        ui.close();
                    }
                }
            })
            .response
            .on_disabled_hover_text(text("ActionInspectionNeeded"));
        });
    }
    pub fn ref_menu(&mut self, ui: &mut egui::Ui, snapshot: &Snapshot, reference: &Reference) {
        if reference.name.starts_with("refs/heads/") {
            if ui.button(text("ActionTitleSwitchBranch")).clicked() {
                self.checkout(snapshot, Checkout::Branch(reference.clone()));
                ui.close();
            }
            if ui.button(text("BranchManageTitleDeleteBranch")).clicked() {
                self.confirm_delete(snapshot, reference);
                ui.close();
            }
            for action in [Management::SetUpstream, Management::UnsetUpstream] {
                if ui.button(text(action.title())).clicked() {
                    self.manage(snapshot, action, Some(reference));
                    ui.close();
                }
            }
        }
    }
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        git: Option<&Arc<Git>>,
        operations: &mut operations::Controller,
        scheduler: &Scheduler,
        enabled: bool,
    ) {
        let Some(dialog) = &mut self.active else {
            return;
        };
        let Some(git) = git else {
            return;
        };
        if let Form::Checkout(review) = &mut dialog.form {
            review.poll();
        }
        let title = match &dialog.form {
            Form::New { .. } => "ActionTitleNewBranch",
            Form::Manage { action, .. } => action.title(),
            Form::ConfirmDelete { .. } => "BranchManageTitleDeleteBranch",
            Form::Checkout(review) => review.title(),
        };
        let mut close = false;
        let mut submitted = None;
        let mut next_review = None;
        let mut visible = false;
        let snapshot = &dialog.snapshot;
        ctx.show_viewport_immediate(
            dialog.viewport,
            egui::ViewportBuilder::default()
                .with_title(text(title))
                .with_inner_size([720., 540.])
                .with_min_inner_size([480., 420.])
                .with_resizable(false)
                .with_minimize_button(false)
                .with_maximize_button(false),
            |ui, _| {
                visible |= ui.is_visible();
                let child_ctx = ui.ctx().clone();
                if !enabled {
                    ui.disable();
                }
                if child_ctx.input(|i| i.viewport().close_requested())
                    || enabled && child_ctx.input(|i| i.key_pressed(egui::Key::Escape))
                {
                    close = true;
                }
                egui::CentralPanel::default().show(ui, |ui| {
                    ui.heading(text(title));
                    ui.add_space(8.);
                    ui.add(egui::Label::new(snapshot.worktree.to_string_lossy()).truncate())
                        .on_hover_text(snapshot.worktree.to_string_lossy());
                    ui.add_space(12.);
                    egui::ScrollArea::vertical()
                        .id_salt("branch-options-body")
                        .max_height((ui.available_height() - 36.).max(120.))
                        .show(ui, |ui| {
                            match &mut dialog.form {
                                Form::New {
                                    name,
                                    start,
                                    checkout,
                                } => {
                                    ui.label(text("RustNewBranchHelp"));
                                    ui.label(text("ActionBranchNameLabel"));
                                    ui.add(
                                        egui::TextEdit::singleline(name)
                                            .char_limit(1024)
                                            .desired_width(f32::INFINITY),
                                    );
                                    ui.label(text("ActionStart"));
                                    egui::ComboBox::from_id_salt("branch-start")
                                        .selected_text(start.as_str())
                                        .width(ui.available_width().min(620.))
                                        .show_ui(ui, |ui| {
                                            for reference in &snapshot.references {
                                                ui.selectable_value(
                                                    start,
                                                    reference.name.clone(),
                                                    format!(
                                                        "{} · {}",
                                                        reference.name, reference.hash
                                                    ),
                                                );
                                            }
                                        });
                                    ui.checkbox(checkout, text("ActionCheckout"));
                                    let reference =
                                        snapshot.references.iter().find(|r| &r.name == start);
                                    let valid = !name.trim().is_empty() && reference.is_some();
                                    if ui
                                        .add_enabled(
                                            valid,
                                            egui::Button::new(text("ActionTitleNewBranch")),
                                        )
                                        .clicked()
                                    {
                                        let start = reference.unwrap().clone();
                                        if *checkout {
                                            next_review = Some(Checkout::New {
                                                name: name.clone(),
                                                start,
                                            });
                                        } else {
                                            submitted = Some(Action::Create {
                                                name: name.clone(),
                                                start,
                                            });
                                        }
                                    }
                                    if !valid {
                                        ui.label(text(if reference.is_none() {
                                            "ActionNoStart"
                                        } else {
                                            "ActionEnterBranchName"
                                        }));
                                    }
                                }
                                Form::Manage {
                                    action,
                                    branch,
                                    upstream,
                                } => {
                                    ui.label(text(action.explanation()));
                                    ui.add_space(10.);
                                    ui.label(text("BranchManageLocalBranch"));
                                    egui::ComboBox::from_id_salt("branch")
                                        .selected_text(branch.trim_start_matches("refs/heads/"))
                                        .width(ui.available_width().min(620.))
                                        .show_ui(ui, |ui| {
                                            for reference in snapshot
                                                .references
                                                .iter()
                                                .filter(|r| r.name.starts_with("refs/heads/"))
                                            {
                                                ui.selectable_value(
                                                    branch,
                                                    reference.name.clone(),
                                                    reference
                                                        .name
                                                        .trim_start_matches("refs/heads/"),
                                                );
                                            }
                                        });
                                    let selected =
                                        snapshot.references.iter().find(|r| &r.name == branch);
                                    if let Some(selected) = selected {
                                        ui.label(strings::format(
                                            "BranchManageTarget",
                                            &[
                                                &selected.name,
                                                &selected.hash,
                                                selected
                                                    .upstream
                                                    .as_deref()
                                                    .unwrap_or(&text("BranchManageNoUpstream")),
                                            ],
                                        ));
                                    }
                                    if *action == Management::SetUpstream {
                                        ui.label(text("BranchManageRemoteBranch"));
                                        egui::ComboBox::from_id_salt("upstream")
                                            .selected_text(
                                                upstream.trim_start_matches("refs/remotes/"),
                                            )
                                            .width(ui.available_width().min(620.))
                                            .show_ui(ui, |ui| {
                                                for reference in
                                                    snapshot.references.iter().filter(|r| {
                                                        r.name.starts_with("refs/remotes/")
                                                            && !r.name.ends_with("/HEAD")
                                                    })
                                                {
                                                    ui.selectable_value(
                                                        upstream,
                                                        reference.name.clone(),
                                                        reference
                                                            .name
                                                            .trim_start_matches("refs/remotes/"),
                                                    );
                                                }
                                            });
                                    }
                                    let remote =
                                        snapshot.references.iter().find(|r| &r.name == upstream);
                                    let problem = if selected.is_none() {
                                        Some("BranchManageChooseBranch")
                                    } else if *action == Management::SetUpstream && remote.is_none()
                                    {
                                        Some("BranchManageNoRemoteRef")
                                    } else {
                                        None
                                    };
                                    if let Some(problem) = problem {
                                        ui.label(text(problem));
                                    }
                                    if ui
                                        .add_enabled(
                                            problem.is_none(),
                                            egui::Button::new(text(action.submit())),
                                        )
                                        .clicked()
                                    {
                                        submitted = Some(Action::Manage {
                                            branch: selected.unwrap().clone(),
                                            action: match action {
                                                Management::UnsetUpstream => Manage::UnsetUpstream,
                                                Management::SetUpstream => {
                                                    Manage::SetUpstream(remote.unwrap().clone())
                                                }
                                            },
                                        });
                                    }
                                }
                                Form::ConfirmDelete { branch } => {
                                    // Confirmation displays the actual immutable repository,
                                    // full branch name and exact reviewed tip before enabling deletion.
                                    ui.label(strings::format(
                                        "RustDeleteBranchConfirmation",
                                        &[
                                            &snapshot.worktree.to_string_lossy(),
                                            &branch.name,
                                            &branch.hash,
                                        ],
                                    ));
                                    let current = branch.name
                                        == format!(
                                            "refs/heads/{}",
                                            snapshot.branch.as_deref().unwrap_or_default()
                                        );
                                    if current {
                                        ui.label(text("BranchManageCurrent"));
                                    }
                                    if ui
                                        .add_enabled(
                                            !current,
                                            egui::Button::new(text("RustConfirmDeleteBranch")),
                                        )
                                        .clicked()
                                    {
                                        submitted = Some(Action::Manage {
                                            branch: branch.clone(),
                                            action: Manage::Delete,
                                        });
                                    }
                                }
                                Form::Checkout(review) => {
                                    ui.label(text(
                                        if matches!(review.target, Checkout::Detached(_)) {
                                            "CheckoutDetachedHelp"
                                        } else {
                                            "CheckoutBranchHelp"
                                        },
                                    ));
                                    ui.strong(review.target_text());
                                    ui.add_space(14.);
                                    if review
                                        .context
                                        .as_ref()
                                        .is_some_and(|c| !c.entries.is_empty())
                                    {
                                        ui.label(text("CheckoutLocalChanges"));
                                        ui.radio_value(
                                            &mut review.changes,
                                            Changes::Keep,
                                            text("CheckoutKeep"),
                                        );
                                        ui.radio_value(
                                            &mut review.changes,
                                            Changes::StashAndReapply,
                                            text("CheckoutStash"),
                                        );
                                        ui.radio_value(
                                            &mut review.changes,
                                            Changes::DiscardTracked,
                                            text("CheckoutDiscard"),
                                        );
                                    }
                                    let discard = review.changes == Changes::DiscardTracked
                                        && review
                                            .context
                                            .as_ref()
                                            .is_some_and(|c| !c.entries.is_empty());
                                    if discard {
                                        // The final button is an explicit destructive confirmation
                                        // naming repository, checkout target and every reviewed tracked path.
                                        ui.label(strings::format(
                                            "RustDiscardCheckoutConfirmation",
                                            &[
                                                &snapshot.worktree.to_string_lossy(),
                                                &review.target_text(),
                                            ],
                                        ));
                                        let files = review
                                            .context
                                            .as_ref()
                                            .unwrap()
                                            .entries
                                            .iter()
                                            .filter(|e| {
                                                e.group != working::Group::Untracked
                                                    && e.group != working::Group::Ignored
                                            })
                                            .flat_map(|e| {
                                                std::iter::once(&e.path).chain(e.original.iter())
                                            })
                                            .collect::<BTreeSet<_>>()
                                            .into_iter()
                                            .collect::<Vec<_>>();
                                        egui::ScrollArea::vertical()
                                            .id_salt("discard-targets")
                                            .max_height(100.)
                                            .show_rows(ui, 20., files.len(), |ui, range| {
                                                for index in range {
                                                    ui.label(files[index]);
                                                }
                                            });
                                    }
                                    ui.add_space(12.);
                                    ui.colored_label(
                                        if review.started
                                            && review.receiver.is_none()
                                            && review.context.is_none()
                                        {
                                            Color32::LIGHT_RED
                                        } else {
                                            ui.visuals().text_color()
                                        },
                                        &review.status,
                                    );
                                    let confirmation = if discard {
                                        "RustConfirmDiscardCheckout"
                                    } else {
                                        title
                                    };
                                    if ui
                                        .add_enabled(
                                            review.context.is_some(),
                                            egui::Button::new(text(confirmation)),
                                        )
                                        .clicked()
                                    {
                                        let context = review.context.as_ref().unwrap().clone();
                                        submitted = Some(Action::Checkout {
                                            target: review.target.clone(),
                                            changes: if context.entries.is_empty() {
                                                Changes::Keep
                                            } else {
                                                review.changes
                                            },
                                            review: context,
                                            stash_message: text("CheckoutStashMessage"),
                                        });
                                    }
                                }
                            }
                        });
                    if ui.button(text("ActionCancel")).clicked() {
                        close = true;
                    }
                });
            },
        );
        if let Some(action) = submitted {
            operations.submit_branch(title, action, snapshot, git.clone(), scheduler, ctx);
            close = true;
        }
        if let Some(target) = next_review {
            dialog.form = Form::Checkout(Review::new(target));
            ctx.request_repaint();
        } else if !close
            && visible
            && let Form::Checkout(review) = &mut dialog.form
            && !review.started
        {
            review.start(git.clone(), snapshot, ctx);
        }
        if close {
            self.active = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shovel_core::process::{self, Lifetime};
    use std::{
        fs,
        path::{Path, PathBuf},
        time::{Duration, Instant},
    };
    fn command(git: &Git, root: &Path, args: &[&str]) -> String {
        let result = process::run(
            &git.request(root, args.iter().copied(), Lifetime::AcceptedCommand, 65536),
            &Cancellation::default(),
            None,
        )
        .unwrap();
        assert!(result.success(), "{result:?}");
        String::from_utf8(result.stdout)
            .unwrap()
            .trim_end()
            .to_owned()
    }
    fn fixture() -> (tempfile::TempDir, Arc<Git>, Snapshot) {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let config = root.join("isolated.config");
        fs::write(&config, "[user]\nname=Fixture\nemail=fixture@example.invalid\n[commit]\ngpgsign=false\n[core]\nautocrlf=false\n").unwrap();
        let mut git = Git::discover(None, &Cancellation::default()).unwrap();
        git.environment
            .insert("GIT_CONFIG_GLOBAL".into(), Some(config.into_os_string()));
        git.environment
            .insert("GIT_CONFIG_NOSYSTEM".into(), Some("1".into()));
        command(&git, root, &["init", "--initial-branch=main"]);
        fs::write(root.join("tracked.txt"), "base\n").unwrap();
        command(&git, root, &["add", "tracked.txt"]);
        command(&git, root, &["commit", "-m", "base"]);
        command(&git, root, &["branch", "target"]);
        let snapshot = git.inspect(root, &Cancellation::default()).unwrap();
        (temp, Arc::new(git), snapshot)
    }
    struct Harness {
        ctx: egui::Context,
        dialogs: Dialogs,
        operations: operations::Controller,
        scheduler: Scheduler,
        git: Arc<Git>,
    }
    impl Harness {
        fn new(git: Arc<Git>) -> Self {
            let ctx = egui::Context::default();
            ctx.enable_accesskit();
            ctx.set_embed_viewports(true);
            Self {
                ctx,
                dialogs: Default::default(),
                operations: Default::default(),
                scheduler: Default::default(),
                git,
            }
        }
        fn render(&mut self, events: Vec<egui::Event>) -> egui::FullOutput {
            let ctx = self.ctx.clone();
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(900., 700.),
                    )),
                    events,
                    ..Default::default()
                },
                |_ui| {
                    self.dialogs.show(
                        &ctx,
                        Some(&self.git),
                        &mut self.operations,
                        &self.scheduler,
                        true,
                    );
                },
            );
            output.textures_delta.clear();
            output
        }
        fn click(&mut self, label: &str) {
            let output = self.visible();
            let update = output.platform_output.accesskit_update.as_ref().unwrap();
            let node = update
                .nodes
                .iter()
                .find(|(_, n)| {
                    n.label() == Some(label)
                        && n.bounds().is_some()
                        && matches!(
                            n.role(),
                            egui::Role::Button | egui::Role::RadioButton | egui::Role::CheckBox
                        )
                })
                .unwrap_or_else(|| panic!("missing control {label}"));
            let bounds = node.1.bounds().unwrap();
            let point = egui::pos2(
                (bounds.x0 + bounds.x1) as f32 / 2.,
                (bounds.y0 + bounds.y1) as f32 / 2.,
            );
            output.drop_without_applying_deltas();
            self.render(vec![egui::Event::PointerMoved(point)])
                .drop_without_applying_deltas();
            for pressed in [true, false] {
                self.render(vec![egui::Event::PointerButton {
                    pos: point,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                }])
                .drop_without_applying_deltas();
            }
        }
        fn visible(&mut self) -> egui::FullOutput {
            for _ in 0..4 {
                let output = self.render(vec![]);
                if output
                    .platform_output
                    .accesskit_update
                    .as_ref()
                    .is_some_and(|u| u.nodes.iter().any(|(_, n)| n.label().is_some()))
                {
                    return output;
                }
                output.drop_without_applying_deltas();
            }
            panic!("dialog did not publish visible controls");
        }
        fn ready(&mut self) {
            let until = Instant::now() + Duration::from_secs(15);
            loop {
                self.render(vec![]).drop_without_applying_deltas();
                if self.dialogs.active.as_ref().is_some_and(
                    |d| matches!(&d.form, Form::Checkout(review) if review.context.is_some()),
                ) {
                    break;
                }
                assert!(Instant::now() < until, "checkout scan did not complete");
                thread::sleep(Duration::from_millis(20));
            }
        }
        fn settled(&mut self) {
            let until = Instant::now() + Duration::from_secs(20);
            while self.scheduler.has_accepted_work() {
                assert!(Instant::now() < until);
                self.operations.poll(&self.ctx);
                thread::sleep(Duration::from_millis(20));
            }
            self.operations.poll(&self.ctx);
            assert!(!self.scheduler.records().is_empty());
            assert!(
                self.scheduler
                    .records()
                    .iter()
                    .all(|r| r.outcome == Some(shovel_core::scheduler::Outcome::Succeeded))
            );
        }
    }
    #[test]
    fn checkout_displays_before_scanning_and_submitted_stash_work_outlives_its_view() {
        let (temp, git, snapshot) = fixture();
        let root = temp.path();
        fs::write(root.join("tracked.txt"), "staged\n").unwrap();
        command(&git, root, &["add", "tracked.txt"]);
        fs::write(root.join("tracked.txt"), "unstaged\n").unwrap();
        let target = snapshot
            .references
            .iter()
            .find(|r| r.name == "refs/heads/target")
            .unwrap()
            .clone();
        let mut h = Harness::new(git.clone());
        h.dialogs.checkout(&snapshot, Checkout::Branch(target));
        assert!(
            matches!(&h.dialogs.active.as_ref().unwrap().form, Form::Checkout(review) if !review.started && review.receiver.is_none())
        );
        let output = h.visible();
        assert!(
            output
                .platform_output
                .accesskit_update
                .as_ref()
                .unwrap()
                .nodes
                .iter()
                .any(|(_, n)| n.value() == Some(text("CheckoutReading").as_str())),
            "first frame labels: {:?}",
            output
                .platform_output
                .accesskit_update
                .as_ref()
                .unwrap()
                .nodes
                .iter()
                .filter_map(|(_, n)| n.value().or(n.label()))
                .collect::<Vec<_>>()
        );
        output.drop_without_applying_deltas();
        h.ready();
        h.click(&text("CheckoutStash"));
        fs::write(root.join(".git/hooks/post-checkout"), "#!/bin/sh\necho entered > .git/entered\nwhile [ ! -f .git/release ]; do sleep 0.05; done\n").unwrap();
        struct Release(PathBuf);
        impl Drop for Release {
            fn drop(&mut self) {
                let _ = fs::write(&self.0, "done");
            }
        }
        let release = Release(root.join(".git/release"));
        h.click(&text("ActionTitleSwitchBranch"));
        assert!(!h.dialogs.is_open());
        assert!(h.scheduler.has_accepted_work());
        let until = Instant::now() + Duration::from_secs(15);
        while !root.join(".git/entered").is_file() {
            assert!(Instant::now() < until);
            thread::sleep(Duration::from_millis(20));
        }
        assert!(h.scheduler.has_accepted_work());
        drop(release);
        h.settled();
        assert_eq!(command(&git, root, &["branch", "--show-current"]), "target");
        assert_eq!(command(&git, root, &["show", ":tracked.txt"]), "staged");
        assert_eq!(
            fs::read_to_string(root.join("tracked.txt")).unwrap(),
            "unstaged\n"
        );
    }
    #[test]
    fn delete_requires_fixed_repository_branch_tip_confirmation_then_executes() {
        let (temp, git, snapshot) = fixture();
        let root = temp.path();
        let target = snapshot
            .references
            .iter()
            .find(|r| r.name == "refs/heads/target")
            .unwrap();
        let mut h = Harness::new(git.clone());
        h.dialogs.confirm_delete(&snapshot, target);
        let output = h.visible();
        let nodes = &output
            .platform_output
            .accesskit_update
            .as_ref()
            .unwrap()
            .nodes;
        assert!(nodes.iter().any(|(_, n)| n.value().is_some_and(|s| {
            s.contains(&snapshot.worktree.to_string_lossy().to_string())
                && s.contains(&target.name)
                && s.contains(&target.hash)
        })));
        output.drop_without_applying_deltas();
        assert!(!h.scheduler.has_accepted_work());
        h.click(&text("ActionCancel"));
        assert!(!h.dialogs.is_open());
        assert_eq!(command(&git, root, &["rev-parse", "target"]), target.hash);
        h.dialogs.confirm_delete(&snapshot, target);
        h.click(&text("RustConfirmDeleteBranch"));
        assert!(!h.dialogs.is_open());
        h.settled();
        assert!(
            !git.inspect(root, &Cancellation::default())
                .unwrap()
                .references
                .iter()
                .any(|r| r.name == target.name)
        );
    }
    #[test]
    fn tracked_discard_lists_exact_paths_and_runs_only_after_specific_confirmation() {
        let (temp, git, snapshot) = fixture();
        let root = temp.path();
        fs::write(root.join("tracked.txt"), "discarded\n").unwrap();
        fs::write(root.join("untracked.txt"), "kept\n").unwrap();
        let target = snapshot
            .references
            .iter()
            .find(|r| r.name == "refs/heads/target")
            .unwrap()
            .clone();
        let mut h = Harness::new(git.clone());
        h.dialogs
            .checkout(&snapshot, Checkout::Branch(target.clone()));
        h.ready();
        h.click(&text("CheckoutDiscard"));
        let output = h.render(vec![]);
        let nodes = &output
            .platform_output
            .accesskit_update
            .as_ref()
            .unwrap()
            .nodes;
        assert!(nodes.iter().any(|(_, n)| n.value() == Some("tracked.txt")));
        assert!(
            !nodes
                .iter()
                .any(|(_, n)| n.value() == Some("untracked.txt"))
        );
        assert!(nodes.iter().any(|(_, n)| {
            n.value()
                .is_some_and(|s| s.contains("Confirm discarding") && s.contains(&target.hash))
        }));
        output.drop_without_applying_deltas();
        assert!(!h.scheduler.has_accepted_work());
        assert_eq!(
            fs::read_to_string(root.join("tracked.txt")).unwrap(),
            "discarded\n"
        );
        h.click(&text("RustConfirmDiscardCheckout"));
        h.settled();
        assert_eq!(
            fs::read_to_string(root.join("tracked.txt")).unwrap(),
            "base\n"
        );
        assert_eq!(
            fs::read_to_string(root.join("untracked.txt")).unwrap(),
            "kept\n"
        );
    }
}
