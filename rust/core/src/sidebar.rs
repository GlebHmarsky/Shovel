//! Bounded local sidebar data, read only after the first history is published.
use crate::{
    git::{Git, Snapshot},
    identity,
    operation::Error,
    process::Cancellation,
};
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Branch {
    pub reference: String,
    pub tracking: String,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Submodule {
    pub path: String,
    pub hash: String,
}
#[derive(Clone, Debug)]
pub struct Sidebar {
    pub branches: Vec<Branch>,
    pub submodules: Vec<Submodule>,
}
fn decode(bytes: &[u8]) -> Result<&str, Error> {
    std::str::from_utf8(bytes).map_err(|_| Error::Code("GitErrorInvalidOutput"))
}
pub(crate) fn modules(bytes: &[u8]) -> Result<Vec<Submodule>, Error> {
    let text = decode(bytes)?;
    if !text.is_empty() && !text.ends_with('\0') {
        return Err(Error::Code("GitErrorInvalidOutput"));
    }
    let mut modules = Vec::new();
    for record in text.split('\0').filter(|r| !r.is_empty()) {
        let (meta, path) = record
            .split_once('\t')
            .ok_or(Error::Code("GitErrorInvalidOutput"))?;
        let fields: Vec<_> = meta.split(' ').collect();
        if fields.len() != 3 {
            return Err(Error::Code("GitErrorInvalidOutput"));
        }
        if fields[0] == "160000" && fields[2] == "0" {
            if path.is_empty()
                || !matches!(fields[1].len(), 40 | 64)
                || !fields[1].bytes().all(|c| c.is_ascii_hexdigit())
            {
                return Err(Error::Code("GitErrorInvalidOutput"));
            }
            modules.push(Submodule {
                path: path.to_owned(),
                hash: fields[1].to_owned(),
            });
        }
    }
    Ok(modules)
}
impl Git {
    pub fn sidebar(&self, expected: &Snapshot, cancel: &Cancellation) -> Result<Sidebar, Error> {
        let path = &expected.worktree;
        if identity::read(path)? != expected.identity {
            return Err(Error::Code("ActionIdentityChanged"));
        }
        let data = self.read(
            path,
            [
                "for-each-ref",
                "--format=%(refname)%00%(upstream:track,nobracket)",
                "refs/heads/",
            ],
            cancel,
            4 * 1024 * 1024,
        )?;
        let branches = decode(&data.stdout)?
            .split('\n')
            .filter(|r| !r.is_empty())
            .map(|line| {
                let (reference, tracking) = line
                    .split_once('\0')
                    .ok_or(Error::Code("GitErrorInvalidOutput"))?;
                if !reference.starts_with("refs/heads/") || tracking.contains('\0') {
                    return Err(Error::Code("GitErrorInvalidOutput"));
                }
                Ok(Branch {
                    reference: reference.to_owned(),
                    tracking: tracking.to_owned(),
                })
            })
            .collect::<Result<Vec<_>, Error>>()?;
        let submodules = self.indexed_submodules(path, cancel)?;
        if identity::read(path)? != expected.identity {
            return Err(Error::Code("ActionIdentityChanged"));
        }
        Ok(Sidebar {
            branches,
            submodules,
        })
    }
    pub(crate) fn indexed_submodules(
        &self,
        path: &std::path::Path,
        cancel: &Cancellation,
    ) -> Result<Vec<Submodule>, Error> {
        modules(
            &self
                .read(path, ["ls-files", "--stage", "-z"], cancel, 4 * 1024 * 1024)?
                .stdout,
        )
    }
}
