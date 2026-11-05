//! Captured-tip History/Blame window. Construction is preparation-free; the
//! first visible layout starts cancellable reads with retained prior results.
use crate::{
    comparison,
    strings::{self, text},
};
use eframe::egui;
use shovel_core::{
    file_history::{BlameLine, Cursor, Page},
    git::Git,
    operation::Error,
    process::{Cancellation, Outcome},
    working::{Change, Context},
};
use std::{
    collections::BTreeMap,
    sync::{
        Arc,
        mpsc::{self, Receiver, Sender},
    },
    thread,
};
#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    History,
    Blame,
}
enum ResultData {
    History(Page),
    Blame(Vec<BlameLine>),
}
struct Event {
    generation: u64,
    mode: Mode,
    result: Result<ResultData, Error>,
}
pub struct Inspector {
    pub open: bool,
    context: Arc<Context>,
    entry: Change,
    git: Arc<Git>,
    id: egui::ViewportId,
    mode: Mode,
    initialized: bool,
    generation: u64,
    cancel: Cancellation,
    sender: Sender<Event>,
    receiver: Receiver<Event>,
    pub(super) busy: bool,
    pub(super) status: String,
    output: String,
    pub(super) page: Option<Page>,
    pub(super) blame: Vec<BlameLine>,
    cursors: BTreeMap<usize, Option<Cursor>>,
    selected: Option<String>,
    comparison: comparison::Pane,
    size: egui::Vec2,
}
impl Inspector {
    pub fn new(context: Arc<Context>, entry: Change, git: Arc<Git>, blame: bool) -> Self {
        let (sender, receiver) = mpsc::channel();
        let comparison = comparison::Pane::new(context.repository.worktree.clone(), git.clone());
        Self {
            context,
            entry,
            git,
            comparison,
            open: true,
            id: egui::ViewportId::from_hash_of(uuid::Uuid::new_v4()),
            mode: if blame { Mode::Blame } else { Mode::History },
            initialized: false,
            generation: 0,
            cancel: Cancellation::default(),
            sender,
            receiver,
            busy: false,
            status: text("FileHistoryLoading"),
            output: String::new(),
            page: None,
            blame: vec![],
            cursors: BTreeMap::from([(0, None)]),
            selected: None,
            size: egui::vec2(1200., 800.),
        }
    }
    fn offset(&self) -> usize {
        self.page.as_ref().map_or(0, |p| p.offset)
    }
    fn load(&mut self, offset: usize, ctx: &egui::Context) {
        if !self.open {
            return;
        }
        let Some(cursor) = self.cursors.get(&offset).cloned() else {
            return;
        };
        self.cancel.cancel();
        self.cancel = Cancellation::default();
        self.generation += 1;
        self.busy = true;
        self.status = text("FileHistoryLoading");
        self.output.clear();
        let (generation, mode, cancel, git, context, entry, tx, ctx) = (
            self.generation,
            self.mode,
            self.cancel.clone(),
            self.git.clone(),
            self.context.clone(),
            self.entry.clone(),
            self.sender.clone(),
            ctx.clone(),
        );
        thread::spawn(move || {
            let result = match mode {
                Mode::History => git
                    .file_history(&context, &entry, cursor.as_ref(), &cancel)
                    .map(ResultData::History),
                Mode::Blame => git
                    .file_blame(&context, &entry, &cancel)
                    .map(ResultData::Blame),
            };
            let _ = tx.send(Event {
                generation,
                mode,
                result,
            });
            ctx.request_repaint();
        });
    }
    fn select(&mut self, index: usize, ctx: &egui::Context) {
        let Some(entry) = self.page.as_ref().and_then(|p| p.entries.get(index)) else {
            return;
        };
        self.selected = Some(entry.commit.hash.clone());
        self.comparison.select_file(&entry.commit, &entry.path, ctx);
    }
    pub fn poll(&mut self, ctx: &egui::Context) {
        self.comparison.poll(ctx);
        while let Ok(event) = self.receiver.try_recv() {
            if !self.open
                || event.generation != self.generation
                || event.mode != self.mode
                || self.cancel.is_cancelled()
            {
                continue;
            }
            self.busy = false;
            match event.result {
                Ok(ResultData::History(page)) => {
                    if let Some(next) = &page.next {
                        self.cursors.insert(next.offset, Some(next.clone()));
                    }
                    while self.cursors.len() > 129 {
                        if let Some(key) = self
                            .cursors
                            .keys()
                            .copied()
                            .find(|k| *k != 0 && *k != page.offset)
                        {
                            self.cursors.remove(&key);
                        } else {
                            break;
                        }
                    }
                    let selected = page
                        .entries
                        .iter()
                        .position(|e| Some(&e.commit.hash) == self.selected.as_ref())
                        .unwrap_or(0);
                    self.status = text(if page.entries.is_empty() {
                        "FileHistoryEmpty"
                    } else if page.next.is_some() {
                        "FileHistoryMore"
                    } else {
                        "FileHistoryEnd"
                    });
                    if page.entries.is_empty() {
                        self.selected = None;
                        self.comparison = comparison::Pane::new(
                            self.context.repository.worktree.clone(),
                            self.git.clone(),
                        );
                    }
                    self.page = Some(page);
                    self.select(selected, ctx);
                }
                Ok(ResultData::Blame(lines)) => {
                    self.status = strings::format("FileBlameCount", &[&lines.len().to_string()]);
                    self.blame = lines;
                }
                Err(error) => {
                    self.status = text(error_code(&error));
                    if let Error::Git(shovel_core::git::Error::Command { output, .. }) = error {
                        self.output = shovel_core::network::redact(&output);
                    }
                }
            }
        }
    }
    fn cancel_reads(&mut self) {
        self.cancel.cancel();
        self.generation += 1;
        self.busy = false;
        self.comparison.cancel();
        self.status = text("ActionResultCancelled");
    }
    pub fn close(&mut self) {
        self.open = false;
        self.cancel_reads();
    }
    fn history(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let mut chosen = None;
        egui::Panel::top("file_history_rows")
            .default_size(250.)
            .min_size(100.)
            .resizable(true)
            .show(ui, |ui| {
                let width = (ui.available_width() - 430.).max(100.);
                ui.horizontal(|ui| {
                    for (key, w) in [
                        ("HistoryDescription", width),
                        ("HistoryAuthor", 145.),
                        ("HistoryDate", 170.),
                        ("HistoryHash", 85.),
                    ] {
                        ui.add_sized([w, 22.], egui::Label::new(text(key)));
                    }
                });
                if let Some(page) = &self.page {
                    egui::ScrollArea::both().show_rows(ui, 22., page.entries.len(), |ui, range| {
                        for i in range {
                            let entry = &page.entries[i];
                            let commit = &entry.commit;
                            let row = ui.horizontal(|ui| {
                                let selected = self.selected.as_ref() == Some(&commit.hash);
                                if ui
                                    .add_sized(
                                        [width, 22.],
                                        egui::Button::selectable(selected, commit.subject()),
                                    )
                                    .on_hover_text(&commit.message)
                                    .clicked()
                                {
                                    chosen = Some(i);
                                }
                                ui.add_sized(
                                    [145., 22.],
                                    egui::Label::new(&commit.author).truncate(),
                                )
                                .on_hover_text(&commit.email);
                                ui.add_sized(
                                    [170., 22.],
                                    egui::Label::new(commit.committed.to_rfc3339()).truncate(),
                                )
                                .on_hover_text(commit.committed.to_rfc3339());
                                ui.add_sized([85., 22.], egui::Label::new(&commit.hash[..8]))
                                    .on_hover_text(&commit.hash);
                            });
                            if row.response.interact(egui::Sense::click()).clicked() {
                                chosen = Some(i);
                            }
                        }
                    });
                }
            });
        egui::Panel::bottom("file_history_paging").show(ui, |ui| {
            ui.horizontal(|ui| {
                let offset = self.offset();
                let count = self.page.as_ref().map_or(0, |p| p.entries.len());
                ui.label(strings::format(
                    "FileHistoryPage",
                    &[
                        &(offset + usize::from(count > 0)).to_string(),
                        &(offset + count).to_string(),
                    ],
                ));
                if ui
                    .add_enabled(
                        !self.busy && offset > 0,
                        egui::Button::new(text("FileHistoryFirst")),
                    )
                    .clicked()
                {
                    self.load(0, &ctx);
                }
                if ui
                    .add_enabled(
                        !self.busy && offset >= 200 && self.cursors.contains_key(&(offset - 200)),
                        egui::Button::new(text("FileHistoryNewer")),
                    )
                    .clicked()
                {
                    self.load(offset - 200, &ctx);
                }
                if ui
                    .add_enabled(
                        !self.busy && self.page.as_ref().is_some_and(|p| p.next.is_some()),
                        egui::Button::new(text("FileHistoryOlder")),
                    )
                    .clicked()
                {
                    self.load(offset + 200, &ctx);
                }
            });
        });
        if let Some(i) = chosen {
            self.select(i, &ctx);
        }
        egui::CentralPanel::default().show(ui, |ui| self.comparison.show(ui));
    }
    fn blame(&self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            for (key, width) in [
                ("FileBlameLine", 55.),
                ("HistoryHash", 85.),
                ("HistoryAuthor", 140.),
                ("HistoryDate", 170.),
                ("FileBlameText", 400.),
            ] {
                ui.add_sized([width, 23.], egui::Label::new(text(key)));
            }
        });
        egui::ScrollArea::both().show_rows(ui, 23., self.blame.len(), |ui, range| {
            for i in range {
                let line = &self.blame[i];
                ui.horizontal(|ui| {
                    ui.add_sized([55., 23.], egui::Label::new(line.number.to_string()));
                    ui.add_sized([85., 23.], egui::Label::new(&line.hash[..8]));
                    ui.add_sized([140., 23.], egui::Label::new(&line.author).truncate());
                    ui.add_sized(
                        [170., 23.],
                        egui::Label::new(line.date.to_rfc3339()).truncate(),
                    );
                    ui.monospace(line.text.replace('\r', "␍").replace('\t', "    "));
                })
                .response
                .on_hover_text(strings::format(
                    "FileBlameHelp",
                    &[&line.hash, &line.author, &line.email, &line.summary],
                ));
            }
        });
    }
    pub fn show(&mut self, ctx: &egui::Context) {
        self.poll(ctx);
        if !self.open {
            return;
        }
        let mut visible = false;
        ctx.show_viewport_immediate(
            self.id,
            egui::ViewportBuilder::default()
                .with_title(strings::format("FileHistoryTitle", &[&self.entry.path]))
                .with_inner_size(self.size)
                .with_min_inner_size([900., 620.])
                .with_minimize_button(false),
            |ui, _| {
                visible = ui.is_visible();
                let ctx = ui.ctx().clone();
                if ctx.input(|i| i.viewport().close_requested() || i.key_pressed(egui::Key::Escape))
                {
                    ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                    self.close();
                    return;
                }
                if let Some(rect) = ctx.input(|i| i.viewport().inner_rect) {
                    self.size = rect.size();
                }
                egui::Panel::top("file_history_scope").show(ui, |ui| {
                    ui.label(strings::format(
                        "FileHistoryScope",
                        &[self
                            .context
                            .repository
                            .head
                            .as_deref()
                            .unwrap_or(&text("NoBranch"))],
                    ));
                    let old = self.mode;
                    ui.horizontal(|ui| {
                        ui.selectable_value(&mut self.mode, Mode::History, text("FileHistoryTab"));
                        ui.selectable_value(&mut self.mode, Mode::Blame, text("FileBlameTab"));
                    });
                    if old != self.mode {
                        self.load(self.offset(), &ctx);
                    }
                });
                egui::Panel::bottom("file_history_status").show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        if ui
                            .add_enabled(
                                self.busy || self.comparison.is_loading(),
                                egui::Button::new(text("ActionCancel")),
                            )
                            .clicked()
                        {
                            self.cancel_reads();
                        }
                        if ui
                            .add_enabled(!self.busy, egui::Button::new(text("RefreshRepository")))
                            .clicked()
                        {
                            self.load(self.offset(), &ctx);
                        }
                        ui.label(&self.status);
                    });
                    if !self.output.is_empty() {
                        ui.collapsing(text("ActionOutput"), |ui| {
                            egui::ScrollArea::vertical()
                                .max_height(100.)
                                .show(ui, |ui| {
                                    ui.monospace(&self.output);
                                });
                        });
                    }
                });
                match self.mode {
                    Mode::History => self.history(ui),
                    Mode::Blame => {
                        egui::CentralPanel::default().show(ui, |ui| self.blame(ui));
                    }
                }
            },
        );
        if visible && self.open && !self.initialized {
            self.initialized = true;
            self.load(0, ctx);
        }
    }
}
impl Drop for Inspector {
    fn drop(&mut self) {
        self.cancel.cancel();
        self.comparison.cancel();
    }
}
fn error_code(error: &Error) -> &str {
    match error {
        Error::Code(code) => code,
        Error::Git(shovel_core::git::Error::Command { outcome, .. }) => match outcome {
            Outcome::Cancelled => "ActionResultCancelled",
            Outcome::TimedOut => "GitErrorTimedOut",
            Outcome::OutputLimit => "GitErrorOutputLimit",
            _ => "ActionReadFailed",
        },
        Error::Git(shovel_core::git::Error::Limit) => "GitErrorOutputLimit",
        Error::Git(shovel_core::git::Error::Invalid) => "GitErrorInvalidOutput",
        _ => "ActionReadFailed",
    }
}
#[cfg(test)]
#[path = "file_history_tests.rs"]
mod tests;
