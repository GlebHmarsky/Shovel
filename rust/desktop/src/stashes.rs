use crate::{
    comparison, operations,
    strings::{self, text},
};
use eframe::egui;
use shovel_core::{
    git::{Git, Snapshot},
    process::Cancellation,
    scheduler::{Outcome, Scheduler},
    stashes::{Action, Entry, Inspection, List, Target},
};
use std::{
    path::PathBuf,
    sync::{
        Arc,
        mpsc::{self, Receiver, Sender},
    },
    thread,
};

enum Event {
    List(u64, Result<List, String>),
    Inspection(u64, String, Box<Result<Inspection, String>>),
}
struct Confirmation {
    snapshot: Snapshot,
    target: Target,
    entry: Entry,
}
pub struct View {
    pub path: PathBuf,
    pub open: bool,
    snapshot: Snapshot,
    git: Arc<Git>,
    viewport: egui::ViewportId,
    list: Option<List>,
    selected: Option<String>,
    navigation: Option<String>,
    message: String,
    include_untracked: bool,
    restore_index: bool,
    status: String,
    list_failed: bool,
    refresh_needed: bool,
    list_generation: u64,
    inspection_generation: u64,
    list_cancel: Cancellation,
    inspection_cancel: Cancellation,
    sender: Sender<Event>,
    receiver: Receiver<Event>,
    reading: bool,
    inspecting: bool,
    inspection: Option<Inspection>,
    comparison: usize,
    pane: Option<comparison::Pane>,
    confirm: Option<Confirmation>,
    pending: Vec<(u64, Option<String>)>,
}
impl View {
    pub fn take_navigation(&mut self) -> Option<String> {
        self.navigation.take()
    }

    pub fn new(snapshot: Snapshot, git: Arc<Git>) -> Self {
        let (sender, receiver) = mpsc::channel();
        Self {
            path: snapshot.worktree.clone(),
            open: true,
            snapshot,
            git,
            viewport: egui::ViewportId::from_hash_of(uuid::Uuid::new_v4()),
            list: None,
            selected: None,
            navigation: None,
            message: String::new(),
            include_untracked: false,
            restore_index: false,
            status: text("StashNotLoaded"),
            list_failed: true,
            refresh_needed: true,
            list_generation: 0,
            inspection_generation: 0,
            list_cancel: Default::default(),
            inspection_cancel: Default::default(),
            sender,
            receiver,
            reading: false,
            inspecting: false,
            inspection: None,
            comparison: 0,
            pane: None,
            confirm: None,
            pending: vec![],
        }
    }
    pub fn activate(&mut self, snapshot: &Snapshot) {
        self.snapshot = snapshot.clone();
        self.open = true;
        self.refresh_needed = true;
    }
    pub fn release_cache(&mut self) {
        if !self.open {
            self.list = None;
            self.inspection = None;
            self.pane = None;
        }
    }
    pub fn observe(&mut self, snapshot: &Snapshot) {
        if self.open && self.snapshot.identity == snapshot.identity {
            self.snapshot = snapshot.clone();
        }
    }
    pub fn changed(&mut self) {
        self.refresh_needed = true;
    }
    fn refresh(&mut self, ctx: &egui::Context) {
        self.list_cancel.cancel();
        self.list_cancel = Cancellation::default();
        self.list_generation += 1;
        self.reading = true;
        self.refresh_needed = false;
        self.status = text(if self.list.is_some() {
            "StashRefreshing"
        } else {
            "StashLoading"
        });
        let (git, expected, cancel, tx, ctx, generation) = (
            self.git.clone(),
            self.snapshot.clone(),
            self.list_cancel.clone(),
            self.sender.clone(),
            ctx.clone(),
            self.list_generation,
        );
        thread::spawn(move || {
            let result = git.stashes(&expected, &cancel).map_err(|e| text(&e.code()));
            let _ = tx.send(Event::List(generation, result));
            ctx.request_repaint();
        });
    }
    fn inspect(&mut self, ctx: &egui::Context) {
        self.inspection_cancel.cancel();
        self.inspection_cancel = Cancellation::default();
        self.inspection_generation += 1;
        let Some(hash) = self.selected.clone() else {
            self.pane = None;
            self.inspection = None;
            self.inspecting = false;
            return;
        };
        self.inspecting = true;
        let (git, expected, cancel, tx, ctx, generation) = (
            self.git.clone(),
            self.snapshot.clone(),
            self.inspection_cancel.clone(),
            self.sender.clone(),
            ctx.clone(),
            self.inspection_generation,
        );
        thread::spawn(move || {
            let result = git
                .inspect_stash(&expected, &hash, &cancel)
                .map_err(|e| text(&e.code()));
            let _ = tx.send(Event::Inspection(generation, hash, Box::new(result)));
            ctx.request_repaint();
        });
    }
    fn select_comparison(&mut self, ctx: &egui::Context) {
        let Some(data) = &self.inspection else {
            return;
        };
        let mut commit = match self.comparison {
            1 => data.index.clone(),
            3 => match &data.untracked {
                Some(commit) => commit.clone(),
                None => return,
            },
            _ => data.worktree.clone(),
        };
        commit.parents = match self.comparison {
            2 => vec![data.index.hash.clone()],
            3 => vec![],
            _ => vec![data.worktree.parents[0].clone()],
        };
        let pane = self
            .pane
            .get_or_insert_with(|| comparison::Pane::new(self.path.clone(), self.git.clone()));
        pane.details_index = 1;
        pane.select(&commit, ctx);
    }
    pub fn poll(&mut self, ctx: &egui::Context, scheduler: &Scheduler) {
        while let Ok(event) = self.receiver.try_recv() {
            if !self.open {
                continue;
            }
            match event {
                Event::List(generation, result) if generation == self.list_generation => {
                    self.reading = false;
                    match result {
                        Ok(list) => {
                            self.list_failed = false;
                            self.status =
                                strings::format("StashCount", &[&list.entries.len().to_string()]);
                            self.selected = list
                                .entries
                                .iter()
                                .find(|e| Some(&e.hash) == self.selected.as_ref())
                                .or(list.entries.first())
                                .map(|e| e.hash.clone());
                            self.list = Some(list);
                            self.inspect(ctx);
                        }
                        Err(error) => {
                            self.list_failed = true;
                            self.status = error;
                        }
                    }
                }
                Event::Inspection(generation, hash, result)
                    if generation == self.inspection_generation
                        && self.selected.as_ref() == Some(&hash) =>
                {
                    self.inspecting = false;
                    match *result {
                        Ok(data) => {
                            self.inspection = Some(data);
                            self.comparison = 0;
                            self.select_comparison(ctx);
                        }
                        Err(error) => self.status = error,
                    }
                }
                _ => {}
            }
        }
        let records = scheduler.records();
        self.pending.retain(|(id, submitted)| {
            if let Some(outcome) = records.iter().find(|r| r.id == *id).and_then(|r| r.outcome) {
                if outcome == Outcome::Succeeded && submitted.as_ref() == Some(&self.message) {
                    self.message.clear();
                }
                self.refresh_needed = true;
                false
            } else {
                true
            }
        });
    }
    fn close(&mut self) {
        self.open = false;
        self.list_cancel.cancel();
        self.inspection_cancel.cancel();
        self.list_generation += 1;
        self.inspection_generation += 1;
        self.reading = false;
        self.inspecting = false;
        self.pane = None;
        self.inspection = None;
        self.snapshot.references.clear();
        self.snapshot.remotes.clear();
        self.confirm = None;
    }
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        operations: &mut operations::Controller,
        scheduler: &Scheduler,
        enabled: bool,
    ) {
        if !self.open {
            return;
        }
        let mut visible = false;
        let mut close = false;
        let mut refresh = false;
        let mut inspect = false;
        let mut compare = false;
        let mut action: Option<(Action, Snapshot)> = None;
        ctx.show_viewport_immediate(
            self.viewport,
            egui::ViewportBuilder::default()
                .with_title(text("Stashes"))
                .with_inner_size([1200., 760.])
                .with_min_inner_size([900., 620.])
                .with_minimize_button(false),
            |ui, _| {
                visible |= ui.is_visible();
                if !enabled {
                    ui.disable();
                }
                let child = ui.ctx().clone();
                close |= child.input(|i| i.viewport().close_requested())
                    || enabled
                        && self.confirm.is_none()
                        && child.input(|i| i.key_pressed(egui::Key::Escape));
                egui::Panel::top("stash-toolbar").show(ui, |ui| {
                    ui.heading(text("Stashes"));
                    ui.label(self.path.to_string_lossy());
                    ui.horizontal(|ui| {
                        ui.label(text("StashMessage"));
                        ui.add(
                            egui::TextEdit::singleline(&mut self.message)
                                .char_limit(4096)
                                .desired_width(f32::INFINITY),
                        );
                    });
                    ui.horizontal_wrapped(|ui| {
                        ui.checkbox(&mut self.include_untracked, text("StashIncludeUntracked"));
                        if ui
                            .add_enabled(
                                self.snapshot.operations.is_empty() && self.snapshot.head.is_some(),
                                egui::Button::new(text("ActionTitleStashCreate")),
                            )
                            .on_hover_text(text("StashCreateHelp"))
                            .on_disabled_hover_text(text(if self.snapshot.head.is_none() {
                                "StashNeedsCommit"
                            } else {
                                "ActionSequence"
                            }))
                            .clicked()
                        {
                            action = Some((
                                Action::Create {
                                    message: self.message.clone(),
                                    include_untracked: self.include_untracked,
                                },
                                self.snapshot.clone(),
                            ));
                        }
                        if ui.button(text("CommitRefresh")).clicked() {
                            refresh = true;
                        }
                        ui.checkbox(&mut self.restore_index, text("StashRestoreIndex"))
                            .on_hover_text(text("StashRestoreHelp"));
                        let selected = self.list.as_ref().and_then(|list| {
                            list.entries
                                .iter()
                                .find(|e| Some(&e.hash) == self.selected.as_ref())
                                .map(|entry| (list, entry))
                        });
                        for (key, pop) in [
                            ("ActionTitleStashApply", false),
                            ("ActionTitleStashPop", true),
                        ] {
                            if ui
                                .add_enabled(
                                    selected.is_some()
                                        && !self.list_failed
                                        && self.snapshot.operations.is_empty(),
                                    egui::Button::new(text(key)),
                                )
                                .on_disabled_hover_text(text(
                                    if self.snapshot.operations.is_empty() {
                                        "StashApplyHelp"
                                    } else {
                                        "ActionSequence"
                                    },
                                ))
                                .clicked()
                                && let Some((list, entry)) = selected
                            {
                                action = Some((
                                    Action::Apply {
                                        target: Target::reviewed(list, entry),
                                        restore_index: self.restore_index,
                                        pop,
                                    },
                                    self.snapshot.clone(),
                                ));
                            }
                        }
                        if ui
                            .add_enabled(
                                selected.is_some() && !self.list_failed,
                                egui::Button::new(text("ActionTitleStashDrop")),
                            )
                            .on_disabled_hover_text(text("StashDropHelp"))
                            .clicked()
                            && let Some((list, entry)) = selected
                        {
                            self.confirm = Some(Confirmation {
                                snapshot: self.snapshot.clone(),
                                target: Target::reviewed(list, entry),
                                entry: entry.clone(),
                            });
                        }
                    });
                    ui.label(&self.status);
                    operations.status(ui, Some(&self.path), scheduler);
                });
                egui::Panel::bottom("stash-close").show(ui, |ui| {
                    if ui.button(text("RustCloseStashes")).clicked() {
                        close = true;
                    }
                });
                egui::Panel::left("stash-list")
                    .default_size(290.)
                    .min_size(160.)
                    .resizable(true)
                    .show(ui, |ui| {
                        if let Some(list) = &self.list {
                            egui::ScrollArea::both().show_rows(
                                ui,
                                48.,
                                list.entries.len(),
                                |ui, range| {
                                    for entry in &list.entries[range] {
                                        if ui
                                            .selectable_label(
                                                self.selected.as_ref() == Some(&entry.hash),
                                                strings::format(
                                                    "StashRow",
                                                    &[
                                                        &entry.selector,
                                                        &entry.message,
                                                        &entry
                                                            .date
                                                            .format("%Y-%m-%d %H:%M UTC")
                                                            .to_string(),
                                                    ],
                                                ),
                                            )
                                            .on_hover_text(&entry.hash)
                                            .clicked()
                                        {
                                            self.selected = Some(entry.hash.clone());
                                            self.navigation = Some(entry.hash.clone());
                                            inspect = true;
                                        }
                                    }
                                },
                            );
                        }
                    });
                egui::CentralPanel::default().show(ui, |ui| {
                    if self.inspecting {
                        ui.label(text("DiffRetainedLoading"));
                    }
                    if let Some(data) = &self.inspection {
                        egui::ScrollArea::vertical()
                            .id_salt("stash-message")
                            .max_height(75.)
                            .show(ui, |ui| {
                                ui.monospace(&data.worktree.hash);
                                ui.label(&data.worktree.message);
                            });
                        let old = self.comparison;
                        ui.horizontal_wrapped(|ui| {
                            ui.label(text("StashComparison"));
                            for (index, key) in [
                                "StashTracked",
                                "StashStaged",
                                "StashUnstaged",
                                "StashUntracked",
                            ]
                            .into_iter()
                            .enumerate()
                            {
                                if index != 3 || data.untracked.is_some() {
                                    ui.selectable_value(&mut self.comparison, index, text(key));
                                }
                            }
                        });
                        compare = old != self.comparison;
                    }
                    if let Some(pane) = &mut self.pane {
                        pane.show(ui);
                    }
                });
                if let Some(confirmation) = &self.confirm {
                    let mut dismiss = false;
                    egui::Modal::new(egui::Id::unique("stash-drop-confirmation")).show(
                        &child,
                        |ui| {
                            if !enabled {
                                ui.disable();
                            }
                            ui.set_max_width(600.);
                            ui.heading(text("ActionTitleStashDrop"));
                            ui.label(strings::format(
                                "StashConfirmDrop",
                                &[
                                    &confirmation.snapshot.worktree.to_string_lossy(),
                                    &confirmation.entry.selector,
                                    &confirmation.entry.hash,
                                    &confirmation.entry.message,
                                ],
                            ));
                            ui.horizontal(|ui| {
                                if ui.button(text("RustConfirmStashDrop")).clicked() {
                                    action = Some((
                                        Action::Drop(confirmation.target.clone()),
                                        confirmation.snapshot.clone(),
                                    ));
                                    dismiss = true;
                                }
                                if ui.button(text("ActionCancel")).clicked() {
                                    dismiss = true;
                                }
                            });
                        },
                    );
                    if dismiss {
                        self.confirm = None;
                    }
                }
            },
        );
        if let Some((action, snapshot)) = action {
            let submitted = if let Action::Create { message, .. } = &action {
                Some(message.clone())
            } else {
                None
            };
            if let Some(id) = operations.submit_stash(
                title(&action),
                action,
                &snapshot,
                self.git.clone(),
                scheduler,
                ctx,
            ) {
                self.pending.push((id, submitted));
            }
        }
        if close {
            self.close();
            return;
        }
        self.refresh_needed |= refresh;
        if visible && !self.reading && self.refresh_needed {
            self.refresh(ctx);
        }
        if inspect {
            self.inspect(ctx);
        } else if compare {
            self.select_comparison(ctx);
        }
    }
}
pub fn title(action: &Action) -> &'static str {
    match action {
        Action::Create { .. } => "ActionTitleStashCreate",
        Action::Apply { pop: false, .. } => "ActionTitleStashApply",
        Action::Apply { pop: true, .. } => "ActionTitleStashPop",
        Action::Drop(_) => "ActionTitleStashDrop",
    }
}
impl Drop for View {
    fn drop(&mut self) {
        self.list_cancel.cancel();
        self.inspection_cancel.cancel();
    }
}

pub struct QuickTarget<'a> {
    pub snapshot: &'a Snapshot,
    pub reference: &'a shovel_core::git::Reference,
    pub message: &'a str,
}
pub fn quick_actions(
    ui: &mut egui::Ui,
    target: QuickTarget<'_>,
    git: Option<&Arc<Git>>,
    operations: &mut operations::Controller,
    dialogs: &mut crate::branches::Dialogs,
    scheduler: &Scheduler,
) {
    let ctx = ui.ctx().clone();
    let selected = Target {
        hash: target.reference.hash.clone(),
        selector: target.reference.name.clone(),
        list_stamp: None,
    };
    for (key, pop) in [
        ("ActionTitleStashApply", false),
        ("ActionTitleStashPop", true),
    ] {
        if ui
            .add_enabled(
                git.is_some() && target.snapshot.operations.is_empty(),
                egui::Button::new(text(key)),
            )
            .on_disabled_hover_text(text("ActionSequence"))
            .clicked()
            && let Some(git) = git
        {
            operations.submit_stash(
                key,
                Action::Apply {
                    target: selected.clone(),
                    restore_index: false,
                    pop,
                },
                target.snapshot,
                git.clone(),
                scheduler,
                &ctx,
            );
            ui.close();
        }
    }
    if ui
        .add_enabled(
            git.is_some(),
            egui::Button::new(text("ActionTitleStashDrop")),
        )
        .clicked()
    {
        dialogs.confirm_stash_drop(target.snapshot, selected, target.message);
        ui.close();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shovel_core::process::{self, Lifetime};
    use std::{
        fs,
        path::Path,
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
        String::from_utf8(result.stdout).unwrap().trim_end().into()
    }
    fn fixture() -> (tempfile::TempDir, Arc<Git>, Snapshot) {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("repo");
        fs::create_dir(&root).unwrap();
        let config = temp.path().join("isolated.config");
        fs::write(&config, "[user]\nname=Fixture\nemail=fixture@example.invalid\n[commit]\ngpgsign=false\n[core]\nautocrlf=false\n").unwrap();
        let mut git = Git::discover(None, &Cancellation::default()).unwrap();
        git.environment
            .insert("GIT_CONFIG_GLOBAL".into(), Some(config.into_os_string()));
        git.environment
            .insert("GIT_CONFIG_NOSYSTEM".into(), Some("1".into()));
        command(&git, &root, &["init", "--initial-branch=main"]);
        fs::write(root.join("file"), "base\n").unwrap();
        command(&git, &root, &["add", "file"]);
        command(&git, &root, &["commit", "-m", "base"]);
        let snapshot = git.inspect(&root, &Cancellation::default()).unwrap();
        (temp, Arc::new(git), snapshot)
    }
    struct Harness {
        ctx: egui::Context,
        view: View,
        operations: operations::Controller,
        scheduler: Scheduler,
    }
    impl Harness {
        fn new(git: Arc<Git>, snapshot: Snapshot) -> Self {
            let ctx = egui::Context::default();
            ctx.enable_accesskit();
            ctx.set_embed_viewports(true);
            Self {
                ctx,
                view: View::new(snapshot, git),
                operations: Default::default(),
                scheduler: Default::default(),
            }
        }
        fn render(&mut self, events: Vec<egui::Event>) -> egui::FullOutput {
            let ctx = self.ctx.clone();
            self.operations.poll(&ctx);
            self.view.poll(&ctx, &self.scheduler);
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1200., 850.),
                    )),
                    events,
                    ..Default::default()
                },
                |_ui| {
                    self.view
                        .show(&ctx, &mut self.operations, &self.scheduler, true);
                },
            );
            output.textures_delta.clear();
            output
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
            panic!("stash controls did not appear");
        }
        fn click(&mut self, label: &str) {
            let output = self.visible();
            let node = output
                .platform_output
                .accesskit_update
                .as_ref()
                .unwrap()
                .nodes
                .iter()
                .find(|(_, n)| {
                    n.label() == Some(label)
                        && n.bounds().is_some()
                        && matches!(
                            n.role(),
                            egui::Role::Button | egui::Role::CheckBox | egui::Role::RadioButton
                        )
                })
                .unwrap_or_else(|| panic!("missing control {label}"));
            let rect = node.1.bounds().unwrap();
            let point = egui::pos2(
                (rect.x0 + rect.x1) as f32 / 2.,
                (rect.y0 + rect.y1) as f32 / 2.,
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
        fn ready(&mut self) {
            let until = Instant::now() + Duration::from_secs(20);
            loop {
                self.render(vec![]).drop_without_applying_deltas();
                if !self.view.reading
                    && !self.view.inspecting
                    && self.view.list.is_some()
                    && !self.view.list_failed
                {
                    break;
                }
                assert!(Instant::now() < until, "stash read: {}", self.view.status);
                thread::sleep(Duration::from_millis(20));
            }
        }
        fn settled(&mut self) {
            let until = Instant::now() + Duration::from_secs(25);
            while self.scheduler.has_accepted_work() {
                self.operations.poll(&self.ctx);
                self.view.poll(&self.ctx, &self.scheduler);
                assert!(Instant::now() < until);
                thread::sleep(Duration::from_millis(20));
            }
            self.operations.poll(&self.ctx);
            self.view.poll(&self.ctx, &self.scheduler);
            assert!(
                self.scheduler
                    .records()
                    .iter()
                    .all(|r| r.outcome == Some(Outcome::Succeeded))
            );
        }
    }
    #[test]
    fn create_reads_after_display_and_survives_close_without_clearing_newer_message() {
        let (_temp, git, snapshot) = fixture();
        let root = snapshot.worktree.clone();
        let mut h = Harness::new(git.clone(), snapshot.clone());
        assert!(!h.view.reading);
        assert_eq!(h.view.list_generation, 0);
        let output = h.visible();
        assert!(
            output
                .platform_output
                .accesskit_update
                .as_ref()
                .unwrap()
                .nodes
                .iter()
                .any(|(_, n)| n.value() == Some(text("StashNotLoaded").as_str()))
        );
        output.drop_without_applying_deltas();
        h.ready();
        fs::write(root.join("file"), "saved\n").unwrap();
        h.view.message = "submitted message".into();
        h.click(&text("ActionTitleStashCreate"));
        assert!(h.scheduler.has_accepted_work());
        h.view.message = "newer message".into();
        h.click(&text("RustCloseStashes"));
        assert!(!h.view.open);
        h.settled();
        assert_eq!(h.view.message, "newer message");
        assert_eq!(fs::read_to_string(root.join("file")).unwrap(), "base\n");
        let list = git.stashes(&snapshot, &Cancellation::default()).unwrap();
        assert_eq!(list.entries.len(), 1);
        assert!(list.entries[0].message.contains("submitted message"));
        h.view.activate(&snapshot);
        h.ready();
        assert_eq!(
            h.view.list.as_ref().unwrap().entries[0].hash,
            list.entries[0].hash
        );
        h.click(&text("ActionTitleStashPop"));
        h.click(&text("RustCloseStashes"));
        h.settled();
        assert_eq!(fs::read_to_string(root.join("file")).unwrap(), "saved\n");
        assert!(
            git.stashes(&snapshot, &Cancellation::default())
                .unwrap()
                .entries
                .is_empty()
        );
    }
    #[test]
    fn drop_confirmation_captures_saved_target_and_cancel_keeps_the_stash() {
        let (_temp, git, snapshot) = fixture();
        let root = &snapshot.worktree;
        fs::write(root.join("file"), "saved\n").unwrap();
        command(
            &git,
            root,
            &["stash", "push", "--message=reviewed saved data"],
        );
        let mut h = Harness::new(git.clone(), snapshot.clone());
        h.ready();
        let entry = h.view.list.as_ref().unwrap().entries[0].clone();
        // Failed refresh retains useful data but disables mutations from that list.
        let mut unavailable = (*git).clone();
        unavailable.executable = root.join("missing-git.exe");
        h.view.git = Arc::new(unavailable);
        h.view.changed();
        let until = Instant::now() + Duration::from_secs(10);
        loop {
            h.render(vec![]).drop_without_applying_deltas();
            if h.view.list_failed && !h.view.reading {
                break;
            }
            assert!(Instant::now() < until);
            thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(h.view.list.as_ref().unwrap().entries[0].hash, entry.hash);
        h.click(&text("ActionTitleStashApply"));
        assert!(!h.scheduler.has_accepted_work());
        h.view.git = git.clone();
        fs::write(root.join("file"), "newer saved bytes\n").unwrap();
        command(&git, root, &["stash", "push", "--message=newer stash"]);
        h.view.changed();
        h.ready();
        assert_eq!(h.view.selected.as_deref(), Some(entry.hash.as_str()));
        let entry = h
            .view
            .list
            .as_ref()
            .unwrap()
            .entries
            .iter()
            .find(|e| e.hash == entry.hash)
            .unwrap()
            .clone();
        assert_eq!(entry.selector, "stash@{1}");
        h.click(&text("ActionTitleStashDrop"));
        h.render(vec![]).drop_without_applying_deltas();
        let output = h.visible();
        let labels = output
            .platform_output
            .accesskit_update
            .as_ref()
            .unwrap()
            .nodes
            .iter()
            .filter_map(|(_, n)| n.value().or(n.label()))
            .collect::<Vec<_>>()
            .join("\n");
        for value in [
            root.to_string_lossy().as_ref(),
            &entry.hash,
            &entry.selector,
            &entry.message,
        ] {
            assert!(labels.contains(value), "missing reviewed target {value}");
        }
        output.drop_without_applying_deltas();
        assert!(!h.scheduler.has_accepted_work());
        h.click(&text("ActionCancel"));
        assert!(
            git.stashes(&snapshot, &Cancellation::default())
                .unwrap()
                .entries
                .iter()
                .any(|e| e.hash == entry.hash)
        );
        h.click(&text("ActionTitleStashDrop"));
        h.click(&text("RustConfirmStashDrop"));
        h.settled();
        let retained = git.stashes(&snapshot, &Cancellation::default()).unwrap();
        assert_eq!(retained.entries.len(), 1);
        assert!(retained.entries[0].message.contains("newer stash"));
        assert_eq!(fs::read_to_string(root.join("file")).unwrap(), "base\n");
    }
}
