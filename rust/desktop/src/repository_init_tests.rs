use super::*;
use crate::workspace_interaction_tests::{frame, workspace};
use shovel_core::process::Cancellation;
use std::{
    fs,
    time::{Duration, Instant},
};
struct Harness {
    temp: tempfile::TempDir,
    app: crate::Workspace,
    ctx: egui::Context,
}
impl Harness {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let config = temp.path().join("isolated.config");
        fs::write(&config,"[user]\nname=Fixture\nemail=fixture@example.invalid\n[commit]\ngpgsign=false\n[init]\ndefaultBranch=main\n").unwrap();
        let mut git = Git::discover(None, &Cancellation::default()).unwrap();
        git.environment
            .insert("GIT_CONFIG_GLOBAL".into(), Some(config.into_os_string()));
        git.environment
            .insert("GIT_CONFIG_NOSYSTEM".into(), Some("1".into()));
        let mut app = workspace();
        app.tabs.clear();
        app.selected = 0;
        app.git = Some(Arc::new(git));
        app.draft_store = Some(shovel_core::drafts::Store {
            directory: temp.path().join("drafts"),
            legacy_directory: None,
        });
        app.settings
            .set("AutoFetchEnabled", serde_json::json!(false));
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        ctx.set_embed_viewports(true);
        Self { temp, app, ctx }
    }
    fn render(&mut self, events: Vec<egui::Event>) -> egui::FullOutput {
        self.app.receive(&self.ctx);
        frame(&self.ctx, &mut self.app, events)
    }
    fn point(&mut self, label: &str) -> egui::Pos2 {
        let mut found = None;
        for _ in 0..3 {
            let out = self.render(vec![]);
            for (_, node) in &out.platform_output.accesskit_update.as_ref().unwrap().nodes {
                if node.label() == Some(label) && node.bounds().is_some() {
                    assert!(!node.is_disabled(), "{label}");
                    let r = node.bounds().unwrap();
                    found = Some(egui::pos2(
                        (r.x0 + r.x1) as f32 / 2.,
                        (r.y0 + r.y1) as f32 / 2.,
                    ));
                }
            }
            out.drop_without_applying_deltas();
        }
        found.unwrap_or_else(|| panic!("missing {label}"))
    }
    fn click(&mut self, label: &str) {
        let pos = self.point(label);
        self.render(vec![egui::Event::PointerMoved(pos)])
            .drop_without_applying_deltas();
        for pressed in [true, false] {
            self.render(vec![egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: Default::default(),
            }])
            .drop_without_applying_deltas();
        }
    }
    fn until(&mut self, ready: impl Fn(&crate::Workspace) -> bool) {
        let start = Instant::now();
        loop {
            self.render(vec![]).drop_without_applying_deltas();
            if ready(&self.app) {
                return;
            }
            assert!(
                start.elapsed() < Duration::from_secs(20),
                "{:?}",
                self.app.notice
            );
            thread::sleep(Duration::from_millis(10));
        }
    }
    fn options(&mut self, menu: &str, target: &std::path::Path) {
        self.click(&text(menu));
        self.click(&text("NewRepository"));
        assert!(self.app.initialization.is_open());
        self.app.initialization.form.as_mut().unwrap().directory =
            target.to_string_lossy().into_owned();
    }
}
#[test]
fn options_and_cancel_perform_no_git_or_destination_preparation() {
    let mut h = Harness::new();
    let target = h.temp.path().join("new/nested");
    let mut missing = h.app.git.as_ref().unwrap().as_ref().clone();
    missing.executable = h.temp.path().join("missing-git.exe");
    h.app.git = Some(Arc::new(missing));
    h.options("FileMenu", &target);
    h.click(&text("ActionCancel"));
    assert!(!h.app.initialization.is_open());
    assert!(!h.app.scheduler.has_accepted_work());
    assert!(h.app.scheduler.records().is_empty());
    assert!(h.app.tabs.is_empty());
    assert!(!target.exists());
    assert!(h.app.notice.is_none());
}

#[test]
fn restart_in_progress_cannot_submit_a_previously_open_init_form() {
    let mut h = Harness::new();
    let target = h.temp.path().join("after-restart");
    h.options("FileMenu", &target);
    h.app.closing = true;
    h.app.updater.restart_requested = true;
    let enter = || egui::Event::Key {
        key: egui::Key::Enter,
        physical_key: Some(egui::Key::Enter),
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    };
    frame(&h.ctx, &mut h.app, vec![enter()]).drop_without_applying_deltas();
    assert!(!h.app.initialization.busy);
    assert!(h.app.scheduler.records().is_empty());
    assert!(!target.exists());
    assert_eq!(
        h.app.initialization.form.as_ref().unwrap().directory,
        target.to_string_lossy()
    );
    h.app.closing = false;
    h.app.updater.restart_requested = false;
    h.click(&text("InitCreate"));
    h.until(|app| !app.initialization.busy);
    assert!(target.join(".git").exists());
}

#[test]
fn menu_create_opens_unborn_history_and_persists_the_real_repository_tab() {
    let mut h = Harness::new();
    let target = h.temp.path().join("-new 日本語 folder");
    fs::create_dir(&target).unwrap();
    fs::write(target.join("keep.bin"), [0u8, 255, 2]).unwrap();
    h.options("RepositoryMenu", &target);
    h.app.initialization.form.as_mut().unwrap().branch = "work/start".into();
    h.click(&text("InitCreate"));
    assert!(!h.app.initialization.is_open());
    assert!(h.app.initialization.busy);
    h.app.close(&h.ctx);
    assert!(!h.app.closing);
    assert_eq!(h.app.notice, Some(text("AppCommandRunningClose")));
    h.until(|a| {
        !a.initialization.busy
            && a.tabs
                .first()
                .is_some_and(|t| t.history.is_some() && !t.loading)
    });
    assert_eq!(h.app.tabs[h.app.selected].path, target);
    let tab = &h.app.tabs[h.app.selected];
    assert_eq!(
        tab.history.as_ref().unwrap().snapshot.branch.as_deref(),
        Some("work/start")
    );
    assert!(tab.page.as_ref().unwrap().entries.is_empty());
    assert_eq!(fs::read(target.join("keep.bin")).unwrap(), [0, 255, 2]);
    assert!(
        !target.join(".git/index").exists(),
        "init never stages files"
    );
    assert!(
        h.app
            .operations
            .summary(&target)
            .unwrap()
            .contains(&text("InitSucceeded"))
    );
    let profile = h.temp.path().join("profile");
    let mut store = shovel_core::settings::SettingsStore::new(profile);
    store.load().unwrap();
    store.save(&h.app.settings).unwrap();
    let restored = store.load().unwrap();
    assert_eq!(
        PathBuf::from(
            restored
                .get("SelectedRepositoryPath")
                .unwrap()
                .as_str()
                .unwrap()
        ),
        target
    );
    h.app.open_commit();
    h.until(|a| a.editors.first().is_some_and(|e| e.context.is_some()));
    assert_eq!(
        h.app.editors[0].context.as_ref().unwrap().entries[0].path,
        "keep.bin"
    );
}
#[test]
fn creation_does_not_steal_selection_after_switching_to_another_repository() {
    let mut h = Harness::new();
    let git = h.app.git.clone().unwrap();
    let first = h.temp.path().join("first");
    let second = h.temp.path().join("second");
    for path in [&first, &second] {
        git.initialize_repository(&Options {
            directory: path.clone(),
            initial_branch: "main".into(),
        })
        .unwrap();
    }
    h.app.open(first.clone(), &h.ctx);
    h.until(|a| !a.tabs[0].loading);
    let target = h.temp.path().join("created");
    let canonical = initialization::destination(&target).unwrap();
    let owner = format!(
        "init:{:x}",
        Sha256::digest(canonical.to_string_lossy().to_uppercase().as_bytes())
    );
    let (tx, rx) = mpsc::channel::<()>();
    h.app.scheduler.submit(
        owner.clone(),
        owner,
        "fixture".into(),
        Lane::Exclusive,
        move || {
            let _ = rx.recv();
            Outcome::Succeeded
        },
        |_| {},
    );
    h.options("FileMenu", &target);
    h.click(&text("InitCreate"));
    assert!(h.app.initialization.busy);
    assert!(!target.exists());
    h.app.open(second.clone(), &h.ctx);
    drop(tx);
    h.until(|a| !a.initialization.busy);
    assert_eq!(h.app.tabs[h.app.selected].path, second);
    let created = h.app.tabs.iter().find(|t| t.path == target).unwrap();
    assert!(created.history.is_none());
    assert!(!created.loading, "no background history preload");
    assert!(target.join(".git/HEAD").exists());
}
#[test]
fn failed_init_retains_files_and_picker_ownership_blocks_exit() {
    let mut h = Harness::new();
    let target = h.temp.path().join("existing-file");
    fs::write(&target, b"keep").unwrap();
    h.options("FileMenu", &target);
    h.click(&text("InitCreate"));
    h.until(|a| !a.initialization.busy);
    assert!(h.app.tabs.is_empty());
    assert_eq!(h.app.notice, Some(text("InitInvalidDirectory")));
    assert_eq!(fs::read(&target).unwrap(), b"keep");
    assert_eq!(h.app.operations.failure_path(), Some(target.as_path()));
    h.click(&text("OperationAttentionDismiss"));
    h.options("FileMenu", &target);
    let (tx, rx) = mpsc::channel::<()>();
    let receiver = std::sync::Mutex::new(rx);
    let chosen = h.temp.path().join("chosen");
    let result = chosen.clone();
    h.app.initialization.picker = Arc::new(move || {
        let _ = receiver.lock().unwrap().recv();
        Some(result.clone())
    });
    h.click(&text("InitBrowse"));
    assert!(h.app.initialization.picking);
    h.app.close(&h.ctx);
    assert!(!h.app.closing);
    assert_eq!(h.app.notice, Some(text("RustRepositoryChooserOpen")));
    drop(tx);
    h.until(|a| !a.initialization.picking);
    assert_eq!(
        h.app.initialization.form.as_ref().unwrap().directory,
        chosen.to_string_lossy()
    );
    h.click(&text("ActionCancel"));
    assert!(!chosen.exists());
}

#[test]
fn failed_native_initialization_opens_inspectable_partial_result_and_retains_error() {
    let mut h = Harness::new();
    let templates = h.temp.path().join("templates");
    fs::create_dir(&templates).unwrap();
    fs::write(
        templates.join("objects"),
        b"blocked native object directory",
    )
    .unwrap();
    let git = h.app.git.as_ref().unwrap();
    let configured = shovel_core::process::run(
        &git.request(
            h.temp.path(),
            [
                "config",
                "--file",
                h.temp.path().join("isolated.config").to_str().unwrap(),
                "init.templateDir",
                templates.to_str().unwrap(),
            ],
            shovel_core::process::Lifetime::AcceptedCommand,
            65536,
        ),
        &Cancellation::default(),
        None,
    )
    .unwrap();
    assert!(configured.success());
    let target = h.temp.path().join("partial");
    fs::create_dir(&target).unwrap();
    fs::write(target.join("keep"), b"untouched").unwrap();
    h.options("FileMenu", &target);
    h.click(&text("InitCreate"));
    h.until(|a| !a.initialization.busy && a.tabs.first().is_some_and(|t| !t.loading));
    assert_eq!(h.app.tabs[h.app.selected].path, target);
    assert_eq!(h.app.notice, Some(text("InitIncomplete")));
    assert!(
        h.app
            .operations
            .summary(&target)
            .unwrap()
            .contains(&text("InitIncomplete"))
    );
    assert_eq!(fs::read(target.join("keep")).unwrap(), b"untouched");
    assert_eq!(
        fs::read(target.join(".git/objects")).unwrap(),
        b"blocked native object directory"
    );
    assert!(!target.join(".git/index").exists());
}

#[path = "repository_clone_tests.rs"]
mod clone;
