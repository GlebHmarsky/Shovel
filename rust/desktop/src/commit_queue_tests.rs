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
    modifiers: egui::Modifiers,
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
            modifiers: egui::Modifiers::NONE,
        };
        h.editor.initialize(&h.ctx);
        h.until(|e| e.draft_loaded && e.context.is_some() && !e.loading);
        h
    }
    fn render(&mut self, mut events: Vec<egui::Event>) -> egui::FullOutput {
        events.insert(0, egui::Event::ModifiersChanged(self.modifiers));
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
                self.editor.show(&ctx, true, |_| {});
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
                modifiers: self.modifiers,
            }])
            .drop_without_applying_deltas();
        }
    }
    fn stage(&mut self, name: &str) {
        self.click(&format!("Unstaged · Modified · {name}"));
        self.click(&text("CommitStage"));
    }
    fn submit_commit(&mut self, subject: &str) {
        self.editor.draft.subject = subject.into();
        self.editor.edited();
        assert!(self.editor.commit_reason(false).is_none());
        self.click(&text("Commit"));
        assert!(!self.editor.open, "accepted Commit closes immediately");
        assert!(self.editor.blocks_exit());
    }
    fn git(&self, args: &[&str]) -> String {
        command(&self.editor.git, &self.editor.path, args)
    }
    fn upstream(&mut self) -> PathBuf {
        let remote = self._temp.path().join("upstream.git");
        self.git(&["init", "--bare", remote.to_str().unwrap()]);
        self.git(&["remote", "add", "team", remote.to_str().unwrap()]);
        self.git(&["push", "-u", "team", "main:refs/heads/review/main"]);
        self.activity.set_authentication_helper(
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/debug/ShovelAskPass.exe"),
        );
        self.editor.authentication = Some(self.activity.external_authentication());
        self.editor.refresh(&self.ctx);
        self.until(|e| !e.loading && e.push_target().is_some());
        remote
    }
    fn commit_shortcut(&mut self, shift: bool, repeat: bool) {
        self.modifiers = egui::Modifiers {
            ctrl: true,
            command: true,
            shift,
            ..Default::default()
        };
        self.render(vec![egui::Event::Key {
            key: egui::Key::Enter,
            physical_key: Some(egui::Key::Enter),
            pressed: true,
            repeat,
            modifiers: self.modifiers,
        }])
        .drop_without_applying_deltas();
        self.render(vec![egui::Event::Key {
            key: egui::Key::Enter,
            physical_key: Some(egui::Key::Enter),
            pressed: false,
            repeat: false,
            modifiers: self.modifiers,
        }])
        .drop_without_applying_deltas();
        self.modifiers = egui::Modifiers::NONE;
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
            shovel_core::case::path_key(&context.repository.common_directory),
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
fn ctrl_shift_enter_commits_and_pushes_the_reviewed_upstream_after_the_window_closes() {
    let mut h = Harness::new();
    let remote = h.upstream();
    h.stage("one.txt");
    h.until(|e| !e.busy);
    let before = h.git(&["rev-parse", "HEAD"]);
    let hook = h.editor.path.join(".git/hooks/pre-push");
    fs::write(&hook, "#!/bin/sh\ntouch .git/push-entered\nwhile test ! -f .git/push-release; do sleep .05; done\ngit add -- two.txt\n").unwrap();
    let release = HookRelease(h.editor.path.join(".git/push-release"));
    h.modifiers = egui::Modifiers::CTRL | egui::Modifiers::SHIFT;
    // Keep Enter held from a press with an empty draft. egui derives repeats
    // from its key-down set, just like the real window integration.
    h.render(vec![egui::Event::Key {
        key: egui::Key::Enter, physical_key: Some(egui::Key::Enter), pressed: true,
        repeat: false, modifiers: h.modifiers,
    }]).drop_without_applying_deltas();
    h.editor.draft.subject = "keyboard commit and push".into();
    h.editor.edited();
    h.commit_shortcut(true, true);
    assert!(h.editor.open && !h.editor.busy, "ignore key repeat");
    h.commit_shortcut(true, false);
    assert!(!h.editor.open && h.editor.blocks_exit());
    h.until(|e| e.path.join(".git/push-entered").exists());
    let created = h.git(&["rev-parse", "HEAD"]);
    assert_ne!(created, before);
    assert_eq!(
        command(&h.editor.git, &remote, &["rev-parse", "review/main"]),
        before
    );
    assert_eq!(h.editor.status, Some(text("ActionStagePushing")));
    drop(release);
    h.until(|e| !e.busy && !e.draft_saving && !e.draft_dirty);
    assert_eq!(
        command(&h.editor.git, &remote, &["rev-parse", "review/main"]),
        created
    );
    assert_eq!(h.git(&["rev-list", "--count", "HEAD"]), "2");
    assert!(h.editor.draft.is_empty() && !h.editor.failed_commit);
    assert!(
        h.editor
            .context
            .as_ref()
            .unwrap()
            .change(Group::Staged, "two.txt")
            .is_some()
    );
    assert!(
        h.activity
            .summary(&h.editor.path)
            .unwrap()
            .contains(&strings::format("CommitCreatedPushed", &[&created]))
    );
}

#[test]
fn shift_click_preserves_new_draft_and_reports_a_non_fast_forward_separately() {
    let mut h = Harness::new();
    let remote = h.upstream();
    let external = h.git(&[
        "commit-tree",
        "HEAD^{tree}",
        "-p",
        "HEAD",
        "-m",
        "remote work",
    ]);
    h.git(&[
        "push",
        "team",
        &format!("{external}:refs/heads/review/main"),
    ]);
    h.stage("one.txt");
    h.until(|e| !e.busy);
    h.editor.draft.subject = "local work".into();
    h.editor.edited();
    let barrier = h.barrier();
    h.modifiers = egui::Modifiers::SHIFT;
    h.click(&text("CommitAndPush"));
    h.modifiers = egui::Modifiers::NONE;
    assert!(!h.editor.open && h.editor.blocks_exit());
    h.editor.draft.subject = "next draft".into();
    h.editor.edited();
    drop(barrier);
    h.until(|e| !e.busy && !e.draft_dirty && !e.draft_saving);
    let created = h.git(&["rev-parse", "HEAD"]);
    assert_eq!(h.git(&["rev-list", "--count", "HEAD"]), "2");
    assert_eq!(h.git(&["log", "-1", "--format=%s"]), "local work");
    assert_eq!(
        command(&h.editor.git, &remote, &["rev-parse", "review/main"]),
        external
    );
    assert_eq!(h.editor.draft.subject, "next draft");
    assert!(!h.editor.failed_commit);
    assert_eq!(h.activity.failure_path(), Some(h.editor.path.as_path()));
    assert!(h.editor.status.as_ref().unwrap().contains(&created));
    assert!(
        h.editor
            .status
            .as_ref()
            .unwrap()
            .contains("Use Push to retry")
    );
    assert!(h.editor.output.contains("[rejected]"));
}

#[test]
fn missing_upstream_disables_shift_only_and_ctrl_enter_still_commits() {
    let mut h = Harness::new();
    h.stage("one.txt");
    h.until(|e| !e.busy);
    h.editor.draft.subject = "plain keyboard commit".into();
    h.editor.edited();
    h.modifiers = egui::Modifiers::SHIFT;
    let frame = h.render(vec![]);
    let nodes = &frame
        .platform_output
        .accesskit_update
        .as_ref()
        .unwrap()
        .nodes;
    assert!(nodes.iter().any(
        |(_, node)| node.label() == Some(text("CommitAndPush").as_str()) && node.is_disabled()
    ));
    assert!(
        nodes
            .iter()
            .any(|(_, node)| node.value() == Some(text("CommitNoUpstream").as_str()))
    );
    frame.drop_without_applying_deltas();
    h.commit_shortcut(true, false);
    assert!(h.editor.open && !h.editor.busy);
    h.commit_shortcut(false, false);
    h.until(|e| !e.busy && !e.draft_dirty && !e.draft_saving);
    assert_eq!(
        h.git(&["log", "-1", "--format=%s"]),
        "plain keyboard commit"
    );
    assert!(h.editor.draft.is_empty());
}

#[test]
fn amend_confirmation_keeps_the_reviewed_head_and_destination_through_refresh() {
    let mut h = Harness::new();
    let remote = h.upstream();
    let original = h.git(&["rev-parse", "HEAD"]);
    h.editor.amend = true;
    h.editor.draft.subject = "reviewed replacement".into();
    h.editor.edited();
    h.commit_shortcut(true, false);
    assert!(h.editor.confirm_amend.is_some() && !h.editor.busy);
    h.git(&["commit", "--allow-empty", "-m", "external commit"]);
    let external = h.git(&["rev-parse", "HEAD"]);
    h.editor.refresh(&h.ctx);
    h.until(|e| {
        !e.loading && e.context.as_ref().unwrap().repository.head.as_ref() == Some(&external)
    });
    let frame = h.render(vec![]);
    let nodes = &frame
        .platform_output
        .accesskit_update
        .as_ref()
        .unwrap()
        .nodes;
    assert!(
        nodes
            .iter()
            .any(|(_, n)| n.value().is_some_and(|v| v.contains(&original)))
    );
    assert!(nodes.iter().any(|(_, n)| n.value()
        == Some(strings::format("CommitPushTarget", &["team", "review/main"]).as_str())));
    frame.drop_without_applying_deltas();
    h.click(&text("CommitAmend"));
    h.until(|e| !e.busy);
    assert_eq!(h.git(&["rev-parse", "HEAD"]), external);
    assert_eq!(
        command(&h.editor.git, &remote, &["rev-parse", "review/main"]),
        original
    );
    assert!(h.editor.failed_commit);
    assert_eq!(h.editor.draft.subject, "reviewed replacement");
}

#[test]
fn confirmed_amend_and_merge_from_the_editor_preserve_native_parents() {
    let mut h = Harness::new();
    let first = h.git(&["rev-parse", "HEAD"]);
    h.editor.amend = true;
    h.editor.draft.subject = "amended base".into();
    h.editor.edited();
    h.commit_shortcut(false, false);
    assert!(h.editor.confirm_amend.is_some());
    h.click(&text("CommitAmend"));
    h.until(|e| !e.busy && !e.draft_saving && !e.draft_dirty);
    let amended = h.git(&["rev-parse", "HEAD"]);
    assert_ne!(first, amended);
    assert!(h.git(&["log", "-1", "--format=%P"]).is_empty());
    assert_eq!(h.git(&["log", "-1", "--format=%s"]), "amended base");

    h.git(&["switch", "-c", "side"]);
    fs::write(h.editor.path.join("side"), "side\n").unwrap();
    h.git(&["add", "side"]);
    h.git(&["commit", "-m", "side"]);
    let side = h.git(&["rev-parse", "HEAD"]);
    h.git(&["switch", "main"]);
    h.git(&["merge", "--no-ff", "--no-commit", "side"]);
    h.editor.activate();
    h.editor.refresh(&h.ctx);
    h.until(|e| !e.loading && e.context.as_ref().unwrap().repository.operations == ["merge"]);
    h.editor.draft.subject = "merge side".into();
    h.editor.draft.description = "reviewed in Local Changes".into();
    h.editor.edited();
    h.commit_shortcut(false, false);
    h.until(|e| !e.busy && !e.draft_saving && !e.draft_dirty);
    assert_eq!(
        h.git(&["log", "-1", "--format=%P"]),
        format!("{amended} {side}")
    );
    assert_eq!(
        h.git(&["log", "-1", "--format=%B"]),
        "merge side\n\nreviewed in Local Changes"
    );
    assert!(
        h.editor
            .context
            .as_ref()
            .unwrap()
            .repository
            .operations
            .is_empty()
    );
    assert!(h.editor.draft.is_empty());
    assert_eq!(
        fs::read_to_string(h.editor.path.join("one.txt")).unwrap(),
        "one\n"
    );
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
            .contains(&strings::format(
                "CommitCreated",
                &[&h.git(&["rev-parse", "HEAD"])]
            ))
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

#[test]
fn failed_commit_can_retire_and_activity_reopens_its_durable_draft_and_real_index() {
    use crate::workspace_interaction_tests::{frame, workspace};
    let mut h = Harness::new();
    h.git(&["add", "one.txt"]);
    h.editor.refresh(&h.ctx);
    h.until(|e| !e.loading);
    h.editor.draft.subject = "retain failed subject".into();
    h.editor.draft.description = "retain failed body".into();
    h.editor.edited();
    fs::write(
        h.editor.path.join(".git/hooks/pre-commit"),
        "#!/bin/sh\necho retired-fixture-failure >&2\nexit 1\n",
    )
    .unwrap();
    let head = h.git(&["rev-parse", "HEAD"]);
    let index = h.git(&["ls-files", "--stage", "-z"]);
    let config = fs::read(h.editor.path.join(".git/config")).unwrap();
    h.editor.request_commit(false, &h.ctx);
    h.until(|e| !e.busy && !e.draft_dirty && !e.draft_saving);
    assert!(h.editor.failed_commit);
    assert!(h.editor.can_retire());
    let root = h.editor.path.clone();
    assert_eq!(h.activity.failure_path(), Some(root.as_path()));
    let ctx = h.ctx.clone();
    let git = h.editor.git.clone();
    let mut app = workspace();
    app.tabs.clear();
    app.selected = 0;
    app.git = Some(git.clone());
    app.draft_store = Some(h.editor.store.clone());
    app.settings
        .set("AutoFetchEnabled", serde_json::json!(false));
    app.operations = std::mem::take(&mut h.activity);
    app.editors.push(h.editor);
    app.open(root.clone(), &ctx);
    let start = Instant::now();
    while app.tabs[0].loading {
        app.receive(&ctx);
        assert!(start.elapsed() < Duration::from_secs(20));
        thread::sleep(Duration::from_millis(10));
    }
    let owner = app.tabs[0].id;
    app.close_tab(owner, &ctx);
    // Add three newer idle closed owners, without reading any other repository.
    app.git = None;
    for mut tab in workspace().tabs {
        tab.id += 100;
        tab.path = h._temp.path().join(format!("unused-cache-{}", tab.id));
        let id = tab.id;
        app.tabs.push(tab);
        app.close_tab(id, &ctx);
    }
    app.git = Some(git.clone());
    assert_eq!(app.retained_tabs.len(), 3);
    assert!(!app.retained_tabs.iter().any(|tab| tab.id == owner));
    assert!(
        app.editors.is_empty(),
        "saved failed editors must not grow without bound"
    );
    assert!(app.tabs.is_empty());
    app.activity_open = true;
    app.activity_all = true;
    let mut point = None;
    let mut saw_failure_log = false;
    for _ in 0..3 {
        let out = frame(&ctx, &mut app, vec![]);
        for (_, node) in &out.platform_output.accesskit_update.as_ref().unwrap().nodes {
            saw_failure_log |= node
                .label()
                .or_else(|| node.value())
                .is_some_and(|label| label.contains("retired-fixture-failure"));
            if node.label() == Some(text("ReopenCommit").as_str()) {
                assert!(!node.is_disabled());
                let r = node.bounds().unwrap();
                point = Some(egui::pos2(
                    (r.x0 + r.x1) as f32 / 2.,
                    (r.y0 + r.y1) as f32 / 2.,
                ));
            }
        }
        out.drop_without_applying_deltas();
    }
    assert!(saw_failure_log, "the failure log survives editor retirement");
    let pos = point.expect("completed Activity retains Reopen after editor retirement");
    frame(&ctx, &mut app, vec![egui::Event::PointerMoved(pos)]).drop_without_applying_deltas();
    for pressed in [true, false] {
        frame(
            &ctx,
            &mut app,
            vec![egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: Default::default(),
            }],
        )
        .drop_without_applying_deltas();
    }
    assert_eq!(app.tabs[app.selected].path, root);
    assert_eq!(app.editors.len(), 1);
    assert!(app.editors[0].open);
    let start = Instant::now();
    while !app.editors[0].draft_loaded || app.editors[0].loading {
        app.receive(&ctx);
        frame(&ctx, &mut app, vec![]).drop_without_applying_deltas();
        assert!(start.elapsed() < Duration::from_secs(20));
        thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(app.editors[0].draft.subject, "retain failed subject");
    assert_eq!(app.editors[0].draft.description, "retain failed body");
    assert_eq!(command(&git, &root, &["rev-parse", "HEAD"]), head);
    assert_eq!(command(&git, &root, &["ls-files", "--stage", "-z"]), index);
    assert_eq!(fs::read(root.join(".git/config")).unwrap(), config);
    assert_eq!(fs::read(root.join("one.txt")).unwrap(), b"one\n");
    assert_eq!(fs::read(root.join("two.txt")).unwrap(), b"two\n");
    assert!(
        app.editors[0]
            .context
            .as_ref()
            .unwrap()
            .entries
            .iter()
            .any(|entry| entry.group == Group::Staged && entry.path == "one.txt")
    );
}

#[test]
fn retirement_keeps_failed_or_pending_draft_writes_and_unfinished_initialization() {
    let mut h = Harness::new();
    h.editor.open = false;
    assert!(h.editor.can_retire());
    h.editor.draft_loaded = false;
    assert!(
        !h.editor.can_retire(),
        "an unresolved draft load retains its owner"
    );
    h.editor.draft_loaded = true;
    let blocked = h._temp.path().join("blocked-profile");
    fs::write(&blocked, "existing file").unwrap();
    h.editor.store.directory = blocked.clone();
    h.editor.draft.subject = "unsaved text must remain".into();
    h.editor.edited();
    assert!(!h.editor.can_retire());
    h.editor.flush(&h.ctx);
    assert!(!h.editor.can_retire());
    h.until(|e| !e.draft_saving && e.draft_error.is_some());
    assert!(
        !h.editor.can_retire(),
        "failed save/recovery must keep the only in-memory text"
    );
    assert_eq!(h.editor.draft.subject, "unsaved text must remain");
    assert_eq!(fs::read_to_string(blocked).unwrap(), "existing file");
    let mut app = crate::workspace_interaction_tests::workspace();
    app.tabs.clear();
    app.editors.push(h.editor);
    app.close(&h.ctx);
    assert!(
        !app.pending_exit && !app.closing && !app.exit_ready,
        "a failed draft write must leave the workspace usable for recovery"
    );
    assert!(app.notice.is_some());
}

#[test]
fn shutdown_waits_for_required_draft_restore_and_saves_newer_typing_before_exit() {
    let h = Harness::new();
    let identity = h.editor.identity.clone().unwrap();
    let old = drafts::Text {
        subject: "restored subject".into(),
        description: "saved body".into(),
    };
    h.editor
        .store
        .save(
            &identity,
            h.editor.revision.as_deref(),
            &old,
            &Cancellation::default(),
        )
        .unwrap();
    let head = h.git(&["rev-parse", "HEAD"]);
    let index = h.git(&["ls-files", "--stage"]);
    let mut app = crate::workspace_interaction_tests::workspace();
    app.tabs.clear();
    app.git = Some(h.editor.git.clone());
    let mut profile = crate::SettingsStore::new(h._temp.path().join("profile"));
    app.settings = profile.load().unwrap();
    app.store = Some(profile);
    let mut editor = Editor::new(
        h.editor.path.clone(),
        h.editor.git.clone(),
        app.scheduler.clone(),
        h.editor.store.clone(),
    );
    editor.read_workers = app.read_workers.clone();
    let (started, release) = crate::history_cache::tests::hold_read(editor.store.directory.clone());
    editor.initialize(&h.ctx);
    started.recv_timeout(Duration::from_secs(5)).unwrap();
    editor.draft.subject = "newer typing during restore".into();
    editor.draft.description = "Unicode \u{05e9}\u{05dc}\u{05d5}\u{05dd}".into();
    editor.edited();
    let expected = editor.draft.clone();
    app.editors.push(editor);
    app.close(&h.ctx);
    assert!(app.pending_exit && !app.exit_ready);
    assert!(!app.editors[0].draft_loaded);
    assert!(!app.read_workers.cancel_and_drained());
    release.send(()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while !app.exit_ready {
        app.receive(&h.ctx);
        assert!(Instant::now() < deadline, "{:?}", app.notice);
        thread::sleep(Duration::from_millis(3));
    }
    assert!(app.editors[0].draft_loaded);
    assert!(!app.editors[0].draft_dirty && !app.editors[0].draft_saving);
    assert!(app.editors[0].draft_error.is_none());
    let actual = h
        .editor
        .store
        .load(&identity, &Cancellation::default())
        .unwrap();
    assert_eq!(actual.text, expected);
    assert_eq!(h.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(h.git(&["ls-files", "--stage"]), index);
}
