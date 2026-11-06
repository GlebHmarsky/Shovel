#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod branches;
mod commit;
mod comparison;
mod conflict;
mod diff_view;
mod file_history;
mod file_picker;
mod file_shell;
mod history_row;
mod operations;
mod stashes;
mod strings;
mod workspace_style;

fn main() -> eframe::Result {
    velopack::VelopackApp::build()
        .set_auto_apply_on_startup(false)
        .run();
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_icon(
                eframe::icon_data::from_png_bytes(include_bytes!(
                    "../../../src/GitGui.App/Assets/Shovel.png"
                ))
                .expect("checked application icon"),
            )
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

use eframe::egui::{self, RichText};
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
    Page(u64, Box<Page>, bool),
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
    refresh_again: bool,
    error: Option<String>,
    comparison: Option<comparison::Pane>,
}
struct HistoryMenu {
    owner: u64,
    snapshot: shovel_core::git::Snapshot,
    commit: shovel_core::git::Commit,
}
struct ReferenceMenu {
    owner: u64,
    snapshot: shovel_core::git::Snapshot,
    reference: shovel_core::git::Reference,
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
    preferences_dirty: bool,
    closing: bool,
    pending_exit: bool,
    exit_ready: bool,
    confirm_exit_without_save: bool,
    scheduler: shovel_core::scheduler::Scheduler,
    editors: Vec<commit::Editor>,
    draft_store: Option<shovel_core::drafts::Store>,
    operations: operations::Controller,
    branches: branches::Dialogs,
    stash_views: Vec<stashes::View>,
    history_menu: Option<HistoryMenu>,
    reference_menu: Option<ReferenceMenu>,
}
impl Workspace {
    fn new(context: &eframe::CreationContext<'_>) -> Self {
        let (sender, receiver) = mpsc::channel();
        let output = sender.clone();
        let ctx = context.egui_ctx.clone();
        theme(&ctx, false);
        let font_ctx = ctx.clone();
        thread::spawn(move || workspace_style::install_windows_font(&font_ctx));
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
            preferences_dirty: false,
            closing: false,
            pending_exit: false,
            exit_ready: false,
            confirm_exit_without_save: false,
            scheduler: Default::default(),
            editors: vec![],
            draft_store: None,
            operations: Default::default(),
            branches: Default::default(),
            stash_views: vec![],
            history_menu: None,
            reference_menu: None,
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
            refresh_again: false,
            error: None,
            comparison: None,
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
            tab.refresh_again = true;
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
            next
        } else {
            let Some(previous) = tab.previous.last().cloned() else {
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
                Ok(page) => Event::Page(id, Box::new(page), older),
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
                            refresh_again: false,
                            error: None,
                            comparison: None,
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
                    if self.tabs.get(self.selected).is_some_and(|tab| tab.id == id) {
                        egui::Popup::close_all(ctx);
                        self.history_menu = None;
                        self.reference_menu = None;
                    }
                    if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == id) {
                        if !page
                            .entries
                            .iter()
                            .any(|e| Some(&e.commit.hash) == tab.selected.as_ref())
                        {
                            tab.selected = page.entries.first().map(|e| e.commit.hash.clone());
                        }
                        for view in &mut self.stash_views {
                            if view.path == history.snapshot.worktree {
                                view.observe(&history.snapshot);
                            }
                        }
                        tab.history = Some(*history);
                        tab.page = Some(*page);
                        tab.previous.clear();
                        tab.loading = false;
                    }
                }
                Event::Page(id, page, older) => {
                    if self.tabs.get(self.selected).is_some_and(|tab| tab.id == id) {
                        egui::Popup::close_all(ctx);
                        self.history_menu = None;
                        self.reference_menu = None;
                    }
                    if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == id) {
                        if older {
                            if let Some(current) = &tab.page {
                                tab.previous.push(current.cursor.clone());
                            }
                        } else {
                            tab.previous.pop();
                        }
                        if !page
                            .entries
                            .iter()
                            .any(|e| Some(&e.commit.hash) == tab.selected.as_ref())
                        {
                            tab.selected = page.entries.first().map(|e| e.commit.hash.clone());
                        }
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
                            if self.closing && !self.preferences_dirty {
                                self.exit_ready = true;
                                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                            }
                        }
                        Err(error) => {
                            self.notice = Some(error);
                            self.confirm_exit_without_save = self.closing;
                            self.closing = false;
                        }
                    }
                }
            }
        }
        let mut changed = self.operations.poll(ctx);
        for editor in &mut self.editors {
            if changed.iter().any(|path| path == &editor.path) {
                editor.external_changed();
            }
            if editor.poll(ctx) {
                changed.push(editor.path.clone());
            }
        }
        for view in &mut self.stash_views {
            if changed.iter().any(|path| path == &view.path) {
                view.changed();
            }
            view.poll(ctx, &self.scheduler);
        }
        for path in changed {
            if let Some(index) = self.tabs.iter().position(|tab| tab.path == path) {
                self.refresh_at(index, ctx);
            }
        }
        for tab in &mut self.tabs {
            if let Some(pane) = &mut tab.comparison {
                pane.poll(ctx);
            }
        }
        let pending = self
            .tabs
            .iter_mut()
            .enumerate()
            .filter_map(|(index, tab)| {
                (!tab.loading && std::mem::take(&mut tab.refresh_again)).then_some(index)
            })
            .collect::<Vec<_>>();
        for index in pending {
            self.refresh_at(index, ctx);
        }
        if self.preferences_dirty && !self.saving {
            self.save_preferences(ctx);
        }
        if self
            .editors
            .iter_mut()
            .any(commit::Editor::take_exit_cancelled)
        {
            self.pending_exit = false;
        }
        if self.pending_exit && !self.editors.iter().any(commit::Editor::exit_prompt_open) {
            self.close(ctx);
        }
    }
    fn open_stashes(&mut self) {
        let Some(git) = self.git.clone() else {
            return;
        };
        let Some(history) = self
            .tabs
            .get(self.selected)
            .and_then(|t| t.history.as_ref())
        else {
            return;
        };
        let snapshot = history.snapshot.clone();
        let view = if let Some(index) = self
            .stash_views
            .iter()
            .position(|v| v.path == snapshot.worktree)
        {
            let mut view = self.stash_views.remove(index);
            view.activate(&snapshot);
            view
        } else {
            stashes::View::new(snapshot, git)
        };
        self.stash_views.push(view);
        for view in self.stash_views.iter_mut().rev().skip(3) {
            view.release_cache();
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
            let mut editor = commit::Editor::new(
                tab.path.clone(),
                git.clone(),
                self.scheduler.clone(),
                store.clone(),
            );
            editor.external_merge_tool = self
                .settings
                .get("ExternalMergeTool")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default();
            self.editors.push(editor);
        }
    }
    fn close(&mut self, ctx: &egui::Context) {
        if self.exit_ready {
            return;
        }
        ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        if self.editors.iter().any(commit::Editor::native_picker_open) {
            self.pending_exit = false;
            self.notice = Some(strings::text("RustPatchChooserOpen"));
            return;
        }
        if self.scheduler.has_accepted_work() {
            self.pending_exit = false;
            self.notice = Some(strings::text("AppCommandRunningClose"));
            return;
        }
        if let Some(editor) = self.editors.iter_mut().find(|e| e.unsaved_conflict()) {
            self.pending_exit = true;
            editor.confirm_exit(ctx);
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
        if !self.initialized {
            return;
        }
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
        self.closing = true;
        self.pending_exit = false;
        self.preferences_dirty = true;
        self.save_preferences(ctx);
    }
    fn save_preferences(&mut self, ctx: &egui::Context) {
        let Some(mut store) = self.store.take() else {
            return;
        };
        self.preferences_dirty = false;
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
    workspace_style::apply(ctx, light);
}
impl eframe::App for Workspace {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.receive(ctx);
    }
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let modal_open = self.editors.iter().any(|editor| editor.open)
            || self.operations.modal_open()
            || self.branches.is_open()
            || self.stash_views.iter().any(|view| view.open);
        if modal_open || self.closing {
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
        if ctx.input(|i| {
            i.events
                .iter()
                .any(|event| matches!(event, egui::Event::WindowFocused(true)))
        }) {
            theme(
                &ctx,
                self.settings
                    .get("Appearance")
                    .and_then(serde_json::Value::as_u64)
                    == Some(1),
            );
        }
        let palette = workspace_style::Palette::current(ui);
        egui::Panel::top("toolbar")
            .frame(workspace_style::pane(palette.chrome, 6, 2))
            .show(ui, |ui| {
                egui::MenuBar::new().ui(ui, |ui| {
                    ui.menu_button(strings::text("FileMenu"), |ui| {
                        if ui.button(strings::text("OpenRepository")).clicked() {
                            self.pick_repository(&ctx);
                            ui.close();
                        }
                    });
                    ui.menu_button(strings::text("ViewMenu"), |ui| {
                        ui.menu_button(strings::text("WorkspaceAppearance"), |ui| {
                            for (value, key) in [(0, "WorkspaceDark"), (1, "WorkspaceLight")] {
                                let current = self
                                    .settings
                                    .get("Appearance")
                                    .and_then(serde_json::Value::as_u64)
                                    .unwrap_or(0);
                                if ui
                                    .selectable_label(current == value, strings::text(key))
                                    .clicked()
                                {
                                    self.settings.set("Appearance", serde_json::json!(value));
                                    theme(&ctx, value == 1);
                                    self.preferences_dirty = true;
                                    ui.close();
                                }
                            }
                        });
                        ui.separator();
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
                    ui.menu_button(strings::text("RepositoryMenu"), |ui| {
                        if ui
                            .add_enabled(
                                !self.tabs.is_empty(),
                                egui::Button::new(strings::text("RefreshRepository")),
                            )
                            .clicked()
                        {
                            self.refresh(&ctx);
                            ui.close();
                        }
                        if ui
                            .add_enabled(
                                self.tabs
                                    .get(self.selected)
                                    .is_some_and(|t| t.history.is_some()),
                                egui::Button::new(strings::text("Stashes")),
                            )
                            .clicked()
                        {
                            self.open_stashes();
                            ui.close();
                        }
                    });
                    ui.add_enabled_ui(!self.tabs.is_empty(), |ui| {
                        ui.menu_button(strings::text("WindowMenu"), |ui| {
                            for (index, tab) in self.tabs.iter().enumerate() {
                                if ui
                                    .selectable_label(
                                        index == self.selected,
                                        tab.path.to_string_lossy(),
                                    )
                                    .clicked()
                                {
                                    self.selected = index;
                                    self.history_menu = None;
                                    self.reference_menu = None;
                                    ui.close();
                                    if self.tabs[index].history.is_none() {
                                        self.refresh(&ctx);
                                    }
                                    break;
                                }
                            }
                        });
                    });
                });
                ui.allocate_ui_with_layout(
                    egui::vec2(ui.available_width(), 48.),
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| {
                        ui.spacing_mut().item_spacing.x = 2.;
                        if workspace_style::action(ui, "OpenRepository", true).clicked() {
                            self.pick_repository(&ctx);
                        }
                        self.preferences_dirty |= self.operations.toolbar(
                            ui,
                            &mut self.settings,
                            self.tabs
                                .get(self.selected)
                                .and_then(|t| t.history.as_ref())
                                .map(|h| &h.snapshot),
                            self.git.as_ref(),
                            &self.scheduler,
                        );
                        if workspace_style::action(
                            ui,
                            "Commit",
                            self.git.is_some() && !self.tabs.is_empty(),
                        )
                        .clicked()
                        {
                            self.open_commit();
                        }
                        let context_width = (ui.available_width() - 150.).clamp(90., 530.);
                        let margin = ((ui.available_width() - 150. - context_width) / 2.).max(0.);
                        ui.add_space(margin);
                        egui::Frame::new()
                            .fill(palette.raised)
                            .stroke(egui::Stroke::new(1., palette.border))
                            .corner_radius(6)
                            .inner_margin(egui::Margin::symmetric(10, 4))
                            .show(ui, |ui| {
                                ui.set_width((context_width - 22.).max(1.));
                                ui.vertical_centered(|ui| {
                                    if let Some(tab) = self.tabs.get(self.selected) {
                                        ui.add(
                                            egui::Label::new(
                                                RichText::new(
                                                    tab.path
                                                        .file_name()
                                                        .unwrap_or_default()
                                                        .to_string_lossy(),
                                                )
                                                .strong(),
                                            )
                                            .truncate(),
                                        )
                                        .on_hover_text(tab.path.to_string_lossy());
                                        if let Some(history) = &tab.history {
                                            ui.add(
                                                egui::Label::new(
                                                    RichText::new(
                                                        history
                                                            .snapshot
                                                            .branch
                                                            .as_deref()
                                                            .unwrap_or("HEAD"),
                                                    )
                                                    .color(palette.muted),
                                                )
                                                .truncate(),
                                            );
                                        } else if tab.loading {
                                            ui.label(
                                                RichText::new(strings::text("HistoryLoading"))
                                                    .color(palette.muted),
                                            );
                                        }
                                    } else {
                                        ui.label(RichText::new(strings::text("AppTitle")).strong());
                                    }
                                });
                            });
                        ui.add_space(margin);
                        self.branches.toolbar(
                            ui,
                            self.tabs
                                .get(self.selected)
                                .and_then(|t| t.history.as_ref())
                                .map(|h| &h.snapshot),
                        );
                        if workspace_style::action(
                            ui,
                            "Stashes",
                            self.tabs
                                .get(self.selected)
                                .is_some_and(|t| t.history.is_some()),
                        )
                        .on_disabled_hover_text(strings::text("ActionInspectionNeeded"))
                        .clicked()
                        {
                            self.open_stashes();
                        }
                    },
                );
                if let Some(notice) = &self.notice {
                    ui.colored_label(palette.diff_removed_text, notice);
                }
                if let Some(tab) = self.tabs.get(self.selected) {
                    self.operations.status(ui, &tab.path, &self.scheduler);
                    if let Some(history) = &tab.history
                        && !history.snapshot.operations.is_empty()
                    {
                        let snapshot = &history.snapshot;
                        if let Some(state) = &snapshot.recovery {
                            ui.horizontal_wrapped(|ui| {
                                ui.label(strings::format(
                                    "RecoveryHelp",
                                    &[&branches::recovery_name(state.kind)],
                                ));
                                if ui
                                    .button(strings::text("ActionTitleContinue"))
                                    .on_hover_text(strings::text("RecoveryContinueHelp"))
                                    .clicked()
                                    && let Some(git) = &self.git
                                {
                                    self.operations.submit_recovery(
                                        "ActionTitleContinue",
                                        shovel_core::recovery::Action::Continue,
                                        snapshot,
                                        git.clone(),
                                        &self.scheduler,
                                        &ctx,
                                    );
                                }
                                if state.can_skip()
                                    && ui
                                        .button(strings::text("ActionTitleSkip"))
                                        .on_hover_text(strings::text("RecoverySkipHelp"))
                                        .clicked()
                                {
                                    self.branches.confirm_recovery(
                                        snapshot,
                                        shovel_core::recovery::Action::Skip,
                                    );
                                }
                                if ui
                                    .button(strings::text("ActionTitleAbort"))
                                    .on_hover_text(strings::text("RecoveryAbortHelp"))
                                    .clicked()
                                {
                                    self.branches.confirm_recovery(
                                        snapshot,
                                        shovel_core::recovery::Action::Abort,
                                    );
                                }
                            });
                        } else {
                            ui.label(strings::text("RecoveryUnavailable"));
                        }
                    }
                }
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
                        if editor.failed_commit
                            && ui.button(strings::text("ReopenCommit")).clicked()
                        {
                            editor.activate();
                        }
                        if !editor.output.is_empty() {
                            egui::CollapsingHeader::new(strings::text("OperationAttentionLogs"))
                                .show(ui, |ui| {
                                    egui::ScrollArea::both().max_height(170.).show(ui, |ui| {
                                        ui.add(
                                            egui::Label::new(
                                                RichText::new(&editor.output).monospace(),
                                            )
                                            .selectable(true),
                                        );
                                    });
                                });
                        }
                    });
                }
            });
        egui::Panel::top("repository-tabs")
            .frame(workspace_style::pane(palette.tab_strip, 4, 0))
            .show(ui, |ui| {
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
                            egui::Button::new(RichText::new(label).color(palette.text))
                                .truncate()
                                .selected(index == self.selected)
                                .fill(if index == self.selected {
                                    palette.sidebar
                                } else {
                                    palette.tab_strip
                                })
                                .corner_radius(egui::CornerRadius {
                                    nw: 5,
                                    ne: 5,
                                    sw: 0,
                                    se: 0,
                                }),
                        );
                        if response.clicked() {
                            selected = Some(index);
                        }
                        response.on_hover_text(tab.path.to_string_lossy());
                    }
                });
                if let Some(index) = selected {
                    egui::Popup::close_all(&ctx);
                    self.history_menu = None;
                    self.reference_menu = None;
                    self.selected = index;
                    if self.tabs[index].history.is_none() {
                        self.refresh(&ctx);
                    }
                }
            });
        egui::Panel::left("sidebar")
            .frame(workspace_style::pane(palette.sidebar, 8, 4))
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
                            egui::CollapsingHeader::new(
                                RichText::new(strings::text(key))
                                    .strong()
                                    .color(palette.muted),
                            )
                            .default_open(true)
                            .show(ui, |ui| {
                                for reference in history
                                    .snapshot
                                    .references
                                    .iter()
                                    .filter(|r| r.name.starts_with(prefix))
                                {
                                    let short = reference
                                        .name
                                        .strip_prefix(prefix)
                                        .unwrap_or(&reference.name);
                                    let response = ui
                                        .add(
                                            egui::Button::selectable(
                                                tab.selected.as_deref() == Some(&reference.hash),
                                                short,
                                            )
                                            .truncate(),
                                        )
                                        .on_hover_text(&reference.name);
                                    if response.clicked() {
                                        tab.selected = Some(reference.hash.clone());
                                    }
                                    if reference.name.starts_with("refs/heads/")
                                        || reference.name.starts_with("stash@{")
                                    {
                                        if response.secondary_clicked() {
                                            self.reference_menu = Some(ReferenceMenu {
                                                owner: tab.id,
                                                snapshot: history.snapshot.clone(),
                                                reference: reference.clone(),
                                            });
                                        }
                                        response.context_menu(|ui| {
                                            if let Some(menu) = &self.reference_menu
                                                && menu.owner == tab.id
                                            {
                                                if menu.reference.name.starts_with("stash@{") {
                                                    let message = history
                                                        .stashes
                                                        .iter()
                                                        .find(|c| c.hash == menu.reference.hash)
                                                        .map(|c| c.subject())
                                                        .unwrap_or_default();
                                                    stashes::quick_actions(
                                                        ui,
                                                        stashes::QuickTarget {
                                                            snapshot: &menu.snapshot,
                                                            reference: &menu.reference,
                                                            message,
                                                        },
                                                        self.git.as_ref(),
                                                        &mut self.operations,
                                                        &mut self.branches,
                                                        &self.scheduler,
                                                    );
                                                } else {
                                                    self.branches.ref_menu(
                                                        ui,
                                                        &menu.snapshot,
                                                        &menu.reference,
                                                    );
                                                }
                                            }
                                        });
                                    }
                                }
                            });
                        }
                    });
                }
            });
        egui::Panel::bottom("details")
            .frame(workspace_style::pane(palette.panel, 6, 2))
            .resizable(true)
            .default_size(205.)
            .min_size(80.)
            .show(ui, |ui| {
                if let Some(tab) = self.tabs.get_mut(self.selected)
                    && let Some(commit) = tab
                        .page
                        .as_ref()
                        .and_then(|p| {
                            p.entries
                                .iter()
                                .find(|e| Some(&e.commit.hash) == tab.selected.as_ref())
                        })
                        .map(|e| e.commit.clone())
                {
                    if tab.comparison.is_none()
                        && let Some(git) = &self.git
                    {
                        let mut pane = comparison::Pane::new(tab.path.clone(), git.clone());
                        pane.view.side_by_side = self
                            .settings
                            .get("DiffSideBySide")
                            .and_then(serde_json::Value::as_bool)
                            .unwrap_or(false);
                        pane.view.font_size = self
                            .settings
                            .get("DiffFontSize")
                            .and_then(serde_json::Value::as_f64)
                            .unwrap_or(12.) as f32;
                        tab.comparison = Some(pane);
                    }
                    if let Some(pane) = &mut tab.comparison {
                        pane.select(&commit, &ctx);
                        pane.show(ui);
                    }
                }
            });
        egui::CentralPanel::default()
            .frame(workspace_style::pane(palette.canvas, 0, 0))
            .show(ui, |ui| {
                let Some(tab) = self.tabs.get_mut(self.selected) else {
                    ui.vertical_centered(|ui| {
                        ui.add_space((ui.available_height() / 2. - 55.).max(8.));
                        ui.heading(strings::text("HistoryEmptyTitle"));
                        ui.label(
                            RichText::new(strings::text("HistoryEmptyBody")).color(palette.muted),
                        );
                        ui.add_space(8.);
                        if ui.button(strings::text("OpenRepository")).clicked() {
                            self.pick_repository(&ctx);
                        }
                    });
                    return;
                };
                if let Some(error) = &tab.error {
                    ui.colored_label(palette.diff_removed_text, error);
                }
                let Some(page) = &tab.page else {
                    return;
                };
                if page.entries.is_empty() {
                    ui.label(strings::text("HistoryEmptyAll"));
                }
                let selected_author = page
                    .entries
                    .iter()
                    .find(|e| Some(&e.commit.hash) == tab.selected.as_ref())
                    .map(|e| (&e.commit.author, &e.commit.email));
                let lanes = page
                    .entries
                    .iter()
                    .map(|e| e.graph.width)
                    .max()
                    .unwrap_or(1)
                    .max(2);
                let refs_right = self
                    .settings
                    .get("HistoryRefsOnRight")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false);
                ui.spacing_mut().item_spacing.y = 0.;
                egui::ScrollArea::both().id_salt(tab.id).show_rows(
                    ui,
                    23.,
                    page.entries.len(),
                    |ui, range| {
                        for index in range {
                            let entry = &page.entries[index];
                            {
                                let selected = tab.selected.as_ref() == Some(&entry.commit.hash);
                                let peer = selected_author.is_some_and(|(name, email)| {
                                    !email.trim().is_empty()
                                        && &entry.commit.author == name
                                        && &entry.commit.email == email
                                });
                                let response = history_row::show(
                                    ui, entry, lanes, selected, peer, refs_right, &palette,
                                );
                                if response.clicked() {
                                    tab.selected = Some(entry.commit.hash.clone());
                                }
                                if response.secondary_clicked()
                                    && let Some(history) = &tab.history
                                {
                                    self.history_menu = Some(HistoryMenu {
                                        owner: tab.id,
                                        snapshot: history.snapshot.clone(),
                                        commit: entry.commit.clone(),
                                    });
                                }
                                response.context_menu(|ui| {
                                    if let Some(menu) = &self.history_menu
                                        && menu.owner == tab.id
                                    {
                                        if let Some(reference) =
                                            menu.snapshot.references.iter().find(|r| {
                                                r.name.starts_with("stash@{")
                                                    && r.hash == menu.commit.hash
                                            })
                                        {
                                            stashes::quick_actions(
                                                ui,
                                                stashes::QuickTarget {
                                                    snapshot: &menu.snapshot,
                                                    reference,
                                                    message: menu.commit.subject(),
                                                },
                                                self.git.as_ref(),
                                                &mut self.operations,
                                                &mut self.branches,
                                                &self.scheduler,
                                            );
                                        } else {
                                            ui.add_enabled_ui(
                                                menu.snapshot.operations.is_empty(),
                                                |ui| {
                                                    if ui
                                                        .button(strings::text(
                                                            "HistoryMenuCheckoutCommit",
                                                        ))
                                                        .clicked()
                                                    {
                                                        self.branches.checkout(
                                                        &menu.snapshot,
                                                        shovel_core::branches::Checkout::Detached(
                                                            menu.commit.hash.clone(),
                                                        ),
                                                    );
                                                        ui.close();
                                                    }
                                                    ui.menu_button(
                                                        strings::text("HistoryResetMenu"),
                                                        |ui| {
                                                            use shovel_core::history_actions::Reset;
                                                            for (mode, label) in [
                                                                (
                                                                    Reset::Soft,
                                                                    "HistoryMenuResetSoft",
                                                                ),
                                                                (
                                                                    Reset::Mixed,
                                                                    "HistoryMenuResetMixed",
                                                                ),
                                                                (
                                                                    Reset::Hard,
                                                                    "HistoryMenuResetHard",
                                                                ),
                                                            ] {
                                                                if ui
                                                                .add_enabled(
                                                                    menu.snapshot.branch.is_some()
                                                                        && menu
                                                                            .snapshot
                                                                            .head
                                                                            .is_some(),
                                                                    egui::Button::new(
                                                                        strings::text(label),
                                                                    ),
                                                                )
                                                                .on_disabled_hover_text(
                                                                    strings::text(
                                                                        "HistoryResetNeedsBranch",
                                                                    ),
                                                                )
                                                                .clicked()
                                                            {
                                                                self.branches.reset(
                                                                    &menu.snapshot,
                                                                    &menu.commit.hash,
                                                                    mode,
                                                                );
                                                                ui.close();
                                                            }
                                                            }
                                                        },
                                                    );
                                                    let mut revert_mainline = None;
                                                    if menu.commit.parents.len() > 1 {
                                                        ui.menu_button(
                                                            strings::text(
                                                                "HistoryMenuRevertCommit",
                                                            ),
                                                            |ui| {
                                                                for (index, parent) in menu
                                                                    .commit
                                                                    .parents
                                                                    .iter()
                                                                    .enumerate()
                                                                {
                                                                    if ui
                                                                        .button(strings::format(
                                                                            "HistoryRevertParent",
                                                                            &[
                                                                                &(index + 1)
                                                                                    .to_string(),
                                                                                parent,
                                                                            ],
                                                                        ))
                                                                        .clicked()
                                                                    {
                                                                        revert_mainline =
                                                                            Some(index + 1);
                                                                        ui.close();
                                                                    }
                                                                }
                                                            },
                                                        )
                                                        .response
                                                        .on_hover_text(strings::text(
                                                            "HistoryRevertParentHelp",
                                                        ));
                                                    } else if ui
                                                        .button(strings::text(
                                                            "HistoryMenuRevertCommit",
                                                        ))
                                                        .clicked()
                                                    {
                                                        revert_mainline = Some(0);
                                                        ui.close();
                                                    }
                                                    if let Some(mainline) = revert_mainline
                                                        && let Some(git) = &self.git
                                                    {
                                                        self.operations.submit_history(
                                                "ActionTitleRevertCommit",
                                                shovel_core::history_actions::Action::Revert {
                                                    target: menu.commit.hash.clone(),
                                                    mainline,
                                                },
                                                &menu.snapshot,
                                                git.clone(),
                                                &self.scheduler,
                                                &ctx,
                                            );
                                                    }
                                                },
                                            )
                                            .response
                                            .on_disabled_hover_text(strings::text(
                                                "ActionSequence",
                                            ));
                                        }
                                        if ui.button(strings::text("HistoryCopySha")).clicked() {
                                            ctx.copy_text(menu.commit.hash.clone());
                                            ui.close();
                                        }
                                    }
                                });
                            }
                        }
                    },
                );
            });
        let stash_input =
            !self.closing && !self.operations.modal_open() && !self.branches.is_open();
        for view in &mut self.stash_views {
            view.show(&ctx, &mut self.operations, &self.scheduler, stash_input);
        }
        for editor in &mut self.editors {
            editor.show(
                &ctx,
                !self.closing && !self.operations.modal_open() && !self.branches.is_open(),
            );
        }
        let allow_branch_input = !self.closing && !self.operations.modal_open();
        self.branches.show(
            &ctx,
            self.git.as_ref(),
            &mut self.operations,
            &self.scheduler,
            allow_branch_input,
        );
        self.operations.show_prompt(&ctx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::App;
    #[test]
    fn option_saves_coalesce_without_closing_or_losing_later_edits() {
        let temporary = tempfile::tempdir().unwrap();
        let mut store = SettingsStore::new(temporary.path().into());
        let settings = store.load().unwrap();
        let (sender, receiver) = mpsc::channel();
        let mut workspace = Workspace {
            settings,
            store: Some(store),
            git: None,
            tabs: vec![],
            selected: 0,
            next_id: 1,
            sender,
            receiver,
            notice: None,
            initialized: true,
            saving: false,
            preferences_dirty: false,
            closing: false,
            pending_exit: false,
            exit_ready: false,
            confirm_exit_without_save: false,
            scheduler: Default::default(),
            editors: vec![],
            draft_store: None,
            operations: Default::default(),
            branches: Default::default(),
            stash_views: vec![],
            history_menu: None,
            reference_menu: None,
        };
        let ctx = egui::Context::default();
        workspace
            .settings
            .set("Fixture", serde_json::json!("first"));
        workspace.save_preferences(&ctx);
        workspace
            .settings
            .set("Fixture", serde_json::json!("later"));
        workspace.preferences_dirty = true;
        workspace.save_preferences(&ctx);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while workspace.saving || workspace.preferences_dirty {
            assert!(std::time::Instant::now() < deadline);
            workspace.receive(&ctx);
            thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(!workspace.exit_ready);
        assert!(!workspace.closing);
        assert!(workspace.notice.is_none());
        let reloaded = SettingsStore::new(temporary.path().into()).load().unwrap();
        assert_eq!(
            reloaded.get("Fixture").and_then(serde_json::Value::as_str),
            Some("later")
        );
    }
    #[test]
    fn pointer_activation_opens_the_real_commit_view_without_waiting_for_history() {
        let temporary = tempfile::tempdir().unwrap();
        let path = temporary.path().join("repo");
        std::fs::create_dir_all(path.join(".git")).unwrap();
        let git = Git::discover(None, &Cancellation::default()).unwrap();
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        ctx.set_embed_viewports(true);
        let (sender, receiver) = mpsc::channel();
        let mut workspace = Workspace {
            settings: Settings::default(),
            store: None,
            git: Some(Arc::new(git)),
            tabs: vec![Tab {
                id: 1,
                path: path.clone(),
                history: None,
                page: None,
                selected: None,
                previous: vec![],
                loading: true,
                refresh_again: false,
                error: None,
                comparison: None,
            }],
            selected: 0,
            next_id: 2,
            sender,
            receiver,
            notice: None,
            initialized: true,
            saving: false,
            preferences_dirty: false,
            closing: false,
            pending_exit: false,
            exit_ready: false,
            confirm_exit_without_save: false,
            scheduler: Default::default(),
            editors: vec![],
            draft_store: Some(shovel_core::drafts::Store {
                directory: temporary.path().join("drafts"),
                legacy_directory: None,
            }),
            operations: Default::default(),
            branches: Default::default(),
            stash_views: vec![],
            history_menu: None,
            reference_menu: None,
        };
        let mut frame = eframe::Frame::_new_kittest();
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1200., 800.));
        let input = egui::RawInput {
            screen_rect: Some(screen),
            ..Default::default()
        };
        let output = ctx.run_ui(input, |ui| workspace.ui(ui, &mut frame));
        let update = output.platform_output.accesskit_update.as_ref().unwrap();
        let node = update
            .nodes
            .iter()
            .find(|(_, node)| node.label() == Some("Commit"))
            .expect("Commit accessibility control");
        let bounds = node.1.bounds().unwrap();
        let position = egui::pos2(
            (bounds.x0 + bounds.x1) as f32 / 2.,
            (bounds.y0 + bounds.y1) as f32 / 2.,
        );
        output.drop_without_applying_deltas();
        ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(screen),
                events: vec![egui::Event::PointerMoved(position)],
                ..Default::default()
            },
            |ui| workspace.ui(ui, &mut frame),
        )
        .drop_without_applying_deltas();
        for pressed in [true, false] {
            let input = egui::RawInput {
                screen_rect: Some(screen),
                events: vec![egui::Event::PointerButton {
                    pos: position,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                }],
                ..Default::default()
            };
            ctx.run_ui(input, |ui| workspace.ui(ui, &mut frame))
                .drop_without_applying_deltas();
        }
        assert_eq!(workspace.editors.len(), 1);
        assert!(workspace.editors[0].open);
        assert_eq!(workspace.editors[0].path, path);
        assert!(
            workspace.tabs[0].loading,
            "Commit must not cancel history initialization"
        );
    }
}
