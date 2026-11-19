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
            |_| self.editor.show(&ctx, true, |_| {}),
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
        self.click("Conflicted · Unmerged · conflict.txt");
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
    h.click(&text("ConflictCurrentSide"));
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
fn stage_all_skips_conflicted_file_and_unchecking_both_sides_restores_base() {
    let mut h = Harness::new();
    h.open();
    let original_index = command(&h.editor.git, &h.editor.path, &["ls-files", "--unmerged"]).stdout;
    h.editor.stage(false, false, &h.ctx);
    assert!(!h.editor.scheduler.has_accepted_work());
    h.editor.stage(false, true, &h.ctx);
    h.until(|e| {
        e.current_context().is_some_and(|context| context.change(Group::Staged, "other.txt").is_some())
            && !e.loading
            && !e.action_busy()
            && e.conflict.as_ref().is_some_and(|p| !p.busy && p.status == text("ConflictChoose"))
    });
    assert_eq!(
        command(&h.editor.git, &h.editor.path, &["ls-files", "--unmerged"]).stdout,
        original_index
    );
    h.click(&text("ConflictCurrentSide"));
    h.click(&text("ConflictIncomingSide"));
    h.click(&text("ConflictBaseAction"));
    h.until(|e| e.conflict.is_none());
    assert_eq!(fs::read_to_string(h.editor.path.join("conflict.txt")).unwrap(), "base\n");
}

#[test]
fn refreshing_after_external_conflict_resolution_removes_the_retained_conflict_diff() {
    let mut h = Harness::new();
    h.until(|e| e.draft_loaded && e.context.is_some() && !e.loading);
    h.click("Unstaged · Modified · other.txt");
    h.until(|e| e.diff_current);
    h.open();
    assert!(
        h.editor.diff.is_some(),
        "the comparison behind the resolver is retained"
    );
    ok(&h.editor.git, &h.editor.path, &["merge", "--abort"]);
    h.editor.refresh(&h.ctx);
    h.until(|e| !e.loading && e.focused.is_none());
    assert!(h.editor.conflict.is_none());
    assert!(h.editor.diff.is_none());
    assert!(h.editor.displayed_file.is_none());
    assert!(h.editor.path.join("conflict.txt").is_file());
}

#[test]
fn recovery_controls_appear_below_tabs_after_rebase_pauses() {
    use crate::workspace_interaction_tests::{sidebar_point, workspace};
    let h = Harness::new();
    let git = &h.editor.git;
    let path = &h.editor.path;
    ok(git, path, &["merge", "--abort"]);
    ok(git, path, &["restore", "other.txt"]);
    assert!(!command(git, path, &["rebase", "side"]).success());
    let mut app = workspace();
    app.tabs.clear();
    app.git = Some(git.clone());
    app.open(path.clone(), &h.ctx);
    crate::history_view::tests::wait(&mut app, &h.ctx);
    let snapshot = &app.tabs[0].history.as_ref().unwrap().snapshot;
    let (release, held) = mpsc::channel();
    let (started, running) = mpsc::channel();
    app.scheduler.submit(
        snapshot.identity.fingerprint.clone(),
        shovel_core::case::path_key(&snapshot.common_directory),
        "fixture-rebase".into(),
        Lane::Exclusive,
        move || {
            let _ = started.send(());
            let _ = held.recv();
            Outcome::Succeeded
        },
        |_| {},
    );
    running.recv_timeout(Duration::from_secs(5)).unwrap();
    let label = text("ActionTitleContinue");
    assert!(sidebar_point(&h.ctx, &mut app, &label).is_none());
    release.send(()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while app.scheduler.has_accepted_work() {
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(5));
    }
    let (banner, _) = sidebar_point(&h.ctx, &mut app, &label).unwrap();
    let name = app.tabs[0].name().into_owned();
    let (tab, _) = sidebar_point(&h.ctx, &mut app, &name).unwrap();
    assert!(banner.y > tab.y);
}

#[test]
fn typed_text_survives_cancelled_navigation_failed_stage_and_exit_confirmation() {
    let mut h = Harness::new();
    h.open();
    h.edit("resolved by editor\n");
    h.click("Unstaged · Modified · other.txt");
    assert!(h.editor.leave.is_some());
    h.click(&text("ActionCancel"));
    assert!(h.editor.unsaved_conflict());
    assert_eq!(h.editor.focused.as_ref().unwrap().path, "conflict.txt");
    fs::write(
        h.editor.path.join(".git/index.lock"),
        "external fixture lock",
    )
    .unwrap();
    h.click(&text("ConflictMergeAction"));
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
        git_selection: Default::default(),
        repository_picker: None,
        tabs: vec![],
        retained_tabs: vec![],
        restore_window_layout: false,
        main_geometry: Default::default(),
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
        discard_settings_on_exit: false,
        read_workers: Default::default(),
        exit_ready: false,
        confirm_exit_without_save: false,
        activity_open: false,
        activity_all: false,
        auto_fetch: Default::default(),
        settings_dialog: None,
        updater: Default::default(),
        external_tools_opened: false,
        tab_drag: None,
        scheduler: h.editor.scheduler.clone(),
        editors: vec![h.editor],
        draft_store: None,
        operations: Default::default(),
        branches: Default::default(),
        repository_dialog: None,
        remotes: Default::default(),
        initialization: Default::default(),
        stash_views: vec![],
        history_menu: None,
        reference_menu: None,
        deferred_history: Default::default(),
        tab_markers: Default::default(),
    };
    workspace.updater.restart_requested = true;
    workspace.close(&h.ctx);
    assert!(workspace.pending_exit && !workspace.closing && !workspace.exit_ready);
    assert!(workspace.editors[0].exit_prompt_open());
    workspace.editors[0].answer_leave(false, &h.ctx);
    workspace.receive(&h.ctx);
    assert!(!workspace.pending_exit && !workspace.closing && !workspace.exit_ready);
    assert!(workspace.editors[0].unsaved_conflict());
    assert!(!workspace.updater.restart_requested);
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

#[test]
fn shutdown_waits_for_removed_conflict_preview_without_resolving_files() {
    let mut h = Harness::new();
    h.until(|e| e.draft_loaded && e.context.is_some() && !e.loading);
    let mut app = crate::workspace_interaction_tests::workspace();
    app.tabs.clear();
    app.git = Some(h.editor.git.clone());
    app.scheduler = h.editor.scheduler.clone();
    let mut profile = crate::SettingsStore::new(h._temp.path().join("profile"));
    app.settings = profile.load().unwrap();
    app.store = Some(profile);
    h.editor.read_workers = app.read_workers.clone();
    let change = h
        .editor
        .context
        .as_ref()
        .unwrap()
        .entries
        .iter()
        .find(|e| e.group == Group::Conflicted)
        .unwrap()
        .clone();
    let before = fs::read(h.editor.path.join(&change.path)).unwrap();
    let index = command(
        &h.editor.git,
        &h.editor.path,
        &["ls-files", "--stage", "-z"],
    )
    .stdout;
    h.editor.focused = Some(change);
    h.editor.open_conflict();
    let (started, release) = crate::history_cache::tests::hold_read(h.editor.path.clone());
    h.render(vec![]).drop_without_applying_deltas();
    started.recv_timeout(Duration::from_secs(5)).unwrap();
    h.editor.conflict = None;
    app.editors.push(h.editor);
    app.close(&h.ctx);
    assert!(app.pending_exit && !app.exit_ready);
    release.send(()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while !app.exit_ready {
        app.receive(&h.ctx);
        assert!(Instant::now() < deadline, "{:?}", app.notice);
        thread::sleep(Duration::from_millis(3));
    }
    let editor = &app.editors[0];
    assert_eq!(fs::read(editor.path.join("conflict.txt")).unwrap(), before);
    assert_eq!(
        command(&editor.git, &editor.path, &["ls-files", "--stage", "-z"]).stdout,
        index
    );
    assert!(app.read_workers.cancel_and_drained());
}
