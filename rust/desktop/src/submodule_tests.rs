use super::*;
use crate::workspace_interaction_tests::{frame, sidebar_click};

fn submodule_fixture(
    ctx: &egui::Context,
) -> (
    crate::history_view::tests::Fixture,
    Workspace,
    PathBuf,
    String,
) {
    use std::fs;
    let f = crate::history_view::tests::Fixture::new();
    let source = f._temp.path().join("child-source");
    fs::create_dir(&source).unwrap();
    let source_arg = source.to_str().unwrap();
    f.command(&["-C", source_arg, "init", "--initial-branch=main"], &[]);
    fs::write(source.join("file"), "one\n").unwrap();
    f.command(&["-C", source_arg, "add", "file"], &[]);
    f.command(&["-C", source_arg, "commit", "-m", "one"], &[]);
    f.command(
        &[
            "-c",
            "protocol.file.allow=always",
            "submodule",
            "add",
            "--",
            source_arg,
            "modules/child",
        ],
        &[],
    );
    f.command(&["commit", "-am", "child"], &[]);
    fs::write(source.join("file"), "two\n").unwrap();
    f.command(&["-C", source_arg, "commit", "-am", "two"], &[]);
    let tip = f.command(&["-C", source_arg, "rev-parse", "HEAD"], &[]);
    let mut app = f.app(ctx);
    app.operations.set_authentication_helper(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/debug/ShovelAskPass.exe"),
    );
    settle_submodule(&mut app, ctx);
    let child = f.root.join("modules/child");
    (f, app, child, tip)
}
fn settle_submodule(app: &mut Workspace, ctx: &egui::Context) {
    use std::time::{Duration, Instant};
    let end = Instant::now() + Duration::from_secs(25);
    loop {
        app.receive(ctx);
        if !app.scheduler.has_accepted_work()
            && app
                .tabs
                .iter()
                .chain(app.retained_tabs.iter())
                .all(|t| !t.loading && !t.sidebar.pending() && !app.operations.has_pending(&t.path))
        {
            break;
        }
        assert!(Instant::now() < end, "submodule work did not settle");
        thread::sleep(Duration::from_millis(5));
    }
}
#[test]
fn sidebar_submodule_open_update_and_sync_use_actual_child_and_reconcile_both_tabs() {
    use std::{
        fs,
        time::{Duration, Instant},
    };
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    ctx.set_embed_viewports(true);
    let (f, mut app, child, tip) = submodule_fixture(&ctx);
    let parent_head = f.command(&["rev-parse", "HEAD"], &[]);
    f.command(
        &["submodule", "deinit", "--force", "--", "modules/child"],
        &[],
    );
    sidebar_click(
        &ctx,
        &mut app,
        "modules/child",
        egui::PointerButton::Primary,
    );
    let end = Instant::now() + Duration::from_secs(5);
    while app.notice.is_none() {
        app.receive(&ctx);
        assert!(Instant::now() < end);
        thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(
        app.notice.as_deref(),
        Some(strings::text("SidebarSubmoduleUnavailable").as_str())
    );
    assert_eq!(app.tabs.len(), 1);
    let config = fs::read(f.root.join(".git/config")).unwrap();
    sidebar_click(
        &ctx,
        &mut app,
        "modules/child",
        egui::PointerButton::Secondary,
    );
    assert_eq!(fs::read(f.root.join(".git/config")).unwrap(), config);
    assert!(
        !app.scheduler.has_accepted_work(),
        "opening a menu cannot submit Git"
    );
    sidebar_click(
        &ctx,
        &mut app,
        &strings::text("ActionTitleSubmoduleUpdate"),
        egui::PointerButton::Primary,
    );
    settle_submodule(&mut app, &ctx);
    assert!(child.join(".git").exists());
    assert!(
        app.scheduler
            .records()
            .iter()
            .any(|r| r.outcome == Some(shovel_core::scheduler::Outcome::Succeeded))
    );
    sidebar_click(
        &ctx,
        &mut app,
        "modules/child",
        egui::PointerButton::Secondary,
    );
    sidebar_click(
        &ctx,
        &mut app,
        &strings::text("SidebarOpenSubmodule"),
        egui::PointerButton::Primary,
    );
    let end = Instant::now() + Duration::from_secs(10);
    while app.tabs.len() != 2 || app.tabs[app.selected].history.is_none() {
        app.receive(&ctx);
        assert!(Instant::now() < end);
        thread::sleep(Duration::from_millis(5));
    }
    assert!(Workspace::same_path(&app.tabs[app.selected].path, &child));
    assert_eq!(app.tabs[app.selected].name(), "history :: child");
    let direct_ctx = egui::Context::default();
    let mut direct = crate::workspace_interaction_tests::workspace();
    direct.tabs.clear();
    direct.git = Some(Arc::new(f.git.clone()));
    direct.open(child.clone(), &direct_ctx);
    settle_submodule(&mut direct, &direct_ctx);
    assert_eq!(direct.tabs[0].name(), "history :: child");
    let child_owner = app.tabs[app.selected].id;
    app.select_tab(0, &ctx);
    f.command(
        &[
            "update-index",
            "--cacheinfo",
            &format!("160000,{tip},modules/child"),
        ],
        &[],
    );
    app.refresh(&ctx);
    settle_submodule(&mut app, &ctx);
    sidebar_click(
        &ctx,
        &mut app,
        "modules/child",
        egui::PointerButton::Secondary,
    );
    sidebar_click(
        &ctx,
        &mut app,
        &strings::text("ActionTitleSubmoduleUpdate"),
        egui::PointerButton::Primary,
    );
    settle_submodule(&mut app, &ctx);
    assert_eq!(
        f.command(&["-C", child.to_str().unwrap(), "rev-parse", "HEAD"], &[]),
        tip
    );
    assert_eq!(f.command(&["rev-parse", "HEAD"], &[]), parent_head);
    let child_index = app.tabs.iter().position(|t| t.id == child_owner).unwrap();
    assert!(app.tabs[child_index].view.stale);
    app.select_tab(child_index, &ctx);
    settle_submodule(&mut app, &ctx);
    assert_eq!(
        app.tabs[child_index]
            .history
            .as_ref()
            .unwrap()
            .snapshot
            .head
            .as_deref(),
        Some(tip.as_str())
    );
    app.select_tab(0, &ctx);
    f.command(
        &[
            "config",
            "--file",
            ".gitmodules",
            "submodule.modules/child.url",
            "../new-child",
        ],
        &[],
    );
    sidebar_click(
        &ctx,
        &mut app,
        "modules/child",
        egui::PointerButton::Secondary,
    );
    sidebar_click(
        &ctx,
        &mut app,
        &strings::text("ActionTitleSubmoduleSync"),
        egui::PointerButton::Primary,
    );
    settle_submodule(&mut app, &ctx);
    assert!(
        f.command(
            &["-C", child.to_str().unwrap(), "remote", "get-url", "origin"],
            &[]
        )
        .replace('\\', "/")
        .ends_with("/new-child")
    );
    assert!(app.tabs[child_index].view.stale);
    assert_eq!(app.selected, 0);
}
#[test]
fn sidebar_submodule_accepted_update_survives_all_tabs_closing_and_retains_failure() {
    use std::{
        fs,
        time::{Duration, Instant},
    };
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    ctx.set_embed_viewports(true);
    let (f, mut app, child, tip) = submodule_fixture(&ctx);
    f.command(
        &[
            "update-index",
            "--cacheinfo",
            &format!("160000,{tip},modules/child"),
        ],
        &[],
    );
    app.refresh(&ctx);
    settle_submodule(&mut app, &ctx);
    let identity = shovel_core::identity::read(&child).unwrap();
    fs::write(identity.git.join("hooks/post-checkout"),
        "#!/bin/sh\ntouch hook-started\nwhile test ! -f hook-release; do sleep .05; done\ntouch hook-finished\n").unwrap();
    struct Release(PathBuf);
    impl Drop for Release {
        fn drop(&mut self) {
            let _ = fs::write(&self.0, "");
        }
    }
    let release = Release(child.join("hook-release"));
    sidebar_click(
        &ctx,
        &mut app,
        "modules/child",
        egui::PointerButton::Secondary,
    );
    sidebar_click(
        &ctx,
        &mut app,
        &strings::text("ActionTitleSubmoduleUpdate"),
        egui::PointerButton::Primary,
    );
    let end = Instant::now() + Duration::from_secs(20);
    while !child.join("hook-started").exists() {
        app.receive(&ctx);
        assert!(Instant::now() < end);
        thread::sleep(Duration::from_millis(5));
    }
    let owner = app.tabs[0].id;
    app.close_tab(owner, &ctx);
    assert!(app.tabs.is_empty() && app.scheduler.has_accepted_work());
    app.close(&ctx);
    assert!(
        !app.exit_ready && !app.closing,
        "accepted work must block exit"
    );
    drop(release);
    settle_submodule(&mut app, &ctx);
    assert!(child.join("hook-finished").exists());
    app.open(f.root.clone(), &ctx);
    settle_submodule(&mut app, &ctx);
    assert_eq!(
        f.command(&["-C", child.to_str().unwrap(), "rev-parse", "HEAD"], &[]),
        tip
    );
    // A later dirty-child failure is retained through the same menu workflow.
    let first = f.command(&["-C", child.to_str().unwrap(), "rev-parse", "HEAD~"], &[]);
    f.command(
        &[
            "update-index",
            "--cacheinfo",
            &format!("160000,{first},modules/child"),
        ],
        &[],
    );
    fs::write(child.join("file"), "local child\n").unwrap();
    app.refresh(&ctx);
    settle_submodule(&mut app, &ctx);
    sidebar_click(
        &ctx,
        &mut app,
        "modules/child",
        egui::PointerButton::Secondary,
    );
    sidebar_click(
        &ctx,
        &mut app,
        &strings::text("ActionTitleSubmoduleUpdate"),
        egui::PointerButton::Primary,
    );
    settle_submodule(&mut app, &ctx);
    assert_eq!(
        app.scheduler.records().last().unwrap().outcome,
        Some(shovel_core::scheduler::Outcome::Failed)
    );
    assert_eq!(
        fs::read_to_string(child.join("file")).unwrap(),
        "local child\n"
    );
    // Activity shows the selected command's output directly in its right pane.
    app.receive(&ctx);
    app.activity_open = true;
    app.activity_all = true;
    frame(&ctx, &mut app, vec![]).drop_without_applying_deltas();
    let output = frame(&ctx, &mut app, vec![]);
    let visible = |needle: &str| {
        output.shapes.iter().any(|shape| {
            matches!(&shape.shape, egui::Shape::Text(text) if text.galley.text().contains(needle))
        })
    };
    assert!(visible(&strings::text("ActivityTitle")));
    assert!(visible("overwritten") && visible("file"), "Activity did not expose the native dirty-file failure");
    output.drop_without_applying_deltas();
}
