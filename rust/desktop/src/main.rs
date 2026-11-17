#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod auto_fetch;
mod branches;
mod caption;
mod commit;
mod comparison;
mod conflict;
mod diff_view;
mod divergence;
mod file_history;
mod file_picker;
mod file_shell;
mod git_identity;
mod git_selection;
mod history_cache;
mod history_row;
mod history_view;
mod operations;
mod read_workers;
mod remotes;
mod repository_init;
mod repository_tools;
mod settings_ui;
mod sidebar;
mod stashes;
mod strings;
mod ui_scale;
mod updater;
mod window_geometry;
mod workspace_split;
mod workspace_style;
mod workspace_tabs;

fn main() -> eframe::Result {
    velopack::VelopackApp::build()
        .set_auto_apply_on_startup(false)
        .run();
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_icon(
                eframe::icon_data::from_png_bytes(include_bytes!(
                    "../../assets/Shovel.png"
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
    SubmoduleOpened(u64, PathBuf, Result<PathBuf, String>),
    Preview(u64, u64, Option<Box<shovel_core::history_cache::Preview>>),
    CachedIdentity(u64, u64, Result<(), history_view::Failure>),
    Loaded(u64, u64, Box<History>, Box<Page>, bool),
    Page(u64, u64, Box<Page>, bool),
    Located(u64, u64, String, Result<(Page, Vec<Cursor>), String>),
    Failed(u64, u64, history_view::Failure),
    Saved(Box<SettingsStore>, Result<(), String>),
    PreferencesSaved(Box<SettingsStore>, Box<Settings>, Result<(), String>, u64),
    ToolFinished(Result<(), String>),
}
struct Tab {
    id: u64,
    path: PathBuf,
    local_changes: bool,
    history: Option<History>,
    owner_snapshot: Option<shovel_core::git::Snapshot>,
    divergence: divergence::Counter,
    remotes: remotes::Cache,
    sidebar: sidebar::Cache,
    page: Option<Page>,
    selected: Option<String>,
    previous: Vec<Cursor>,
    view: history_view::State,
    loading: bool,
    refresh_again: bool,
    locating: Option<String>,
    locate_cancel: Cancellation,
    scroll_selected: bool,
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
    git_selection: git_selection::Controller,
    repository_picker: Option<Receiver<Option<PathBuf>>>,
    tabs: Vec<Tab>,
    retained_tabs: Vec<Tab>,
    restore_window_layout: bool,
    main_geometry: window_geometry::Geometry,
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
    discard_settings_on_exit: bool,
    read_workers: read_workers::Workers,
    exit_ready: bool,
    confirm_exit_without_save: bool,
    activity_open: bool,
    activity_all: bool,
    auto_fetch: auto_fetch::Controller,
    settings_dialog: Option<settings_ui::Dialog>,
    updater: updater::Controller,
    external_tools_opened: bool,
    tab_drag: Option<(u64, Vec<u64>)>,
    scheduler: shovel_core::scheduler::Scheduler,
    editors: Vec<commit::Editor>,
    draft_store: Option<shovel_core::drafts::Store>,
    operations: operations::Controller,
    branches: branches::Dialogs,
    remotes: remotes::Dialogs,
    initialization: repository_init::Controller,
    stash_views: Vec<stashes::View>,
    history_menu: Option<HistoryMenu>,
    reference_menu: Option<ReferenceMenu>,
}
impl Workspace {
    fn new(context: &eframe::CreationContext<'_>) -> Self {
        caption::install(context);
        let (sender, receiver) = mpsc::channel();
        let output = sender.clone();
        let ctx = context.egui_ctx.clone();
        theme(&ctx, false);
        let font_ctx = ctx.clone();
        let read_workers = read_workers::Workers::default();
        let font_lease = read_workers.start_required();
        thread::spawn(move || {
            let _lease = font_lease;
            workspace_style::install_windows_font(&font_ctx);
        });
        let startup_lease = read_workers.start_required();
        thread::spawn(move || {
            let _lease = startup_lease;
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
            git_selection: Default::default(),
            repository_picker: None,
            tabs: vec![],
            retained_tabs: vec![],
            restore_window_layout: false,
            main_geometry: Default::default(),
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
            discard_settings_on_exit: false,
            read_workers,
            exit_ready: false,
            confirm_exit_without_save: false,
            activity_open: false,
            activity_all: false,
            auto_fetch: Default::default(),
            settings_dialog: None,
            updater: Default::default(),
            external_tools_opened: false,
            tab_drag: None,
            scheduler: Default::default(),
            editors: vec![],
            draft_store: None,
            operations: Default::default(),
            branches: Default::default(),
            remotes: Default::default(),
            initialization: Default::default(),
            stash_views: vec![],
            history_menu: None,
            reference_menu: None,
        }
    }
    fn open(&mut self, path: PathBuf, ctx: &egui::Context) {
        let index = self.ensure_tab(path);
        if self.tabs[index].view.identity_blocked {
            let path = self.tabs[index].path.clone();
            self.forget_identity(&path);
            self.tabs[index].owner_snapshot = None;
        }
        self.tabs[index].view.identity_blocked = false;
        self.select_tab(index, ctx);
    }
    fn ensure_tab(&mut self, path: PathBuf) -> usize {
        if let Some(index) = self
            .tabs
            .iter()
            .position(|t| Self::same_path(&t.path, &path))
        {
            return index;
        }
        if let Some(index) = self
            .retained_tabs
            .iter()
            .position(|t| Self::same_path(&t.path, &path))
        {
            let owner = self.retained_tabs.remove(index);
            self.tabs.push(owner);
            return self.tabs.len() - 1;
        }
        let id = self.next_id;
        self.next_id += 1;
        self.tabs.push(Tab {
            id,
            path,
            local_changes: false,
            history: None,
            owner_snapshot: None,
            divergence: Default::default(),
            remotes: Default::default(),
            sidebar: Default::default(),
            page: None,
            selected: None,
            previous: vec![],
            view: Default::default(),
            loading: false,
            refresh_again: false,
            locating: None,
            locate_cancel: Cancellation::default(),
            scroll_selected: false,
            error: None,
            comparison: None,
        });
        self.tabs.len() - 1
    }
    fn refresh(&mut self, ctx: &egui::Context) {
        if let Some(index) = self.inline_editor() {
            self.editors[index].refresh(ctx);
        } else {
            self.refresh_at(self.selected, ctx);
        }
    }
    fn refresh_at(&mut self, index: usize, ctx: &egui::Context) {
        if let Some(tab) = self.tabs.get(index) {
            self.refresh_owner(tab.id, ctx);
        }
    }
    fn refresh_owner(&mut self, id: u64, ctx: &egui::Context) {
        self.refresh_history(id, false, ctx);
    }
    fn first_page(&mut self, ctx: &egui::Context) {
        self.show_history(ctx);
        if let Some(tab) = self.tabs.get(self.selected) {
            self.refresh_history(tab.id, true, ctx);
        }
    }
    fn refresh_history(&mut self, id: u64, first: bool, ctx: &egui::Context) {
        if self.closing || self.pending_exit || self.confirm_exit_without_save {
            return;
        }
        let known_identity = self
            .tabs
            .iter()
            .chain(self.retained_tabs.iter())
            .find(|t| t.id == id)
            .and_then(|t| {
                t.observed_snapshot()
                    .map(|s| s.identity.fingerprint.clone())
                    .or_else(|| self.known_identity(&t.path))
            });
        let Some(git) = self.git.clone() else {
            return;
        };
        let Some(tab) = self
            .tabs
            .iter_mut()
            .chain(self.retained_tabs.iter_mut())
            .find(|t| t.id == id)
        else {
            return;
        };
        if tab.view.identity_blocked {
            return;
        }
        if tab.loading {
            tab.refresh_again = true;
            return;
        }
        tab.locate_cancel.cancel();
        tab.locating = None;
        tab.loading = true;
        tab.divergence.invalidate();
        tab.remotes.invalidate();
        tab.sidebar.invalidate();
        tab.error = None;
        let generation = tab.view.begin();
        let previous = tab
            .history
            .clone()
            .zip(tab.page.as_ref().map(|p| p.cursor.clone()));
        let path = tab.path.clone();
        let id = tab.id;
        let sender = self.sender.clone();
        let ctx = ctx.clone();
        let cancel = Cancellation::default();
        let lease = self.read_workers.start(cancel.clone());
        thread::spawn(move || {
            let _lease = lease;
            #[cfg(test)]
            history_cache::tests::wait_for_read(&path);
            let result = history_view::read_expected(
                &git,
                &path,
                previous.as_ref().map(|(h, c)| (h, c)),
                first,
                known_identity.as_deref(),
                &cancel,
            );
            let event = match result {
                Ok((history, page, keep)) => {
                    Event::Loaded(id, generation, Box::new(history), Box::new(page), keep)
                }
                Err(error) => Event::Failed(id, generation, error),
            };
            let _ = sender.send(event);
            ctx.request_repaint();
        });
    }
    fn page(&mut self, older: bool, ctx: &egui::Context) {
        if self.closing || self.pending_exit || self.confirm_exit_without_save {
            return;
        }
        self.show_history(ctx);
        let Some(git) = self.git.clone() else {
            return;
        };
        let Some(tab) = self.tabs.get_mut(self.selected) else {
            return;
        };
        if tab.loading || tab.view.stale || tab.view.identity_blocked {
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
        tab.locate_cancel.cancel();
        tab.locating = None;
        tab.loading = true;
        let generation = tab.view.begin();
        let history = history.clone();
        let id = tab.id;
        let sender = self.sender.clone();
        let ctx = ctx.clone();
        let cancel = Cancellation::default();
        let lease = self.read_workers.start(cancel.clone());
        thread::spawn(move || {
            let _lease = lease;
            let result = history_view::read_page(&git, &history, &cursor, &cancel);
            let _ = sender.send(match result {
                Ok(page) => Event::Page(id, generation, Box::new(page), older),
                Err(error) => Event::Failed(id, generation, error),
            });
            ctx.request_repaint();
        });
    }
    fn locate(&mut self, hash: String, ctx: &egui::Context) {
        if self.closing || self.pending_exit || self.confirm_exit_without_save {
            return;
        }
        self.show_history(ctx);
        let Some(tab) = self.tabs.get_mut(self.selected) else {
            return;
        };
        if tab.loading || tab.view.stale || tab.view.identity_blocked {
            return;
        }
        tab.view.locate_generation = tab.view.locate_generation.wrapping_add(1);
        let generation = tab.view.locate_generation;
        tab.locate_cancel.cancel();
        tab.locating = None;
        if tab
            .page
            .as_ref()
            .is_some_and(|p| p.entries.iter().any(|e| e.commit.hash == hash))
        {
            tab.selected = Some(hash);
            tab.scroll_selected = true;
            return;
        }
        let (Some(git), Some(history)) = (self.git.clone(), tab.history.clone()) else {
            return;
        };
        tab.locate_cancel = Cancellation::default();
        let cancel = tab.locate_cancel.clone();
        tab.locating = Some(hash.clone());
        let id = tab.id;
        let tx = self.sender.clone();
        let ctx = ctx.clone();
        let lease = self.read_workers.start(cancel.clone());
        thread::spawn(move || {
            let _lease = lease;
            let result = (|| {
                let mut cursor = Cursor::default();
                let mut previous = Vec::new();
                loop {
                    let page = git.page(&history, &cursor, &cancel).map_err(error_text)?;
                    if page.entries.iter().any(|e| e.commit.hash == hash) {
                        return Ok((page, previous));
                    }
                    let Some(next) = page.next else {
                        return Err(strings::text("RustRefNotFound"));
                    };
                    history_view::push_cursor(&mut previous, cursor);
                    cursor = next;
                }
            })();
            let _ = tx.send(Event::Located(id, generation, hash, result));
            ctx.request_repaint();
        });
    }
    fn pick_repository(&mut self, ctx: &egui::Context) {
        self.begin_repository_picker(ctx, || rfd::FileDialog::new().pick_folder());
    }
    fn begin_repository_picker(
        &mut self,
        ctx: &egui::Context,
        pick: impl FnOnce() -> Option<PathBuf> + Send + 'static,
    ) {
        if self.repository_picker.is_some() || self.closing || self.pending_exit {
            return;
        }
        let (sender, receiver) = mpsc::channel();
        self.repository_picker = Some(receiver);
        let lease = self.read_workers.start_required();
        let ctx = ctx.clone();
        thread::spawn(move || {
            let _lease = lease;
            let _ = sender.send(pick());
            ctx.request_repaint();
        });
    }
    fn begin_tool_handoff(
        &self,
        ctx: &egui::Context,
        launch: impl FnOnce() -> Result<(), String> + Send + 'static,
    ) {
        let lease = self.read_workers.start_required();
        let tx = self.sender.clone();
        let ctx = ctx.clone();
        thread::spawn(move || {
            let _lease = lease;
            let _ = tx.send(Event::ToolFinished(launch()));
            ctx.request_repaint();
        });
    }
    fn receive(&mut self, ctx: &egui::Context) {
        if !self.closing
            && !self.pending_exit
            && !self.confirm_exit_without_save
            && !self.exit_ready
        {
            self.read_workers.resume();
            self.updater.resume();
        }
        self.sync_ui_scale(ctx);
        self.poll_git_selection(ctx);
        if let Some(result) = self
            .repository_picker
            .as_ref()
            .and_then(|rx| match rx.try_recv() {
                Ok(path) => Some(path),
                Err(mpsc::TryRecvError::Disconnected) => Some(None),
                Err(mpsc::TryRecvError::Empty) => None,
            })
        {
            self.repository_picker = None;
            if let Some(path) = result {
                self.open(path, ctx);
            }
        }
        while let Ok(event) = self.receiver.try_recv() {
            match event {
                Event::ToolFinished(result) => {
                    self.external_tools_opened |= result.is_ok();
                    self.notice = result.err();
                }
                Event::PreferencesSaved(store, mut saved, result, scale_revision) => {
                    self.store = Some(*store);
                    self.saving = false;
                    if let Err(error) = result {
                        if let Some(dialog) = &mut self.settings_dialog {
                            dialog.saving = false;
                            dialog.error = Some(error);
                        }
                    } else {
                        if ui_scale::revision(ctx) != scale_revision {
                            saved.set("UiScale", serde_json::json!(ui_scale::current(ctx)));
                            self.preferences_dirty = true;
                        }
                        for key in settings_ui::KEYS {
                            if let Some(value) = saved.get(key) {
                                self.settings.set(*key, value.clone());
                            }
                        }
                        self.settings_dialog = None;
                        self.apply_preferences(ctx);
                    }
                }
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
                    ui_scale::set(ctx, self.settings.ui_scale() as f32);
                    self.restore_window_layout = true;
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
                            local_changes: false,
                            history: None,
                            owner_snapshot: None,
                            divergence: Default::default(),
                            remotes: Default::default(),
                            sidebar: Default::default(),
                            page: None,
                            selected: None,
                            previous: vec![],
                            view: Default::default(),
                            loading: false,
                            refresh_again: false,
                            locating: None,
                            locate_cancel: Cancellation::default(),
                            scroll_selected: false,
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
                    self.restore_startup_preview(ctx);
                }
                Event::Preview(id, generation, preview) => {
                    if !self.closing
                        && let Some(preview) = preview
                        && let Some(tab) = self.tabs.get_mut(self.selected)
                        && tab.id == id
                        && tab.view.generation == generation
                        && tab.page.is_none()
                        && tab.history.is_none()
                        && tab.owner_snapshot.is_none()
                        && tab.error.is_none()
                        && !tab.view.identity_blocked
                    {
                        tab.apply_preview(*preview);
                    }
                }
                Event::SubmoduleOpened(owner, parent, result) => {
                    if self.closing {
                        continue;
                    }
                    match result {
                        Ok(path)
                            if self.tabs.len() < 20
                                || self.tabs.iter().any(|t| Self::same_path(&t.path, &path)) =>
                        {
                            if self.tabs.get(self.selected).is_some_and(|t| t.id == owner) {
                                self.open(path, ctx);
                            } else {
                                self.ensure_tab(path);
                                self.remember_tabs();
                            }
                        }
                        result => {
                            let error = result
                                .err()
                                .unwrap_or_else(|| strings::text("TooManyRepositories"));
                            self.operations.record_local(
                                parent,
                                error.clone(),
                                String::new(),
                                None,
                            );
                            self.notice = Some(error);
                        }
                    }
                }
                Event::Loaded(id, generation, history, page, keep) => {
                    if !self
                        .tabs
                        .iter()
                        .chain(self.retained_tabs.iter())
                        .any(|t| t.id == id && t.view.generation == generation)
                    {
                        continue;
                    }
                    self.remember_identity(
                        &history.snapshot.worktree,
                        &history.snapshot.identity.fingerprint,
                    );
                    if let Some(git) = &self.git {
                        self.operations.resolve_initialization(
                            &history.snapshot.worktree,
                            Ok(&history.snapshot),
                            git,
                            &self.scheduler,
                            ctx,
                        );
                    }
                    if self.tabs.get(self.selected).is_some_and(|tab| tab.id == id) {
                        egui::Popup::close_all(ctx);
                        self.history_menu = None;
                        self.reference_menu = None;
                    }
                    if let Some(tab) = self
                        .tabs
                        .iter_mut()
                        .chain(self.retained_tabs.iter_mut())
                        .find(|t| t.id == id)
                    {
                        for view in &mut self.stash_views {
                            if view.path == history.snapshot.worktree {
                                view.observe(&history.snapshot);
                            }
                        }
                        tab.publish_history(*history, *page, keep);
                        if !self.closing
                            && !self.pending_exit
                            && !self.confirm_exit_without_save
                            && let (Some(git), Some(history)) = (&self.git, &tab.history)
                        {
                            tab.divergence.start(
                                git.clone(),
                                &history.snapshot,
                                ctx,
                                &self.read_workers,
                            );
                            tab.remotes.start(
                                git.clone(),
                                &history.snapshot,
                                ctx,
                                &self.read_workers,
                            );
                            tab.sidebar.start(
                                git.clone(),
                                &history.snapshot,
                                ctx,
                                &self.read_workers,
                            );
                        }
                    }
                }
                Event::Page(id, generation, page, older) => {
                    if !self
                        .tabs
                        .iter()
                        .chain(self.retained_tabs.iter())
                        .any(|t| t.id == id && t.view.generation == generation)
                    {
                        continue;
                    }
                    if self.tabs.get(self.selected).is_some_and(|tab| tab.id == id) {
                        egui::Popup::close_all(ctx);
                        self.history_menu = None;
                        self.reference_menu = None;
                    }
                    if let Some(tab) = self
                        .tabs
                        .iter_mut()
                        .chain(self.retained_tabs.iter_mut())
                        .find(|t| t.id == id)
                    {
                        tab.publish_page(*page, older);
                    }
                }
                Event::Located(id, generation, hash, result) => {
                    if let Some(tab) = self
                        .tabs
                        .iter_mut()
                        .chain(self.retained_tabs.iter_mut())
                        .find(|t| t.id == id)
                        && tab.locating.as_ref() == Some(&hash)
                        && tab.view.locate_generation == generation
                    {
                        tab.locating = None;
                        match result {
                            Ok((page, previous)) => {
                                tab.view.remember(tab.page.as_ref(), &tab.selected);
                                tab.view.restore(&page, &mut tab.selected);
                                tab.page = Some(page);
                                tab.previous = previous;
                                tab.selected = Some(hash);
                                tab.scroll_selected = true;
                                tab.estimate_cache();
                            }
                            Err(error) => tab.error = Some(error),
                        }
                    }
                }
                Event::CachedIdentity(id, generation, result) => {
                    if let Some(tab) = self
                        .tabs
                        .iter_mut()
                        .chain(self.retained_tabs.iter_mut())
                        .find(|t| t.id == id && t.view.generation == generation)
                    {
                        tab.view.checking_identity = false;
                        if let Err(failure) = result {
                            let _ = self.sender.send(Event::Failed(id, generation, failure));
                        }
                    }
                }
                Event::Failed(id, generation, failure) => {
                    if let Some(tab) = self
                        .tabs
                        .iter_mut()
                        .chain(self.retained_tabs.iter_mut())
                        .find(|t| t.id == id && t.view.generation == generation)
                    {
                        tab.loading = false;
                        tab.view.checking_identity = false;
                        let error = failure.message;
                        if failure.identity {
                            tab.locate_cancel.cancel();
                            tab.locating = None;
                            tab.view.locate_generation = tab.view.locate_generation.wrapping_add(1);
                            tab.divergence = Default::default();
                            tab.remotes = Default::default();
                            tab.sidebar = Default::default();
                            tab.history = None;
                            tab.owner_snapshot = None;
                            tab.view.preview = None;
                            tab.view.bytes = 0;
                            tab.page = None;
                            tab.selected = None;
                            tab.previous.clear();
                            tab.comparison = None;
                            tab.view.clear_views();
                            tab.view.identity_blocked = true;
                            tab.refresh_again = false;
                        }
                        tab.view.stale = true;
                        tab.view.validated = None;
                        if tab.history.is_none()
                            && let Some(git) = &self.git
                        {
                            self.operations.resolve_initialization(
                                &tab.path,
                                Err(&error),
                                git,
                                &self.scheduler,
                                ctx,
                            );
                        }
                        tab.error = Some(error);
                    }
                }
                Event::Saved(store, result) => {
                    self.store = Some(*store);
                    self.saving = false;
                    match result {
                        Ok(()) => {
                            if self.closing && !self.preferences_dirty {
                                if !self.prepare_exit(ctx) {
                                    self.closing = false;
                                    self.updater.restart_requested = false;
                                    continue;
                                }
                                if self.updater.restart_requested {
                                    self.updater.apply(
                                        ctx,
                                        self.git.clone(),
                                        self.tabs
                                            .iter()
                                            .chain(self.retained_tabs.iter())
                                            .map(|t| t.path.clone())
                                            .collect(),
                                    );
                                    continue;
                                }
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
        for path in self.operations.pending_paths() {
            if let Some(owner) = self.tabs.iter().chain(self.retained_tabs.iter()).find(|t| {
                Self::same_path(&t.path, &path)
                    && t.history.is_none()
                    && !t.loading
                    && !t.view.identity_blocked
            }) {
                self.refresh_owner(owner.id, ctx);
            }
        }
        self.restore_main_window_layout(ctx);
        if self.initialized {
            self.updater.tick(
                ctx,
                self.settings.channel(),
                self.settings
                    .get("CheckUpdatesPeriodically")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(true),
            );
        }
        if self.updater.exit_ready {
            self.exit_ready = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        } else if self.closing
            && !self.updater.restart_requested
            && !self.saving
            && !self.exit_ready
        {
            self.closing = false;
        }
        if let Some(result) = self.initialization.poll(ctx) {
            if let Some(notice) = result.notice {
                self.notice = Some(notice);
            }
            if let Some(path) = result.path {
                if self.tabs.is_empty()
                    || self.tabs.get(self.selected).map(|t| t.id) == result.origin
                {
                    self.open(path, ctx);
                } else {
                    self.ensure_tab(path);
                    self.remember_tabs();
                }
            }
        }
        if let Some((path, activity)) = self.initialization.take_activity() {
            self.operations.record_owned(path, activity);
        }
        let mut changed = self.operations.poll(ctx);
        changed.extend(
            self.auto_fetch.tick(
                ctx,
                self.initialized
                    && !self.closing
                    && !self.pending_exit
                    && !self.confirm_exit_without_save
                    && !self.git_selection.busy()
                    && self
                        .settings
                        .get("AutoFetchEnabled")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(true),
                self.settings
                    .get("AutoFetchIntervalSeconds")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(60)
                    .clamp(10, 3600),
                self.tabs
                    .iter()
                    .filter_map(|t| t.observed_snapshot().cloned())
                    .collect(),
                self.git.clone(),
                &self.scheduler,
            ),
        );
        for tab in self.tabs.iter_mut().chain(self.retained_tabs.iter_mut()) {
            tab.divergence.poll(ctx);
            let remote_changed = tab.remotes.poll(ctx);
            let sidebar_changed = tab.sidebar.poll(ctx);
            if remote_changed || sidebar_changed {
                tab.estimate_cache();
            }
        }
        for editor in &mut self.editors {
            if changed.iter().any(|path| path == &editor.path) {
                editor.external_changed();
            }
            let was_busy = editor.busy;
            let owned_work = editor.has_working_queue();
            if editor.poll(ctx) {
                changed.push(editor.path.clone());
            }
            if let Some(context) = &editor.context {
                // Commit can finish its independent scan before history. Bind
                // waiting requests now, before its controls can submit mutations.
                self.operations.bind_initialization(
                    &editor.path,
                    &context.repository,
                    &self.scheduler,
                );
            }
            for activity in editor.take_activity() {
                self.operations.record_owned(editor.path.clone(), activity);
            }
            let failure = editor.failed_operation.take();
            if (failure.is_some() || was_busy && !owned_work && !editor.busy)
                && let Some(status) = &editor.status
            {
                self.operations.record_local(
                    editor.path.clone(),
                    status.clone(),
                    editor.output.clone(),
                    failure,
                );
            }
        }
        for view in &mut self.stash_views {
            if changed.iter().any(|path| path == &view.path) {
                view.changed();
            }
            view.poll(ctx, &self.scheduler);
        }
        let common: Vec<_> = self
            .tabs
            .iter()
            .chain(self.retained_tabs.iter())
            .filter(|t| changed.iter().any(|p| Self::same_path(p, &t.path)))
            .filter_map(|t| t.observed_snapshot().map(|s| s.common_directory.clone()))
            .collect();
        for tab in self.tabs.iter().chain(self.retained_tabs.iter()) {
            if tab
                .observed_snapshot()
                .is_some_and(|s| common.contains(&s.common_directory))
                && !changed.contains(&tab.path)
            {
                changed.push(tab.path.clone());
            }
        }
        for path in changed {
            if let Some(index) = self.tabs.iter().position(|tab| tab.path == path) {
                if index == self.selected {
                    self.refresh_at(index, ctx);
                } else {
                    self.tabs[index].refresh_again = true;
                    self.tabs[index].view.stale = true;
                    self.tabs[index].divergence.invalidate();
                    self.tabs[index].remotes.invalidate();
                    self.tabs[index].sidebar.invalidate();
                }
            } else if let Some(owner) = self
                .retained_tabs
                .iter_mut()
                .find(|t| Self::same_path(&t.path, &path))
            {
                owner.refresh_again = true;
                owner.remotes.invalidate();
                owner.sidebar.invalidate();
            }
        }
        self.prune_retained_tabs();
        self.evict_history_caches();
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
                (index == self.selected && !tab.loading && std::mem::take(&mut tab.refresh_again))
                    .then_some(index)
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
            self.discard_settings_on_exit = false;
        }
        if self.pending_exit && !self.editors.iter().any(commit::Editor::exit_prompt_open) {
            if self.discard_settings_on_exit {
                self.exit_without_saving(ctx);
            } else {
                self.close(ctx);
            }
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
            stashes::View::new(snapshot, git).with_read_workers(self.read_workers.clone())
        };
        self.stash_views.push(view);
        for view in self.stash_views.iter_mut().rev().skip(3) {
            view.release_cache();
        }
    }

    fn open_commit(&mut self) {
        let Some(tab) = self.tabs.get_mut(self.selected) else {
            return;
        };
        let inline = self.settings.commit_in_workspace();
        if let Some(editor) = self
            .editors
            .iter_mut()
            .find(|editor| Self::same_path(&editor.path, &tab.path))
        {
            tab.local_changes = inline;
            editor.inline = inline;
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
            tab.local_changes = inline;
            editor.inline = inline;
            editor.read_workers = self.read_workers.clone();
            editor.restore_layout(&self.settings);
            editor.apply_preferences(&self.settings);
            self.editors.push(editor);
        }
    }

    fn inline_editor(&self) -> Option<usize> {
        let tab = self
            .tabs
            .get(self.selected)
            .filter(|tab| tab.local_changes)?;
        self.editors.iter().position(|editor| {
            editor.inline && editor.open && Self::same_path(&editor.path, &tab.path)
        })
    }

    fn show_history(&mut self, ctx: &egui::Context) {
        if let Some(tab) = self.tabs.get_mut(self.selected)
            && std::mem::take(&mut tab.local_changes)
        {
            self.sync_inline_editor(ctx);
        }
    }

    fn sync_inline_editor(&mut self, ctx: &egui::Context) {
        let target = self.tabs.get(self.selected).filter(|tab| tab.local_changes);
        for editor in &mut self.editors {
            if editor.inline
                && editor.open
                && !editor.exit_prompt_open()
                && target.is_none_or(|tab| !Self::same_path(&editor.path, &tab.path))
            {
                editor.hide(ctx);
                editor.save_layout(&mut self.settings);
                self.preferences_dirty = true;
            }
        }
        if target.is_some() && self.inline_editor().is_none() {
            self.open_commit();
        }
    }
    // Recheck immediately before every final close: saving settings yields to
    // other views and must never turn a previously safe exit into a command kill.
    fn prepare_exit(&mut self, ctx: &egui::Context) -> bool {
        if self.git_selection.busy() {
            self.notice = Some(strings::text("RustGitSelecting"));
            return false;
        }
        if self.initialization.picking || self.repository_picker.is_some() {
            self.notice = Some(strings::text("RustRepositoryChooserOpen"));
            return false;
        }
        if self.editors.iter().any(commit::Editor::native_picker_open) {
            self.pending_exit = false;
            self.notice = Some(strings::text("RustPatchChooserOpen"));
            return false;
        }
        self.auto_fetch.cancel();
        if self
            .settings_dialog
            .as_ref()
            .is_some_and(|d| d.identity.busy)
        {
            self.notice = Some(strings::text("SettingsIdentityWorking"));
            return false;
        }
        let work = self.scheduler.records();
        if work
            .iter()
            .any(|record| record.outcome.is_none() && !record.key.starts_with("automatic-fetch:"))
        {
            self.pending_exit = false;
            self.discard_settings_on_exit = false;
            self.notice = Some(strings::text("AppCommandRunningClose"));
            self.updater.restart_requested = false;
            return false;
        }
        if !self.auto_fetch.drained() || work.iter().any(|record| record.outcome.is_none()) {
            self.pending_exit = true;
            self.notice = Some(strings::text("RustShutdownBackground"));
            ctx.request_repaint_after(std::time::Duration::from_millis(25));
            return false;
        }
        if !self.updater.stop_and_drained() {
            self.pending_exit = true;
            self.notice = Some(strings::text("RustShutdownUpdates"));
            ctx.request_repaint_after(std::time::Duration::from_millis(25));
            return false;
        }
        if !self.read_workers.cancel_and_drained() {
            self.pending_exit = true;
            self.notice = Some(strings::text("RustShutdownReads"));
            ctx.request_repaint_after(std::time::Duration::from_millis(25));
            return false;
        }
        // The background wait is over. A later draft failure must leave
        // the workspace usable rather than retain a modal pending-exit state.
        self.pending_exit = false;
        if let Some(editor) = self.editors.iter_mut().find(|e| e.unsaved_conflict()) {
            self.pending_exit = true;
            editor.confirm_exit(ctx);
            return false;
        }
        for editor in &mut self.editors {
            editor.flush(ctx);
            if editor.open {
                editor.save_layout(&mut self.settings);
            }
        }
        if self.editors.iter().any(commit::Editor::blocks_exit) {
            // Continue the same close after a load/save/result delivery finishes.
            // A failed draft write instead returns control for explicit recovery.
            self.pending_exit = !self
                .editors
                .iter()
                .any(|editor| editor.draft_problem().is_some());
            if self.pending_exit {
                ctx.request_repaint_after(std::time::Duration::from_millis(25));
            }
            self.notice = Some(
                self.editors
                    .iter()
                    .find_map(|editor| editor.draft_problem().map(str::to_owned))
                    .unwrap_or_else(|| strings::text("CommitDraftSaving")),
            );
            return false;
        }
        if !self.initialized {
            return false;
        }
        true
    }
    fn exit_without_saving(&mut self, ctx: &egui::Context) {
        if self.saving {
            return;
        }
        self.confirm_exit_without_save = false;
        self.discard_settings_on_exit = true;
        self.updater.restart_requested = false;
        if !self.prepare_exit(ctx) {
            self.closing = false;
            return;
        }
        self.pending_exit = false;
        self.exit_ready = true;
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }
    fn close(&mut self, ctx: &egui::Context) {
        if self.exit_ready {
            return;
        }
        ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        if self.closing {
            return;
        }
        self.discard_settings_on_exit = false;
        if !self.prepare_exit(ctx) {
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
        self.sync_ui_scale(ctx);
        let maximized = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
        self.settings
            .set("IsMaximized", serde_json::json!(maximized));
        let layout = window_geometry::saved_layout(&self.settings, "MainWindow");
        let size = self.main_geometry.native_size.unwrap_or_else(|| {
            let value = |key, legacy, fallback| {
                layout
                    .and_then(|v| v.get(key))
                    .or_else(|| self.settings.get(legacy))
                    .and_then(|v| v.as_f64())
                    .unwrap_or(fallback) as f32
            };
            egui::vec2(
                value("Width", "WindowWidth", 1200.),
                value("Height", "WindowHeight", 800.),
            )
        });
        self.settings.set("WindowWidth", serde_json::json!(size.x));
        self.settings.set("WindowHeight", serde_json::json!(size.y));
        window_geometry::save_layout(&mut self.settings, "MainWindow", size, maximized);
        self.closing = true;
        self.pending_exit = false;
        self.preferences_dirty = true;
        self.save_preferences(ctx);
    }
    fn apply_preferences(&mut self, ctx: &egui::Context) {
        ui_scale::set(ctx, self.settings.ui_scale() as f32);
        theme(
            ctx,
            self.settings.get("Appearance").and_then(|v| v.as_u64()) == Some(1),
        );
        for tab in &mut self.tabs {
            if let Some(pane) = &mut tab.comparison {
                pane.view.side_by_side = self
                    .settings
                    .get("DiffSideBySide")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                pane.view.font_size = self
                    .settings
                    .get("DiffFontSize")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(12.) as f32;
            }
        }
        let inline = self.settings.commit_in_workspace();
        for editor in &mut self.editors {
            if editor.inline != inline && !editor.exit_prompt_open() {
                if editor.open {
                    editor.hide(ctx);
                    editor.save_layout(&mut self.settings);
                    self.preferences_dirty = true;
                }
                editor.inline = inline;
            }
            editor.apply_preferences(&self.settings);
        }
        if !inline {
            for tab in self.tabs.iter_mut().chain(self.retained_tabs.iter_mut()) {
                tab.local_changes = false;
            }
        }
    }
    fn settings_window(&mut self, ctx: &egui::Context) {
        let Some(dialog) = &mut self.settings_dialog else {
            return;
        };
        let (save, close) = dialog.show(ctx, self.git.clone());
        if close {
            self.settings_dialog = None;
            return;
        }
        if save
            && !self.saving
            && let Some(mut store) = self.store.take()
        {
            let mut settings = self.settings.clone();
            dialog.merge(&mut settings);
            dialog.saving = true;
            dialog.error = None;
            self.saving = true;
            let tx = self.sender.clone();
            let scale_revision = ui_scale::revision(ctx);
            let ctx = ctx.clone();
            thread::spawn(move || {
                let result = store
                    .save(&settings)
                    .map_err(|_| strings::text("SettingsSaveFailed"));
                let _ = tx.send(Event::PreferencesSaved(
                    Box::new(store),
                    Box::new(settings),
                    result,
                    scale_revision,
                ));
                ctx.request_repaint();
            });
        }
    }
    fn save_preferences(&mut self, ctx: &egui::Context) {
        let cache = self
            .closing
            .then(|| (self.startup_store(), self.capture_startup_preview()));
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
            if result.is_ok()
                && let Some((Some(cache_store), preview)) = cache
            {
                cache_store.save(preview.as_ref());
            }
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
    ui_scale::install(ctx);
    workspace_style::apply(ctx, light);
}
impl eframe::App for Workspace {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.receive(ctx);
    }
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.branches.read_workers = self.read_workers.clone();
        self.sync_ui_scale(&ctx);
        self.main_geometry.observe(&ctx);
        let sidebar_commit = self.settings.commit_in_workspace();
        let modal_open = self.closing
            || self.pending_exit
            || self.confirm_exit_without_save
            || self.git_selection.busy()
            || self.settings_dialog.is_some()
            || self
                .editors
                .iter()
                .any(|editor| editor.open && !editor.inline)
            || self.operations.modal_open()
            || self.remotes.is_open()
            || self.branches.is_open()
            || self.initialization.is_open()
            || self.stash_views.iter().any(|view| view.open);
        let shortcuts_blocked = modal_open
            || self
                .inline_editor()
                .is_some_and(|i| self.editors[i].has_prompt());
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
                            self.exit_without_saving(&ctx);
                        }
                        if ui.button(strings::text("RustStayOpen")).clicked() {
                            self.confirm_exit_without_save = false;
                            self.pending_exit = false;
                            self.discard_settings_on_exit = false;
                            self.updater.restart_requested = false;
                        }
                    });
                });
        }
        if ctx.input(|i| i.viewport().close_requested()) {
            self.close(&ctx);
        }
        if !shortcuts_blocked && ctx.input(|i| i.key_pressed(egui::Key::F5)) {
            self.refresh(&ctx);
        }
        if !shortcuts_blocked
            && ctx.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::PageDown))
        {
            self.page(true, &ctx);
        }
        if !shortcuts_blocked && ctx.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::PageUp))
        {
            self.page(false, &ctx);
        }
        if ctx.input(|i| {
            i.events
                .iter()
                .any(|event| matches!(event, egui::Event::WindowFocused(true)))
        }) {
            if self.tabs.get(self.selected).is_some_and(|tab| {
                tab.view
                    .validated
                    .is_none_or(|at| at.elapsed() >= std::time::Duration::from_secs(30))
            }) {
                self.activate_history(self.selected, &ctx);
            }
            theme(
                &ctx,
                self.settings
                    .get("Appearance")
                    .and_then(serde_json::Value::as_u64)
                    == Some(1),
            );
        }
        if !shortcuts_blocked && !self.tabs.is_empty() {
            if ctx.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::W)) {
                self.close_tab(self.tabs[self.selected].id, &ctx);
            } else if ctx.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::Tab)) {
                let count = self.tabs.len();
                let step = if ctx.input(|i| i.modifiers.shift) {
                    count - 1
                } else {
                    1
                };
                self.select_tab((self.selected + step) % count, &ctx);
            }
        }
        let palette = workspace_style::Palette::current(ui);
        egui::Panel::top("toolbar")
            .frame(workspace_style::pane(palette.chrome, 6, 2))
            .show(ui, |ui| {
                egui::MenuBar::new().ui(ui, |ui| {
                    ui.menu_button(strings::text("FileMenu"), |ui| {
                        if ui
                            .add_enabled(
                                self.git.is_some()
                                    && self.tabs.len() < 20
                                    && !self.initialization.busy,
                                egui::Button::new(strings::text("NewRepository")),
                            )
                            .on_hover_text(strings::text(if self.git.is_none() {
                                "GitErrorMissingGit"
                            } else if self.tabs.len() >= 20 {
                                "TooManyRepositories"
                            } else if self.initialization.busy {
                                "RustRepositoryCreationBusy"
                            } else {
                                "ActionExplainInit"
                            }))
                            .clicked()
                        {
                            self.initialization.open();
                            ui.close();
                        }

                        if ui
                            .add_enabled(
                                self.git.is_some()
                                    && self.tabs.len() < 20
                                    && !self.initialization.busy,
                                egui::Button::new(strings::text("ActionTitleClone")),
                            )
                            .on_hover_text(strings::text(if self.git.is_none() {
                                "GitErrorMissingGit"
                            } else if self.tabs.len() >= 20 {
                                "TooManyRepositories"
                            } else if self.initialization.busy {
                                "RustRepositoryCreationBusy"
                            } else {
                                "CloneReady"
                            }))
                            .clicked()
                        {
                            self.initialization.open_clone();
                            ui.close();
                        }

                        if ui.button(strings::text("SettingsMenu")).clicked() {
                            self.settings_dialog = Some(settings_ui::Dialog::new(&self.settings));
                            ui.close();
                        }
                        if ui.button(strings::text("OpenRepository")).clicked() {
                            self.pick_repository(&ctx);
                            ui.close();
                        }
                        if ui
                            .add_enabled(
                                !self.tabs.is_empty(),
                                egui::Button::new(strings::text("CloseTab")),
                            )
                            .clicked()
                        {
                            self.close_tab(self.tabs[self.selected].id, &ctx);
                            ui.close();
                        }
                    });
                    ui.menu_button(strings::text("ViewMenu"), |ui| {
                        if ui.button(strings::text("ActivityTitle")).clicked() {
                            self.activity_open = true;
                            ui.close();
                        }

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
                        let ready = self
                            .tabs
                            .get(self.selected)
                            .is_some_and(|t| !t.loading && !t.view.stale);
                        let older = ready
                            && self
                                .tabs
                                .get(self.selected)
                                .is_some_and(|t| t.page.as_ref().is_some_and(|p| p.next.is_some()));
                        let newer = ready
                            && self
                                .tabs
                                .get(self.selected)
                                .is_some_and(|t| !t.previous.is_empty());
                        let unavailable = self
                            .tabs
                            .get(self.selected)
                            .map(|t| t.history_notice())
                            .unwrap_or_else(|| strings::text("HistoryNotLoaded"));
                        if ui
                            .add_enabled(
                                older,
                                egui::Button::new(strings::text("LoadOlderCommits")),
                            )
                            .on_disabled_hover_text(&unavailable)
                            .clicked()
                        {
                            self.page(true, &ctx);
                            ui.close();
                        }
                        if ui
                            .add_enabled(newer, egui::Button::new(strings::text("NewerCommits")))
                            .on_disabled_hover_text(if ready {
                                strings::text("RustHistoryNoNewer")
                            } else {
                                unavailable.clone()
                            })
                            .clicked()
                        {
                            self.page(false, &ctx);
                            ui.close();
                        }
                        let first = self.tabs.get(self.selected).is_some_and(|t| {
                            !t.loading && !t.view.identity_blocked && t.page.is_some()
                        });
                        if ui
                            .add_enabled(
                                first,
                                egui::Button::new(strings::text("FirstHistoryPage")),
                            )
                            .on_disabled_hover_text(
                                self.tabs
                                    .get(self.selected)
                                    .and_then(|t| t.error.as_ref())
                                    .unwrap_or(&unavailable),
                            )
                            .clicked()
                        {
                            self.first_page(&ctx);
                            ui.close();
                        }
                        if let Some(tab) = self.tabs.get(self.selected) {
                            ui.separator();
                            if let Some(page) = &tab.page {
                                let start = if page.entries.is_empty() {
                                    0
                                } else {
                                    page.cursor.offset + 1
                                };
                                ui.label(strings::format(
                                    "HistoryRange",
                                    &[
                                        &start.to_string(),
                                        &(page.cursor.offset + page.entries.len()).to_string(),
                                    ],
                                ));
                            }
                            ui.label(tab.history_notice());
                        }
                    });
                    ui.menu_button(strings::text("RepositoryMenu"), |ui| {
                        if ui
                            .add_enabled(
                                self.can_choose_git(),
                                egui::Button::new(strings::text("ChooseGit")),
                            )
                            .on_disabled_hover_text(strings::text("RustGitSelectionBusy"))
                            .clicked()
                        {
                            self.choose_git(&ctx);
                            ui.close();
                        }
                        ui.separator();
                        let tab = self.tabs.get(self.selected);
                        self.remotes.menu(
                            ui,
                            tab.and_then(|t| t.history.as_ref()).map(|h| &h.snapshot),
                            tab.map(|t| &t.remotes),
                            None,
                        );
                        ui.separator();
                        if ui
                            .add_enabled(
                                self.git.is_some()
                                    && self.tabs.len() < 20
                                    && !self.initialization.busy,
                                egui::Button::new(strings::text("NewRepository")),
                            )
                            .on_hover_text(strings::text(if self.git.is_none() {
                                "GitErrorMissingGit"
                            } else if self.tabs.len() >= 20 {
                                "TooManyRepositories"
                            } else if self.initialization.busy {
                                "RustRepositoryCreationBusy"
                            } else {
                                "ActionExplainInit"
                            }))
                            .clicked()
                        {
                            self.initialization.open();
                            ui.close();
                        }

                        if ui
                            .add_enabled(
                                self.git.is_some()
                                    && self.tabs.len() < 20
                                    && !self.initialization.busy,
                                egui::Button::new(strings::text("ActionTitleClone")),
                            )
                            .on_hover_text(strings::text(if self.git.is_none() {
                                "GitErrorMissingGit"
                            } else if self.tabs.len() >= 20 {
                                "TooManyRepositories"
                            } else if self.initialization.busy {
                                "RustRepositoryCreationBusy"
                            } else {
                                "CloneReady"
                            }))
                            .clicked()
                        {
                            self.initialization.open_clone();
                            ui.close();
                        }

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
                                    ui.close();
                                    self.select_tab(index, &ctx);
                                    break;
                                }
                            }
                        });
                    });
                    ui.menu_button(strings::text("HelpMenu"), |ui| {
                        if ui.button(strings::text("RustUpdatesTitle")).clicked() {
                            self.updater.open = true;
                            self.updater.check(&ctx);
                            ui.close();
                        }
                    });
                    caption::controls(ui);
                });
                ui.allocate_ui_with_layout(
                    egui::vec2(ui.available_width(), 52.),
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| {
                        ui.spacing_mut().item_spacing.x = 2.;
                        if workspace_style::action(ui, "OpenRepository", true).clicked() {
                            self.pick_repository(&ctx);
                        }
                        self.operations.divergence = self
                            .tabs
                            .get(self.selected)
                            .map(|t| t.divergence.display())
                            .unwrap_or_default();
                        self.preferences_dirty |= self.operations.toolbar(
                            ui,
                            &mut self.settings,
                            self.tabs.get(self.selected).map(|t| &t.path),
                            self.tabs
                                .get(self.selected)
                                .and_then(|t| t.history.as_ref())
                                .map(|h| &h.snapshot),
                            self.git.as_ref(),
                            &self.scheduler,
                        );
                        if !sidebar_commit
                            && workspace_style::action(
                                ui,
                                "Commit",
                                self.git.is_some() && !self.tabs.is_empty(),
                            )
                            .clicked()
                        {
                            self.open_commit();
                        }
                        let offer = self.updater.available_version(self.settings.channel());
                        let reserved = 180. + offer.map_or(0., |_| updater::UPDATE_CARD_WIDTH + 2.);
                        let context_width = (ui.available_width() - reserved).clamp(90., 530.);
                        let margin =
                            ((ui.available_width() - reserved - context_width) / 2.).max(0.);
                        ui.add_space(margin);
                        if updater::offer_card(ui, offer) {
                            self.updater.open = true;
                        }
                        let context_response = egui::Frame::new()
                            .fill(palette.raised)
                            .stroke(egui::Stroke::new(1., palette.border))
                            .corner_radius(6)
                            .inner_margin(egui::Margin::symmetric(10, 2))
                            .show(ui, |ui| {
                                ui.set_width((context_width - 22.).max(1.));
                                ui.spacing_mut().item_spacing.y = 1.;
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
                                        if let Some(snapshot) =
                                            tab.history.as_ref().map(|h| &h.snapshot).or_else(
                                                || tab.view.preview.as_ref().map(|(s, _)| s),
                                            )
                                        {
                                            ui.add(
                                                egui::Label::new(
                                                    RichText::new(
                                                        snapshot
                                                            .branch
                                                            .as_deref()
                                                            .unwrap_or("HEAD"),
                                                    )
                                                    .small()
                                                    .color(palette.muted),
                                                )
                                                .truncate(),
                                            );
                                        }
                                        let editor = self
                                            .editors
                                            .iter()
                                            .find(|e| Self::same_path(&e.path, &tab.path));
                                        let progress = self
                                            .operations
                                            .progress(&tab.path, &self.scheduler)
                                            .or_else(|| editor.and_then(commit::Editor::progress))
                                            .or_else(|| {
                                                tab.locating
                                                    .is_some()
                                                    .then(|| strings::text("HistoryLocatingCommit"))
                                            })
                                            .or_else(|| {
                                                tab.loading.then(|| strings::text("HistoryLoading"))
                                            });
                                        let active = progress.is_some();
                                        let status = progress.or_else(|| {
                                            let completed = self.operations.summary(&tab.path);
                                            let local = editor.and_then(|e| e.status.clone());
                                            match (completed, local) {
                                                (Some(run), Some(local)) => {
                                                    Some(format!("{run} · {local}"))
                                                }
                                                (run, local) => run.or(local),
                                            }
                                        });
                                        if let Some(status) = status {
                                            ui.horizontal(|ui| {
                                                if active {
                                                    ui.add(egui::Spinner::new().size(14.));
                                                }
                                                let response = ui
                                                    .add(
                                                        egui::Label::new(
                                                            RichText::new(&status)
                                                                .small()
                                                                .color(palette.muted),
                                                        )
                                                        .truncate()
                                                        .sense(egui::Sense::click()),
                                                    )
                                                    .on_hover_text(status);
                                                if response.clicked() {
                                                    self.activity_open = true;
                                                }
                                            });
                                        } else {
                                            ui.allocate_space(egui::vec2(
                                                0.,
                                                ui.text_style_height(&egui::TextStyle::Small),
                                            ));
                                        }
                                    } else {
                                        ui.label(RichText::new(strings::text("AppTitle")).strong());
                                    }
                                });
                            });
                        if context_response
                            .response
                            .interact(egui::Sense::click())
                            .on_hover_text(strings::text("ActivityTitle"))
                            .clicked()
                        {
                            self.activity_open = true;
                        }
                        if self.branches.toolbar(
                            ui,
                            self.tabs
                                .get(self.selected)
                                .and_then(|t| t.history.as_ref())
                                .map(|h| &h.snapshot),
                            self.git.is_some() && self.tabs.get(self.selected).is_some(),
                        ) && let Some(tab) = self.tabs.get(self.selected)
                        {
                            self.operations.defer_new_branch(
                                &tab.path,
                                &self.settings,
                                &self.scheduler,
                                &ctx,
                            );
                        }
                        let tab = self.tabs.get(self.selected);
                        if let Some(kind) = repository_tools::toolbar(ui, tab.is_some())
                            && let Some(tab) = tab
                        {
                            let path = tab.path.clone();
                            let settings = self.settings.clone();
                            self.begin_tool_handoff(&ctx, move || {
                                repository_tools::launch(kind, &path, &settings)
                            });
                        }
                    },
                );
                if self.initialization.busy
                    && let Some(summary) = &self.initialization.summary
                {
                    ui.horizontal(|ui| {
                        ui.add(egui::Spinner::new().size(14.));
                        ui.label(summary);
                    });
                }
                if let Some(notice) = &self.notice {
                    ui.colored_label(palette.diff_removed_text, notice);
                }
                if let Some(tab) = self.tabs.get(self.selected)
                    && let Some(history) = &tab.history
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
            });
        if self.activity_open {
            let mut open = true;
            let mut reopen = None;
            let selected_path = self.tabs.get(self.selected).map(|t| t.path.clone());
            egui::Window::new(strings::text("ActivityTitle"))
                .open(&mut open)
                .enabled(!modal_open && !self.closing)
                .default_width(580.)
                .show(&ctx, |ui| {
                    ui.checkbox(
                        &mut self.activity_all,
                        strings::text("ActivityAllRepositories"),
                    );
                    egui::ScrollArea::vertical()
                        .max_height(420.)
                        .show(ui, |ui| {
                            egui::CollapsingHeader::new(strings::text("ActivityBackground")).show(
                                ui,
                                |ui| {
                                    for (path, message) in self.auto_fetch.activity.iter().rev() {
                                        ui.label(
                                            path.file_name().unwrap_or_default().to_string_lossy(),
                                        );
                                        ui.add(egui::Label::new(message).wrap());
                                    }
                                },
                            );
                            if self.activity_all || selected_path.is_some() {
                                reopen = self.operations.status(
                                    ui,
                                    if self.activity_all {
                                        None
                                    } else {
                                        selected_path.as_ref()
                                    },
                                    &self.scheduler,
                                );
                            }
                            for editor in self.editors.iter_mut().filter(|editor| {
                                (self.activity_all || selected_path.as_ref() == Some(&editor.path))
                                    && !editor.failed_commit
                                    && (editor.status.is_some() || !editor.output.is_empty())
                            }) {
                                ui.label(
                                    editor
                                        .path
                                        .file_name()
                                        .unwrap_or_default()
                                        .to_string_lossy(),
                                );
                                ui.horizontal(|ui| {
                                    if let Some(status) = &editor.status {
                                        ui.label(status);
                                    }
                                    if !editor.output.is_empty() {
                                        egui::CollapsingHeader::new(strings::text(
                                            "OperationAttentionLogs",
                                        ))
                                        .show(ui, |ui| {
                                            egui::ScrollArea::both().max_height(170.).show(
                                                ui,
                                                |ui| {
                                                    ui.add(
                                                        egui::Label::new(
                                                            RichText::new(&editor.output)
                                                                .monospace(),
                                                        )
                                                        .selectable(true),
                                                    );
                                                },
                                            );
                                        });
                                    }
                                });
                            }
                        });
                });
            self.activity_open = open;
            if let Some(path) = reopen {
                self.open(path, &ctx);
                self.open_commit();
            }
        }
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
                let mut target = None;
                let mut action = None;
                ui.horizontal(|ui| {
                    for (index, tab) in self.tabs.iter().enumerate() {
                        let label = tab.path.file_name().unwrap_or_default().to_string_lossy();
                        let response = ui.add_sized(
                            [width, 26.],
                            egui::Button::new(RichText::new(label).color(palette.text))
                                .truncate()
                                .sense(egui::Sense::click_and_drag())
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
                        if response.drag_started() {
                            self.tab_drag =
                                Some((tab.id, self.tabs.iter().map(|t| t.id).collect()));
                        }
                        if ctx.input(|i| i.pointer.interact_pos()).is_some_and(|pos| {
                            pos.x >= response.rect.left() && pos.x <= response.rect.right()
                        }) {
                            target = Some(index);
                        }
                        if response.clicked() {
                            selected = Some(index);
                        }
                        if response.clicked_by(egui::PointerButton::Middle) {
                            action = Some(workspace_tabs::TabAction::Close(tab.id));
                        }
                        response.context_menu(|ui| {
                            use workspace_tabs::TabAction;
                            for (key, command, enabled) in [
                                ("RefreshRepository", TabAction::Refresh(tab.id), true),
                                ("MoveLeft", TabAction::Move(tab.id, -1), index > 0),
                                ("MoveRight", TabAction::Move(tab.id, 1), index + 1 < count),
                                ("CloseTab", TabAction::Close(tab.id), true),
                                ("CloseOtherTabs", TabAction::CloseOthers(tab.id), count > 1),
                            ] {
                                if ui
                                    .add_enabled(enabled, egui::Button::new(strings::text(key)))
                                    .clicked()
                                {
                                    action = Some(command);
                                    ui.close();
                                }
                            }
                        });
                        response.on_hover_text(tab.path.to_string_lossy());
                    }
                });
                if let Some((drag, original)) = &self.tab_drag {
                    let active = self.tabs.get(self.selected).map(|t| t.id);
                    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                        self.tabs.sort_by_key(|t| {
                            original
                                .iter()
                                .position(|id| *id == t.id)
                                .unwrap_or(usize::MAX)
                        });
                        self.tab_drag = None;
                    } else {
                        if let (Some(target), Some(from)) =
                            (target, self.tabs.iter().position(|t| t.id == *drag))
                            && target != from
                        {
                            let moved = self.tabs.remove(from);
                            self.tabs.insert(target, moved);
                        }
                        if !ctx.input(|i| i.pointer.primary_down()) {
                            self.tab_drag = None;
                            self.settings.set(
                                "RepositoryPaths",
                                serde_json::json!(
                                    self.tabs.iter().map(|t| &t.path).collect::<Vec<_>>()
                                ),
                            );
                            self.preferences_dirty = true;
                        }
                    }
                    if let Some(index) = self.tabs.iter().position(|t| Some(t.id) == active) {
                        self.selected = index;
                    }
                    selected = None;
                }
                if let Some(action) = action {
                    self.tab_action(action, &ctx);
                } else if let Some(index) = selected {
                    self.select_tab(index, &ctx);
                }
            });
        use workspace_split::Divider;
        let width = ui.available_width();
        let mut locate_request = None;
        let mut submodule_open = None;
        let sidebar = egui::Panel::left("sidebar")
            .frame(workspace_style::pane(palette.sidebar, 8, 4))
            .exact_size(Divider::Sidebar.size(ui, &self.settings, width))
            .resizable(false)
            .show(ui, |ui| {
                if sidebar_commit && self.tabs.get(self.selected).is_some() {
                    let local = self.inline_editor().is_some();
                    if ui.selectable_label(!local, strings::text("FileHistoryTab")).clicked() {
                        self.show_history(&ctx);
                    }
                    if ui.add_enabled(
                        self.git.is_some() && self.draft_store.is_some(),
                        egui::Button::selectable(local, strings::text("WorkspaceLocalChanges")),
                    ).clicked() {
                        self.open_commit();
                    }
                    ui.separator();
                }
                if let Some(tab) = self.tabs.get_mut(self.selected)
                    && let Some(history) = &tab.history
                {
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        for (key, prefix) in [
                            ("Branches", "refs/heads/"),
                            ("Remotes", "refs/remotes/"),
                            ("Tags", "refs/tags/"),
                            ("Stashes", "stash@{"),
                            ("Submodules", ""),
                        ] {
                            let group_id = format!("{}|{}", tab.path.display(), key);
                            let mut collapsed: Vec<String> = self
                                .settings
                                .get("CollapsedSidebarGroups")
                                .and_then(|v| serde_json::from_value(v.clone()).ok())
                                .unwrap_or_default();
                            let group = egui::CollapsingHeader::new(
                                RichText::new(format!(
                                    "{}  {}",
                                    strings::text(key),
                                    if key == "Submodules" { tab.sidebar.value.as_ref().map_or(0,|v|v.submodules.len()) }
                                    else { history.snapshot.references.iter().filter(|r|r.name.starts_with(prefix)).count() }
                                ))
                                .strong()
                                .color(palette.muted),
                            )
                            .default_open(!collapsed.iter().any(|g| g == &group_id))
                            .id_salt((tab.id, key))
                            .show(ui, |ui| {
                                let group_height = ((ui.clip_rect().height() - 112.) / 4.).max(48.);
                                egui::ScrollArea::vertical()
                                    .id_salt(("refs", key))
                                    .max_height(group_height)
                                    .auto_shrink([false, true])
                                    .show(ui, |ui| {
                                        if key == "Submodules" {
                                            if let Some(value) = &tab.sidebar.value {
                                                for module in &value.submodules {
                                                    let response = workspace_style::reference_row(ui, false, &module.path, key).on_hover_text(&module.path);
                                                    if response.clicked() || response.has_focus() && ui.input_mut(|i|i.consume_key(egui::Modifiers::NONE,egui::Key::Enter)) {
                                                        submodule_open = Some((tab.id, history.snapshot.clone(), module.clone()));
                                                    }
                                                    response.context_menu(|ui| {
                                                        if ui.button(strings::text("SidebarOpenSubmodule")).clicked() {
                                                            submodule_open=Some((tab.id,history.snapshot.clone(),module.clone())); ui.close();
                                                        }
                                                        for (operation,label) in [(shovel_core::submodules::Operation::Update,"ActionTitleSubmoduleUpdate"),
                                                            (shovel_core::submodules::Operation::Sync,"ActionTitleSubmoduleSync")] {
                                                            if ui.add_enabled(history.snapshot.operations.is_empty() && self.git.is_some(),egui::Button::new(strings::text(label)))
                                                                .on_disabled_hover_text(strings::text(if self.git.is_none() { "GitErrorMissingGit" } else { "ActionSequence" })).clicked()
                                                                && let Some(git)=&self.git {
                                                                self.operations.submit_submodule(shovel_core::submodules::Action{target:module.clone(),operation},
                                                                    &history.snapshot,git.clone(),&self.scheduler,&ctx);
                                                                ui.close();
                                                            }
                                                        }
                                                    });
                                                }
                                            } else if tab.sidebar.pending() { ui.label(strings::text("SidebarLoading")); }
                                            if let Some(error) = &tab.sidebar.error { ui.label(error); }
                                        }
                                        if key == "Remotes" {
                                            let names: std::collections::BTreeSet<_> = history
                                                .snapshot
                                                .remotes
                                                .iter()
                                                .map(String::as_str)
                                                .chain(
                                                    history.snapshot.references.iter().filter_map(
                                                        |r| {
                                                            r.name
                                                                .strip_prefix("refs/remotes/")
                                                                .and_then(|s| s.split('/').next())
                                                        },
                                                    ),
                                                )
                                                .collect();
                                            for name in names {
                                                let prefix = format!("refs/remotes/{name}/");
                                                let remote_id = format!(
                                                    "{}|Remotes/{}",
                                                    tab.path.display(),
                                                    name
                                                );
                                                let remote = egui::CollapsingHeader::new(name)
                                                    .default_open(!collapsed.contains(&remote_id))
                                                    .id_salt((tab.id, "remote", name))
                                                    .show(ui, |ui| {
                                                        for reference in history
                                                            .snapshot
                                                            .references
                                                            .iter()
                                                            .filter(|r| r.name.starts_with(&prefix))
                                                        {
                                                            let response =
                                                                workspace_style::reference_row(
                                                                    ui,
                                                                    tab.selected.as_deref()
                                                                        == Some(&reference.hash),
                                                                    reference
                                                                        .name
                                                                        .strip_prefix(&prefix)
                                                                        .unwrap_or(&reference.name),
                                                                    key,
                                                                )
                                                                .on_hover_text(&reference.name);
                                                            if response.clicked()
                                                                || response.has_focus()
                                                                    && ui.input_mut(|i| {
                                                                        i.consume_key(
                                                                            egui::Modifiers::NONE,
                                                                            egui::Key::Enter,
                                                                        )
                                                                    })
                                                            {
                                                                locate_request =
                                                                    Some(reference.hash.clone());
                                                            }
                                                            response.context_menu(|ui| {
                                                                if ui
                                                                    .button(strings::text(
                                                                        "SidebarLocate",
                                                                    ))
                                                                    .clicked()
                                                                {
                                                                    locate_request = Some(
                                                                        reference.hash.clone(),
                                                                    );
                                                                    ui.close();
                                                                }
                                                            });
                                                        }
                                                    });
                                                let clicked = remote.header_response.clicked();
                                                remote
                                                    .header_response
                                                    .on_hover_text(name)
                                                    .context_menu(|ui| {
                                                        if history
                                                            .snapshot
                                                            .remotes
                                                            .iter()
                                                            .any(|r| r == name)
                                                            && ui
                                                                .add_enabled(
                                                                    self.git.is_some(),
                                                                    egui::Button::new(
                                                                        strings::text(
                                                                            "ActionTitleFetch",
                                                                        ),
                                                                    ),
                                                                )
                                                                .on_disabled_hover_text(
                                                                    strings::text(
                                                                        "GitErrorMissingGit",
                                                                    ),
                                                                )
                                                                .clicked()
                                                            && let Some(git) = &self.git
                                                        {
                                                            self.operations.fetch_remote(
                                                                name,
                                                                &history.snapshot,
                                                                git.clone(),
                                                                &self.scheduler,
                                                                &ctx,
                                                            );
                                                            ui.close();
                                                        }
                                                        self.remotes.menu(
                                                            ui,
                                                            Some(&history.snapshot),
                                                            Some(&tab.remotes),
                                                            Some(name),
                                                        );
                                                    });
                                                if clicked {
                                                    if collapsed.contains(&remote_id) {
                                                        collapsed.retain(|id| id != &remote_id);
                                                    } else {
                                                        if collapsed.len() >= 256 {
                                                            collapsed.remove(0);
                                                        }
                                                        collapsed.push(remote_id);
                                                    }
                                                    self.settings.set(
                                                        "CollapsedSidebarGroups",
                                                        serde_json::json!(&collapsed),
                                                    );
                                                    self.preferences_dirty = true;
                                                }
                                            }
                                            if let Some(error) = &tab.remotes.error {
                                                ui.label(error);
                                            }
                                        }
                                        for reference in
                                            history.snapshot.references.iter().filter(|r| {
                                                key != "Remotes" && key != "Submodules" && r.name.starts_with(prefix)
                                            })
                                        {
                                            let short = if key == "Stashes" {
                                                history
                                                    .stashes
                                                    .iter()
                                                    .find(|c| c.hash == reference.hash)
                                                    .map(|c| {
                                                        format!(
                                                            "{}  {}",
                                                            reference.name,
                                                            c.subject()
                                                        )
                                                    })
                                                    .unwrap_or_else(|| reference.name.clone())
                                            } else {
                                                reference
                                                    .name
                                                    .strip_prefix(prefix)
                                                    .unwrap_or(&reference.name)
                                                    .to_owned()
                                            };
                                            let detail = if key == "Branches" { tab.sidebar.tracking(reference) } else { String::new() };
                                            let label = if detail.is_empty() { short } else { format!("{short}  {detail}") };
                                            let response = workspace_style::reference_row(
                                                ui,
                                                tab.selected.as_deref() == Some(&reference.hash),
                                                &label,
                                                key,
                                            )
                                            .on_hover_text(&reference.name);
                                            let activated = response.double_clicked()
                                                || response.has_focus()
                                                    && ui.input_mut(|i| {
                                                        i.consume_key(
                                                            egui::Modifiers::NONE,
                                                            egui::Key::Enter,
                                                        )
                                                    });
                                            if activated
                                                && reference.name.starts_with("refs/heads/")
                                            {
                                                if history.snapshot.operations.is_empty() {
                                                    self.branches.checkout(
                                                        &history.snapshot,
                                                        shovel_core::branches::Checkout::Branch(
                                                            reference.clone(),
                                                        ),
                                                    );
                                                } else {
                                                    self.notice =
                                                        Some(strings::text("ActionSequence"));
                                                }
                                            } else if response.clicked() || activated {
                                                locate_request = Some(reference.hash.clone());
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
                                                        if menu
                                                            .reference
                                                            .name
                                                            .starts_with("stash@{")
                                                        {
                                                            let message = history
                                                                .stashes
                                                                .iter()
                                                                .find(|c| {
                                                                    c.hash == menu.reference.hash
                                                                })
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
                            });
                            if key == "Remotes" {
                                group.header_response.context_menu(|ui| {
                                    self.remotes.menu(
                                        ui,
                                        Some(&history.snapshot),
                                        Some(&tab.remotes),
                                        None,
                                    )
                                });
                            }
                            if group.header_response.clicked() {
                                if collapsed.iter().any(|g| g == &group_id) {
                                    collapsed.retain(|g| g != &group_id);
                                } else {
                                    if collapsed.len() >= 256 {
                                        collapsed.remove(0);
                                    }
                                    collapsed.push(group_id);
                                }
                                self.settings
                                    .set("CollapsedSidebarGroups", serde_json::json!(collapsed));
                                self.preferences_dirty = true;
                            }
                        }
                    });
                }
            });
        if let Some((owner, snapshot, target)) = submodule_open
            && let Some(git) = &self.git
        {
            let git = git.clone();
            let sender = self.sender.clone();
            let ctx = ctx.clone();
            let lease = self.read_workers.start_required();
            thread::spawn(move || {
                let _lease = lease;
                let result = git
                    .open_submodule(&snapshot, &target)
                    .map_err(|e| strings::text(&e.code()));
                let _ = sender.send(Event::SubmoduleOpened(owner, snapshot.worktree, result));
                ctx.request_repaint();
            });
        }
        if let Some(hash) = locate_request {
            self.locate(hash, &ctx);
        }
        let height = ui.available_height();
        self.preferences_dirty |=
            Divider::Sidebar.handle(ui, sidebar.response.rect, width, &mut self.settings);
        let inline_editor = self.inline_editor();
        let details_panel = egui::Panel::bottom("details")
            .frame(workspace_style::pane(palette.panel, 6, 3))
            .exact_size(Divider::Details.size(ui, &self.settings, height))
            .resizable(false);
        let details = inline_editor.is_none().then(|| {
            details_panel.show(ui, |ui| {
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
                    if tab.view.preview.is_some() {
                        comparison::metadata(ui, &commit);
                        return;
                    }
                    if tab.comparison.is_none()
                        && let Some(git) = &self.git
                    {
                        let mut pane = comparison::Pane::new(tab.path.clone(), git.clone())
                            .with_read_workers(self.read_workers.clone());
                        pane.files_width = self
                            .settings
                            .get("HistoryFilesWidth")
                            .and_then(|v| v.as_f64())
                            .unwrap_or(340.) as f32;
                        pane.details_index = self
                            .settings
                            .get("RustDetailsTab")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(0)
                            .min(1) as usize;
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
                        pane.view.font_size = self
                            .settings
                            .get("DiffFontSize")
                            .and_then(|v| v.as_f64())
                            .unwrap_or(12.) as f32;
                        pane.select(&commit, &ctx);
                        let before = pane.details_index;
                        let old_width = pane.files_width;
                        let h = ui.available_height();
                        egui::ScrollArea::vertical()
                            .id_salt("details-viewport")
                            .max_height(h)
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                ui.set_max_height(h);
                                pane.show(ui);
                            });
                        if (old_width - pane.files_width).abs() > 1. {
                            self.settings
                                .set("HistoryFilesWidth", serde_json::json!(pane.files_width));
                            self.preferences_dirty = true;
                        }
                        if before != pane.details_index {
                            self.settings
                                .set("RustDetailsTab", serde_json::json!(pane.details_index));
                            self.preferences_dirty = true;
                        }
                    }
                }
            })
        });
        if let Some(details) = details {
            self.preferences_dirty |=
                Divider::Details.handle(ui, details.response.rect, height, &mut self.settings);
        }
        egui::CentralPanel::default()
            .frame(workspace_style::pane(palette.canvas, 0, 0))
            .show(ui, |ui| {
                if let Some(index) = inline_editor {
                    self.editors[index].show_inline(ui, !modal_open);
                    return;
                }
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
                if tab.view.preview.is_some() {
                    ui.label(tab.history_notice());
                }
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
                let mut scroll = egui::ScrollArea::both().id_salt(tab.id);
                if let Some(offset) = tab.view.restore_offset.take() {
                    scroll = scroll.scroll_offset(offset);
                }
                if std::mem::take(&mut tab.scroll_selected)
                    && let Some(index) = page
                        .entries
                        .iter()
                        .position(|e| Some(&e.commit.hash) == tab.selected.as_ref())
                {
                    scroll = scroll.vertical_scroll_offset((index as f32 * 23. - 46.).max(0.));
                }
                let scroll_output = scroll.show_rows(ui, 23., page.entries.len(), |ui, range| {
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
                                                            (Reset::Soft, "HistoryMenuResetSoft"),
                                                            (Reset::Mixed, "HistoryMenuResetMixed"),
                                                            (Reset::Hard, "HistoryMenuResetHard"),
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
                                                        strings::text("HistoryMenuRevertCommit"),
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
                                        .on_disabled_hover_text(strings::text("ActionSequence"));
                                    }
                                    if ui.button(strings::text("HistoryCopySha")).clicked() {
                                        ctx.copy_text(menu.commit.hash.clone());
                                        ui.close();
                                    }
                                }
                            });
                        }
                    }
                });
                tab.view.offset = scroll_output.state.clamped_offset();
            });
        let stash_input = !self.closing
            && !self.operations.modal_open()
            && !self.branches.is_open()
            && !self.remotes.is_open();
        let mut stash_navigation = None;
        for view in &mut self.stash_views {
            view.show(&ctx, &mut self.operations, &self.scheduler, stash_input);
            if let Some(hash) = view.take_navigation() {
                stash_navigation = Some((view.path.clone(), hash));
            }
        }
        if let Some((path, hash)) = stash_navigation
            && self
                .tabs
                .get(self.selected)
                .is_some_and(|tab| tab.path == path)
        {
            self.locate(hash, &ctx);
        }
        let failure_editor = self.operations.failure_path().and_then(|path| {
            self.editors.iter().position(|editor| {
                editor.open && !editor.inline && Self::same_path(&editor.path, path)
            })
        });
        for (index, editor) in self.editors.iter_mut().enumerate() {
            let open = editor.open;
            let show_failure = !self.closing && failure_editor == Some(index);
            if editor.inline && editor.exit_prompt_open() {
                editor.show_leave_prompt(&ctx);
            }
            editor.show(
                &ctx,
                !self.closing
                    && !self.operations.modal_open()
                    && !self.branches.is_open()
                    && !self.remotes.is_open(),
                |ctx| {
                    if show_failure {
                        self.operations.show_failure(ctx);
                    }
                },
            );
            if open && !editor.open {
                editor.save_layout(&mut self.settings);
                self.preferences_dirty = true;
            }
        }
        self.initialization.show(
            &ctx,
            self.git.as_ref(),
            &self.scheduler,
            self.tabs.get(self.selected).map(|t| t.id),
            &self.operations,
        );
        self.settings_window(&ctx);
        let update_blocked = self.initialization.picking
            || self.scheduler.has_accepted_work()
            || self.saving
            || self
                .editors
                .iter()
                .any(|e| e.blocks_exit() || e.unsaved_conflict() || e.native_picker_open());
        self.updater
            .show(&ctx, update_blocked, self.external_tools_opened);
        if self.updater.restart_requested && !self.closing {
            self.close(&ctx);
        }

        if !self.remotes.is_open() {
            self.operations.open_ready_branch(&mut self.branches);
        }
        if !self.branches.is_open() && !self.remotes.is_open() {
            self.preferences_dirty |= self.operations.options_dialog(
                &ctx,
                &mut self.settings,
                self.git.as_ref(),
                &self.scheduler,
            );
        }
        let allow_remote_input =
            !self.closing && !self.operations.active_modal_open() && !self.branches.is_open();
        self.remotes.show(
            &ctx,
            self.git.as_ref(),
            &mut self.operations,
            &self.scheduler,
            allow_remote_input,
        );
        let allow_branch_input =
            !self.closing && !self.operations.active_modal_open() && !self.remotes.is_open();
        self.branches.show(
            &ctx,
            self.git.as_ref(),
            &mut self.operations,
            &self.scheduler,
            allow_branch_input,
        );
        if !self.closing && failure_editor.is_none() {
            self.operations.show_failure(&ctx);
        }
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
            git_selection: Default::default(),
            repository_picker: None,
            tabs: vec![],
            retained_tabs: vec![],
            restore_window_layout: false,
            main_geometry: Default::default(),
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
            discard_settings_on_exit: false,
            read_workers: Default::default(),
            exit_ready: false,
            confirm_exit_without_save: false,
            activity_open: false,
            activity_all: false,
            auto_fetch: Default::default(),
            settings_dialog: None,
            updater: Default::default(),
            external_tools_opened: false,
            tab_drag: None,
            scheduler: Default::default(),
            editors: vec![],
            draft_store: None,
            operations: Default::default(),
            branches: Default::default(),
            remotes: Default::default(),
            initialization: Default::default(),
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
            git_selection: Default::default(),
            repository_picker: None,
            tabs: vec![Tab {
                id: 1,
                path: path.clone(),
                local_changes: false,
                history: None,
                owner_snapshot: None,
                divergence: Default::default(),
                remotes: Default::default(),
                sidebar: Default::default(),
                page: None,
                selected: None,
                previous: vec![],
                view: Default::default(),
                loading: true,
                refresh_again: false,
                locating: None,
                locate_cancel: Cancellation::default(),
                scroll_selected: false,
                error: None,
                comparison: None,
            }],
            retained_tabs: vec![],
            restore_window_layout: false,
            main_geometry: Default::default(),
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
            discard_settings_on_exit: false,
            read_workers: Default::default(),
            exit_ready: false,
            confirm_exit_without_save: false,
            activity_open: false,
            activity_all: false,
            auto_fetch: Default::default(),
            settings_dialog: None,
            updater: Default::default(),
            external_tools_opened: false,
            tab_drag: None,
            scheduler: Default::default(),
            editors: vec![],
            draft_store: Some(shovel_core::drafts::Store {
                directory: temporary.path().join("drafts"),
                legacy_directory: None,
            }),
            operations: Default::default(),
            branches: Default::default(),
            remotes: Default::default(),
            initialization: Default::default(),
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

#[cfg(test)]
mod submodule_tests;
#[cfg(test)]
mod workspace_interaction_tests;
