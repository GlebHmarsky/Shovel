//! Page presentation survives reads; no repository command is owned by this cache.
use super::*;
pub(super) use shovel_core::git::HistoryRead as Read;
use shovel_core::identity::Identity;
use std::{collections::BTreeMap, time::Instant};

const VIEW_LIMIT: usize = 128;
#[derive(Clone)]
struct SavedView {
    selected: Option<String>,
    offset: egui::Vec2,
}
#[derive(Default)]
pub(super) struct State {
    pub generation: u64,
    pub prewarm: bool,
    pub preview: Option<(shovel_core::git::Snapshot, chrono::DateTime<chrono::Utc>)>,
    pub initial_history: bool,
    pub last_use: Option<Instant>,
    pub bytes: usize,
    pub locate_generation: u64,
    pub stale: bool,
    pub identity_blocked: bool,
    pub checking_identity: bool,
    pub reopen_commit_owner: Option<String>,
    pub validated: Option<Instant>,
    pub watch: Option<repository_watch::Watch>,
    pub external_history_change: bool,
    pub stash_selection: stashes::SidebarSelection,
    pub offset: egui::Vec2,
    pub restore_offset: Option<egui::Vec2>,
    views: BTreeMap<usize, SavedView>,
}
impl State {
    pub(super) fn restored() -> Self {
        Self { prewarm: true, ..Self::default() }
    }
    pub fn remember(&mut self, page: Option<&Page>, selected: &Option<String>) {
        if let Some(page) = page {
            self.views.insert(
                page.cursor.offset,
                SavedView {
                    selected: selected.clone(),
                    offset: self.offset,
                },
            );
            while self.views.len() > VIEW_LIMIT {
                self.views.pop_first();
            }
        }
    }
    pub fn restore(&mut self, page: &Page, selected: &mut Option<String>) {
        let saved = self.views.get(&page.cursor.offset);
        if let Some(saved) = saved {
            *selected = saved.selected.clone();
        }
        if !page
            .entries
            .iter()
            .any(|e| Some(&e.commit.hash) == selected.as_ref())
        {
            *selected = page.entries.first().map(|e| e.commit.hash.clone());
        }
        self.offset = saved.map_or(egui::Vec2::ZERO, |s| s.offset);
        self.restore_offset = Some(self.offset);
    }
    pub fn begin(&mut self) -> u64 {
        self.generation = self.generation.wrapping_add(1);
        self.locate_generation = self.locate_generation.wrapping_add(1);
        self.stale = true;
        self.checking_identity = false;
        self.validated = None;
        self.generation
    }
    pub fn published(&mut self) {
        self.stale = false;
        self.validated = Some(Instant::now());
    }
    pub fn clear_views(&mut self) {
        self.views.clear();
        self.stash_selection = Default::default();
        self.offset = egui::Vec2::ZERO;
        self.restore_offset = Some(self.offset);
    }
}
pub(super) fn push_cursor(cursors: &mut Vec<Cursor>, cursor: Cursor) {
    cursors.push(cursor);
    if cursors.len() > VIEW_LIMIT {
        cursors.remove(0);
    }
}
#[derive(Debug)]
pub(super) struct Failure {
    pub message: String,
    pub identity: bool,
}
impl Failure {
    pub(super) fn interrupted() -> Self {
        Self {
            message: strings::text("RustHistoryWorkerFailed"),
            identity: false,
        }
    }
}
impl From<shovel_core::git::Error> for Failure {
    fn from(error: shovel_core::git::Error) -> Self {
        Self {
            message: error_text(error),
            identity: false,
        }
    }
}
fn identity_failure(changed: bool) -> Failure {
    Failure {
        message: strings::text(if changed {
            "IdentityChanged"
        } else {
            "IdentityUnavailable"
        }),
        identity: true,
    }
}

pub(super) fn read_expected(
    git: &Git,
    path: &std::path::Path,
    previous: Option<(&Identity, &str)>,
    first: bool,
    expected_identity: Option<&str>,
    initial_history: Option<&dyn Fn(&Page)>,
    cancel: &Cancellation,
) -> Result<Read, Failure> {
    let identity = shovel_core::identity::read(path).map_err(|_| identity_failure(false))?;
    if expected_identity.is_some_and(|expected| !identity.matches_recorded_fingerprint(expected))
        || previous.is_some_and(|(previous, _)| previous != &identity)
    {
        return Err(identity_failure(true));
    }
    let previous_fingerprint = previous
        .filter(|_| !first)
        .map(|(_, fingerprint)| fingerprint);
    let result = git.read_history(path, &identity, previous_fingerprint, initial_history, cancel);
    if shovel_core::identity::read(path).ok().as_ref() != Some(&identity) {
        return Err(identity_failure(true));
    }
    result.map_err(Into::into)
}
#[cfg(test)]
fn read(
    git: &Git,
    path: &std::path::Path,
    previous: Option<&History>,
    first: bool,
) -> Result<Read, Failure> {
    read_expected(
        git,
        path,
        previous.map(|h| (&h.snapshot.identity, h.snapshot.fingerprint.as_str())),
        first,
        previous.map(|h| h.snapshot.identity.fingerprint.as_str()),
        None,
        &Cancellation::default(),
    )
}
pub(super) fn read_page(
    git: &Git,
    history: &History,
    cursor: &Cursor,
    cancel: &Cancellation,
) -> Result<Page, Failure> {
    let result = git.page(history, cursor, cancel);
    if shovel_core::identity::read(&history.snapshot.worktree)
        .ok()
        .as_ref()
        != Some(&history.snapshot.identity)
    {
        return Err(identity_failure(true));
    }
    result.map_err(Into::into)
}
// Seek only after a changed refresh. Each page keeps its verified cursor
// chain, so scrolling back still works from the restored position.
pub(super) fn anchor_refresh(
    git: &Git,
    read: Read,
    anchor: Option<(String, usize)>,
    cancel: &Cancellation,
) -> Result<(Read, Vec<Cursor>), Failure> {
    let (Read::Page(history, first), Some((hash, prior_offset))) = (&read, anchor) else {
        return Ok((read, Vec::new()));
    };
    if first.entries.iter().any(|entry| entry.commit.hash == hash) {
        return Ok((read, Vec::new()));
    }
    let mut previous = Vec::new();
    let mut page = first.as_ref().clone();
    let mut nearest = None;
    while let Some(next) = page.next.clone() {
        if next.offset > prior_offset.saturating_add(shovel_core::git::PAGE_SIZE * 2) {
            break;
        }
        push_cursor(&mut previous, page.cursor);
        page = read_page(git, history, &next, cancel)?;
        if page.entries.iter().any(|entry| entry.commit.hash == hash) {
            return Ok((Read::Page(history.clone(), Box::new(page)), previous));
        }
        if page.cursor.offset <= prior_offset {
            nearest = Some((page.clone(), previous.clone()));
        }
    }
    if let Some((page, previous)) = nearest {
        return Ok((Read::Page(history.clone(), Box::new(page)), previous));
    }
    Ok((read, Vec::new()))
}

pub(super) fn vertical_wheel(input: &egui::InputState) -> f32 {
    input.events.iter().filter_map(|event| match event {
        egui::Event::MouseWheel { delta, modifiers, source, .. }
            if *source != egui::MouseWheelSource::Momentum
                && !modifiers.shift && !modifiers.ctrl && !modifiers.command
                && delta.y.abs() > delta.x.abs() => Some(delta.y),
        _ => None,
    }).sum()
}
impl Workspace {
    pub(super) fn warm_restored_tabs(&mut self, ctx: &egui::Context) {
        if self.closing || self.pending_exit || self.confirm_exit_without_save
            || self.git_selection.busy() || self.scheduler.has_accepted_work()
            || self.tabs.iter().any(|tab| tab.loading || tab.locating.is_some())
        {
            return;
        }
        let Some(active) = self.tabs.get(self.selected) else { return; };
        let Some(validated) = active.view.validated.filter(|_| active.history.is_some()) else { return; };
        let Some(id) = self.tabs.iter().find(|tab| {
            tab.id != active.id && tab.view.prewarm && tab.page.is_none()
                && tab.error.is_none() && !tab.view.identity_blocked
        }).map(|tab| tab.id) else { return; };
        let wait = std::time::Duration::from_millis(300).saturating_sub(validated.elapsed());
        if !wait.is_zero() {
            ctx.request_repaint_after(wait);
        } else if self.read_workers.idle() {
            self.refresh_owner(id, ctx);
        } else {
            ctx.request_repaint_after(std::time::Duration::from_millis(150));
        }
    }

    pub(super) fn activate_history(&mut self, index: usize, ctx: &egui::Context) {
        if self.closing || self.pending_exit || self.confirm_exit_without_save {
            return;
        }
        let Some(tab) = self.tabs.get_mut(index) else {
            return;
        };
        if tab.view.identity_blocked {
            return;
        }
        if tab.loading {
            let _ = tab.start_watching(ctx);
            return;
        }
        if tab.history.is_none()
            || tab.view.reopen_commit_owner.is_some()
            || tab.refresh_again
            || tab.view.stale
            || tab.view.watch.as_ref().is_none_or(|watch| !watch.available())
            || tab
                .view
                .validated
                .is_none_or(|at| at.elapsed() >= std::time::Duration::from_secs(30))
        {
            tab.refresh_again = false;
            self.refresh_at(index, ctx);
            return;
        }
        if tab.view.checking_identity {
            return;
        }
        // Cached rows publish immediately. Activation still validates the directory
        // identity off-thread, without running Git for a fresh page.
        tab.view.generation = tab.view.generation.wrapping_add(1);
        tab.view.checking_identity = true;
        let generation = tab.view.generation;
        let id = tab.id;
        let path = tab.path.clone();
        let expected = tab.history.as_ref().unwrap().snapshot.identity.clone();
        let tx = self.sender.clone();
        let ctx = ctx.clone();
        let lease = self.read_workers.start(Cancellation::default());
        thread::spawn(move || {
            let _lease = lease;
            let result = shovel_core::identity::read(&path)
                .map_err(|_| identity_failure(false))
                .and_then(|identity| {
                    if identity == expected {
                        Ok(())
                    } else {
                        Err(identity_failure(true))
                    }
                });
            let _ = tx.send(Event::CachedIdentity(id, generation, result));
            ctx.request_repaint();
        });
    }
}
impl Tab {
    pub(super) fn show_initial_history(&mut self, mut page: Page) {
        self.selected = page.entries.first().map(|entry| entry.commit.hash.clone());
        page.next = None;
        self.page = Some(page);
        self.view.initial_history = true;
        self.estimate_cache();
    }

    pub(super) fn discard_initial_history(&mut self) {
        if std::mem::take(&mut self.view.initial_history) {
            self.page = None;
            self.selected = None;
            self.scroll_selected = false;
            self.view.clear_views();
            self.estimate_cache();
        }
    }

    pub(super) fn publish_history(&mut self, read: Read, first: bool) {
        self.owner_snapshot = Some(history_cache::metadata(read.snapshot()));
        match read {
            Read::Unchanged(snapshot) => self.history.as_mut().unwrap().snapshot = *snapshot,
            Read::Page(history, page) => self.replace_history_page(*history, *page, first),
        }
        if let Some(history) = &self.history {
            self.view.stash_selection.retain(&history.snapshot.references);
        }
        self.view.preview = None;
        self.view.initial_history = false;
        self.error = None;
        self.estimate_cache();
        self.loading = false;
        self.view.published();
    }

    fn replace_history_page(&mut self, history: History, page: Page, first: bool) {
        let anchored_offset = (!first).then(|| {
            let old = self.page.as_ref()?;
            let offset = self.view.offset;
            if old.cursor.offset == 0 && offset.y <= 0.5 {
                return Some(egui::vec2(offset.x, 0.));
            }
            let top = (offset.y / history_row::HEIGHT).floor() as usize;
            let closest = old.entries.iter().enumerate().filter_map(|(index, entry)| {
                let new_index = page.entries.iter().position(|candidate| candidate.commit.hash == entry.commit.hash)?;
                Some((index.abs_diff(top), new_index as f32 - index as f32))
            }).min_by_key(|(distance, _)| *distance);
            Some(egui::vec2(offset.x, closest.map_or(offset.y, |(_, delta)| {
                (offset.y + delta * history_row::HEIGHT).max(0.)
            })))
        }).flatten();
        let selected = self.selected.clone();
        if self.view.preview.is_none() {
            self.view.remember(self.page.as_ref(), &self.selected);
        }
        self.previous.clear();
        self.view.restore(&page, &mut self.selected);
        if !first && page.entries.iter().any(|entry| Some(&entry.commit.hash) == selected.as_ref()) {
            self.selected = selected;
        }
        if let Some(offset) = anchored_offset {
            self.view.offset = offset;
            self.view.restore_offset = Some(offset);
        }
        self.page = Some(page);
        self.history = Some(history);
    }
    pub(super) fn publish_page(&mut self, page: Page, older: bool, viewport_height: Option<f32>) {
        let horizontal = self.view.offset.x;
        self.view.remember(self.page.as_ref(), &self.selected);
        if older {
            if let Some(current) = &self.page {
                push_cursor(&mut self.previous, current.cursor.clone());
            }
        } else {
            self.previous.pop();
        }
        self.view.restore(&page, &mut self.selected);
        if let Some(height) = viewport_height {
            let edge = if older { 0. } else {
                (page.entries.len() as f32 * history_row::HEIGHT - height).max(0.)
            };
            self.view.offset = egui::vec2(horizontal, edge);
            self.view.restore_offset = Some(self.view.offset);
        }
        self.page = Some(page);
        self.estimate_cache();
        self.loading = false;
        self.view.published();
    }
    pub(super) fn history_notice(&self, ctx: &egui::Context) -> String {
        if self.view.initial_history {
            return strings::text("HistoryCheckingRefs");
        }
        if let Some((_, saved_at)) = &self.view.preview {
            return strings::format(
                if self.loading {
                    "HistoryStartupCached"
                } else {
                    "RustHistoryStartupRetained"
                },
                &[&crate::dates::local(ctx, saved_at)],
            );
        }
        if self.loading {
            return strings::text(if self.page.is_some() {
                "HistoryRefreshing"
            } else {
                "HistoryLoading"
            });
        }
        if self.view.stale {
            return strings::text("RustHistoryStale");
        }
        match &self.page {
            Some(page) if page.entries.is_empty() => strings::text("HistoryEmptyAll"),
            Some(page) if page.next.is_none() => strings::text(
                if self.history.as_ref().is_some_and(|h| h.snapshot.shallow) {
                    "HistoryShallowEnd"
                } else {
                    "HistoryEnd"
                },
            ),
            Some(_) => strings::text("RustHistoryMore"),
            None => strings::text("HistoryNotLoaded"),
        }
    }
}

#[cfg(test)]
#[path = "history_view_tests.rs"]
pub(crate) mod tests;
