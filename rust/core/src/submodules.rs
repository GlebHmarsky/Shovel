//! Selected indexed submodules. Native Git owns dirty-file protections,
//! authentication, filters and partial initialization. No force or recursion.
use crate::{
    authentication,
    branches::{Context, Progress, Stage},
    git::{Git, Snapshot},
    identity,
    operation::{Completed, Error, Transcript},
    process::{Cancellation, Lifetime},
    sidebar::Submodule,
    working::MutationGate,
};
use std::{
    fs,
    os::windows::fs::MetadataExt,
    path::{Path, PathBuf},
};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Operation {
    Update,
    Sync,
}
#[derive(Clone, Debug)]
pub struct Action {
    pub target: Submodule,
    pub operation: Operation,
}
pub fn path(root: &Path, relative: &str) -> Result<PathBuf, Error> {
    if relative.is_empty()
        || relative.contains(['\0', ':'])
        || relative.starts_with(['\\', '/'])
        || Path::new(relative).is_absolute()
        || relative.split(['\\', '/']).any(|p| {
            p.is_empty()
                || p == "."
                || p == ".."
                || p.trim_end_matches([' ', '.']).eq_ignore_ascii_case(".git")
                || p.ends_with([' ', '.'])
        })
    {
        return Err(Error::Code("SidebarSubmodulePath"));
    }
    let mut path = root.to_path_buf();
    for part in relative.split(['\\', '/']) {
        path.push(part);
        match fs::symlink_metadata(&path) {
            Ok(meta) if meta.file_attributes() & 0x400 != 0 => {
                return Err(Error::Code("SidebarSubmodulePath"));
            }
            Ok(_) => (),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
            Err(e) => return Err(e.into()),
        }
    }
    Ok(path)
}
impl Git {
    pub fn superproject_worktree(
        &self,
        snapshot: &Snapshot,
        cancel: &Cancellation,
    ) -> Result<Option<PathBuf>, Error> {
        let output = self.read(
            &snapshot.worktree,
            ["rev-parse", "--show-superproject-working-tree"],
            cancel,
            32768,
        )?;
        let root = std::str::from_utf8(&output.stdout)
            .map_err(|_| Error::Code("GitErrorInvalidOutput"))?
            .trim_end_matches(['\r', '\n']);
        if root.is_empty() {
            return Ok(None);
        }
        let root = PathBuf::from(root);
        if !root.is_absolute() || identity::read(&snapshot.worktree)? != snapshot.identity {
            return Err(Error::Code("ActionIdentityChanged"));
        }
        Ok(Some(root))
    }

    pub fn open_submodule(
        &self,
        expected: &Snapshot,
        target: &Submodule,
    ) -> Result<PathBuf, Error> {
        if identity::read(&expected.worktree)? != expected.identity {
            return Err(Error::Code("ActionIdentityChanged"));
        }
        let child = path(&expected.worktree, &target.path)?;
        let observed =
            identity::read(&child).map_err(|_| Error::Code("SidebarSubmoduleUnavailable"))?;
        // Identity reads the exact directory's .git marker, never an ancestor.
        if !observed
            .root
            .to_string_lossy()
            .replace('/', "\\")
            .eq_ignore_ascii_case(&child.to_string_lossy().replace('/', "\\"))
        {
            return Err(Error::Code("SidebarSubmoduleUnavailable"));
        }
        Ok(child)
    }
    fn uninitialized_submodule_common(
        &self,
        context: &Context,
        target: &Submodule,
        cancel: &Cancellation,
    ) -> Result<PathBuf, Error> {
        let root = &context.repository.worktree;
        let result = self.read(
            root,
            [
                "config",
                "--file",
                ".gitmodules",
                "--null",
                "--get-regexp",
                r"^submodule\..*\.path$",
            ],
            cancel,
            1024 * 1024,
        )?;
        let data = std::str::from_utf8(&result.stdout)
            .map_err(|_| Error::Code("GitErrorInvalidOutput"))?;
        let mut names = Vec::new();
        for record in data.split('\0').filter(|s| !s.is_empty()) {
            let (key, value) = record
                .split_once('\n')
                .ok_or(Error::Code("GitErrorInvalidOutput"))?;
            if value == target.path {
                let name = key
                    .strip_prefix("submodule.")
                    .and_then(|key| key.strip_suffix(".path"))
                    .ok_or(Error::Code("GitErrorInvalidOutput"))?;
                names.push(name);
            }
        }
        names.sort_unstable();
        names.dedup();
        if names.len() != 1 {
            return Err(Error::Code("ActionChanged"));
        }
        // Ask Git for its metadata location, including linked-worktree routing.
        let result = self.read(
            root,
            [
                "rev-parse",
                "--path-format=absolute",
                "--git-path",
                "modules",
            ],
            cancel,
            32768,
        )?;
        let modules = PathBuf::from(
            std::str::from_utf8(&result.stdout)
                .map_err(|_| Error::Code("GitErrorInvalidOutput"))?
                .trim_end_matches(['\r', '\n']),
        );
        if !modules.is_absolute() {
            return Err(Error::Code("GitErrorInvalidOutput"));
        }
        if fs::symlink_metadata(&modules).is_ok_and(|m| m.file_attributes() & 0x400 != 0) {
            return Err(Error::Code("SidebarSubmodulePath"));
        }
        path(&modules, names[0])
    }
    pub fn execute_submodule(
        &self,
        context: &Context,
        action: &Action,
        auth: Option<&authentication::Session>,
        progress: Option<Progress>,
    ) -> Result<Completed, Error> {
        let root = &context.repository.worktree;
        let cancel = Cancellation::default();
        let _parent = MutationGate::acquire(&context.repository.common_directory, false)?;
        self.validate_local(context, true, &cancel)?;
        if !self
            .indexed_submodules(root, &cancel)?
            .contains(&action.target)
        {
            return Err(Error::Code("ActionChanged"));
        }
        let child = path(root, &action.target.path)?;
        let marker = child.join(".git");
        let _child = if marker.try_exists()? {
            let observed = identity::read(&child)?;
            let gate = MutationGate::acquire(&observed.common, false)?;
            let state = self.inspect(&child, &cancel)?;
            if state.identity != observed {
                return Err(Error::Code("ActionIdentityChanged"));
            }
            if !state.operations.is_empty() {
                return Err(Error::Code("ActionSequence"));
            }
            Some(gate)
        } else if action.operation == Operation::Update {
            // Reserve the future child's native repository lock before clone
            // creates its .git marker. An opened child must not overlap checkout.
            Some(MutationGate::acquire(
                &self.uninitialized_submodule_common(context, &action.target, &cancel)?,
                false,
            )?)
        } else {
            None
        };
        let mut request = self.request(
            root,
            [
                "-c",
                "core.fsmonitor=false",
                "-c",
                "submodule.recurse=false",
                "-c",
                "protocol.allow=never",
                "-c",
                "protocol.file.allow=always",
                "-c",
                "protocol.https.allow=always",
                "-c",
                "protocol.http.allow=always",
                "-c",
                "protocol.ssh.allow=always",
                "-c",
                "protocol.git.allow=always",
            ],
            Lifetime::AcceptedCommand,
            1024 * 1024,
        );
        if action.operation == Operation::Update {
            request
                .environment
                .extend(self.network_environment(root, auth, &cancel)?);
            request
                .arguments
                .extend(["submodule", "update", "--init", "--checkout", "--"].map(Into::into));
        } else {
            request
                .arguments
                .extend(["submodule", "sync", "--"].map(Into::into));
        }
        request
            .arguments
            .push(format!(":(literal){}", action.target.path).into());
        if let Some(progress) = &progress {
            progress(Stage::UpdatingSubmodule);
        }
        let mut transcript = Transcript::default();
        let result = transcript.run(&request).map(|_| None);
        if let Some(progress) = &progress {
            progress(Stage::Reconciling);
        }
        let mut completed = transcript.finish(self, root, result)?;
        completed.related.push(child);
        completed.authentication_failure = auth.and_then(authentication::Session::failure);
        Ok(completed)
    }
}
