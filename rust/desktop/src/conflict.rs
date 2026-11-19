//! Inline conflict editor. Display reads are cancellable; accepted resolutions
//! belong to the repository scheduler and retain their result after view closure.
use crate::strings::{self, text};
use eframe::egui;
use shovel_core::{
    conflict_text::Encoding,
    conflicts::{Choice, Preview, Tool, ToolSettings},
    git::Git,
    process::Cancellation,
    recovery::Kind,
    scheduler::{Lane, Outcome, Scheduler},
    working::{Change, Context, Group},
};
use std::{
    sync::{
        Arc,
        mpsc::{self, Receiver, Sender},
    },
    thread,
};

struct Loaded {
    preview: Arc<Preview>,
    rows: [Vec<String>; 2],
}
struct Report {
    code: String,
    output: String,
    resolved: bool,
    retry: bool,
    updated: Option<Result<Context, String>>,
}
enum Event {
    Loaded(u64, Box<Result<Loaded, String>>),
    Started,
    Done(Box<Report>),
    Removed,
}
pub struct Refresh {
    pub context: Option<Result<Context, String>>,
    pub status: String,
    pub output: String,
    pub failed: bool,
}
pub struct Pane {
    read_workers: crate::read_workers::Workers,
    pub path: String,
    pub busy: bool,
    pub mutating: bool,
    pub resolved: bool,
    pub dirty: bool,
    pub status: String,
    pub output: String,
    context: Arc<Context>,
    change: Change,
    git: Arc<Git>,
    scheduler: Scheduler,
    saved_tool: ToolSettings,
    preview: Option<Arc<Preview>>,
    rows: [Vec<String>; 2],
    current: bool,
    incoming: bool,
    value: String,
    combined: String,
    tool: String,
    can_submit: bool,
    started: bool,
    cancel: Cancellation,
    generation: u64,
    sender: Sender<Event>,
    receiver: Receiver<Event>,
}
impl Pane {
    pub fn with_read_workers(mut self, workers: crate::read_workers::Workers) -> Self {
        self.read_workers = workers;
        self
    }
    pub fn new(
        context: Arc<Context>,
        change: Change,
        git: Arc<Git>,
        scheduler: Scheduler,
        mut saved_tool: ToolSettings,
    ) -> Self {
        if saved_tool.validate().is_err() {
            saved_tool = ToolSettings::default();
        }
        let tool = if saved_tool.mode != 0 {
            "@shovel".into()
        } else {
            String::new()
        };
        let (sender, receiver) = mpsc::channel();
        Self {
            read_workers: Default::default(),
            path: change.path.clone(),
            busy: false,
            mutating: false,
            resolved: false,
            dirty: false,
            status: text("ConflictLoading"),
            output: String::new(),
            context,
            change,
            git,
            scheduler,
            saved_tool,
            preview: None,
            rows: [vec![], vec![]],
            current: true,
            incoming: true,
            value: String::new(),
            combined: String::new(),
            tool,
            can_submit: false,
            started: false,
            cancel: Default::default(),
            generation: 0,
            sender,
            receiver,
        }
    }
    pub fn unsaved(&self) -> bool {
        self.dirty && !self.resolved && !self.mutating
    }
    pub fn pause(&mut self) {
        if !self.mutating {
            self.cancel.cancel();
            self.generation += 1;
            self.busy = false;
            self.started = false;
            self.can_submit = false;
        }
    }
    pub fn observe(&mut self, context: &Arc<Context>) {
        if !self.mutating
            && (context.index_stamp != self.context.index_stamp
                || context.config_stamp != self.context.config_stamp
                || context.repository.head != self.context.repository.head
                || context.repository.recovery != self.context.repository.recovery)
        {
            self.cancel.cancel();
            self.generation += 1;
            self.busy = false;
            self.can_submit = false;
            if !self.dirty {
                self.started = false;
            }
            self.status = text("ConflictReloadRequired");
        }
    }
    fn load(&mut self, fresh: bool, ctx: &egui::Context) {
        if self.mutating || self.read_workers.stopping() {
            return;
        }
        self.cancel.cancel();
        self.cancel = Cancellation::default();
        self.generation += 1;
        self.busy = true;
        self.started = true;
        self.status = text("ConflictLoading");
        self.can_submit = false;
        let (git, mut context, mut change, cancel, generation, tx, ctx) = (
            self.git.clone(),
            self.context.clone(),
            self.change.clone(),
            self.cancel.clone(),
            self.generation,
            self.sender.clone(),
            ctx.clone(),
        );
        let lease = self.read_workers.start(cancel.clone());
        thread::spawn(move || {
            let _lease = lease;
            #[cfg(test)]
            crate::history_cache::tests::wait_for_read(&context.repository.worktree);
            let result = (|| -> Result<Loaded, String> {
                if fresh {
                    context = Arc::new(
                        git.working_copy(&context.repository.worktree, &cancel)
                            .map_err(|e| text(&e.to_string()))?,
                    );
                    change = context
                        .entries
                        .iter()
                        .find(|e| e.path == change.path && e.group == Group::Conflicted)
                        .cloned()
                        .ok_or_else(|| text("ConflictNoLongerUnmerged"))?;
                }
                let preview = git
                    .read_conflict(context, &change, &cancel)
                    .map_err(|e| text(&e.code()))?;
                let rows = [1, 2].map(|index| {
                    preview.sides[index]
                        .as_ref()
                        .and_then(|s| s.content.as_ref())
                        .map(|c| {
                            c.text
                                .split('\n')
                                .map(|s| s.trim_end_matches('\r').to_owned())
                                .collect()
                        })
                        .unwrap_or_default()
                });
                Ok(Loaded {
                    preview: Arc::new(preview),
                    rows,
                })
            })();
            let _ = tx.send(Event::Loaded(generation, Box::new(result)));
            ctx.request_repaint();
        });
    }
    fn selected_side(&self) -> Option<usize> {
        match (self.current, self.incoming) {
            (true, true) => None,
            (true, false) => Some(1),
            (false, true) => Some(2),
            (false, false) => Some(0),
        }
    }
    fn choose(&mut self) {
        self.value = match self.selected_side() {
            None => self.combined.clone(),
            Some(index) => self
                .preview
                .as_ref()
                .and_then(|p| p.sides[index].as_ref())
                .and_then(|s| s.content.as_ref())
                .map(|c| c.text.clone())
                .unwrap_or_default(),
        };
    }
    fn merge_tool(&self) -> Tool {
        match self.tool.as_str() {
            "@shovel" => Tool::Saved(self.saved_tool.clone()),
            "" => Tool::Configured,
            name => Tool::Named(name.to_owned()),
        }
    }
    pub fn poll(&mut self, _ctx: &egui::Context) -> Option<Refresh> {
        let mut refresh = None;
        while let Ok(event) = self.receiver.try_recv() {
            match event {
                Event::Loaded(generation, result) if generation == self.generation => {
                    self.busy = false;
                    match *result {
                        Ok(loaded) => {
                            self.context = loaded.preview.context.clone();
                            self.change = loaded.preview.change.clone();
                            if !self.dirty {
                                self.combined = loaded
                                    .preview
                                    .working
                                    .as_ref()
                                    .map(|c| c.text.clone())
                                    .unwrap_or_default();
                                self.current = true;
                                self.incoming = true;
                            }
                            if !self.tool.is_empty()
                                && self.tool != "@shovel"
                                && !loaded.preview.tools.contains(&self.tool)
                            {
                                self.tool.clear();
                            }
                            self.preview = Some(loaded.preview);
                            self.rows = loaded.rows;
                            self.choose();
                            self.can_submit = true;
                            self.status = text("ConflictChoose");
                        }
                        Err(error) => {
                            self.can_submit = false;
                            self.status = error;
                        }
                    }
                }
                Event::Started => self.status = text("ConflictResolving"),
                Event::Done(report) => {
                    self.busy = false;
                    self.mutating = false;
                    self.resolved = report.resolved;
                    self.can_submit = report.retry;
                    if report.resolved {
                        self.dirty = false;
                    }
                    self.status = text(&report.code);
                    self.output = report.output;
                    if !report.retry {
                        refresh = Some(Refresh {
                            failed: !report.resolved
                                || report.updated.as_ref().is_some_and(Result::is_err),
                            context: report.updated,
                            status: self.status.clone(),
                            output: self.output.clone(),
                        });
                    }
                }
                Event::Removed => {
                    self.busy = false;
                    self.mutating = false;
                    self.can_submit = true;
                    self.status = text("QueuedRequestCancelled");
                }
                _ => {}
            }
        }
        refresh
    }
    fn submit(&mut self, choice: Choice, ctx: &egui::Context) {
        if self.busy || !self.can_submit {
            return;
        }
        let Some(preview) = self.preview.clone() else {
            return;
        };
        let (git, tx, discarded, ctx, discard_ctx) = (
            self.git.clone(),
            self.sender.clone(),
            self.sender.clone(),
            ctx.clone(),
            ctx.clone(),
        );
        let (_, accepted) = self.scheduler.submit(
            preview.context.repository.identity.fingerprint.clone(),
            shovel_core::case::path_key(&preview.context.repository.common_directory),
            format!("resolve:{}", self.path),
            Lane::WorkingCopy,
            move || {
                let _ = tx.send(Event::Started);
                ctx.request_repaint();
                let report = match git.resolve_conflict(&preview, &choice) {
                    Ok(result) => {
                        let mut output = result.operation.output;
                        if let Some(warning) = result.operation.warning {
                            output.push_str(&format!("\n{}", text(&warning)));
                        }
                        if result.operation.output_truncated {
                            output.push_str(&format!("\n{}", text("ActivityOutputLimit")));
                        }
                        Report {
                            code: if result.resolved {
                                "ConflictResolved".into()
                            } else {
                                result
                                    .operation
                                    .message
                                    .unwrap_or_else(|| "ActionFailedShort".into())
                            },
                            output,
                            resolved: result.resolved,
                            retry: false,
                            updated: Some(result.updated.map_err(|e| text(&e.to_string()))),
                        }
                    }
                    Err(error) => {
                        let code = error.code();
                        let retry = matches!(
                            code.as_str(),
                            "ConflictMarkersRemain" | "ConflictTextLimit" | "ConflictToolMissing"
                        );
                        let updated = (!retry).then(|| {
                            git.working_copy(
                                &preview.context.repository.worktree,
                                &Cancellation::default(),
                            )
                            .map_err(|e| text(&e.to_string()))
                        });
                        Report {
                            output: crate::operations::error_log(&error),
                            code,
                            resolved: false,
                            retry,
                            updated,
                        }
                    }
                };
                let outcome = if report.resolved {
                    Outcome::Succeeded
                } else {
                    Outcome::Failed
                };
                let _ = tx.send(Event::Done(Box::new(report)));
                ctx.request_repaint();
                outcome
            },
            move |_| {
                let _ = discarded.send(Event::Removed);
                discard_ctx.request_repaint();
            },
        );
        if accepted {
            self.busy = true;
            self.mutating = true;
            self.can_submit = false;
            self.status = text("OperationWaiting");
        }
    }
    fn result_label(&self) -> String {
        let encoding = self
            .preview
            .as_ref()
            .and_then(|p| p.working.as_ref().map(|content| content.encoding));
        encoding
            .map(|value| {
                strings::format(
                    "ConflictResultEncoding",
                    &[&text(match value {
                        Encoding::Utf8 => "ConflictEncodingUtf8",
                        Encoding::Utf8Bom => "ConflictEncodingUtf8Bom",
                        Encoding::Utf16LittleEndianBom => "ConflictEncodingUtf16LittleEndianBom",
                        Encoding::Utf16BigEndianBom => "ConflictEncodingUtf16BigEndianBom",
                    })],
                )
            })
            .unwrap_or_else(|| text("ConflictResult"))
    }
    pub fn show(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let mut submit = None;
        let mut reload = false;
        let preview = self.preview.clone();
        let editable = preview.as_ref().is_some_and(|p| p.can_edit);
        ui.heading(egui::RichText::new(text("ConflictHeading")).color(
            crate::workspace_style::Palette::change_status('M', ui.visuals().dark_mode),
        ));
        ui.add(egui::Label::new(&self.path).truncate())
            .on_hover_text(&self.path);
        if let Some(preview) = &preview {
            let rebase = preview
                .context
                .repository
                .recovery
                .as_ref()
                .is_some_and(|r| r.kind == Kind::Rebase);
            let old = (self.current, self.incoming);
            ui.columns(2, |columns| {
                for (index, column) in columns.iter_mut().enumerate() {
                    egui::Frame::group(column.style()).show(column, |ui| {
                        ui.set_width(ui.available_width());
                        let (checked, label, source) = if index == 0 {
                            (
                                &mut self.current,
                                if rebase {
                                    "ConflictRebasedSide"
                                } else {
                                    "ConflictCurrentSide"
                                },
                                &preview.current_source,
                            )
                        } else {
                            (
                                &mut self.incoming,
                                if rebase {
                                    "ConflictReplayedSide"
                                } else {
                                    "ConflictIncomingSide"
                                },
                                &preview.incoming_source,
                            )
                        };
                        ui.add_enabled(!self.busy, egui::Checkbox::new(checked, text(label)));
                        if !source.is_empty() {
                            ui.add(egui::Label::new(source).truncate())
                                .on_hover_text(source);
                        }
                        if let Some(side) = &preview.sides[index + 1] {
                            ui.horizontal_wrapped(|ui| {
                                if let Some(ext) = std::path::Path::new(&self.path)
                                    .extension()
                                    .and_then(|ext| ext.to_str())
                                {
                                    ui.monospace(format!(".{ext}"));
                                }
                                if side.lfs {
                                    let palette = crate::workspace_style::Palette::current(ui);
                                    ui.label(
                                        egui::RichText::new(text("BinaryLfs"))
                                            .strong()
                                            .color(palette.selection_text)
                                            .background_color(palette.selected),
                                    );
                                }
                                ui.label(crate::diff_view::binary_size(side.bytes));
                            });
                        } else {
                            ui.weak(text("ConflictDeletedSide"));
                        }
                    });
                }
            });
            if old != (self.current, self.incoming) {
                self.choose();
            }
        }
        let side = self.selected_side();
        let action = match side {
            None => "ConflictMergeAction",
            Some(0) => "ConflictBaseAction",
            Some(_) => "ConflictResolveStage",
        };
        let absent =
            side.is_some_and(|index| preview.as_ref().is_some_and(|p| p.sides[index].is_none()));
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(
                    !self.busy && self.can_submit,
                    egui::Button::new(text(action)),
                )
                .on_hover_text(text(if absent {
                    "ConflictDeletedSide"
                } else if side.is_none() && !editable {
                    "ConflictExternal"
                } else {
                    "ConflictResolveHelp"
                }))
                .clicked()
            {
                submit = Some(match side {
                    None if editable => Choice::Edited(self.value.clone()),
                    None => Choice::External(self.merge_tool()),
                    Some(0) => Choice::Base,
                    Some(1) => Choice::Current,
                    Some(_) => Choice::Incoming,
                });
            }
            if ui
                .add_enabled(
                    self.busy && !self.mutating,
                    egui::Button::new(text("ActionCancel")),
                )
                .clicked()
            {
                self.cancel.cancel();
                self.generation += 1;
                self.busy = false;
                self.can_submit = false;
                self.status = text("ActionResultCancelled");
            }
            if ui
                .add_enabled(!self.busy, egui::Button::new(text("ConflictReload")))
                .clicked()
            {
                reload = true;
            }
            if ui
                .add_enabled(
                    !self.busy && self.can_submit && (editable || side.is_some()),
                    egui::Button::new(text("ConflictExternal")),
                )
                .clicked()
            {
                submit = Some(Choice::External(self.merge_tool()));
            }
            ui.add_enabled_ui(!self.busy, |ui| {
                egui::ComboBox::from_id_salt(("conflict-tool", &self.path))
                    .width(165.)
                    .selected_text(if self.tool == "@shovel" {
                        text("ConflictToolSaved")
                    } else if self.tool.is_empty() {
                        text("ConflictToolDefault")
                    } else {
                        self.tool.clone()
                    })
                    .show_ui(ui, |ui| {
                        if self.saved_tool.mode != 0 {
                            ui.selectable_value(
                                &mut self.tool,
                                "@shovel".into(),
                                text("ConflictToolSaved"),
                            );
                        }
                        ui.selectable_value(
                            &mut self.tool,
                            String::new(),
                            text("ConflictToolDefault"),
                        );
                        if let Some(preview) = &preview {
                            for name in &preview.tools {
                                ui.selectable_value(&mut self.tool, name.clone(), name);
                            }
                        }
                    })
                    .response
                    .on_hover_text(text("ConflictToolHelp"));
            });
        });
        if absent {
            ui.label(text("ConflictDeletedSide"));
        }
        if self.can_submit && editable && side.is_none() && self.status == text("ConflictChoose") {
            ui.label(text("ConflictEditorHelp"));
        } else {
            ui.label(&self.status);
        }
        if editable && side.is_none() {
            egui::Panel::top("conflict-sources")
                .resizable(true)
                .default_size((ui.available_height() * 0.45).max(70.))
                .min_size(60.)
                .show(ui, |ui| {
                    ui.columns(2, |columns| {
                        for (index, column) in columns.iter_mut().enumerate() {
                            egui::ScrollArea::both()
                                .id_salt(("conflict-source", index))
                                .show_rows(column, 21., self.rows[index].len(), |ui, range| {
                                    for row in range {
                                        ui.horizontal(|ui| {
                                            ui.monospace(format!("{:>5}", row + 1));
                                            ui.monospace(&self.rows[index][row]);
                                        });
                                    }
                                });
                        }
                    });
                });
            ui.strong(self.result_label());
            egui::ScrollArea::both()
                .id_salt("conflict-result")
                .show(ui, |ui| {
                    if ui
                        .add_enabled(
                            !self.busy,
                            egui::TextEdit::multiline(&mut self.value)
                                .code_editor()
                                .char_limit(524288)
                                .desired_rows(8)
                                .desired_width(f32::INFINITY),
                        )
                        .changed()
                    {
                        self.combined = self.value.clone();
                        self.dirty = true;
                        self.current = true;
                        self.incoming = true;
                    }
                });
        }
        if let Some(choice) = submit {
            self.submit(choice, &ctx);
        } else if reload {
            self.load(true, &ctx);
        } else if !self.started && ui.is_visible() {
            self.load(self.preview.is_some(), &ctx);
        }
    }
}
impl Drop for Pane {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}
