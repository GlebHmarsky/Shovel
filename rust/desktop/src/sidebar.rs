//! Repository-owned secondary sidebar data. Menus consume only published values.
use crate::strings::text;
use eframe::egui;
use shovel_core::{
    git::{Git, Snapshot},
    process::Cancellation,
    sidebar::Sidebar,
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
    pub value: Option<Sidebar>,
    pub error: Option<String>,
    receiver: Option<Receiver<Result<Sidebar, String>>>,
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
        self.receiver = None;
    }
    pub fn start(&mut self, git: Arc<Git>, snapshot: &Snapshot, ctx: &egui::Context) {
        self.invalidate();
        self.cancel = Cancellation::default();
        self.error = None;
        let cancel = self.cancel.clone();
        let snapshot = crate::history_cache::metadata(snapshot);
        let (sender, receiver) = mpsc::channel();
        self.receiver = Some(receiver);
        let ctx = ctx.clone();
        thread::spawn(move || {
            let result = git.sidebar(&snapshot, &cancel).map_err(|e| text(&e.code()));
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
    pub fn pending(&self) -> bool {
        self.receiver.is_some()
    }
    pub fn bytes(&self) -> usize {
        self.value.as_ref().map_or(0, |v| {
            v.branches
                .iter()
                .map(|b| 128 + 2 * (b.reference.len() + b.tracking.len()))
                .sum::<usize>()
                + v.submodules
                    .iter()
                    .map(|m| 128 + 2 * (m.path.len() + m.hash.len()))
                    .sum::<usize>()
        })
    }
    pub fn tracking(&self, reference: &shovel_core::git::Reference) -> String {
        let Some(branch) = self
            .value
            .as_ref()
            .and_then(|v| v.branches.iter().find(|b| b.reference == reference.name))
        else {
            return if reference.upstream.is_some() {
                "…".into()
            } else {
                String::new()
            };
        };
        if branch.tracking == "gone" {
            return text("SidebarGone");
        }
        branch
            .tracking
            .split(',')
            .filter_map(|part| {
                let (key, value) = part.trim().split_once(' ')?;
                let count = value.parse::<u64>().ok()?;
                match key {
                    "ahead" => Some(format!("↑ {count}")),
                    "behind" => Some(format!("↓ {count}")),
                    _ => None,
                }
            })
            .collect::<Vec<_>>()
            .join(" ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{history_view::tests::Fixture, workspace_interaction_tests::frame};
    use std::time::{Duration, Instant};
    #[test]
    fn real_tracking_counts_gone_and_cached_failure_retention() {
        let f = Fixture::new();
        let tip = f.command(&["rev-parse", "HEAD"], &[]);
        let old = f.command(&["rev-parse", "HEAD~2"], &[]);
        f.command(&["remote", "add", "origin", "no-contact::fixture"], &[]);
        f.command(&["update-ref", "refs/remotes/origin/old", &old], &[]);
        f.command(&["update-ref", "refs/remotes/origin/current", &tip], &[]);
        f.command(&["branch", "behind", &old], &[]);
        f.command(&["branch", "gone"], &[]);
        f.command(&["branch", "equal"], &[]);
        for (name, upstream) in [
            ("main", "old"),
            ("behind", "current"),
            ("equal", "current"),
            ("gone", "missing"),
        ] {
            f.command(&["config", &format!("branch.{name}.remote"), "origin"], &[]);
            f.command(
                &[
                    "config",
                    &format!("branch.{name}.merge"),
                    &format!("refs/heads/{upstream}"),
                ],
                &[],
            );
        }
        let config = std::fs::read(f.root.join(".git/config")).unwrap();
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let mut app = f.app(&ctx);
        let end = Instant::now() + Duration::from_secs(10);
        while app.tabs[0].sidebar.pending() {
            app.receive(&ctx);
            assert!(Instant::now() < end);
            thread::sleep(Duration::from_millis(5));
        }
        let out = frame(&ctx, &mut app, vec![]);
        let labels = out
            .platform_output
            .accesskit_update
            .as_ref()
            .unwrap()
            .nodes
            .iter()
            .filter_map(|(_, n)| n.label().map(str::to_owned))
            .collect::<Vec<_>>();
        out.drop_without_applying_deltas();
        for label in [
            "main  ↑ 2".to_owned(),
            "behind  ↓ 2".to_owned(),
            "equal".to_owned(),
            format!("gone  {}", text("SidebarGone")),
        ] {
            assert!(labels.contains(&label), "Missing {label}: {labels:?}");
        }
        assert_eq!(std::fs::read(f.root.join(".git/config")).unwrap(), config);
        let snapshot = app.tabs[0].history.as_ref().unwrap().snapshot.clone();
        let cache = &mut app.tabs[0].sidebar;
        let mut missing = f.git.clone();
        missing.executable = f.root.join("missing-git.exe");
        cache.start(Arc::new(missing), &snapshot, &ctx);
        let end = Instant::now() + Duration::from_secs(5);
        while cache.pending() {
            cache.poll(&ctx);
            assert!(Instant::now() < end);
            thread::sleep(Duration::from_millis(5));
        }
        assert!(cache.error.is_some());
        assert_eq!(cache.value.as_ref().unwrap().branches.len(), 4);
        let (sender, receiver) = mpsc::channel();
        cache.receiver = Some(receiver);
        cache.invalidate();
        assert!(
            sender
                .send(Ok(Sidebar {
                    branches: vec![],
                    submodules: vec![]
                }))
                .is_err()
        );
        cache.poll(&ctx);
        assert_eq!(cache.value.as_ref().unwrap().branches.len(), 4);
    }
}
