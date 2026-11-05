//! Selected stashes use a private index so the saved index parent cannot replay
//! unrelated staged changes. Only after saving do we reset reviewed real entries.
use crate::{
    files::{self, Completed, PrivateIndex, file_request as request},
    git::Git,
    operation::{Error, Transcript},
    process::Cancellation,
    working::{Change, Context, Group, MutationGate},
};
use std::{fs, io::Write};

fn oid(value: &str) -> bool {
    matches!(value.len(), 40 | 64) && value.bytes().all(|b| b.is_ascii_hexdigit())
}
impl Git {
    fn selected_stash_tip(
        &self,
        context: &Context,
        cancel: &Cancellation,
    ) -> Result<Option<String>, Error> {
        let raw = self.read(
            &context.repository.worktree,
            ["for-each-ref", "--format=%(objectname)", "refs/stash"],
            cancel,
            4096,
        )?;
        let value = std::str::from_utf8(&raw.stdout)
            .map_err(|_| Error::Code("GitErrorInvalidOutput"))?
            .trim();
        if value.is_empty() {
            Ok(None)
        } else if oid(value) {
            Ok(Some(value.into()))
        } else {
            Err(Error::Code("GitErrorInvalidOutput"))
        }
    }
    pub fn stash_files(
        &self,
        context: &Context,
        entries: &[Change],
        message: &str,
    ) -> Result<Completed, Error> {
        if message.contains('\0') || message.len() > 4096 {
            return Err(Error::Code("StashMessageLimit"));
        }
        if entries.is_empty() || entries.iter().any(|e| !context.entries.contains(e)) {
            return Err(Error::Code("CommitStale"));
        }
        if !files::supported(entries) {
            return Err(Error::Code("FileActionUnsupported"));
        }
        let head = context
            .repository
            .head
            .as_deref()
            .ok_or(Error::Code("StashNeedsCommit"))?;
        if !context.repository.operations.is_empty() {
            return Err(Error::Code("ActionSequence"));
        }
        let _gate = MutationGate::acquire(&context.repository.common_directory, false)?;
        let cancel = Cancellation::default();
        self.validate_file_selection(context, entries, &cancel)?;
        let root = &context.repository.worktree;
        let tracked = files::paths(
            &entries
                .iter()
                .filter(|e| e.group != Group::Untracked)
                .cloned()
                .collect::<Vec<_>>(),
        );
        let tracked_input = files::input(&tracked)?;
        let all_input = files::input(&files::paths(entries))?;
        let before = self.selected_stash_tip(context, &cancel)?;
        let mut log = Transcript::default();
        let mut saved = None;
        let result = (|| -> Result<(), Error> {
            // Beside the real index: Git split-index shared files resolve here.
            let mut temp = tempfile::Builder::new()
                .prefix("shovel-rust-index-")
                .tempfile_in(&context.repository.git_directory)?;
            match fs::File::open(context.repository.git_directory.join("index")) {
                Ok(mut source) => {
                    std::io::copy(&mut source, temp.as_file_mut())?;
                    temp.flush()?;
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(e.into()),
            }
            let empty = temp.as_file().metadata()?.len() == 0;
            let private = PrivateIndex(temp.into_temp_path());
            let mut git = self.clone();
            git.environment
                .insert("GIT_INDEX_FILE".into(), Some(private.0.as_os_str().into()));
            if empty {
                fs::remove_file(&private.0)?;
                if !log.run(&request(&git, root, ["read-tree", "--empty"]))? {
                    return Ok(());
                }
            }
            if git.semantic_index(&context.repository, &cancel)? != context.index_stamp {
                return Err(Error::Code("CommitStale"));
            }
            let tree = log.run_capture(&request(&git, root, ["write-tree"]))?;
            if !tree.success() {
                return Ok(());
            }
            let tree = std::str::from_utf8(&tree.stdout)
                .map_err(|_| Error::Code("GitErrorInvalidOutput"))?
                .trim();
            if !oid(tree)
                || self.semantic_index(&context.repository, &cancel)? != context.index_stamp
            {
                return Err(Error::Code("CommitStale"));
            }
            if !log.run(&request(&git, root, ["read-tree", head]))? {
                return Ok(());
            }
            if !tracked.is_empty() {
                let mut r = request(
                    &git,
                    root,
                    [
                        "restore".to_owned(),
                        "--staged".into(),
                        format!("--source={tree}"),
                        "--pathspec-from-file=-".into(),
                        "--pathspec-file-nul".into(),
                    ],
                );
                r.input = tracked_input.clone();
                if !log.run(&r)? {
                    return Ok(());
                }
            }
            // Private preparation may run hooks. Revalidate real state before
            // the first command that saves and changes selected working files.
            self.validate_file_selection(context, entries, &cancel)?;
            let mut r = request(
                &git,
                root,
                [
                    "stash".to_owned(),
                    "push".into(),
                    "--no-keep-index".into(),
                    "--include-untracked".into(),
                    format!("--message={message}"),
                    "--pathspec-from-file=-".into(),
                    "--pathspec-file-nul".into(),
                ],
            );
            r.input = all_input;
            let pushed = log.run(&r)?;
            let after = self.selected_stash_tip(context, &cancel)?;
            if after != before {
                saved = after;
            }
            if !pushed {
                return Ok(());
            }
            if saved.is_none() {
                return Err(Error::Code("RustFileStashNotSaved"));
            }
            if !tracked.is_empty() {
                let fresh = self.inspect(root, &cancel)?;
                if fresh.identity != context.repository.identity
                    || fresh.head != context.repository.head
                    || fresh.branch != context.repository.branch
                    || fresh.operations != context.repository.operations
                    || self.semantic_index(&fresh, &cancel)? != context.index_stamp
                    || self.config_stamp(root, &cancel)? != context.config_stamp
                {
                    return Err(Error::Code("FileStashIndexRetained"));
                }
                let mut r = request(
                    self,
                    root,
                    [
                        "restore".to_owned(),
                        "--staged".into(),
                        format!("--source={head}"),
                        "--pathspec-from-file=-".into(),
                        "--pathspec-file-nul".into(),
                    ],
                );
                r.input = tracked_input;
                if !log.run(&r)? {
                    return Err(Error::Code("FileStashIndexRetained"));
                }
            }
            Ok(())
        })();
        let (commands, output, output_truncated) = log.into_parts();
        Ok(Completed {
            commands,
            output,
            output_truncated,
            message: result.err().map(|e| e.code()),
            warning: None,
            saved_stash: saved,
            filesystem_changes: 0,
            updated: self.working_copy(root, &cancel),
        })
    }
}
