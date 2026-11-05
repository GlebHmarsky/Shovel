use super::*;
use crate::file_shell;
impl Editor {
    pub(super) fn begin_shell(
        &mut self,
        selection: file_menu::Selection,
        explorer: bool,
        ctx: &egui::Context,
    ) {
        if self.action_busy()
            || !self
                .context
                .as_ref()
                .is_some_and(|c| Arc::ptr_eq(c, &selection.context))
        {
            self.status = Some(text("FileMenuStale"));
            return;
        }
        if selection.entries.len() != 1 {
            self.status = Some(text("FileMenuSingle"));
            return;
        }
        if !explorer && selection.entries[0].code == 'D' {
            self.status = Some(text("FileMenuDeleted"));
            return;
        }
        let guard = match file_shell::Guard::acquire() {
            Ok(guard) => guard,
            Err(code) => {
                self.status = Some(text(code));
                return;
            }
        };
        let context = selection.context;
        let relative = selection.entries[0].path.clone();
        let key = format!(
            "file-shell:{explorer}:{}",
            shovel_core::files::discard_key(&context, &selection.entries)
        );
        let (launch, tx, dropped, ctx, drop_ctx) = (
            self.shell_launcher.clone(),
            self.sender.clone(),
            self.sender.clone(),
            ctx.clone(),
            ctx.clone(),
        );
        let (_, accepted) = self.scheduler.submit(
            context.repository.identity.fingerprint.clone(),
            context
                .repository
                .common_directory
                .to_string_lossy()
                .to_uppercase(),
            key,
            Lane::Read,
            move || {
                let _guard = guard;
                let result = file_shell::prepare(&context.repository, &relative, explorer)
                    .and_then(|target| {
                        launch(&target)?;
                        Ok(target.missing_file_folder)
                    });
                let outcome = if result.is_ok() {
                    Outcome::Succeeded
                } else {
                    Outcome::Failed
                };
                let _ = tx.send(Event::ShellCompleted(result));
                ctx.request_repaint();
                outcome
            },
            move |_| {
                let _ = dropped.send(Event::Removed);
                drop_ctx.request_repaint();
            },
        );
        if accepted {
            self.busy = true;
            self.status = Some(text("FileMenuOpening"));
            self.output.clear();
            self.operation_output.clear();
        }
    }
    pub(super) fn shell_completed(&mut self, result: Result<bool, &'static str>) {
        self.busy = false;
        self.status = Some(text(match result {
            Ok(true) => "FileMenuOpenedFolder",
            Ok(false) => "FileMenuOpened",
            Err(code) => code,
        }));
    }
}
