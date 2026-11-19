use super::*;
use shovel_core::{
    git::Snapshot,
    repository_actions::{Action, Bisect},
};

#[derive(Clone, Copy)]
pub(super) enum Item {
    Refresh,
    Network(operations::Kind),
    Stash,
    Branch,
    Tag,
    Worktree,
    Patch,
    ExportPatch,
    Bisect,
    FlowInit,
    Feature(bool),
    LfsInstall,
    LfsFetch,
    LfsPull,
    LfsPattern(bool),
    Tool(&'static str),
}

impl Item {
    fn label(self) -> &'static str {
        use operations::Kind;
        match self {
            Self::Refresh => "RefreshRepository",
            Self::Network(Kind::Fetch) => "RepositoryFetch",
            Self::Network(Kind::Pull) => "RepositoryPull",
            Self::Network(Kind::Push) => "RepositoryPush",
            Self::Stash => "RepositorySaveStash",
            Self::Branch => "RepositoryNewBranch",
            Self::Tag => "RepositoryNewTag",
            Self::Worktree => "RepositoryNewWorktree",
            Self::Patch => "RepositoryApplyPatch",
            Self::ExportPatch => "HistoryExportPatch",
            Self::Bisect => "RepositoryBisect",
            Self::FlowInit => "RepositoryFlowInit",
            Self::Feature(false) => "RepositoryFlowStart",
            Self::Feature(true) => "RepositoryFlowFinish",
            Self::LfsInstall => "RepositoryLfsInstall",
            Self::LfsFetch => "RepositoryLfsFetch",
            Self::LfsPull => "RepositoryLfsPull",
            Self::LfsPattern(true) => "RepositoryLfsTrack",
            Self::LfsPattern(false) => "RepositoryLfsUntrack",
            Self::Tool("Explorer") => "RepositoryOpenExplorer",
            Self::Tool(_) => "RepositoryOpenConsole",
        }
    }
}

pub(super) const SHORTCUTS: &[(Item, egui::Modifiers, egui::Key)] = &[
    (Item::Refresh, egui::Modifiers::NONE, egui::Key::F5),
    (
        Item::Network(operations::Kind::Fetch),
        egui::Modifiers::CTRL.plus(egui::Modifiers::SHIFT),
        egui::Key::F,
    ),
    (
        Item::Network(operations::Kind::Pull),
        egui::Modifiers::CTRL.plus(egui::Modifiers::SHIFT),
        egui::Key::L,
    ),
    (
        Item::Network(operations::Kind::Push),
        egui::Modifiers::CTRL.plus(egui::Modifiers::SHIFT),
        egui::Key::P,
    ),
    (
        Item::Stash,
        egui::Modifiers::CTRL.plus(egui::Modifiers::SHIFT),
        egui::Key::H,
    ),
    (
        Item::Branch,
        egui::Modifiers::CTRL.plus(egui::Modifiers::SHIFT),
        egui::Key::B,
    ),
    (
        Item::Tag,
        egui::Modifiers::CTRL.plus(egui::Modifiers::SHIFT),
        egui::Key::G,
    ),
    (
        Item::Tool("Explorer"),
        egui::Modifiers::CTRL.plus(egui::Modifiers::ALT),
        egui::Key::O,
    ),
    (
        Item::Tool("Bash"),
        egui::Modifiers::CTRL.plus(egui::Modifiers::ALT),
        egui::Key::T,
    ),
];

fn shortcut_text(ctx: &egui::Context, modifiers: egui::Modifiers, key: egui::Key) -> String {
    ctx.format_shortcut(&egui::KeyboardShortcut::new(modifiers, key))
}

impl Workspace {
    fn repository_menu_unavailable(&self, item: Item) -> Option<&'static str> {
        let Some(tab) = self.tabs.get(self.selected) else {
            return Some("RepositorySelectFirst");
        };
        if matches!(item, Item::Refresh) {
            return None;
        }
        if tab.view.identity_blocked {
            return Some("IdentityChanged");
        }
        if matches!(item, Item::Tool(_)) {
            return None;
        }
        if self.git.is_none() {
            return Some("GitErrorMissingGit");
        }
        let Some(snapshot) = tab.history.as_ref().map(|history| &history.snapshot) else {
            return if matches!(item, Item::Network(_) | Item::Branch) {
                None
            } else {
                Some("HistoryLoading")
            };
        };
        if !snapshot.operations.is_empty() {
            if matches!(item, Item::Bisect) && snapshot.operations == ["bisect"] {
                return None;
            }
            if !matches!(item, Item::Network(operations::Kind::Fetch)) {
                return Some("ActionSequence");
            }
        }
        if matches!(item, Item::Network(_) | Item::LfsFetch | Item::LfsPull)
            && snapshot.remotes.is_empty()
        {
            return Some("RepositoryNeedRemote");
        }
        if matches!(
            item,
            Item::Stash
                | Item::Tag
                | Item::Worktree
                | Item::Bisect
                | Item::FlowInit
                | Item::Feature(_)
                | Item::LfsFetch
                | Item::LfsPull
        ) && snapshot.head.is_none()
        {
            return Some("RepositoryNeedCommit");
        }
        None
    }

    pub(super) fn repository_shortcuts(&mut self, ctx: &egui::Context) {
        for &(item, modifiers, key) in SHORTCUTS {
            if ctx.input_mut(|input| input.consume_key(modifiers, key)) {
                self.invoke_repository_item(item, ctx);
                break;
            }
        }
    }

    fn repository_row(&mut self, ui: &mut egui::Ui, item: Item) {
        let unavailable = self.repository_menu_unavailable(item);
        let mut button = egui::Button::new(strings::text(item.label()));
        if let Some(&(_, modifiers, key)) = SHORTCUTS
            .iter()
            .find(|(candidate, _, _)| candidate.label() == item.label())
        {
            button = button.shortcut_text(shortcut_text(ui.ctx(), modifiers, key));
        }
        let response = ui.add_enabled(unavailable.is_none(), button);
        let response = if let Some(hint) = unavailable {
            response.on_disabled_hover_text(strings::text(hint))
        } else {
            response
        };
        if response.clicked() {
            self.invoke_repository_item(item, ui.ctx());
            ui.close();
        }
    }

    pub(super) fn repository_menu(&mut self, ui: &mut egui::Ui) {
        use operations::Kind;
        for group in [
            &[Item::Refresh][..],
            &[
                Item::Network(Kind::Fetch),
                Item::Network(Kind::Pull),
                Item::Network(Kind::Push),
            ],
            &[Item::Stash],
            &[Item::Branch, Item::Tag, Item::Worktree],
        ] {
            for &item in group {
                self.repository_row(ui, item);
            }
            ui.separator();
        }
        ui.menu_button(strings::text("RepositoryGitFlow"), |ui| {
            for item in [Item::FlowInit, Item::Feature(false), Item::Feature(true)] {
                self.repository_row(ui, item);
            }
        });
        ui.menu_button(strings::text("RepositoryGitLfs"), |ui| {
            for item in [
                Item::LfsInstall,
                Item::LfsFetch,
                Item::LfsPull,
                Item::LfsPattern(true),
                Item::LfsPattern(false),
            ] {
                self.repository_row(ui, item);
            }
        });
        for group in [
            &[Item::Patch][..],
            &[Item::Bisect],
            &[Item::Tool("Explorer"), Item::Tool("Bash")],
        ] {
            ui.separator();
            for &item in group {
                self.repository_row(ui, item);
            }
        }
        ui.separator();
        let tab = self.tabs.get(self.selected);
        self.remotes.menu(
            ui,
            tab.and_then(|tab| tab.history.as_ref())
                .map(|history| &history.snapshot),
            tab.map(|tab| &tab.remotes),
            None,
        );
        if ui
            .add_enabled(
                self.can_choose_git(),
                egui::Button::new(strings::text("ChooseGit")),
            )
            .on_disabled_hover_text(strings::text("RustGitSelectionBusy"))
            .clicked()
        {
            self.choose_git(ui.ctx());
            ui.close();
        }
        self.repository_row_stashes(ui);
    }

    fn repository_row_stashes(&mut self, ui: &mut egui::Ui) {
        let available = self.git.is_some()
            && self
                .tabs
                .get(self.selected)
                .is_some_and(|tab| tab.history.is_some() && !tab.view.identity_blocked);
        if ui
            .add_enabled(available, egui::Button::new(strings::text("Stashes")))
            .on_disabled_hover_text(strings::text("RepositorySelectFirst"))
            .clicked()
        {
            self.open_stashes(None);
            ui.close();
        }
    }

    fn invoke_repository_item(&mut self, item: Item, ctx: &egui::Context) {
        if self.repository_menu_unavailable(item).is_some() {
            return;
        }
        let tab = &self.tabs[self.selected];
        match item {
            Item::Refresh => self.refresh(ctx),
            Item::Network(kind) => self.operations.open_network_options(
                kind,
                &tab.path,
                tab.history.as_ref().map(|history| &history.snapshot),
                &self.settings,
                &self.scheduler,
                ctx,
            ),
            Item::Branch => {
                self.operations
                    .defer_new_branch(&tab.path, &self.settings, &self.scheduler, ctx)
            }
            Item::Tool(kind) => {
                let path = tab.path.clone();
                let settings = self.settings.clone();
                self.begin_tool_handoff(ctx, move || {
                    repository_tools::launch(kind, &path, &settings)
                });
            }
            _ => {
                let Some(history) = &tab.history else {
                    return;
                };
                let snapshot = &history.snapshot;
                let action = match item {
                    Item::LfsInstall => Some(Action::LfsInstall),
                    Item::LfsFetch => Some(Action::LfsFetch),
                    Item::LfsPull => Some(Action::LfsPull),
                    _ => None,
                };
                if let Some(action) = action
                    && let Some(git) = self.git.clone()
                {
                    self.operations.submit_repository_action(
                        item.label(),
                        action,
                        snapshot,
                        git,
                        &self.scheduler,
                        ctx,
                    );
                } else if self.repository_dialog.is_none() {
                    self.repository_dialog =
                        Some(Dialog::new(item, snapshot.clone(), tab.selected.as_deref()));
                }
            }
        }
    }

    pub(super) fn show_repository_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut dialog) = self.repository_dialog.take() else {
            return;
        };
        if let Some(submission) = dialog.show(
            ctx,
            &self.read_workers,
            !self.closing && !self.operations.authentication_open(),
        ) {
            if let Some(git) = self.git.clone() {
                match submission {
                    Submission::Repository(action) => self.operations.submit_repository_action(
                        dialog.item.label(),
                        action,
                        &dialog.snapshot,
                        git,
                        &self.scheduler,
                        ctx,
                    ),
                    Submission::Stash(action) => {
                        self.operations.submit_stash(
                            dialog.item.label(),
                            action,
                            &dialog.snapshot,
                            git,
                            &self.scheduler,
                            ctx,
                        );
                    }
                }
            }
        } else if dialog.open {
            self.repository_dialog = Some(dialog);
        }
    }
}

enum Submission {
    Repository(Action),
    Stash(shovel_core::stashes::Action),
}

pub(super) struct Dialog {
    item: Item,
    snapshot: Snapshot,
    open: bool,
    name: String,
    detail: String,
    destination: String,
    target: String,
    fixed_target: bool,
    patch_parent: Option<String>,
    include_untracked: bool,
    picker: Option<mpsc::Receiver<Option<PathBuf>>>,
}

impl Dialog {
    pub(super) fn at_commit(snapshot: Snapshot, hash: &str, branch: bool) -> Self {
        let mut dialog = Self::new(if branch { Item::Branch } else { Item::Tag }, snapshot, Some(hash));
        dialog.fixed_target = true;
        dialog
    }
    pub(super) fn export_patch(
        snapshot: Snapshot,
        hash: &str,
        parent: Option<String>,
        ctx: &egui::Context,
        workers: &read_workers::Workers,
    ) -> Self {
        let mut dialog = Self::new(Item::ExportPatch, snapshot, Some(hash));
        dialog.patch_parent = parent;
        dialog.browse(ctx, workers);
        dialog
    }
    fn new(item: Item, snapshot: Snapshot, selection: Option<&str>) -> Self {
        let target = selection
            .or(snapshot.head.as_deref())
            .unwrap_or_default()
            .to_owned();
        let (name, detail) = if matches!(item, Item::FlowInit) {
            let production = ["main", "master"].into_iter().find(|name| {
                snapshot
                    .references
                    .iter()
                    .any(|r| r.name == format!("refs/heads/{name}"))
            });
            (
                production
                    .or(snapshot.branch.as_deref())
                    .unwrap_or_default()
                    .to_owned(),
                "develop".to_owned(),
            )
        } else {
            (String::new(), String::new())
        };
        Self {
            item,
            snapshot,
            target,
            fixed_target: false,
            patch_parent: None,
            open: true,
            name,
            detail,
            destination: String::new(),
            include_untracked: false,
            picker: None,
        }
    }

    pub(super) fn picking(&self) -> bool {
        self.picker.is_some()
    }

    fn browse(&mut self, ctx: &egui::Context, workers: &read_workers::Workers) {
        let item = self.item;
        let (sender, receiver) = mpsc::channel();
        let lease = workers.start_required();
        let ctx = ctx.clone();
        let root = self.snapshot.worktree.clone();
        let filename = format!("{}.patch", self.target.chars().take(12).collect::<String>());
        self.picker = Some(receiver);
        thread::spawn(move || {
            let _lease = lease;
            let picker = rfd::FileDialog::new().set_directory(root);
            let result = match item {
                Item::Patch => picker.add_filter("Git patch", &["patch", "diff"]).pick_file(),
                Item::ExportPatch => picker.add_filter("Git patch", &["patch"])
                    .set_file_name(filename).save_file(),
                _ => picker.pick_folder(),
            };
            let _ = sender.send(result);
            ctx.request_repaint();
        });
    }

    fn show(
        &mut self,
        ctx: &egui::Context,
        workers: &read_workers::Workers,
        enabled: bool,
    ) -> Option<Submission> {
        if matches!(self.item, Item::ExportPatch) {
            let result = self.picker.as_ref()?.try_recv();
            if matches!(result, Err(mpsc::TryRecvError::Empty)) {
                return None;
            }
            self.picker = None;
            self.open = false;
            let path = result.ok().flatten()?;
            return Some(Submission::Repository(Action::ExportPatch {
                commit: self.target.clone(),
                parent: self.patch_parent.take(),
                destination: path,
            }));
        }
        if let Some(picker) = &self.picker {
            match picker.try_recv() {
                Ok(result) => {
                    if let Some(path) = result {
                        self.destination = path.to_string_lossy().into_owned();
                    }
                    self.picker = None;
                }
                Err(mpsc::TryRecvError::Disconnected) => self.picker = None,
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        let mut open = self.open;
        let mut dismissed = false;
        let mut submission = None;
        let is_bisecting = self.snapshot.operations == ["bisect"];
        egui::Window::new(strings::text(self.item.label()))
            .id(egui::Id::unique("repository-command"))
            .open(&mut open)
            .collapsible(false)
            .default_width(410.)
            .enabled(enabled && !self.picking())
            .show(ctx, |ui| {
                ui.label(self.snapshot.worktree.to_string_lossy());
                match self.item {
                    Item::Stash => {
                        field(ui, "StashMessage", &mut self.detail);
                        ui.checkbox(
                            &mut self.include_untracked,
                            strings::text("RepositoryIncludeUntracked"),
                        );
                    }
                    Item::Branch => {
                        field(ui, "RepositoryBranchName", &mut self.name);
                        ui.monospace(&self.target);
                    }
                    Item::Tag => {
                        field(ui, "RepositoryTagName", &mut self.name);
                        if self.fixed_target {
                            ui.monospace(&self.target);
                        } else {
                            field(ui, "RepositoryRevision", &mut self.target);
                        }
                        field(ui, "RepositoryTagMessage", &mut self.detail);
                    }
                    Item::Worktree => {
                        field(ui, "RepositoryBranchName", &mut self.name);
                        field(ui, "RepositoryRevision", &mut self.target);
                        field(ui, "RepositoryWorktreePath", &mut self.destination);
                    }
                    Item::Patch => field(ui, "RepositoryPatchPath", &mut self.destination),
                    Item::FlowInit => {
                        field(ui, "RepositoryProductionBranch", &mut self.name);
                        field(ui, "RepositoryDevelopmentBranch", &mut self.detail);
                    }
                    Item::Feature(_) => field(ui, "RepositoryFeatureName", &mut self.name),
                    Item::LfsPattern(_) => field(ui, "RepositoryLfsPattern", &mut self.name),
                    Item::Bisect if !is_bisecting => {
                        field(ui, "RepositoryGoodRevision", &mut self.name);
                        field(ui, "RepositoryBadRevision", &mut self.target);
                    }
                    _ => {}
                }
                if matches!(self.item, Item::Patch | Item::Worktree)
                    && ui.button(strings::text("InitBrowse")).clicked()
                {
                    self.browse(ctx, workers);
                }
                ui.horizontal_wrapped(|ui| {
                    if is_bisecting {
                        for (key, step) in [
                            ("RepositoryBisectGood", Bisect::Good),
                            ("RepositoryBisectBad", Bisect::Bad),
                            ("RepositoryBisectSkip", Bisect::Skip),
                            ("RepositoryBisectReset", Bisect::Reset),
                        ] {
                            if ui.button(strings::text(key)).clicked() {
                                submission = Some(Submission::Repository(Action::Bisect(step)));
                            }
                        }
                    } else if ui
                        .add_enabled(
                            self.submission_ready(),
                            egui::Button::new(strings::text(self.submit_label())),
                        )
                        .clicked()
                    {
                        submission = self.submission();
                    }
                    dismissed = ui.button(strings::text("Cancel")).clicked();
                });
            });
        self.open = (open && !dismissed) || self.picking();
        submission
    }

    fn submit_label(&self) -> &'static str {
        match self.item {
            Item::Stash => "ActionTitleStashCreate",
            Item::Branch | Item::Tag | Item::Worktree => "RepositoryCreate",
            Item::Patch => "RepositoryApply",
            Item::Bisect | Item::Feature(false) => "RepositoryStart",
            Item::Feature(true) => "RepositoryMergeFeature",
            Item::FlowInit => "RepositoryConfigure",
            Item::LfsPattern(true) => "FileLfsTrack",
            Item::LfsPattern(false) => "FileLfsUntrack",
            _ => self.item.label(),
        }
    }

    fn submission_ready(&self) -> bool {
        let named = !self.name.trim().is_empty();
        let target = !self.target.trim().is_empty();
        let path = PathBuf::from(self.destination.trim()).is_absolute();
        match self.item {
            Item::Stash => true,
            Item::Branch | Item::Tag | Item::Bisect => named && target,
            Item::Worktree => named && target && path,
            Item::Patch => path,
            Item::FlowInit => {
                named && !self.detail.trim().is_empty() && self.name.trim() != self.detail.trim()
            }
            Item::Feature(_) | Item::LfsPattern(_) => named,
            _ => false,
        }
    }

    fn submission(&self) -> Option<Submission> {
        let name = self.name.trim().to_owned();
        let target = self.target.trim().to_owned();
        let path = PathBuf::from(self.destination.trim());
        let action = match self.item {
            Item::Stash => {
                return Some(Submission::Stash(shovel_core::stashes::Action::Create {
                    message: self.detail.clone(),
                    include_untracked: self.include_untracked,
                }));
            }
            Item::Branch => Action::BranchAt { name, target },
            Item::Tag => Action::Tag {
                name,
                target,
                message: self.detail.clone(),
            },
            Item::Worktree => Action::Worktree {
                path,
                branch: name,
                target,
            },
            Item::Patch => Action::Patch(path),
            Item::Bisect => Action::BisectStart {
                good: name,
                bad: target,
            },
            Item::FlowInit => Action::FlowInit {
                production: name,
                development: self.detail.trim().to_owned(),
            },
            Item::Feature(finish) => Action::Feature { name, finish },
            Item::LfsPattern(track) => Action::LfsPattern {
                pattern: name,
                track,
            },
            _ => return None,
        };
        Some(Submission::Repository(action))
    }
}

fn field(ui: &mut egui::Ui, label: &str, value: &mut String) {
    let label = ui.label(strings::text(label));
    ui.add(
        egui::TextEdit::singleline(value)
            .desired_width(f32::INFINITY)
            .char_limit(4096),
    )
    .labelled_by(label.id);
}

pub(super) fn open_help(ctx: &egui::Context, key: &'static str) {
    ctx.data_mut(|data| data.insert_temp(egui::Id::unique(("help-open", key)), true));
}

pub(super) fn show_help(ctx: &egui::Context) {
    for key in ["HelpShortcuts", "HelpReleaseNotes"] {
        let id = egui::Id::unique(("help-open", key));
        if !ctx.data(|data| data.get_temp::<bool>(id).unwrap_or(false)) {
            continue;
        }
        let mut open = true;
        egui::Window::new(strings::text(key))
            .id(egui::Id::unique(("help-window", key)))
            .open(&mut open)
            .collapsible(false)
            .default_size(egui::vec2(650., 470.))
            .show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        if key == "HelpShortcuts" {
                            egui::Grid::new("workspace-shortcuts")
                                .spacing([36., 10.])
                                .striped(true)
                                .show(ui, |ui| {
                                    for &(item, modifiers, key) in SHORTCUTS {
                                        ui.label(strings::text(item.label()));
                                        ui.monospace(shortcut_text(ctx, modifiers, key));
                                        ui.end_row();
                                    }
                                    for (label, keys) in [
                                        ("HelpNextTab", "Ctrl+Tab"),
                                        ("HelpPreviousTab", "Ctrl+Shift+Tab"),
                                        ("CloseTab", "Ctrl+W"),
                                        ("HelpOlderHistory", "Ctrl+Page Down"),
                                        ("HelpNewerHistory", "Ctrl+Page Up"),
                                        ("Commit", "Ctrl+Enter"),
                                        ("HelpCommitPush", "Ctrl+Shift+Enter"),
                                        ("HelpStageAll", "Shift"),
                                    ] {
                                        ui.label(strings::text(label));
                                        ui.monospace(keys);
                                        ui.end_row();
                                    }
                                });
                        } else {
                            let mut releases = 0;
                            for line in include_str!("../../../CHANGELOG.md").lines() {
                                if let Some(title) = line.strip_prefix("## ") {
                                    if releases == 12 {
                                        break;
                                    }
                                    if releases != 0 {
                                        ui.separator();
                                    }
                                    releases += 1;
                                    ui.heading(title);
                                } else if releases > 0 && !line.trim().is_empty() {
                                    ui.add(egui::Label::new(line).selectable(true).wrap());
                                }
                            }
                        }
                    });
            });
        if !open {
            ctx.data_mut(|data| data.remove::<bool>(id));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        history_view::tests::{Fixture, wait},
        workspace_interaction_tests::{frame, sidebar_click, workspace},
    };

    #[test]
    fn tag_shortcut_captures_its_owner_across_tab_selection() {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let mut empty = workspace();
        empty.tabs.clear();
        assert_eq!(
            empty.repository_menu_unavailable(Item::Tag),
            Some("RepositorySelectFirst")
        );

        let first = Fixture::new();
        let second = Fixture::new();
        let mut app = first.app(&ctx);
        let modifiers = egui::Modifiers::CTRL.plus(egui::Modifiers::SHIFT);
        let input = vec![
            egui::Event::ModifiersChanged(modifiers),
            egui::Event::Key {
                key: egui::Key::G,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers,
            },
        ];
        frame(&ctx, &mut app, input).drop_without_applying_deltas();
        let dialog = app
            .repository_dialog
            .as_mut()
            .expect("tag shortcut opened its form");
        assert_eq!(dialog.snapshot.worktree, first.root);
        dialog.name = "keyboard-menu-tag".into();
        app.open(second.root.clone(), &ctx);
        wait(&mut app, &ctx);
        sidebar_click(
            &ctx,
            &mut app,
            &strings::text("RepositoryCreate"),
            egui::PointerButton::Primary,
        );
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
        loop {
            app.receive(&ctx);
            if !app.scheduler.has_accepted_work() {
                break;
            }
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert_eq!(
            first.command(&["tag", "--list", "keyboard-menu-tag"], &[]),
            "keyboard-menu-tag"
        );
        assert!(
            second
                .command(&["tag", "--list", "keyboard-menu-tag"], &[])
                .is_empty()
        );
        assert_eq!(app.recent_repositories()[0], second.root);
    }
}
