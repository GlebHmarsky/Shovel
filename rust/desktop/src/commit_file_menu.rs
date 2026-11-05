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
    pub all: Vec<Change>,
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
}
impl Confirmation {
    fn new(selection: Selection, action: FileAction) -> Self {
        Self {
            selection,
            action,
            message: String::new(),
        }
    }
}
impl Editor {
    fn file_selection_current(&self, selection: &Selection) -> bool {
        self.context
            .as_ref()
            .is_some_and(|c| Arc::ptr_eq(c, &selection.context))
    }
    pub(super) fn begin_file_menu(&mut self, menu: Menu) {
        if self
            .conflict
            .as_ref()
            .is_some_and(|p| p.path != menu.entry.path || menu.entry.group != Group::Conflicted)
        {
            self.conflict = None;
        }
        self.selection = menu
            .selected
            .entries
            .iter()
            .map(|e| (e.group, e.path.clone()))
            .collect();
        self.focused = Some(menu.entry.clone());
        self.diff_cancel.cancel();
        self.diff_generation += 1;
        self.diff = None;
        self.diff_error = None;
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
                        .map(|p| p.to_string_lossy().into_owned())
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
        let enabled = current && !self.action_busy() && !self.loading;
        let reason = text(if !current {
            "FileMenuStale"
        } else {
            "CommitWaitForRead"
        });
        let staged = menu.entry.group == Group::Staged;
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
                    enabled && !menu.selected.entries.is_empty(),
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
                    enabled && !menu.all.is_empty(),
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
                        menu.selected.entries.clone()
                    } else {
                        menu.all.clone()
                    };
                    self.submit(
                        if staged {
                            Action::Unstage(entries)
                        } else {
                            Action::Stage(entries)
                        },
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
            ui.set_width(620.);
            ui.heading(text(if stash {
                "FileStashTitle"
            } else {
                "FileDiscardTitle"
            }));
            ui.label(strings::format(
                if stash {
                    "FileStashExplanation"
                } else {
                    "FileDiscardExplanation"
                },
                &[
                    selection
                        .context
                        .repository
                        .head
                        .as_deref()
                        .unwrap_or(&text("NoBranch")),
                    &selection.context.repository.worktree.to_string_lossy(),
                ],
            ));
            egui::ScrollArea::vertical().max_height(260.).show_rows(
                ui,
                42.,
                selection.entries.len(),
                |ui, range| {
                    for index in range {
                        let entry = &selection.entries[index];
                        let path = entry
                            .original
                            .as_ref()
                            .filter(|_| entry.code == 'R')
                            .map(|old| strings::format("FileActionPath", &[&entry.path, old]))
                            .unwrap_or_else(|| entry.path.clone());
                        ui.label(strings::format(
                            if stash {
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
                            },
                            &[&path],
                        ));
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
            ui.horizontal(|ui| {
                accept = ui
                    .add_enabled(
                        current
                            && !self.action_busy()
                            && !self.loading
                            && (!stash
                                || confirmation.message.len() <= 4096
                                    && !confirmation.message.contains('\0')),
                        egui::Button::new(text(if stash {
                            "FileStashButton"
                        } else {
                            "FileDiscardButton"
                        })),
                    )
                    .clicked();
                cancel = ui.button(text("ActionCancel")).clicked();
            });
        });
        if accept {
            self.submit_file_action(confirmation, ctx);
        } else if !cancel && !response.should_close() {
            self.file_confirmation = Some(confirmation);
        }
    }
    fn submit_file_action(&mut self, confirmation: Confirmation, ctx: &egui::Context) {
        let Confirmation {
            selection,
            action,
            message,
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
            context
                .repository
                .common_directory
                .to_string_lossy()
                .to_uppercase(),
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
                    Err(error) => {
                        self.status = Some(format!(
                            "{}\n{}",
                            self.status.as_deref().unwrap_or_default(),
                            error_text(error)
                        ))
                    }
                }
            }
            Err(error) => {
                self.status = Some(text(&error.code()));
                self.refresh(ctx);
            }
        }
    }
    pub(super) fn file_shortcuts(&mut self, ctx: &egui::Context) {
        if self.action_busy()
            || self.loading
            || self.leave.is_some()
            || self.file_menu.is_some()
            || self.file_confirmation.is_some()
            || self.confirm_amend
            || self.files_focus.is_none()
            || ctx.memory(|m| m.focused()) != self.files_focus
        {
            return;
        }
        let Some(context) = self.context.clone() else {
            return;
        };
        let Some(focus) = self.focused.clone() else {
            return;
        };
        let entries = context
            .entries
            .iter()
            .filter(|e| {
                self.selection.contains(&(e.group, e.path.clone()))
                    && (e.group == Group::Staged) == (focus.group == Group::Staged)
            })
            .cloned()
            .collect::<Vec<_>>();
        if entries.is_empty() {
            return;
        }
        let selection = Selection { context, entries };
        let modifiers = ctx.input(|i| i.modifiers);
        if modifiers.ctrl
            && modifiers.shift
            && modifiers.alt
            && ctx.input_mut(|i| i.consume_key(modifiers, egui::Key::O))
        {
            self.begin_shell(selection, false, ctx);
        } else if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Delete))
            && files::supported(&selection.entries)
        {
            self.file_confirmation = Some(Confirmation::new(selection, FileAction::Discard));
        } else if modifiers.ctrl
            && !modifiers.alt
            && ctx.input_mut(|i| i.consume_key(modifiers, egui::Key::C))
        {
            self.copy_paths(&selection, modifiers.shift, ctx);
        } else if !modifiers.ctrl
            && !modifiers.alt
            && ctx.input_mut(|i| i.consume_key(modifiers, egui::Key::Enter))
        {
            self.stage(focus.group == Group::Staged, modifiers.shift, ctx);
        } else if modifiers.ctrl
            && modifiers.shift
            && modifiers.alt
            && ctx.input_mut(|i| i.consume_key(modifiers, egui::Key::S))
        {
            self.stage(focus.group == Group::Staged, false, ctx);
        }
    }
}
