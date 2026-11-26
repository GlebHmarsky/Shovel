//! Preparation-free remote forms and repository-owned secondary configuration.
use crate::{
    operations,
    strings::{self, text},
    workspace_style::{self, Palette},
};
use eframe::egui;
use shovel_core::{
    git::{Git, Snapshot},
    process::Cancellation,
    remotes::{Action, Configuration, valid},
    scheduler::Scheduler,
};
use std::{
    sync::{
        Arc,
        mpsc::{self, Receiver},
    },
    thread,
};

#[derive(Default)]
pub struct Cache {
    pub value: Option<Configuration>,
    pub error: Option<String>,
    receiver: Option<Receiver<Result<Configuration, String>>>,
    cancel: Cancellation,
}
impl Drop for Cache {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}
impl Cache {
    pub fn invalidate(&mut self) {
        self.cancel.cancel();
        self.receiver = None; // old workers cannot publish into a new generation
    }
    pub fn start(
        &mut self,
        git: Arc<Git>,
        snapshot: &Snapshot,
        ctx: &egui::Context,
        workers: &crate::read_workers::Workers,
    ) {
        self.invalidate();
        self.cancel = Cancellation::default();
        let cancel = self.cancel.clone();
        let snapshot = crate::history_cache::metadata(snapshot);
        let (sender, receiver) = mpsc::channel();
        self.receiver = Some(receiver);
        let ctx = ctx.clone();
        let lease = workers.start(cancel.clone());
        thread::spawn(move || {
            let _lease = lease;
            #[cfg(test)]
            crate::history_cache::tests::wait_for_read(&snapshot.worktree);
            let result = git
                .remote_configuration(&snapshot, &cancel)
                .map_err(|e| text(&e.code()));
            let _ = sender.send(result);
            ctx.request_repaint();
        });
    }
    pub fn poll(&mut self, ctx: &egui::Context) -> bool {
        let mut changed = false;
        if let Some(result) = self.receiver.as_ref().and_then(|r| r.try_recv().ok()) {
            self.receiver = None;
            match result {
                Ok(value) => {
                    self.value = Some(value);
                    self.error = None;
                    changed = true;
                }
                Err(error) => self.error = Some(error),
            }
        }
        if self.receiver.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(50));
        }
        changed
    }
    pub fn bytes(&self) -> usize {
        self.value.as_ref().map_or(0, |v| {
            v.remotes
                .iter()
                .map(|r| 256 + 2 * (r.name.len() + r.location.len() + r.push_location.len()))
                .sum()
        })
    }
}
enum Form {
    Add {
        names: Vec<String>,
        name: String,
        location: String,
    },
    Manage {
        reviewed: Configuration,
        selected: usize,
        location: String,
        push: String,
        confirm: bool,
    },
}
struct Dialog {
    snapshot: Snapshot,
    form: Form,
    id: egui::Id,
    focus: bool,
}
#[derive(Default)]
pub struct Dialogs {
    active: Option<Dialog>,
}
impl Dialogs {
    pub fn is_open(&self) -> bool {
        self.active.is_some()
    }
    pub fn add(&mut self, snapshot: &Snapshot) {
        self.open(
            snapshot,
            Form::Add {
                names: snapshot.remotes.clone(),
                name: if snapshot.remotes.iter().any(|s| s == "origin") {
                    "upstream"
                } else {
                    "origin"
                }
                .into(),
                location: String::new(),
            },
        );
    }
    pub fn manage(&mut self, snapshot: &Snapshot, reviewed: &Configuration, name: Option<&str>) {
        let selected = reviewed
            .remotes
            .iter()
            .position(|r| Some(r.name.as_str()) == name)
            .unwrap_or(0);
        let remote = reviewed.remotes.get(selected);
        self.open(
            snapshot,
            Form::Manage {
                reviewed: reviewed.clone(),
                selected,
                location: remote.map(|r| r.location.clone()).unwrap_or_default(),
                push: remote.map(|r| r.push_location.clone()).unwrap_or_default(),
                confirm: false,
            },
        );
    }
    fn open(&mut self, snapshot: &Snapshot, form: Form) {
        self.active = Some(Dialog {
            snapshot: crate::history_cache::metadata(snapshot),
            form,
            id: egui::Id::unique(("remote-options", uuid::Uuid::new_v4())),
            focus: true,
        });
    }
    pub fn menu(
        &mut self,
        ui: &mut egui::Ui,
        snapshot: Option<&Snapshot>,
        cache: Option<&Cache>,
        name: Option<&str>,
    ) {
        if ui
            .add_enabled(
                snapshot.is_some(),
                egui::Button::new(text("RemoteAddTitle")),
            )
            .on_disabled_hover_text(text("ActionWaitingShort"))
            .clicked()
        {
            self.add(snapshot.unwrap());
            ui.close();
        }
        let value = cache.and_then(|c| c.value.as_ref());
        if ui
            .add_enabled(
                snapshot.is_some() && value.is_some(),
                egui::Button::new(text("RemoteManageMenu")),
            )
            .on_disabled_hover_text(
                cache
                    .and_then(|c| c.error.clone())
                    .unwrap_or_else(|| text("RemoteManageLoading")),
            )
            .clicked()
        {
            self.manage(snapshot.unwrap(), value.unwrap(), name);
            ui.close();
        }
    }
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        git: Option<&Arc<Git>>,
        operations: &mut operations::Controller,
        scheduler: &Scheduler,
        enabled: bool,
    ) {
        let Some(dialog) = &mut self.active else {
            return;
        };
        let Some(git) = git else { return };
        let mut close = false;
        let mut submitted = None;
        let mut visible = false;
        let title = match dialog.form {
            Form::Add { .. } => "RemoteAddTitle",
            Form::Manage { confirm: true, .. } => "ActionTitleRemoveRemote",
            Form::Manage { .. } => "RemoteManageTitle",
        };
        let (_, dismissed) =
            workspace_style::show_dialog(ctx, dialog.id, &text(title), 465., enabled, true, |ui| {
                visible = ui.is_visible() && !ui.is_sizing_pass();
                let path = dialog.snapshot.worktree.to_string_lossy();
                ui.add(egui::Label::new(path.as_ref()).truncate())
                    .on_hover_text(path.as_ref());
                match &mut dialog.form {
                    Form::Add {
                        names,
                        name,
                        location,
                    } => {
                        let readiness = workspace_style::dialog_body(ui, |ui| {
                            remote_field(ui, "RemoteAddName", name);
                            let input = remote_field(ui, "RemoteAddLocation", location);
                            if dialog.focus && visible {
                                input.request_focus();
                            }
                            let readiness = if names.iter().any(|n| n == name.trim()) {
                                "RemoteAddExists"
                            } else if !valid(name.trim(), location.trim()) {
                                "RemoteAddInvalid"
                            } else {
                                "RemoteAddReady"
                            };
                            if readiness != "RemoteAddReady" {
                                ui.weak(text(readiness));
                            }
                            readiness
                        });
                        workspace_style::dialog_actions(ui, |ui| {
                            close |= ui.button(text("Cancel")).clicked();
                            if ui
                                .add_enabled(
                                    readiness == "RemoteAddReady",
                                    egui::Button::new(text("RemoteAddSubmit")),
                                )
                                .clicked()
                            {
                                submitted = Some((
                                    "RemoteAddSubmit",
                                    Action::Add {
                                        name: name.trim().into(),
                                        location: location.trim().into(),
                                    },
                                ));
                            }
                        });
                    }
                    Form::Manage {
                        reviewed,
                        selected,
                        location,
                        push,
                        confirm,
                    } => {
                        if *confirm {
                            let remote = reviewed.remotes.get(*selected);
                            workspace_style::dialog_body(ui, |ui| {
                                if let Some(remote) = remote {
                                    ui.label(strings::format(
                                        "RemoteRemoveReview",
                                        &[&remote.name],
                                    ));
                                }
                            });
                            workspace_style::dialog_actions(ui, |ui| {
                                if ui.button(text("Cancel")).clicked() {
                                    *confirm = false;
                                }
                                let remove = egui::Button::new(
                                    egui::RichText::new(text("ActionTitleRemoveRemote"))
                                        .color(Palette::current(ui).diff_removed_text),
                                );
                                if ui.add_enabled(remote.is_some(), remove).clicked()
                                    && let Some(remote) = remote
                                {
                                    submitted = Some((
                                        "ActionTitleRemoveRemote",
                                        Action::Change {
                                            reviewed: reviewed.clone(),
                                            name: remote.name.clone(),
                                            location: String::new(),
                                            push_location: String::new(),
                                            remove: true,
                                        },
                                    ));
                                }
                            });
                            return;
                        }
                        let (remote, readiness) = workspace_style::dialog_body(ui, |ui| {
                            if reviewed.remotes.is_empty() {
                                ui.weak(text("RemoteManageEmpty"));
                                return (None, None);
                            }
                            let before = *selected;
                            let label = ui.label(text("RemoteAddName"));
                            let picker = egui::ComboBox::from_id_salt("remote")
                                .width(ui.available_width())
                                .selected_text(&reviewed.remotes[*selected].name)
                                .show_ui(ui, |ui| {
                                    for (i, remote) in reviewed.remotes.iter().enumerate() {
                                        ui.selectable_value(selected, i, &remote.name);
                                    }
                                });
                            let input = picker.response.labelled_by(label.id);
                            if dialog.focus && visible {
                                input.request_focus();
                            }
                            let remote = &reviewed.remotes[*selected];
                            if *selected != before {
                                location.clone_from(&remote.location);
                                push.clone_from(&remote.push_location);
                            }
                            ui.add_enabled_ui(remote.can_edit, |ui| {
                                remote_field(ui, "RemoteFetchUrl", location);
                                remote_field(ui, "RemotePushUrl", push);
                            });
                            let problem = if !remote.can_edit {
                                Some("RemoteManageUnsupported")
                            } else if !valid(&remote.name, location.trim())
                                || !push.trim().is_empty() && !valid(&remote.name, push.trim())
                            {
                                Some("RemoteAddInvalid")
                            } else {
                                None
                            };
                            if let Some(problem) = problem {
                                ui.weak(text(problem));
                            }
                            (Some(remote), problem)
                        });
                        workspace_style::dialog_actions(ui, |ui| {
                            close |= ui.button(text("Cancel")).clicked();
                            if ui
                                .add_enabled(
                                    remote.is_some() && readiness.is_none(),
                                    egui::Button::new(text("RemoteSave")),
                                )
                                .clicked()
                                && let Some(remote) = remote
                            {
                                submitted = Some((
                                    "ActionTitleEditRemote",
                                    Action::Change {
                                        reviewed: reviewed.clone(),
                                        name: remote.name.clone(),
                                        location: location.trim().into(),
                                        push_location: push.trim().into(),
                                        remove: false,
                                    },
                                ));
                            }
                            if ui
                                .add_enabled(
                                    remote.is_some(),
                                    egui::Button::new(text("ActionTitleRemoveRemote")),
                                )
                                .clicked()
                            {
                                *confirm = true;
                            }
                        });
                    }
                }
            });
        if dismissed && let Form::Manage { confirm, .. } = &mut dialog.form && *confirm {
            *confirm = false;
        } else {
            close |= dismissed;
        }
        if visible {
            dialog.focus = false;
        }
        if let Some((title, action)) = submitted {
            operations.submit_remote(title, action, &dialog.snapshot, git.clone(), scheduler, ctx);
            close = true;
        }
        if close {
            self.active = None;
        }
    }
}

fn remote_field(ui: &mut egui::Ui, key: &str, value: &mut String) -> egui::Response {
    let label = ui.label(text(key));
    ui.add(egui::TextEdit::singleline(value).desired_width(f32::INFINITY))
        .labelled_by(label.id)
}

#[cfg(test)]
#[path = "remote_tests.rs"]
mod tests;
