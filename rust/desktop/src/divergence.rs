//! Per-tab secondary reads never delay first history or follow Push destinations.
use crate::strings::{self, text};
use eframe::egui;
use shovel_core::{
    divergence::State,
    git::{Git, Snapshot},
    process::Cancellation,
};
use std::{
    collections::HashSet,
    sync::{
        Arc,
        mpsc::{self, Receiver, Sender},
    },
    thread,
};
#[derive(Clone)]
pub struct Display {
    pub ahead: Option<String>,
    pub behind: Option<String>,
    pub help: String,
}
impl Default for Display {
    fn default() -> Self {
        Self {
            ahead: None,
            behind: None,
            help: text("DivergencePending"),
        }
    }
}
type Comparison = (State, HashSet<String>);
struct Event(u64, Result<Comparison, String>);
pub struct Counter {
    state: State,
    generation: u64,
    stale: bool,
    owner: String,
    branch: Option<String>,
    target: Option<shovel_core::network::Target>,
    incoming_commits: HashSet<String>,
    busy: bool,
    upstream: String,
    fingerprint: String,
    observed: String,
    error: Option<String>,
    cancel: Cancellation,
    sender: Sender<Event>,
    receiver: Receiver<Event>,
}
impl Default for Counter {
    fn default() -> Self {
        let (sender, receiver) = mpsc::channel();
        Self {
            state: State::Pending,
            generation: 0,
            stale: false,
            owner: String::new(),
            branch: None,
            target: None,
            incoming_commits: HashSet::new(),
            busy: false,
            upstream: String::new(),
            fingerprint: String::new(),
            observed: String::new(),
            error: None,
            cancel: Default::default(),
            sender,
            receiver,
        }
    }
}
impl Drop for Counter {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}
impl Counter {
    pub fn mark_stale(&mut self) {
        self.stale = true;
    }
    pub fn invalidate(&mut self) {
        self.cancel.cancel();
        self.generation += 1;
        self.busy = false;
        self.fingerprint.clear();
        self.stale = true;
    }
    pub fn start(
        &mut self,
        git: Arc<Git>,
        snapshot: &Snapshot,
        ctx: &egui::Context,
        workers: &crate::read_workers::Workers,
    ) {
        if self.owner == snapshot.identity.fingerprint && self.fingerprint == snapshot.fingerprint
            && (self.busy || self.error.is_none())
        {
            self.stale = false;
            return;
        }
        self.invalidate();
        self.fingerprint.clone_from(&snapshot.fingerprint);
        let upstream = snapshot.upstream.as_deref().unwrap_or_default();
        if self.owner != snapshot.identity.fingerprint
            || self.branch != snapshot.branch
            || self.target != snapshot.upstream_target
            || self.upstream != upstream
        {
            self.state = State::Pending;
            self.incoming_commits.clear();
            self.owner.clone_from(&snapshot.identity.fingerprint);
            self.branch.clone_from(&snapshot.branch);
            self.target.clone_from(&snapshot.upstream_target);
            upstream.clone_into(&mut self.upstream);
            self.error = None;
        }
        self.busy = true;
        self.cancel = Cancellation::default();
        let cancel = self.cancel.clone();
        let snapshot = snapshot.clone();
        let sender = self.sender.clone();
        let generation = self.generation;
        let ctx = ctx.clone();
        let lease = workers.start(cancel.clone());
        thread::spawn(move || {
            let _lease = lease;
            #[cfg(test)]
            crate::history_cache::tests::wait_for_read(&snapshot.worktree);
            let result = git
                .divergence_with_incoming(&snapshot, &cancel)
                .map_err(crate::error_text);
            let _ = sender.send(Event(generation, result));
            ctx.request_repaint();
        });
    }
    pub fn poll(&mut self, ctx: &egui::Context) {
        while let Ok(Event(generation, result)) = self.receiver.try_recv() {
            if generation != self.generation {
                continue;
            }
            self.busy = false;
            match result {
                Ok((state, incoming)) => {
                    self.state = state;
                    self.incoming_commits = incoming;
                    self.observed = crate::dates::local(ctx, &chrono::Utc::now());
                    self.stale = false;
                    self.error = None;
                }
                Err(error) => {
                    if !matches!(self.state, State::Available { .. }) {
                        self.state = State::Failed;
                    }
                    self.stale = true;
                    self.error = Some(error);
                }
            }
        }
        if self.busy {
            ctx.request_repaint_after(std::time::Duration::from_millis(50));
        }
    }
    pub fn is_incoming(&self, hash: &str) -> bool {
        self.incoming_commits.contains(hash)
    }
    pub fn display(&self) -> Display {
        if let State::Available { ahead, behind } = self.state {
            let mut help = strings::format(
                if self.stale {
                    "DivergenceStale"
                } else {
                    "DivergenceAvailable"
                },
                &[
                    &ahead.to_string(),
                    &behind.to_string(),
                    &self.upstream,
                    &self.observed,
                ],
            );
            if let Some(error) = &self.error {
                help.push('\n');
                help.push_str(error);
            }
            return Display {
                help,
                ahead: (ahead > 0).then(|| ahead.to_string()),
                behind: (behind > 0).then(|| behind.to_string()),
            };
        }
        let key = match self.state {
            State::NoUpstream => "DivergenceNoUpstream",
            State::MissingUpstream => "DivergenceMissingUpstream",
            State::Detached => "DivergenceDetached",
            State::Unborn => "DivergenceUnborn",
            State::Shallow => "DivergenceShallow",
            State::Failed => "DivergenceFailed",
            _ => "DivergencePending",
        };
        let mut help = text(key);
        if let Some(error) = &self.error {
            help.push('\n');
            help.push_str(error);
        }
        Display {
            help,
            ..Default::default()
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn published_history_drives_real_pull_push_badges_without_using_push_destination() {
        use crate::workspace_interaction_tests::{frame, sidebar_click, sidebar_point, workspace};
        use std::{
            fs,
            path::Path,
            time::{Duration, Instant},
        };
        let temp = tempfile::tempdir().unwrap();
        let config = temp.path().join("isolated.config");
        fs::write(
            &config,
            "[user]\nname=Fixture\nemail=fixture@example.invalid\n[commit]\ngpgsign=false\n",
        )
        .unwrap();
        let mut git = Git::discover(None, &Cancellation::default()).unwrap();
        git.environment
            .insert("GIT_CONFIG_GLOBAL".into(), Some(config.into_os_string()));
        git.environment
            .insert("GIT_CONFIG_NOSYSTEM".into(), Some("1".into()));
        let root = temp.path().join("repository");
        fs::create_dir(&root).unwrap();
        let cmd = |root: &Path, args: &[&str]| {
            let result = shovel_core::process::run(
                &git.request(
                    root,
                    args.iter().copied(),
                    shovel_core::process::Lifetime::AcceptedCommand,
                    65536,
                ),
                &Cancellation::default(),
                None,
            )
            .unwrap();
            assert!(result.success(), "{result:?}");
            String::from_utf8(result.stdout)
                .unwrap()
                .trim_end()
                .to_owned()
        };
        cmd(&root, &["init", "--initial-branch=master"]);
        cmd(&root, &["commit", "--allow-empty", "-m", "base"]);
        let base = cmd(&root, &["rev-parse", "HEAD"]);
        cmd(&root, &["commit", "--allow-empty", "-m", "local"]);
        cmd(
            &root,
            &[
                "remote",
                "add",
                "team",
                "https://example.invalid/never-contact",
            ],
        );
        let tree = cmd(&root, &["rev-parse", "HEAD^{tree}"]);
        let incoming = cmd(
            &root,
            &["commit-tree", &tree, "-p", &base, "-m", "incoming"],
        );
        cmd(&root, &["update-ref", "refs/remotes/team/topic", &incoming]);
        cmd(
            &root,
            &[
                "symbolic-ref",
                "refs/remotes/team/HEAD",
                "refs/remotes/team/topic",
            ],
        );
        cmd(&root, &["config", "branch.master.remote", "team"]);
        cmd(
            &root,
            &["config", "branch.master.merge", "refs/heads/topic"],
        );
        cmd(&root, &["config", "branch.master.pushRemote", "different"]);
        let mut app = workspace();
        app.tabs.clear();
        app.settings
            .set("AutoFetchEnabled", serde_json::json!(false));
        app.git = Some(Arc::new(git.clone()));
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        app.open(root, &ctx);
        let start = Instant::now();
        let incoming_label = loop {
            app.receive(&ctx);
            if app.tabs[0].history.is_some() {
                app.tabs[0].selected = Some(base.clone());
            }
            let output = frame(&ctx, &mut app, vec![]);
            if app.tabs[0].divergence.state
                == (State::Available {
                    ahead: 1,
                    behind: 1,
                })
            {
                let labels = output
                    .platform_output
                    .accesskit_update
                    .as_ref()
                    .unwrap()
                    .nodes
                    .iter()
                    .filter_map(|(_, n)| n.label().or_else(|| n.value()).map(str::to_owned))
                    .collect::<Vec<_>>();
                let palette =
                    crate::workspace_style::Palette::for_mode(ctx.global_style().visuals.dark_mode);
                let has_text_color = |label, color| {
                    output.shapes.iter().any(|shape| {
                    matches!(&shape.shape, egui::Shape::Text(text)
                        if text.galley.text() == label
                            && text.galley.job.sections.iter().any(|section| section.format.color == color))
                })
                };
                assert!(has_text_color("local", palette.history_text));
                assert!(has_text_color("master", palette.diff_added_text));
                assert!(has_text_color("team/topic", palette.muted.lerp_to_gamma(palette.canvas, 0.16)));
                output.drop_without_applying_deltas();
                assert!(app.tabs[0].history.is_some());
                assert!(!app.tabs[0].loading);
                for label in ["Commits ahead of upstream: 1", "Pull", "Push"] {
                    assert!(
                        labels.iter().any(|n| n == label),
                        "missing {label}; labels={labels:?}"
                    );
                }
                assert!(labels.iter().any(|n| n == "Commits behind upstream: 1"));
                let snapshot = &app.tabs[0].history.as_ref().unwrap().snapshot;
                let row = |hash: &str| labels.iter().find(|label| label.contains(hash)).unwrap();
                let local_row = row(snapshot.head.as_deref().unwrap());
                assert!(local_row.contains("local") && !local_row.contains("Waiting to pull"));
                assert!(row(&incoming).contains("Waiting to pull"));
                assert!(!row(&base).contains("Waiting to pull"));
                assert!(
                    !labels
                        .iter()
                        .any(|n| n.contains("team/HEAD") || n == "HEAD")
                );
                assert!(!local_row.contains(" HEAD"));
                assert!(app.tabs[0].divergence.is_incoming(&incoming));
                assert!(!app.tabs[0].divergence.is_incoming(&base));
                break row(&incoming).clone();
            }
            output.drop_without_applying_deltas();
            assert!(start.elapsed() < Duration::from_secs(20));
            thread::sleep(Duration::from_millis(10));
        };
        sidebar_click(
            &ctx,
            &mut app,
            &incoming_label,
            egui::PointerButton::Secondary,
        );
        assert_eq!(app.history_menu.as_ref().unwrap().commit.hash, incoming);
        assert_eq!(app.tabs[0].selected.as_ref(), Some(&base));
        for key in [
            "RepositoryNewBranch",
            "RepositoryNewTag",
            "HistoryMergeIntoCurrent",
            "HistoryRebaseCurrent",
            "HistoryResetMenu",
            "HistoryMenuCheckoutCommit",
            "HistoryCherryPick",
            "HistoryMenuRevertCommit",
            "HistoryExportPatch",
            "HistoryCompareWorktree",
            "HistoryCopySha",
            "HistoryCopyInfo",
        ] {
            assert_eq!(
                sidebar_point(&ctx, &mut app, &text(key)).map(|(_, disabled)| disabled),
                Some(false),
                "missing or disabled: {key}"
            );
        }
        sidebar_click(
            &ctx,
            &mut app,
            &text("HistoryCompareWorktree"),
            egui::PointerButton::Primary,
        );
        assert_eq!(app.tabs[0].selected.as_ref(), Some(&incoming));
        assert_eq!(app.tabs[0].comparison.as_ref().unwrap().details_index, 1);
        assert!(
            app.tabs[0]
                .divergence
                .display()
                .help
                .contains("refs/remotes/team/topic")
        );
        let previous = app.tabs[0].history.as_ref().unwrap().snapshot.clone();
        cmd(
            &previous.worktree,
            &[
                "update-ref",
                "refs/remotes/team/topic",
                previous.head.as_deref().unwrap(),
            ],
        );
        let git = app.git.as_ref().unwrap().clone();
        let updated = git
            .inspect(&previous.worktree, &Cancellation::default())
            .unwrap();
        let (started, release) = crate::history_cache::tests::hold_read(updated.worktree.clone());
        let counter = &mut app.tabs[0].divergence;
        counter.start(git.clone(), &updated, &ctx, &app.read_workers);
        started.recv_timeout(Duration::from_secs(5)).unwrap();
        let generation = counter.generation;
        counter.mark_stale();
        counter.start(git, &updated, &ctx, &app.read_workers);
        assert_eq!(counter.generation, generation);
        assert_eq!(counter.display().ahead.as_deref(), Some("1"));
        assert!(counter.is_incoming(&incoming));
        release.send(()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        while counter.busy {
            counter.poll(&ctx);
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(5));
        }
        assert!(counter.display().ahead.is_none());
        assert!(!counter.is_incoming(&incoming));
    }
    #[test]
    fn stale_values_remain_labeled_and_obsolete_completions_cannot_replace_new_counts() {
        let ctx = egui::Context::default();
        let mut value = Counter::default();
        value
            .sender
            .send(Event(
                0,
                Ok((
                    State::Available {
                        ahead: 0,
                        behind: 2,
                    },
                    HashSet::new(),
                )),
            ))
            .unwrap();
        value.poll(&ctx);
        value.invalidate();
        assert!(value.display().ahead.is_none());
        assert!(value.display().help.starts_with("Stale:"));
        value
            .sender
            .send(Event(
                0,
                Ok((
                    State::Available {
                        ahead: 99,
                        behind: 99,
                    },
                    HashSet::from(["obsolete".into()]),
                )),
            ))
            .unwrap();
        value.poll(&ctx);
        assert_eq!(value.display().behind.as_deref(), Some("2"));
        assert!(!value.is_incoming("obsolete"));
        value
            .sender
            .send(Event(
                1,
                Ok((
                    State::Available {
                        ahead: 3,
                        behind: 4,
                    },
                    HashSet::from(["ahead".into()]),
                )),
            ))
            .unwrap();
        value.poll(&ctx);
        assert_eq!(value.display().ahead.as_deref(), Some("3"));
        assert!(value.is_incoming("ahead"));
        value
            .sender
            .send(Event(1, Err("fixture failure".into())))
            .unwrap();
        value.poll(&ctx);
        assert_eq!(value.display().ahead.as_deref(), Some("3"));
        assert_eq!(value.display().behind.as_deref(), Some("4"));
        assert!(value.is_incoming("ahead"));
        assert!(value.display().help.contains("fixture failure"));
        value
            .sender
            .send(Event(1, Ok((State::NoUpstream, HashSet::new()))))
            .unwrap();
        value.poll(&ctx);
        assert!(value.display().ahead.is_none());
        assert!(value.display().behind.is_none());
        assert!(!value.is_incoming("ahead"));
    }
}
