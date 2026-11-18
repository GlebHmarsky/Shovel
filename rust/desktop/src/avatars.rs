//! Session-only author pictures. Visible rows admit at most two background reads.
use crate::{
    avatar_provider::{self, Request, Source},
    read_workers::Workers,
};
use eframe::egui::{self, ColorImage, TextureHandle};
use shovel_core::{
    git::Commit,
    process::Cancellation,
    settings::{AvatarPreferences, AvatarProvider, avatar_website},
};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use url::Url;

#[derive(Clone, PartialEq, Eq, Hash)]
struct Key {
    source: Source,
    author: String,
}

struct Cached {
    created: Instant,
    image: Option<TextureHandle>,
}
struct Active {
    cancel: Cancellation,
    // A row and a separate Commit overview can share the same request.
    visible: HashMap<egui::ViewportId, (u64, u64)>,
}
#[derive(Default)]
struct State {
    prefs: AvatarPreferences,
    website: Option<Url>,
    sources: HashMap<PathBuf, Source>,
    cache: HashMap<Key, Cached>,
    generation: u64,
    active: HashMap<(u64, Key), Active>,
}
#[derive(Clone)]
struct Session {
    state: Arc<Mutex<State>>,
    workers: Workers,
}
fn session(ctx: &egui::Context) -> Option<Session> {
    ctx.data(|d| d.get_temp(egui::Id::unique("author-pictures")))
}
pub fn install(ctx: &egui::Context, workers: Workers) {
    let session = Session {
        state: Default::default(),
        workers,
    };
    ctx.data_mut(|d| d.insert_temp(egui::Id::unique("author-pictures"), session.clone()));
    ctx.on_end_pass(
        "Release offscreen author pictures",
        Arc::new(move |ui| {
            let ctx = ui.ctx();
            let mut state = session.state.lock().unwrap();
            if state.active.is_empty() {
                return;
            }
            let viewport = ctx.viewport_id();
            let pass = ctx.cumulative_pass_nr();
            let root_pass = ctx.cumulative_pass_nr_for(egui::ViewportId::ROOT);
            for active in state.active.values_mut() {
                active.visible.retain(|id, (seen, root_seen)| {
                    if *id == viewport {
                        return *seen == pass;
                    }
                    // A new child may first paint before the root receives its OS viewport list.
                    root_pass.saturating_sub(*root_seen) <= 1
                        || ctx.input(|i| i.raw.viewports.contains_key(id))
                });
                if active.visible.is_empty() {
                    active.cancel.cancel();
                }
            }
        }),
    );
}
impl crate::Workspace {
    pub(super) fn refresh_avatar_sources(&self, ctx: &egui::Context) {
        let Some(session) = session(ctx) else {
            return;
        };
        let mut state = session.state.lock().unwrap();
        let prefs = AvatarPreferences::from_settings(&self.settings);
        if state.prefs != prefs {
            for active in state.active.values() {
                active.cancel.cancel();
            }
            state.generation = state.generation.wrapping_add(1);
            state.cache.clear();
            state.website = avatar_website(&prefs.site);
            state.prefs = prefs;
        }
        state.sources.clear();
        if state.prefs.provider != AvatarProvider::Repository {
            return;
        }
        for tab in self
            .tabs
            .iter()
            .chain(&self.retained_tabs)
            .filter(|t| !t.view.identity_blocked)
        {
            let Some(remotes) = &tab.remotes.value else {
                continue;
            };
            let remote = remotes
                .remotes
                .iter()
                .find(|r| r.name == "origin")
                .or_else(|| remotes.remotes.first());
            let Some(source) = remote
                .and_then(|r| avatar_provider::repository(&r.location, state.website.as_ref()))
            else {
                continue;
            };
            state.sources.insert(tab.path.clone(), source.clone());
            if let Some(snapshot) = tab.observed_snapshot() {
                state.sources.insert(snapshot.worktree.clone(), source);
            }
        }
    }
}

pub fn picture(ctx: &egui::Context, worktree: &Path, commit: &Commit) -> Option<TextureHandle> {
    session(ctx)?.picture(ctx, worktree, commit, |request, cancel| {
        request.load(cancel)
    })
}
impl Session {
    fn picture(
        &self,
        ctx: &egui::Context,
        worktree: &Path,
        commit: &Commit,
        load: impl FnOnce(&Request, &Cancellation) -> Option<ColorImage> + Send + 'static,
    ) -> Option<TextureHandle> {
        let mut state = self.state.lock().unwrap();
        let source = match state.prefs.provider {
            AvatarProvider::Repository => state.sources.get(worktree)?.clone(),
            AvatarProvider::Gravatar if !commit.email.trim().is_empty() => Source::Gravatar,
            _ => return None,
        };
        let email = commit.email.trim();
        let key = Key {
            source: source.clone(),
            author: if email.is_empty() {
                commit.hash.clone()
            } else {
                email.to_lowercase()
            },
        };
        if let Some(item) = state.cache.get(&key)
            && item.created.elapsed() < Duration::from_secs(3600)
        {
            return item.image.clone();
        }
        let id = (state.generation, key);
        let viewport = ctx.viewport_id();
        let seen = (
            ctx.cumulative_pass_nr(),
            ctx.cumulative_pass_nr_for(egui::ViewportId::ROOT),
        );
        if let Some(active) = state.active.get_mut(&id) {
            active.visible.insert(viewport, seen);
            return None;
        }
        if state.active.len() >= 2 || self.workers.stopping() {
            return None;
        }
        let request = Request {
            source,
            email: email.to_owned(),
            commit: commit.hash.clone(),
        };
        let cancel = Cancellation::default();
        state.active.insert(
            id.clone(),
            Active {
                cancel: cancel.clone(),
                visible: HashMap::from([(viewport, seen)]),
            },
        );
        let lease = self.workers.start(cancel.clone());
        let shared = self.state.clone();
        let ctx = ctx.clone();
        drop(state);
        std::thread::spawn(move || {
            let _lease = lease;
            let image = load(&request, &cancel);
            let mut state = shared.lock().unwrap();
            state.active.remove(&id);
            if !cancel.is_cancelled() && state.generation == id.0 {
                if state.cache.len() >= 128
                    && !state.cache.contains_key(&id.1)
                    && let Some(oldest) = state
                        .cache
                        .iter()
                        .min_by_key(|(_, v)| v.created)
                        .map(|(k, _)| k.clone())
                {
                    state.cache.remove(&oldest);
                }
                let image = image.map(|pixels| {
                    ctx.load_texture("author picture", pixels, egui::TextureOptions::LINEAR)
                });
                state.cache.insert(
                    id.1,
                    Cached {
                        created: Instant::now(),
                        image,
                    },
                );
            }
            drop(state);
            ctx.request_repaint();
        });
        None
    }
}

#[cfg(test)]
#[path = "avatar_tests.rs"]
mod tests;
