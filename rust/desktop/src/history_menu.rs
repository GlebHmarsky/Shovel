use super::*;
use crate::strings::{format, text};
use shovel_core::{
    git::{Commit, Reference, Snapshot},
    history_actions::Action,
};

#[derive(Clone)]
pub(super) struct HistoryMenu {
    pub owner: u64,
    pub snapshot: Snapshot,
    pub commit: Commit,
    pub stash: Option<Reference>,
}

pub(super) enum Command {
    Create(bool),
    Checkout,
    Reset,
    Integrate(Action, &'static str),
    CherryPick(usize),
    Apply(Action, &'static str),
    Rename(Reference),
    Pull,
    Push(Reference),
    ExportPatch(Option<String>),
    Compare,
}

fn button(ui: &mut egui::Ui, label: &str, enabled: bool) -> bool {
    ui.add_enabled(enabled, egui::Button::new(text(label)))
        .clicked()
}

fn relative_to_parent(
    ui: &mut egui::Ui,
    label: &str,
    commit: &Commit,
    enabled: bool,
) -> Option<usize> {
    if commit.parents.len() <= 1 {
        return button(ui, label, enabled).then_some(0);
    }
    let mut mainline = None;
    ui.add_enabled_ui(enabled, |ui| {
        ui.menu_button(text(label), |ui| {
            for (index, parent) in commit.parents.iter().enumerate() {
                if ui
                    .button(format(
                        "HistoryRevertParent",
                        &[&(index + 1).to_string(), parent],
                    ))
                    .clicked()
                {
                    mainline = Some(index + 1);
                }
            }
        });
    });
    mainline
}

impl HistoryMenu {
    pub(super) fn show(&self, ui: &mut egui::Ui, ready: bool) -> Option<Command> {
        let snapshot = &self.snapshot;
        let target = &self.commit.hash;
        let writable = ready && snapshot.operations.is_empty();
        let attached = snapshot.head.is_some() && snapshot.branch.is_some();
        let other = snapshot.head.as_ref() != Some(target);
        let mut choice = None;
        for reference in snapshot.references.iter().filter(|r| &r.hash == target) {
            let Some(branch) = reference.name.strip_prefix("refs/heads/") else {
                continue;
            };
            let active = snapshot.branch.as_deref() == Some(branch);
            ui.menu_button(branch, |ui| {
                if active {
                    if let Some(upstream) = reference.upstream.as_ref().and_then(|name| {
                        snapshot
                            .references
                            .iter()
                            .find(|r| &r.name == name && r.hash != *target)
                    }) {
                        let name = upstream
                            .name
                            .strip_prefix("refs/remotes/")
                            .unwrap_or(&upstream.name);
                        if ui
                            .add_enabled(
                                writable,
                                egui::Button::new(format("HistoryFastForwardTo", &[name])),
                            )
                            .clicked()
                        {
                            choice = Some(Command::Integrate(
                                Action::Merge {
                                    target: upstream.hash.clone(),
                                    fast_forward: true,
                                },
                                "HistoryFastForward",
                            ));
                        }
                    }
                    if button(
                        ui,
                        "RepositoryPull",
                        writable && !snapshot.remotes.is_empty(),
                    ) {
                        choice = Some(Command::Pull);
                    }
                }
                if button(
                    ui,
                    "RepositoryPush",
                    writable && !snapshot.remotes.is_empty(),
                ) {
                    choice = Some(Command::Push(reference.clone()));
                }
                if button(ui, "HistoryRenameBranch", writable) {
                    choice = Some(Command::Rename(reference.clone()));
                }
                if button(ui, "HistoryCopyBranchName", true) {
                    ui.ctx().copy_text(branch.to_owned());
                    ui.close();
                }
            });
        }
        ui.separator();
        if button(ui, "RepositoryNewBranch", writable) {
            choice = Some(Command::Create(true));
        }
        if button(ui, "RepositoryNewTag", writable) {
            choice = Some(Command::Create(false));
        }
        ui.separator();
        if button(ui, "HistoryMergeIntoCurrent", writable && attached && other) {
            choice = Some(Command::Integrate(
                Action::Merge {
                    target: target.clone(),
                    fast_forward: false,
                },
                "HistoryMerge",
            ));
        }
        if button(ui, "HistoryRebaseCurrent", writable && attached && other) {
            choice = Some(Command::Integrate(
                Action::Rebase {
                    target: target.clone(),
                },
                "HistoryRebase",
            ));
        }
        if button(ui, "HistoryResetMenu", writable && attached) {
            choice = Some(Command::Reset);
        }
        if button(ui, "HistoryMenuCheckoutCommit", writable) {
            choice = Some(Command::Checkout);
        }
        if let Some(mainline) = relative_to_parent(
            ui,
            "HistoryCherryPick",
            &self.commit,
            writable && snapshot.head.is_some(),
        ) {
            choice = Some(Command::CherryPick(mainline));
        }
        if let Some(mainline) = relative_to_parent(
            ui,
            "HistoryMenuRevertCommit",
            &self.commit,
            writable && snapshot.head.is_some(),
        ) {
            choice = Some(Command::Apply(
                Action::Revert {
                    target: target.clone(),
                    mainline,
                },
                "ActionTitleRevertCommit",
            ));
        }
        ui.separator();
        if let Some(parent) = relative_to_parent(ui, "HistoryExportPatch", &self.commit, ready) {
            choice = Some(Command::ExportPatch(
                self.commit.parents.get(parent.saturating_sub(1)).cloned(),
            ));
        }
        if button(ui, "HistoryCompareWorktree", writable) {
            choice = Some(Command::Compare);
        }
        ui.separator();
        if button(ui, "HistoryCopySha", true) {
            ui.ctx().copy_text(target.clone());
            ui.close();
        }
        if button(ui, "HistoryCopyInfo", true) {
            let author = history_row::author_details(ui.ctx(), &self.commit);
            let info = format!("{}\n{}\n\n{}", target, author, self.commit.message);
            ui.ctx().copy_text(info);
            ui.close();
        }
        if choice.is_some() {
            ui.close();
        }
        choice
    }
}

impl Workspace {
    pub(super) fn invoke_history_menu(
        &mut self,
        menu: HistoryMenu,
        command: Command,
        ctx: &egui::Context,
    ) {
        let Some(tab) = self
            .tabs
            .get_mut(self.selected)
            .filter(|tab| tab.id == menu.owner)
        else {
            return;
        };
        let Some(git) = self.git.clone() else {
            return;
        };
        let snapshot = menu.snapshot;
        let commit = menu.commit;
        match command {
            Command::Create(branch) => {
                self.repository_dialog = Some(repository_menu::Dialog::at_commit(
                    snapshot,
                    commit,
                    branch,
                ))
            }
            Command::Checkout => self.branches.checkout_commit(&snapshot, commit),
            Command::Reset => self.branches.reset(&snapshot, commit),
            Command::Integrate(action, title) => {
                self.branches.confirm_integration(&snapshot, action, commit, title)
            }
            Command::CherryPick(mainline) => self
                .branches
                .confirm_cherry_pick(&snapshot, commit, mainline),
            Command::Apply(action, title) => {
                self.operations
                    .submit_history(title, action, &snapshot, git, &self.scheduler, ctx)
            }
            Command::Rename(reference) => self.branches.rename(&snapshot, &reference),
            Command::Pull => self.operations.open_network_options(
                operations::Kind::Pull,
                &snapshot.worktree,
                Some(&snapshot),
                &self.settings,
                &self.scheduler,
                ctx,
            ),
            Command::Push(reference) => self.operations.open_branch_push(&snapshot, &reference),
            Command::ExportPatch(parent) => {
                self.repository_dialog = Some(repository_menu::Dialog::export_patch(
                    snapshot,
                    &commit.hash,
                    parent,
                    ctx,
                    &self.read_workers,
                ));
            }
            Command::Compare => {
                tab.selected = Some(commit.hash.clone());
                prime_comparison(tab, &git, &self.settings, &self.read_workers, ctx);
                if let Some(pane) = &mut tab.comparison {
                    pane.select_worktree(&commit, snapshot, ctx);
                }
            }
        }
    }
}
