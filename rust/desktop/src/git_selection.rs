//! Session-only explicit Git selection. Discovery runs after user selection.
use crate::{Workspace, error_text, strings::text};
use eframe::egui;
use shovel_core::{git::Git, process::Cancellation};
use std::{
    path::PathBuf,
    sync::{
        Arc,
        mpsc::{self, Receiver},
    },
    thread,
};
type Selection = Result<Option<Git>, String>;
#[derive(Default)]
pub(super) struct Controller {
    receiver: Option<Receiver<Selection>>,
    #[cfg(test)]
    pub test_choice: Option<Option<PathBuf>>,
}
impl Controller {
    pub fn busy(&self) -> bool {
        self.receiver.is_some()
    }
    fn start(&mut self, ctx: &egui::Context) {
        let (sender, receiver) = mpsc::channel();
        self.receiver = Some(receiver);
        let ctx = ctx.clone();
        #[cfg(test)]
        let fixture = self.test_choice.take();
        thread::spawn(move || {
            #[cfg(test)]
            let selected = match fixture {
                Some(path) => path,
                None => pick(),
            };
            #[cfg(not(test))]
            let selected = pick();
            let result = selected
                .map(|path| {
                    Git::discover(Some(&path), &Cancellation::default()).map_err(error_text)
                })
                .transpose();
            let _ = sender.send(result);
            ctx.request_repaint();
        });
    }
    fn poll(&mut self) -> Option<Selection> {
        let receiver = self.receiver.as_ref()?;
        let value = match receiver.try_recv() {
            Ok(value) => value,
            Err(mpsc::TryRecvError::Empty) => return None,
            Err(mpsc::TryRecvError::Disconnected) => Err(text("GitErrorStartFailed")),
        };
        self.receiver = None;
        Some(value)
    }
}
fn pick() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title(text("ChooseGitTitle"))
        .add_filter(text("RustGitExecutableFilter"), &["exe"])
        .pick_file()
}
impl Workspace {
    pub(super) fn can_choose_git(&self) -> bool {
        self.initialized
            && !self.closing
            && !self.git_selection.busy()
            && !self.scheduler.has_accepted_work()
            && !self.initialization.busy
            && self
                .tabs
                .iter()
                .chain(self.retained_tabs.iter())
                .all(|t| !t.loading)
            && self.editors.iter().all(|e| e.can_retire())
            && self.stash_views.iter().all(|v| v.can_retire())
    }
    pub(super) fn choose_git(&mut self, ctx: &egui::Context) {
        if !self.can_choose_git() {
            self.notice = Some(text("RustGitSelectionBusy"));
            return;
        }
        self.auto_fetch.cancel();
        self.notice = Some(text("RustGitSelecting"));
        self.git_selection.start(ctx);
    }
    pub(super) fn poll_git_selection(&mut self, ctx: &egui::Context) {
        let Some(result) = self.git_selection.poll() else {
            return;
        };
        match result {
            Ok(None) => self.notice = None,
            Err(error) => self.notice = Some(error),
            Ok(Some(mut git)) => {
                // Keep the current process environment policy (including isolated
                // test/config environments) when replacing the executable.
                if let Some(current) = &self.git {
                    git.environment = current.environment.clone();
                }
                self.git = Some(Arc::new(git));
                self.tab_markers.reset();
                // Selection is admitted only with retired editors and no user
                // commands. Persisted drafts remain owned by the draft store.
                self.editors.clear();
                self.stash_views.clear();
                for tab in self.tabs.iter_mut().chain(self.retained_tabs.iter_mut()) {
                    tab.view.stale = true;
                    tab.view.validated = None;
                    tab.refresh_again = true;
                    tab.comparison = None;
                    tab.divergence.invalidate();
                    tab.remotes.invalidate();
                    tab.sidebar.invalidate();
                }
                self.notice = None;
                if !self.tabs.is_empty() {
                    self.refresh(ctx);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        history_view::tests::{Fixture, wait},
        workspace_interaction_tests::{frame, sidebar_click},
    };
    use std::time::{Duration, Instant};
    fn finish(app: &mut Workspace, ctx: &egui::Context) {
        let end = Instant::now() + Duration::from_secs(15);
        while app.git_selection.busy() {
            app.receive(ctx);
            assert!(Instant::now() < end);
            thread::sleep(Duration::from_millis(5));
        }
        wait(app, ctx);
    }
    fn select(app: &mut Workspace, ctx: &egui::Context, path: Option<PathBuf>) {
        app.git_selection.test_choice = Some(path);
        sidebar_click(
            ctx,
            app,
            &text("RepositoryMenu"),
            egui::PointerButton::Primary,
        );
        sidebar_click(ctx, app, &text("ChooseGit"), egui::PointerButton::Primary);
        finish(app, ctx);
    }
    #[test]
    fn actual_repository_menu_selects_native_git_and_refreshes_without_saving_choice() {
        let f = Fixture::new();
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let mut app = f.app(&ctx);
        let chosen = f
            .git
            .executable
            .ancestors()
            .map(|p| p.join("cmd/git.exe"))
            .find(|p| p.is_file())
            .unwrap();
        let old = app.git.as_ref().unwrap().clone();
        let config = std::fs::read(f.root.join(".git/config")).unwrap();
        sidebar_click(
            &ctx,
            &mut app,
            &text("RepositoryMenu"),
            egui::PointerButton::Primary,
        );
        assert!(!app.git_selection.busy());
        assert!(Arc::ptr_eq(app.git.as_ref().unwrap(), &old));
        frame(
            &ctx,
            &mut app,
            vec![egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
        )
        .drop_without_applying_deltas();
        select(&mut app, &ctx, Some(chosen.clone()));
        assert_eq!(app.git.as_ref().unwrap().executable, chosen);
        assert!(!Arc::ptr_eq(app.git.as_ref().unwrap(), &old));
        assert!(app.tabs[0].error.is_none());
        assert!(app.tabs[0].history.is_some());
        assert_eq!(std::fs::read(f.root.join(".git/config")).unwrap(), config);
        assert!(
            app.settings.get("GitExecutable").is_none(),
            "selection is session-only in baseline"
        );
        // A real native mutation uses the newly selected Git and original
        // isolated fixture configuration, not any normal user repository.
        let snapshot = app.tabs[0].history.as_ref().unwrap().snapshot.clone();
        app.operations.submit_remote(
            "RemoteAddSubmit",
            shovel_core::remotes::Action::Add {
                name: "chosen-git".into(),
                location: "../no-contact".into(),
            },
            &snapshot,
            app.git.as_ref().unwrap().clone(),
            &app.scheduler,
            &ctx,
        );
        let end = Instant::now() + Duration::from_secs(15);
        while app.scheduler.has_accepted_work() || app.operations.has_pending(&snapshot.worktree) {
            app.receive(&ctx);
            assert!(Instant::now() < end);
            thread::sleep(Duration::from_millis(5));
        }
        app.receive(&ctx);
        wait(&mut app, &ctx);
        assert_eq!(
            f.command(&["remote", "get-url", "chosen-git"], &[]),
            "../no-contact"
        );
    }
    #[test]
    fn cancel_and_invalid_selection_preserve_current_git_and_accepted_work_blocks_selection() {
        let f = Fixture::new();
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let mut app = f.app(&ctx);
        let old = app.git.as_ref().unwrap().clone();
        app.initialized = false;
        app.git_selection.test_choice = Some(None);
        app.choose_git(&ctx);
        assert!(
            !app.git_selection.busy(),
            "startup discovery must not race an explicit choice"
        );
        app.initialized = true;
        select(&mut app, &ctx, None);
        assert!(Arc::ptr_eq(app.git.as_ref().unwrap(), &old));
        select(&mut app, &ctx, Some(f.root.join("missing-git.exe")));
        assert!(Arc::ptr_eq(app.git.as_ref().unwrap(), &old));
        assert!(app.notice.is_some());
        let snapshot = app.tabs[0].history.as_ref().unwrap().snapshot.clone();
        let (id, _) = app.scheduler.reserve(
            snapshot.identity.fingerprint.clone(),
            shovel_core::case::path_key(&snapshot.common_directory),
            "held".into(),
            shovel_core::scheduler::Lane::Exclusive,
            |_| {},
        );
        assert!(!app.can_choose_git());
        app.choose_git(&ctx);
        assert!(!app.git_selection.busy());
        assert!(Arc::ptr_eq(app.git.as_ref().unwrap(), &old));
        assert!(app.scheduler.remove_waiting(id));
    }
}
