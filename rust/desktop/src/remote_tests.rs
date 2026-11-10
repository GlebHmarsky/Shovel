use super::*;
use crate::{history_view::tests::Fixture, workspace_interaction_tests::frame};
use shovel_core::scheduler::{Lane, Outcome};
use std::{
    fs,
    time::{Duration, Instant},
};

struct Harness {
    ctx: egui::Context,
    dialogs: Dialogs,
    operations: operations::Controller,
    scheduler: Scheduler,
    git: Arc<Git>,
    activity: bool,
}
impl Harness {
    fn new(git: Git) -> Self {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        ctx.set_embed_viewports(true);
        Self {
            ctx,
            dialogs: Default::default(),
            operations: Default::default(),
            scheduler: Default::default(),
            git: Arc::new(git),
            activity: false,
        }
    }
    fn render(&mut self, events: Vec<egui::Event>) -> egui::FullOutput {
        let ctx = self.ctx.clone();
        ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(900., 700.),
                )),
                events,
                ..Default::default()
            },
            |ui| {
                if self.activity {
                    egui::CentralPanel::default().show(ui, |ui| {
                        self.operations.status(ui, None, &self.scheduler);
                    });
                }
                self.dialogs.show(
                    &ctx,
                    Some(&self.git),
                    &mut self.operations,
                    &self.scheduler,
                    true,
                )
            },
        )
    }
    fn node(&mut self, label: Option<&str>, input: usize) -> (egui::Pos2, bool) {
        // Embedded native viewport layout settles without any desktop capture.
        self.render(vec![]).drop_without_applying_deltas();
        let out = self.render(vec![]);
        let nodes = &out.platform_output.accesskit_update.as_ref().unwrap().nodes;
        let mut choices: Vec<_> = nodes
            .iter()
            .filter(|(_, n)| match label {
                Some(label) => n.label() == Some(label) && n.role() != egui::Role::Window,
                None => n.role() == egui::Role::TextInput,
            })
            .filter_map(|(_, n)| {
                n.bounds().map(|r| {
                    (
                        egui::pos2(((r.x0 + r.x1) / 2.) as f32, ((r.y0 + r.y1) / 2.) as f32),
                        n.is_disabled(),
                    )
                })
            })
            .collect();
        choices.sort_by(|a, b| a.0.y.total_cmp(&b.0.y).then(a.0.x.total_cmp(&b.0.x)));
        let labels = nodes
            .iter()
            .filter_map(|(_, n)| n.label().map(str::to_owned))
            .collect::<Vec<_>>();
        out.drop_without_applying_deltas();
        *choices
            .get(input)
            .unwrap_or_else(|| panic!("Missing {label:?}/{input}: {labels:?}"))
    }
    fn at(&mut self, p: egui::Pos2) {
        self.render(vec![egui::Event::PointerMoved(p)])
            .drop_without_applying_deltas();
        for pressed in [true, false] {
            self.render(vec![egui::Event::PointerButton {
                pos: p,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            }])
            .drop_without_applying_deltas();
        }
    }
    fn click(&mut self, label: &str) {
        let (p, disabled) = self.node(Some(label), 0);
        if disabled
            && let Some(Dialog {
                form: Form::Add { name, location, .. },
                ..
            }) = &self.dialogs.active
        {
            panic!("{label} disabled with name={name:?}, location={location:?}");
        }
        assert!(!disabled, "{label} disabled");
        self.at(p);
    }
    fn fill(&mut self, index: usize, value: &str) {
        let (p, disabled) = self.node(None, index);
        assert!(!disabled);
        self.at(p);
        self.render(vec![egui::Event::Key {
            key: egui::Key::A,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers {
                ctrl: true,
                command: true,
                ..Default::default()
            },
        }])
        .drop_without_applying_deltas();
        for pressed in [true, false] {
            self.render(vec![egui::Event::Key {
                key: egui::Key::Backspace,
                physical_key: None,
                pressed,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }])
            .drop_without_applying_deltas();
        }
        self.render(vec![egui::Event::Text(value.into())])
            .drop_without_applying_deltas();
    }
    fn settle(&mut self) {
        self.settle_outcome(Outcome::Succeeded);
    }
    fn settle_outcome(&mut self, outcome: Outcome) {
        let deadline = Instant::now() + Duration::from_secs(20);
        while self.scheduler.has_accepted_work() {
            self.operations.poll(&self.ctx);
            assert!(Instant::now() < deadline, "remote mutation did not finish");
            thread::sleep(Duration::from_millis(5));
        }
        self.operations.poll(&self.ctx);
        assert_eq!(
            self.scheduler.records().last().unwrap().outcome,
            Some(outcome)
        );
    }
}
#[test]
fn actual_form_input_add_edit_fallback_noop_and_confirmed_remove() {
    let f = Fixture::new();
    let snapshot = f.git.inspect(&f.root, &Cancellation::default()).unwrap();
    let mut h = Harness::new(f.git.clone());
    let before = fs::read(f.root.join(".git/config")).unwrap();
    h.dialogs.add(&snapshot);
    assert!(h.node(Some(&text("RemoteAddSubmit")), 0).1);
    h.fill(1, "../initial");
    assert_eq!(before, fs::read(f.root.join(".git/config")).unwrap());
    assert!(h.scheduler.records().is_empty());
    h.click(&text("RemoteAddSubmit"));
    assert!(!h.dialogs.is_open());
    h.settle();
    assert_eq!(
        f.command(&["remote", "get-url", "origin"], &[]),
        "../initial"
    );
    for (fetch, push) in [("../fetch", "../push"), ("../fetch", ""), ("../fetch", "")] {
        let config = f
            .git
            .remote_configuration(&snapshot, &Cancellation::default())
            .unwrap();
        h.dialogs.manage(&snapshot, &config, Some("origin"));
        h.fill(0, fetch);
        h.fill(1, push);
        h.click(&text("RemoteSave"));
        h.settle();
        assert_eq!(
            f.command(&["remote", "get-url", "--push", "origin"], &[]),
            if push.is_empty() { fetch } else { push }
        );
    }
    let stale = f
        .git
        .remote_configuration(&snapshot, &Cancellation::default())
        .unwrap();
    h.dialogs.manage(&snapshot, &stale, Some("origin"));
    f.command(&["config", "remote.origin.url", "../external"], &[]);
    h.click(&text("RemoteSave"));
    h.settle_outcome(Outcome::Failed);
    assert!(
        h.operations
            .summary(&snapshot.worktree)
            .unwrap()
            .contains(&text("ActionFailedShort"))
    );
    h.activity = true;
    h.click(&text("ActionOutput"));
    let output = h.render(vec![]);
    let expected = text("RemoteManageChanged");
    let found = output
        .platform_output
        .accesskit_update
        .as_ref()
        .unwrap()
        .nodes
        .iter()
        .any(|(_, n)| {
            n.label()
                .or(n.value())
                .is_some_and(|s| s.contains(&expected))
        });
    output.drop_without_applying_deltas();
    assert!(found, "Activity must retain the native stale-config error");
    h.activity = false;
    let config = f
        .git
        .remote_configuration(&snapshot, &Cancellation::default())
        .unwrap();
    h.dialogs.manage(&snapshot, &config, Some("origin"));
    h.click(&text("ActionTitleRemoveRemote"));
    assert!(matches!(
        h.dialogs.active.as_ref().unwrap().form,
        Form::Manage { confirm: true, .. }
    ));
    let output = h.render(vec![]);
    let expected = strings::format(
        "RemoteRemoveConfirm",
        &["origin", &snapshot.worktree.to_string_lossy()],
    );
    let found = output
        .platform_output
        .accesskit_update
        .as_ref()
        .unwrap()
        .nodes
        .iter()
        .any(|(_, n)| n.label().or(n.value()) == Some(expected.as_str()));
    output.drop_without_applying_deltas();
    assert!(found, "exact repository and remote confirmation missing");
    h.click(&text("Cancel"));
    assert_eq!(f.command(&["remote"], &[]), "origin");
    h.dialogs.manage(&snapshot, &config, Some("origin"));
    h.click(&text("ActionTitleRemoveRemote"));
    h.click(&text("ActionTitleRemoveRemote"));
    h.settle();
    assert!(f.command(&["remote"], &[]).is_empty());
}
#[test]
fn cached_unsupported_dialog_and_removed_waiting_request_never_write() {
    let f = Fixture::new();
    f.command(&["remote", "add", "origin", "../one"], &[]);
    f.command(&["config", "--add", "remote.origin.url", "../two"], &[]);
    let snapshot = f.git.inspect(&f.root, &Cancellation::default()).unwrap();
    let config = f
        .git
        .remote_configuration(&snapshot, &Cancellation::default())
        .unwrap();
    let mut missing = f.git.clone();
    missing.executable = f.root.join("missing-git.exe");
    let mut h = Harness::new(missing);
    h.dialogs.manage(&snapshot, &config, Some("origin"));
    assert!(h.node(Some(&text("RemoteSave")), 0).1);
    assert!(h.node(None, 0).1);
    h.click(&text("Cancel"));
    assert!(h.scheduler.records().is_empty());
    h.git = Arc::new(f.git.clone());
    let (held, accepted) = h.scheduler.reserve(
        snapshot.identity.fingerprint.clone(),
        snapshot.common_directory.to_string_lossy().to_uppercase(),
        "fixture pending".into(),
        Lane::Exclusive,
        |_| {},
    );
    assert!(accepted);
    h.dialogs.add(&snapshot);
    h.fill(1, "../new");
    h.click(&text("RemoteAddSubmit"));
    let pending = h
        .scheduler
        .records()
        .into_iter()
        .find(|r| r.id != held)
        .unwrap();
    assert!(!pending.running);
    assert!(h.scheduler.remove_waiting(pending.id));
    assert!(h.scheduler.remove_waiting(held));
    h.operations.poll(&h.ctx);
    assert_eq!(f.command(&["remote"], &[]), "origin");
}
#[test]
fn workspace_menus_refresh_linked_caches_and_retained_owner_after_submission() {
    let f = Fixture::new();
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    let mut app = f.app(&ctx);
    let linked = f._temp.path().join("linked");
    f.command(
        &["worktree", "add", "--detach", linked.to_str().unwrap()],
        &[],
    );
    app.open(linked.clone(), &ctx);
    crate::history_view::tests::wait(&mut app, &ctx);
    let deadline = Instant::now() + Duration::from_secs(10);
    while app.tabs.iter().any(|t| t.remotes.receiver.is_some()) {
        app.receive(&ctx);
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(5));
    }
    assert!(
        app.tabs
            .iter()
            .all(|t| t.remotes.value.as_ref().unwrap().remotes.is_empty())
    );
    app.select_tab(0, &ctx);
    // Exercise the actual Repository menu and native modal opening from its cached state.
    let click = |app: &mut crate::Workspace, label: &str| {
        frame(&ctx, app, vec![]).drop_without_applying_deltas();
        let out = frame(&ctx, app, vec![]);
        let node = out
            .platform_output
            .accesskit_update
            .as_ref()
            .unwrap()
            .nodes
            .iter()
            .find(|(_, n)| n.label() == Some(label) && n.bounds().is_some())
            .unwrap();
        let r = node.1.bounds().unwrap();
        let p = egui::pos2(((r.x0 + r.x1) / 2.) as f32, ((r.y0 + r.y1) / 2.) as f32);
        out.drop_without_applying_deltas();
        frame(&ctx, app, vec![egui::Event::PointerMoved(p)]).drop_without_applying_deltas();
        for pressed in [true, false] {
            frame(
                &ctx,
                app,
                vec![egui::Event::PointerButton {
                    pos: p,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                }],
            )
            .drop_without_applying_deltas();
        }
    };
    click(&mut app, &text("RepositoryMenu"));
    click(&mut app, &text("RemoteAddTitle"));
    assert!(app.remotes.is_open());
    assert!(app.scheduler.records().is_empty());
    app.remotes.active = None;
    let snapshot = app.tabs[0].history.as_ref().unwrap().snapshot.clone();
    let (held, _) = app.scheduler.reserve(
        snapshot.identity.fingerprint.clone(),
        snapshot.common_directory.to_string_lossy().to_uppercase(),
        "pending".into(),
        Lane::Exclusive,
        |_| {},
    );
    app.operations.submit_remote(
        "RemoteAddSubmit",
        Action::Add {
            name: "origin".into(),
            location: "../local".into(),
        },
        &snapshot,
        app.git.as_ref().unwrap().clone(),
        &app.scheduler,
        &ctx,
    );
    let owner = app.tabs[0].id;
    app.close_tab(owner, &ctx);
    assert!(app.retained_tabs.iter().any(|t| t.id == owner));
    assert!(app.scheduler.remove_waiting(held));
    let deadline = Instant::now() + Duration::from_secs(20);
    while app.scheduler.has_accepted_work() || app.operations.has_pending(&snapshot.worktree) {
        app.receive(&ctx);
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(5));
    }
    app.receive(&ctx);
    crate::history_view::tests::wait(&mut app, &ctx);
    let deadline = Instant::now() + Duration::from_secs(10);
    while app.tabs[0].remotes.receiver.is_some() {
        app.receive(&ctx);
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(app.tabs[0].path, linked);
    assert_eq!(
        app.tabs[0].remotes.value.as_ref().unwrap().remotes[0].name,
        "origin"
    );
    assert!(
        app.retained_tabs
            .iter()
            .any(|t| t.id == owner && t.refresh_again)
    );
    let out = frame(&ctx, &mut app, vec![]);
    let point = out
        .platform_output
        .accesskit_update
        .as_ref()
        .unwrap()
        .nodes
        .iter()
        .find(|(_, n)| n.label() == Some("origin") && n.bounds().is_some())
        .map(|(_, n)| {
            let r = n.bounds().unwrap();
            egui::pos2(((r.x0 + r.x1) / 2.) as f32, ((r.y0 + r.y1) / 2.) as f32)
        });
    out.drop_without_applying_deltas();
    let point = point.expect("configured remote missing from sidebar");
    frame(&ctx, &mut app, vec![egui::Event::PointerMoved(point)]).drop_without_applying_deltas();
    for pressed in [true, false] {
        frame(
            &ctx,
            &mut app,
            vec![egui::Event::PointerButton {
                pos: point,
                button: egui::PointerButton::Secondary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            }],
        )
        .drop_without_applying_deltas();
    }
    click(&mut app, &text("RemoteManageTitle"));
    assert!(matches!(&app.remotes.active.as_ref().unwrap().form,
        Form::Manage { reviewed, selected, .. } if reviewed.remotes[*selected].name=="origin"));
}
#[test]
fn cache_discards_late_generation_and_preserves_values_on_read_failure() {
    let f = Fixture::new();
    f.command(&["remote", "add", "origin", "../one"], &[]);
    let snapshot = f.git.inspect(&f.root, &Cancellation::default()).unwrap();
    let ctx = egui::Context::default();
    let mut cache = Cache::default();
    let (sender, receiver) = mpsc::channel();
    cache.receiver = Some(receiver);
    cache.invalidate();
    assert!(
        sender
            .send(Ok(f
                .git
                .remote_configuration(&snapshot, &Cancellation::default())
                .unwrap()))
            .is_err()
    );
    cache.poll(&ctx);
    assert!(cache.value.is_none());
    cache.value = Some(
        f.git
            .remote_configuration(&snapshot, &Cancellation::default())
            .unwrap(),
    );
    let mut missing = f.git.clone();
    missing.executable = f.root.join("missing-git.exe");
    cache.start(Arc::new(missing), &snapshot, &ctx, &Default::default());
    let deadline = Instant::now() + Duration::from_secs(5);
    while cache.receiver.is_some() {
        cache.poll(&ctx);
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(5));
    }
    assert!(cache.error.is_some());
    assert_eq!(cache.value.as_ref().unwrap().remotes[0].name, "origin");
}
