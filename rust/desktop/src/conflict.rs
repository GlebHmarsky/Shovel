//! Local Changes conflict summary. Display reads are cancellable; accepted resolutions
//! belong to the repository scheduler and retain their result after view closure.
use crate::{
    strings::{self, text},
    workspace_style::Palette,
};
use eframe::egui;
use shovel_core::{
    conflict_text::Encoding,
    conflicts::{Choice, Preview, Tool, ToolSettings},
    git::Git,
    process::Cancellation,
    recovery::Kind,
    scheduler::{Lane, Outcome, Scheduler},
    stashes::ConflictOrigin,
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
    saved_path_available: bool,
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
    stash_conflict: Option<Arc<ConflictOrigin>>,
    preview: Option<Arc<Preview>>,
    current: bool,
    incoming: bool,
    value: String,
    inline_edit: bool,
    tool: String,
    saved_path_available: bool,
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
    pub fn with_stash_conflict(mut self, origin: Option<Arc<ConflictOrigin>>) -> Self {
        self.stash_conflict = origin;
        self
    }

    pub fn observe_stash_conflict(&mut self, origin: Option<Arc<ConflictOrigin>>) {
        self.stash_conflict = origin;
        self.pause();
    }

    pub fn update_saved_tool(&mut self, mut settings: ToolSettings) {
        if settings.validate().is_err() {
            settings = ToolSettings::default();
        }
        if self.saved_tool != settings {
            self.tool = if settings.mode == 0 { "" } else { "@shovel" }.into();
            self.saved_tool = settings;
            self.pause();
        }
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
            stash_conflict: None,
            preview: None,
            current: true,
            incoming: true,
            value: String::new(),
            inline_edit: false,
            tool,
            saved_path_available: true,
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
        let stash_conflict = self.stash_conflict.clone();
        let saved_path =
            (self.saved_tool.mode == 1).then(|| self.saved_tool.executable_path.clone());
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
                    .read_conflict_with_stash(context, &change, stash_conflict.as_deref(), &cancel)
                    .map_err(|e| text(&e.code()))?;
                Ok(Loaded {
                    preview: Arc::new(preview),
                    saved_path_available: saved_path
                        .as_deref()
                        .is_none_or(|path| path.is_empty() || std::path::Path::new(path).is_file()),
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
                                self.value = loaded
                                    .preview
                                    .working
                                    .as_ref()
                                    .map(|c| c.text.clone())
                                    .unwrap_or_default();
                                if self.preview.as_ref().is_none_or(|old| {
                                    old.sides.iter().zip(&loaded.preview.sides).any(|(a, b)| {
                                        a.as_ref().map(|side| &side.hash)
                                            != b.as_ref().map(|side| &side.hash)
                                    })
                                }) {
                                    self.current = true;
                                    self.incoming = true;
                                }
                            }
                            if !self.tool.is_empty()
                                && self.tool != "@shovel"
                                && !loaded.preview.tools.contains(&self.tool)
                            {
                                self.tool.clear();
                            }
                            self.saved_path_available = loaded.saved_path_available;
                            self.preview = Some(loaded.preview);
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
    fn merge_blocker(&self, preview: &Preview) -> Option<&'static str> {
        if preview.sides[1].is_none() || preview.sides[2].is_none() {
            Some("ConflictMergeNeedsBothFiles")
        } else if self.tool == "@shovel" && !self.saved_path_available {
            Some("ConflictToolExecutableMissing")
        } else if self.tool.is_empty() && preview.configured_tool.is_none() {
            Some("ConflictToolMissing")
        } else {
            None
        }
    }

    fn tool_picker(&mut self, ui: &mut egui::Ui, preview: &Preview) {
        let name = if self.tool == "@shovel" {
            if self.saved_tool.mode == 1 {
                self.saved_tool.tool_name.clone()
            } else {
                text("ConflictToolSaved")
            }
        } else if self.tool.is_empty() {
            preview
                .configured_tool
                .clone()
                .unwrap_or_else(|| text("ConflictToolNone"))
        } else {
            self.tool.clone()
        };
        ui.add_enabled_ui(!self.busy, |ui| {
            egui::ComboBox::from_id_salt(("conflict-tool", &self.path))
                .width(165.)
                .selected_text(name)
                .show_ui(ui, |ui| {
                    if self.saved_tool.mode != 0 {
                        ui.selectable_value(
                            &mut self.tool,
                            "@shovel".into(),
                            text("ConflictToolSaved"),
                        );
                    }
                    ui.selectable_value(&mut self.tool, String::new(), text("ConflictToolDefault"));
                    for name in &preview.tools {
                        ui.selectable_value(&mut self.tool, name.clone(), name);
                    }
                })
                .response
                .on_hover_text(text("ConflictToolHelp"));
        });
    }

    pub fn show(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let mut submit = None;
        let mut reload = false;
        let preview = self.preview.clone();
        let palette = Palette::current(ui);
        egui::ScrollArea::vertical()
            .id_salt(("conflict-summary", &self.path))
            .show(ui, |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(14.);
                    ui.heading(
                        egui::RichText::new(text("ConflictHeading"))
                            .color(Palette::change_status('M', ui.visuals().dark_mode)),
                    );
                    let path = crate::path_text::PathText::new(ui.ctx(), &self.path);
                    let label = ui
                        .add(
                            egui::Label::new(
                                egui::RichText::new(format!("▤  {}", path.visual())).monospace(),
                            )
                            .truncate(),
                        )
                        .on_hover_text(path.spoken());
                    ui.ctx()
                        .accesskit_node_builder(label.id, |node| node.set_label(path.spoken()));
                    ui.add_space(12.);

                    if let Some(preview) = &preview {
                        let width = ui.available_width().min(900.);
                        ui.allocate_ui_with_layout(
                            egui::vec2(width, 0.),
                            egui::Layout::top_down(egui::Align::Center),
                            |ui| {
                                ui.set_width(width);
                                let gap = ui
                                    .horizontal(|ui| {
                                        ui.spacing_mut().item_spacing.x = 0.;
                                        let gap_width = (width * 0.20).clamp(48., 150.).min(width);
                                        let card_width = ((width - gap_width) / 2.).max(0.);
                                        source_card(
                                            ui,
                                            preview,
                                            0,
                                            card_width,
                                            self.current,
                                            &palette,
                                        );
                                        let gap = ui
                                            .allocate_exact_size(
                                                egui::vec2(gap_width, SOURCE_HEIGHT),
                                                egui::Sense::hover(),
                                            )
                                            .0;
                                        source_card(
                                            ui,
                                            preview,
                                            1,
                                            card_width,
                                            self.incoming,
                                            &palette,
                                        );
                                        gap
                                    })
                                    .inner;
                                let mut controls =
                                    ui.new_child(egui::UiBuilder::new().max_rect(gap));
                                if self.busy {
                                    controls.disable();
                                }
                                let mut ends = [egui::Pos2::ZERO; 2];
                                for (index, selected) in [&mut self.current, &mut self.incoming]
                                    .into_iter()
                                    .enumerate()
                                {
                                    let x = if index == 0 {
                                        gap.left() + 12.
                                    } else {
                                        gap.right() - 12.
                                    };
                                    let rect = egui::Rect::from_center_size(
                                        egui::pos2(x, gap.center().y),
                                        egui::Vec2::splat(20.),
                                    );
                                    let control =
                                        controls.put(rect, egui::Checkbox::without_text(selected));
                                    let label = source_role(preview, index);
                                    controls.ctx().accesskit_node_builder(control.id, |node| {
                                        node.set_label(text(label))
                                    });
                                    control.on_hover_text(text(label));
                                    ends[index] = egui::pos2(
                                        if index == 0 {
                                            rect.right()
                                        } else {
                                            rect.left()
                                        },
                                        rect.center().y,
                                    );
                                }

                                let side = self.selected_side();
                                let (action, blocker, help) = match side {
                                    None => (
                                        "ConflictMergeAction",
                                        self.merge_blocker(preview),
                                        "ConflictToolHelp",
                                    ),
                                    Some(0) => (
                                        "ConflictBaseAction",
                                        preview.sides[0].is_none().then_some("ConflictNoBase"),
                                        "ConflictResolveHelp",
                                    ),
                                    Some(index) => (
                                        if index == 1 {
                                            "ConflictUseOurs"
                                        } else {
                                            "ConflictUseTheirs"
                                        },
                                        None,
                                        if preview.sides[index].is_none() {
                                            "ConflictDeletedSide"
                                        } else {
                                            "ConflictResolveHelp"
                                        },
                                    ),
                                };
                                ui.add_space(13.);
                                let ready = !self.busy && self.can_submit;
                                let primary = ui
                                    .add_enabled(
                                        ready && blocker.is_none(),
                                        egui::Button::new(text(action))
                                            .min_size(egui::vec2(width.min(145.), 29.)),
                                    )
                                    .on_hover_text(text(help));
                                for (index, selected) in
                                    [self.current, self.incoming].into_iter().enumerate()
                                {
                                    let bend = gap.center().x + if index == 0 { -3. } else { 3. };
                                    let stroke = egui::Stroke::new(
                                        if selected { 1.8 } else { 1. },
                                        if selected {
                                            palette.focus
                                        } else {
                                            palette.border
                                        },
                                    );
                                    ui.painter().add(egui::Shape::line(
                                        vec![
                                            ends[index],
                                            egui::pos2(bend, gap.center().y),
                                            egui::pos2(bend, primary.rect.top() - 5.),
                                        ],
                                        stroke,
                                    ));
                                }
                                if primary.clicked() {
                                    submit = Some(match side {
                                        None => Choice::External(self.merge_tool()),
                                        Some(0) => Choice::Base,
                                        Some(1) => Choice::Current,
                                        Some(_) => Choice::Incoming,
                                    });
                                }
                                if ready && let Some(blocker) = blocker {
                                    ui.weak(text(blocker));
                                } else if ready
                                    && side.is_some_and(|index| preview.sides[index].is_none())
                                {
                                    ui.weak(text("ConflictDeletedSide"));
                                }
                                if side.is_none() {
                                    self.tool_picker(ui, preview);
                                }
                                if preview.can_edit
                                    && (side.is_none() || self.dirty)
                                    && ui
                                        .add_enabled(
                                            !self.busy,
                                            egui::Button::new(text(if self.inline_edit {
                                                "ConflictHideEditor"
                                            } else {
                                                "ConflictShowEditor"
                                            })),
                                        )
                                        .clicked()
                                {
                                    self.inline_edit = !self.inline_edit;
                                }
                            },
                        );
                    }
                    ui.weak(&self.status);
                    if self.busy && !self.mutating {
                        if ui.button(text("ActionCancel")).clicked() {
                            self.cancel.cancel();
                            self.generation += 1;
                            self.busy = false;
                            self.can_submit = false;
                            self.status = text("ActionResultCancelled");
                        }
                    } else if ui
                        .add_enabled(!self.busy, egui::Button::new(text("ConflictReload")))
                        .clicked()
                    {
                        reload = true;
                    }
                });
                if self.inline_edit
                    && preview.as_ref().is_some_and(|p| p.can_edit)
                    && (self.selected_side().is_none() || self.dirty)
                {
                    ui.separator();
                    ui.strong(self.result_label());
                    ui.weak(text("ConflictEditorHelp"));
                    let stage_edit = ui
                        .add_enabled(
                            !self.busy && self.can_submit && self.selected_side().is_none(),
                            egui::Button::new(text("ConflictStageEdited")),
                        )
                        .clicked();
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
                        self.dirty = true;
                        self.current = true;
                        self.incoming = true;
                    }
                    if stage_edit {
                        submit = Some(Choice::Edited(self.value.clone()));
                    }
                }
            });
        if let Some(choice) = submit {
            self.submit(choice, &ctx);
        } else if reload {
            self.load(true, &ctx);
        } else if !self.started && ui.is_visible() {
            self.load(self.preview.is_some(), &ctx);
        }
    }
}

const SOURCE_HEIGHT: f32 = 126.;

fn source_role(preview: &Preview, index: usize) -> &'static str {
    let operation = preview
        .context
        .repository
        .recovery
        .as_ref()
        .map(|state| state.kind);
    match (index, operation) {
        (0, Some(Kind::Rebase)) => "ConflictRebasedSide",
        (0, _) => "ConflictCurrentSide",
        (_, Some(Kind::Rebase)) => "ConflictReplayedSide",
        (_, Some(Kind::CherryPick)) => "ConflictCherryPickSide",
        (_, Some(Kind::Revert)) => "ConflictRevertSide",
        _ if preview.from_stash => "ConflictStashSide",
        _ => "ConflictIncomingSide",
    }
}

fn source_card(
    ui: &mut egui::Ui,
    preview: &Preview,
    index: usize,
    width: f32,
    selected: bool,
    palette: &Palette,
) {
    let source = &preview.sources[index];
    let role = text(source_role(preview, index));
    let title = source.reference.clone().unwrap_or_else(|| {
        text(if index == 0 {
            "ConflictOurs"
        } else {
            "ConflictTheirs"
        })
    });
    let base = preview.sides[0].as_ref();
    let side = preview.sides[index + 1].as_ref();
    let (status, code) = match (base, side) {
        (None, None) => ("BinaryAbsent", None),
        (None, Some(_)) => ("ChangeCodeA", Some('A')),
        (Some(_), None) => ("ChangeCodeD", Some('D')),
        (Some(base), Some(side)) if base.hash == side.hash && base.mode == side.mode => {
            ("ConflictUnchangedSide", None)
        }
        _ => ("ChangeCodeM", Some('M')),
    };
    let status_color = if palette.high_contrast {
        palette.text
    } else {
        code.map_or(palette.muted, |code| {
            Palette::change_status(code, ui.visuals().dark_mode)
        })
    };
    egui::Frame::new()
        .fill(palette.raised)
        .stroke((
            1.,
            if selected {
                palette.focus
            } else {
                palette.border
            },
        ))
        .inner_margin(8)
        .corner_radius(4)
        .show(ui, |ui| {
            ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                ui.set_width((width - 18.).max(0.));
                ui.set_min_height(SOURCE_HEIGHT - 18.);
                ui.add(egui::Label::new(egui::RichText::new(&title).strong()).truncate())
                    .on_hover_text(title);
                ui.add(
                    egui::Label::new(egui::RichText::new(&role).small().color(palette.muted))
                        .truncate(),
                )
                .on_hover_text(role);
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(text(status))
                            .small()
                            .strong()
                            .color(status_color),
                    );
                    if let Some(side) = side {
                        if side.lfs {
                            ui.label(egui::RichText::new(text("BinaryLfs")).small().strong());
                        }
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(crate::diff_view::binary_size(side.bytes))
                                    .small(),
                            )
                            .truncate(),
                        );
                    }
                });
                if let Some(revision) = &source.revision {
                    let short = revision.get(..8).unwrap_or(revision);
                    let subject = source
                        .commit
                        .as_ref()
                        .map(|commit| commit.subject())
                        .filter(|subject| !subject.is_empty());
                    let commit_label = subject
                        .map_or_else(|| short.to_owned(), |subject| format!("{short}  {subject}"));
                    ui.add(
                        egui::Label::new(egui::RichText::new(&commit_label).monospace()).truncate(),
                    )
                    .on_hover_text(subject.map_or_else(
                        || revision.clone(),
                        |subject| format!("{revision}\n{subject}"),
                    ));
                } else {
                    ui.weak(text("ConflictCommitUnavailable"));
                }
                if let Some(commit) = &source.commit {
                    ui.horizontal(|ui| {
                        let (avatar, _) =
                            ui.allocate_exact_size(egui::Vec2::splat(18.), egui::Sense::hover());
                        crate::history_row::paint_avatar(
                            ui,
                            avatar,
                            commit,
                            palette,
                            &preview.context.repository.worktree,
                        );
                        let author_and_date = format!(
                            "{} · {}",
                            commit.author,
                            crate::dates::local(ui.ctx(), &commit.authored)
                        );
                        ui.add(
                            egui::Label::new(egui::RichText::new(author_and_date).small())
                                .truncate(),
                        )
                        .on_hover_text(crate::history_row::author_details(ui.ctx(), commit));
                    });
                }
            });
        });
}

impl Drop for Pane {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}
