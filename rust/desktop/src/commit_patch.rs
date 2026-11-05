use super::*;
use shovel_core::{operation, patches};
impl Editor {
    pub(super) fn begin_patch(&mut self, selected: file_menu::Selection, ctx: &egui::Context) {
        if self.action_busy()
            || !self
                .context
                .as_ref()
                .is_some_and(|c| Arc::ptr_eq(c, &selected.context))
        {
            self.status = Some(text("FileMenuStale"));
            return;
        }
        self.patch_picking = true;
        self.status = Some(text("RustPatchChoosing"));
        let dialog = crate::file_picker::patch();
        let (picker, tx, ctx) = (self.patch_picker.clone(), self.sender.clone(), ctx.clone());
        thread::spawn(move || {
            let target = picker(dialog);
            let _ = tx.send(Event::PatchPicked(selected, target));
            ctx.request_repaint();
        });
    }
    pub(super) fn patch_picked(
        &mut self,
        selected: file_menu::Selection,
        target: Option<PathBuf>,
        ctx: &egui::Context,
    ) {
        self.patch_picking = false;
        let Some(target) = target else {
            self.status = Some(text("RustPatchNotSaved"));
            return;
        };
        if !self
            .context
            .as_ref()
            .is_some_and(|c| Arc::ptr_eq(c, &selected.context))
            || self.action_busy()
        {
            self.status = Some(text("FileMenuStale"));
            return;
        }
        let (git, tx, dropped, ctx, drop_ctx) = (
            self.git.clone(),
            self.sender.clone(),
            self.sender.clone(),
            ctx.clone(),
            ctx.clone(),
        );
        let context = selected.context;
        let entries = selected.entries;
        let (_, accepted) = self.scheduler.submit(
            context.repository.identity.fingerprint.clone(),
            context
                .repository
                .common_directory
                .to_string_lossy()
                .to_uppercase(),
            patches::key(&context, &entries, &target),
            Lane::Read,
            move || {
                let _ = tx.send(Event::PatchStarted);
                ctx.request_repaint();
                let result = git.export_selected_patch(&context, &entries, &target);
                let outcome = if result.is_ok() {
                    Outcome::Succeeded
                } else {
                    Outcome::Failed
                };
                let _ = tx.send(Event::PatchCompleted(result));
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
            self.status = Some(text("OperationWaiting"));
            self.output.clear();
            self.operation_output.clear();
        }
    }
    pub(super) fn patch_completed(
        &mut self,
        result: Result<usize, operation::Error>,
        ctx: &egui::Context,
    ) {
        self.busy = false;
        match result {
            Ok(_) => self.status = Some(text("FilePatchSaved")),
            Err(error) => {
                self.status = Some(text(&error.code()));
                if let operation::Error::Git(shovel_core::git::Error::Command { output, .. }) =
                    &error
                {
                    self.operation_output = shovel_core::network::redact(output);
                }
            }
        }
        // The destination may be inside this worktree. Publish its actual new
        // untracked/modified row without changing source files or staging.
        self.refresh(ctx);
    }
}
