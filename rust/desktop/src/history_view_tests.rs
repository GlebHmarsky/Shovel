use super::*;
use crate::workspace_interaction_tests::{frame, workspace};
use shovel_core::process::{self, Lifetime};
use std::{
    fs,
    time::{Duration, Instant},
};

pub(crate) struct Fixture {
    pub(crate) _temp: tempfile::TempDir,
    pub(crate) root: PathBuf,
    pub(crate) git: Git,
}
impl Fixture {
    pub(crate) fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("history");
        fs::create_dir(&root).unwrap();
        let config = temp.path().join("gitconfig");
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
        let fixture = Self {
            _temp: temp,
            root,
            git,
        };
        fixture.command(&["init", "--initial-branch=main"], &[]);
        let mut stream = String::new();
        for index in 1..=610 {
            let message = format!("Commit {index}\n\nFull retained metadata");
            stream.push_str(&format!("commit refs/heads/main\nmark :{index}\ncommitter Fixture <fixture@example.invalid> {} +0000\ndata {}\n{}\n\n",
                1600000000 + index, message.len(), message));
        }
        stream.push_str("done\n");
        fixture.command(&["fast-import", "--quiet"], stream.as_bytes());
        fixture
    }
    pub(crate) fn command(&self, args: &[&str], input: &[u8]) -> String {
        let mut request = self.git.request(
            &self.root,
            args.iter().copied(),
            Lifetime::AcceptedCommand,
            4 * 1024 * 1024,
        );
        request.input = input.to_vec();
        let output = process::run(&request, &Cancellation::default(), None).unwrap();
        assert!(output.success(), "{output:?}");
        String::from_utf8(output.stdout)
            .unwrap()
            .trim_end()
            .to_owned()
    }
    pub(crate) fn app(&self, ctx: &egui::Context) -> Workspace {
        let mut app = workspace();
        app.tabs.clear();
        app.settings
            .set("AutoFetchEnabled", serde_json::json!(false));
        app.settings
            .set("CheckUpdatesPeriodically", serde_json::json!(false));
        app.git = Some(Arc::new(self.git.clone()));
        app.open(self.root.clone(), ctx);
        wait(&mut app, ctx);
        assert!(app.tabs[0].error.is_none(), "{:?}", app.tabs[0].error);
        app
    }
}
pub(crate) fn wait(app: &mut Workspace, ctx: &egui::Context) {
    let start = Instant::now();
    while app
        .tabs
        .iter()
        .any(|t| t.loading || t.locating.is_some() || t.view.checking_identity)
    {
        app.receive(ctx);
        assert!(
            start.elapsed() < Duration::from_secs(25),
            "history did not settle"
        );
        thread::sleep(Duration::from_millis(5));
    }
}
pub(crate) fn paint(app: &mut Workspace, ctx: &egui::Context, events: Vec<egui::Event>) {
    frame(ctx, app, events).drop_without_applying_deltas();
}
pub(crate) fn page(app: &mut Workspace, ctx: &egui::Context, older: bool) {
    let key = if older {
        egui::Key::PageDown
    } else {
        egui::Key::PageUp
    };
    paint(
        app,
        ctx,
        vec![
            egui::Event::ModifiersChanged(egui::Modifiers::CTRL),
            egui::Event::Key {
                key,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::CTRL,
            },
        ],
    );
    wait(app, ctx);
    paint(
        app,
        ctx,
        vec![
            egui::Event::ModifiersChanged(egui::Modifiers::NONE),
            egui::Event::Key {
                key,
                physical_key: None,
                pressed: false,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            },
        ],
    );
}
pub(crate) fn position(
    app: &mut Workspace,
    ctx: &egui::Context,
    row: usize,
    offset: egui::Vec2,
) -> (String, egui::Vec2) {
    let tab = &mut app.tabs[0];
    tab.selected = Some(tab.page.as_ref().unwrap().entries[row].commit.hash.clone());
    tab.view.restore_offset = Some(offset);
    paint(app, ctx, vec![]);
    (
        app.tabs[0].selected.clone().unwrap(),
        app.tabs[0].view.offset,
    )
}
pub(crate) fn click_label(app: &mut Workspace, ctx: &egui::Context, label: &str) {
    let output = frame(ctx, app, vec![]);
    let bounds = output
        .platform_output
        .accesskit_update
        .as_ref()
        .unwrap()
        .nodes
        .iter()
        .find(|(_, n)| n.label() == Some(label))
        .and_then(|(_, n)| n.bounds());
    output.drop_without_applying_deltas();
    let bounds = bounds.unwrap_or_else(|| panic!("missing {label}"));
    let pos = egui::pos2(
        ((bounds.x0 + bounds.x1) / 2.) as f32,
        ((bounds.y0 + bounds.y1) / 2.) as f32,
    );
    paint(app, ctx, vec![egui::Event::PointerMoved(pos)]);
    for pressed in [true, false] {
        paint(
            app,
            ctx,
            vec![egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            }],
        );
    }
}

#[test]
fn refresh_and_keyboard_pages_preserve_real_selection_scroll_and_row_allocation() {
    let fixture = Fixture::new();
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    let mut app = fixture.app(&ctx);
    let config = fs::read(fixture.root.join(".git/config")).unwrap();
    let head = fixture.command(&["rev-parse", "HEAD"], &[]);
    assert!(!fixture.root.join(".git/index").exists());
    app.settings.set("SidebarWidth", serde_json::json!(420.));
    app.settings
        .set("RustSidebarRatio", serde_json::json!(0.46));
    paint(&mut app, &ctx, vec![]);
    let first = position(&mut app, &ctx, 34, egui::vec2(20., 700.));
    page(&mut app, &ctx, true);
    assert_eq!(app.tabs[0].page.as_ref().unwrap().cursor.offset, 200);
    let second = position(&mut app, &ctx, 52, egui::vec2(30., 1100.));
    assert!(second.1.y > 1000., "{second:?}");
    assert!(
        first.1.x > 0. && second.1.x > 0.,
        "horizontal offsets: {first:?}, {second:?}"
    );
    let allocation = app.tabs[0].page.as_ref().unwrap().entries.as_ptr() as usize;
    paint(
        &mut app,
        &ctx,
        vec![egui::Event::Key {
            key: egui::Key::F5,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }],
    );
    assert!(app.tabs[0].loading);
    assert_eq!(app.tabs[0].page.as_ref().unwrap().cursor.offset, 200);
    wait(&mut app, &ctx);
    paint(&mut app, &ctx, vec![]);
    assert_eq!(
        app.tabs[0].page.as_ref().unwrap().entries.as_ptr() as usize,
        allocation
    );
    assert_eq!(
        (
            app.tabs[0].selected.clone().unwrap(),
            app.tabs[0].view.offset
        ),
        second
    );
    assert_eq!(app.tabs[0].previous.len(), 1);
    page(&mut app, &ctx, true);
    assert_eq!(app.tabs[0].page.as_ref().unwrap().cursor.offset, 400);
    page(&mut app, &ctx, false);
    assert_eq!(
        (
            app.tabs[0].selected.clone().unwrap(),
            app.tabs[0].view.offset
        ),
        second
    );
    page(&mut app, &ctx, false);
    assert_eq!(
        (
            app.tabs[0].selected.clone().unwrap(),
            app.tabs[0].view.offset
        ),
        first
    );
    page(&mut app, &ctx, true);
    click_label(&mut app, &ctx, "View");
    click_label(&mut app, &ctx, "First history page");
    wait(&mut app, &ctx);
    paint(&mut app, &ctx, vec![]);
    assert_eq!(app.tabs[0].page.as_ref().unwrap().cursor.offset, 0);
    assert_eq!(
        (
            app.tabs[0].selected.clone().unwrap(),
            app.tabs[0].view.offset
        ),
        first
    );
    assert_eq!(fs::read(fixture.root.join(".git/config")).unwrap(), config);
    assert_eq!(fixture.command(&["rev-parse", "HEAD"], &[]), head);
    assert!(!fixture.root.join(".git/index").exists());
}

#[test]
fn changed_boundary_failed_refresh_and_retry_keep_useful_rows_until_validated() {
    let fixture = Fixture::new();
    let ctx = egui::Context::default();
    let mut app = fixture.app(&ctx);
    page(&mut app, &ctx, true);
    let before = position(&mut app, &ctx, 21, egui::vec2(0., 480.));
    let allocation = app.tabs[0].page.as_ref().unwrap().entries.as_ptr() as usize;
    fixture.command(&["commit", "--allow-empty", "-m", "external change"], &[]);
    app.page(true, &ctx);
    wait(&mut app, &ctx);
    assert!(app.tabs[0].error.is_some());
    assert!(app.tabs[0].view.stale);
    assert_eq!(
        app.tabs[0].page.as_ref().unwrap().entries.as_ptr() as usize,
        allocation
    );
    assert_eq!(
        (
            app.tabs[0].selected.clone().unwrap(),
            app.tabs[0].view.offset
        ),
        before
    );
    let generation = app.tabs[0].view.generation;
    app.page(true, &ctx);
    assert_eq!(app.tabs[0].view.generation, generation);
    let mut unavailable = fixture.git.clone();
    unavailable.executable = fixture.root.join("missing-git.exe");
    app.git = Some(Arc::new(unavailable));
    app.refresh(&ctx);
    wait(&mut app, &ctx);
    assert!(app.tabs[0].view.stale);
    assert_eq!(
        app.tabs[0].page.as_ref().unwrap().entries.as_ptr() as usize,
        allocation
    );
    app.git = Some(Arc::new(fixture.git.clone()));
    app.refresh(&ctx);
    wait(&mut app, &ctx);
    assert!(!app.tabs[0].view.stale);
    assert!(app.tabs[0].error.is_none());
    assert_eq!(app.tabs[0].page.as_ref().unwrap().cursor.offset, 0);
    assert!(app.tabs[0].previous.is_empty());
    assert_eq!(
        app.tabs[0].page.as_ref().unwrap().entries[0]
            .commit
            .message
            .trim(),
        "external change"
    );
}

#[test]
fn replaced_identity_clears_presentation_and_requires_explicit_reopen() {
    let fixture = Fixture::new();
    let ctx = egui::Context::default();
    let mut app = fixture.app(&ctx);
    // Retain the original .git in the fixture; never delete or move user data.
    fs::rename(fixture.root.join(".git"), fixture.root.join("original.git")).unwrap();
    fixture.command(&["init", "--initial-branch=replaced"], &[]);
    app.refresh(&ctx);
    wait(&mut app, &ctx);
    assert!(app.tabs[0].view.identity_blocked);
    assert!(app.tabs[0].page.is_none() && app.tabs[0].history.is_none());
    assert_eq!(
        app.tabs[0].error.as_deref(),
        Some(strings::text("IdentityChanged").as_str())
    );
    app.refresh(&ctx);
    assert!(!app.tabs[0].loading);
    app.open(fixture.root.clone(), &ctx);
    wait(&mut app, &ctx);
    assert!(!app.tabs[0].view.identity_blocked);
    assert!(app.tabs[0].page.as_ref().unwrap().entries.is_empty());
    assert_eq!(
        app.tabs[0]
            .history
            .as_ref()
            .unwrap()
            .snapshot
            .branch
            .as_deref(),
        Some("replaced")
    );
}

#[test]
fn obsolete_results_cannot_publish_or_invalidate_newer_history() {
    let fixture = Fixture::new();
    let ctx = egui::Context::default();
    let mut app = fixture.app(&ctx);
    let tab = &app.tabs[0];
    let id = tab.id;
    let old = tab.view.generation;
    let history = tab.history.clone().unwrap();
    let page = tab.page.clone().unwrap();
    app.refresh(&ctx);
    wait(&mut app, &ctx);
    let allocation = app.tabs[0].page.as_ref().unwrap().entries.as_ptr() as usize;
    app.sender
        .send(Event::Failed(id, old, identity_failure(true)))
        .unwrap();
    app.sender
        .send(Event::Loaded(
            id,
            old,
            Box::new(history),
            Box::new(page.clone()),
            false,
            false,
        ))
        .unwrap();
    app.sender
        .send(Event::Page(id, old, Box::new(page), true))
        .unwrap();
    app.receive(&ctx);
    assert_eq!(
        app.tabs[0].page.as_ref().unwrap().entries.as_ptr() as usize,
        allocation
    );
    assert!(!app.tabs[0].view.stale && !app.tabs[0].view.identity_blocked);
    assert!(app.tabs[0].previous.is_empty());
    assert!(app.tabs[0].error.is_none());
}

#[test]
fn refreshed_history_waits_for_the_context_menu_and_keeps_the_visible_commit_anchored() {
    use crate::workspace_interaction_tests::{sidebar_click, sidebar_point};
    let fixture = Fixture::new();
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    let mut app = fixture.app(&ctx);
    let (selected, offset) = position(&mut app, &ctx, 5, egui::vec2(0., 49.));
    let output = frame(&ctx, &mut app, vec![]);
    let row = output
        .platform_output
        .accesskit_update
        .as_ref()
        .unwrap()
        .nodes
        .iter()
        .find_map(|(_, node)| {
            node.label()
                .filter(|label| node.role() == egui::Role::Button && label.contains(&selected))
                .map(str::to_owned)
        })
        .unwrap();
    output.drop_without_applying_deltas();
    sidebar_click(&ctx, &mut app, &row, egui::PointerButton::Secondary);
    let copy = strings::text("HistoryCopySha");
    assert!(sidebar_point(&ctx, &mut app, &copy).is_some());
    assert_eq!(app.history_menu.as_ref().unwrap().commit.hash, selected);

    fixture.command(&["commit", "--allow-empty", "-m", "new tip"], &[]);
    app.refresh(&ctx);
    let deadline = Instant::now() + Duration::from_secs(15);
    while app.deferred_history.is_empty() {
        app.receive(&ctx);
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(5));
    }
    assert!(sidebar_point(&ctx, &mut app, &copy).is_some());
    assert_eq!(
        app.tabs[0].page.as_ref().unwrap().entries[0]
            .commit
            .subject(),
        "Commit 610"
    );
    sidebar_click(&ctx, &mut app, &copy, egui::PointerButton::Primary);
    assert!(!egui::Popup::is_any_open(&ctx));
    wait(&mut app, &ctx);
    paint(&mut app, &ctx, vec![]);
    let tab = &app.tabs[0];
    assert_eq!(
        tab.page.as_ref().unwrap().entries[0].commit.subject(),
        "new tip"
    );
    assert_eq!(tab.selected.as_ref(), Some(&selected));
    assert_eq!(tab.view.offset.y, offset.y + history_row::HEIGHT);
}

#[test]
fn page_views_and_backward_cursors_are_bounded_without_total_history_ceiling() {
    let mut state = State::default();
    let mut cursors = Vec::new();
    for index in 0..140 {
        let cursor = Cursor {
            offset: index * 200,
            ..Default::default()
        };
        let page = Page {
            entries: vec![],
            cursor: cursor.clone(),
            next: None,
        };
        state.remember(Some(&page), &None);
        push_cursor(&mut cursors, cursor);
    }
    assert_eq!(cursors.len(), 128);
    assert_eq!(state.views.len(), 128);
    assert_eq!(cursors[0].offset, 12 * 200);
    assert_eq!(cursors.last().unwrap().offset, 139 * 200);
}

#[test]
fn native_page_results_match_pinned_baseline_and_preserve_repository_bytes() {
    let fixture = Fixture::new();
    let config_before = fs::read(fixture.root.join(".git/config")).unwrap();
    let head_before = fixture.command(&["rev-parse", "HEAD"], &[]);
    let mut records = vec![];
    let mut record = |name: &str, page: &Page| {
        let config = fs::read(fixture.root.join(".git/config")).unwrap();
        assert_eq!(config, config_before);
        assert!(!fixture.root.join(".git/index").exists());
        records.push(serde_json::json!({
            "case": name, "offset": page.cursor.offset, "next": page.next.as_ref().map(|c| c.offset),
            "hashes": page.entries.iter().map(|e| e.commit.hash.as_str()).collect::<Vec<_>>().join("\n"),
            "head": fixture.command(&["rev-parse", "HEAD"], &[]),
            "config": config.iter().map(|b| format!("{b:02X}")).collect::<String>(),
            "indexExists": false, "status": fixture.command(&["status", "--porcelain", "-z"], &[]),
        }));
    };
    let (history, first, _) = read(&fixture.git, &fixture.root, None, false).unwrap();
    record("first", &first);
    let second = read_page(
        &fixture.git,
        &history,
        first.next.as_ref().unwrap(),
        &Cancellation::default(),
    )
    .unwrap();
    record("older", &second);
    let (fresh, refreshed, keep) = read(
        &fixture.git,
        &fixture.root,
        Some((&history, &second.cursor)),
        false,
    )
    .unwrap();
    assert!(keep);
    record("unchanged-refresh", &refreshed);
    let mut last = refreshed;
    while let Some(next) = &last.next {
        last = read_page(&fixture.git, &fresh, next, &Cancellation::default()).unwrap();
    }
    record("end", &last);
    let newer = read_page(
        &fixture.git,
        &history,
        &first.cursor,
        &Cancellation::default(),
    )
    .unwrap();
    record("newer", &newer);
    assert_eq!(fixture.command(&["rev-parse", "HEAD"], &[]), head_before);
    fixture.command(
        &[
            "update-ref",
            "refs/heads/main",
            &first.entries[1].commit.hash,
        ],
        &[],
    );
    assert!(
        read_page(
            &fixture.git,
            &history,
            second.next.as_ref().unwrap(),
            &Cancellation::default()
        )
        .is_err()
    );
    let (_, changed, keep) = read(
        &fixture.git,
        &fixture.root,
        Some((&history, &second.cursor)),
        false,
    )
    .unwrap();
    assert!(!keep);
    record("changed-refresh", &changed);
    if let Some(output) = std::env::var_os("SHOVEL_HISTORY_PARITY_OUTPUT") {
        fs::write(output, serde_json::to_vec_pretty(&records).unwrap()).unwrap();
    }
}

#[test]
fn fresh_activation_only_checks_identity_and_old_activation_refreshes_without_preload() {
    let fixture = Fixture::new();
    let ctx = egui::Context::default();
    let mut app = fixture.app(&ctx);
    app.ensure_tab(fixture.root.join("not-opened"));
    let page_allocation = app.tabs[0].page.as_ref().unwrap().entries.as_ptr() as usize;
    let mut unavailable = fixture.git.clone();
    unavailable.executable = fixture.root.join("missing-git.exe");
    app.git = Some(Arc::new(unavailable));
    app.select_tab(0, &ctx);
    assert!(app.tabs[0].view.checking_identity);
    assert!(!app.tabs[0].loading);
    wait(&mut app, &ctx);
    assert!(
        app.tabs[0].error.is_none(),
        "fresh activation must not run Git"
    );
    assert_eq!(
        app.tabs[0].page.as_ref().unwrap().entries.as_ptr() as usize,
        page_allocation
    );
    assert!(app.tabs[1].history.is_none() && !app.tabs[1].loading);
    app.git = Some(Arc::new(fixture.git.clone()));
    fixture.command(
        &[
            "update-ref",
            "refs/heads/main",
            &app.tabs[0].page.as_ref().unwrap().entries[1]
                .commit
                .hash
                .clone(),
        ],
        &[],
    );
    app.tabs[0].view.validated = Some(Instant::now() - Duration::from_secs(31));
    paint(&mut app, &ctx, vec![egui::Event::WindowFocused(true)]);
    assert!(app.tabs[0].loading);
    wait(&mut app, &ctx);
    assert_eq!(
        app.tabs[0].page.as_ref().unwrap().entries[0]
            .commit
            .subject(),
        "Commit 609"
    );
    assert!(app.tabs[1].history.is_none() && !app.tabs[1].loading);
    fs::rename(fixture.root.join(".git"), fixture.root.join("saved.git")).unwrap();
    fixture.command(&["init", "--initial-branch=new"], &[]);
    app.select_tab(0, &ctx); // Fresh age still must reject directory replacement.
    wait(&mut app, &ctx);
    assert!(app.tabs[0].view.identity_blocked);
    assert!(app.tabs[0].page.is_none());
}
