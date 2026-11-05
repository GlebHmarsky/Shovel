#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod commit;
mod strings;

fn main() -> eframe::Result {
    velopack::VelopackApp::build().run();
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1200., 800.])
            .with_min_inner_size([900., 620.]),
        ..Default::default()
    };
    eframe::run_native(
        &strings::text("AppTitle"),
        options,
        Box::new(|context| Ok(Box::new(Workspace::new(context)))),
    )
}

use eframe::egui::{self, Color32, RichText};
use shovel_core::{
    git::{Cursor, Git, History, Page},
    process::Cancellation,
    settings::{Settings, SettingsStore},
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
    Initialized(
        Box<Settings>,
        Box<SettingsStore>,
        Result<Git, String>,
        Option<String>,
    ),
    Open(PathBuf),
    Loaded(u64, Box<History>, Box<Page>),
    Page(u64, Box<Page>),
    Failed(u64, String),
    Saved(Box<SettingsStore>, Result<(), String>),
}
struct Tab {
    id: u64,
    path: PathBuf,
    history: Option<History>,
    page: Option<Page>,
    selected: Option<String>,
    previous: Vec<Cursor>,
    loading: bool,
    error: Option<String>,
}
struct Workspace {
    settings: Settings,
    store: Option<SettingsStore>,
    git: Option<Arc<Git>>,
    tabs: Vec<Tab>,
    selected: usize,
    next_id: u64,
    sender: Sender<Event>,
    receiver: Receiver<Event>,
    notice: Option<String>,
    initialized: bool,
    saving: bool,
    exit_ready: bool,
    confirm_exit_without_save: bool,
    scheduler: shovel_core::scheduler::Scheduler,
    editors: Vec<commit::Editor>,
    draft_store: Option<shovel_core::drafts::Store>,
}
impl Workspace {
    fn new(context: &eframe::CreationContext<'_>) -> Self {
        let (sender, receiver) = mpsc::channel();
        let output = sender.clone();
        let ctx = context.egui_ctx.clone();
        theme(&ctx, false);
        thread::spawn(move || {
            let local = std::env::var_os("LOCALAPPDATA")
                .map(PathBuf::from)
                .unwrap_or_else(std::env::temp_dir);
            let directory = std::env::args_os()
                .skip(1)
                .collect::<Vec<_>>()
                .windows(2)
                .find(|a| a[0] == "--data-directory")
                .map(|a| PathBuf::from(&a[1]))
                .filter(|p| p.is_absolute())
                .unwrap_or_else(|| local.join("ShovelRust/Foundation"));
            let mut store = SettingsStore::new(directory);
            let (settings, notice) =
                match store.load_or_import(&local.join("GitGui/Foundation/settings.json")) {
                    Ok((settings, _)) => (settings, None),
                    Err(_) => (Settings::default(), Some(strings::text("SettingsInvalid"))),
                };
            let git = Git::discover(None, &Cancellation::default()).map_err(error_text);
            let _ = output.send(Event::Initialized(
                Box::new(settings),
                Box::new(store),
                git,
                notice,
            ));
            ctx.request_repaint();
        });
        Self {
            settings: Settings::default(),
            store: None,
            git: None,
            tabs: vec![],
            selected: 0,
            next_id: 1,
            sender,
            receiver,
            notice: None,
            initialized: false,
            saving: false,
            exit_ready: false,
            confirm_exit_without_save: false,
            scheduler: Default::default(),
            editors: vec![],
            draft_store: None,
        }
    }
    fn open(&mut self, path: PathBuf, ctx: &egui::Context) {
        if let Some(index) = self.tabs.iter().position(|t| t.path == path) {
            self.selected = index;
            return;
        }
        let id = self.next_id;
        self.next_id += 1;
        self.tabs.push(Tab {
            id,
            path,
            history: None,
            page: None,
            selected: None,
            previous: vec![],
            loading: false,
            error: None,
        });
        self.selected = self.tabs.len() - 1;
        self.refresh(ctx);
    }
    fn refresh(&mut self, ctx: &egui::Context) {
        self.refresh_at(self.selected, ctx);
    }
    fn refresh_at(&mut self, index: usize, ctx: &egui::Context) {
        let Some(git) = self.git.clone() else {
            return;
        };
        let Some(tab) = self.tabs.get_mut(index) else {
            return;
        };
        if tab.loading {
            return;
        }
        tab.loading = true;
        tab.error = None;
        let path = tab.path.clone();
        let id = tab.id;
        let sender = self.sender.clone();
        let ctx = ctx.clone();
        thread::spawn(move || {
            let cancel = Cancellation::default();
            let result = git
                .inspect(&path, &cancel)
                .and_then(|snapshot| git.history(snapshot, &cancel))
                .and_then(|history| {
                    git.page(&history, &Cursor::default(), &cancel)
                        .map(|page| (history, page))
                });
            let event = match result {
                Ok((history, page)) => Event::Loaded(id, Box::new(history), Box::new(page)),
                Err(error) => Event::Failed(id, error_text(error)),
            };
            let _ = sender.send(event);
            ctx.request_repaint();
        });
    }
    fn page(&mut self, older: bool, ctx: &egui::Context) {
        let Some(git) = self.git.clone() else {
            return;
        };
        let Some(tab) = self.tabs.get_mut(self.selected) else {
            return;
        };
        if tab.loading {
            return;
        }
        let (Some(history), Some(page)) = (&tab.history, &tab.page) else {
            return;
        };
        let cursor = if older {
            let Some(next) = page.next.clone() else {
                return;
            };
            tab.previous.push(page.cursor.clone());
            next
        } else {
            let Some(previous) = tab.previous.pop() else {
                return;
            };
            previous
        };
        tab.loading = true;
        let history = history.clone();
        let id = tab.id;
        let sender = self.sender.clone();
        let ctx = ctx.clone();
        thread::spawn(move || {
            let result = git.page(&history, &cursor, &Cancellation::default());
            let _ = sender.send(match result {
                Ok(page) => Event::Page(id, Box::new(page)),
                Err(error) => Event::Failed(id, error_text(error)),
            });
            ctx.request_repaint();
        });
    }
    fn pick_repository(&self, ctx: &egui::Context) {
        let sender = self.sender.clone();
        let ctx = ctx.clone();
        thread::spawn(move || {
            if let Some(path) = rfd::FileDialog::new().pick_folder() {
                let _ = sender.send(Event::Open(path));
                ctx.request_repaint();
            }
        });
    }
    fn receive(&mut self, ctx: &egui::Context) {
        while let Ok(event) = self.receiver.try_recv() {
            match event {
                Event::Initialized(settings, store, git, notice) => {
                    self.settings = *settings;
                    let local = std::env::var_os("LOCALAPPDATA")
                        .map(PathBuf::from)
                        .unwrap_or_default();
                    self.draft_store = Some(shovel_core::drafts::Store {
                        directory: store.path().parent().unwrap().join("CommitDrafts"),
                        legacy_directory: Some(local.join("GitGui/CommitDrafts")),
                    });
                    self.store = Some(*store);
                    self.initialized = true;
                    self.notice = notice;
                    ctx.set_zoom_factor(self.settings.ui_scale() as f32);
                    theme(
                        ctx,
                        self.settings
                            .get("Appearance")
                            .and_then(serde_json::Value::as_u64)
                            == Some(1),
                    );
                    match git {
                        Ok(git) => self.git = Some(Arc::new(git)),
                        Err(error) => self.notice = Some(error),
                    }
                    let paths = self.settings.repositories();
                    for path in paths {
                        let id = self.next_id;
                        self.next_id += 1;
                        self.tabs.push(Tab {
                            id,
                            path,
                            history: None,
                            page: None,
                            selected: None,
                            previous: vec![],
                            loading: false,
                            error: None,
                        });
                    }
                    self.selected = self
                        .settings
                        .get("SelectedRepositoryPath")
                        .and_then(serde_json::Value::as_str)
                        .and_then(|p| self.tabs.iter().position(|t| t.path == *p))
                        .unwrap_or(0);
                    self.refresh(ctx);
                }
                Event::Open(path) => self.open(path, ctx),
                Event::Loaded(id, history, page) => {
                    if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == id) {
                        tab.history = Some(*history);
                        tab.page = Some(*page);
                        tab.previous.clear();
                        tab.loading = false;
                    }
                }
                Event::Page(id, page) => {
                    if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == id) {
                        tab.page = Some(*page);
                        tab.loading = false;
                    }
                }
                Event::Failed(id, error) => {
                    if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == id) {
                        tab.loading = false;
                        tab.error = Some(error);
                    }
                }
                Event::Saved(store, result) => {
                    self.store = Some(*store);
                    self.saving = false;
                    match result {
                        Ok(()) => {
                            self.exit_ready = true;
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                        Err(error) => {
                            self.notice = Some(error);
                            self.confirm_exit_without_save = true;
                        }
                    }
                }
            }
        }
        let mut changed = vec![];
        for editor in &mut self.editors {
            if editor.poll(ctx) {
                changed.push(editor.path.clone());
            }
        }
        for path in changed {
            if let Some(index) = self.tabs.iter().position(|tab| tab.path == path) {
                self.refresh_at(index, ctx);
            }
        }
    }
    fn open_commit(&mut self) {
        let Some(tab) = self.tabs.get(self.selected) else {
            return;
        };
        if let Some(editor) = self
            .editors
            .iter_mut()
            .find(|editor| editor.path == tab.path)
        {
            editor.activate();
            return;
        }
        if let (Some(git), Some(store)) = (&self.git, &self.draft_store) {
            self.editors.push(commit::Editor::new(
                tab.path.clone(),
                git.clone(),
                self.scheduler.clone(),
                store.clone(),
            ));
        }
    }
    fn close(&mut self, ctx: &egui::Context) {
        if self.exit_ready {
            return;
        }
        ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        if self.scheduler.has_accepted_work() {
            self.notice = Some(strings::text("AppCommandRunningClose"));
            return;
        }
        for editor in &mut self.editors {
            editor.flush(ctx);
        }
        if self.editors.iter().any(commit::Editor::blocks_exit) {
            self.notice = Some(
                self.editors
                    .iter()
                    .find_map(|editor| editor.draft_problem().map(str::to_owned))
                    .unwrap_or_else(|| strings::text("CommitDraftSaving")),
            );
            return;
        }
        if self.saving || !self.initialized {
            return;
        }
        let Some(mut store) = self.store.take() else {
            return;
        };
        self.settings.set(
            "RepositoryPaths",
            serde_json::json!(
                self.tabs
                    .iter()
                    .map(|t| t.path.to_string_lossy())
                    .collect::<Vec<_>>()
            ),
        );
        self.settings.set(
            "SelectedRepositoryPath",
            serde_json::json!(
                self.tabs
                    .get(self.selected)
                    .map(|t| t.path.to_string_lossy())
            ),
        );
        self.settings
            .set("UiScale", serde_json::json!(ctx.zoom_factor()));
        if let Some(rect) = ctx.input(|i| i.viewport().inner_rect) {
            self.settings
                .set("WindowWidth", serde_json::json!(rect.width()));
            self.settings
                .set("WindowHeight", serde_json::json!(rect.height()));
        }
        let settings = self.settings.clone();
        self.saving = true;
        let sender = self.sender.clone();
        let ctx = ctx.clone();
        thread::spawn(move || {
            let result = store
                .save(&settings)
                .map_err(|_| strings::text("SettingsSaveFailedStatus"));
            let _ = sender.send(Event::Saved(Box::new(store), result));
            ctx.request_repaint();
        });
    }
}
fn error_text(error: shovel_core::git::Error) -> String {
    match error {
        shovel_core::git::Error::Command { output, .. } => {
            format!("{}\n{output}", strings::text("HistoryFailed"))
        }
        shovel_core::git::Error::MissingGit => strings::text("GitErrorMissingGit"),
        shovel_core::git::Error::Invalid => strings::text("GitErrorInvalidOutput"),
        shovel_core::git::Error::Changed => strings::text("GitErrorChangedDuringRead"),
        shovel_core::git::Error::Io(_) => strings::text("IdentityUnavailable"),
        _ => strings::text("HistoryFailed"),
    }
}
fn theme(ctx: &egui::Context, light: bool) {
    let mut visuals = if light {
        egui::Visuals::light()
    } else {
        egui::Visuals::dark()
    };
    if !light {
        visuals.panel_fill = Color32::from_rgb(34, 34, 34);
        visuals.window_fill = Color32::from_rgb(50, 50, 50);
        visuals.extreme_bg_color = Color32::from_rgb(39, 39, 39);
        visuals.override_text_color = Some(Color32::from_rgb(228, 228, 228));
        visuals.selection.bg_fill = Color32::from_rgb(36, 101, 170);
    }
    ctx.set_visuals(visuals);
}
impl eframe::App for Workspace {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.receive(ctx);
    }
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let modal_open = self.editors.iter().any(|editor| editor.open);
        if modal_open || self.saving {
            ui.disable();
        }
        if self.confirm_exit_without_save {
            egui::Window::new(strings::text("SettingsSaveFailedStatus"))
                .collapsible(false)
                .resizable(false)
                .show(&ctx, |ui| {
                    ui.label(strings::text("SettingsSaveFailedQuestion"));
                    ui.horizontal(|ui| {
                        if ui.button(strings::text("RustExitWithoutSaving")).clicked() {
                            self.exit_ready = true;
                            self.confirm_exit_without_save = false;
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                        if ui.button(strings::text("RustStayOpen")).clicked() {
                            self.confirm_exit_without_save = false;
                        }
                    });
                });
        }
        if ctx.input(|i| i.viewport().close_requested()) {
            self.close(&ctx);
        }
        if !modal_open && ctx.input(|i| i.key_pressed(egui::Key::F5)) {
            self.refresh(&ctx);
        }
        if !modal_open && ctx.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::PageDown)) {
            self.page(true, &ctx);
        }
        if !modal_open && ctx.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::PageUp)) {
            self.page(false, &ctx);
        }
        egui::Panel::top("toolbar").show(ui, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                ui.menu_button(strings::text("FileMenu"), |ui| {
                    if ui.button(strings::text("OpenRepository")).clicked() {
                        self.pick_repository(&ctx);
                        ui.close();
                    }
                });
                ui.menu_button(strings::text("ViewMenu"), |ui| {
                    if ui.button(strings::text("RefreshRepository")).clicked() {
                        self.refresh(&ctx);
                        ui.close();
                    }
                    if ui.button(strings::text("LoadOlderCommits")).clicked() {
                        self.page(true, &ctx);
                        ui.close();
                    }
                    if ui.button(strings::text("NewerCommits")).clicked() {
                        self.page(false, &ctx);
                        ui.close();
                    }
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(strings::text("AppTitle"));
                });
            });
            ui.horizontal(|ui| {
                if ui.button(strings::text("OpenRepository")).clicked() {
                    self.pick_repository(&ctx);
                }
                if ui.button(strings::text("RefreshRepository")).clicked() {
                    self.refresh(&ctx);
                }
                if ui
                    .add_enabled(
                        self.git.is_some() && !self.tabs.is_empty(),
                        egui::Button::new(strings::text("Commit")),
                    )
                    .clicked()
                {
                    self.open_commit();
                }
                if let Some(tab) = self.tabs.get(self.selected) {
                    ui.label(tab.path.file_name().unwrap_or_default().to_string_lossy());
                    if let Some(history) = &tab.history {
                        ui.label(history.snapshot.branch.as_deref().unwrap_or("HEAD"));
                    }
                    if tab.loading {
                        ui.label(strings::text("HistoryLoading"));
                    }
                }
                if let Some(notice) = &self.notice {
                    ui.colored_label(Color32::LIGHT_RED, notice);
                }
            });
            if let Some(tab) = self.tabs.get(self.selected)
                && let Some(editor) = self
                    .editors
                    .iter_mut()
                    .find(|editor| editor.path == tab.path)
                && (editor.status.is_some() || !editor.output.is_empty())
            {
                ui.horizontal(|ui| {
                    if let Some(status) = &editor.status {
                        ui.label(status);
                    }
                    if editor.failed_commit && ui.button(strings::text("ReopenCommit")).clicked() {
                        editor.activate();
                    }
                    if !editor.output.is_empty() {
                        egui::CollapsingHeader::new(strings::text("OperationAttentionLogs")).show(
                            ui,
                            |ui| {
                                egui::ScrollArea::both().max_height(170.).show(ui, |ui| {
                                    ui.add(
                                        egui::Label::new(RichText::new(&editor.output).monospace())
                                            .selectable(true),
                                    );
                                });
                            },
                        );
                    }
                });
            }
            let count = self.tabs.len();
            let width = if count == 0 {
                0.
            } else {
                (ui.available_width() - ui.spacing().item_spacing.x * (count - 1) as f32)
                    / count as f32
            };
            let mut selected = None;
            ui.horizontal(|ui| {
                for (index, tab) in self.tabs.iter().enumerate() {
                    let label = tab.path.file_name().unwrap_or_default().to_string_lossy();
                    let response = ui.add_sized(
                        [width, 26.],
                        egui::Button::new(label).selected(index == self.selected),
                    );
                    if response.clicked() {
                        selected = Some(index);
                    }
                    response.on_hover_text(tab.path.to_string_lossy());
                }
            });
            if let Some(index) = selected {
                self.selected = index;
                if self.tabs[index].history.is_none() {
                    self.refresh(&ctx);
                }
            }
        });
        egui::Panel::left("sidebar")
            .default_size(210.)
            .min_size(160.)
            .show(ui, |ui| {
                if let Some(tab) = self.tabs.get_mut(self.selected)
                    && let Some(history) = &tab.history
                {
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        for (key, prefix) in [
                            ("Branches", "refs/heads/"),
                            ("Remotes", "refs/remotes/"),
                            ("Tags", "refs/tags/"),
                            ("Stashes", "stash@{"),
                        ] {
                            egui::CollapsingHeader::new(strings::text(key))
                                .default_open(true)
                                .show(ui, |ui| {
                                    for reference in history
                                        .snapshot
                                        .references
                                        .iter()
                                        .filter(|r| r.name.starts_with(prefix))
                                    {
                                        if ui
                                            .selectable_label(
                                                tab.selected.as_deref() == Some(&reference.hash),
                                                reference
                                                    .name
                                                    .strip_prefix(prefix)
                                                    .unwrap_or(&reference.name),
                                            )
                                            .clicked()
                                        {
                                            tab.selected = Some(reference.hash.clone());
                                        }
                                    }
                                });
                        }
                    });
                }
            });
        egui::Panel::bottom("details")
            .resizable(true)
            .default_size(180.)
            .min_size(80.)
            .show(ui, |ui| {
                if let Some(tab) = self.tabs.get(self.selected)
                    && let Some(entry) = tab.page.as_ref().and_then(|p| {
                        p.entries
                            .iter()
                            .find(|e| Some(&e.commit.hash) == tab.selected.as_ref())
                    })
                {
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.strong(&entry.commit.author);
                            ui.label(entry.commit.authored.to_rfc3339());
                        });
                        ui.horizontal(|ui| {
                            ui.monospace(&entry.commit.hash);
                            if ui.button(strings::text("HistoryCopySha")).clicked() {
                                ctx.copy_text(entry.commit.hash.clone());
                            }
                        });
                        ui.label(&entry.commit.message);
                    });
                }
            });
        egui::CentralPanel::default().show(ui, |ui| {
            let Some(tab) = self.tabs.get_mut(self.selected) else {
                ui.centered_and_justified(|ui| {
                    ui.vertical(|ui| {
                        ui.heading(strings::text("HistoryEmptyTitle"));
                        ui.label(strings::text("HistoryEmptyBody"));
                    });
                });
                return;
            };
            if let Some(error) = &tab.error {
                ui.colored_label(Color32::LIGHT_RED, error);
            }
            let Some(page) = &tab.page else {
                return;
            };
            if page.entries.is_empty() {
                ui.label(strings::text("HistoryEmptyAll"));
            }
            egui::ScrollArea::vertical().id_salt(tab.id).show_rows(
                ui,
                23.,
                page.entries.len(),
                |ui, range| {
                    for index in range {
                        let entry = &page.entries[index];
                        ui.horizontal(|ui| {
                            let graph_width = entry.graph.width.max(2) as f32 * 12.;
                            let (rect, _) = ui.allocate_exact_size(
                                egui::vec2(graph_width, 23.),
                                egui::Sense::hover(),
                            );
                            let colors = [
                                Color32::from_rgb(225, 128, 96),
                                Color32::from_rgb(100, 181, 246),
                                Color32::from_rgb(129, 199, 132),
                                Color32::from_rgb(186, 104, 200),
                                Color32::from_rgb(255, 213, 79),
                            ];
                            let point = |lane: usize, y: f32| {
                                egui::pos2(
                                    rect.left() + 6. + lane as f32 * 12.,
                                    rect.top() + y * 23.,
                                )
                            };
                            for line in &entry.graph.lines {
                                ui.painter().line_segment(
                                    [point(line.from, line.from_y), point(line.to, line.to_y)],
                                    egui::Stroke::new(1.7, colors[line.color % colors.len()]),
                                );
                            }
                            let center = point(entry.graph.node, 0.5);
                            let color = colors[entry.graph.color % colors.len()];
                            if entry.graph.is_merge || entry.graph.is_stash {
                                ui.painter().circle_stroke(
                                    center,
                                    3.5,
                                    egui::Stroke::new(1.7, color),
                                );
                            } else {
                                ui.painter().circle_filled(center, 3.5, color);
                            }
                            for reference in &entry.references {
                                ui.label(
                                    RichText::new(
                                        reference
                                            .name
                                            .trim_start_matches("refs/heads/")
                                            .trim_start_matches("refs/remotes/")
                                            .trim_start_matches("refs/tags/"),
                                    )
                                    .color(Color32::LIGHT_BLUE),
                                );
                            }
                            let response = ui.selectable_label(
                                tab.selected.as_deref() == Some(&entry.commit.hash),
                                entry.commit.subject(),
                            );
                            if response.clicked() {
                                tab.selected = Some(entry.commit.hash.clone());
                            }
                            response.on_hover_text(&entry.commit.message);
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    ui.monospace(
                                        &entry.commit.hash[..entry.commit.hash.len().min(8)],
                                    );
                                    ui.label(entry.commit.authored.format("%Y-%m-%d").to_string());
                                    ui.label(&entry.commit.author);
                                },
                            );
                        });
                    }
                },
            );
        });
        for editor in &mut self.editors {
            editor.show(&ctx);
        }
    }
}
