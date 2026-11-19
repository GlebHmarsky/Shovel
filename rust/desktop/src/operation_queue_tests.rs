use super::tests::{command, fixture, wait};
use super::*;
use crate::workspace_interaction_tests::{frame, workspace};
use std::fs;

fn controller() -> Controller {
    let helper =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/debug/ShovelAskPass.exe");
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
fn interrupted_initialization_finishes_queued_push_after_tab_closure_and_allows_reopen() {
    let (_temp, git, snapshot) = fixture();
    let ctx = egui::Context::default();
    let (started, fail) = crate::history_cache::tests::hold_failed_read(snapshot.worktree.clone());
    let mut app = workspace();
    app.tabs.clear();
    app.selected = 0;
    app.git = Some(git.clone());
    app.operations = controller();
    app.settings.set("AutoFetchEnabled", serde_json::json!(false));
    app.open(snapshot.worktree.clone(), &ctx);
    started.recv_timeout(std::time::Duration::from_secs(5)).unwrap();
    let owner = app.tabs[0].id;
    app.operations.defer_network(Kind::Push, &snapshot.worktree, &app.settings, &app.scheduler, &ctx);
    let request = app.operations.pending[0].request;
    app.close_tab(owner, &ctx);
    assert!(app.tabs.is_empty() && app.scheduler.has_accepted_work());
    fail.send(()).unwrap();
    wait(|| {
        app.receive(&ctx);
        !app.scheduler.has_accepted_work() && app.operations.runs[0].done
    });
    let record = app.scheduler.records().into_iter().find(|record| record.id == request).unwrap();
    assert_eq!(record.outcome, Some(Outcome::Failed));
    assert_eq!(app.operations.runs[0].output, text("RustHistoryWorkerFailed"));
    let retained = app.retained_tabs.iter().find(|tab| tab.id == owner).unwrap();
    assert!(!retained.loading && retained.history.is_none());
    assert!(command(&git, &snapshot.worktree.join("remote.git"), &["for-each-ref", "refs/heads"]).is_empty());

    app.open(snapshot.worktree.clone(), &ctx);
    wait(|| {
        app.receive(&ctx);
        !app.tabs[0].loading
    });
    assert_eq!(app.tabs[0].id, owner);
    assert!(app.tabs[0].history.is_some());
    assert!(command(&git, &snapshot.worktree.join("remote.git"), &["for-each-ref", "refs/heads"]).is_empty());
}

#[test]
fn lost_worker_result_finishes_its_activity_and_requests_a_fresh_read() {
    let (_temp, git, snapshot) = fixture();
    let ctx = egui::Context::default();
    let scheduler = Scheduler::default();
    let mut ops = controller();
    let common = shovel_core::case::path_key(&snapshot.common_directory);
    let (request, _) = ops
        .reserve_run(
            RequestKind::Network(Kind::Push),
            &snapshot.worktree,
            (&snapshot.identity.fingerprint, &common),
            "fixture failing worker".into(),
            &scheduler,
            &ctx,
        )
        .unwrap();
    assert!(scheduler.activate(request, || panic!("fixture worker unwind")));
    let mut refreshed = Vec::new();
    wait(|| {
        refreshed.extend(ops.poll(&ctx));
        ops.runs[0].done && !scheduler.has_accepted_work()
    });
    assert_eq!(refreshed.as_slice(), std::slice::from_ref(&snapshot.worktree));
    assert_eq!(ops.failure_path(), Some(snapshot.worktree.as_path()));
    assert_eq!(ops.runs[0].status, text("ActionWorkerFailed"));
    assert_eq!(
        command(&git, &snapshot.worktree, &["rev-parse", "HEAD"]),
        snapshot.head.as_deref().unwrap()
    );
}

#[test]
fn explicit_push_targets_keep_distinct_requests_and_coalesce_the_matching_initialization() {
    let (_temp, git, snapshot) = fixture();
    let ctx = egui::Context::default();
    let scheduler = Scheduler::default();
    let mut ops = controller();
    let mut settings = Settings::default();
    let mut first = Options::load(&settings, &snapshot, Kind::Push);
    first.explicit = true;
    first.branch = "review/alpha".into();
    first.save(&mut settings, &snapshot, false);
    let mut second = first.clone();
    second.branch = "review/beta".into();
    let (hold, _) = scheduler.reserve(
        "hold".into(),
        shovel_core::case::path_key(&snapshot.common_directory),
        "hold".into(),
        Lane::Exclusive,
        |_| {},
    );
    ops.defer_network(Kind::Push, &snapshot.worktree, &settings, &scheduler, &ctx);
    let pending = ops.pending[0].request;
    let duplicate = ops
        .submit(first.clone(), &snapshot, git.clone(), &scheduler, &ctx)
        .unwrap();
    let distinct = ops
        .submit(second, &snapshot, git.clone(), &scheduler, &ctx)
        .expect("another target must get its own request");
    ops.resolve_initialization(&snapshot.worktree, Ok(&snapshot), &git, &scheduler, &ctx);
    assert!(
        ops.submit(first, &snapshot, git.clone(), &scheduler, &ctx)
            .is_none()
    );
    assert!(scheduler.remove_waiting(hold));
    wait(|| {
        ops.poll(&ctx);
        ops.runs.iter().all(|r| r.done) && !scheduler.has_accepted_work()
    });
    let records = scheduler.records();
    let outcome = |id| records.iter().find(|r| r.id == id).unwrap().outcome;
    assert_eq!(outcome(pending), Some(Outcome::Succeeded));
    assert_eq!(outcome(duplicate), Some(Outcome::Coalesced));
    assert_eq!(outcome(distinct), Some(Outcome::Succeeded));
    assert!(
        ops.runs
            .iter()
            .any(|r| r.status == text("RustRequestCoalesced"))
    );
    for branch in ["review/alpha", "review/beta"] {
        assert_eq!(
            command(
                &git,
                &snapshot.worktree.join("remote.git"),
                &["rev-parse", branch]
            ),
            snapshot.head.as_deref().unwrap()
        );
    }
}

#[test]
fn pending_push_bound_by_local_changes_rejects_a_different_history_owner() {
    let (_temp, git, first) = fixture();
    let (_other_temp, _, other) = fixture();
    let ctx = egui::Context::default();
    let scheduler = Scheduler::default();
    let mut ops = controller();
    ops.defer_network(
        Kind::Push,
        &first.worktree,
        &Settings::default(),
        &scheduler,
        &ctx,
    );
    ops.bind_initialization(&first.worktree, &first, &scheduler);
    ops.resolve_initialization(&first.worktree, Ok(&other), &git, &scheduler, &ctx);
    wait(|| {
        ops.poll(&ctx);
        ops.runs[0].done && !scheduler.has_accepted_work()
    });
    assert_eq!(scheduler.records()[0].outcome, Some(Outcome::Failed));
    assert_eq!(ops.runs[0].output, text("ActionChanged"));
    for snapshot in [first, other] {
        assert!(
            command(
                &git,
                &snapshot.worktree.join("remote.git"),
                &["for-each-ref", "refs/heads"]
            )
            .is_empty()
        );
    }
}

#[test]
fn network_options_display_the_configured_destination_with_a_custom_tracking_ref() {
    let (_temp, git, first) = fixture();
    command(
        &git,
        &first.worktree,
        &[
            "remote",
            "add",
            "mirror",
            first.worktree.join("remote.git").to_str().unwrap(),
        ],
    );
    command(
        &git,
        &first.worktree,
        &[
            "config",
            "remote.mirror.fetch",
            "+refs/heads/*:refs/tracking/mirror/*",
        ],
    );
    command(
        &git,
        &first.worktree,
        &["config", "branch.main.remote", "mirror"],
    );
    command(
        &git,
        &first.worktree,
        &["config", "branch.main.merge", "refs/heads/integration"],
    );
    let snapshot = git
        .inspect(&first.worktree, &Cancellation::default())
        .unwrap();
    for kind in [Kind::Pull, Kind::Push] {
        let options = Options::load(&Settings::default(), &snapshot, kind);
        assert_eq!(
            (&*options.remote, &*options.branch),
            ("mirror", "integration")
        );
        assert!(!options.explicit);
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
    saved.save(&mut app.settings, &snapshot, true);
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
        shovel_core::case::path_key(&snapshot.common_directory),
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
                    && matches!(
                        node.role(),
                        egui::Role::Button | egui::Role::CheckBox | egui::Role::RadioButton
                    )
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
fn toolbar_network_actions_confirm_once_per_worktree_and_use_saved_edits() {
    let (temp, git, snapshot) = fixture();
    let (_other, _, other) = fixture();
    let path = &snapshot.worktree;
    let spare = path.join("spare.git");
    command(&git, path, &["init", "--bare", spare.to_str().unwrap()]);
    command(
        &git,
        path,
        &["remote", "add", "spare", spare.to_str().unwrap()],
    );
    for remote in ["origin", "spare"] {
        command(&git, path, &["push", remote, "HEAD:refs/heads/review"]);
    }
    let clear_tracking_refs = |git: &Git| {
        for reference in ["refs/remotes/origin/review", "refs/remotes/spare/review"] {
            command(git, path, &["update-ref", "-d", reference]);
        }
    };
    let tracking_refs = |git: &Git| {
        command(
            git,
            path,
            &["for-each-ref", "--format=%(refname)", "refs/remotes"],
        )
    };
    clear_tracking_refs(&git);
    let mut app = workspace();
    app.tabs.clear();
    app.git = Some(git.clone());
    app.operations = controller();
    app.settings
        .set("AutoFetchEnabled", serde_json::json!(false));
    app.settings.set(
        "NetworkActions",
        serde_json::json!([
            { "Path": path, "Action": Kind::Fetch.number(), "AllRemotes": true }
        ]),
    );
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    app.open(path.clone(), &ctx);
    crate::history_view::tests::wait(&mut app, &ctx);

    click_label(&ctx, &mut app, &text("Fetch"));
    assert!(app.operations.options.as_ref().unwrap().0.all_remotes);
    click_label(&ctx, &mut app, &text("ActionAllRemotes"));
    click_label(&ctx, &mut app, &text("Close"));
    assert!(!Options::confirmed(&app.settings, path, Kind::Fetch));
    assert!(app.operations.runs.is_empty());

    click_label(&ctx, &mut app, &text("Fetch"));
    assert!(!app.operations.options.as_ref().unwrap().0.all_remotes);
    click_label(&ctx, &mut app, &text("ActionAllRemotes"));
    click_label(&ctx, &mut app, &text("Fetch"));
    assert!(Options::confirmed(&app.settings, path, Kind::Fetch));
    wait(|| {
        app.receive(&ctx);
        app.operations.runs[0].done
    });
    assert_eq!(
        tracking_refs(&git),
        "refs/remotes/origin/review\nrefs/remotes/spare/review",
        "{}",
        app.operations.runs[0].output
    );
    clear_tracking_refs(&git);

    let profile = temp.path().join("profile");
    let mut store = shovel_core::settings::SettingsStore::new(profile.clone());
    store.load().unwrap();
    store.save(&app.settings).unwrap();
    app.settings = shovel_core::settings::SettingsStore::new(profile)
        .load()
        .unwrap();
    click_label(&ctx, &mut app, &text("RustFetchOptions"));
    assert!(app.operations.options.as_ref().unwrap().0.all_remotes);
    click_label(&ctx, &mut app, &text("ActionAllRemotes"));
    click_label(&ctx, &mut app, &text("Close"));
    click_label(&ctx, &mut app, &text("Fetch"));
    assert!(!app.operations.modal_open());
    wait(|| {
        app.receive(&ctx);
        app.operations.runs[1].done
    });
    assert_eq!(
        tracking_refs(&git),
        "refs/remotes/origin/review",
        "{}",
        app.operations.runs[1].output
    );

    for kind in [Kind::Pull, Kind::Push] {
        click_label(&ctx, &mut app, &text(kind.key()));
        assert_eq!(app.operations.options.as_ref().unwrap().0.kind, kind);
        click_label(&ctx, &mut app, &text("Close"));
    }
    app.open(other.worktree.clone(), &ctx);
    crate::history_view::tests::wait(&mut app, &ctx);
    click_label(&ctx, &mut app, &text("Fetch"));
    assert_eq!(
        app.operations.options.as_ref().unwrap().1.worktree,
        other.worktree
    );
    click_label(&ctx, &mut app, &text("Close"));
    assert_eq!(app.operations.runs.len(), 2);
}

#[test]
fn initializing_main_fetch_opens_options_and_keeps_the_original_worktree() {
    let (_temp, git, snapshot) = fixture();
    let mut app = workspace();
    app.tabs.clear();
    app.git = Some(git);
    app.operations = controller();
    app.settings
        .set("AutoFetchEnabled", serde_json::json!(false));
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    app.open(snapshot.worktree.clone(), &ctx);
    let owner = app.tabs[0].id;
    for _ in 0..2 {
        click_label(&ctx, &mut app, &text("Fetch"));
    }
    assert_eq!(app.operations.pending.len(), 1);
    app.close_tab(owner, &ctx);
    wait(|| {
        app.receive(&ctx);
        !app.operations.ready_options.is_empty()
    });
    frame(&ctx, &mut app, vec![]).drop_without_applying_deltas();
    let (_, captured) = app.operations.options.as_ref().unwrap();
    assert_eq!(captured.worktree, snapshot.worktree);
    click_label(&ctx, &mut app, &text("Close"));
    wait(|| {
        app.receive(&ctx);
        !app.scheduler.has_accepted_work()
    });
    assert!(!snapshot.git_directory.join("FETCH_HEAD").exists());
    assert!(!Options::confirmed(
        &app.settings,
        &snapshot.worktree,
        Kind::Fetch
    ));
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
    assert_eq!(
        app.operations.pending[0].action,
        RequestKind::Options(Kind::Fetch)
    );
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
fn initializing_pull_and_push_options_keep_their_order_and_original_repository() {
    let (_temp, git, snapshot) = fixture();
    let remote = snapshot.worktree.join("remote.git");
    command(&git, &snapshot.worktree, &["tag", "from-options"]);
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    let mut app = workspace();
    app.tabs.clear();
    app.selected = 0;
    app.git = Some(git.clone());
    app.operations = controller();
    app.settings
        .set("AutoFetchEnabled", serde_json::json!(false));
    app.open(snapshot.worktree.clone(), &ctx);
    let owner = app.tabs[0].id;
    for kind in [Kind::Pull, Kind::Push, Kind::Pull, Kind::Push] {
        click_label(
            &ctx,
            &mut app,
            &strings::format("RustNetworkOptions", &[&text(kind.key())]),
        );
    }
    assert_eq!(app.operations.pending.len(), 2);
    app.close_tab(owner, &ctx);
    wait(|| {
        app.receive(&ctx);
        !app.operations.ready_options.is_empty()
    });
    click_label(&ctx, &mut app, &text("ActionAutoStash"));
    assert_eq!(
        app.operations.options.as_ref().unwrap().1.worktree,
        snapshot.worktree
    );
    click_label(&ctx, &mut app, &text("Close"));
    assert!(Options::load(&app.settings, &snapshot, Kind::Pull).auto_stash);
    assert!(!snapshot.git_directory.join("FETCH_HEAD").exists());
    wait(|| {
        app.receive(&ctx);
        !app.operations.ready_options.is_empty()
    });
    assert!(command(&git, &remote, &["for-each-ref"]).is_empty());
    click_label(&ctx, &mut app, &text("ActionAllTags"));
    click_label(&ctx, &mut app, &text("Push"));
    wait(|| {
        app.receive(&ctx);
        !app.scheduler.has_accepted_work()
    });
    assert!(Options::load(&app.settings, &snapshot, Kind::Push).all_tags);
    assert!(Options::confirmed(&app.settings, &snapshot.worktree, Kind::Push));
    assert_eq!(
        command(&git, &remote, &["rev-parse", "main"]),
        snapshot.head.clone().unwrap()
    );
    assert_eq!(
        command(&git, &remote, &["rev-parse", "refs/tags/from-options"]),
        snapshot.head.unwrap()
    );
}

#[test]
fn initializing_fetch_options_removal_failure_and_multiple_owners_are_scoped() {
    let (_a, git_a, a) = fixture();
    let (_b, git_b, b) = fixture();
    let scheduler = Scheduler::default();
    let ctx = egui::Context::default();
    let settings = Settings::default();
    let mut ops = controller();
    ops.defer(
        RequestKind::Options(Kind::Fetch),
        &a.worktree,
        &settings,
        &scheduler,
        &ctx,
    );
    let removed = ops.pending[0].request;
    assert!(scheduler.remove_waiting(removed));
    // Resolve before consuming the removal event: activation must still fail.
    ops.resolve_initialization(&a.worktree, Ok(&a), &git_a, &scheduler, &ctx);
    ops.poll(&ctx);
    assert!(ops.ready_options.is_empty());
    ops.defer(
        RequestKind::Options(Kind::Fetch),
        &a.worktree,
        &settings,
        &scheduler,
        &ctx,
    );
    ops.defer(
        RequestKind::Options(Kind::Fetch),
        &b.worktree,
        &settings,
        &scheduler,
        &ctx,
    );
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
    ops.defer(
        RequestKind::Options(Kind::Fetch),
        &a.worktree,
        &settings,
        &scheduler,
        &ctx,
    );
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

fn enter_branch_name(ctx: &egui::Context, app: &mut crate::Workspace, value: &str) {
    let mut point = None;
    for _ in 0..4 {
        let output = frame(ctx, app, vec![]);
        if let Some(update) = &output.platform_output.accesskit_update {
            for (_, node) in &update.nodes {
                if node.role() == egui::Role::TextInput
                    && !node.is_disabled()
                    && let Some(r) = node.bounds()
                {
                    point = Some(egui::pos2(
                        ((r.x0 + r.x1) / 2.) as f32,
                        ((r.y0 + r.y1) / 2.) as f32,
                    ));
                }
            }
        }
        output.drop_without_applying_deltas();
    }
    let pos = point.expect("branch name input");
    frame(ctx, app, vec![egui::Event::PointerMoved(pos)]).drop_without_applying_deltas();
    for pressed in [true, false] {
        frame(
            ctx,
            app,
            vec![egui::Event::PointerButton {
                pos,
                pressed,
                button: egui::PointerButton::Primary,
                modifiers: Default::default(),
            }],
        )
        .drop_without_applying_deltas();
    }
    frame(ctx, app, vec![egui::Event::Text(value.into())]).drop_without_applying_deltas();
}

#[test]
fn initializing_new_branch_keeps_owner_and_submission_order_through_checkout_review() {
    let (_temp, git, snapshot) = fixture();
    let index = command(&git, &snapshot.worktree, &["ls-files", "--stage", "-z"]);
    let mut app = workspace();
    app.tabs.clear();
    app.selected = 0;
    app.git = Some(git.clone());
    app.operations = controller();
    app.settings
        .set("AutoFetchEnabled", serde_json::json!(false));
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    ctx.set_embed_viewports(true);
    app.open(snapshot.worktree.clone(), &ctx);
    let owner = app.tabs[0].id;
    for _ in 0..2 {
        click_label(&ctx, &mut app, &text("BranchToolbar"));
    }
    assert_eq!(app.operations.pending.len(), 1);
    assert_eq!(app.operations.pending[0].action, RequestKind::NewBranch);
    // Submitted after the options request but before initialization resolves.
    app.operations.defer_network(
        Kind::Push,
        &snapshot.worktree,
        &app.settings,
        &app.scheduler,
        &ctx,
    );
    app.close_tab(owner, &ctx);
    wait(|| {
        app.receive(&ctx);
        !app.operations.ready_options.is_empty()
    });
    frame(&ctx, &mut app, vec![]).drop_without_applying_deltas();
    assert!(app.branches.is_open());
    assert!(app.tabs.is_empty());
    assert_eq!(
        command(&git, &snapshot.worktree, &["branch", "--show-current"]),
        "main"
    );
    assert_eq!(
        command(
            &git,
            &snapshot.worktree,
            &["for-each-ref", "--format=%(refname)", "refs/heads"]
        ),
        "refs/heads/main"
    );
    enter_branch_name(&ctx, &mut app, "queued-branch");
    let (review_started, interrupt) =
        crate::history_cache::tests::hold_failed_read(snapshot.worktree.clone());
    click_label(&ctx, &mut app, &text("ActionTitleNewBranch"));
    wait(|| {
        frame(&ctx, &mut app, vec![]).drop_without_applying_deltas();
        review_started.try_recv().is_ok()
    });
    interrupt.send(()).unwrap();
    wait(|| {
        let output = frame(&ctx, &mut app, vec![]);
        let retry = output
            .platform_output
            .accesskit_update
            .as_ref()
            .is_some_and(|update| {
                update.nodes.iter().any(|(_, node)| {
                    node.role() == egui::Role::Button
                        && node.label() == Some(text("CheckoutRetry").as_str())
                        && !node.is_disabled()
                })
            });
        output.drop_without_applying_deltas();
        retry
    });
    let active = app.scheduler.active_records();
    let push = active
        .iter()
        .find(|record| record.key.starts_with("network:Push"))
        .expect("later Push remains queued");
    assert!(!push.running);
    click_label(&ctx, &mut app, &text("CheckoutRetry"));
    // Creating with checkout first opens real clean/dirty review.
    wait(|| {
        app.receive(&ctx);
        let output = frame(&ctx, &mut app, vec![]);
        let ready = output
            .platform_output
            .accesskit_update
            .as_ref()
            .is_some_and(|u| {
                u.nodes.iter().any(|(_, n)| {
                    n.role() == egui::Role::Button
                        && n.label() == Some(text("ActionTitleSwitchBranch").as_str())
                        && !n.is_disabled()
                })
            });
        output.drop_without_applying_deltas();
        ready
    });
    assert_eq!(
        command(&git, &snapshot.worktree, &["branch", "--show-current"]),
        "main"
    );
    click_label(&ctx, &mut app, &text("ActionTitleSwitchBranch"));
    assert!(!app.branches.is_open());
    wait(|| {
        app.receive(&ctx);
        !app.scheduler.has_accepted_work()
    });
    app.receive(&ctx);
    assert_eq!(
        command(&git, &snapshot.worktree, &["branch", "--show-current"]),
        "queued-branch"
    );
    assert_eq!(
        command(&git, &snapshot.worktree, &["rev-parse", "HEAD"]),
        snapshot.head.unwrap()
    );
    assert_eq!(
        command(&git, &snapshot.worktree, &["ls-files", "--stage", "-z"]),
        index
    );
    // Branch command inherited the dialog's earlier place. The later Push
    // sees branch drift and refuses; it must never have published main first.
    assert_eq!(app.operations.runs[1].status, text("ActionFailedShort"));
    assert!(
        app.operations.runs[1]
            .output
            .contains(&text("ActionChanged"))
    );
    assert_eq!(
        app.operations.runs[2].status,
        text("ActionCompletedShort"),
        "{}",
        app.operations.runs[2].output
    );
    assert_eq!(
        command(
            &git,
            &snapshot.worktree.join("remote.git"),
            &["for-each-ref", "--format=%(refname)", "refs/heads"]
        ),
        ""
    );
}

#[test]
fn failed_branch_preparation_keeps_earlier_options_and_stops_later_work() {
    let (temp, git, snapshot) = fixture();
    let linked = temp.path().join("linked");
    command(
        &git,
        &snapshot.worktree,
        &["worktree", "add", "--detach", linked.to_str().unwrap()],
    );
    let linked = git.inspect(&linked, &Cancellation::default()).unwrap();
    assert_eq!(snapshot.common_directory, linked.common_directory);
    fs::write(
        snapshot.git_directory.join("MERGE_HEAD"),
        format!("{}\n", snapshot.head.as_deref().unwrap()),
    )
    .unwrap();
    let snapshot = git
        .inspect(&snapshot.worktree, &Cancellation::default())
        .unwrap();
    assert!(!snapshot.operations.is_empty());
    let mut ops = controller();
    let scheduler = Scheduler::default();
    let ctx = egui::Context::default();
    let settings = Settings::default();
    let (hold, _) = scheduler.reserve(
        linked.identity.fingerprint,
        shovel_core::case::path_key(&linked.common_directory),
        "hold".into(),
        Lane::Exclusive,
        |_| {},
    );
    for action in [
        RequestKind::Options(Kind::Fetch),
        RequestKind::NewBranch,
        RequestKind::Options(Kind::Push),
    ] {
        ops.defer(action, &snapshot.worktree, &settings, &scheduler, &ctx);
    }
    ops.resolve_initialization(&snapshot.worktree, Ok(&snapshot), &git, &scheduler, &ctx);
    ops.poll(&ctx);
    assert!(ops.runs.iter().all(|run| !run.done));
    assert!(ops.ready_options.is_empty());
    assert!(scheduler.remove_waiting(hold));

    wait(|| {
        ops.poll(&ctx);
        !ops.ready_options.is_empty()
    });
    let first = ops.ready_options.pop_front().unwrap();
    assert_eq!(first.options.as_ref().unwrap().kind, Kind::Fetch);
    assert!(ops.runs.iter().all(|run| !run.done));
    drop(first);
    wait(|| {
        ops.poll(&ctx);
        ops.runs.iter().all(|run| run.done) && !scheduler.has_accepted_work()
    });
    assert!(ops.ready_options.is_empty());
    assert_eq!(ops.runs[0].status, text("RustOptionsClosed"));
    assert_eq!(ops.runs[1].status, text("ActionFailedShort"));
    assert_eq!(ops.runs[1].output, text("ActionSequence"));
    assert_eq!(ops.runs[2].status, text("ActionStoppedShort"));
}

#[test]
fn initializing_new_branch_removal_sequence_failure_and_cancel_do_not_mutate() {
    let (_temp, git, snapshot) = fixture();
    let mut ops = controller();
    let scheduler = Scheduler::default();
    let ctx = egui::Context::default();
    let settings = Settings::default();
    ops.defer_new_branch(&snapshot.worktree, &settings, &scheduler, &ctx);
    assert!(scheduler.remove_waiting(ops.pending[0].request));
    ops.resolve_initialization(&snapshot.worktree, Ok(&snapshot), &git, &scheduler, &ctx);
    ops.poll(&ctx);
    assert!(ops.ready_options.is_empty());
    ops.defer_new_branch(&snapshot.worktree, &settings, &scheduler, &ctx);
    let mut sequence = snapshot.clone();
    sequence.operations.push("merge".into());
    ops.resolve_initialization(&snapshot.worktree, Ok(&sequence), &git, &scheduler, &ctx);
    wait(|| {
        ops.poll(&ctx);
        ops.runs[1].done && !scheduler.has_accepted_work()
    });
    assert_eq!(ops.runs[1].status, text("ActionFailedShort"));
    assert_eq!(ops.runs[1].output, text("ActionSequence"));
    assert!(ops.ready_options.is_empty());

    let mut app = workspace();
    app.tabs.clear();
    app.git = Some(git.clone());
    app.operations = ops;
    app.scheduler = scheduler;
    ctx.enable_accesskit();
    ctx.set_embed_viewports(true);
    app.operations
        .defer_new_branch(&snapshot.worktree, &settings, &app.scheduler, &ctx);
    app.operations.resolve_initialization(
        &snapshot.worktree,
        Ok(&snapshot),
        &git,
        &app.scheduler,
        &ctx,
    );
    wait(|| {
        app.operations.poll(&ctx);
        !app.operations.ready_options.is_empty()
    });
    // A ready dialog must not disable an already open remote editor.
    app.remotes.add(&snapshot);
    click_label(&ctx, &mut app, &text("ActionCancel"));
    assert!(!app.remotes.is_open());
    // Branch now opens. An independent ready Fetch must not disable it either.
    frame(&ctx, &mut app, vec![]).drop_without_applying_deltas();
    assert!(app.branches.is_open());
    let (_other_temp, other_git, other) = fixture();
    app.operations.defer(
        RequestKind::Options(Kind::Fetch),
        &other.worktree,
        &settings,
        &app.scheduler,
        &ctx,
    );
    app.operations.resolve_initialization(
        &other.worktree,
        Ok(&other),
        &other_git,
        &app.scheduler,
        &ctx,
    );
    wait(|| {
        app.operations.poll(&ctx);
        !app.operations.ready_options.is_empty()
    });
    click_label(&ctx, &mut app, &text("ActionCancel"));
    assert!(!app.branches.is_open());
    click_label(&ctx, &mut app, &text("Close"));
    wait(|| {
        app.operations.poll(&ctx);
        !app.scheduler.has_accepted_work()
    });
    assert!(!app.branches.is_open());
    assert_eq!(
        command(
            &git,
            &snapshot.worktree,
            &["for-each-ref", "--format=%(refname)", "refs/heads"]
        ),
        "refs/heads/main"
    );
}
