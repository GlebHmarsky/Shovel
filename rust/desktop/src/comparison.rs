//! Selected immutable history comparison. Obsolete reads are cancellable and
//! generation-checked; useful prior files/diffs survive a failed new selection.
use crate::{
    diff_view,
    history_row::text as row_text,
    strings::{self, text},
    workspace_split::Divider,
    workspace_style::Palette,
};
use eframe::egui::{self, RichText};
use shovel_core::{
    diff::{CommitFiles, FileDiff},
    git::{Commit, Git},
    process::Cancellation,
    settings::Settings,
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
    pub(super) read_workers: crate::read_workers::Workers,
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
    pub view: diff_view::View,
}
impl Pane {
    pub fn with_read_workers(mut self, workers: crate::read_workers::Workers) -> Self {
        self.read_workers = workers;
        self
    }

    pub fn new(path: PathBuf, git: Arc<Git>) -> Self {
        let (sender, receiver) = mpsc::channel();
        Self {
            read_workers: Default::default(),
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
            view: Default::default(),
        }
    }
    pub fn select(&mut self, commit: &Commit, ctx: &egui::Context) {
        if self.read_workers.stopping() {
            return;
        }
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
        if self.read_workers.stopping() {
            return;
        }
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
    pub(super) fn load_files(&mut self, ctx: &egui::Context) {
        if self.read_workers.stopping() {
            return;
        }
        let Some(commit) = self.selected.clone() else {
            return;
        };
        let parent = commit.parents.get(self.parent).cloned();
        let (generation, cancel) = self.begin();
        let git = self.git.clone();
        let path = self.path.clone();
        let tx = self.sender.clone();
        let ctx = ctx.clone();
        let lease = self.read_workers.start(cancel.clone());
        thread::spawn(move || {
            let _lease = lease;
            #[cfg(test)]
            crate::history_cache::tests::wait_for_read(&path);
            let result = git.commit_files(&path, &commit.hash, parent.as_deref(), &cancel);
            let _ = tx.send(Event::Files(generation, result));
            ctx.request_repaint();
        });
    }
    fn load_diff(&mut self, path: &str, ctx: &egui::Context) {
        if self.read_workers.stopping() {
            return;
        }
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
        let lease = self.read_workers.start(cancel.clone());
        thread::spawn(move || {
            let _lease = lease;
            #[cfg(test)]
            crate::history_cache::tests::wait_for_read(&root);
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
        self.show_in(ui, None);
    }

    pub fn show_history(&mut self, ui: &mut egui::Ui, settings: &mut Settings) -> bool {
        self.show_in(ui, Some(settings))
    }

    fn show_in(&mut self, ui: &mut egui::Ui, settings: Option<&mut Settings>) -> bool {
        let ctx = ui.ctx().clone();
        self.poll(&ctx);
        ui.horizontal(|ui| {
            for (index, key) in [(0, "Commit"), (1, "ChangedFiles")] {
                if crate::workspace_style::lower_tab(ui, self.details_index == index, text(key))
                    .clicked()
                {
                    self.details_index = index;
                }
            }
            if settings.is_none()
                && ui
                    .add_enabled(!self.loading, egui::Button::new(text("CommitRefresh")))
                    .clicked()
            {
                self.load_files(&ctx);
            }
            if let Some(error) = &self.error {
                ui.add(
                    egui::Label::new(RichText::new(error).color(ui.visuals().error_fg_color))
                        .truncate(),
                )
                .on_hover_text(error);
            } else if self.loading {
                ui.add(
                    egui::Label::new(text(if self.diff.is_some() {
                        "DiffRetainedLoading"
                    } else {
                        "CommitFilesLoading"
                    }))
                    .truncate(),
                );
            }
        });
        let Some(commit) = self.selected.clone() else {
            ui.label(text("SelectCommit"));
            return false;
        };
        if self.details_index == 0 {
            if settings.is_some() {
                self.show_summary(ui, &commit);
                return false;
            }
            egui::ScrollArea::vertical()
                .id_salt("metadata")
                .max_height(125.)
                .show(ui, |ui| inspector_metadata(ui, &commit, &self.path));
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
            let span = ui.available_width();
            let panel = egui::Panel::left("files");
            let panel = if let Some(settings) = settings.as_deref() {
                panel
                    .frame(crate::workspace_style::pane(
                        Palette::current(ui).canvas,
                        6,
                        2,
                    ))
                    .exact_size(Divider::HistoryFiles.size(ui, settings, span))
                    .resizable(false)
            } else {
                panel
                    .default_size(340.)
                    .max_size((span - 200.).max(160.))
                    .min_size(160.)
                    .resizable(true)
            };
            let files = panel.show(ui, |ui| {
                if let Some(comparison) = &self.comparison {
                    if comparison.commit != commit.hash {
                        ui.label(text("DiffRetainedLoading"));
                    }
                    ui.scope(|ui| {
                        ui.spacing_mut().item_spacing.y = 0.;
                        egui::ScrollArea::vertical()
                            .id_salt("comparison_files")
                            .auto_shrink([false, false])
                            .show_rows(
                                ui,
                                FILE_ROW_HEIGHT,
                                comparison.entries.len(),
                                |ui, range| {
                                    let mut chosen = None;
                                    for entry in &comparison.entries[range] {
                                        let selected =
                                            self.selected_file.as_deref() == Some(&entry.path);
                                        let kind = RowKind::Diff {
                                            selected,
                                            enabled: !self.loading,
                                        };
                                        if file_row(ui, entry, kind) {
                                            chosen = Some(entry.path.clone());
                                        }
                                    }
                                    chosen
                                },
                            )
                            .inner
                    })
                    .inner
                } else {
                    None
                }
            });
            if let Some(path) = files.inner {
                self.load_diff(&path, &ctx);
            }
            let resized = settings.is_some_and(|settings| {
                Divider::HistoryFiles.handle(ui, files.response.rect, span, settings)
            });
            egui::CentralPanel::default().show(ui, |ui| {
                if let Some(diff) = &self.diff {
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::Label::new(RichText::new(&self.displayed_file).strong())
                                .truncate(),
                        )
                        .on_hover_text(&self.displayed_file);
                        ui.monospace(&self.displayed_commit[..self.displayed_commit.len().min(8)]);
                    });
                    self.view.show(ui, diff, diff_view::HunkActions::ReadOnly);
                } else {
                    ui.label(text("DiffChooseFile"));
                }
            });
            resized
        })
        .inner
    }

    fn show_summary(&self, ui: &mut egui::Ui, commit: &Commit) {
        egui::ScrollArea::vertical()
            .id_salt("commit_summary")
            .auto_shrink([false, false])
            .show_viewport(ui, |ui, viewport| {
                let top = ui.next_widget_position().y;
                metadata(ui, commit, &self.path);
                let Some(comparison) = self.comparison.as_ref().filter(|c| c.commit == commit.hash)
                else {
                    return;
                };
                if comparison.entries.is_empty() {
                    return;
                }
                ui.separator();
                ui.scope(|ui| {
                    ui.spacing_mut().item_spacing.y = 0.;
                    let file_top = ui.next_widget_position().y - top;
                    let len = comparison.entries.len();
                    let start =
                        (((viewport.min.y - file_top) / FILE_ROW_HEIGHT).floor() as usize).min(len);
                    let end = (((viewport.max.y - file_top) / FILE_ROW_HEIGHT).ceil() as usize)
                        .clamp(start, len);
                    ui.add_space(start as f32 * FILE_ROW_HEIGHT);
                    for entry in &comparison.entries[start..end] {
                        file_row(ui, entry, RowKind::Summary);
                    }
                    ui.add_space((len - end) as f32 * FILE_ROW_HEIGHT);
                });
            });
    }
}

const FILE_ROW_HEIGHT: f32 = 22.;

#[derive(Clone, Copy)]
enum RowKind {
    Summary,
    Diff { selected: bool, enabled: bool },
}

fn file_row(ui: &mut egui::Ui, entry: &working::Change, kind: RowKind) -> bool {
    let palette = Palette::current(ui);
    let (selected, enabled) = match kind {
        RowKind::Summary => (false, false),
        RowKind::Diff { selected, enabled } => (selected, enabled && ui.is_enabled()),
    };
    let sense = if enabled {
        egui::Sense::click()
    } else {
        egui::Sense::hover()
    };
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), FILE_ROW_HEIGHT), sense);
    let path = crate::path_text::PathText::new(ui.ctx(), &entry.path);
    let original = entry
        .original
        .as_ref()
        .map(|p| crate::path_text::PathText::new(ui.ctx(), p));
    let label = format!("{} {}", entry.code, path.spoken());
    response.widget_info(|| match kind {
        RowKind::Summary => egui::WidgetInfo::labeled(egui::Role::Label, true, &label),
        RowKind::Diff { .. } => {
            egui::WidgetInfo::selected(egui::Role::Button, enabled, selected, &label)
        }
    });
    let tooltip = original.map_or_else(
        || path.visual().to_owned(),
        |old| strings::format("FileActionPath", &[path.visual(), old.visual()]),
    );
    let highlighted = selected || enabled && response.hovered();
    let color = if highlighted {
        let fill = if selected {
            palette.selected
        } else {
            palette.hover
        };
        ui.painter().rect_filled(rect, 0., fill);
        palette.selection_text
    } else {
        palette.text
    };
    if response.has_focus() {
        ui.painter().rect_stroke(
            rect,
            0.,
            egui::Stroke::new(1., palette.focus),
            egui::StrokeKind::Inside,
        );
    }
    let font = egui::FontId::proportional(13.);
    let x = rect.left() + 6.;
    let mut path_rect = rect;
    if matches!(kind, RowKind::Diff { .. }) {
        let mut badge = rect;
        badge.min.x = x;
        badge.max.x = x + 18.;
        row_text(
            ui,
            badge,
            entry.code.encode_utf8(&mut [0; 4]),
            font.clone(),
            if palette.high_contrast {
                color
            } else {
                Palette::change_status(entry.code, ui.visuals().dark_mode || highlighted)
            },
        );
        path_rect.min.x = badge.max.x;
    } else {
        path_rect.min.x = x;
    }
    path_rect.max.x = (path_rect.max.x - 6.).max(path_rect.min.x);
    row_text(ui, path_rect, path.visual(), font, color);
    response.on_hover_text(tooltip).clicked()
}

impl Drop for Pane {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}
fn error_text(error: working::Error) -> String {
    crate::operations::error_log(&error.into())
}

pub(super) fn metadata(ui: &mut egui::Ui, commit: &Commit, worktree: &std::path::Path) {
    let palette = Palette::current(ui);
    ui.horizontal(|ui| {
        let (avatar, response) = ui.allocate_exact_size(egui::vec2(28., 28.), egui::Sense::hover());
        crate::history_row::paint_avatar(ui, avatar, commit, &palette, worktree);
        response.on_hover_text(crate::history_row::author_details(ui.ctx(), commit));
        ui.strong(&commit.author)
            .on_hover_text(crate::history_row::author_details(ui.ctx(), commit));
        ui.label(
            RichText::new(crate::dates::local(ui.ctx(), &commit.committed)).color(palette.muted),
        )
        .on_hover_text(crate::history_row::committer_details(ui.ctx(), commit));
    });
    let (subject, body) = commit
        .message
        .split_once('\n')
        .unwrap_or((&commit.message, ""));
    ui.add(egui::Label::new(RichText::new(subject).strong()).selectable(true));
    let body = body.trim_start_matches(['\r', '\n']).trim_end();
    if !body.is_empty() {
        ui.add(egui::Label::new(body).selectable(true));
    }
}

fn inspector_metadata(ui: &mut egui::Ui, commit: &Commit, worktree: &std::path::Path) {
    let ctx = ui.ctx().clone();
    let p = crate::workspace_style::Palette::current(ui);
    ui.horizontal_top(|ui| {
        let (avatar, avatar_response) =
            ui.allocate_exact_size(egui::vec2(44., 44.), egui::Sense::hover());
        crate::history_row::paint_avatar(ui, avatar, commit, &p, worktree);
        avatar_response.on_hover_text(format!("{} <{}>", commit.author, commit.email));
        ui.vertical(|ui| {
            ui.horizontal_wrapped(|ui| {
                ui.strong(&commit.author);
                ui.label(
                    RichText::new(crate::dates::local(ui.ctx(), &commit.committed)).color(p.muted),
                )
                .on_hover_text(commit.committed.to_rfc3339());
            });
            ui.horizontal_wrapped(|ui| {
                ui.monospace(&commit.hash);
                if ui.button(text("HistoryCopySha")).clicked() {
                    ctx.copy_text(commit.hash.clone());
                }
            });
            ui.add(egui::Label::new(RichText::new(&commit.message).strong()).selectable(true));
            egui::CollapsingHeader::new(text("CommitMetadata")).show(ui, |ui| {
                ui.label(crate::history_row::author_details(ui.ctx(), commit))
                    .on_hover_text(commit.authored.to_rfc3339());
                ui.label(crate::history_row::committer_details(ui.ctx(), commit))
                    .on_hover_text(commit.committed.to_rfc3339());
                let parents = if commit.parents.is_empty() {
                    text("RootCommit")
                } else {
                    commit.parents.join(", ")
                };
                ui.label(crate::strings::format(
                    "RustCommitParentDetails",
                    &[&parents],
                ));
            });
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use shovel_core::process::{self, Lifetime};
    use std::time::{Duration, Instant};
    #[test]
    fn comparison_and_history_read_errors_bound_and_redact_git_output() {
        let failure = || shovel_core::git::Error::Command {
            outcome: shovel_core::process::Outcome::Exited,
            code: 128,
            output: format!(
                "fatal HTTPS://fixture:password@host/path?private\n{}",
                "ש".repeat(40000)
            ),
        };
        for (message, key) in [
            (error_text(failure().into()), "ActionReadFailed"),
            (crate::error_text(failure()), "HistoryFailed"),
        ] {
            let (status, detail) = message.split_once('\n').unwrap();
            assert_eq!(status, text(key));
            assert!(detail.starts_with("fatal HTTPS://[redacted]@host/path?[redacted]\n"));
            assert_eq!(detail.chars().count(), 32768);
        }
    }
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
        let other_file = "z-second-longer-file.txt";
        std::fs::write(root.join(other_file), "second file diff\n").unwrap();
        command(vec!["add", "--", "file.txt", other_file]);
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
        ctx.enable_accesskit();
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
        let settings = std::cell::RefCell::new(Settings::default());
        let frame = |pane: &mut Pane, size: [f32; 2], events| {
            ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size.into())),
                    events,
                    ..Default::default()
                },
                |ui| {
                    pane.show_history(ui, &mut settings.borrow_mut());
                },
            )
        };
        let click = |pane: &mut Pane, label: &str| {
            let size = [1200., 500.];
            let output = frame(pane, size, vec![]);
            let bounds = output
                .platform_output
                .accesskit_update
                .as_ref()
                .unwrap()
                .nodes
                .iter()
                .find_map(|(_, node)| {
                    (node.label() == Some(label))
                        .then(|| node.bounds())
                        .flatten()
                })
                .unwrap_or_else(|| panic!("missing clickable {label}"));
            output.drop_without_applying_deltas();
            let pos = egui::pos2(
                (bounds.x0 + bounds.x1) as f32 / 2.,
                (bounds.y0 + bounds.y1) as f32 / 2.,
            );
            frame(pane, size, vec![egui::Event::PointerMoved(pos)]).drop_without_applying_deltas();
            for pressed in [true, false] {
                frame(
                    pane,
                    size,
                    vec![egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    }],
                )
                .drop_without_applying_deltas();
            }
        };
        let output = frame(&mut pane, [1200., 500.], vec![]);
        let paints = |label| {
            output.shapes.iter().any(|shape| {
                matches!(&shape.shape, egui::Shape::Text(text) if text.galley.text() == label)
            })
        };
        for label in ["initial", "full message", "file.txt", other_file] {
            assert!(paints(label), "{label}");
        }
        for label in ["immutable text", "Path", "Refresh"] {
            assert!(!paints(label), "{label}");
        }
        output.drop_without_applying_deltas();

        click(&mut pane, "Changed files");
        assert_eq!(pane.details_index, 1);
        for size in [[1200., 500.], [760., 320.]] {
            let output = frame(&mut pane, size, vec![]);
            let text_left = |label| {
                output
                    .shapes
                    .iter()
                    .filter_map(|shape| match &shape.shape {
                        egui::Shape::Text(text) if text.galley.text() == label => Some(text.pos.x),
                        _ => None,
                    })
                    .reduce(f32::min)
                    .unwrap()
            };
            let path_column = text_left("file.txt");
            assert_eq!(text_left(other_file), path_column);
            assert!(text_left("A") < path_column);
            output.drop_without_applying_deltas();
        }
        let size = [1200., 500.];
        let start = egui::pos2(337.5, 200.);
        let end = start + egui::vec2(70., 0.);
        let button = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        for event in [
            egui::Event::PointerMoved(start),
            button(start, true),
            egui::Event::PointerMoved(end),
            button(end, false),
        ] {
            frame(&mut pane, size, vec![event]).drop_without_applying_deltas();
        }
        let width = settings
            .borrow()
            .get("HistoryFilesWidth")
            .and_then(serde_json::Value::as_f64);
        assert!(
            width.is_some_and(|width| (405. ..415.).contains(&width)),
            "{width:?}"
        );
        let directory = temp.path().join("profile");
        let mut store = shovel_core::settings::SettingsStore::new(directory.clone());
        store.load().unwrap();
        store.save(&settings.borrow()).unwrap();
        let mut restored = shovel_core::settings::SettingsStore::new(directory)
            .load()
            .unwrap();
        assert_eq!(
            restored
                .get("HistoryFilesWidth")
                .and_then(serde_json::Value::as_f64),
            width
        );
        let reopened = egui::Context::default().run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size.into())),
                ..Default::default()
            },
            |ui| assert!(!pane.show_history(ui, &mut restored)),
        );
        let divider_x = width.unwrap() as f32;
        assert!(reopened.shapes.iter().any(|shape| {
            matches!(
                &shape.shape,
                egui::Shape::Text(text)
                  if text.galley.text() == "file.txt" && text.pos.x >= divider_x
            )
        }));
        reopened.drop_without_applying_deltas();

        click(&mut pane, &format!("A {other_file}"));
        settle(&mut pane);
        assert_eq!(pane.displayed_file, other_file);
        let mut missing = commit;
        missing.hash = "a".repeat(40);
        pane.select(&missing, &ctx);
        settle(&mut pane);
        assert!(pane.error.is_some());
        assert_eq!(pane.displayed_commit, hash);
        assert_eq!(pane.displayed_file, other_file);
        assert!(
            pane.diff
                .as_ref()
                .unwrap()
                .lines
                .iter()
                .any(|l| l.text == "second file diff")
        );
    }
}
