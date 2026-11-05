//! Native configured diff tools keep Git's side variables, filters and exit
//! status. Discovery occurs on submission, never while building a file menu.
use crate::{
    files::{self, Completed, PrivateIndex, file_request},
    git::Git,
    operation::{Error, Transcript},
    process::{self, Cancellation, Lifetime},
    working::{Change, Context, Group, MutationGate},
};
use std::ffi::OsString;
impl Git {
    pub fn external_file_diff(
        &self,
        context: &Context,
        entries: &[Change],
    ) -> Result<Completed, Error> {
        if entries.len() != 1 {
            return Err(Error::Code("FileMenuSingle"));
        }
        if !context.entries.contains(&entries[0]) {
            return Err(Error::Code("CommitStale"));
        }
        if !files::supported(entries) {
            return Err(Error::Code("FileActionUnsupported"));
        }
        let _gate = MutationGate::acquire(&context.repository.common_directory, false)?;
        let cancel = Cancellation::default();
        self.validate_file_selection(context, entries, &cancel)?;
        let root = &context.repository.worktree;
        let mut tool = None;
        for key in ["diff.guitool", "merge.guitool", "diff.tool", "merge.tool"] {
            let r = process::run(
                &self.request(root, ["config", "--get", key], Lifetime::Read, 65536),
                &cancel,
                None,
            )?;
            if r.outcome != process::Outcome::Exited || r.exit_code > 1 {
                return Err(Error::Code("ActionReadFailed"));
            }
            if r.exit_code == 0 {
                let value = std::str::from_utf8(&r.stdout)
                    .map_err(|_| Error::Code("GitErrorInvalidOutput"))?
                    .trim_end_matches(['\r', '\n']);
                if value.trim().is_empty() || value.chars().any(char::is_control) {
                    return Err(Error::Code("FileDiffToolMissing"));
                }
                tool = Some(value.to_owned());
                break;
            }
        }
        let tool = tool.ok_or(Error::Code("FileDiffToolMissing"))?;
        let entry = &entries[0];
        let mut log = Transcript::default();
        let result = (|| -> Result<(), Error> {
            let mut git = self.clone();
            let mut _private = None;
            if entry.group == Group::Untracked {
                // --no-index uses exit 1 for differences, which can hide an
                // actual tool failure. An isolated intent-to-add index keeps
                // normal native difftool success/failure semantics.
                let temp = tempfile::Builder::new()
                    .prefix("shovel-rust-index-")
                    .tempfile_in(&context.repository.git_directory)?;
                let private = PrivateIndex(temp.into_temp_path());
                std::fs::remove_file(&private.0)?;
                git.environment
                    .insert("GIT_INDEX_FILE".into(), Some(private.0.as_os_str().into()));
                _private = Some(private);
                if !log.run(&file_request(&git, root, ["read-tree", "--empty"]))?
                    || !log.run(&file_request(
                        &git,
                        root,
                        ["add", "--intent-to-add", "--", &entry.path],
                    ))?
                {
                    return Ok(());
                }
            }
            self.validate_file_selection(context, entries, &cancel)?;
            let mut args: Vec<OsString> = [
                "difftool",
                "--no-prompt",
                "--trust-exit-code",
                "--no-textconv",
            ]
            .map(Into::into)
            .to_vec();
            args.push(format!("--tool={tool}").into());
            if entry.group == Group::Staged {
                args.push("--cached".into());
                if let Some(head) = &context.repository.head {
                    args.push(head.into());
                }
            }
            args.extend(["--no-exit-code".into(), "--".into()]);
            args.extend(files::paths(entries).into_iter().map(Into::into));
            log.run(&file_request(&git, root, args))?;
            Ok(())
        })();
        let (commands, output, output_truncated) = log.into_parts();
        Ok(Completed {
            commands,
            output,
            output_truncated,
            message: result.err().map(|e| e.code()),
            warning: None,
            saved_stash: None,
            filesystem_changes: 0,
            updated: self.working_copy(root, &cancel),
        })
    }
}
