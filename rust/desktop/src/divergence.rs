//! Per-tab secondary reads never delay first history or follow Push destinations.
use crate::strings::{self, text};
use eframe::egui;
use shovel_core::{
    divergence::State,
    git::{Git, Snapshot},
    process::Cancellation,
};
use std::{
    sync::{
        Arc,
        mpsc::{self, Receiver, Sender},
    },
    thread,
};
#[derive(Clone)]
pub struct Display {
    pub ahead: String,
    pub behind: String,
    pub help: String,
}
impl Default for Display {
    fn default() -> Self {
        Self {
            ahead: text("CountUnavailable"),
            behind: text("CountUnavailable"),
            help: text("DivergencePending"),
        }
    }
}
struct Event(u64, Result<State, String>);
pub struct Counter {
    state: State,
    generation: u64,
    stale: bool,
    busy: bool,
    upstream: String,
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
            busy: false,
            upstream: String::new(),
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
    pub fn invalidate(&mut self) {
        self.cancel.cancel();
        self.generation += 1;
        self.busy = false;
        self.stale = true;
    }
    pub fn start(&mut self, git: Arc<Git>, snapshot: &Snapshot, ctx: &egui::Context) {
        self.invalidate();
        self.state = State::Pending;
        self.error = None;
        self.stale = false;
        self.busy = true;
        self.upstream = snapshot.upstream.clone().unwrap_or_default();
        self.observed = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        self.cancel = Cancellation::default();
        let cancel = self.cancel.clone();
        let snapshot = snapshot.clone();
        let sender = self.sender.clone();
        let generation = self.generation;
        let ctx = ctx.clone();
        thread::spawn(move || {
            let result = git
                .divergence(&snapshot, &cancel)
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
                Ok(state) => self.state = state,
                Err(error) => {
                    self.state = State::Failed;
                    self.error = Some(error);
                }
            }
        }
        if self.busy {
            ctx.request_repaint_after(std::time::Duration::from_millis(50));
        }
    }
    pub fn display(&self) -> Display {
        if let State::Available { ahead, behind } = self.state {
            let ahead = ahead.to_string();
            let behind = behind.to_string();
            return Display {
                help: strings::format(
                    if self.stale {
                        "DivergenceStale"
                    } else {
                        "DivergenceAvailable"
                    },
                    &[&ahead, &behind, &self.upstream, &self.observed],
                ),
                ahead,
                behind,
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
        use crate::workspace_interaction_tests::{frame, workspace};
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
        cmd(&root, &["init", "--initial-branch=main"]);
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
        cmd(&root, &["update-ref", "refs/remotes/team/topic", &base]);
        cmd(&root, &["config", "branch.main.remote", "team"]);
        cmd(&root, &["config", "branch.main.merge", "refs/heads/topic"]);
        cmd(&root, &["config", "branch.main.pushRemote", "different"]);
        let mut app = workspace();
        app.tabs.clear();
        app.settings
            .set("AutoFetchEnabled", serde_json::json!(false));
        app.git = Some(Arc::new(git));
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        app.open(root, &ctx);
        let start = Instant::now();
        loop {
            app.receive(&ctx);
            let output = frame(&ctx, &mut app, vec![]);
            if app.tabs[0].divergence.state
                == (State::Available {
                    ahead: 1,
                    behind: 0,
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
                output.drop_without_applying_deltas();
                assert!(app.tabs[0].history.is_some());
                assert!(!app.tabs[0].loading);
                for label in [
                    "Commits ahead of upstream: 1",
                    "Commits behind upstream: 0",
                    "Pull",
                    "Push",
                ] {
                    assert!(
                        labels.iter().any(|n| n == label),
                        "missing {label}; labels={labels:?}"
                    );
                }
                break;
            }
            output.drop_without_applying_deltas();
            assert!(start.elapsed() < Duration::from_secs(20));
            thread::sleep(Duration::from_millis(10));
        }
        assert!(
            app.tabs[0]
                .divergence
                .display()
                .help
                .contains("refs/remotes/team/topic")
        );
    }
    #[test]
    fn stale_values_remain_labeled_and_obsolete_completions_cannot_replace_new_counts() {
        let ctx = egui::Context::default();
        let mut value = Counter::default();
        value
            .sender
            .send(Event(
                0,
                Ok(State::Available {
                    ahead: 0,
                    behind: 2,
                }),
            ))
            .unwrap();
        value.poll(&ctx);
        value.invalidate();
        assert_eq!(value.display().ahead, "0");
        assert!(value.display().help.starts_with("Stale:"));
        value
            .sender
            .send(Event(
                0,
                Ok(State::Available {
                    ahead: 99,
                    behind: 99,
                }),
            ))
            .unwrap();
        value.poll(&ctx);
        assert_eq!(value.display().behind, "2");
        value
            .sender
            .send(Event(
                1,
                Ok(State::Available {
                    ahead: 3,
                    behind: 4,
                }),
            ))
            .unwrap();
        value.poll(&ctx);
        assert_eq!(value.display().ahead, "3");
        value
            .sender
            .send(Event(1, Err("fixture failure".into())))
            .unwrap();
        value.poll(&ctx);
        assert_eq!(value.display().ahead, text("CountUnavailable"));
        assert!(value.display().help.contains("fixture failure"));
    }
}
