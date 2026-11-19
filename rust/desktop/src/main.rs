#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod activity_output;
mod auto_fetch;
mod avatar_provider;
mod avatars;
mod branches;
mod caption;
mod commit;
mod comparison;
mod conflict;
mod dates;
mod diff_view;
mod divergence;
mod file_history;
mod file_picker;
mod file_shell;
mod git_identity;
mod git_selection;
mod history_cache;
mod history_menu;
use history_menu::HistoryMenu;
mod history_row;
mod history_view;
mod numbers;
mod operations;
mod path_text;
mod read_workers;
mod remotes;
mod repository_init;
mod repository_menu;
mod repository_tools;
mod repository_watch;
mod settings_ui;
mod sidebar;
mod stashes;
mod strings;
mod ui_scale;
mod updater;
mod update_restart;
mod window_geometry;
mod tab_markers;
mod workspace_split;
mod workspace_style;
mod workspace_tabs;

#[cfg(test)]
mod native_navigation_tests;

fn main() -> eframe::Result {
    velopack::VelopackApp::build()
        .set_auto_apply_on_startup(false)
        .run();
    let profile = std::env::args_os()
        .skip(1)
        .collect::<Vec<_>>()
        .windows(2)
        .find(|a| a[0] == "--data-directory")
        .map(|a| PathBuf::from(&a[1]))
        .filter(|p| p.is_absolute());
    eframe::run_native(
        &strings::text("AppTitle"),
        native_options(),
        Box::new(move |context| Ok(Box::new(Workspace::new(context, profile)))),
    )
}

fn native_options() -> eframe::NativeOptions {
    eframe::NativeOptions {
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
    }
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
    InitialHistory(u64, u64, Box<Page>),
    CachedIdentity(u64, u64, Result<(), history_view::Failure>),
    Loaded(u64, u64, history_view::Read, bool),
    Page(u64, u64, Box<Page>, bool, Option<f32>),
    Located(u64, u64, String, Result<(Page, Vec<Cursor>), String>),
    Failed(u64, u64, history_view::Failure),
    Saved(Box<SettingsStore>, Result<(), String>),
    PreferencesSaved(Box<SettingsStore>, Box<Settings>, Result<(), String>, u64),
    ToolFinished(Result<(), String>),
}
struct Tab {
    id: u64,
    path: PathBuf,
    qualified_name: Option<String>,
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
struct ReferenceMenu {
    owner: u64,
    snapshot: shovel_core::git::Snapshot,
    reference: shovel_core::git::Reference,
    stash_targets: Vec<shovel_core::stashes::Target>,
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
    repository_dialog: Option<repository_menu::Dialog>,
    remotes: remotes::Dialogs,
    initialization: repository_init::Controller,
    stash_views: Vec<stashes::View>,
    history_menu: Option<HistoryMenu>,
    reference_menu: Option<ReferenceMenu>,
    deferred_history: std::collections::VecDeque<Event>,
    tab_markers: tab_markers::Controller,
}
impl Workspace {
    fn new(context: &eframe::CreationContext<'_>, profile: Option<PathBuf>) -> Self {
        caption::install(context);
        let (sender, receiver) = mpsc::channel();
        let output = sender.clone();
        let ctx = context.egui_ctx.clone();
        theme(&ctx, false);
        let font_ctx = ctx.clone();
        let read_workers = read_workers::Workers::default();
        avatars::install(&ctx, read_workers.clone());
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
            let directory = profile.unwrap_or_else(|| local.join("ShovelRust/Foundation"));
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
            repository_dialog: None,
            remotes: Default::default(),
            initialization: Default::default(),
            stash_views: vec![],
            history_menu: None,
            reference_menu: None,
            deferred_history: Default::default(),
            tab_markers: Default::default(),
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
        if let Some(index) = self.tabs.iter().position(|t| t.matches_path(&path)) {
            return index;
        }
        if let Some(index) = self
            .retained_tabs
            .iter()
            .position(|t| t.matches_path(&path))
        {
            let mut owner = self.retained_tabs.remove(index);
            if owner.view.watch.as_ref().is_none_or(|watch| !watch.available())
                && !owner.view.identity_blocked
            {
                // The cached page can display immediately while observation resumes.
                owner.view.stale = owner.history.is_some();
            }
            self.tabs.push(owner);
            self.update_tab_names();
            return self.tabs.len() - 1;
        }
        let id = self.next_id;
        self.next_id += 1;
        self.tabs.push(Tab {
            id,
            path,
            qualified_name: None,
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
        self.update_tab_names();
        self.tabs.len() - 1
    }
    fn refresh(&mut self, ctx: &egui::Context) {
        if let Some(tab) = self.tabs.get(self.selected) {
            self.tab_markers.invalidate(tab.id);
        }
        if let Some(index) = self.inline_editor() {
            self.editors[index].request_refresh(ctx);
        } else {
            if let Some(pane) = self
                .tabs
                .get_mut(self.selected)
                .and_then(|tab| tab.comparison.as_mut())
            {
                pane.load_files(ctx);
            }
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
    fn spawn_history_read(
        &self,
        ctx: &egui::Context,
        cancel: Cancellation,
        read: impl FnOnce() -> Event + Send + 'static,
        failed: impl FnOnce() -> Event + Clone + Send + 'static,
    ) {
        let sender = self.sender.clone();
        let worker_ctx = ctx.clone();
        let lease = self.read_workers.start(cancel);
        let worker_failed = failed.clone();
        let worker = thread::Builder::new().spawn(move || {
            let _lease = lease;
            let event = std::panic::catch_unwind(std::panic::AssertUnwindSafe(read))
                .unwrap_or_else(|_| worker_failed());
            let _ = sender.send(event);
            worker_ctx.request_repaint();
        });
        if worker.is_err() {
            let _ = self.sender.send(failed());
            ctx.request_repaint();
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
                t.view.reopen_commit_owner.clone().or_else(|| {
                    t.observed_snapshot()
                        .map(|s| s.identity.fingerprint.clone())
                        .or_else(|| self.known_identity(&t.path))
                })
            });
        let Some(git) = self.git.clone() else {
            return;
        };
        let foreground = self.tabs.get(self.selected).is_some_and(|tab| tab.id == id);
        let open = self.tabs.iter().any(|tab| tab.id == id);
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
        if tab.loading || (!first && tab.locating.is_some()) {
            tab.refresh_again = true;
            return;
        }
        tab.locate_cancel.cancel();
        if foreground {
            tab.view.prewarm = false;
        }
        tab.locating = None;
        tab.loading = true;
        tab.divergence.invalidate();
        tab.remotes.invalidate();
        tab.sidebar.invalidate();
        tab.view.external_history_change = false;
        let generation = tab.view.begin();
        let previous = tab
            .history
            .as_ref()
            .filter(|_| tab.page.is_some())
            .map(|history| {
                (
                    history.snapshot.identity.clone(),
                    history.snapshot.fingerprint.clone(),
                )
            });
        let path = tab.path.clone();
        let id = tab.id;
        let watching = if open {
            tab.start_watching(ctx)
        } else {
            None
        };
        let cancel = Cancellation::default();
        let initial = (foreground && tab.page.is_none() && tab.observed_snapshot().is_none())
            .then(|| (self.sender.clone(), ctx.clone()));
        self.spawn_history_read(
            ctx,
            cancel.clone(),
            move || {
                if let Some(ready) = watching {
                    // Keep startup responsive on filesystems that stall watch setup.
                    let _ = ready.recv_timeout(std::time::Duration::from_millis(75));
                }
                #[cfg(test)]
                history_cache::tests::wait_for_read(&path);
                let git = git.as_ref().clone().with_foreground_reads(foreground);
                let publish = |page: &Page| {
                    if let Some((sender, ctx)) = &initial
                        && sender
                            .send(Event::InitialHistory(id, generation, Box::new(page.clone())))
                            .is_ok()
                    {
                        ctx.request_repaint();
                    }
                };
                let result = history_view::read_expected(
                    &git,
                    &path,
                    previous
                        .as_ref()
                        .map(|(identity, fingerprint)| (identity, fingerprint.as_str())),
                    first,
                    known_identity.as_deref(),
                    initial.as_ref().map(|_| &publish as &dyn Fn(&Page)),
                    &cancel,
                );
                match result {
                    Ok(read) => Event::Loaded(id, generation, read, first),
                    Err(error) => Event::Failed(id, generation, error),
                }
            },
            move || Event::Failed(id, generation, history_view::Failure::interrupted()),
        );
    }
    fn page(&mut self, older: bool, ctx: &egui::Context) {
        self.read_history_page(older, None, ctx);
    }
    fn read_history_page(
        &mut self,
        older: bool,
        viewport_height: Option<f32>,
        ctx: &egui::Context,
    ) {
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
        if tab.loading || tab.view.identity_blocked {
            return;
        }
        if !older && tab.page.as_ref().is_some_and(|page| page.cursor.offset > 0)
            && (tab.view.stale || tab.previous.is_empty())
        {
            self.first_page(ctx);
            return;
        }
        if tab.view.stale {
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
        let cancel = Cancellation::default();
        self.spawn_history_read(
            ctx,
            cancel.clone(),
            move || {
                #[cfg(test)]
                history_cache::tests::wait_for_read(&history.snapshot.worktree);
                let git = git.as_ref().clone().with_foreground_reads(true);
                match history_view::read_page(&git, &history, &cursor, &cancel) {
                    Ok(page) => Event::Page(id, generation, Box::new(page), older, viewport_height),
                    Err(error) => Event::Failed(id, generation, error),
                }
            },
            move || Event::Failed(id, generation, history_view::Failure::interrupted()),
        );
    }
    fn locate(&mut self, hash: String, ctx: &egui::Context) {
        if self.closing || self.pending_exit || self.confirm_exit_without_save {
            return;
        }
        self.show_history(ctx);
        let Some(tab) = self.tabs.get_mut(self.selected) else {
            return;
        };
        if tab.view.identity_blocked {
            return;
        }
        if tab
            .page
            .as_ref()
            .is_some_and(|p| p.entries.iter().any(|e| e.commit.hash == hash))
        {
            tab.locate_cancel.cancel();
            tab.locating = None;
            tab.selected = Some(hash);
            tab.scroll_selected = true;
            return;
        }
        if tab.loading || tab.view.stale {
            return;
        }
        let (Some(git), Some(history)) = (self.git.clone(), tab.history.clone()) else {
            return;
        };
        tab.locate_cancel = Cancellation::default();
        tab.view.locate_generation = tab.view.locate_generation.wrapping_add(1);
        let generation = tab.view.locate_generation;
        let cancel = tab.locate_cancel.clone();
        tab.locating = Some(hash.clone());
        let id = tab.id;
        let failed_hash = hash.clone();
        self.spawn_history_read(
            ctx,
            cancel.clone(),
            move || {
                #[cfg(test)]
                history_cache::tests::wait_for_read(&history.snapshot.worktree);
                let git = git.as_ref().clone().with_foreground_reads(true);
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
                Event::Located(id, generation, hash, result)
            },
            move || {
                Event::Located(
                    id,
                    generation,
                    failed_hash,
                    Err(strings::text("RustHistoryWorkerFailed")),
                )
            },
        );
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
        self.sync_inspector_layouts(false);
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
        self.poll_repository_changes(ctx);
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
        // Keep the displayed rows and popup anchors together until the user dismisses the menu.
        let protected_owner = self
            .tabs
            .get(self.selected)
            .filter(|_| !self.pending_exit && !self.closing && egui::Popup::is_any_open(ctx))
            .map(|tab| tab.id);
        let mut avatar_sources_changed = false;
        let mut deferred = std::mem::take(&mut self.deferred_history);
        while let Some(event) = deferred
            .pop_front()
            .or_else(|| self.receiver.try_recv().ok())
        {
            let owner = match &event {
                Event::Loaded(id, ..)
                | Event::Page(id, ..)
                | Event::Located(id, ..)
                | Event::Preview(id, ..)
                | Event::InitialHistory(id, ..) => Some(*id),
                Event::Failed(id, _, failure) if !failure.identity => Some(*id),
                _ => None,
            };
            if owner.is_some() && owner == protected_owner {
                self.deferred_history.push_back(event);
                continue;
            }
            match event {
                Event::ToolFinished(result) => {
                    if result.is_ok() {
                        self.external_tools_opened = true;
                        self.updater.tool_opened();
                    }
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
                    self.refresh_avatar_sources(ctx);
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
                            qualified_name: None,
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
                            view: history_view::State::restored(),
                            loading: false,
                            refresh_again: false,
                            locating: None,
                            locate_cancel: Cancellation::default(),
                            scroll_selected: false,
                            error: None,
                            comparison: None,
                        });
                    }
                    self.update_tab_names();
                    self.selected = self
                        .settings
                        .get("SelectedRepositoryPath")
                        .and_then(serde_json::Value::as_str)
                        .and_then(|p| {
                            self.tabs
                                .iter()
                                .position(|t| Self::same_path(&t.path, std::path::Path::new(p)))
                        })
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
                Event::InitialHistory(id, generation, page) => {
                    if !self.closing
                        && !self.pending_exit
                        && let Some(tab) = self.tabs.iter_mut().find(|tab| tab.id == id)
                        && tab.view.generation == generation
                        && tab.loading
                        && tab.page.is_none()
                        && tab.observed_snapshot().is_none()
                        && !tab.view.external_history_change
                        && !tab.view.identity_blocked
                    {
                        tab.show_initial_history(*page);
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
                                self.open(path.clone(), ctx);
                                self.tabs[self.selected].sidebar.set_superproject(&path, Some(&parent));
                            } else {
                                let index = self.ensure_tab(path.clone());
                                self.tabs[index].sidebar.set_superproject(&path, Some(&parent));
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
                Event::Loaded(id, generation, read, first) => {
                    if let Some(tab) = self
                        .tabs
                        .iter_mut()
                        .chain(self.retained_tabs.iter_mut())
                        .find(|tab| tab.id == id && tab.view.generation == generation)
                        && tab.view.external_history_change
                        && tab.history.is_some()
                    {
                        tab.loading = false;
                        continue;
                    }
                    let Some(owner_path) = self
                        .tabs
                        .iter()
                        .chain(self.retained_tabs.iter())
                        .find(|t| t.id == id && t.view.generation == generation)
                        .map(|t| t.path.clone())
                    else {
                        continue;
                    };
                    let snapshot = read.snapshot();
                    self.remember_identity(&owner_path, &snapshot.identity.fingerprint);
                    if self.tabs.get(self.selected).is_some_and(|tab| tab.id == id) {
                        self.remember_recent_repository(&owner_path);
                    }
                    let mut reopen_inline = false;
                    let mut reopen_failed = false;
                    for editor in &mut self.editors {
                        if (Self::same_path(&editor.path, &owner_path)
                            || Self::same_path(&editor.path, &snapshot.worktree))
                            && !editor.accept_identity(&snapshot.identity)
                        {
                            reopen_inline |= editor.inline && editor.open;
                            editor.detach_replaced();
                        }
                    }
                    if let Some(git) = &self.git {
                        self.operations.resolve_initialization(
                            &owner_path,
                            Ok(snapshot),
                            git,
                            &self.scheduler,
                            ctx,
                        );
                    }
                    if let Some(tab) = self
                        .tabs
                        .iter_mut()
                        .chain(self.retained_tabs.iter_mut())
                        .find(|t| t.id == id)
                    {
                        for view in &mut self.stash_views {
                            if view.path == snapshot.worktree {
                                view.observe(snapshot);
                            }
                        }
                        if let Some(expected) = tab.view.reopen_commit_owner.take() {
                            reopen_failed =
                                snapshot.identity.matches_recorded_fingerprint(&expected);
                            if !reopen_failed {
                                reopen_inline = false;
                                self.notice = Some(strings::text("FailedCommitOwnerUnavailable"));
                            }
                        }
                        let watch_mismatch = tab.view.watch.as_ref().is_some_and(|watch| {
                            !watch.covers(&snapshot.identity.fingerprint)
                        });
                        tab.publish_history(read, first);
                        let prewarmed = std::mem::take(&mut tab.view.prewarm);
                        if watch_mismatch {
                            tab.view.watch = None;
                            tab.view.stale = true;
                            tab.refresh_again = true;
                        }
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
                            if prewarmed {
                                prime_comparison(tab, git, &self.settings, &self.read_workers, ctx);
                            }
                        }
                    }
                    if !self.closing
                        && !self.pending_exit
                        && !self.confirm_exit_without_save
                        && self.tabs.get(self.selected).is_some_and(|tab| {
                            tab.id == id && (reopen_failed || reopen_inline && tab.local_changes)
                        })
                    {
                        self.open_commit();
                    }
                }
                Event::Page(id, generation, page, older, viewport_height) => {
                    if let Some(tab) = self
                        .tabs
                        .iter_mut()
                        .chain(self.retained_tabs.iter_mut())
                        .find(|t| t.id == id && t.view.generation == generation)
                    {
                        if tab.view.external_history_change {
                            tab.loading = false;
                        } else {
                            tab.publish_page(*page, older, viewport_height);
                        }
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
                        if tab.view.external_history_change {
                            continue;
                        }
                        match result {
                            Ok((page, previous)) => {
                                tab.error = None;
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
                    avatar_sources_changed |= failure.identity;
                    if failure.identity
                        && self
                            .tabs
                            .get(self.selected)
                            .is_some_and(|tab| tab.id == id && tab.view.generation == generation)
                    {
                        egui::Popup::close_all(ctx);
                        self.history_menu = None;
                        self.reference_menu = None;
                    }
                    if let Some(tab) = self
                        .tabs
                        .iter_mut()
                        .chain(self.retained_tabs.iter_mut())
                        .find(|t| t.id == id && t.view.generation == generation)
                    {
                        tab.loading = false;
                        tab.view.checking_identity = false;
                        tab.discard_initial_history();
                        if tab.view.reopen_commit_owner.take().is_some() {
                            self.notice = Some(if failure.identity {
                                strings::text("FailedCommitOwnerUnavailable")
                            } else {
                                failure.message.clone()
                            });
                        }
                        let error = failure.message;
                        if failure.identity {
                            for editor in &mut self.editors {
                                if tab.matches_path(&editor.path) {
                                    editor.block_repository(&error);
                                }
                            }
                            self.tab_markers.forget(id);
                            tab.view.generation = tab.view.generation.wrapping_add(1);
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
                            tab.view.watch = None;
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
                                    if !self.pending_exit {
                                        self.updater.restart_requested = false;
                                    }
                                    continue;
                                }
                                if self.updater.restart_requested {
                                    self.updater.apply(
                                        ctx,
                                        self.git.clone(),
                                        self.tabs
                                            .iter()
                                            .chain(self.retained_tabs.iter())
                                            .map(|tab| update_restart::Repository {
                                                path: tab.path.clone(),
                                                observed: tab.observed_snapshot().map(|s| s.identity.clone()),
                                                identity_blocked: tab.view.identity_blocked,
                                            })
                                            .collect(),
                                    );
                                    continue;
                                }
                                self.exit_ready = true;
                                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                            }
                        }
                        Err(error) => {
                            if self.closing && self.updater.restart_requested {
                                self.updater.profile_save_failed();
                            } else {
                                self.notice = Some(error);
                                self.confirm_exit_without_save = self.closing;
                            }
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
        if self.updater.exit_ready() {
            if self.scheduler.has_accepted_work() {
                self.notice = Some(strings::text("AppCommandRunningClose"));
                ctx.request_repaint_after(std::time::Duration::from_millis(25));
            } else {
                self.exit_ready = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        } else if self.closing
            && !self.updater.restart_requested
            && !self.saving
            // A closing layout can still be queued behind an earlier write.
            && !self.preferences_dirty
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
        for result in self.auto_fetch.tick(
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
            self.tabs.iter().filter_map(Tab::observed_snapshot),
            self.git.clone(),
            &self.scheduler,
        ) {
            if result.succeeded {
                changed.push(result.path.clone());
            }
            self.operations.record_background(result);
        }
        for tab in self.tabs.iter_mut().chain(self.retained_tabs.iter_mut()) {
            tab.divergence.poll(ctx);
            if Some(tab.id) == protected_owner {
                continue;
            }
            let remote_changed = tab.remotes.poll(ctx);
            avatar_sources_changed |= remote_changed;
            let sidebar_changed = tab.sidebar.poll(ctx, &tab.path);
            if remote_changed || sidebar_changed {
                tab.estimate_cache();
            }
        }
        if avatar_sources_changed {
            self.refresh_avatar_sources(ctx);
        }
        for editor in &mut self.editors {
            if changed.iter().any(|path| {
                Self::same_path(path, &editor.path)
                    || editor
                        .current_context()
                        .is_some_and(|c| Self::same_path(path, &c.repository.worktree))
            }) {
                editor.external_changed();
            }
            let was_busy = editor.busy;
            let owned_work = editor.has_working_queue();
            if editor.poll(ctx) {
                changed.push(editor.path.clone());
            }
            if let Some(context) = editor.current_context() {
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
        let common: Vec<_> = self
            .tabs
            .iter()
            .chain(self.retained_tabs.iter())
            .filter(|t| changed.iter().any(|p| t.matches_path(p)))
            .filter_map(|t| t.observed_snapshot().map(|s| s.common_directory.clone()))
            .collect();
        for tab in self.tabs.iter().chain(self.retained_tabs.iter()) {
            if let Some(snapshot) = tab.observed_snapshot()
                && common.contains(&snapshot.common_directory)
            {
                for path in [&tab.path, &snapshot.worktree] {
                    if !changed.contains(path) {
                        changed.push(path.clone());
                    }
                }
            }
        }
        for view in &mut self.stash_views {
            if changed.iter().any(|path| Self::same_path(path, &view.path)) {
                view.changed();
            }
            view.poll(ctx, &self.scheduler);
        }
        for path in changed {
            if let Some(index) = self.tabs.iter().position(|tab| tab.path == path) {
                self.tab_markers.invalidate(self.tabs[index].id);
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
        self.evict_closed_history();
        for tab in self.tabs.iter_mut().chain(self.retained_tabs.iter_mut()) {
            if let Some(pane) = &mut tab.comparison {
                pane.poll(ctx);
            }
        }
        let pending = self
            .tabs
            .iter_mut()
            .enumerate()
            .filter_map(|(index, tab)| {
                (index == self.selected
                    && !tab.loading
                    && tab.locating.is_none()
                    && std::mem::take(&mut tab.refresh_again))
                .then_some(index)
            })
            .collect::<Vec<_>>();
        for index in pending {
            self.refresh_at(index, ctx);
        }
        self.poll_tab_markers(ctx);
        self.warm_restored_tabs(ctx);
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
            self.updater.restart_requested = false;
        }
        if self.pending_exit && !self.editors.iter().any(commit::Editor::exit_prompt_open) {
            if self.discard_settings_on_exit {
                self.exit_without_saving(ctx);
            } else {
                self.close(ctx);
            }
        }
    }
    fn open_stashes(&mut self, selected: Option<String>) {
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
        let mut view = if let Some(index) = self
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
        if let Some(hash) = selected {
            view.focus(hash);
        }
        self.stash_views.push(view);
        for view in self.stash_views.iter_mut().rev().skip(3) {
            view.release_cache();
        }
    }

    fn reopen_failed_commit(&mut self, path: PathBuf, owner: String, ctx: &egui::Context) {
        if self.git.is_none() || self.closing || self.pending_exit || self.confirm_exit_without_save
        {
            return;
        }
        if self.tabs.len() >= 20 && !self.tabs.iter().any(|tab| tab.matches_path(&path)) {
            self.notice = Some(strings::text("TooManyRepositories"));
            return;
        }
        if self.tabs.iter().chain(&self.retained_tabs).any(|tab| {
            tab.matches_path(&path)
                && (tab.view.identity_blocked
                    || tab.observed_snapshot().is_some_and(|snapshot| {
                        !snapshot.identity.matches_recorded_fingerprint(&owner)
                    }))
        }) {
            self.notice = Some(strings::text("FailedCommitOwnerUnavailable"));
            return;
        }
        let index = self.ensure_tab(path);
        self.tabs[index].view.reopen_commit_owner = Some(owner);
        self.select_tab(index, ctx);
    }

    fn open_commit(&mut self) {
        let Some(tab) = self.tabs.get(self.selected) else {
            return;
        };
        if tab.view.identity_blocked {
            self.notice = tab.error.clone();
            return;
        }
        let expected = tab
            .observed_snapshot()
            .map(|s| s.identity.fingerprint.clone())
            .or_else(|| self.known_identity(&tab.path));
        let tab = &mut self.tabs[self.selected];
        let inline = self.settings.commit_in_workspace();
        let observed_recently = tab
            .view
            .watch
            .as_ref()
            .is_some_and(|watch| watch.available())
            && tab
                .view
                .validated
                .is_some_and(|time| time.elapsed().as_secs() < 30);
        self.editors.retain(|editor| {
            !(tab.matches_path(&editor.path)
                && !editor.matches_owner(expected.as_deref())
                && editor.can_retire())
        });
        // A draft from the old repository can remain open for copying. A
        // deliberately reopened replacement receives its own editor and draft.
        for editor in &mut self.editors {
            if tab.matches_path(&editor.path) && !editor.matches_owner(expected.as_deref()) {
                editor.block_repository(&strings::text("IdentityChanged"));
                editor.detach_replaced();
            }
        }
        if let Some(editor) = self.editors.iter_mut().find(|editor| {
            tab.matches_path(&editor.path) && editor.matches_owner(expected.as_deref())
        }) {
            if inline && !editor.open && (!observed_recently || editor.current_context().is_none()) {
                editor.external_changed();
            }
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
            editor.owner_fingerprint = expected;
            if let Some(rows) = self.tab_markers.preview_rows(tab) {
                editor.seed_preview(rows);
            }
            editor.authentication = Some(self.operations.external_authentication());
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
        self.editors
            .iter()
            .position(|editor| editor.inline && editor.open && tab.owns_editor(editor))
    }

    fn show_history(&mut self, ctx: &egui::Context) {
        if let Some(tab) = self.tabs.get_mut(self.selected) {
            tab.view.reopen_commit_owner = None;
            if std::mem::take(&mut tab.local_changes) {
                self.sync_inline_editor(ctx);
            }
        }
    }

    fn sync_inline_editor(&mut self, ctx: &egui::Context) {
        let target = self.tabs.get(self.selected).filter(|tab| tab.local_changes);
        for editor in &mut self.editors {
            if editor.inline
                && editor.open
                && !editor.exit_prompt_open()
                && target.is_none_or(|tab| !tab.owns_editor(editor))
            {
                editor.suspend(ctx);
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
        self.sync_inspector_layouts(true);
        if self.updater.restart_requested
            && self.updater.needs_tool_confirmation(self.external_tools_opened)
        {
            self.pending_exit = false;
            self.updater.restart_requested = false;
            self.updater.open = true;
            return false;
        }
        if self.git_selection.busy() {
            self.notice = Some(strings::text("RustGitSelecting"));
            return false;
        }
        if self.initialization.picking || self.repository_picker.is_some()
            || self.repository_dialog.as_ref().is_some_and(repository_menu::Dialog::picking)
        {
            self.notice = Some(strings::text("RustRepositoryChooserOpen"));
            return false;
        }
        if let Some(notice) = self.editors.iter().find_map(commit::Editor::picker_notice) {
            self.pending_exit = false;
            self.notice = Some(notice);
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
        let work = self.scheduler.active_records();
        if work
            .iter()
            .any(|record| !record.is_automatic_fetch())
        {
            self.pending_exit = false;
            self.discard_settings_on_exit = false;
            self.notice = Some(strings::text("AppCommandRunningClose"));
            self.updater.restart_requested = false;
            return false;
        }
        if !self.auto_fetch.drained() || !work.is_empty() {
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
            if let Some(editor) = self
                .editors
                .iter_mut()
                .find(|editor| editor.draft_problem().is_some())
            {
                self.pending_exit = false;
                self.notice = editor.draft_problem().map(str::to_owned);
                editor.inline = false;
                editor.activate();
            } else {
                self.pending_exit = true;
                self.notice = Some(strings::text("CommitDraftSaving"));
                ctx.request_repaint_after(std::time::Duration::from_millis(25));
            }
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
        if self.closing || self.confirm_exit_without_save {
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
        self.refresh_avatar_sources(ctx);
        ui_scale::set(ctx, self.settings.ui_scale() as f32);
        theme(
            ctx,
            self.settings.get("Appearance").and_then(|v| v.as_u64()) == Some(1),
        );
        for tab in self.tabs.iter_mut().chain(self.retained_tabs.iter_mut()) {
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
            if editor.inline != inline && !editor.exit_prompt_open() && !editor.native_picker_open()
            {
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
            operations::git_error_message(&strings::text("HistoryFailed"), &output)
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
fn prime_comparison(tab: &mut Tab, git: &Arc<Git>, settings: &Settings, workers: &read_workers::Workers, ctx: &egui::Context) {
    if tab.comparison.is_some() {
        return;
    }
    let Some(commit) = tab.page.as_ref().and_then(|page| page.entries.iter()
        .find(|entry| Some(&entry.commit.hash) == tab.selected.as_ref()))
    else { return; };
    let mut pane = comparison::Pane::new(tab.path.clone(), git.clone()).with_read_workers(workers.clone());
    pane.details_index = settings.get("RustDetailsTab").and_then(|value| value.as_u64()).unwrap_or(0).min(1) as usize;
    pane.view.side_by_side = settings.get("DiffSideBySide").and_then(|value| value.as_bool()).unwrap_or(false);
    pane.view.font_size = settings.get("DiffFontSize").and_then(|value| value.as_f64()).unwrap_or(12.) as f32;
    pane.select(&commit.commit, ctx);
    tab.comparison = Some(pane);
}

fn recovery_controls_visible(
    snapshot: &shovel_core::git::Snapshot,
    scheduler: &shovel_core::scheduler::Scheduler,
) -> bool {
    let rebase = snapshot.operations.iter().any(|name| name.starts_with("rebase"));
    if !rebase {
        return true;
    }
    snapshot.recovery.as_ref().is_some_and(|state| !state.current.is_empty())
        && !scheduler.active_records().iter().any(|record| {
            record.running
                && record.lane == shovel_core::scheduler::Lane::Exclusive
                && record.repository == snapshot.identity.fingerprint
        })
}

impl eframe::App for Workspace {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.receive(ctx);
    }
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let activity_was_open = self.activity_open;
        let mut activity_anchor = None;
        let ctx = ui.ctx().clone();
        let popup_was_open = egui::Popup::is_any_open(&ctx);
        self.branches.read_workers = self.read_workers.clone();
        self.sync_ui_scale(&ctx);
        self.main_geometry.observe(&ctx);
        let sidebar_commit = self.settings.commit_in_workspace();
        let workspace_modal = self.closing
            || self.pending_exit
            || self.confirm_exit_without_save
            || self.git_selection.busy()
            || self.settings_dialog.is_some()
            || self.operations.authentication_open()
            || self.initialization.is_open();
        let modal_open = workspace_modal
            || self
                .editors
                .iter()
                .any(|editor| editor.open && !editor.inline)
            || self.operations.modal_open()
            || self.remotes.is_open()
            || self.branches.is_open()
            || self.repository_dialog.is_some()
            || self.stash_views.iter().any(|view| view.open);
        let inline_prompt = self
            .inline_editor()
            .is_some_and(|i| self.editors[i].has_prompt());
        let shortcuts_blocked = modal_open || inline_prompt;
        if workspace_modal {
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
        if !shortcuts_blocked {
            self.repository_shortcuts(&ctx);
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
        let focused = ctx.input(|i| {
            i.events.iter().any(|event| matches!(event, egui::Event::WindowFocused(true)))
        });
        if focused {
            for tab in &self.tabs {
                self.tab_markers.invalidate(tab.id);
            }
            if self.tabs.get(self.selected).is_some_and(|tab| {
                tab.view
                    .validated
                    .is_none_or(|at| at.elapsed() >= std::time::Duration::from_secs(30))
                    || tab.view.watch.as_ref().is_none_or(|watch| !watch.available())
            }) {
                self.activate_history(self.selected, &ctx);
            }
        }
        let system_changed = caption::take_system_change();
        if system_changed {
            numbers::refresh();
            dates::invalidate(&ctx);
        }
        if system_changed || focused {
            theme(
                &ctx,
                self.settings
                    .get("Appearance")
                    .and_then(serde_json::Value::as_u64)
                    == Some(1),
            );
        }
        if !workspace_modal && !inline_prompt && !self.tabs.is_empty() {
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
            .frame(workspace_style::pane(palette.chrome, 6, 0))
            .show(ui, |ui| {
                if modal_open {
                    ui.disable();
                }
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
                        let recent = self.recent_repositories();
                        ui.add_enabled_ui(!recent.is_empty(), |ui| {
                            ui.menu_button(strings::text("RecentRepositories"), |ui| {
                                for path in recent {
                                    let allowed = self.tabs.len() < 20 || self.tabs.iter().any(|tab| tab.matches_path(&path));
                                    if ui.add_enabled(allowed, egui::Button::new(path.to_string_lossy()).truncate())
                                        .on_disabled_hover_text(strings::text("TooManyRepositories")).clicked()
                                    {
                                        self.open(path, &ctx);
                                        ui.close();
                                    }
                                }
                            });
                        });
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
                        let newer = self.tabs.get(self.selected).is_some_and(|tab| {
                            !tab.loading && !tab.view.identity_blocked
                                && tab.page.as_ref().is_some_and(|page| page.cursor.offset > 0)
                        });
                        let unavailable = self
                            .tabs
                            .get(self.selected)
                            .map(|t| t.history_notice(&ctx))
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
                            ui.label(tab.history_notice(&ctx));
                        }
                    });
                    ui.menu_button(strings::text("RepositoryMenu"), |ui| self.repository_menu(ui));
                    ui.menu_button(strings::text("WindowMenu"), |ui| {
                        if ui.button(strings::text("WindowMinimize")).clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
                            ui.close();
                        }
                        let maximized = ctx.input(|input| input.viewport().maximized.unwrap_or(false));
                        if ui.button(strings::text(if maximized { "WindowRestore" } else { "WindowMaximize" })).clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
                            ui.close();
                        }
                        if ui.button(strings::text("WindowClose")).clicked() {
                            self.close(&ctx);
                            ui.close();
                        }
                    });
                    ui.menu_button(strings::text("HelpMenu"), |ui| {
                        for key in ["HelpReleaseNotes", "HelpShortcuts"] {
                            if ui.button(strings::text(key)).clicked() {
                                repository_menu::open_help(&ctx, key);
                                ui.close();
                            }
                        }
                        if ui.button(strings::text("RustUpdatesTitle")).clicked() {
                            self.updater.open_window(&ctx);
                            ui.close();
                        }
                    });
                    caption::controls(ui);
                });
                ui.allocate_ui_with_layout(
                    egui::vec2(ui.available_width(), workspace_style::TOOLBAR_HEIGHT),
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
                        self.operations.toolbar(
                            ui,
                            &self.settings,
                            self.tabs.get(self.selected).map(|t| &t.path),
                            self.tabs
                                .get(self.selected)
                                .and_then(|t| t.history.as_ref())
                                .map(|h| &h.snapshot),
                            self.git.as_ref(),
                            &self.scheduler,
                        );
                        if !sidebar_commit {
                            let count = self
                                .tabs
                                .get(self.selected)
                                .and_then(|tab| self.tab_markers.count(tab))
                                .filter(|count| *count > 0)
                                .map(|count| count.to_string());
                            if workspace_style::action_with_count(
                                ui,
                                "Commit",
                                self.git.is_some() && !self.tabs.is_empty(),
                                count.as_deref().map(|count| (count, "WorkspaceLocalChanges")),
                            )
                            .clicked()
                            {
                                self.open_commit();
                            }
                        }
                        let left = ui.next_widget_position().x;
                        let right = ui.max_rect().right() - 215.;
                        let update_slot = updater::UPDATE_CARD_WIDTH + 6.;
                        let context_width = (right - left - update_slot).clamp(105., 246.);
                        let centered = ui.max_rect().center().x - context_width / 2.;
                        let context_left =
                            centered.min(right - context_width).max(left + update_slot);
                        ui.add_space((context_left - update_slot - left).max(0.));
                        let offer = self.updater.available_version(self.settings.channel());
                        let (update_rect, _) = ui.allocate_exact_size(
                            egui::vec2(updater::UPDATE_CARD_WIDTH, workspace_style::TOOLBAR_HEIGHT),
                            egui::Sense::hover(),
                        );
                        if offer.is_some() {
                            let mut slot = ui.new_child(
                                egui::UiBuilder::new()
                                    .max_rect(update_rect)
                                    .layout(egui::Layout::left_to_right(egui::Align::Center)),
                            );
                            if slot
                                .add_enabled_ui(self.updater.can_update(), |ui| {
                                    updater::offer_card(ui, offer)
                                })
                                .inner
                            {
                                self.updater.start_update(&ctx);
                            }
                        }
                        ui.add_space(2.);
                        let context_response = egui::Frame::new()
                            .fill(palette.raised)
                            .stroke(egui::Stroke::new(1., palette.border))
                            .corner_radius(6)
                            .inner_margin(egui::Margin::symmetric(10, 0))
                            .show(ui, |ui| {
                                ui.set_width((context_width - 22.).max(1.));
                                ui.spacing_mut().item_spacing.y = 1.;
                                ui.vertical_centered(|ui| {
                                    if let Some(tab) = self.tabs.get(self.selected) {
                                        let name = tab.name();
                                        let name = path_text::PathText::new(&ctx, &name);
                                        let root = tab.path.to_string_lossy();
                                        let root = path_text::PathText::new(&ctx, &root);
                                        let label = ui
                                            .add(
                                                egui::Label::new(
                                                    RichText::new(name.visual()).size(15.).strong(),
                                                )
                                                .truncate(),
                                            )
                                            .on_hover_text(root.visual());
                                        if name.reordered() {
                                            ctx.accesskit_node_builder(label.id, |node| {
                                                node.set_value(name.spoken())
                                            });
                                        }
                                        let branch = tab
                                            .history
                                            .as_ref()
                                            .map(|h| &h.snapshot)
                                            .or_else(|| tab.view.preview.as_ref().map(|(s, _)| s))
                                            .and_then(|snapshot| snapshot.branch.as_deref())
                                            .unwrap_or_default();
                                        let editor =
                                            self.editors.iter().find(|e| tab.owns_editor(e));
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
                                        let width = ui.available_width();
                                        let branch_width = ui
                                            .painter()
                                            .layout_no_wrap(
                                                branch.to_owned(),
                                                egui::FontId::proportional(14.),
                                                palette.text,
                                            )
                                            .size()
                                            .x
                                            .min(width * 0.55);
                                        ui.allocate_ui_with_layout(
                                            egui::vec2(width, 17.),
                                            egui::Layout::left_to_right(egui::Align::Center),
                                            |ui| {
                                                ui.spacing_mut().item_spacing.x = 4.;
                                                ui.add_space((width - branch_width) / 2.);
                                                if !branch.is_empty() {
                                                    ui.add_sized(
                                                        egui::vec2(branch_width, 17.),
                                                        egui::Label::new(
                                                            RichText::new(branch).size(14.),
                                                        )
                                                        .truncate(),
                                                    )
                                                    .on_hover_text(branch);
                                                }
                                                if active {
                                                    ui.add(egui::Spinner::new().size(13.));
                                                }
                                                if let Some(status) = status {
                                                    ui.add(
                                                        egui::Label::new(
                                                            RichText::new(&status)
                                                                .small()
                                                                .color(palette.muted),
                                                        )
                                                        .truncate(),
                                                    )
                                                    .on_hover_text(status);
                                                }
                                            },
                                        );
                                    } else {
                                        ui.label(
                                            RichText::new(strings::text("AppTitle"))
                                                .size(15.)
                                                .strong(),
                                        );
                                        ui.allocate_space(egui::vec2(0., 17.));
                                    }
                                });
                            });
                        let rect = context_response.response.rect;
                        activity_anchor = Some(rect.center_bottom() + egui::vec2(0., 4.));
                        let activity = context_response.response.interact(egui::Sense::click());
                        let activity_title = strings::text("ActivityTitle");
                        activity.widget_info(|| {
                            egui::WidgetInfo::labeled(
                                egui::Role::Button,
                                ui.is_enabled(),
                                &activity_title,
                            )
                        });
                        if activity.on_hover_text(activity_title).clicked() {
                            self.activity_open = !self.activity_open;
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
                    ui.label(summary);
                }
                if let Some(notice) = &self.notice {
                    ui.colored_label(palette.diff_removed_text, notice);
                }
            });
        if self.activity_open {
            let mut open = true;
            let selected_path = self.tabs.get(self.selected).map(|t| t.path.clone());
            let available = ctx.content_rect();
            let min_size = egui::vec2(360., 180.);
            let size = egui::vec2(760., 415.)
                .min(available.size() - egui::vec2(16., 80.))
                .max(min_size);
            let mut window = egui::Window::new(strings::text("ActivityTitle"))
                .id(egui::Id::unique("workspace-activity"))
                .open(&mut open)
                .enabled(!modal_open && !self.closing)
                .default_size(size)
                .min_size(min_size)
                .resizable(true)
                .collapsible(false);
            if let Some(anchor) = activity_anchor {
                let x = (anchor.x - size.x / 2.)
                    .min(available.right() - size.x)
                    .max(available.left());
                let pos = egui::pos2(x, anchor.y);
                window = window.default_pos(pos);
                if !activity_was_open {
                    window = window.current_pos(pos);
                }
            }
            let reopen = window
                .show(&ctx, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        for (filter, key) in [
                            (operations::ActivityFilter::All, "ActivityAll"),
                            (operations::ActivityFilter::User, "ActivityUser"),
                            (operations::ActivityFilter::Background, "ActivityBackground"),
                        ] {
                            ui.selectable_value(&mut self.operations.filter, filter, strings::text(key));
                        }
                        ui.separator();
                        ui.checkbox(&mut self.activity_all, strings::text("ActivityAllRepositories"));
                    });
                    ui.separator();
                    let scope = selected_path.as_ref().filter(|_| !self.activity_all);
                    self.operations.activity(ui, scope, &self.scheduler)
                })
                .and_then(|response| response.inner)
                .flatten();
            self.activity_open = open;
            if let Some((path, owner)) = reopen {
                self.reopen_failed_commit(path, owner, &ctx);
            }
        }
        egui::Panel::top("repository-tabs")
            .frame(workspace_style::pane(
                if palette.high_contrast { palette.tab_strip } else { egui::Color32::from_gray(16) },
                0, 0,
            ))
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.x = 0.;
                let count = self.tabs.len();
                let width = if count == 0 {
                    0.
                } else {
                    ui.available_width() / count as f32
                };
                let mut selected = None;
                let pointer = ctx.input(|i| {
                    i.pointer
                        .interact_pos()
                        .filter(|_| i.pointer.primary_down() || self.tab_drag.is_some())
                });
                let mut nearest: Option<(f32, usize, egui::Rect)> = None;
                let mut action = None;
                ui.horizontal(|ui| {
                    for (index, tab) in self.tabs.iter().enumerate() {
                        let name = tab.name();
                        let name = path_text::PathText::new(&ctx, &name);
                        let root = tab.path.to_string_lossy();
                        let root = path_text::PathText::new(&ctx, &root);
                        let marker = tab_markers::color(&self.settings, &tab.path);
                        let status = if self.git.is_some() {
                            self.tab_markers.status(tab)
                        } else {
                            tab_markers::Status::Unavailable
                        };
                        let (rect, response) = ui.allocate_exact_size(egui::vec2(width,26.), egui::Sense::click_and_drag());
                        let active = index == self.selected;
                        if active || response.hovered() || response.has_focus() {
                            ui.painter().rect_filled(rect, 0., if active { palette.selected } else { palette.hover });
                        }
                        let mut title_rect = rect.shrink2(egui::vec2(4., 0.));
                        if marker.is_some() && width > 20. {
                            title_rect.min.x = rect.left() + 19.;
                            if width >= 64. { title_rect.max.x = rect.right() - 19.; }
                        }
                        let color = if palette.high_contrast && (active || response.hovered() || response.has_focus()) {
                            palette.selection_text
                        } else if palette.high_contrast { palette.text } else { egui::Color32::from_gray(225) };
                        let painter = ui.painter().with_clip_rect(rect.intersect(ui.clip_rect()));
                        let mut job = egui::text::LayoutJob::simple_singleline(name.visual().to_owned(), egui::FontId::proportional(13.), color);
                        job.wrap.max_width = title_rect.width().max(1.);
                        job.wrap.max_rows = 1;
                        let galley = painter.layout_job(job);
                        painter.galley(title_rect.center() - galley.size()/2., galley, color);
                        response.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, ui.is_enabled(), active, name.spoken()));
                        if let Some(color) = marker {
                            tab_markers::paint_marker(ui.painter(), response.rect.left_center() + egui::vec2(10., 0.), color, status);
                        }
                        if index + 1 < count {
                            ui.painter().vline(
                                response.rect.right() - 0.5,
                                (response.rect.top() + 4.)..=(response.rect.bottom() - 4.),
                                egui::Stroke::new(1., palette.border),
                            );
                        }
                        ctx.accesskit_node_builder(response.id, |node| {
                            if name.reordered() {
                                node.set_label(name.spoken());
                            }
                            node.set_description(root.spoken());
                        });
                        if response.drag_started() {
                            self.tab_drag =
                                Some((tab.id, self.tabs.iter().map(|t| t.id).collect()));
                        }
                        if let Some(pointer) = pointer {
                            let distance = (response.rect.center().x - pointer.x).abs();
                            if nearest.is_none_or(|(previous, _, _)| distance < previous) {
                                nearest = Some((distance, index, response.rect));
                            }
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
                                ("RepositoryOpenExplorer", TabAction::OpenTool(tab.id, "Explorer"), true),
                                ("RepositoryOpenConsole", TabAction::OpenTool(tab.id, "Bash"), true),
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
                            if let Some(color) = tab_markers::picker(ui, marker) {
                                action = Some(TabAction::Marker(tab.id, color));
                                ui.close();
                            }
                        });
                        if marker.is_some() {
                            response.on_hover_text(format!(
                                "{}\n{}",
                                root.visual(),
                                strings::text(status.label())
                            ));
                        } else {
                            response.on_hover_text(root.visual());
                        }
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
                        let target = nearest.map(|(_, index, _)| index);
                        if let (Some(target), Some(from)) =
                            (target, self.tabs.iter().position(|t| t.id == *drag))
                            && target != from
                        {
                            let moved = self.tabs.remove(from);
                            self.tabs.insert(target, moved);
                        }
                        if ctx.input(|i| i.pointer.primary_down()) {
                            if let (Some((_, _, slot)), Some(pointer), Some(tab)) =
                                (nearest, pointer, self.tabs.iter().find(|t| t.id == *drag))
                            {
                                workspace_tabs::paint_drag(
                                    &ctx,
                                    slot,
                                    pointer,
                                    &tab.name(),
                                    palette,
                                );
                            }
                        } else {
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
        if let Some(tab) = self.tabs.get(self.selected)
            && let Some(history) = &tab.history
            && !history.snapshot.operations.is_empty()
            && recovery_controls_visible(&history.snapshot, &self.scheduler)
        {
            let snapshot = &history.snapshot;
            egui::Panel::top("repository-recovery")
                .frame(workspace_style::pane(palette.chrome, 6, 3))
                .show(ui, |ui| {
                    ui.add_enabled_ui(!modal_open, |ui| {
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
                            ui.label(strings::text(if snapshot.operations == ["bisect"] {
                                "RepositoryBisectActive"
                            } else {
                                "RecoveryUnavailable"
                            }));
                        }
                    });
                });
        }
        use workspace_split::Divider;
        let width = ui.available_width();
        let mut locate_request = None;
        let mut stash_request = None;
        let mut submodule_open = None;
        let sidebar = egui::Panel::left("sidebar")
            .frame(workspace_style::pane(palette.sidebar, 8, 4))
            .exact_size(Divider::Sidebar.size(ui, &self.settings, width))
            .resizable(false)
            .show(ui, |ui| {
                if modal_open {
                    ui.disable();
                }
                if sidebar_commit && self.tabs.get(self.selected).is_some() {
                    let local = self.inline_editor().is_some();
                    let row_size = egui::vec2(ui.available_width(), 24.);
                    let navigation = |selected, label| {
                        egui::Button::selectable(selected, (label, egui::Atom::grow()))
                            .min_size(row_size)
                            .truncate()
                    };
                    if ui.add(navigation(!local, strings::text("FileHistoryTab"))).clicked() {
                        self.show_history(&ctx);
                    }
                    let tab = &self.tabs[self.selected];
                    let label = self.tab_markers.count(tab).map_or_else(
                        || strings::text("WorkspaceLocalChanges"),
                        |count| strings::format("WorkspaceLocalChangesCount", &[&count.to_string()]),
                    );
                    if ui.add_enabled(
                        self.git.is_some() && self.draft_store.is_some(),
                        navigation(local, label),
                    ).clicked() {
                        self.open_commit();
                    }
                    ui.separator();
                }
                if let Some(tab) = self.tabs.get_mut(self.selected)
                    && let Some(history) = &tab.history
                {
                    egui::ScrollArea::vertical().id_salt(("repository-sidebar", tab.id)).show(ui, |ui| {
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
                                                            .filter(|r| r.name.starts_with(&prefix) && history_row::visible_ref(&r.name))
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
                                            let stash = key == "Stashes";
                                            let response = workspace_style::reference_row(
                                                ui,
                                                if stash {
                                                    tab.view.stash_selection.contains(reference)
                                                } else {
                                                    tab.selected.as_deref() == Some(&reference.hash)
                                                },
                                                &label,
                                                key,
                                            )
                                            .on_hover_text(if stash {
                                                strings::text("StashSidebarSelect")
                                            } else {
                                                reference.name.clone()
                                            });
                                            let activated = response.double_clicked()
                                                || response.has_focus()
                                                    && ui.input_mut(|i| {
                                                        i.consume_key(
                                                            egui::Modifiers::NONE,
                                                            egui::Key::Enter,
                                                        )
                                                    });
                                            let modifiers = ui.input(|i| i.modifiers);
                                            if stash && response.clicked() {
                                                tab.view.stash_selection.select(reference, &history.snapshot.references, modifiers);
                                            }
                                            let multiselect = stash && (modifiers.ctrl || modifiers.shift);
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
                                            } else if !multiselect && (response.clicked() || activated) {
                                                if stash {
                                                    if activated {
                                                        stash_request = Some(reference.hash.clone());
                                                    }
                                                } else {
                                                    locate_request = Some(reference.hash.clone());
                                                }
                                            }
                                            if reference.name.starts_with("refs/heads/")
                                                || reference.name.starts_with("stash@{")
                                            {
                                                if response.secondary_clicked() {
                                                    self.reference_menu = Some(ReferenceMenu {
                                                        owner: tab.id,
                                                        snapshot: history.snapshot.clone(),
                                                        reference: reference.clone(),
                                                        stash_targets: if stash {
                                                            tab.view.stash_selection.context_targets(reference, &history.snapshot.references)
                                                        } else {
                                                            vec![]
                                                        },
                                                    });
                                                }
                                                response.context_menu(|ui| {
                                                    if let Some(menu) = &self.reference_menu
                                                        && menu.owner == tab.id
                                                    {
                                                        if menu.stash_targets.len() > 1 {
                                                            let count = menu.stash_targets.len().to_string();
                                                            if ui.add_enabled(
                                                                self.git.is_some(),
                                                                egui::Button::new(strings::format("StashDropManyMenu", &[&count])),
                                                            ).clicked() {
                                                                self.branches.confirm_stashes_drop(&menu.snapshot, menu.stash_targets.clone());
                                                                ui.close();
                                                            }
                                                        } else if menu
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
        if let Some(hash) = stash_request {
            self.open_stashes(Some(hash));
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
                let bounds = ui.max_rect();
                let mut content = ui.new_child(egui::UiBuilder::new().max_rect(bounds));
                content.set_clip_rect(ui.clip_rect().intersect(bounds));
                let ui = &mut content;
                if modal_open {
                    ui.disable();
                }
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
                    if tab.view.preview.is_some() || tab.view.initial_history {
                        egui::ScrollArea::vertical()
                            .auto_shrink([false, false])
                            .show(ui, |ui| comparison::metadata(ui, &commit, &tab.path));
                        return;
                    }
                    if tab.comparison.is_none()
                        && let Some(git) = &self.git
                    {
                        prime_comparison(tab, git, &self.settings, &self.read_workers, &ctx);
                    }
                    if let Some(pane) = &mut tab.comparison {
                        pane.view.font_size = self
                            .settings
                            .get("DiffFontSize")
                            .and_then(|v| v.as_f64())
                            .unwrap_or(12.) as f32;
                        pane.select(&commit, &ctx);
                        let before = pane.details_index;
                        let changed = ui
                            .push_id(("details-viewport", tab.id), |ui| {
                                pane.show_history(ui, &mut self.settings)
                            })
                            .inner;
                        self.preferences_dirty |= changed;
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
        let mut history_edge = None;
        let mut history_action = None;
        egui::CentralPanel::default()
            .frame(workspace_style::pane(palette.canvas, 0, 0))
            .show(ui, |ui| {
                if modal_open {
                    ui.disable();
                }
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
                if tab.view.preview.is_some() || tab.view.initial_history {
                    ui.label(tab.history_notice(&ctx));
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
                let restoring = tab.view.restore_offset.is_some() || tab.scroll_selected;
                if let Some(offset) = tab.view.restore_offset.take() {
                    scroll = scroll.scroll_offset(offset);
                }
                use history_row::HEIGHT;
                if std::mem::take(&mut tab.scroll_selected)
                    && let Some(index) = page
                        .entries
                        .iter()
                        .position(|e| Some(&e.commit.hash) == tab.selected.as_ref())
                {
                    scroll = scroll.vertical_scroll_offset(((index as f32 - 2.) * HEIGHT).max(0.));
                }
                let scroll_output = scroll.show_rows(ui, HEIGHT, page.entries.len(), |ui, range| {
                    for index in range {
                        let entry = &page.entries[index];
                        {
                            let selected = tab.selected.as_ref() == Some(&entry.commit.hash);
                            let peer = selected_author.is_some_and(|(name, email)| {
                                !email.trim().is_empty()
                                    && &entry.commit.author == name
                                    && &entry.commit.email == email
                            });
                            let response = ui
                                .push_id(&entry.commit.hash, |ui| {
                                    history_row::show(
                                        ui, entry, lanes,
                                        history_row::Style {
                                            selected, peer, refs_right,
                                            worktree: &tab.path,
                                            incoming: tab.divergence.is_incoming(&entry.commit.hash),
                                        },
                                        &palette,
                                    )
                                })
                                .inner;
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
                                    && menu.commit.hash == entry.commit.hash
                                    && let Some(command) = menu.show(
                                        ui,
                                        self.git.is_some() && !tab.view.stale && !tab.view.identity_blocked,
                                    )
                                {
                                    history_action = Some((menu.clone(), command));
                                }
                            });
                        }
                    }
                });
                tab.view.offset = scroll_output.state.clamped_offset();
                if !modal_open && !restoring && !tab.loading && !tab.view.identity_blocked
                    && ctx.pointer_hover_pos().is_some_and(|point| {
                        scroll_output.inner_rect.contains(point)
                            && ctx.layer_id_at(point) == Some(ui.layer_id())
                    })
                {
                    let wheel = ctx.input(history_view::vertical_wheel);
                    let height = scroll_output.inner_rect.height();
                    let bottom = (scroll_output.content_size.y - height).max(0.);
                    if wheel > 0. && tab.view.offset.y <= 0.5 && page.cursor.offset > 0 {
                        history_edge = Some((false, height));
                    } else if wheel < 0.
                        && tab.view.offset.y >= bottom - 0.5
                        && page.next.is_some()
                    {
                        history_edge = Some((true, height));
                    }
                }
            });
        if let Some((menu, command)) = history_action {
            self.invoke_history_menu(menu, command, &ctx);
        }
        if let Some((older, height)) = history_edge {
            self.read_history_page(older, Some(height), &ctx);
        }
        let stash_input = !self.closing
            && !self.operations.modal_open()
            && !self.branches.is_open()
            && !self.remotes.is_open();
        self.sync_inspector_layouts(false);
        for view in &mut self.stash_views {
            view.show(&ctx, &mut self.operations, &self.scheduler, stash_input);
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
        if !self.closing {
            self.initialization.show(
                &ctx,
                self.git.as_ref(),
                &self.scheduler,
                self.tabs.get(self.selected).map(|t| t.id),
                &self.operations,
            );
            self.settings_window(&ctx);
        }
        let update_blocked = self.initialization.picking
            || self.scheduler.has_accepted_work()
            || self.saving
            || self
                .editors
                .iter()
                .any(|e| e.blocks_exit() || e.unsaved_conflict() || e.native_picker_open());
        self.show_repository_dialog(&ctx);
        repository_menu::show_help(&ctx);
        self.updater
            .show(&ctx, update_blocked, self.external_tools_opened);
        if self.updater.restart_requested && !self.closing {
            self.close(&ctx);
        }

        if !self.closing && !self.remotes.is_open() {
            self.operations.open_ready_branch(&mut self.branches);
            if !self.branches.is_open() {
                self.preferences_dirty |= self.operations.options_dialog(
                    &ctx,
                    &mut self.settings,
                    self.git.as_ref(),
                    &self.scheduler,
                );
            }
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
        if popup_was_open && !egui::Popup::is_any_open(&ctx) {
            ctx.request_repaint();
        }
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
            repository_dialog: None,
            remotes: Default::default(),
            initialization: Default::default(),
            stash_views: vec![],
            history_menu: None,
            reference_menu: None,
            deferred_history: Default::default(),
            tab_markers: Default::default(),
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
                qualified_name: None,
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
            repository_dialog: None,
            remotes: Default::default(),
            initialization: Default::default(),
            stash_views: vec![],
            history_menu: None,
            reference_menu: None,
            deferred_history: Default::default(),
            tab_markers: Default::default(),
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
