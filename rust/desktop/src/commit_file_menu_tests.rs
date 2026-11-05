use super::*;
use shovel_core::process::{self, Lifetime};
use std::fs;
// These two simulated applications exercise the process-wide shell admission
// guard; keep their injected handoffs separate even in the optional full suite.
static SHELL_TEST: std::sync::Mutex<()> = std::sync::Mutex::new(());
#[test]
fn file_inspection_menu_constructs_without_reads_and_opens_real_history_and_blame() {
    let mut h = Harness::new();
    let original = h.editor.git.clone();
    let mut missing = (*original).clone();
    missing.executable = h._temp.path().join("missing-git.exe");
    h.editor.git = Arc::new(missing);
    h.menu();
    assert!(!h.editor.busy && !h.editor.scheduler.has_accepted_work());
    h.editor.git = original;
    h.click(&text("FileHistoryMenu"));
    assert!(
        h.editor
            .inspector
            .as_ref()
            .is_some_and(|v| !v.busy && v.page.is_none())
    );
    h.until(|e| {
        e.inspector
            .as_ref()
            .is_some_and(|v| !v.busy && v.page.is_some())
    });
    assert_eq!(
        h.editor
            .inspector
            .as_ref()
            .unwrap()
            .page
            .as_ref()
            .unwrap()
            .entries
            .len(),
        1
    );
    h.editor.inspector.as_mut().unwrap().close();
    h.render(vec![]).drop_without_applying_deltas();
    h.menu();
    h.click(&text("FileBlameMenu"));
    assert!(
        h.editor
            .inspector
            .as_ref()
            .is_some_and(|v| !v.busy && v.blame.is_empty())
    );
    h.until(|e| {
        e.inspector
            .as_ref()
            .is_some_and(|v| !v.busy && !v.blame.is_empty())
    });
    assert_eq!(h.editor.inspector.as_ref().unwrap().blame[0].text, "base");
}

struct Harness {
    _temp: tempfile::TempDir,
    ctx: egui::Context,
    editor: Editor,
    copied: Vec<String>,
}

#[test]
fn patch_picker_cancellation_reads_nothing_and_keeps_owner_alive() {
    let mut h = Harness::new();
    let index = fs::read(h.editor.path.join(".git/index")).unwrap();
    let original = h.editor.git.clone();
    let mut missing = (*original).clone();
    missing.executable = h._temp.path().join("missing-git.exe");
    h.editor.git = Arc::new(missing);
    let (answer, receive) = mpsc::channel::<Option<PathBuf>>();
    let receive = std::sync::Mutex::new(receive);
    h.editor.patch_picker = Arc::new(move |_| {
        receive
            .lock()
            .unwrap()
            .recv_timeout(Duration::from_secs(20))
            .ok()
            .flatten()
    });
    h.menu();
    h.click(&text("FilePatchMenu"));
    assert!(h.editor.patch_picking && h.editor.action_busy() && h.editor.blocks_exit());
    assert!(!h.editor.scheduler.has_accepted_work());
    h.editor.request_leave(Leave::Close, &h.ctx);
    assert!(h.editor.open && h.editor.native_picker_open());
    assert_eq!(h.editor.status, Some(text("RustPatchChooserOpen")));
    h.editor.request_leave(Leave::Exit, &h.ctx);
    assert!(h.editor.open && h.editor.take_exit_cancelled());
    h.editor.git = original;
    answer.send(None).unwrap();
    h.until(|e| !e.patch_picking);
    assert_eq!(h.editor.status, Some(text("RustPatchNotSaved")));
    assert!(!h.editor.busy && !h.editor.scheduler.has_accepted_work());
    assert_eq!(fs::read(h.editor.path.join(".git/index")).unwrap(), index);
}

#[test]
fn patch_menu_saves_native_export_after_view_closes_without_changing_index() {
    let mut h = Harness::new();
    let output = h._temp.path().join("selected.patch");
    let destination = output.clone();
    h.editor.patch_picker = Arc::new(move |_| Some(destination.clone()));
    let index = fs::read(h.editor.path.join(".git/index")).unwrap();
    h.menu();
    h.click(&text("FilePatchMenu"));
    h.until(|e| e.busy);
    assert!(!h.editor.patch_picking && h.editor.scheduler.has_accepted_work());
    h.editor.request_leave(Leave::Close, &h.ctx);
    assert!(!h.editor.open);
    h.until(|e| !e.busy && !e.scheduler.has_accepted_work());
    assert_eq!(h.editor.status, Some(text("FilePatchSaved")));
    let patch = fs::read_to_string(output).unwrap();
    assert!(
        patch.contains("-staged") && patch.contains("+worktree") && !patch.contains("other.txt")
    );
    assert_eq!(fs::read(h.editor.path.join(".git/index")).unwrap(), index);
    assert_eq!(
        fs::read(h.editor.path.join("file.txt")).unwrap(),
        b"worktree\n"
    );
}

#[test]
fn patch_picker_return_rejects_a_replaced_selection_before_saving() {
    let mut h = Harness::new();
    let output = h._temp.path().join("keep.patch");
    fs::write(&output, "keep existing").unwrap();
    let (answer, receive) = mpsc::channel::<Option<PathBuf>>();
    let receive = std::sync::Mutex::new(receive);
    h.editor.patch_picker = Arc::new(move |_| {
        receive
            .lock()
            .unwrap()
            .recv_timeout(Duration::from_secs(20))
            .ok()
            .flatten()
    });
    h.menu();
    h.click(&text("FilePatchMenu"));
    assert!(h.editor.patch_picking);
    h.editor.context = Some(Arc::new(
        h.editor
            .git
            .working_copy(&h.editor.path, &Cancellation::default())
            .unwrap(),
    ));
    answer.send(Some(output.clone())).unwrap();
    h.until(|e| !e.patch_picking);
    assert_eq!(h.editor.status, Some(text("FileMenuStale")));
    assert!(!h.editor.busy && !h.editor.scheduler.has_accepted_work());
    assert_eq!(fs::read(output).unwrap(), b"keep existing");
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
fn shell_preparation_validates_literal_paths_missing_parents_and_junctions_without_git() {
    use std::os::windows::{fs::MetadataExt, process::CommandExt};
    let h = Harness::new();
    let repository = &h.editor.context.as_ref().unwrap().repository;
    let root = &repository.worktree;
    let literal = "-literal[1] שלום.txt";
    fs::write(root.join(literal), "working bytes").unwrap();
    let target = crate::file_shell::prepare(repository, literal, false).unwrap();
    assert_eq!(target.path, root.join(literal));
    assert!(!target.select_in_explorer && !target.missing_file_folder);
    assert!(
        crate::file_shell::prepare(repository, literal, true)
            .unwrap()
            .select_in_explorer
    );
    fs::create_dir(root.join("existing")).unwrap();
    let folder =
        crate::file_shell::prepare(repository, "existing/missing/deleted.txt", true).unwrap();
    assert_eq!(folder.path, root.join("existing"));
    assert!(folder.missing_file_folder && !folder.select_in_explorer);
    assert_eq!(
        crate::file_shell::prepare(repository, "existing", false).unwrap_err(),
        "FileMenuMissingFile"
    );
    assert_eq!(
        crate::file_shell::prepare(repository, "gone.txt", false).unwrap_err(),
        "FileMenuMissingFile"
    );
    for path in [
        "../outside.txt",
        ".git/config",
        "C:/outside.txt",
        "bad\0path",
    ] {
        assert!(crate::file_shell::prepare(repository, path, true).is_err());
    }
    let outside = h._temp.path().join("outside");
    fs::create_dir(&outside).unwrap();
    fs::write(outside.join("keep.txt"), "keep").unwrap();
    let link = root.join("junction");
    let cmd = PathBuf::from(std::env::var_os("SystemRoot").unwrap()).join("System32").join("cmd.exe");
    let native = |path: &std::path::Path| {
        let path = path.to_string_lossy().replace('/', "\\");
        let path = path.strip_prefix("\\\\?\\").unwrap_or(&path).to_owned();
        assert!(!path.contains(['"', '%', '!', '^', '&', '|', '<', '>']));
        path
    };
    let created = std::process::Command::new(cmd)
        .args(["/d", "/c"])
        .raw_arg(format!("mklink /J \"{}\" \"{}\"", native(&link), native(&outside)))
        .creation_flags(0x08000000)
        .output()
        .unwrap();
    assert!(
        created.status.success(),
        "junction fixture: {} {} {}",
        native(&link), native(&outside), String::from_utf8_lossy(&created.stderr)
    );
    assert_ne!(
        fs::symlink_metadata(&link).unwrap().file_attributes() & 0x400,
        0
    );
    let rejected = crate::file_shell::prepare(repository, "junction/keep.txt", true);
    fs::remove_dir(&link).unwrap(); // Exact nonrecursive junction removal, never its target.
    assert_eq!(rejected.unwrap_err(), "FileMenuLinkedPath");
    assert_eq!(fs::read(outside.join("keep.txt")).unwrap(), b"keep");
    let mut replaced = repository.clone();
    replaced.identity.fingerprint.push('0');
    assert_eq!(
        crate::file_shell::prepare(&replaced, literal, false).unwrap_err(),
        "FileMenuStale"
    );
}

#[test]
fn shell_open_menu_and_shortcut_handoff_current_working_bytes_survive_view_close() {
    let _serial = SHELL_TEST.lock().unwrap_or_else(|e| e.into_inner());
    let mut h = Harness::new();
    let original = h.editor.git.clone();
    let mut missing = (*original).clone();
    missing.executable = h._temp.path().join("missing-git.exe");
    h.editor.git = Arc::new(missing);
    let captured = Arc::new(std::sync::Mutex::new(Vec::new()));
    let targets = captured.clone();
    let (release, receive) = mpsc::channel::<()>();
    let receive = std::sync::Mutex::new(receive);
    h.editor.shell_launcher = Arc::new(move |target| {
        targets
            .lock()
            .unwrap()
            .push((target.clone(), fs::read(&target.path).unwrap()));
        let _ = receive
            .lock()
            .unwrap()
            .recv_timeout(Duration::from_secs(20));
        Ok(())
    });
    let index = fs::read(h.editor.path.join(".git/index")).unwrap();
    h.menu();
    assert!(!h.editor.scheduler.has_accepted_work());
    h.click(&format!("{} {}", text("FileMenuOpen"), text("FileMenuOpenKeys")));
    h.until(|_| !captured.lock().unwrap().is_empty());
    assert!(h.editor.busy && h.editor.scheduler.has_accepted_work());
    assert!(matches!(
        crate::file_shell::Guard::acquire(),
        Err("FileMenuShellBusy")
    ));
    h.editor.request_leave(Leave::Close, &h.ctx);
    assert!(!h.editor.open);
    release.send(()).unwrap();
    h.until(|e| !e.busy && !e.scheduler.has_accepted_work());
    assert_eq!(h.editor.status, Some(text("FileMenuOpened")));
    assert_eq!(captured.lock().unwrap()[0].1, b"worktree\n");
    assert!(!captured.lock().unwrap()[0].0.select_in_explorer);
    assert_eq!(fs::read(h.editor.path.join(".git/index")).unwrap(), index);
    h.editor.git = original;
    h.editor.activate();
    h.until(|e| !e.loading && !e.refresh_pending);
    let invoked = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let count = invoked.clone();
    h.editor.shell_launcher = Arc::new(move |_| {
        count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(())
    });
    h.click("M  file.txt");
    h.key(
        egui::Key::O,
        egui::Modifiers {
            ctrl: true,
            shift: true,
            alt: true,
            command: true,
            ..Default::default()
        },
    );
    h.until(|e| !e.busy && invoked.load(std::sync::atomic::Ordering::SeqCst) == 1);
    assert_eq!(h.editor.status, Some(text("FileMenuOpened")));
}

#[test]
fn shell_explorer_fallback_and_launch_failures_keep_the_real_repository_unchanged() {
    let _serial = SHELL_TEST.lock().unwrap_or_else(|e| e.into_inner());
    let mut h = Harness::new();
    fs::remove_file(h.editor.path.join("file.txt")).unwrap();
    h.editor.refresh(&h.ctx);
    h.until(|e| {
        !e.loading
            && e.context.as_ref().is_some_and(|c| {
                c.entries
                    .iter()
                    .any(|e| e.path == "file.txt" && e.code == 'D')
            })
    });
    let point = h.point("D  file.txt");
    h.click_at(point, egui::PointerButton::Secondary);
    let output = h.render(vec![]);
    assert!(
        output
            .platform_output
            .accesskit_update
            .as_ref()
            .unwrap()
            .nodes
            .iter()
            .any(|(_, n)| n.label() == Some(format!("{} {}", text("FileMenuOpen"), text("FileMenuOpenKeys")).as_str()) && n.is_disabled())
    );
    output.drop_without_applying_deltas();
    let captured = Arc::new(std::sync::Mutex::new(None));
    let target = captured.clone();
    h.editor.shell_launcher = Arc::new(move |prepared| {
        *target.lock().unwrap() = Some(prepared.clone());
        Ok(())
    });
    h.click(&text("FileMenuExplorer"));
    h.until(|e| !e.busy && !e.scheduler.has_accepted_work());
    assert_eq!(h.editor.status, Some(text("FileMenuOpenedFolder")));
    assert_eq!(
        captured.lock().unwrap().as_ref().unwrap().path,
        h.editor.context.as_ref().unwrap().repository.worktree
    );
    let index = fs::read(h.editor.path.join(".git/index")).unwrap();
    let point = h.point("D  file.txt");
    h.click_at(point, egui::PointerButton::Secondary);
    h.editor.shell_launcher = Arc::new(|_| Err("FileMenuOpenFailed"));
    h.click(&text("FileMenuExplorer"));
    h.until(|e| !e.busy && !e.scheduler.has_accepted_work());
    assert_eq!(h.editor.status, Some(text("FileMenuOpenFailed")));
    assert!(!h.editor.path.join("file.txt").exists());
    assert_eq!(fs::read(h.editor.path.join(".git/index")).unwrap(), index);
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

#[test]
fn selected_stash_confirms_frozen_paths_and_message_then_finishes_after_view_closes() {
    let mut h = Harness::new();
    let original = h.editor.git.clone();
    let mut missing = (*original).clone();
    missing.executable = h._temp.path().join("missing-git.exe");
    h.editor.git = Arc::new(missing);
    h.menu();
    h.click(&strings::format("FileStashLabel", &["1"]));
    assert!(!h.editor.scheduler.has_accepted_work());
    assert!(
        h.editor
            .file_confirmation
            .as_ref()
            .is_some_and(|c| c.action == file_menu::FileAction::Stash)
    );
    let out = h.render(vec![]);
    let nodes = &out.platform_output.accesskit_update.as_ref().unwrap().nodes;
    assert!(nodes.iter().any(
        |(_, n)| n.value() == Some(strings::format("FileStashTarget", &["file.txt"]).as_str())
    ));
    let input = nodes
        .iter()
        .find(|(_, n)| {
            n.role() == egui::Role::TextInput
                && n.bounds().is_some_and(|r| r.height() < 60.)
                && n.labelled_by().len() == 1
        })
        .unwrap();
    let bounds = input.1.bounds().unwrap();
    let point = egui::pos2(
        (bounds.x0 + bounds.x1) as f32 / 2.,
        (bounds.y0 + bounds.y1) as f32 / 2.,
    );
    out.drop_without_applying_deltas();
    h.click_at(point, egui::PointerButton::Primary);
    h.render(vec![egui::Event::Text("selected שלום".into())])
        .drop_without_applying_deltas();
    assert_eq!(
        h.editor.file_confirmation.as_ref().unwrap().message,
        "selected שלום"
    );
    h.click(&text("ActionCancel"));
    assert!(!h.editor.busy);
    assert_eq!(
        fs::read(h.editor.path.join("file.txt")).unwrap(),
        b"worktree\n"
    );
    h.editor.git = original;
    h.menu();
    h.click(&strings::format("FileStashLabel", &["1"]));
    h.editor.file_confirmation.as_mut().unwrap().message = "selected שלום".into();
    h.click(&text("FileStashButton"));
    assert!(h.editor.busy);
    h.editor.request_leave(Leave::Close, &h.ctx);
    assert!(!h.editor.open && h.editor.blocks_exit());
    h.until(|e| !e.busy);
    assert!(
        h.editor
            .status
            .as_deref()
            .unwrap()
            .starts_with(&text("CommitSucceededStashFiles")),
        "{:?}",
        h.editor.status
    );
    assert_eq!(fs::read(h.editor.path.join("file.txt")).unwrap(), b"base\n");
    assert_eq!(
        fs::read(h.editor.path.join("other.txt")).unwrap(),
        b"keep changed\n"
    );
    let context = h.editor.context.as_ref().unwrap();
    let list = h
        .editor
        .git
        .stashes(&context.repository, &Cancellation::default())
        .unwrap();
    assert_eq!(list.entries.len(), 1);
    assert!(list.entries[0].message.ends_with("selected שלום"));
    assert!(!h.editor.open);
}

#[test]
fn configured_external_diff_menu_submits_directly_and_tool_survives_closed_view() {
    let mut h = Harness::new();
    let old = h.editor.context.as_ref().unwrap().config_stamp.clone();
    for args in [
        vec!["config", "diff.guitool", "fixture"],
        vec![
            "config",
            "difftool.fixture.cmd",
            "cat \"$LOCAL\" > tool-old; cat \"$REMOTE\" > tool-new; touch tool-started; while test ! -e tool-release; do sleep .05; done; printf 'tool edited\\n' > \"$REMOTE\"; echo tool-finished",
        ],
    ] {
        let r = process::run(
            &h.editor
                .git
                .request(&h.editor.path, args, Lifetime::AcceptedCommand, 65536),
            &Cancellation::default(),
            None,
        )
        .unwrap();
        assert!(r.success(), "{r:?}");
    }
    h.editor.refresh(&h.ctx);
    h.until(|e| !e.loading && e.context.as_ref().is_some_and(|c| c.config_stamp != old));
    let original = h.editor.git.clone();
    let mut missing = (*original).clone();
    missing.executable = h._temp.path().join("missing-git.exe");
    h.editor.git = Arc::new(missing);
    h.menu();
    assert!(!h.editor.scheduler.has_accepted_work() && !h.editor.loading);
    h.editor.git = original;
    h.click(&text("FileExternalDiff"));
    assert!(h.editor.busy && h.editor.file_confirmation.is_none());
    h.until(|e| e.path.join("tool-started").exists());
    assert_eq!(
        fs::read(h.editor.path.join("tool-old")).unwrap(),
        b"staged\n"
    );
    assert_eq!(
        fs::read(h.editor.path.join("tool-new")).unwrap(),
        b"worktree\n"
    );
    h.editor.request_leave(Leave::Close, &h.ctx);
    assert!(!h.editor.open && h.editor.blocks_exit() && h.editor.scheduler.has_accepted_work());
    fs::write(h.editor.path.join("tool-release"), "").unwrap();
    h.until(|e| !e.busy);
    assert_eq!(h.editor.status, Some(text("CommitSucceededExternalDiff")));
    assert!(h.editor.operation_output.contains("tool-finished"));
    assert_eq!(
        fs::read(h.editor.path.join("file.txt")).unwrap(),
        b"tool edited\n"
    );
    assert!(!h.editor.open);
    assert!(
        h.editor
            .context
            .as_ref()
            .unwrap()
            .entries
            .iter()
            .any(|e| e.group == Group::Unstaged && e.path == "file.txt")
    );
}

#[test]
fn ignore_submenu_reads_nothing_and_writes_only_the_selected_root_rule() {
    let mut h = Harness::new();
    fs::write(h.editor.path.join("new [literal].tmp"), "keep selected").unwrap();
    fs::write(h.editor.path.join("keep.tmp"), "keep unrelated").unwrap();
    h.editor.refresh(&h.ctx);
    h.until(|e| {
        !e.loading
            && e.context
                .as_ref()
                .is_some_and(|c| c.entries.iter().any(|e| e.path == "new [literal].tmp"))
    });
    let original = h.editor.git.clone();
    let mut missing = (*original).clone();
    missing.executable = h._temp.path().join("missing-git.exe");
    h.editor.git = Arc::new(missing);
    let point = h.point("?  new [literal].tmp");
    h.click_at(point, egui::PointerButton::Secondary);
    h.click(&text("FileIgnoreMenu"));
    assert!(!h.editor.loading && !h.editor.scheduler.has_accepted_work());
    assert!(!h.editor.path.join(".gitignore").exists());
    h.editor.git = original;
    h.click(&text("FileIgnorePaths"));
    assert!(h.editor.busy && h.editor.file_confirmation.is_none());
    h.until(|e| !e.busy);
    assert_eq!(h.editor.status, Some(text("CommitSucceededIgnoreFiles")));
    assert_eq!(
        fs::read(h.editor.path.join(".gitignore")).unwrap(),
        b"/new\\ \\[literal\\].tmp\n"
    );
    assert_eq!(
        fs::read(h.editor.path.join("new [literal].tmp")).unwrap(),
        b"keep selected"
    );
    assert!(
        h.editor
            .context
            .as_ref()
            .unwrap()
            .entries
            .iter()
            .any(|e| e.path == "keep.tmp")
    );
    assert!(
        !h.editor
            .context
            .as_ref()
            .unwrap()
            .entries
            .iter()
            .any(|e| e.path == "new [literal].tmp")
    );
}

#[test]
fn lfs_submenu_applies_attribute_reset_and_tracked_ignore_is_disabled() {
    let mut h = Harness::new();
    h.menu();
    let out = h.render(vec![]);
    assert!(
        out.platform_output
            .accesskit_update
            .as_ref()
            .unwrap()
            .nodes
            .iter()
            .any(|(_, n)| n.label() == Some(text("FileIgnoreMenu").as_str()) && n.is_disabled())
    );
    out.drop_without_applying_deltas();
    h.click(&text("FileLfsMenu"));
    assert!(!h.editor.scheduler.has_accepted_work());
    h.click(&text("FileLfsUntrack"));
    assert!(h.editor.busy && h.editor.file_confirmation.is_none());
    h.until(|e| !e.busy);
    assert_eq!(h.editor.status, Some(text("CommitSucceededLfsUntrack")));
    assert_eq!(
        fs::read(h.editor.path.join(".gitattributes")).unwrap(),
        b"\"/file.txt\" !filter !diff !merge !text\n"
    );
    assert_eq!(
        fs::read(h.editor.path.join("file.txt")).unwrap(),
        b"worktree\n"
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
}
