//! A file menu owns one immutable selection. Opening/confirming it uses only the
//! displayed snapshot; execution and required validation belong to the worker.
use super::*;
use shovel_core::{
    file_rules::{self, Action as RuleAction},
    files, operation,
};

#[derive(Clone)]
pub(super) struct Selection {
    pub context: Arc<Context>,
    pub entries: Vec<Change>,
}
pub(super) struct Menu {
    pub selected: Selection,
    pub entry: Change,
    pub position: egui::Pos2,
    pub layer: egui::LayerId,
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum FileAction {
    Discard,
    Stash,
    ExternalDiff,
    Rules(RuleAction),
}
impl FileAction {
    pub(super) fn phase(self) -> &'static str {
        match self {
            Self::Discard => "CommitPhaseDiscard",
            Self::Stash => "CommitPhaseStashFiles",
            Self::ExternalDiff => "CommitPhaseExternalDiff",
            Self::Rules(action) => match action {
                RuleAction::IgnorePaths => "CommitPhaseIgnoreFiles",
                RuleAction::IgnoreExtensions => "CommitPhaseIgnoreExtension",
                RuleAction::LfsTrack => "CommitPhaseLfsTrack",
                RuleAction::LfsUntrack => "CommitPhaseLfsUntrack",
            },
        }
    }
    fn succeeded(self) -> &'static str {
        match self {
            Self::Discard => "CommitSucceededDiscard",
            Self::Stash => "CommitSucceededStashFiles",
            Self::ExternalDiff => "CommitSucceededExternalDiff",
            Self::Rules(action) => match action {
                RuleAction::IgnorePaths => "CommitSucceededIgnoreFiles",
                RuleAction::IgnoreExtensions => "CommitSucceededIgnoreExtension",
                RuleAction::LfsTrack => "CommitSucceededLfsTrack",
                RuleAction::LfsUntrack => "CommitSucceededLfsUntrack",
            },
        }
    }
    fn partial(self) -> &'static str {
        match self {
            Self::Discard => "RustDiscardPartial",
            Self::Stash => "RustFileStashPartial",
            Self::ExternalDiff => "RustFileDiffPartial",
            Self::Rules(_) => "RustFileRulesPartial",
        }
    }
}
pub(super) struct Confirmation {
    pub selection: Selection,
    pub action: FileAction,
    pub message: String,
    keyboard_focus: bool,
}
impl Confirmation {
    fn new(selection: Selection, action: FileAction) -> Self {
        Self {
            selection,
            action,
            message: String::new(),
            keyboard_focus: true,
        }
    }
}
impl Editor {
    pub(super) fn selected_entries(&self, context: &Context, staged: bool) -> Vec<Change> {
        self.selection
            .iter()
            .filter(|(group, _)| (*group == Group::Staged) == staged && *group != Group::Ignored)
            .filter_map(|(group, path)| context.change(*group, path).cloned())
            .collect()
    }
    pub(super) fn file_selection_current(&self, selection: &Selection) -> bool {
        self.snapshot_error.is_none()
            && self.context.as_ref().is_some_and(|context| {
                let reviewed = &selection.context;
                Arc::ptr_eq(context, reviewed)
                    || context.repository.identity == reviewed.repository.identity
                        && context.repository.head == reviewed.repository.head
                        && context.repository.branch == reviewed.repository.branch
                        && context.repository.operations == reviewed.repository.operations
                        && context.index_stamp == reviewed.index_stamp
                        && context.config_stamp == reviewed.config_stamp
                        && selection.entries.iter().all(|entry| context.change(entry.group, &entry.path) == Some(entry))
            })
    }
    pub(super) fn begin_file_menu(&mut self, menu: Menu) {
        if self
            .conflict
            .as_ref()
            .is_some_and(|p| p.path != menu.entry.path || menu.entry.group != Group::Conflicted)
        {
            self.conflict = None;
        }
        let key = (menu.entry.group, menu.entry.path.clone());
        if !self.selection.contains(&key) {
            self.selection_anchor = Some(key);
        }
        self.selection = menu
            .selected
            .entries
            .iter()
            .map(|e| (e.group, e.path.clone()))
            .collect();
        self.set_diff_target(menu.entry.clone());
        self.diff_deferred = true;
        self.file_menu = Some(menu);
    }
    fn copy_paths(&self, selection: &Selection, full: bool, ctx: &egui::Context) {
        let paths = selection
            .entries
            .iter()
            .map(|e| {
                if full {
                    working::path(&selection.context.repository.worktree, &e.path)
                        .map(|p| p.to_string_lossy().replace('/', "\\"))
                } else {
                    Ok(e.path.clone())
                }
            })
            .collect::<Result<Vec<_>, working::Error>>();
        if let Ok(paths) = paths {
            ctx.copy_text(paths.join("\r\n"));
        }
    }
    pub(super) fn show_file_menu(&mut self, ctx: &egui::Context) {
        let Some(menu) = self.file_menu.take() else {
            return;
        };
        let current = self.file_selection_current(&menu.selected);
        let enabled = current && !self.action_busy();
        let reason = text(if !current {
            "FileMenuStale"
        } else {
            "CommitWaitForRead"
        });
        let staged = menu.entry.group == Group::Staged;
        let can_stage = |entry: &Change| {
            if staged {
                entry.group == Group::Staged
            } else {
                entry.group.can_stage()
            }
        };
        let mut open = true;
        let mut action = None;
        egui::Popup::new(
            egui::Id::unique("change-file-menu"),
            ctx.clone(),
            menu.position,
            menu.layer,
        )
        .kind(egui::PopupKind::Menu)
        .open_bool(&mut open)
        .layout(egui::Layout::top_down_justified(egui::Align::Min))
        .show(|ui| {
            ui.label(strings::format(
                "FileMenuSelection",
                &[&menu.selected.entries.len().to_string()],
            ));
            let single = menu.selected.entries.len() == 1;
            let deleted = menu.entry.code == 'D';
            if ui
                .add_enabled(enabled && single && !deleted, egui::Button::new(text("FileMenuOpen")).shortcut_text(text("FileMenuOpenKeys")))
                .on_hover_text(text("FileMenuOpenHelp"))
                .on_disabled_hover_text(if !enabled {
                    reason.clone()
                } else if !single {
                    text("FileMenuSingle")
                } else {
                    text("FileMenuDeleted")
                })
                .clicked()
            {
                action = Some(12);
            }
            if ui
                .add_enabled(enabled && single, egui::Button::new(text("FileMenuExplorer")))
                .on_hover_text(text("FileMenuExplorerHelp"))
                .on_disabled_hover_text(if !enabled { reason.clone() } else { text("FileMenuSingle") })
                .clicked()
            {
                action = Some(13);
            }
            ui.separator();
            if ui
                .add_enabled(
                    self.working_ready() && menu.selected.entries.iter().any(can_stage),
                    egui::Button::new(text(if staged {
                        "CommitUnstage"
                    } else {
                        "CommitStage"
                    })),
                )
                .on_disabled_hover_text(&reason)
                .clicked()
            {
                action = Some(0);
            }
            if ui
                .add_enabled(
                    self.working_ready() && super::file_list::FileList::new(Some(&menu.selected.context), staged)
                        .stageable(staged)
                        .next()
                        .is_some(),
                    egui::Button::new(text(if staged {
                        "CommitUnstageAll"
                    } else {
                        "CommitStageAll"
                    })),
                )
                .on_disabled_hover_text(&reason)
                .clicked()
            {
                action = Some(1);
            }
            ui.separator();
            if ui
                .add_enabled(
                    !menu.selected.entries.is_empty(),
                    egui::Button::new(text("FileMenuCopy")),
                )
                .clicked()
            {
                action = Some(2);
            }
            if ui
                .add_enabled(
                    !menu.selected.entries.is_empty(),
                    egui::Button::new(text("FileMenuCopyFull")),
                )
                .clicked()
            {
                action = Some(3);
            }
            ui.separator();
            if ui
                .add_enabled(
                    enabled && files::supported(&menu.selected.entries),
                    egui::Button::new(text("FileDiscardMenu")),
                )
                .on_disabled_hover_text(if enabled {
                    text("FileActionUnsupported")
                } else {
                    reason.clone()
                })
                .clicked()
            {
                action = Some(4);
            }
            let stash_reason = if !files::supported(&menu.selected.entries) {
                "FileActionUnsupported"
            } else if menu.selected.context.repository.head.is_none() {
                "StashNeedsCommit"
            } else {
                "ActionSequence"
            };
            if ui
                .add_enabled(
                    enabled
                        && files::supported(&menu.selected.entries)
                        && menu.selected.context.repository.head.is_some()
                        && menu.selected.context.repository.operations.is_empty(),
                    egui::Button::new(strings::format(
                        "FileStashLabel",
                        &[&menu.selected.entries.len().to_string()],
                    )),
                )
                .on_disabled_hover_text(if enabled {
                    text(stash_reason)
                } else {
                    reason.clone()
                })
                .clicked()
            {
                action = Some(5);
            }
            if ui
                .add_enabled(
                    enabled
                        && menu.selected.entries.len() == 1
                        && files::supported(&menu.selected.entries),
                    egui::Button::new(text("FileExternalDiff")),
                )
                .on_hover_text(text("FileDiffToolHelp"))
                .on_disabled_hover_text(if !enabled {
                    reason.clone()
                } else if menu.selected.entries.len() != 1 {
                    text("FileMenuSingle")
                } else {
                    text("FileActionUnsupported")
                })
                .clicked()
            {
                action = Some(6);
            }
            ui.separator();
            let supported = files::supported(&menu.selected.entries);
            for (id, key, available) in [
                (14, "FileHistoryMenu", true),
                (15, "FileBlameMenu", menu.selected.context.repository.head.is_some()),
            ] {
                if ui.add_enabled(enabled && single && available, egui::Button::new(text(key)))
                    .on_hover_text(text("FileHistoryHelp"))
                    .on_disabled_hover_text(if !enabled {
                        reason.clone()
                    } else if !single {
                        text("FileMenuSingle")
                    } else {
                        text("FileBlameNoHead")
                    })
                    .clicked()
                {
                    action = Some(id);
                }
            }
            if ui
                .add_enabled(enabled && supported, egui::Button::new(text("FilePatchMenu")))
                .on_hover_text(text("FilePatchHelp"))
                .on_disabled_hover_text(if !enabled {
                    reason.clone()
                } else {
                    text("FileActionUnsupported")
                })
                .clicked()
            {
                action = Some(11);
            }
            let untracked = menu
                .selected
                .entries
                .iter()
                .all(|e| e.group == Group::Untracked);
            ui.add_enabled_ui(enabled && supported && untracked, |ui| {
                ui.menu_button(text("FileIgnoreMenu"), |ui| {
                    if ui
                        .button(text("FileIgnorePaths"))
                        .on_hover_text(text("FileIgnoreHelp"))
                        .clicked()
                    {
                        action = Some(7);
                    }
                    let extensions = menu
                        .selected
                        .entries
                        .iter()
                        .all(|e| file_rules::extension(&e.path).is_some());
                    if ui
                        .add_enabled(extensions, egui::Button::new(text("FileIgnoreExtensions")))
                        .on_hover_text(text("FileIgnoreExtensionHelp"))
                        .on_disabled_hover_text(text("FileIgnoreNoExtension"))
                        .clicked()
                    {
                        action = Some(8);
                    }
                });
            })
            .response
            .on_disabled_hover_text(if !enabled {
                reason.clone()
            } else if !supported {
                text("FileActionUnsupported")
            } else {
                text("FileIgnoreTracked")
            });
            ui.add_enabled_ui(enabled && supported, |ui| {
                ui.menu_button(text("FileLfsMenu"), |ui| {
                    if ui
                        .button(text("FileLfsTrack"))
                        .on_hover_text(text("FileLfsHelp"))
                        .clicked()
                    {
                        action = Some(9);
                    }
                    if ui
                        .button(text("FileLfsUntrack"))
                        .on_hover_text(text("FileLfsHelp"))
                        .clicked()
                    {
                        action = Some(10);
                    }
                });
            })
            .response
            .on_disabled_hover_text(if !enabled {
                reason.clone()
            } else {
                text("FileActionUnsupported")
            });
        });
        if let Some(action) = action {
            match action {
                0 | 1 => {
                    let entries = if action == 0 {
                        menu.selected.entries.iter().filter(|entry| can_stage(entry)).cloned().collect()
                    } else {
                        super::file_list::FileList::new(Some(&menu.selected.context), staged)
                            .stageable(staged)
                            .cloned()
                            .collect()
                    };
                    self.submit_captured(
                        if staged {
                            Action::Unstage(entries)
                        } else {
                            Action::Stage(entries)
                        },
                        menu.selected.context.clone(),
                        None,
                        None,
                        ctx,
                    );
                }
                2 | 3 => self.copy_paths(&menu.selected, action == 3, ctx),
                4 | 5 => {
                    self.file_confirmation = Some(Confirmation::new(
                        menu.selected.clone(),
                        if action == 4 {
                            FileAction::Discard
                        } else {
                            FileAction::Stash
                        },
                    ))
                }
                6 => self.submit_file_action(
                    Confirmation::new(menu.selected.clone(), FileAction::ExternalDiff),
                    ctx,
                ),
                7..=10 => self.submit_file_action(
                    Confirmation::new(
                        menu.selected.clone(),
                        FileAction::Rules(match action {
                            7 => RuleAction::IgnorePaths,
                            8 => RuleAction::IgnoreExtensions,
                            9 => RuleAction::LfsTrack,
                            _ => RuleAction::LfsUntrack,
                        }),
                    ),
                    ctx,
                ),
                11 => self.begin_patch(menu.selected.clone(), ctx),
                12 | 13 => self.begin_shell(menu.selected.clone(), action == 13, ctx),
                14 | 15 => {
                    let mut inspector = crate::file_history::Inspector::new(
                        menu.selected.context.clone(),
                        menu.selected.entries[0].clone(),
                        self.git.clone(),
                        action == 15,
                    )
                    .with_read_workers(self.read_workers.clone());
                    if let Some(mut previous) = self.inspector.take() {
                        std::mem::swap(&mut inspector.layout, &mut previous.layout);
                        inspector.layout.new_viewport();
                    }
                    self.inspector = Some(inspector);
                    ctx.request_repaint();
                }
                _ => unreachable!(),
            }
            open = false;
        }
        if open {
            self.file_menu = Some(menu);
        }
    }
    pub(super) fn show_file_confirmation(&mut self, ctx: &egui::Context) {
        let Some(mut confirmation) = self.file_confirmation.take() else {
            return;
        };
        let selection = &confirmation.selection;
        let stash = confirmation.action == FileAction::Stash;
        let current = self.file_selection_current(selection);
        let mut accept = false;
        let mut cancel = false;
        let response = egui::Modal::new(egui::Id::unique("discard-files")).show(ctx, |ui| {
            ui.set_width(460.);
            ui.heading(text(if stash {
                "FileStashTitle"
            } else {
                "FileDiscardTitle"
            }));
            let repository = &selection.context.repository;
            let root = repository.worktree.to_string_lossy();
            let root = crate::path_text::PathText::new(ctx, &root);
            ui.label(text(if stash {
                "FileStashExplanation"
            } else {
                "FileDiscardExplanation"
            }));
            crate::path_text::label(
                ui,
                root.visual().to_owned(),
                root.reordered().then(|| root.spoken().to_owned()),
            );
            egui::ScrollArea::vertical().max_height(260.).show_rows(
                ui,
                19.,
                selection.entries.len(),
                |ui, range| {
                    for index in range {
                        let entry = &selection.entries[index];
                        let path = crate::path_text::PathText::new(ctx, &entry.path);
                        let old = entry
                            .original
                            .as_ref()
                            .filter(|_| entry.code == 'R')
                            .map(|p| crate::path_text::PathText::new(ctx, p));
                        let key = if stash {
                            "FileStashTarget"
                        } else if entry.group == Group::Untracked
                            || entry.group == Group::Staged
                                && selection.context.repository.head.is_none()
                        {
                            "FileDiscardNewTarget"
                        } else if entry.group == Group::Staged {
                            "FileDiscardStagedTarget"
                        } else {
                            "FileDiscardWorktreeTarget"
                        };
                        let message = |path: &str, old: Option<&str>| match old {
                            Some(old) => strings::format(
                                key,
                                &[&strings::format("FileActionPath", &[path, old])],
                            ),
                            None => strings::format(key, &[path]),
                        };
                        let reordered =
                            path.reordered() || old.as_ref().is_some_and(|p| p.reordered());
                        let target = message(path.visual(), old.as_ref().map(|p| p.visual()));
                        let response = ui
                            .add(egui::Label::new(&target).truncate())
                            .on_hover_text(&target);
                        if reordered {
                            ctx.accesskit_node_builder(response.id, |node| {
                                node.set_value(message(
                                    path.spoken(),
                                    old.as_ref().map(|p| p.spoken()),
                                ));
                            });
                        }
                    }
                },
            );
            if !current {
                ui.label(text("FileMenuStale"));
            }
            if stash {
                let label = ui.label(text("StashMessage"));
                ui.add(
                    egui::TextEdit::singleline(&mut confirmation.message)
                        .char_limit(4096)
                        .hint_text(text("StashMessage")),
                )
                .labelled_by(label.id);
                if confirmation.message.len() > 4096 || confirmation.message.contains('\0') {
                    ui.label(text("StashMessageLimit"));
                }
            }
            let enabled = current
                && !self.action_busy()
                && (!stash || confirmation.message.len() <= 4096 && !confirmation.message.contains('\0'));
            let (left, right, enter) = if stash {
                (false, false, false)
            } else {
                ui.input_mut(|input| {
                    let left = input.consume_key(egui::Modifiers::NONE, egui::Key::ArrowLeft);
                    let right = input.consume_key(egui::Modifiers::NONE, egui::Key::ArrowRight);
                    let mut enter = false;
                    input.events.retain(|event| {
                        if let egui::Event::Key { key: egui::Key::Enter, pressed: true, repeat, modifiers, .. } = event {
                            enter |= !repeat && modifiers.is_none();
                            return false;
                        }
                        true
                    });
                    (left, right, enter)
                })
            };
            ui.horizontal(|ui| {
                let discard = ui.add_enabled(enabled, egui::Button::new(text(if stash {
                    "FileStashButton"
                } else {
                    "FileDiscardButton"
                })));
                let dismiss = ui.button(text("ActionCancel"));
                if !stash {
                    if left || !enabled && (confirmation.keyboard_focus || discard.has_focus()) {
                        dismiss.request_focus();
                    } else if enabled && (right || confirmation.keyboard_focus) {
                        discard.request_focus();
                    }
                    confirmation.keyboard_focus = false;
                }
                accept = discard.clicked() || enabled && enter && discard.has_focus();
                cancel = dismiss.clicked() || enter && dismiss.has_focus();
            });
        });
        if accept || cancel || response.should_close() {
            if !stash && let Some(id) = self.files_focus {
                ctx.memory_mut(|memory| memory.request_focus(id));
            }
            if accept {
                self.submit_file_action(confirmation, ctx);
            }
        } else {
            self.file_confirmation = Some(confirmation);
        }
    }
    fn submit_file_action(&mut self, confirmation: Confirmation, ctx: &egui::Context) {
        let Confirmation {
            selection,
            action,
            message,
            ..
        } = confirmation;
        if !self.file_selection_current(&selection) || self.action_busy() {
            self.status = Some(text("FileMenuStale"));
            return;
        }
        let (git, tx, dropped, ctx, drop_ctx) = (
            self.git.clone(),
            self.sender.clone(),
            self.sender.clone(),
            ctx.clone(),
            ctx.clone(),
        );
        let context = selection.context;
        let entries = selection.entries;
        let (_, accepted) = self.scheduler.submit(
            context.repository.identity.fingerprint.clone(),
            shovel_core::case::path_key(&context.repository.common_directory),
            match action {
                FileAction::Discard => files::discard_key(&context, &entries),
                FileAction::Stash => files::stash_key(&context, &entries, &message),
                FileAction::ExternalDiff => {
                    format!("diff-tool:{}", files::discard_key(&context, &entries))
                }
                FileAction::Rules(action) => format!(
                    "file-rules:{action:?}:{}",
                    files::discard_key(&context, &entries)
                ),
            },
            Lane::WorkingCopy,
            move || {
                let _ = tx.send(Event::FileStarted(action));
                ctx.request_repaint();
                let result = match action {
                    FileAction::Discard => git.discard_files(&context, &entries),
                    FileAction::Stash => git.stash_files(&context, &entries, &message),
                    FileAction::ExternalDiff => git.external_file_diff(&context, &entries),
                    FileAction::Rules(action) => git.edit_file_rules(&context, &entries, action),
                };
                let outcome = if result.as_ref().is_ok_and(files::Completed::succeeded) {
                    Outcome::Succeeded
                } else {
                    Outcome::Failed
                };
                let _ = tx.send(Event::FileCompleted(action, Box::new(result)));
                ctx.request_repaint();
                outcome
            },
            move |_| {
                let _ = dropped.send(Event::Removed);
                drop_ctx.request_repaint();
            },
        );
        if accepted {
            self.busy = true;
            self.status = Some(text("OperationWaiting"));
            self.output.clear();
            self.operation_output.clear();
        }
    }
    pub(super) fn file_completed(
        &mut self,
        action: FileAction,
        result: Result<files::Completed, operation::Error>,
        ctx: &egui::Context,
    ) {
        self.busy = false;
        self.refresh_history = true;
        self.failed_operation = (!result
            .as_ref()
            .is_ok_and(|r| r.succeeded() && r.updated.is_ok()))
        .then_some(action.phase());
        match result {
            Ok(result) => {
                let partial = !result.succeeded()
                    && (!result.commands.is_empty() || result.filesystem_changes > 0);
                self.status = Some(text(if result.succeeded() {
                    if result.warning.is_some() {
                        "ActionCompletedShort"
                    } else {
                        action.succeeded()
                    }
                } else {
                    result.message.as_deref().unwrap_or("ActionFailedShort")
                }));
                self.output = result.output;
                if let Some(warning) = result.warning {
                    self.status = Some(format!(
                        "{}\n{}",
                        self.status.as_deref().unwrap_or_default(),
                        text(warning)
                    ));
                }
                if partial {
                    self.status = Some(format!(
                        "{}\n{}",
                        self.status.as_deref().unwrap_or_default(),
                        text(action.partial())
                    ));
                }
                if let Some(saved) = &result.saved_stash {
                    self.status = Some(format!(
                        "{}\n{}",
                        self.status.as_deref().unwrap_or_default(),
                        strings::format("RustFileStashSaved", &[saved])
                    ));
                }
                if result.output_truncated {
                    self.output
                        .push_str(&format!("\n{}", text("ActivityOutputLimit")));
                }
                self.operation_output = self.output.clone();
                match result.updated {
                    Ok(context) => self.apply(context, ctx),
                    Err(error) => self.failed_reinspection(error_text(error)),
                }
            }
            Err(error) => {
                self.status = Some(text(&error.code()));
                self.output = crate::operations::error_log(&error);
                self.refresh(ctx);
            }
        }
    }
    pub(super) fn file_shortcuts(&mut self, ctx: &egui::Context) {
        if self.action_busy()
            || self.leave.is_some()
            || self.file_menu.is_some()
            || self.file_confirmation.is_some()
            || self.confirm_amend.is_some()
            || self.files_focus.is_none()
            || ctx.memory(|m| m.focused()) != self.files_focus
        {
            return;
        }
        if !self.working_blocked()
            && let Some(focus) = &self.focused
            && ctx.input_mut(|input| {
                input.modifiers.ctrl
                    && !input.modifiers.shift
                    && !input.modifiers.alt
                    && !input.modifiers.mac_cmd
                    && input.consume_key(egui::Modifiers::CTRL, egui::Key::A)
            })
        {
            let entries = self.context.as_ref().map(|context| context.entries.as_slice());
            let rows = file_list::FileList::from_entries(
                entries.or(self.preview_rows.as_deref()).unwrap_or_default(),
                focus.group == Group::Staged,
            );
            self.selection = rows.entries().map(|entry| (entry.group, entry.path.clone())).collect();
            return;
        }
        enum Shortcut {
            Open,
            Discard,
            Copy(bool),
            Stage(bool),
        }
        let shortcut = ctx.input_mut(|input| {
            let modifiers = input.modifiers;
            if let Some(index) = input
                .events
                .iter()
                .position(|event| matches!(event, egui::Event::Copy))
            {
                // egui-winit replaces the native Ctrl+C / Ctrl+Insert keydown with Copy.
                input.events.remove(index);
                Some(Shortcut::Copy(false))
            } else if modifiers.ctrl
                && modifiers.shift
                && modifiers.alt
                && input.consume_key(modifiers, egui::Key::O)
            {
                Some(Shortcut::Open)
            } else if input.consume_key(egui::Modifiers::NONE, egui::Key::Delete) {
                Some(Shortcut::Discard)
            } else if modifiers.ctrl && !modifiers.alt && input.consume_key(modifiers, egui::Key::C)
            {
                Some(Shortcut::Copy(modifiers.shift))
            } else if !modifiers.ctrl
                && !modifiers.alt
                && input.consume_key(modifiers, egui::Key::Enter)
            {
                Some(Shortcut::Stage(modifiers.shift))
            } else if modifiers.ctrl
                && modifiers.shift
                && modifiers.alt
                && input.consume_key(modifiers, egui::Key::S)
            {
                Some(Shortcut::Stage(false))
            } else {
                None
            }
        });
        let (Some(shortcut), Some(context), Some(focus)) =
            (shortcut, self.context.as_ref(), self.focused.as_ref())
        else {
            return;
        };
        let staged = focus.group == Group::Staged;
        let entries = self.selected_entries(context, staged);
        if entries.is_empty() {
            return;
        }
        let selection = Selection {
            context: context.clone(),
            entries,
        };
        if self.snapshot_error.is_some() && !matches!(shortcut, Shortcut::Copy(_)) {
            return;
        }
        match shortcut {
            Shortcut::Open => self.begin_shell(selection, false, ctx),
            Shortcut::Discard if files::supported(&selection.entries) => {
                self.file_confirmation = Some(Confirmation::new(selection, FileAction::Discard));
            }
            Shortcut::Discard => {}
            Shortcut::Copy(full) => self.copy_paths(&selection, full, ctx),
            Shortcut::Stage(all) => self.stage(staged, all, ctx),
        }
    }
}
