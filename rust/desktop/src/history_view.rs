//! Page presentation survives reads; no repository command is owned by this cache.
use super::*;
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
    pub preview: Option<(shovel_core::git::Snapshot, chrono::DateTime<chrono::Utc>)>,
    pub last_use: Option<Instant>,
    pub bytes: usize,
    pub locate_generation: u64,
    pub stale: bool,
    pub identity_blocked: bool,
    pub checking_identity: bool,
    pub validated: Option<Instant>,
    pub offset: egui::Vec2,
    pub restore_offset: Option<egui::Vec2>,
    views: BTreeMap<usize, SavedView>,
}
impl State {
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
    previous: Option<(&History, &Cursor)>,
    first: bool,
    expected_identity: Option<&str>,
    cancel: &Cancellation,
) -> Result<(History, Page, bool), Failure> {
    let identity = shovel_core::identity::read(path).map_err(|_| identity_failure(false))?;
    if expected_identity.is_some_and(|expected| !identity.matches_recorded_fingerprint(expected))
        || previous.is_some_and(|(history, _)| history.snapshot.identity != identity)
    {
        return Err(identity_failure(true));
    }
    let result: Result<_, shovel_core::git::Error> = (|| {
        let history = git.history(git.inspect(path, cancel)?, cancel)?;
        let keep = !first
            && previous.is_some_and(|(old, _)| {
                old.snapshot.fingerprint == history.snapshot.fingerprint
                    && old.snapshot.head == history.snapshot.head
            });
        let cursor = if keep {
            previous.unwrap().1.clone()
        } else {
            Cursor::default()
        };
        let page = git.page(&history, &cursor, cancel)?;
        Ok((history, page, keep))
    })();
    if shovel_core::identity::read(path).ok().as_ref() != Some(&identity) {
        return Err(identity_failure(true));
    }
    result.map_err(Into::into)
}
#[cfg(test)]
fn read(
    git: &Git,
    path: &std::path::Path,
    previous: Option<(&History, &Cursor)>,
    first: bool,
) -> Result<(History, Page, bool), Failure> {
    read_expected(
        git,
        path,
        previous,
        first,
        previous.map(|(h, _)| h.snapshot.identity.fingerprint.as_str()),
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
impl Workspace {
    pub(super) fn activate_history(&mut self, index: usize, ctx: &egui::Context) {
        if self.closing || self.pending_exit || self.confirm_exit_without_save {
            return;
        }
        let Some(tab) = self.tabs.get_mut(index) else {
            return;
        };
        if tab.loading || tab.view.identity_blocked {
            return;
        }
        if tab.history.is_none()
            || tab.refresh_again
            || tab.view.stale
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
    pub(super) fn publish_history(
        &mut self,
        history: History,
        page: Page,
        keep: bool,
        first: bool,
    ) {
        let anchored_offset = (!keep && !first)
            .then(|| {
                self.page
                    .as_ref()
                    .filter(|old| old.cursor.offset == page.cursor.offset)
            })
            .flatten()
            .and_then(|old| {
                let offset = self.view.offset;
                let top = (offset.y / history_row::HEIGHT).floor() as usize;
                let anchor = old.entries.get(top)?;
                let new_top = page
                    .entries
                    .iter()
                    .position(|entry| entry.commit.hash == anchor.commit.hash)?;
                Some(egui::vec2(
                    offset.x,
                    offset.y + (new_top as f32 - top as f32) * history_row::HEIGHT,
                ))
            });
        if self.view.preview.is_none() {
            self.view.remember(self.page.as_ref(), &self.selected);
        }
        if !keep {
            self.previous.clear();
            self.view.restore(&page, &mut self.selected);
            if let Some(offset) = anchored_offset {
                self.view.offset = offset;
                self.view.restore_offset = Some(offset);
            }
        }
        // Keep the actual allocation that the selected/focused rows use.
        // Metadata is immutable for unchanged commits and ref fingerprints.
        let same_rows = keep
            && self.page.as_ref().is_some_and(|old| {
                old.entries
                    .iter()
                    .map(|e| &e.commit.hash)
                    .eq(page.entries.iter().map(|e| &e.commit.hash))
            });
        if same_rows {
            let old = self.page.as_mut().unwrap();
            old.cursor = page.cursor;
            old.next = page.next;
        } else {
            self.page = Some(page);
        }
        self.owner_snapshot = Some(history_cache::metadata(&history.snapshot));
        self.history = Some(history);
        self.view.preview = None;
        self.estimate_cache();
        self.loading = false;
        self.view.published();
    }
    pub(super) fn publish_page(&mut self, page: Page, older: bool) {
        self.view.remember(self.page.as_ref(), &self.selected);
        if older {
            if let Some(current) = &self.page {
                push_cursor(&mut self.previous, current.cursor.clone());
            }
        } else {
            self.previous.pop();
        }
        self.view.restore(&page, &mut self.selected);
        self.page = Some(page);
        self.estimate_cache();
        self.loading = false;
        self.view.published();
    }
    pub(super) fn history_notice(&self, ctx: &egui::Context) -> String {
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
