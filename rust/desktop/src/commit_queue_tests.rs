use super::*;
use shovel_core::process::{self, Lifetime};
use std::{fs, path::Path};

fn command(git: &Git, root: &Path, args: &[&str]) -> String {
    let result = process::run(
        &git.request(root, args.iter().copied(), Lifetime::AcceptedCommand, 65536),
        &Cancellation::default(),
        None,
    )
    .unwrap();
    assert!(result.success(), "{result:?}");
    String::from_utf8(result.stdout)
        .unwrap()
        .trim_end()
        .to_owned()
}
struct Harness {
    _temp: tempfile::TempDir,
    ctx: egui::Context,
    editor: Editor,
    activity: crate::operations::Controller,
    show_activity: bool,
}
impl Harness {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("repo");
        fs::create_dir(&root).unwrap();
        let config = temp.path().join("isolated.config");
        fs::write(&config, "[user]\nname=Fixture\nemail=fixture@example.invalid\n[commit]\ngpgsign=false\n[core]\nautocrlf=false\n").unwrap();
        let mut git = Git::discover(None, &Cancellation::default()).unwrap();
        git.environment
            .insert("GIT_CONFIG_GLOBAL".into(), Some(config.into_os_string()));
        git.environment
            .insert("GIT_CONFIG_NOSYSTEM".into(), Some("1".into()));
        for name in ["GIT_AUTHOR_DATE", "GIT_COMMITTER_DATE"] {
            git.environment
                .insert(name.into(), Some("2026-01-01T00:00:00+00:00".into()));
        }
        command(&git, &root, &["init", "--initial-branch=main"]);
        for name in ["one.txt", "two.txt", "other.txt"] {
            fs::write(root.join(name), "base\n").unwrap();
        }
        command(&git, &root, &["add", "."]);
        command(&git, &root, &["commit", "-m", "base"]);
        for (name, value) in [("one.txt", "one\n"), ("two.txt", "two\n")] {
            fs::write(root.join(name), value).unwrap();
        }
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        ctx.set_embed_viewports(true);
        let editor = Editor::new(
            root,
            Arc::new(git),
            Scheduler::default(),
            drafts::Store {
                directory: temp.path().join("drafts"),
                legacy_directory: None,
            },
        );
        let mut h = Self {
            _temp: temp,
            ctx,
            editor,
            activity: Default::default(),
            show_activity: false,
        };
        h.editor.initialize(&h.ctx);
        h.until(|e| e.draft_loaded && e.context.is_some() && !e.loading);
        h
    }
    fn render(&mut self, events: Vec<egui::Event>) -> egui::FullOutput {
        self.editor.poll(&self.ctx);
        for activity in self.editor.take_activity() {
            self.activity
                .record_owned(self.editor.path.clone(), activity);
        }
        let ctx = self.ctx.clone();
        ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1400., 1000.),
                )),
                events,
                ..Default::default()
            },
            |ui| {
                if self.show_activity {
                    egui::CentralPanel::default().show(ui, |ui| {
                        self.activity.status(ui, None, &self.editor.scheduler)
                    });
                }
                self.editor.show(&ctx, true);
            },
        )
    }
    fn until(&mut self, ready: impl Fn(&Editor) -> bool) {
        let started = Instant::now();
        loop {
            self.render(vec![]).drop_without_applying_deltas();
            if ready(&self.editor) {
                return;
            }
            assert!(
                started.elapsed() < Duration::from_secs(25),
                "{:?}",
                self.editor.status
            );
            thread::sleep(Duration::from_millis(15));
        }
    }
    fn point(&mut self, label: &str) -> egui::Pos2 {
        let out = self.render(vec![]);
        let nodes = &out.platform_output.accesskit_update.as_ref().unwrap().nodes;
        let node = &nodes
            .iter()
            .find(|(_, n)| {
                n.label() == Some(label) && n.role() == egui::Role::Button && n.bounds().is_some()
            })
            .unwrap_or_else(|| {
                panic!(
                    "missing {label}: {:?}",
                    nodes
                        .iter()
                        .map(|(_, n)| (n.label(), n.role()))
                        .collect::<Vec<_>>()
                )
            })
            .1;
        assert!(
            !node.is_disabled(),
            "{label} must stay available while another request runs"
        );
        let r = node.bounds().unwrap();
        let p = egui::pos2((r.x0 + r.x1) as f32 / 2., (r.y0 + r.y1) as f32 / 2.);
        out.drop_without_applying_deltas();
        p
    }
    fn click(&mut self, label: &str) {
        let p = self.point(label);
        self.render(vec![egui::Event::PointerMoved(p)])
            .drop_without_applying_deltas();
        for pressed in [true, false] {
            self.render(vec![egui::Event::PointerButton {
                pos: p,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: Default::default(),
            }])
            .drop_without_applying_deltas();
        }
    }
    fn stage(&mut self, name: &str) {
        self.click(&format!("M  {name}"));
        self.click(&text("CommitStage"));
    }
    fn submit_commit(&mut self, subject: &str) {
        self.editor.draft.subject = subject.into();
        self.editor.edited();
        assert!(self.editor.commit_reason().is_none());
        self.click(&text("Commit"));
        assert!(!self.editor.open, "accepted Commit closes immediately");
        assert!(self.editor.blocks_exit());
    }
    fn git(&self, args: &[&str]) -> String {
        command(&self.editor.git, &self.editor.path, args)
    }
    fn export(&self, scenario: &str) {
        let Some(directory) = std::env::var_os("SHOVEL_QUEUE_PARITY_OUTPUT") else {
            return;
        };
        let directory = PathBuf::from(directory);
        assert!(directory.is_absolute());
        fs::create_dir_all(&directory).unwrap();
        let hex = |bytes: Vec<u8>| bytes.iter().map(|b| format!("{b:02X}")).collect::<String>();
        let files = ["one.txt", "two.txt", "other.txt"]
            .into_iter()
            .map(|name| (name, hex(fs::read(self.editor.path.join(name)).unwrap())))
            .collect::<std::collections::BTreeMap<_, _>>();
        let state = serde_json::json!({
            "head": self.git(&["rev-parse", "HEAD"]),
            "index": self.git(&["ls-files", "--stage", "-z"]),
            "status": self.git(&["status", "--porcelain", "-z"]),
            "worktree": files,
            "config": hex(fs::read(self.editor.path.join(".git/config")).unwrap()),
        });
        fs::write(
            directory.join(format!("{scenario}.json")),
            serde_json::to_vec_pretty(&state).unwrap(),
        )
        .unwrap();
    }
    fn barrier(&self) -> std::sync::mpsc::Sender<()> {
        let context = self.editor.context.as_ref().unwrap();
        let (tx, rx) = mpsc::channel();
        self.editor.scheduler.submit(
            context.repository.identity.fingerprint.clone(),
            context
                .repository
                .common_directory
                .to_string_lossy()
                .to_uppercase(),
            "fixture-barrier".into(),
            Lane::WorkingCopy,
            move || {
                let _ = rx.recv();
                Outcome::Succeeded
            },
            |_| {},
        );
        tx
    }
}
struct HookRelease(PathBuf);
impl Drop for HookRelease {
    fn drop(&mut self) {
        let _ = fs::write(&self.0, "");
    }
}
fn hold_index(h: &Harness) -> HookRelease {
    fs::write(h.editor.path.join(".git/hooks/post-index-change"),
        "#!/bin/sh\nif test ! -f .git/queue-entered; then\n touch .git/queue-entered\n while test ! -f .git/queue-release; do sleep .05; done\nfi\n").unwrap();
    HookRelease(h.editor.path.join(".git/queue-release"))
}

#[test]
fn distinct_stage_requests_and_commit_use_owned_index_after_view_closes() {
    let mut h = Harness::new();
    let before = h.editor.context.as_ref().unwrap().repository.head.clone();
    let config = fs::read(h.editor.path.join(".git/config")).unwrap();
    let release = hold_index(&h);
    h.stage("one.txt");
    h.until(|e| e.path.join(".git/queue-entered").exists());
    h.stage("two.txt");
    // The same immutable request coalesces even while the first Stage is running.
    h.editor.stage(false, false, &h.ctx);
    assert_eq!(
        h.editor
            .scheduler
            .records()
            .iter()
            .filter(|r| r.outcome.is_none())
            .count(),
        2
    );
    h.submit_commit("queued commit");
    assert_eq!(
        h.editor
            .scheduler
            .records()
            .iter()
            .filter(|r| r.outcome.is_none())
            .count(),
        3
    );
    assert_eq!(h.git(&["rev-parse", "HEAD"]), before.unwrap());
    drop(release);
    h.until(|e| !e.busy && !e.draft_dirty && !e.draft_saving);
    assert_eq!(h.git(&["log", "-1", "--format=%s"]), "queued commit");
    assert_eq!(h.git(&["show", "HEAD:one.txt"]), "one");
    assert_eq!(h.git(&["show", "HEAD:two.txt"]), "two");
    assert_eq!(h.git(&["show", "HEAD:other.txt"]), "base");
    assert!(h.git(&["status", "--porcelain"]).is_empty());
    assert_eq!(fs::read(h.editor.path.join(".git/config")).unwrap(), config);
    assert!(h.editor.draft.is_empty());
    assert_eq!(
        h.editor
            .scheduler
            .records()
            .iter()
            .filter(|r| r.outcome == Some(Outcome::Succeeded))
            .count(),
        3
    );
    assert!(
        h.activity
            .summary(&h.editor.path)
            .unwrap()
            .contains(&text("ActionCompletedShort"))
    );
    h.export("stage-commit");
}

#[test]
fn activity_removes_only_waiting_commit_and_preserves_accepted_stage_and_draft() {
    let mut h = Harness::new();
    let head = h.git(&["rev-parse", "HEAD"]);
    let release = hold_index(&h);
    h.stage("one.txt");
    h.until(|e| e.path.join(".git/queue-entered").exists());
    h.submit_commit("preserve waiting draft");
    h.show_activity = true;
    h.click(&text("CancelWaitingRequest"));
    h.until(|e| {
        e.scheduler
            .records()
            .iter()
            .any(|r| r.outcome == Some(Outcome::Removed))
    });
    assert!(h.editor.busy);
    assert!(h.editor.scheduler.has_accepted_work());
    drop(release);
    h.until(|e| !e.busy && !e.draft_dirty && !e.draft_saving);
    assert_eq!(h.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(h.git(&["show", ":one.txt"]), "one");
    assert_eq!(h.git(&["show", ":two.txt"]), "base");
    assert_eq!(h.editor.draft.subject, "preserve waiting draft");
    let saved = h
        .editor
        .store
        .load(
            &identity::read(&h.editor.path).unwrap(),
            &Cancellation::default(),
        )
        .unwrap();
    assert_eq!(saved.text.subject, "preserve waiting draft");
    assert_eq!(
        h.editor
            .scheduler
            .records()
            .iter()
            .filter(|r| r.outcome == Some(Outcome::Succeeded))
            .count(),
        1
    );
    h.export("removed-commit");
}

#[test]
fn external_index_change_rejects_commit_and_stops_only_own_waiting_work() {
    let mut h = Harness::new();
    h.git(&["add", "one.txt"]);
    h.editor.refresh(&h.ctx);
    h.until(|e| !e.loading);
    let head = h.git(&["rev-parse", "HEAD"]);
    let gate = h.barrier();
    h.submit_commit("must remain");
    // Queue another immutable request from this same retained owner.
    let entry = h
        .editor
        .context
        .as_ref()
        .unwrap()
        .entries
        .iter()
        .find(|e| e.path == "two.txt")
        .unwrap()
        .clone();
    h.editor.submit(Action::Stage(vec![entry]), &h.ctx);
    h.git(&["add", "two.txt"]);
    let index = h.git(&["ls-files", "--stage", "-z"]);
    drop(gate);
    h.until(|e| !e.busy && !e.draft_dirty && !e.draft_saving);
    assert!(h.editor.failed_commit);
    assert_eq!(h.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(h.git(&["ls-files", "--stage", "-z"]), index);
    assert_eq!(h.editor.draft.subject, "must remain");
    let records = h.editor.scheduler.records();
    assert_eq!(
        records
            .iter()
            .filter(|r| r.outcome == Some(Outcome::Failed))
            .count(),
        1
    );
    assert_eq!(
        records
            .iter()
            .filter(|r| r.outcome == Some(Outcome::StoppedAfterFailure))
            .count(),
        1
    );
    h.export("external-index");
}

#[test]
fn distinct_unstage_requests_adopt_only_prior_owned_index() {
    let mut h = Harness::new();
    h.git(&["add", "one.txt", "two.txt"]);
    h.editor.refresh(&h.ctx);
    h.until(|e| !e.loading);
    let context = h.editor.context.clone().unwrap();
    let head = h.git(&["rev-parse", "HEAD"]);
    let gate = h.barrier();
    for name in ["one.txt", "two.txt"] {
        let entry = context
            .entries
            .iter()
            .find(|e| e.group == Group::Staged && e.path == name)
            .unwrap()
            .clone();
        h.editor.submit(Action::Unstage(vec![entry]), &h.ctx);
    }
    assert_eq!(
        h.editor
            .scheduler
            .records()
            .iter()
            .filter(|r| r.outcome.is_none())
            .count(),
        3
    );
    h.editor.open = false;
    drop(gate);
    h.until(|e| !e.busy);
    assert_eq!(h.git(&["rev-parse", "HEAD"]), head);
    assert!(h.git(&["diff", "--cached", "--name-only"]).is_empty());
    assert_eq!(
        fs::read_to_string(h.editor.path.join("one.txt")).unwrap(),
        "one\n"
    );
    assert_eq!(
        fs::read_to_string(h.editor.path.join("two.txt")).unwrap(),
        "two\n"
    );
    assert_eq!(
        h.editor
            .scheduler
            .records()
            .iter()
            .filter(|r| r.outcome == Some(Outcome::Succeeded))
            .count(),
        3
    );
    h.export("unstage");
}
