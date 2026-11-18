//! Opt-in command-line verification with real Windows viewports and disposable Git data.
use super::*;
use eframe::App;
use std::time::{Duration, Instant};
use winit::platform::windows::EventLoopBuilderExtWindows;

#[derive(Clone, Copy, Debug)]
enum Step {
    Start,
    OpenSmall,
    PositionHistory,
    OpenMergeHeavy,
    SwitchBack,
    OpenDialog,
    SwitchWithDialog,
    CloseOwner,
    ReopenOwner,
    Refresh,
    Exit,
}

struct Scenario {
    app: Workspace,
    repositories: [PathBuf; 2],
    result: mpsc::Sender<Vec<serde_json::Value>>,
    step: Step,
    started: Instant,
    metrics: Vec<serde_json::Value>,
    selection: String,
    scroll: egui::Vec2,
    owner_id: u64,
    layout_recorded: bool,
}

impl Scenario {
    fn next(&mut self, step: Step) {
        self.step = step;
        self.started = Instant::now();
        self.layout_recorded = false;
    }

    fn record(&mut self, event: &str) {
        self.metrics.push(serde_json::json!({
            "event": event,
            "milliseconds": self.started.elapsed().as_secs_f64() * 1000.,
        }));
    }

    fn history_ready(&self) -> bool {
        self.app.tabs.get(self.app.selected).is_some_and(|tab| {
            tab.page.is_some() && !tab.loading && !tab.view.stale && !tab.view.checking_identity
        })
    }

    fn record_first_history_rows(&mut self) {
        let event = match self.step {
            Step::OpenSmall => "cold_small_first_rows",
            Step::OpenMergeHeavy => "cold_merge_first_rows",
            _ => return,
        };
        let tab = &self.app.tabs[self.app.selected];
        if !self.layout_recorded && let Some(page) = &tab.page {
            if tab.view.initial_history {
                assert!(tab.loading && tab.history.is_none() && tab.observed_snapshot().is_none());
                assert!(page.next.is_none());
            }
            self.record(event);
            self.layout_recorded = true;
        }
    }

    fn verify_position(&self) {
        let tab = &self.app.tabs[self.app.selected];
        assert_eq!(tab.id, self.owner_id);
        assert_eq!(tab.selected.as_deref(), Some(self.selection.as_str()));
        assert!((tab.view.offset - self.scroll).length() < 2.);
    }

    fn verify_dialog(&self) {
        let editor = &self.app.editors[0];
        assert!(editor.presented() && !editor.inline);
        assert_eq!(editor.path, self.repositories[0]);
        let context = editor.current_context().unwrap();
        assert!(Workspace::same_path(
            &context.repository.worktree,
            &self.repositories[0]
        ));
    }

    fn after_layout(&mut self, ctx: &egui::Context) {
        assert!(
            self.started.elapsed() < Duration::from_secs(20),
            "native navigation stalled at {:?}",
            self.step
        );
        self.record_first_history_rows();
        match self.step {
            Step::Start if self.app.initialized => {
                assert!(self.app.git.is_some() && self.app.tabs.is_empty());
                self.next(Step::OpenSmall);
                self.app.open(self.repositories[0].clone(), ctx);
            }
            Step::OpenSmall if self.history_ready() => {
                self.record("cold_small_first_layout");
                let tab = &self.app.tabs[0];
                let page = tab.page.as_ref().unwrap();
                assert_eq!(page.entries.len(), 200);
                self.selection = page.entries[50].commit.hash.clone();
                self.owner_id = tab.id;
                self.next(Step::PositionHistory);
                self.app.locate(self.selection.clone(), ctx);
            }
            Step::PositionHistory if self.app.tabs[0].view.offset.y > 0. => {
                self.scroll = self.app.tabs[0].view.offset;
                self.next(Step::OpenMergeHeavy);
                self.app.open(self.repositories[1].clone(), ctx);
            }
            Step::OpenMergeHeavy if self.history_ready() => {
                self.record("cold_merge_first_layout");
                assert_eq!(self.app.tabs[1].page.as_ref().unwrap().entries.len(), 200);
                self.next(Step::SwitchBack);
                self.app.select_tab(0, ctx);
            }
            Step::SwitchBack => {
                self.verify_position();
                if !self.layout_recorded {
                    self.record("cached_switch_first_layout");
                    self.layout_recorded = true;
                }
                if self.history_ready() {
                    self.record("cached_switch_validated");
                    self.next(Step::OpenDialog);
                    self.app.open_commit();
                }
            }
            Step::OpenDialog => {
                let editor = &self.app.editors[0];
                let presented = editor.presented();
                let ready = editor.current_context().is_some();
                if presented && !self.layout_recorded {
                    assert!(!ctx.embed_viewports());
                    self.record("commit_window_first_layout");
                    self.layout_recorded = true;
                }
                if presented && ready {
                    self.verify_dialog();
                    self.record("commit_window_fresh_status");
                    self.next(Step::SwitchWithDialog);
                    self.app.select_tab(1, ctx);
                }
            }
            Step::SwitchWithDialog => {
                self.verify_dialog();
                assert_eq!(self.app.selected, 1);
                assert!(self.app.tabs[1].page.is_some());
                self.record("switch_with_dialog_first_layout");
                self.next(Step::CloseOwner);
                self.app.close_tab(self.owner_id, ctx);
            }
            Step::CloseOwner => {
                self.verify_dialog();
                assert_eq!(self.app.tabs.len(), 1);
                assert!(
                    self.app
                        .retained_tabs
                        .iter()
                        .any(|tab| tab.id == self.owner_id)
                );
                self.record("close_owner_first_layout");
                self.next(Step::ReopenOwner);
                self.app.open(self.repositories[0].clone(), ctx);
            }
            Step::ReopenOwner => {
                self.verify_dialog();
                self.verify_position();
                if !self.layout_recorded {
                    self.record("warm_reopen_first_layout");
                    self.layout_recorded = true;
                }
                if self.history_ready() {
                    self.record("warm_reopen_validated");
                    self.next(Step::Refresh);
                    self.app.refresh(ctx);
                }
            }
            Step::Refresh if self.history_ready() => {
                self.verify_position();
                self.verify_dialog();
                self.record("explicit_refresh_fresh_layout");
                self.result.send(std::mem::take(&mut self.metrics)).unwrap();
                self.next(Step::Exit);
                self.app.close(ctx);
            }
            _ => {}
        }
        ctx.request_repaint_after(Duration::from_millis(16));
    }
}

impl App for Scenario {
    fn logic(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.app.logic(ctx, frame);
    }

    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        self.app.ui(ui, frame);
        self.after_layout(ui.ctx());
    }
}

#[test]
#[ignore = "launches native Windows viewports; requires disposable repositories and isolated Git config"]
fn measure_native_repository_navigation() {
    let fixtures = PathBuf::from(std::env::var_os("SHOVEL_NAVIGATION_FIXTURES").unwrap());
    let repositories = ["Small", "Merge-20k"].map(|name| fixtures.join(name));
    assert!(fixtures.is_absolute() && repositories.iter().all(|path| path.join(".git").is_dir()));
    let expected_config = fixtures.join("gitconfig");
    assert!(expected_config.is_file());
    assert_eq!(
        std::env::var_os("GIT_CONFIG_GLOBAL").as_deref(),
        Some(expected_config.as_os_str())
    );
    assert_eq!(std::env::var("GIT_CONFIG_NOSYSTEM").as_deref(), Ok("1"));

    let profile = tempfile::tempdir().unwrap();
    let directory = profile.path().to_owned();
    let mut store = SettingsStore::new(directory.clone());
    let mut settings = store.load().unwrap();
    settings.set("AutoFetchEnabled", serde_json::json!(false));
    settings.set("CheckUpdatesPeriodically", serde_json::json!(false));
    store.save(&settings).unwrap();

    let mut options = native_options();
    options.viewport = options.viewport.with_active(false);
    options.event_loop_builder = Some(Box::new(|builder| {
        builder.with_any_thread(true);
    }));
    let (result, verified) = mpsc::channel();
    eframe::run_native(
        "Shovel navigation check",
        options,
        Box::new(move |context| {
            Ok(Box::new(Scenario {
                app: Workspace::new(context, Some(directory)),
                repositories,
                result,
                step: Step::Start,
                started: Instant::now(),
                metrics: vec![],
                selection: String::new(),
                scroll: egui::Vec2::ZERO,
                owner_id: 0,
                layout_recorded: false,
            }))
        }),
    )
    .unwrap();
    let metrics = verified.try_recv().expect("navigation completed");
    assert!(profile.path().join("history-view.json").is_file());
    println!(
        "NATIVE_NAVIGATION {}",
        serde_json::to_string(&metrics).unwrap()
    );
}
