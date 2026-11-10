use super::tests::{command, fixture, wait};
use super::*;
use crate::workspace_interaction_tests::{frame, workspace};
use std::fs;

fn controller() -> Controller {
    let helper =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/debug/ShovelRustAskPass.exe");
    assert!(
        helper.is_file(),
        "build shovel-askpass before this focused check"
    );
    Controller {
        helper: Some(helper),
        ..Default::default()
    }
}

#[test]
fn initializing_toolbar_push_coalesces_and_finishes_after_all_tabs_close() {
    let (_temp, git, snapshot) = fixture();
    command(&git, &snapshot.worktree, &["tag", "remembered-tag"]);
    let before_index = command(&git, &snapshot.worktree, &["ls-files", "--stage", "-z"]);
    let before_config = fs::read(snapshot.git_directory.join("config")).unwrap();
    let mut app = workspace();
    app.tabs.clear();
    app.selected = 0;
    app.git = Some(git.clone());
    app.operations = controller();
    app.settings
        .set("AutoFetchEnabled", serde_json::json!(false));
    let mut saved = Options::load(&app.settings, &snapshot, Kind::Push);
    saved.all_tags = true;
    saved.save(&mut app.settings, &snapshot);
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    app.open(snapshot.worktree.clone(), &ctx);
    let owner = app.tabs[0].id;
    let output = frame(&ctx, &mut app, vec![]);
    let node = &output
        .platform_output
        .accesskit_update
        .as_ref()
        .unwrap()
        .nodes
        .iter()
        .find(|(_, node)| node.label() == Some("Push"))
        .unwrap()
        .1;
    assert!(
        !node.is_disabled(),
        "initialization must not disable the requested action"
    );
    let bounds = node.bounds().unwrap();
    let pos = egui::pos2(
        (bounds.x0 + bounds.x1) as f32 / 2.,
        (bounds.y0 + bounds.y1) as f32 / 2.,
    );
    output.drop_without_applying_deltas();
    for _ in 0..2 {
        for events in [
            vec![egui::Event::PointerMoved(pos)],
            vec![egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: Default::default(),
            }],
            vec![egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: Default::default(),
            }],
        ] {
            frame(&ctx, &mut app, events).drop_without_applying_deltas();
        }
    }
    assert!(
        app.tabs[0].loading,
        "initialization result has not yet been published"
    );
    assert_eq!(app.operations.pending.len(), 1);
    assert_eq!(app.operations.runs.len(), 1);
    assert!(app.scheduler.has_accepted_work());
    assert!(app.scheduler.records().iter().all(|r| !r.running));
    app.close_tab(owner, &ctx);
    app.close(&ctx);
    assert!(!app.exit_ready && !app.closing);
    assert!(app.tabs.is_empty());
    wait(|| {
        app.receive(&ctx);
        app.operations.runs[0].done && !app.scheduler.has_accepted_work()
    });
    assert_eq!(
        app.operations.runs[0].status,
        text("ActionCompletedShort"),
        "{}",
        app.operations.runs[0].output
    );
    assert!(app.operations.pending.is_empty());
    assert_eq!(app.retained_tabs[0].id, owner);
    assert!(app.retained_tabs[0].history.is_some());
    let remote = snapshot.worktree.join("remote.git");
    assert_eq!(
        command(&git, &remote, &["rev-parse", "main"]),
        snapshot.head.clone().unwrap()
    );
    assert_eq!(
        command(&git, &remote, &["rev-parse", "refs/tags/remembered-tag"]),
        snapshot.head.unwrap()
    );
    assert_eq!(
        command(&git, &snapshot.worktree, &["ls-files", "--stage", "-z"]),
        before_index
    );
    assert_eq!(
        fs::read(snapshot.git_directory.join("config")).unwrap(),
        before_config
    );
}

#[test]
fn initializing_removal_and_failure_are_scoped_and_never_start_native_git() {
    let (_a, git_a, a) = fixture();
    let (_b, git_b, b) = fixture();
    let scheduler = Scheduler::default();
    let ctx = egui::Context::default();
    let settings = Settings::default();
    let mut ops = controller();
    ops.defer_network(Kind::Push, &a.worktree, &settings, &scheduler, &ctx);
    assert!(scheduler.remove_waiting(ops.pending[0].request));
    ops.poll(&ctx);
    assert!(ops.pending.is_empty());
    assert_eq!(ops.runs[0].status, text("QueuedRequestCancelled"));
    ops.defer_network(Kind::Push, &a.worktree, &settings, &scheduler, &ctx);
    ops.defer_network(Kind::Pull, &a.worktree, &settings, &scheduler, &ctx);
    ops.defer_network(Kind::Push, &b.worktree, &settings, &scheduler, &ctx);
    ops.resolve_initialization(
        &a.worktree,
        Err("fixture initialization failed"),
        &git_a,
        &scheduler,
        &ctx,
    );
    ops.poll(&ctx);
    assert_eq!(ops.runs[1].status, text("ActionFailedShort"));
    assert!(ops.runs[1].output.contains("fixture initialization failed"));
    assert_eq!(ops.runs[2].status, text("ActionStoppedShort"));
    assert_eq!(ops.pending.len(), 1);
    assert!(scheduler.has_accepted_work());
    assert_eq!(
        command(
            &git_a,
            &a.worktree.join("remote.git"),
            &["for-each-ref", "--format=%(refname)", "refs/heads"]
        ),
        ""
    );
    ops.resolve_initialization(&b.worktree, Ok(&b), &git_b, &scheduler, &ctx);
    wait(|| {
        ops.poll(&ctx);
        ops.runs[3].done && !scheduler.has_accepted_work()
    });
    assert_eq!(
        ops.runs[3].status,
        text("ActionCompletedShort"),
        "{}",
        ops.runs[3].output
    );
    assert_eq!(
        command(
            &git_b,
            &b.worktree.join("remote.git"),
            &["rev-parse", "main"]
        ),
        b.head.unwrap()
    );
}

fn queue_after_native_change(change_branch: bool) {
    let (_temp, git, snapshot) = fixture();
    let ctx = egui::Context::default();
    let scheduler = Scheduler::default();
    let mut ops = controller();
    let (release, held) = mpsc::channel();
    let (entered, started) = mpsc::channel();
    let native_git = git.clone();
    let root = snapshot.worktree.clone();
    scheduler.submit(
        snapshot.identity.fingerprint.clone(),
        snapshot.common_directory.to_string_lossy().to_uppercase(),
        "fixture accepted local work".into(),
        Lane::WorkingCopy,
        move || {
            entered.send(()).unwrap();
            held.recv_timeout(Duration::from_secs(10)).unwrap();
            if change_branch {
                command(&native_git, &root, &["checkout", "-b", "replacement"]);
            } else {
                fs::write(root.join("file"), b"next accepted commit\n").unwrap();
                command(&native_git, &root, &["add", "--", "file"]);
                command(
                    &native_git,
                    &root,
                    &["commit", "-m", "accepted before queued push"],
                );
            }
            Outcome::Succeeded
        },
        |_| panic!("accepted command removed"),
    );
    started.recv_timeout(Duration::from_secs(5)).unwrap();
    ops.submit(
        Options::load(&Settings::default(), &snapshot, Kind::Push),
        &snapshot,
        git.clone(),
        &scheduler,
        &ctx,
    );
    assert_eq!(
        scheduler
            .records()
            .iter()
            .filter(|r| !r.running && r.outcome.is_none())
            .count(),
        1
    );
    release.send(()).unwrap();
    wait(|| {
        ops.poll(&ctx);
        ops.runs[0].done && !scheduler.has_accepted_work()
    });
    let remote = snapshot.worktree.join("remote.git");
    if change_branch {
        assert_eq!(ops.runs[0].status, text("ActionFailedShort"));
        assert!(ops.runs[0].output.contains(&text("ActionChanged")));
        assert_eq!(
            command(
                &git,
                &remote,
                &["for-each-ref", "--format=%(refname)", "refs/heads"]
            ),
            ""
        );
    } else {
        assert_eq!(
            ops.runs[0].status,
            text("ActionCompletedShort"),
            "{}",
            ops.runs[0].output
        );
        let current = command(&git, &snapshot.worktree, &["rev-parse", "HEAD"]);
        assert_ne!(Some(&current), snapshot.head.as_ref());
        assert_eq!(command(&git, &remote, &["rev-parse", "main"]), current);
        assert_eq!(
            command(&git, &snapshot.worktree, &["show", "HEAD:file"]),
            "next accepted commit"
        );
        assert_eq!(
            command(&git, &snapshot.worktree, &["diff", "--", "file"]),
            ""
        );
        assert_eq!(
            command(
                &git,
                &snapshot.worktree,
                &["diff", "--cached", "--", "file"]
            ),
            ""
        );
    }
}

#[test]
fn queued_push_uses_the_tip_after_an_accepted_commit() {
    queue_after_native_change(false);
}

#[test]
fn queued_push_rejects_changed_branch_instead_of_retargeting() {
    queue_after_native_change(true);
}

fn click_label(ctx: &egui::Context, app: &mut crate::Workspace, label: &str) {
    let mut position = None;
    for _ in 0..3 {
        let output = frame(ctx, app, vec![]);
        if let Some(update) = &output.platform_output.accesskit_update {
            for (_, node) in &update.nodes {
                if node.label() == Some(label)
                    && !node.is_disabled()
                    && let Some(r) = node.bounds()
                {
                    position = Some(egui::pos2(
                        ((r.x0 + r.x1) / 2.) as f32,
                        ((r.y0 + r.y1) / 2.) as f32,
                    ));
                }
            }
        }
        output.drop_without_applying_deltas();
    }
    let pos = position.unwrap_or_else(|| panic!("missing enabled control: {label}"));
    for events in [
        vec![egui::Event::PointerMoved(pos)],
        vec![egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: Default::default(),
        }],
        vec![egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: Default::default(),
        }],
    ] {
        frame(ctx, app, events).drop_without_applying_deltas();
    }
}

#[test]
fn initializing_fetch_options_survive_closure_save_without_fetch_and_hold_queue_order() {
    let (_temp, git, snapshot) = fixture();
    // FETCH_HEAD does not exist until a real fetch runs.
    let before_config = fs::read(snapshot.git_directory.join("config")).unwrap();
    let mut app = workspace();
    app.tabs.clear();
    app.selected = 0;
    app.git = Some(git.clone());
    app.operations = controller();
    app.settings
        .set("AutoFetchEnabled", serde_json::json!(false));
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    app.open(snapshot.worktree.clone(), &ctx);
    let owner = app.tabs[0].id;
    for _ in 0..2 {
        click_label(&ctx, &mut app, &text("RustFetchOptions"));
    }
    assert_eq!(app.operations.pending.len(), 1);
    assert_eq!(app.operations.runs.len(), 1);
    assert!(app.operations.pending[0].options_only);
    assert!(app.operations.options.is_none());
    app.close_tab(owner, &ctx);
    wait(|| {
        app.receive(&ctx);
        !app.operations.ready_options.is_empty()
    });
    assert!(app.tabs.is_empty());
    assert_eq!(app.retained_tabs[0].id, owner);
    assert!(app.scheduler.has_accepted_work());
    click_label(&ctx, &mut app, &text("ActionAllRemotes"));
    assert!(app.operations.options.as_ref().unwrap().0.all_remotes);
    assert_eq!(
        app.operations.options.as_ref().unwrap().1.worktree,
        snapshot.worktree
    );
    assert!(!snapshot.git_directory.join("FETCH_HEAD").exists());
    assert_eq!(
        fs::read(snapshot.git_directory.join("config")).unwrap(),
        before_config
    );

    // An accepted later fetch cannot overtake this options editor.
    app.operations.submit(
        Options::load(&app.settings, &snapshot, Kind::Fetch),
        &snapshot,
        git.clone(),
        &app.scheduler,
        &ctx,
    );
    let fetch = app.operations.runs.last().unwrap().request;
    assert!(
        app.scheduler
            .records()
            .iter()
            .any(|r| r.id == fetch && !r.running && r.outcome.is_none())
    );
    click_label(&ctx, &mut app, &text("Close"));
    wait(|| {
        app.receive(&ctx);
        !app.scheduler.has_accepted_work()
    });
    app.receive(&ctx);
    assert!(Options::load(&app.settings, &snapshot, Kind::Fetch).all_remotes);
    assert_eq!(app.operations.runs[0].status, text("RustOptionsClosed"));
    assert_eq!(
        app.operations.runs[1].status,
        text("ActionCompletedShort"),
        "{}",
        app.operations.runs[1].output
    );
    assert!(snapshot.git_directory.join("FETCH_HEAD").exists());
}

#[test]
fn initializing_fetch_options_removal_failure_and_multiple_owners_are_scoped() {
    let (_a, git_a, a) = fixture();
    let (_b, git_b, b) = fixture();
    let scheduler = Scheduler::default();
    let ctx = egui::Context::default();
    let settings = Settings::default();
    let mut ops = controller();
    ops.defer_fetch_options(&a.worktree, &settings, &scheduler, &ctx);
    let removed = ops.pending[0].request;
    assert!(scheduler.remove_waiting(removed));
    // Resolve before consuming the removal event: activation must still fail.
    ops.resolve_initialization(&a.worktree, Ok(&a), &git_a, &scheduler, &ctx);
    ops.poll(&ctx);
    assert!(ops.ready_options.is_empty());
    ops.defer_fetch_options(&a.worktree, &settings, &scheduler, &ctx);
    ops.defer_fetch_options(&b.worktree, &settings, &scheduler, &ctx);
    ops.resolve_initialization(&a.worktree, Err("fixture failed"), &git_a, &scheduler, &ctx);
    ops.resolve_initialization(&b.worktree, Ok(&b), &git_b, &scheduler, &ctx);
    wait(|| {
        ops.poll(&ctx);
        ops.ready_options.len() == 1
    });
    assert_eq!(ops.runs[0].status, text("QueuedRequestCancelled"));
    assert_eq!(ops.runs[1].status, text("ActionFailedShort"));
    assert_eq!(ops.runs[1].output, "fixture failed");
    assert_eq!(ops.ready_options[0].snapshot.worktree, b.worktree);
    // A configured remote can legally be named "options"; its action must
    // remain distinct from the options editor's scheduler key.
    let before = ops.runs.len();
    ops.fetch_remote("options", &b, git_b.clone(), &scheduler, &ctx);
    assert_eq!(ops.runs.len(), before + 1);
    assert!(scheduler.remove_waiting(ops.runs.last().unwrap().request));
    ops.poll(&ctx);

    // Independent completions queue their dialogs rather than overwriting one another.
    ops.defer_fetch_options(&a.worktree, &settings, &scheduler, &ctx);
    ops.resolve_initialization(&a.worktree, Ok(&a), &git_a, &scheduler, &ctx);
    wait(|| {
        ops.poll(&ctx);
        ops.ready_options.len() == 2
    });
    assert_eq!(ops.ready_options[0].snapshot.worktree, b.worktree);
    assert_eq!(ops.ready_options[1].snapshot.worktree, a.worktree);
    ops.ready_options.clear(); // Equivalent to both dialogs dismissing their ownership.
    wait(|| {
        ops.poll(&ctx);
        !scheduler.has_accepted_work()
    });
    assert!(!a.git_directory.join("FETCH_HEAD").exists());
    assert!(!b.git_directory.join("FETCH_HEAD").exists());
    assert!(ops.runs.iter().all(|r| r.done));
}
