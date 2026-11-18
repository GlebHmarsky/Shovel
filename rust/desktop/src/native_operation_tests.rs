//! Command-line-driven verification of a real Fetch after its tab closes.
use super::*;
use crate::history_view::tests::Fixture;
use shovel_core::{git::Reference, scheduler::Outcome};
use std::{fs, path::Path};

struct FetchFixture {
    repository: Fixture,
    entered: PathBuf,
    release: PathBuf,
    head: String,
    incoming: String,
}

impl FetchFixture {
    fn new() -> Self {
        let repository = Fixture::new();
        let temporary = repository._temp.path();
        let peer = temporary.join("peer");
        let remote = temporary.join("remote.git");
        let entered = temporary.join("fetch-entered");
        let release = temporary.join("fetch-release");
        let script = temporary.join("upload-pack.sh");
        let command = |args: &[&str]| repository.command(args, &[]);
        command(&["clone", "--no-hardlinks", ".", peer.to_str().unwrap()]);
        let peer = peer.to_str().unwrap();
        command(&[
            "-C",
            peer,
            "commit",
            "--allow-empty",
            "-m",
            "Incoming change",
        ]);
        let incoming = command(&["-C", peer, "rev-parse", "HEAD"]);
        command(&["clone", "--bare", peer, remote.to_str().unwrap()]);
        command(&["remote", "add", "origin", remote.to_str().unwrap()]);
        fs::write(
            &script,
            format!(
                "#!/bin/sh\ntouch {}\nfor i in $(seq 1 300); do\n  if test -f {}; then exec git upload-pack \"$@\"; fi\n  sleep .05\ndone\nexit 1\n",
                shell_path(&entered),
                shell_path(&release),
            ),
        )
        .unwrap();
        command(&[
            "config",
            "remote.origin.uploadpack",
            &format!("sh {}", shell_path(&script)),
        ]);
        let head = command(&["rev-parse", "HEAD"]);
        assert_ne!(head, incoming);
        Self {
            repository,
            entered,
            release,
            head,
            incoming,
        }
    }
}

fn shell_path(path: &Path) -> String {
    format!(
        "'{}'",
        path.to_str()
            .unwrap()
            .replace('\\', "/")
            .replace('\'', "'\\''")
    )
}

#[derive(Debug)]
enum Step {
    Start,
    OpenOwner,
    OpenOther,
    Submit,
    FetchEntered,
    FetchCompleted,
    Reopen,
    Exit,
}

struct Scenario {
    app: Workspace,
    owner: PathBuf,
    other: PathBuf,
    askpass: PathBuf,
    entered: PathBuf,
    release: PathBuf,
    incoming: String,
    step: Step,
    since: Instant,
    owner_id: u64,
    owner_identity: String,
    other_id: u64,
    other_selection: Option<String>,
    result: mpsc::Sender<()>,
}

impl Scenario {
    fn next(&mut self, step: Step) {
        self.step = step;
        self.since = Instant::now();
    }

    fn selected_history(&self) -> Option<&History> {
        let tab = self.app.tabs.get(self.app.selected)?;
        if tab.loading || tab.view.stale || tab.view.checking_identity {
            return None;
        }
        tab.history.as_ref()
    }

    fn after_layout(&mut self, ctx: &egui::Context) {
        assert!(
            self.since.elapsed() < Duration::from_secs(20),
            "native Fetch stalled at {:?}",
            self.step
        );
        match self.step {
            Step::Start if self.app.initialized => {
                assert!(self.app.git.is_some() && self.app.tabs.is_empty());
                self.app
                    .operations
                    .set_authentication_helper(self.askpass.clone());
                self.app.open(self.owner.clone(), ctx);
                self.owner_id = self.app.tabs[0].id;
                self.next(Step::OpenOwner);
            }
            Step::OpenOwner if self.selected_history().is_some() => {
                self.owner_identity = self
                    .selected_history()
                    .unwrap()
                    .snapshot
                    .identity
                    .fingerprint
                    .clone();
                self.app.open(self.other.clone(), ctx);
                self.next(Step::OpenOther);
            }
            Step::OpenOther if self.selected_history().is_some() => {
                let other = &self.app.tabs[self.app.selected];
                self.other_id = other.id;
                self.other_selection.clone_from(&other.selected);
                self.app.select_tab(0, ctx);
                self.next(Step::Submit);
            }
            Step::Submit if self.selected_history().is_some() => {
                assert_eq!(self.app.tabs[self.app.selected].id, self.owner_id);
                let snapshot = self.selected_history().unwrap().snapshot.clone();
                self.app.operations.fetch_remote(
                    "origin",
                    &snapshot,
                    self.app.git.clone().unwrap(),
                    &self.app.scheduler,
                    ctx,
                );
                assert!(self.app.scheduler.has_accepted_work());
                self.next(Step::FetchEntered);
            }
            Step::FetchEntered if self.entered.is_file() => {
                assert!(
                    self.app.scheduler.active_records().iter().any(|record| {
                        record.running && record.repository == self.owner_identity
                    })
                );
                self.app.select_tab(1, ctx);
                self.app.close_tab(self.owner_id, ctx);
                assert_eq!(self.app.tabs[self.app.selected].id, self.other_id);
                assert_eq!(
                    self.app.tabs[self.app.selected].selected,
                    self.other_selection
                );
                assert!(
                    self.app
                        .retained_tabs
                        .iter()
                        .any(|tab| tab.id == self.owner_id)
                );
                self.app.close(ctx);
                assert!(!self.app.closing && !self.app.exit_ready);
                assert_eq!(
                    self.app.notice.as_deref(),
                    Some(strings::text("AppCommandRunningClose").as_str())
                );
                fs::write(&self.release, b"").unwrap();
                self.next(Step::FetchCompleted);
            }
            Step::FetchCompleted if !self.app.scheduler.has_accepted_work() => {
                let completed = self
                    .app
                    .scheduler
                    .records()
                    .into_iter()
                    .find(|record| {
                        record.repository == self.owner_identity
                            && record.key.starts_with("network:Fetch:")
                    })
                    .unwrap();
                assert_eq!(completed.outcome, Some(Outcome::Succeeded));
                let other = &self.app.tabs[self.app.selected];
                assert_eq!(other.id, self.other_id);
                assert_eq!(other.selected, self.other_selection);
                assert!(self.app.operations.summary(&self.other).is_none());
                self.app.open(self.owner.clone(), ctx);
                self.next(Step::Reopen);
            }
            Step::Reopen
                if self.selected_history().is_some_and(|history| {
                    history
                        .snapshot
                        .references
                        .iter()
                        .any(|reference: &Reference| {
                            reference.name == "refs/remotes/origin/main"
                                && reference.hash == self.incoming
                        })
                }) =>
            {
                assert_eq!(self.app.tabs[self.app.selected].id, self.owner_id);
                self.result.send(()).unwrap();
                self.app.close(ctx);
                self.next(Step::Exit);
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
#[ignore = "launches native Windows viewports and fetches from a disposable local remote"]
fn accepted_fetch_finishes_after_tab_closure_in_native_workspace() {
    let other = navigation_fixtures().join("Small");
    assert!(other.join(".git").is_dir());
    let askpass = PathBuf::from(std::env::var_os("SHOVEL_NATIVE_ASKPASS").unwrap());
    assert!(askpass.is_absolute() && askpass.is_file());
    let fixture = FetchFixture::new();
    let owner = fixture.repository.root.clone();
    let entered = fixture.entered.clone();
    let release = fixture.release.clone();
    let incoming = fixture.incoming.clone();
    let profile = isolated_profile();
    let directory = profile.path().to_owned();
    let (result, verified) = mpsc::channel();
    eframe::run_native(
        "Shovel Fetch ownership check",
        test_options(),
        Box::new(move |context| {
            Ok(Box::new(Scenario {
                app: Workspace::new(context, Some(directory)),
                owner,
                other,
                entered,
                release,
                askpass,
                incoming,
                result,
                step: Step::Start,
                since: Instant::now(),
                owner_id: 0,
                owner_identity: String::new(),
                other_id: 0,
                other_selection: None,
            }))
        }),
    )
    .unwrap();
    verified
        .try_recv()
        .expect("Fetch ownership scenario completed");
    let git = |args: &[&str]| fixture.repository.command(args, &[]);
    assert_eq!(
        git(&["rev-parse", "refs/remotes/origin/main"]),
        fixture.incoming
    );
    assert_eq!(git(&["rev-parse", "HEAD"]), fixture.head);
    assert!(git(&["status", "--porcelain"]).is_empty());
    assert!(profile.path().join("history-view.json").is_file());
}
