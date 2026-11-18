//! Local repository creation owns its worker independently of any visible tab.
use crate::{
    commit::{Capture, OwnedActivity},
    strings::text,
};
use eframe::egui;
use sha2::{Digest, Sha256};
use shovel_core::{
    cloning,
    git::Git,
    initialization::{self, Options},
    operation::Error,
    scheduler::{Lane, Outcome, Scheduler},
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
    Picked(Option<PathBuf>),
    Started,
    Output(bool, Vec<u8>),
    Completed(Box<Result<initialization::Completed, Error>>),
    Removed,
}
#[derive(Clone, Copy, Default, PartialEq)]
enum Kind {
    #[default]
    Initialize,
    Clone,
}
impl Kind {
    fn title(self) -> &'static str {
        if self == Self::Clone {
            "ActionTitleClone"
        } else {
            "ActionTitleInit"
        }
    }
}
enum Request {
    Initialize(Options),
    Clone(cloning::Options),
}
impl Request {
    fn directory(&self) -> &std::path::Path {
        match self {
            Self::Initialize(o) => &o.directory,
            Self::Clone(o) => &o.directory,
        }
    }
}
struct Form {
    source: String,
    directory: String,
    branch: String,
}
pub struct Controller {
    kind: Kind,
    capture: Capture,
    form: Option<Form>,
    pub busy: bool,
    pub picking: bool,
    pub summary: Option<String>,
    activity: Option<OwnedActivity>,
    path: PathBuf,
    request_id: u64,
    origin: Option<u64>,
    sender: Sender<Event>,
    receiver: Receiver<Event>,
    picker: Arc<dyn Fn() -> Option<PathBuf> + Send + Sync>,
}
pub struct Completion {
    pub path: Option<PathBuf>,
    pub origin: Option<u64>,
    pub notice: Option<String>,
}
impl Default for Controller {
    fn default() -> Self {
        let (sender, receiver) = mpsc::channel();
        Self {
            kind: Kind::Initialize,
            capture: Default::default(),
            form: None,
            busy: false,
            picking: false,
            summary: None,
            activity: None,
            path: PathBuf::new(),
            request_id: 0,
            origin: None,
            sender,
            receiver,
            picker: Arc::new(|| {
                rfd::FileDialog::new()
                    .set_title(text("InitChooseFolder"))
                    .pick_folder()
            }),
        }
    }
}
impl Controller {
    pub fn is_open(&self) -> bool {
        self.form.is_some() || self.picking
    }
    pub fn open(&mut self) {
        self.open_kind(Kind::Initialize);
    }
    pub fn open_clone(&mut self) {
        self.open_kind(Kind::Clone);
    }
    fn open_kind(&mut self, kind: Kind) {
        if !self.busy {
            self.kind = kind;
            self.form = Some(Form {
                source: String::new(),
                directory: String::new(),
                branch: String::new(),
            });
        }
    }
    pub fn take_activity(&mut self) -> Option<(PathBuf, OwnedActivity)> {
        self.activity.take().map(|a| (self.path.clone(), a))
    }
    pub fn poll(&mut self, ctx: &egui::Context) -> Option<Completion> {
        let mut completed = None;
        while let Ok(event) = self.receiver.try_recv() {
            match event {
                Event::Picked(path) => {
                    self.picking = false;
                    if let (Some(form), Some(path)) = (&mut self.form, path) {
                        form.directory = path.to_string_lossy().into_owned();
                    }
                }
                Event::Started => {
                    self.summary = Some(text(self.kind.title()));
                    self.activity = Some(OwnedActivity {
                        request: self.request(),
                        label: self.kind.title(),
                        status: text(self.kind.title()),
                        output: String::new(),
                        done: false,
                        failed: false,
                        reopen_commit: false,
                    });
                }
                Event::Output(stderr, bytes) => {
                    self.capture.append(stderr, &bytes);
                    self.activity = Some(OwnedActivity {
                        request: self.request(),
                        label: self.kind.title(),
                        status: text(self.kind.title()),
                        output: self.capture.text.clone(),
                        done: false,
                        failed: false,
                        reopen_commit: false,
                    });
                }
                Event::Completed(result) => {
                    self.busy = false;
                    let (path, status, output, notice) = match *result {
                        Ok(result) => {
                            let status = text(result.message);
                            let notice = (!result.succeeded || result.repository.is_none())
                                .then(|| status.clone());
                            (
                                result.repository.map(|r| r.worktree),
                                status,
                                result.output,
                                notice,
                            )
                        }
                        Err(error) => {
                            let output = if let Error::Git(shovel_core::git::Error::Command {
                                output,
                                ..
                            }) = &error
                            {
                                shovel_core::network::redact(output)
                            } else {
                                String::new()
                            };
                            let status = text(&error.code());
                            (None, status.clone(), output, Some(status))
                        }
                    };
                    // The final operation result owns complete output. The streaming
                    // fallback can retain a partial line if a pipe was interrupted.
                    self.capture.finish(false);
                    let output = if output.is_empty() {
                        &self.capture.text
                    } else {
                        &output
                    };
                    self.summary = Some(status.clone());
                    if let Some(activity) = &mut self.activity {
                        activity.done = true;
                        activity.failed = notice.is_some();
                        activity.status = status;
                        activity.output = output.chars().take(32768).collect();
                    } else {
                        // The pending record may already have been forwarded to Activity.
                        self.activity = Some(OwnedActivity {
                            request: self.request(),
                            label: self.kind.title(),
                            status,
                            output: output.chars().take(32768).collect(),
                            done: true,
                            failed: notice.is_some(),
                            reopen_commit: false,
                        });
                    }
                    completed = Some(Completion {
                        path,
                        origin: self.origin,
                        notice,
                    });
                }
                Event::Removed => {
                    self.busy = false;
                    self.summary = Some(text("QueuedRequestCancelled"));
                    self.activity = Some(OwnedActivity {
                        request: self.request(),
                        label: self.kind.title(),
                        status: text("QueuedRequestCancelled"),
                        output: String::new(),
                        done: true,
                        failed: false,
                        reopen_commit: false,
                    });
                }
            }
        }
        if self.busy || self.picking {
            ctx.request_repaint_after(std::time::Duration::from_millis(50));
        }
        completed
    }
    fn request(&self) -> u64 {
        self.request_id
    }
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        git: Option<&Arc<Git>>,
        scheduler: &Scheduler,
        origin: Option<u64>,
        operations: &crate::operations::Controller,
    ) {
        let Some(mut form) = self.form.take() else {
            return;
        };
        let mut close = false;
        let mut create = false;
        let ready = if self.kind == Kind::Clone {
            cloning::Options {
                source: form.source.trim().into(),
                directory: form.directory.trim().into(),
                branch: form.branch.trim().into(),
            }
            .valid()
        } else {
            initialization::destination(&PathBuf::from(&form.directory)).is_ok()
        };
        let response =
            egui::Modal::new(egui::Id::unique("initialize_repository")).show(ctx, |ui| {
                ui.set_width(560.);
                ui.heading(text(self.kind.title()));
                ui.label(text(if self.kind == Kind::Clone {
                    "ActionExplainClone"
                } else {
                    "ActionExplainInit"
                }));
                ui.separator();
                if self.kind == Kind::Clone {
                    let source_label = ui.label(text("CloneSource"));
                    ui.add_enabled(
                        !self.picking,
                        egui::TextEdit::singleline(&mut form.source).desired_width(f32::INFINITY),
                    )
                    .labelled_by(source_label.id);
                }
                let directory_label = ui.label(text(if self.kind == Kind::Clone {
                    "CloneDirectory"
                } else {
                    "InitDirectory"
                }));
                ui.horizontal(|ui| {
                    ui.add_enabled(
                        !self.picking,
                        egui::TextEdit::singleline(&mut form.directory).desired_width(440.),
                    )
                    .labelled_by(directory_label.id);
                    if ui
                        .add_enabled(!self.picking, egui::Button::new(text("InitBrowse")))
                        .clicked()
                    {
                        self.picking = true;
                        let picker = self.picker.clone();
                        let tx = self.sender.clone();
                        let ctx = ctx.clone();
                        thread::spawn(move || {
                            let _ = tx.send(Event::Picked(picker()));
                            ctx.request_repaint();
                        });
                    }
                });
                let branch_label = ui.label(text(if self.kind == Kind::Clone {
                    "CloneBranch"
                } else {
                    "InitBranch"
                }));
                ui.add_enabled(
                    !self.picking,
                    egui::TextEdit::singleline(&mut form.branch).desired_width(f32::INFINITY),
                )
                .labelled_by(branch_label.id);
                if self.kind == Kind::Initialize {
                    ui.label(text("InitBranchHelp"));
                }
                ui.label(text(match (self.kind, ready) {
                    (Kind::Initialize, true) => "InitReady",
                    (Kind::Initialize, false) => "InitEnterDirectory",
                    (Kind::Clone, true) => "CloneReady",
                    (Kind::Clone, false) => "CloneInvalid",
                }));
                ui.separator();
                ui.horizontal(|ui| {
                    create = ui
                        .add_enabled(
                            ready && !self.picking && git.is_some(),
                            egui::Button::new(text(if self.kind == Kind::Clone {
                                "CloneSubmit"
                            } else {
                                "InitCreate"
                            })),
                        )
                        .clicked();
                    close = ui
                        .add_enabled(!self.picking, egui::Button::new(text("ActionCancel")))
                        .clicked();
                });
            });
        if !self.picking && ready && ctx.input(|i| i.key_pressed(egui::Key::Enter)) {
            create = true;
        }
        if create && let Some(git) = git {
            self.submit(
                if self.kind == Kind::Clone {
                    Request::Clone(cloning::Options {
                        source: form.source.trim().into(),
                        directory: form.directory.trim().into(),
                        branch: form.branch.trim().into(),
                    })
                } else {
                    Request::Initialize(Options {
                        directory: form.directory.into(),
                        initial_branch: form.branch,
                    })
                },
                origin,
                git.clone(),
                scheduler,
                ctx,
                operations,
            );
        } else if !close && (!response.should_close() || self.picking) {
            self.form = Some(form);
        }
    }
    fn submit(
        &mut self,
        options: Request,
        origin: Option<u64>,
        git: Arc<Git>,
        scheduler: &Scheduler,
        ctx: &egui::Context,
        operations: &crate::operations::Controller,
    ) {
        if self.busy {
            return;
        }
        let target = initialization::destination(options.directory())
            .unwrap_or_else(|_| options.directory().to_path_buf());
        let owner = format!(
            "init:{:x}",
            Sha256::digest(shovel_core::case::path_key(&target).as_bytes())
        );
        let tx = self.sender.clone();
        let drop_ctx = ctx.clone();
        let (request, accepted) = scheduler.reserve(
            owner.clone(),
            owner,
            "init".into(),
            Lane::Exclusive,
            move |_| {
                let _ = tx.send(Event::Removed);
                drop_ctx.request_repaint();
            },
        );
        if !accepted {
            return;
        }
        self.capture = Default::default();
        self.request_id = request;
        self.path = target;
        self.origin = origin;
        self.busy = true;
        self.summary = Some(text("OperationWaiting"));
        self.activity = Some(OwnedActivity {
            request,
            label: self.kind.title(),
            status: text("OperationWaiting"),
            output: String::new(),
            done: false,
            failed: false,
            reopen_commit: false,
        });
        let tx = self.sender.clone();
        let ctx = ctx.clone();
        let authentication = operations.external_authentication();
        scheduler.activate(request, move || {
            let _ = tx.send(Event::Started);
            ctx.request_repaint();
            let result = match options {
                Request::Initialize(options) => git.initialize_repository(&options),
                Request::Clone(options) => {
                    let sender = tx.clone();
                    let repaint = ctx.clone();
                    let progress = Arc::new(move |stderr, bytes: &[u8]| {
                        let _ = sender.send(Event::Output(stderr, bytes.to_vec()));
                        repaint.request_repaint();
                    });
                    authentication
                        .start(request, options.directory.clone(), "ActionTitleClone", &ctx)
                        .and_then(|auth| {
                            git.clone_repository(&options, Some(&auth), Some(progress))
                        })
                }
            };
            let outcome = if result
                .as_ref()
                .is_ok_and(|r| r.succeeded && r.repository.is_some())
            {
                Outcome::Succeeded
            } else {
                Outcome::Failed
            };
            let _ = tx.send(Event::Completed(Box::new(result)));
            ctx.request_repaint();
            outcome
        });
    }
}

#[cfg(test)]
#[path = "repository_init_tests.rs"]
mod tests;
