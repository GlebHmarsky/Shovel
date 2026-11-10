//! Selected startup preview and bounded session history. Only verified reads
//! update repository ownership; a disk preview remains presentation-only.
use super::*;
use shovel_core::{
    git::{Commit, Entry, Snapshot},
    history_cache::{Preview, Store},
};
use std::time::{Duration, Instant};

const TABS: usize = 3;
const BYTES: usize = 32 * 1024 * 1024;

pub(super) fn metadata(snapshot: &Snapshot) -> Snapshot {
    let mut result = snapshot.clone();
    result.references = Vec::new();
    result
}
fn commit_bytes(c: &Commit) -> usize {
    4096 + 2
        * (c.message.len()
            + c.author.len()
            + c.email.len()
            + c.committer.len()
            + c.committer_email.len()
            + c.parents.iter().map(String::len).sum::<usize>())
}
fn entry_bytes(e: &Entry) -> usize {
    commit_bytes(&e.commit)
        + e.graph.lines.len() * std::mem::size_of::<shovel_core::graph::Line>()
        + e.references
            .iter()
            .map(|r| 128 + 2 * (r.name.len() + r.hash.len()))
            .sum::<usize>()
}
impl Tab {
    pub(super) fn observed_snapshot(&self) -> Option<&Snapshot> {
        self.owner_snapshot
            .as_ref()
            .or_else(|| self.history.as_ref().map(|h| &h.snapshot))
    }
    pub(super) fn estimate_cache(&mut self) {
        self.view.bytes = self.page.as_ref().map_or(0, |page| {
            page.entries.iter().map(entry_bytes).sum::<usize>()
        }) + self.history.as_ref().map_or(0, |h| {
            h.snapshot
                .references
                .iter()
                .map(|r| 128 + 2 * (r.name.len() + r.hash.len()))
                .sum::<usize>()
                + h.stashes.iter().map(commit_bytes).sum::<usize>()
        });
    }
    fn evict_history(&mut self) {
        // Keep only previously observed repository metadata. It is not an action
        // context and lets background fetch/accepted ownership survive eviction.
        self.history = None;
        self.page = None;
        self.selected = None;
        self.previous.clear();
        if let Some(pane) = &mut self.comparison {
            pane.cancel();
        }
        self.comparison = None;
        self.locate_cancel.cancel();
        self.locating = None;
        self.scroll_selected = false;
        self.divergence = Default::default();
        self.view.clear_views();
        self.view.preview = None;
        self.view.stale = true;
        self.view.validated = None;
        self.view.bytes = 0;
        self.error = None;
    }
    pub(super) fn apply_preview(&mut self, preview: Preview) {
        self.selected = preview.selected;
        self.page = Some(Page {
            entries: preview.entries,
            next: None,
            cursor: Cursor {
                offset: preview.offset,
                ..Default::default()
            },
        });
        if !self
            .page
            .as_ref()
            .unwrap()
            .entries
            .iter()
            .any(|e| Some(&e.commit.hash) == self.selected.as_ref())
        {
            self.selected = self
                .page
                .as_ref()
                .unwrap()
                .entries
                .first()
                .map(|e| e.commit.hash.clone());
        }
        self.view.offset = egui::vec2(preview.horizontal, preview.vertical);
        self.view.restore_offset = Some(self.view.offset);
        self.view.preview = Some((preview.repository, preview.saved_at));
        self.view.stale = true;
        self.estimate_cache();
    }
}
impl Workspace {
    pub(super) fn known_identity(&self, path: &std::path::Path) -> Option<String> {
        self.settings
            .get("RepositoryIdentities")?
            .as_array()?
            .iter()
            .find_map(|v| {
                let p = v.get("Path")?.as_str()?;
                let fingerprint = v.get("Fingerprint")?.as_str()?;
                (std::path::Path::new(p).is_absolute()
                    && fingerprint.len() == 64
                    && fingerprint.bytes().all(|b| b.is_ascii_hexdigit())
                    && Self::same_path(std::path::Path::new(p), path))
                .then(|| fingerprint.to_owned())
            })
    }
    pub(super) fn forget_identity(&mut self, path: &std::path::Path) {
        let mut values = self
            .settings
            .get("RepositoryIdentities")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        values.retain(|v| {
            v.get("Path")
                .and_then(|p| p.as_str())
                .is_none_or(|p| !Self::same_path(std::path::Path::new(p), path))
        });
        self.settings
            .set("RepositoryIdentities", serde_json::json!(values));
        self.preferences_dirty = true;
    }
    pub(super) fn remember_identity(&mut self, path: &std::path::Path, fingerprint: &str) {
        if self.known_identity(path).as_deref() == Some(fingerprint) {
            return;
        }
        self.forget_identity(path);
        let mut values = self
            .settings
            .get("RepositoryIdentities")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        values.insert(
            0,
            serde_json::json!({"Path":path,"Fingerprint":fingerprint}),
        );
        values.truncate(20);
        self.settings
            .set("RepositoryIdentities", serde_json::json!(values));
    }
    pub(super) fn startup_store(&self) -> Option<Store> {
        Some(Store {
            directory: self.store.as_ref()?.path().parent()?.to_path_buf(),
        })
    }
    pub(super) fn restore_startup_preview(&self, ctx: &egui::Context) {
        let Some(tab) = self.tabs.get(self.selected) else {
            return;
        };
        let (Some(store), Some(identity)) = (self.startup_store(), self.known_identity(&tab.path))
        else {
            return;
        };
        let (path, id, generation) = (tab.path.clone(), tab.id, tab.view.generation);
        let tx = self.sender.clone();
        let ctx = ctx.clone();
        thread::spawn(move || {
            let started = Instant::now();
            let preview = store.read(&path, &identity).filter(|_| {
                started.elapsed() < Duration::from_secs(2)
                    && shovel_core::identity::read(&path)
                        .is_ok_and(|actual| actual.fingerprint == identity)
                    && started.elapsed() < Duration::from_secs(2)
            });
            let _ = tx.send(Event::Preview(id, generation, preview.map(Box::new)));
            ctx.request_repaint();
        });
    }
    pub(super) fn capture_startup_preview(&self) -> Option<Preview> {
        let tab = self.tabs.get(self.selected)?;
        if tab.view.preview.is_some() || tab.view.stale {
            return None;
        }
        let validated = tab.view.validated?;
        let history = tab.history.as_ref()?;
        let page = tab.page.as_ref()?;
        let identity = self.known_identity(&tab.path)?;
        if identity != history.snapshot.identity.fingerprint {
            return None;
        }
        Some(Preview {
            schema: 1,
            directory: tab.path.clone(),
            identity,
            repository: metadata(&history.snapshot),
            entries: page.entries.clone(),
            offset: page.cursor.offset,
            selected: tab.selected.clone(),
            vertical: tab.view.offset.y,
            horizontal: tab.view.offset.x,
            saved_at: chrono::Utc::now() - chrono::Duration::from_std(validated.elapsed()).ok()?,
        })
    }
    pub(super) fn evict_history_caches(&mut self) {
        let selected = self.tabs.get(self.selected).map(|t| t.id);
        let records = self.scheduler.records();
        loop {
            let cached = self
                .tabs
                .iter()
                .chain(self.retained_tabs.iter())
                .filter(|t| t.page.is_some());
            let (count, bytes) = cached.fold((0, 0usize), |(count, bytes), t| {
                (count + 1, bytes.saturating_add(t.view.bytes))
            });
            if count <= TABS && (count <= 1 || bytes <= BYTES) {
                break;
            }
            let oldest = self
                .tabs
                .iter()
                .chain(self.retained_tabs.iter())
                .filter(|t| {
                    t.page.is_some()
                        && Some(t.id) != selected
                        && !t.loading
                        && !t.view.checking_identity
                        && t.locating.is_none()
                        && !self.operations.has_pending(&t.path)
                        && !t.observed_snapshot().is_some_and(|s| {
                            records.iter().any(|r| {
                                r.outcome.is_none() && r.repository == s.identity.fingerprint
                            })
                        })
                        && !self
                            .editors
                            .iter()
                            .any(|e| Self::same_path(&e.path, &t.path) && !e.can_retire())
                        && !self
                            .stash_views
                            .iter()
                            .any(|v| Self::same_path(&v.path, &t.path) && !v.can_retire())
                })
                .min_by_key(|t| t.view.last_use)
                .map(|t| t.id);
            let Some(id) = oldest else { break };
            self.tabs
                .iter_mut()
                .chain(self.retained_tabs.iter_mut())
                .find(|t| t.id == id)
                .unwrap()
                .evict_history();
        }
    }
}

#[cfg(test)]
#[path = "history_cache_tests.rs"]
pub(crate) mod tests;
