use super::*;
use shovel_core::process::{self, Lifetime};
use std::{fs, path::Path};

fn command(git: &Git, root: &Path, args: &[&str]) -> process::Result {
    process::run(
        &git.request(root, args.iter().copied(), Lifetime::AcceptedCommand, 65536),
        &Cancellation::default(),
        None,
    )
    .unwrap()
}
fn ok(git: &Git, root: &Path, args: &[&str]) {
    let result = command(git, root, args);
    assert!(result.success(), "{result:?}");
}
struct Harness {
    _temp: tempfile::TempDir,
    ctx: egui::Context,
    editor: Editor,
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
        ok(&git, &root, &["init", "--initial-branch=main"]);
        fs::write(root.join("conflict.txt"), "base\n").unwrap();
        fs::write(root.join("other.txt"), "keep\n").unwrap();
        ok(&git, &root, &["add", "conflict.txt", "other.txt"]);
        ok(&git, &root, &["commit", "-m", "base"]);
        ok(&git, &root, &["switch", "-c", "side"]);
        fs::write(root.join("conflict.txt"), "incoming\n").unwrap();
        ok(&git, &root, &["commit", "-am", "incoming"]);
        ok(&git, &root, &["switch", "main"]);
        fs::write(root.join("conflict.txt"), "current\n").unwrap();
        ok(&git, &root, &["commit", "-am", "current"]);
        assert!(!command(&git, &root, &["merge", "--no-edit", "side"]).success());
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
        Self {
            _temp: temp,
            ctx,
            editor,
        }
    }
    fn render(&mut self, events: Vec<egui::Event>) -> egui::FullOutput {
        let ctx = self.ctx.clone();
        self.editor.poll(&ctx);
        let mut output = ctx.run_ui(
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
        output.textures_delta.clear();
        output
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
                "{:?} / {:?}",
                self.editor.status,
                self.editor.conflict.as_ref().map(|p| &p.status)
            );
            thread::sleep(Duration::from_millis(15));
        }
    }
    fn point(&mut self, predicate: impl Fn(&egui::accesskit::Node) -> bool) -> egui::Pos2 {
        let output = self.render(vec![]);
        let nodes = &output
            .platform_output
            .accesskit_update
            .as_ref()
            .unwrap()
            .nodes;
        let node = nodes
            .iter()
            .find(|(_, n)| n.bounds().is_some() && predicate(n))
            .unwrap_or_else(|| {
                panic!(
                    "missing control: {:?}",
                    nodes
                        .iter()
                        .map(|(_, n)| (n.role(), n.label(), n.value()))
                        .collect::<Vec<_>>()
                )
            });
        let rect = node.1.bounds().unwrap();
        let point = egui::pos2(
            (rect.x0 + rect.x1) as f32 / 2.,
            (rect.y0 + rect.y1) as f32 / 2.,
        );
        output.drop_without_applying_deltas();
        point
    }
    fn at(&mut self, point: egui::Pos2) {
        self.render(vec![egui::Event::PointerMoved(point)])
            .drop_without_applying_deltas();
        for pressed in [true, false] {
            self.render(vec![egui::Event::PointerButton {
                pos: point,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            }])
            .drop_without_applying_deltas();
        }
    }
    fn click(&mut self, label: &str) {
        let point = self.point(|n| {
            n.label() == Some(label)
                && matches!(n.role(), egui::Role::Button | egui::Role::CheckBox)
        });
        self.at(point);
    }
    fn open(&mut self) {
        self.until(|e| e.draft_loaded && e.context.is_some() && !e.loading);
        let change = self
            .editor
            .context
            .as_ref()
            .unwrap()
            .entries
            .iter()
            .find(|e| e.path == "conflict.txt")
            .unwrap()
            .clone();
        self.click(&format!("{}  {}", change.code, change.path));
        self.click(&text("ConflictOpen"));
        assert!(self.editor.conflict.is_some());
        self.until(|e| {
            e.conflict
                .as_ref()
                .is_some_and(|p| !p.busy && p.status == text("ConflictChoose"))
        });
    }
    fn edit(&mut self, value: &str) {
        let point = self.point(|n| {
            n.role() == egui::Role::MultilineTextInput
                && n.value().is_some_and(|v| v.contains("<<<<<<<"))
        });
        self.at(point);
        let ctrl = egui::Modifiers {
            ctrl: true,
            command: true,
            ..Default::default()
        };
        self.render(vec![egui::Event::Key {
            key: egui::Key::A,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: ctrl,
        }])
        .drop_without_applying_deltas();
        self.render(vec![egui::Event::Text(value.into())])
            .drop_without_applying_deltas();
        assert!(self.editor.unsaved_conflict());
    }
}

#[test]
fn checkboxes_only_preview_then_explicit_resolution_stages_and_reconciles() {
    let mut h = Harness::new();
    h.render(vec![]).drop_without_applying_deltas();
    assert!(
        !h.editor.initialized,
        "the invisible sizing pass must not start reads"
    );
    h.open();
    let before = fs::read(h.editor.path.join("conflict.txt")).unwrap();
    let index = h.editor.context.as_ref().unwrap().index_stamp.clone();
    h.click(&strings::format("ConflictCurrentSide", &["main"]));
    assert_eq!(
        fs::read(h.editor.path.join("conflict.txt")).unwrap(),
        before
    );
    assert_eq!(h.editor.context.as_ref().unwrap().index_stamp, index);
    assert!(!h.editor.scheduler.has_accepted_work());
    assert!(!h.editor.unsaved_conflict());
    h.click(&text("ConflictResolveStage"));
    h.until(|e| e.conflict.is_none());
    assert_eq!(
        fs::read_to_string(h.editor.path.join("conflict.txt")).unwrap(),
        "incoming\n"
    );
    assert_eq!(h.editor.status, Some(text("ConflictResolved")));
    assert!(
        h.editor
            .context
            .as_ref()
            .unwrap()
            .entries
            .iter()
            .any(|e| e.path == "conflict.txt" && e.group == Group::Staged)
    );
    assert_eq!(h.editor.focused.as_ref().unwrap().group, Group::Staged);
    assert!(h.editor.path.join(".git/MERGE_HEAD").is_file());
    assert_eq!(
        fs::read_to_string(h.editor.path.join("other.txt")).unwrap(),
        "keep changed\n"
    );
}

#[test]
fn typed_text_survives_cancelled_navigation_failed_stage_and_exit_confirmation() {
    let mut h = Harness::new();
    h.open();
    h.edit("resolved by editor\n");
    let change = h
        .editor
        .context
        .as_ref()
        .unwrap()
        .entries
        .iter()
        .find(|e| e.path == "other.txt")
        .unwrap()
        .clone();
    h.click(&format!("{}  {}", change.code, change.path));
    assert!(h.editor.leave.is_some());
    h.click(&text("ActionCancel"));
    assert!(h.editor.unsaved_conflict());
    assert_eq!(h.editor.focused.as_ref().unwrap().path, "conflict.txt");
    fs::write(
        h.editor.path.join(".git/index.lock"),
        "external fixture lock",
    )
    .unwrap();
    h.click(&text("ConflictResolveStage"));
    h.until(|e| !e.action_busy());
    assert!(h.editor.unsaved_conflict());
    assert!(h.editor.output.contains("index.lock"));
    assert_eq!(
        fs::read_to_string(h.editor.path.join("conflict.txt")).unwrap(),
        "resolved by editor\n"
    );
    assert!(
        h.editor
            .context
            .as_ref()
            .unwrap()
            .entries
            .iter()
            .any(|e| e.group == Group::Conflicted)
    );
    fs::remove_file(h.editor.path.join(".git/index.lock")).unwrap();
    h.editor.confirm_exit(&h.ctx);
    assert!(h.editor.exit_prompt_open());
    h.click(&text("ActionCancel"));
    assert!(h.editor.take_exit_cancelled());
    assert!(h.editor.open && h.editor.unsaved_conflict());
    h.editor.confirm_exit(&h.ctx);
    h.click(&text("RustDiscardConflictText"));
    assert!(!h.editor.open && !h.editor.unsaved_conflict());
    assert_eq!(
        fs::read_to_string(h.editor.path.join("conflict.txt")).unwrap(),
        "resolved by editor\n"
    );
    h.editor.activate();
    h.open();
    assert!(!h.editor.unsaved_conflict());
}

#[test]
fn accepted_external_merge_finishes_after_commit_window_closes() {
    let mut h = Harness::new();
    h.editor.external_merge_tool = shovel_core::conflicts::ToolSettings {
        mode: 2, command: "touch tool-started\nwhile test ! -f tool-release; do sleep .05; done\ncp -- \"$REMOTE\" \"$MERGED\"".into(), ..Default::default()
    };
    h.open();
    h.click(&text("ConflictExternal"));
    h.until(|e| e.path.join("tool-started").exists());
    assert!(h.editor.scheduler.has_accepted_work());
    h.editor.request_leave(Leave::Close, &h.ctx);
    assert!(!h.editor.open && h.editor.blocks_exit());
    assert!(h.editor.conflict.as_ref().unwrap().mutating);
    fs::write(h.editor.path.join("tool-release"), "").unwrap();
    h.until(|e| !e.action_busy());
    assert!(!h.editor.open);
    assert_eq!(h.editor.status, Some(text("ConflictResolved")));
    assert_eq!(
        fs::read_to_string(h.editor.path.join("conflict.txt")).unwrap(),
        "incoming\n"
    );
    assert!(
        h.editor
            .context
            .as_ref()
            .unwrap()
            .entries
            .iter()
            .all(|e| e.group != Group::Conflicted)
    );
    h.editor.activate();
    assert!(h.editor.open && h.editor.conflict.is_none());
}

#[test]
fn app_exit_waits_for_unsaved_conflict_choice_and_resumes_settings_save() {
    let mut h = Harness::new();
    h.open();
    h.edit("text only in the editor\n");
    let mut store = shovel_core::settings::SettingsStore::new(h._temp.path().join("profile"));
    let settings = store.load().unwrap();
    let (sender, receiver) = mpsc::channel();
    let mut workspace = crate::Workspace {
        settings,
        store: Some(store),
        git: Some(h.editor.git.clone()),
        tabs: vec![],
        selected: 0,
        next_id: 1,
        sender,
        receiver,
        notice: None,
        initialized: true,
        saving: false,
        preferences_dirty: false,
        closing: false,
        pending_exit: false,
        exit_ready: false,
        confirm_exit_without_save: false,
        scheduler: h.editor.scheduler.clone(),
        editors: vec![h.editor],
        draft_store: None,
        operations: Default::default(),
        branches: Default::default(),
        stash_views: vec![],
        history_menu: None,
        reference_menu: None,
    };
    workspace.close(&h.ctx);
    assert!(workspace.pending_exit && !workspace.closing && !workspace.exit_ready);
    assert!(workspace.editors[0].exit_prompt_open());
    workspace.editors[0].answer_leave(false, &h.ctx);
    workspace.receive(&h.ctx);
    assert!(!workspace.pending_exit && !workspace.closing && !workspace.exit_ready);
    assert!(workspace.editors[0].unsaved_conflict());
    workspace.close(&h.ctx);
    workspace.editors[0].answer_leave(true, &h.ctx);
    workspace.receive(&h.ctx);
    assert!(workspace.closing && !workspace.pending_exit);
    let start = Instant::now();
    while !workspace.exit_ready {
        workspace.receive(&h.ctx);
        assert!(start.elapsed() < Duration::from_secs(5));
        thread::sleep(Duration::from_millis(10));
    }
    assert!(
        fs::read_to_string(workspace.editors[0].path.join("conflict.txt"))
            .unwrap()
            .contains("<<<<<<<")
    );
}
