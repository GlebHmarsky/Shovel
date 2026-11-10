//! Preparation-free remote forms and repository-owned secondary configuration.
use crate::{
    operations,
    strings::{self, text},
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
        self.error = None;
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
    viewport: egui::ViewportId,
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
            viewport: egui::ViewportId::from_hash_of(("remote-options", uuid::Uuid::new_v4())),
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
                egui::Button::new(text("RemoteManageTitle")),
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
        ctx.show_viewport_immediate(
            dialog.viewport,
            egui::ViewportBuilder::default()
                .with_title(text(title))
                .with_inner_size(if matches!(dialog.form, Form::Add { .. }) {
                    [540., 340.]
                } else {
                    [660., 460.]
                })
                .with_resizable(false)
                .with_minimize_button(false)
                .with_maximize_button(false),
            |ui, _| {
                visible |= ui.is_visible();
                if !enabled {
                    ui.disable();
                }
                if ui.ctx().input(|i| {
                    i.viewport().close_requested() || enabled && i.key_pressed(egui::Key::Escape)
                }) {
                    close = true;
                }
                egui::CentralPanel::default()
                    .frame(crate::workspace_style::pane(
                        crate::workspace_style::Palette::current(ui).sidebar,
                        20,
                        16,
                    ))
                    .show(ui, |ui| {
                        ui.heading(text(title));
                        ui.add(
                            egui::Label::new(dialog.snapshot.worktree.to_string_lossy()).truncate(),
                        )
                        .on_hover_text(dialog.snapshot.worktree.to_string_lossy());
                        ui.add_space(12.);
                        egui::ScrollArea::vertical().show(ui, |ui| match &mut dialog.form {
                            Form::Add {
                                names,
                                name,
                                location,
                            } => {
                                let label = ui.label(text("RemoteAddName"));
                                ui.text_edit_singleline(name).labelled_by(label.id);
                                let label = ui.label(text("RemoteAddLocation"));
                                let input = ui.text_edit_singleline(location).labelled_by(label.id);
                                if dialog.focus {
                                    input.request_focus();
                                }
                                let readiness = if names.iter().any(|n| n == name.trim()) {
                                    "RemoteAddExists"
                                } else if !valid(name.trim(), location.trim()) {
                                    "RemoteAddInvalid"
                                } else {
                                    "RemoteAddReady"
                                };
                                ui.add_space(10.);
                                ui.label(text(readiness));
                                ui.horizontal(|ui| {
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
                                    if ui.button(text("Cancel")).clicked() {
                                        close = true;
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
                                    if let Some(remote) = reviewed.remotes.get(*selected) {
                                        ui.label(strings::format(
                                            "RemoteRemoveConfirm",
                                            &[
                                                &remote.name,
                                                &dialog.snapshot.worktree.to_string_lossy(),
                                            ],
                                        ));
                                        ui.add_space(16.);
                                        ui.horizontal(|ui| {
                                            if ui.button(text("ActionTitleRemoveRemote")).clicked()
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
                                            if ui.button(text("Cancel")).clicked() {
                                                close = true;
                                            }
                                        });
                                    }
                                    return;
                                }
                                let before = *selected;
                                egui::ComboBox::from_label(text("RemoteAddName"))
                                    .selected_text(
                                        reviewed
                                            .remotes
                                            .get(*selected)
                                            .map(|r| r.name.as_str())
                                            .unwrap_or_default(),
                                    )
                                    .show_ui(ui, |ui| {
                                        for (i, remote) in reviewed.remotes.iter().enumerate() {
                                            ui.selectable_value(selected, i, &remote.name);
                                        }
                                    });
                                let remote = reviewed.remotes.get(*selected);
                                if *selected != before
                                    && let Some(remote) = remote
                                {
                                    *location = remote.location.clone();
                                    *push = remote.push_location.clone();
                                }
                                let editable = remote.is_some_and(|r| r.can_edit);
                                ui.add_enabled_ui(editable, |ui| {
                                    let label = ui.label(text("RemoteFetchUrl"));
                                    ui.add(
                                        egui::TextEdit::singleline(location)
                                            .desired_width(f32::INFINITY),
                                    )
                                    .labelled_by(label.id);
                                    let label = ui.label(text("RemotePushUrl"));
                                    ui.add(
                                        egui::TextEdit::singleline(push)
                                            .desired_width(f32::INFINITY),
                                    )
                                    .labelled_by(label.id);
                                });
                                let readiness = match remote {
                                    None => "RemoteManageEmpty",
                                    Some(_) if !editable => "RemoteManageUnsupported",
                                    Some(r)
                                        if !valid(&r.name, location.trim())
                                            || !push.trim().is_empty()
                                                && !valid(&r.name, push.trim()) =>
                                    {
                                        "RemoteAddInvalid"
                                    }
                                    _ => "RemoteManageReady",
                                };
                                ui.add_space(10.);
                                ui.label(text(readiness));
                                ui.horizontal(|ui| {
                                    if ui
                                        .add_enabled(
                                            remote.is_some(),
                                            egui::Button::new(text("ActionTitleRemoveRemote")),
                                        )
                                        .clicked()
                                    {
                                        *confirm = true;
                                    }
                                    if ui
                                        .add_enabled(
                                            readiness == "RemoteManageReady",
                                            egui::Button::new(text("RemoteSave")),
                                        )
                                        .clicked()
                                    {
                                        submitted = Some((
                                            "ActionTitleEditRemote",
                                            Action::Change {
                                                reviewed: reviewed.clone(),
                                                name: remote.unwrap().name.clone(),
                                                location: location.trim().into(),
                                                push_location: push.trim().into(),
                                                remove: false,
                                            },
                                        ));
                                    }
                                    if ui.button(text("Cancel")).clicked() {
                                        close = true;
                                    }
                                });
                            }
                        });
                    });
            },
        );
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

#[cfg(test)]
#[path = "remote_tests.rs"]
mod tests;
