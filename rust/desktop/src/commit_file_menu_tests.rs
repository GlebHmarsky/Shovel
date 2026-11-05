use super::*;
use shovel_core::process::{self, Lifetime};
use std::fs;
struct Harness {
    _temp: tempfile::TempDir,
    ctx: egui::Context,
    editor: Editor,
    copied: Vec<String>,
}
impl Harness {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("repo");
        fs::create_dir(&root).unwrap();
        let config = temp.path().join("isolated.config");
        fs::write(&config,"[user]\nname=Fixture\nemail=fixture@example.invalid\n[commit]\ngpgsign=false\n[core]\nautocrlf=false\n").unwrap();
        let mut git = Git::discover(None, &Cancellation::default()).unwrap();
        git.environment
            .insert("GIT_CONFIG_GLOBAL".into(), Some(config.into_os_string()));
        git.environment
            .insert("GIT_CONFIG_NOSYSTEM".into(), Some("1".into()));
        let cmd = |args: &[&str]| {
            let r = process::run(
                &git.request(
                    &root,
                    args.iter().copied(),
                    Lifetime::AcceptedCommand,
                    65536,
                ),
                &Cancellation::default(),
                None,
            )
            .unwrap();
            assert!(r.success(), "{r:?}");
        };
        cmd(&["init", "--initial-branch=main"]);
        fs::write(root.join("file.txt"), "base\n").unwrap();
        fs::write(root.join("other.txt"), "keep\n").unwrap();
        cmd(&["add", "file.txt", "other.txt"]);
        cmd(&["commit", "-m", "base"]);
        fs::write(root.join("file.txt"), "staged\n").unwrap();
        cmd(&["add", "file.txt"]);
        fs::write(root.join("file.txt"), "worktree\n").unwrap();
        fs::write(root.join("other.txt"), "keep changed\n").unwrap();
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
            copied: vec![],
        };
        h.until(|e| e.draft_loaded && e.context.is_some() && !e.loading);
        h
    }
    fn render(&mut self, mut events: Vec<egui::Event>) -> egui::FullOutput {
        let ctx = self.ctx.clone();
        self.editor.poll(&ctx);
        let modifiers = events
            .iter()
            .rev()
            .find_map(|event| match event {
                egui::Event::Key { modifiers, .. }
                | egui::Event::PointerButton { modifiers, .. } => Some(*modifiers),
                _ => None,
            })
            .unwrap_or_default();
        events.insert(0, egui::Event::ModifiersChanged(modifiers));
        let mut out = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1400., 1000.),
                )),
                events,
                ..Default::default()
            },
            |_| self.editor.show(&ctx, true),
        );
        out.textures_delta.clear();
        for command in &out.platform_output.commands {
            if let egui::OutputCommand::CopyText(text) = command {
                self.copied.push(text.clone());
            }
        }
        out
    }
    fn until(&mut self, ready: impl Fn(&Editor) -> bool) {
        let start = Instant::now();
        loop {
            self.render(vec![]).drop_without_applying_deltas();
            if ready(&self.editor) {
                return;
            }
            assert!(
                start.elapsed() < Duration::from_secs(25),
                "{:?}",
                self.editor.status
            );
            thread::sleep(Duration::from_millis(15));
        }
    }
    fn point(&mut self, label: &str) -> egui::Pos2 {
        let out = self.render(vec![]);
        let nodes = &out.platform_output.accesskit_update.as_ref().unwrap().nodes;
        let n = nodes
            .iter()
            .filter(|(_, n)| {
                n.role() == egui::Role::Button && n.label() == Some(label) && n.bounds().is_some()
            })
            .min_by(|(_, a), (_, b)| a.bounds().unwrap().y0.total_cmp(&b.bounds().unwrap().y0))
            .unwrap_or_else(|| {
                panic!(
                    "missing {label}: {:?}",
                    nodes
                        .iter()
                        .map(|(_, n)| (n.role(), n.label(), n.value()))
                        .collect::<Vec<_>>()
                )
            });
        let r = n.1.bounds().unwrap();
        let point = egui::pos2((r.x0 + r.x1) as f32 / 2., (r.y0 + r.y1) as f32 / 2.);
        out.drop_without_applying_deltas();
        point
    }
    fn click_at(&mut self, point: egui::Pos2, button: egui::PointerButton) {
        self.render(vec![egui::Event::PointerMoved(point)])
            .drop_without_applying_deltas();
        for pressed in [true, false] {
            self.render(vec![egui::Event::PointerButton {
                pos: point,
                button,
                pressed,
                modifiers: Default::default(),
            }])
            .drop_without_applying_deltas();
        }
    }
    fn click(&mut self, label: &str) {
        let point = self.point(label);
        self.click_at(point, egui::PointerButton::Primary);
    }
    fn menu(&mut self) {
        // Both lists contain the partially staged path; the first matching row
        // is the unstaged list, matching its actual displayed order.
        let point = self.point("M  file.txt");
        self.click_at(point, egui::PointerButton::Secondary);
        assert!(self.editor.file_menu.is_some());
        self.render(vec![]).drop_without_applying_deltas();
    }
    fn key(&mut self, key: egui::Key, modifiers: egui::Modifiers) {
        for pressed in [true, false] {
            self.render(vec![egui::Event::Key {
                key,
                physical_key: None,
                pressed,
                repeat: false,
                modifiers,
            }])
            .drop_without_applying_deltas();
        }
    }
}

#[test]
fn file_menu_reads_nothing_and_confirmation_freezes_actual_targets_before_discard() {
    let mut h = Harness::new();
    let original = h.editor.git.clone();
    let mut missing = (*original).clone();
    missing.executable = h._temp.path().join("missing-git.exe");
    h.editor.git = Arc::new(missing);
    h.menu();
    assert!(!h.editor.loading && !h.editor.busy && h.editor.diff_deferred);
    assert!(!h.editor.scheduler.has_accepted_work());
    h.editor.git = original;
    h.click(&text("FileMenuCopy"));
    assert_eq!(h.copied, vec!["file.txt"]);
    h.menu();
    h.click(&text("FileDiscardMenu"));
    assert!(h.editor.file_confirmation.is_some());
    let out = h.render(vec![]);
    let nodes = &out.platform_output.accesskit_update.as_ref().unwrap().nodes;
    assert!(
        nodes.iter().any(|(_, n)| n.value()
            == Some(strings::format("FileDiscardWorktreeTarget", &["file.txt"]).as_str())),
        "{:?}",
        nodes
            .iter()
            .filter_map(|(_, n)| n.value())
            .collect::<Vec<_>>()
    );
    assert!(nodes.iter().any(|(_, n)| {
        n.value().is_some_and(|v| {
            v.contains(
                h.editor
                    .context
                    .as_ref()
                    .unwrap()
                    .repository
                    .worktree
                    .to_string_lossy()
                    .as_ref(),
            )
        })
    }));
    out.drop_without_applying_deltas();
    assert_eq!(
        fs::read(h.editor.path.join("file.txt")).unwrap(),
        b"worktree\n"
    );
    h.click(&text("ActionCancel"));
    assert!(!h.editor.busy);
    h.menu();
    h.click(&text("FileDiscardMenu"));
    h.click(&text("FileDiscardButton"));
    h.until(|e| !e.busy);
    assert_eq!(h.editor.status, Some(text("CommitSucceededDiscard")));
    assert_eq!(
        fs::read(h.editor.path.join("file.txt")).unwrap(),
        b"staged\n"
    );
    assert_eq!(
        fs::read(h.editor.path.join("other.txt")).unwrap(),
        b"keep changed\n"
    );
}

#[test]
fn stale_confirmation_disables_discard_and_delete_does_not_allow_commit_shortcut() {
    let mut h = Harness::new();
    h.click("M  file.txt");
    h.editor.draft.subject = "must not commit".into();
    h.key(egui::Key::Delete, egui::Modifiers::NONE);
    assert!(h.editor.file_confirmation.is_some());
    let head = h.editor.context.as_ref().unwrap().repository.head.clone();
    h.key(
        egui::Key::Enter,
        egui::Modifiers {
            ctrl: true,
            command: true,
            ..Default::default()
        },
    );
    assert!(!h.editor.busy && h.editor.open);
    assert_eq!(h.editor.context.as_ref().unwrap().repository.head, head);
    let updated = h
        .editor
        .git
        .working_copy(&h.editor.path, &Cancellation::default())
        .unwrap();
    h.editor.apply(updated, &h.ctx);
    let out = h.render(vec![]);
    let nodes = &out.platform_output.accesskit_update.as_ref().unwrap().nodes;
    assert!(nodes.iter().any(|(_, n)| n.role() == egui::Role::Button
        && n.label() == Some(text("FileDiscardButton").as_str())
        && n.is_disabled()));
    out.drop_without_applying_deltas();
    h.click(&text("ActionCancel"));
    assert_eq!(
        fs::read(h.editor.path.join("file.txt")).unwrap(),
        b"worktree\n"
    );
}

#[test]
fn accepted_discard_drains_native_hook_and_reconciles_after_view_closes() {
    let mut h = Harness::new();
    fs::write(
        h.editor.path.join(".git/hooks/post-index-change"),
        "#!/bin/sh\ntouch hook-started\nwhile test ! -f hook-release; do sleep .05; done\n",
    )
    .unwrap();
    h.menu();
    h.click(&text("FileDiscardMenu"));
    h.click(&text("FileDiscardButton"));
    h.until(|e| e.path.join("hook-started").exists());
    h.editor.request_leave(Leave::Close, &h.ctx);
    assert!(!h.editor.open && h.editor.blocks_exit() && h.editor.scheduler.has_accepted_work());
    fs::write(h.editor.path.join("hook-release"), "").unwrap();
    h.until(|e| !e.busy);
    assert_eq!(h.editor.status, Some(text("CommitSucceededDiscard")));
    assert_eq!(
        fs::read(h.editor.path.join("file.txt")).unwrap(),
        b"staged\n"
    );
    assert!(
        h.editor
            .context
            .as_ref()
            .unwrap()
            .entries
            .iter()
            .any(|e| e.path == "file.txt" && e.group == Group::Staged)
    );
    assert!(!h.editor.open);
}
