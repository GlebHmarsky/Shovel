//! Root ignore/attribute edits preserve existing bytes and never stage,
//! migrate, install filters or change global configuration.
use crate::{
    files::{self, Completed},
    git::Git,
    network::redact,
    operation::Error,
    process::{self, Cancellation, Lifetime},
    working::{Change, Context, Group, MutationGate},
};
use std::{
    collections::BTreeSet,
    fs::OpenOptions,
    io::{Read, Write},
    os::windows::fs::OpenOptionsExt,
    path::Path,
};
const LIMIT: usize = 1024 * 1024;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    IgnorePaths,
    IgnoreExtensions,
    LfsTrack,
    LfsUntrack,
}
impl Action {
    fn ignore(self) -> bool {
        matches!(self, Self::IgnorePaths | Self::IgnoreExtensions)
    }
}
/// Match System.IO.Path.GetExtension: a leading-dot name has an extension,
/// while a trailing dot or a dot only in the parent directory does not.
pub fn extension(path: &str) -> Option<&str> {
    let leaf = path.rsplit(['/', '\\']).next()?;
    let dot = leaf.rfind('.')?;
    (dot + 1 < leaf.len()).then_some(&leaf[dot..])
}
fn escape_ignore(value: &str) -> Result<String, Error> {
    if value.chars().any(char::is_control) {
        return Err(Error::Code("FileRulePathUnsupported"));
    }
    let mut out = String::new();
    for c in value.chars() {
        if "\\*?[]!# ".contains(c) {
            out.push('\\');
        }
        out.push(c);
    }
    Ok(out)
}
fn attribute_pattern(value: &str) -> Result<String, Error> {
    if value.chars().any(char::is_control) {
        return Err(Error::Code("FileRulePathUnsupported"));
    }
    let mut pattern = String::from("/");
    for c in value.chars() {
        if "*?[]".contains(c) {
            pattern.push('\\');
        }
        pattern.push(c);
    }
    Ok(format!(
        "\"{}\"",
        pattern.replace('\\', "\\\\").replace('"', "\\\"")
    ))
}
fn append(root: &Path, relative: &str, rules: &[String], changed: &mut bool) -> Result<(), Error> {
    let path = files::regular(root, relative)?;
    let mut options = OpenOptions::new();
    options.read(true).write(true).share_mode(0);
    let mut file = match options.clone().create_new(true).open(&path) {
        Ok(file) => {
            *changed = true;
            file
        }
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => options.open(&path)?,
        Err(e) => return Err(e.into()),
    };
    let size = file.metadata()?.len();
    if size > LIMIT as u64 {
        return Err(Error::Code("FileRulesTooLarge"));
    }
    let mut bytes = vec![];
    file.read_to_end(&mut bytes)?;
    let text = std::str::from_utf8(&bytes).map_err(|_| Error::Code("FileRulesEncoding"))?;
    let newline = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let mut unique = BTreeSet::new();
    let additions = rules
        .iter()
        .filter(|s| unique.insert(s.as_str()))
        .map(String::as_str)
        .collect::<Vec<_>>();
    // Append after existing negations/overrides even if an older identical
    // pattern exists. Never replace a concurrent editor's existing text.
    let extra = format!(
        "{}{}{}",
        if !bytes.is_empty() && bytes.last() != Some(&b'\n') {
            newline
        } else {
            ""
        },
        additions.join(newline),
        newline
    );
    if bytes.len() + extra.len() > LIMIT {
        return Err(Error::Code("FileRulesTooLarge"));
    }
    let result = file.write_all(extra.as_bytes()).and_then(|_| file.flush());
    *changed |= file.metadata().is_ok_and(|m| m.len() != size);
    result.map_err(Error::from)
}
impl Git {
    pub fn edit_file_rules(
        &self,
        context: &Context,
        entries: &[Change],
        action: Action,
    ) -> Result<Completed, Error> {
        if entries.is_empty() || entries.iter().any(|e| !context.entries.contains(e)) {
            return Err(Error::Code("CommitStale"));
        }
        if !files::supported(entries) {
            return Err(Error::Code("FileActionUnsupported"));
        }
        let paths = files::paths(entries);
        if paths.len() > 128
            || paths
                .iter()
                .map(|p| p.encode_utf16().count() + if action.ignore() { 3 } else { 8 })
                .sum::<usize>()
                >= 16000
        {
            return Err(Error::Code("FileActionSelectionLimit"));
        }
        if action.ignore() && entries.iter().any(|e| e.group != Group::Untracked) {
            return Err(Error::Code("FileIgnoreTracked"));
        }
        let rules = paths
            .iter()
            .map(|path| match action {
                Action::IgnorePaths => Ok(format!("/{}", escape_ignore(path)?)),
                Action::IgnoreExtensions => Ok(format!(
                    "*{}",
                    escape_ignore(extension(path).ok_or(Error::Code("FileIgnoreNoExtension"))?)?
                )),
                Action::LfsTrack | Action::LfsUntrack => Ok(format!(
                    "{}{}",
                    attribute_pattern(path)?,
                    if action == Action::LfsTrack {
                        " filter=lfs diff=lfs merge=lfs -text"
                    } else {
                        " !filter !diff !merge !text"
                    }
                )),
            })
            .collect::<Result<Vec<_>, Error>>()?;
        let _gate = MutationGate::acquire(&context.repository.common_directory, false)?;
        let cancel = Cancellation::default();
        self.validate_file_selection(context, entries, &cancel)?;
        let root = &context.repository.worktree;
        let relative = if action.ignore() {
            ".gitignore"
        } else {
            ".gitattributes"
        };
        files::regular(root, relative)?;
        if action == Action::LfsTrack {
            let version = process::run(
                &self.request(root, ["lfs", "version"], Lifetime::Read, 65536),
                &cancel,
                None,
            )?;
            if !version.success() {
                return Err(Error::Code("FileLfsMissing"));
            }
            let filters = process::run(
                &self.request(
                    root,
                    ["config", "--get-regexp", r"^filter\.lfs\.(clean|process)$"],
                    Lifetime::Read,
                    65536,
                ),
                &cancel,
                None,
            )?;
            if !filters.success() || filters.stdout.is_empty() {
                return Err(Error::Code("FileLfsSetup"));
            }
        }
        let mut changed = false;
        let mut warning = None;
        let mut output = String::new();
        let mut output_truncated = false;
        let result = (|| -> Result<(), Error> {
            append(root, relative, &rules, &mut changed).map_err(|e| match e {
                Error::Io(_) => Error::Code("RustFileRulesWriteFailed"),
                other => other,
            })?;
            let args = if action.ignore() {
                vec!["check-ignore", "--stdin", "-z"]
            } else {
                vec![
                    "--literal-pathspecs",
                    "check-attr",
                    "--stdin",
                    "-z",
                    "filter",
                ]
            };
            let mut request = self.request(root, args, Lifetime::Read, LIMIT);
            request.input = files::input(&paths)?;
            let result = process::run(&request, &cancel, None);
            let matches = match result {
                Ok(r) => {
                    output = redact(&String::from_utf8_lossy(&r.stderr));
                    output_truncated = r.outcome == process::Outcome::OutputLimit;
                    if r.outcome != process::Outcome::Exited
                        || r.exit_code > u32::from(action.ignore())
                    {
                        false
                    } else if let Ok(text) = std::str::from_utf8(&r.stdout) {
                        if action.ignore() {
                            let matched = text.split_terminator('\0').collect::<BTreeSet<_>>();
                            text.ends_with('\0')
                                && paths.iter().all(|p| matched.contains(p.as_str()))
                        } else {
                            let fields = text.split_terminator('\0').collect::<Vec<_>>();
                            text.ends_with('\0')
                                && fields.len() == paths.len() * 3
                                && fields.as_chunks::<3>().0.iter().zip(&paths).all(|(f, p)| {
                                    f[0] == p
                                        && f[1] == "filter"
                                        && (f[2] == "lfs") == (action == Action::LfsTrack)
                                })
                        }
                    } else {
                        false
                    }
                }
                Err(_) => false,
            };
            if !matches {
                warning = Some(if action.ignore() {
                    "FileIgnoreInherited"
                } else {
                    "FileLfsInherited"
                });
            }
            Ok(())
        })();
        Ok(Completed {
            commands: vec![],
            output,
            output_truncated,
            message: result.err().map(|e| e.code()),
            warning,
            saved_stash: None,
            filesystem_changes: usize::from(changed),
            updated: self.working_copy(root, &cancel),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn extensions_and_control_character_rejection_match_rule_contract() {
        assert_eq!(extension("folder.name/no-extension"), None);
        assert_eq!(extension("folder/.hidden"), Some(".hidden"));
        assert_eq!(extension("folder/file."), None);
        assert_eq!(extension("folder/file.tar.gz"), Some(".gz"));
        assert_eq!(
            escape_ignore("!#space [x].dat").unwrap(),
            "\\!\\#space\\ \\[x\\].dat"
        );
        assert_eq!(
            attribute_pattern("space [x].dat").unwrap(),
            "\"/space \\\\[x\\\\].dat\""
        );
        assert!(matches!(
            escape_ignore("bad\npath"),
            Err(Error::Code("FileRulePathUnsupported"))
        ));
        assert!(matches!(
            attribute_pattern("bad\tpath"),
            Err(Error::Code("FileRulePathUnsupported"))
        ));
    }
}
