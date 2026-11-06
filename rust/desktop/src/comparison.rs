//! Selected immutable history comparison. Obsolete reads are cancellable and
//! generation-checked; useful prior files/diffs survive a failed new selection.
use crate::{
    diff_view,
    strings::{self, text},
};
use eframe::egui::{self, RichText};
use shovel_core::{
    diff::{CommitFiles, FileDiff},
    git::{Commit, Git},
    process::Cancellation,
    working,
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
    Files(u64, Result<CommitFiles, working::Error>),
    Diff(u64, String, String, Result<FileDiff, working::Error>),
}
pub struct Pane {
    path: PathBuf,
    git: Arc<Git>,
    selected: Option<Commit>,
    parent: usize,
    comparison: Option<Arc<CommitFiles>>,
    selected_file: Option<String>,
    diff: Option<FileDiff>,
    displayed_commit: String,
    displayed_file: String,
    generation: u64,
    cancel: Cancellation,
    sender: Sender<Event>,
    receiver: Receiver<Event>,
    loading: bool,
    error: Option<String>,
    pub details_index: usize,
    pub files_width: f32,
    pub view: diff_view::View,
}
impl Pane {
    pub fn new(path: PathBuf, git: Arc<Git>) -> Self {
        let (sender, receiver) = mpsc::channel();
        Self {
            path,
            git,
            selected: None,
            parent: 0,
            comparison: None,
            selected_file: None,
            diff: None,
            displayed_commit: String::new(),
            displayed_file: String::new(),
            generation: 0,
            cancel: Cancellation::default(),
            sender,
            receiver,
            loading: false,
            error: None,
            details_index: 0,
            files_width: 340.,
            view: Default::default(),
        }
    }
    pub fn select(&mut self, commit: &Commit, ctx: &egui::Context) {
        if self
            .selected
            .as_ref()
            .is_some_and(|old| old.hash == commit.hash && old.parents == commit.parents)
        {
            return;
        }
        self.selected = Some(commit.clone());
        self.parent = 0;
        self.load_files(ctx);
    }
    pub fn select_file(&mut self, commit: &Commit, path: &str, ctx: &egui::Context) {
        let same = self
            .selected
            .as_ref()
            .is_some_and(|old| old.hash == commit.hash && old.parents == commit.parents);
        let changed = self.selected_file.as_deref() != Some(path);
        self.selected_file = Some(path.to_owned());
        if same && (changed || self.error.is_some()) {
            self.load_files(ctx);
        } else {
            self.select(commit, ctx);
        }
    }
    pub fn cancel(&mut self) {
        self.cancel.cancel();
        self.generation += 1;
        if self.loading {
            self.error = Some(text("ActionResultCancelled"));
        }
        self.loading = false;
    }
    pub fn is_loading(&self) -> bool {
        self.loading
    }
    fn begin(&mut self) -> (u64, Cancellation) {
        self.cancel.cancel();
        self.cancel = Cancellation::default();
        self.generation += 1;
        self.loading = true;
        self.error = None;
        (self.generation, self.cancel.clone())
    }
    fn load_files(&mut self, ctx: &egui::Context) {
        let Some(commit) = self.selected.clone() else {
            return;
        };
        let parent = commit.parents.get(self.parent).cloned();
        let (generation, cancel) = self.begin();
        let git = self.git.clone();
        let path = self.path.clone();
        let tx = self.sender.clone();
        let ctx = ctx.clone();
        thread::spawn(move || {
            let result = git.commit_files(&path, &commit.hash, parent.as_deref(), &cancel);
            let _ = tx.send(Event::Files(generation, result));
            ctx.request_repaint();
        });
    }
    fn load_diff(&mut self, path: &str, ctx: &egui::Context) {
        let Some(comparison) = self.comparison.clone() else {
            return;
        };
        let Some(entry) = comparison.entries.iter().find(|e| e.path == path).cloned() else {
            return;
        };
        self.selected_file = Some(entry.path.clone());
        let (generation, cancel) = self.begin();
        let git = self.git.clone();
        let root = self.path.clone();
        let tx = self.sender.clone();
        let ctx = ctx.clone();
        thread::spawn(move || {
            let result = git.commit_file_diff(&root, &comparison, &entry, &cancel);
            let _ = tx.send(Event::Diff(
                generation,
                comparison.commit.clone(),
                entry.path,
                result,
            ));
            ctx.request_repaint();
        });
    }
    pub fn poll(&mut self, ctx: &egui::Context) {
        while let Ok(event) = self.receiver.try_recv() {
            match event {
                Event::Files(generation, result) if generation == self.generation => {
                    self.loading = false;
                    match result {
                        Ok(comparison) => {
                            let selected = self
                                .selected_file
                                .as_ref()
                                .and_then(|path| {
                                    comparison.entries.iter().find(|e| {
                                        &e.path == path || e.original.as_ref() == Some(path)
                                    })
                                })
                                .or_else(|| comparison.entries.first())
                                .map(|e| e.path.clone());
                            self.comparison = Some(Arc::new(comparison));
                            if let Some(path) = selected {
                                self.load_diff(&path, ctx);
                            } else {
                                self.diff = None;
                                self.selected_file = None;
                                self.displayed_file.clear();
                            }
                        }
                        Err(error) => self.error = Some(error_text(error)),
                    }
                }
                Event::Diff(generation, commit, path, result) if generation == self.generation => {
                    self.loading = false;
                    match result {
                        Ok(diff) => {
                            self.diff = Some(diff);
                            self.displayed_commit = commit;
                            self.displayed_file = path;
                            self.view.reset();
                        }
                        Err(error) => self.error = Some(error_text(error)),
                    }
                }
                _ => {}
            }
        }
    }
    pub fn show(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        self.poll(&ctx);
        ui.horizontal(|ui| {
            for (index, key) in [(0, "Commit"), (1, "CommitFilesTitle")] {
                if crate::workspace_style::lower_tab(ui, self.details_index == index, text(key))
                    .clicked()
                {
                    self.details_index = index;
                }
            }
            if ui
                .add_enabled(!self.loading, egui::Button::new(text("CommitRefresh")))
                .clicked()
            {
                self.load_files(&ctx);
            }
            if let Some(error) = &self.error {
                ui.colored_label(ui.visuals().error_fg_color, error);
            } else if self.loading {
                ui.label(text(if self.diff.is_some() {
                    "DiffRetainedLoading"
                } else {
                    "CommitFilesLoading"
                }));
            }
        });
        let Some(commit) = self.selected.clone() else {
            ui.label(text("SelectCommit"));
            return;
        };
        if self.details_index == 0 {
            let p = crate::workspace_style::Palette::current(ui);
            egui::ScrollArea::vertical()
                .id_salt("metadata")
                .max_height(125.)
                .show(ui, |ui| {
                    ui.horizontal_top(|ui| {
                        let (avatar, avatar_response) =
                            ui.allocate_exact_size(egui::vec2(44., 44.), egui::Sense::hover());
                        crate::history_row::paint_avatar(ui, avatar, &commit, &p);
                        avatar_response
                            .on_hover_text(format!("{} <{}>", commit.author, commit.email));
                        ui.vertical(|ui| {
                            ui.horizontal_wrapped(|ui| {
                                ui.strong(&commit.author);
                                ui.label(
                                    RichText::new(
                                        commit.authored.format("%Y-%m-%d %H:%M %:z").to_string(),
                                    )
                                    .color(p.muted),
                                );
                            });
                            ui.horizontal_wrapped(|ui| {
                                ui.monospace(&commit.hash);
                                if ui.button(text("HistoryCopySha")).clicked() {
                                    ctx.copy_text(commit.hash.clone());
                                }
                            });
                            ui.add(
                                egui::Label::new(RichText::new(&commit.message).strong())
                                    .selectable(true),
                            );
                            egui::CollapsingHeader::new(text("CommitMetadata")).show(ui, |ui| {
                                ui.label(format!(
                                    "{}: {} <{}> · {}",
                                    text("HistoryAuthor"),
                                    commit.author,
                                    commit.email,
                                    commit.authored.to_rfc3339()
                                ));
                                ui.label(format!(
                                    "{}: {} <{}> · {}",
                                    text("Committer"),
                                    commit.committer,
                                    commit.committer_email,
                                    commit.committed.to_rfc3339()
                                ));
                                ui.label(format!(
                                    "{}: {}",
                                    text("CommitParents"),
                                    if commit.parents.is_empty() {
                                        text("RootCommit")
                                    } else {
                                        commit.parents.join(", ")
                                    }
                                ));
                            });
                        });
                    });
                });
            ui.separator();
        }
        ui.horizontal(|ui| {
            ui.label(text("CommitFilesCompare"));
            let old = self.parent;
            egui::ComboBox::from_id_salt("parent")
                .selected_text(
                    commit
                        .parents
                        .get(self.parent)
                        .map(|p| {
                            strings::format(
                                "CommitFilesParent",
                                &[&(self.parent + 1).to_string(), p],
                            )
                        })
                        .unwrap_or_else(|| text("CommitFilesRoot")),
                )
                .show_ui(ui, |ui| {
                    for (index, parent) in commit.parents.iter().enumerate() {
                        ui.selectable_value(
                            &mut self.parent,
                            index,
                            strings::format(
                                "CommitFilesParent",
                                &[&(index + 1).to_string(), parent],
                            ),
                        );
                    }
                });
            if old != self.parent {
                self.load_files(&ctx);
            }
        });
        ui.push_id("history_comparison", |ui| {
            let files = egui::Panel::left("files")
                .default_size(self.files_width)
                .max_size((ui.available_width() - 200.).max(160.))
                .min_size(160.)
                .resizable(true)
                .show(ui, |ui| {
                    let mut chosen = None;
                    if let Some(comparison) = &self.comparison {
                        if comparison.commit != commit.hash {
                            ui.label(text("DiffRetainedLoading"));
                        }
                        egui::ScrollArea::both().show_rows(
                            ui,
                            22.,
                            comparison.entries.len(),
                            |ui, range| {
                                for index in range {
                                    let entry = &comparison.entries[index];
                                    if ui
                                        .selectable_label(
                                            self.selected_file.as_deref() == Some(&entry.path),
                                            format!("{}  {}", entry.code, entry.path),
                                        )
                                        .clicked()
                                        && !self.loading
                                    {
                                        chosen = Some(entry.path.clone());
                                    }
                                }
                            },
                        );
                    }
                    if let Some(path) = chosen {
                        self.load_diff(&path, &ctx);
                    }
                });
            if ctx.input(|i| i.pointer.any_released()) {
                self.files_width = files.response.rect.width();
            }
            egui::CentralPanel::default().show(ui, |ui| {
                if let Some(diff) = &self.diff {
                    ui.horizontal_wrapped(|ui| {
                        ui.strong(&self.displayed_file);
                        ui.monospace(&self.displayed_commit[..self.displayed_commit.len().min(8)]);
                    });
                    self.view.show(ui, diff, false);
                } else {
                    ui.label(text("DiffChooseFile"));
                }
            });
        });
    }
}
impl Drop for Pane {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}
fn error_text(error: working::Error) -> String {
    match error {
        working::Error::Git(shovel_core::git::Error::Command { output, .. }) => {
            format!("{}\n{output}", text("ActionReadFailed"))
        }
        other => text(&other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shovel_core::process::{self, Lifetime};
    use std::time::{Duration, Instant};
    #[test]
    fn comparison_renders_and_retains_exact_old_result_when_new_read_fails() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("repo");
        std::fs::create_dir(&root).unwrap();
        let mut git = Git::discover(None, &Cancellation::default()).unwrap();
        git.environment
            .insert("GIT_CONFIG_NOSYSTEM".into(), Some("1".into()));
        git.environment
            .insert("GIT_CONFIG_GLOBAL".into(), Some("NUL".into()));
        let command = |args: Vec<&str>| {
            let result = process::run(
                &git.request(&root, args, Lifetime::AcceptedCommand, 1024 * 1024),
                &Cancellation::default(),
                None,
            )
            .unwrap();
            assert!(result.success(), "{result:?}");
            String::from_utf8(result.stdout).unwrap()
        };
        command(vec!["init", "--initial-branch=main"]);
        std::fs::write(root.join("file.txt"), "immutable text\n").unwrap();
        command(vec!["add", "--", "file.txt"]);
        command(vec![
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-m",
            "initial",
        ]);
        let hash = command(vec!["rev-parse", "HEAD"]).trim().to_owned();
        let commit = Commit {
            hash: hash.clone(),
            parents: vec![],
            author: "Fixture".into(),
            email: "fixture@example.invalid".into(),
            authored: "2026-11-05T12:00:00+02:00".parse().unwrap(),
            committer: "Fixture".into(),
            committer_email: "fixture@example.invalid".into(),
            committed: "2026-11-05T12:00:00+02:00".parse().unwrap(),
            message: "initial\n\nfull message".into(),
        };
        let ctx = egui::Context::default();
        let mut pane = Pane::new(root, Arc::new(git));
        pane.select(&commit, &ctx);
        let settle = |pane: &mut Pane| {
            let start = Instant::now();
            while pane.loading {
                pane.poll(&ctx);
                assert!(start.elapsed() < Duration::from_secs(20));
                thread::sleep(Duration::from_millis(10));
            }
        };
        settle(&mut pane);
        assert!(pane.error.is_none(), "{:?}", pane.error);
        assert!(
            pane.diff
                .as_ref()
                .unwrap()
                .lines
                .iter()
                .any(|l| l.text == "immutable text")
        );
        for size in [[1200., 500.], [760., 320.]] {
            ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size.into())),
                    ..Default::default()
                },
                |ui| pane.show(ui),
            )
            .drop_without_applying_deltas();
        }
        let mut missing = commit;
        missing.hash = "a".repeat(40);
        pane.select(&missing, &ctx);
        settle(&mut pane);
        assert!(pane.error.is_some());
        assert_eq!(pane.displayed_commit, hash);
        assert_eq!(pane.displayed_file, "file.txt");
        assert!(
            pane.diff
                .as_ref()
                .unwrap()
                .lines
                .iter()
                .any(|l| l.text == "immutable text")
        );
    }
}
