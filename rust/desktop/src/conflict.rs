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
}
pub struct Pane {
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
            self.status = text("ConflictReloadRequired");
        }
    }
    fn load(&mut self, fresh: bool, ctx: &egui::Context) {
        if self.mutating {
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
        thread::spawn(move || {
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
    fn choose(&mut self) {
        self.value = if self.current && self.incoming {
            self.combined.clone()
        } else {
            self.preview
                .as_ref()
                .and_then(|p| p.sides[if self.current { 1 } else { 2 }].as_ref())
                .filter(|_| self.current || self.incoming)
                .and_then(|s| s.content.as_ref())
                .map(|c| c.text.clone())
                .unwrap_or_default()
        };
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
                    if !report.output.is_empty() {
                        self.output = report.output;
                    }
                    if !report.retry {
                        refresh = Some(Refresh {
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
            preview
                .context
                .repository
                .common_directory
                .to_string_lossy()
                .to_uppercase(),
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
                            code,
                            output: String::new(),
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
        let encoding = self.preview.as_ref().and_then(|p| {
            if self.current && self.incoming {
                p.working.as_ref().map(|c| c.encoding)
            } else {
                p.sides[if self.current { 1 } else { 2 }]
                    .as_ref()
                    .filter(|_| self.current || self.incoming)
                    .and_then(|s| s.content.as_ref())
                    .map(|c| c.encoding)
            }
        });
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
        ui.strong(&self.path);
        ui.horizontal_wrapped(|ui| {
            let reason = if self.busy {
                "ConflictLoading"
            } else if !self.can_submit {
                "ConflictReloadRequired"
            } else if !self.current && !self.incoming {
                "ConflictChoose"
            } else if self.current && self.incoming && !editable {
                "ConflictBinaryHelp"
            } else {
                "ConflictResolveHelp"
            };
            if ui
                .add_enabled(
                    !self.busy
                        && self.can_submit
                        && (self.current || self.incoming)
                        && (!(self.current && self.incoming) || editable),
                    egui::Button::new(text("ConflictResolveStage")),
                )
                .on_disabled_hover_text(text(reason))
                .on_hover_text(text("ConflictResolveHelp"))
                .clicked()
            {
                submit = Some(if self.current && self.incoming {
                    Choice::Edited(self.value.clone())
                } else if self.current {
                    Choice::Current
                } else {
                    Choice::Incoming
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
                    !self.busy && self.can_submit,
                    egui::Button::new(text("ConflictExternal")),
                )
                .clicked()
            {
                submit = Some(Choice::External(if self.tool == "@shovel" {
                    Tool::Saved(self.saved_tool.clone())
                } else if self.tool.is_empty() {
                    Tool::Configured
                } else {
                    Tool::Named(self.tool.clone())
                }));
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
        ui.label(text(if editable {
            "ConflictEditorHelp"
        } else if preview.is_some() {
            "ConflictBinaryHelp"
        } else {
            "ConflictLoading"
        }));
        ui.label(&self.status);
        if let Some(preview) = &preview {
            let rebase = preview
                .context
                .repository
                .recovery
                .as_ref()
                .is_some_and(|r| r.kind == Kind::Rebase);
            let labels = [
                strings::format(
                    if rebase {
                        "ConflictRebasedSide"
                    } else {
                        "ConflictCurrentSide"
                    },
                    &[&preview.current_source],
                ),
                if rebase {
                    strings::format("ConflictReplayedSide", &[&preview.incoming_source])
                } else if preview.incoming_source.is_empty() {
                    text("ConflictIncomingUnknown")
                } else {
                    strings::format("ConflictIncomingSide", &[&preview.incoming_source])
                },
            ];
            let old = (self.current, self.incoming);
            egui::Panel::top("conflict-sources")
                .resizable(true)
                .default_size((ui.available_height() * 0.55).max(70.))
                .min_size(70.)
                .show(ui, |ui| {
                    ui.columns(2, |columns| {
                        for index in 0..2 {
                            let column = &mut columns[index];
                            column.add_enabled_ui(!self.busy, |ui| {
                                ui.checkbox(
                                    if index == 0 {
                                        &mut self.current
                                    } else {
                                        &mut self.incoming
                                    },
                                    &labels[index],
                                );
                            });
                            if editable {
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
                            } else {
                                column.label(text(if preview.sides[index + 1].is_none() {
                                    "ConflictDeletedSide"
                                } else {
                                    "ConflictBinarySide"
                                }));
                            }
                        }
                    });
                });
            if old != (self.current, self.incoming) {
                self.choose();
            }
            if editable {
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
