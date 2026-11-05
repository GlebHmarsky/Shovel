//! Bounded comparisons and exact whole-hunk capabilities. A preview is created
//! only from native Git output; UI selection never supplies edited patch text.
use crate::{
    git::{self, Git},
    process::{self, Cancellation, Lifetime},
    working::{Change, Context, Error, Group, path},
};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    ffi::OsString,
    fs::{self, OpenOptions},
    io::Read,
    os::windows::fs::{MetadataExt, OpenOptionsExt},
    path::Path,
    sync::Arc,
};

pub const MAX_BYTES: usize = 2 * 1024 * 1024;
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Options {
    pub ignore_whitespace: bool,
    pub whole_file: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Text,
    Binary,
    Submodule,
    Directory,
    UnsupportedPath,
    UnsupportedEncoding,
    TooLarge,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineKind {
    Metadata,
    Hunk,
    Context,
    Added,
    Removed,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Line {
    pub kind: LineKind,
    pub text: String,
    pub old: Option<u32>,
    pub new: Option<u32>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Summary {
    pub bytes: Option<u64>,
    pub lfs: bool,
    pub binary: bool,
    pub absent: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HunkAvailability {
    Available,
    ChangeType,
    PreviewOptions,
    ContentType,
    Filtered,
    NoChanges,
}
#[derive(Clone, Debug)]
pub struct FileDiff {
    pub kind: Kind,
    pub lines: Vec<Line>,
    pub old: Summary,
    pub new: Summary,
    pub added: u32,
    pub removed: u32,
    pub availability: HunkAvailability,
    pub hunks: Option<Arc<HunkPreview>>,
    pub(crate) patch: String,
}
#[derive(Clone, Debug)]
pub struct CommitFiles {
    pub commit: String,
    pub parent: Option<String>,
    pub entries: Vec<Change>,
}
impl FileDiff {
    fn empty(kind: Kind, old: Summary, new: Summary) -> Self {
        Self {
            kind,
            lines: vec![],
            old,
            new,
            added: 0,
            removed: 0,
            availability: HunkAvailability::ContentType,
            hunks: None,
            patch: String::new(),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hunk {
    pub id: usize,
    pub start: usize,
    pub count: usize,
    pub header: String,
}
#[derive(Clone, Debug)]
pub struct HunkPreview {
    pub(crate) context: Context,
    pub(crate) change: Change,
    pub(crate) patch: String,
    pub(crate) content_stamp: String,
    pub(crate) attribute_stamp: String,
    hunks: Vec<Hunk>,
}
impl HunkPreview {
    pub fn hunks(&self) -> &[Hunk] {
        &self.hunks
    }
    pub fn staged(&self) -> bool {
        self.change.group == Group::Staged
    }
    pub(crate) fn select(&self, ids: &[usize]) -> Result<Vec<u8>, Error> {
        let ids = ids.iter().copied().collect::<BTreeSet<_>>();
        if ids.is_empty() || ids.iter().any(|id| *id >= self.hunks.len()) {
            return Err(Error::HunkSelection);
        }
        let lines = self.patch.split('\n').collect::<Vec<_>>();
        let mut patch = String::new();
        for line in &lines[..self.hunks[0].start] {
            patch.push_str(line);
            patch.push('\n');
        }
        for id in ids {
            let hunk = &self.hunks[id];
            for line in &lines[hunk.start..hunk.start + hunk.count] {
                patch.push_str(line);
                patch.push('\n');
            }
        }
        Ok(patch.into_bytes())
    }
}
pub(crate) fn candidate(entry: &Change) -> bool {
    entry.code == 'M'
        && entry.original.is_none()
        && matches!(entry.group, Group::Staged | Group::Unstaged)
        && matches!(entry.index_mode.as_str(), "100644" | "100755")
        && entry.head_mode == entry.index_mode
}
fn hash(bytes: &[u8]) -> String {
    format!("{:X}", Sha256::digest(bytes))
}
fn object_id(id: &str) -> bool {
    matches!(id.len(), 40 | 64) && id.bytes().all(|b| b.is_ascii_hexdigit())
}
pub(crate) fn has_reparse(root: &Path, file: &Path) -> std::io::Result<bool> {
    let mut current = file;
    while current != root {
        match fs::symlink_metadata(current) {
            Ok(m) => {
                if m.file_attributes() & 0x400 != 0 {
                    return Ok(true);
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
        current = current.parent().ok_or(std::io::ErrorKind::InvalidInput)?;
    }
    Ok(false)
}
pub(crate) fn content_stamp(
    root: &Path,
    relative: &str,
    cancel: &Cancellation,
) -> Result<String, Error> {
    let file = path(root, relative)?;
    if has_reparse(root, &file)? || file.is_dir() {
        return Err(Error::HunkPath);
    }
    let mut file = match OpenOptions::new()
        .read(true)
        .share_mode(1 | 2 | 4)
        .open(&file)
    {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok("missing".into()),
        Err(e) => return Err(e.into()),
    };
    let mut digest = Sha256::new();
    let mut buffer = [0u8; 65536];
    loop {
        if cancel.is_cancelled() {
            return Err(Error::Io(std::io::ErrorKind::Interrupted.into()));
        }
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
    }
    Ok(format!("{:X}", digest.finalize()))
}
fn file_stamp(file: &Path) -> std::io::Result<Option<(u64, u64)>> {
    match fs::metadata(file) {
        Ok(m) => Ok(Some((m.len(), m.last_write_time()))),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}
fn lfs_size(bytes: &[u8]) -> Option<u64> {
    if bytes.is_empty() || bytes.len() >= 1024 {
        return None;
    }
    let text = std::str::from_utf8(bytes).ok()?;
    let lines = text.split('\n').collect::<Vec<_>>();
    if lines.len() < 4
        || lines[0] != "version https://git-lfs.github.com/spec/v1"
        || lines.last() != Some(&"")
    {
        return None;
    }
    let mut keys = BTreeSet::new();
    let mut size = None;
    let mut oid = false;
    for line in &lines[1..lines.len() - 1] {
        let (key, value) = line.split_once(' ')?;
        if key.is_empty() || line.contains('\r') || !keys.insert(key) {
            return None;
        }
        if key == "oid" {
            oid = value.strip_prefix("sha256:").is_some_and(|s| {
                s.len() == 64
                    && s.bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            });
        }
        if key == "size" {
            if value.is_empty() || !value.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            size = Some(value.parse::<u64>().ok()?);
        }
    }
    if oid { size } else { None }
}
impl Git {
    pub fn commit_files(
        &self,
        root: &Path,
        commit: &str,
        parent: Option<&str>,
        cancel: &Cancellation,
    ) -> Result<CommitFiles, Error> {
        if !object_id(commit) || parent.is_some_and(|p| !object_id(p)) {
            return Err(Error::Invalid);
        }
        if parent.is_none() {
            let raw = self
                .read(root, ["cat-file", "-p", commit], cancel, MAX_BYTES)?
                .stdout;
            let header = std::str::from_utf8(&raw)
                .map_err(|_| Error::Invalid)?
                .split("\n\n")
                .next()
                .unwrap_or_default();
            if header.lines().any(|l| l.starts_with("parent ")) {
                return Err(Error::Stale);
            }
        }
        let mut args = [
            "diff-tree",
            "--root",
            "--no-commit-id",
            "-r",
            "--raw",
            "-z",
            "--no-abbrev",
            "--find-renames",
            "--no-ext-diff",
            "--no-textconv",
            "--ignore-submodules=dirty",
        ]
        .map(OsString::from)
        .to_vec();
        args.extend(parent.map(OsString::from));
        args.push(commit.into());
        args.push("--".into());
        let bytes = self.read(root, args, cancel, 4 * 1024 * 1024)?.stdout;
        let text = std::str::from_utf8(&bytes).map_err(|_| Error::Invalid)?;
        if !text.is_empty() && !text.ends_with('\0') {
            return Err(Error::Invalid);
        }
        let records = text.split_terminator('\0').collect::<Vec<_>>();
        let mut i = 0;
        let mut entries = vec![];
        while i < records.len() {
            let fields = records[i].split(' ').collect::<Vec<_>>();
            if fields.len() != 5
                || !fields[0].starts_with(':')
                || fields[0].len() != 7
                || fields[1].len() != 6
                || !object_id(fields[2])
                || !object_id(fields[3])
            {
                return Err(Error::Invalid);
            }
            let code = fields[4]
                .chars()
                .next()
                .filter(|c| "MADRCT".contains(*c))
                .ok_or(Error::Invalid)?;
            i += 1;
            let mut name = *records.get(i).ok_or(Error::Invalid)?;
            i += 1;
            let original = if matches!(code, 'R' | 'C') {
                let old = Some(name.into());
                name = *records.get(i).ok_or(Error::Invalid)?;
                i += 1;
                old
            } else {
                None
            };
            if name.is_empty() || entries.len() >= 20000 {
                return Err(Error::Limit);
            }
            entries.push(Change {
                group: Group::Staged,
                path: name.into(),
                original,
                code,
                submodule: "N...".into(),
                head_object: fields[2].into(),
                index_object: fields[3].into(),
                head_mode: fields[0][1..].into(),
                index_mode: fields[1].into(),
                worktree_mode: String::new(),
            });
        }
        Ok(CommitFiles {
            commit: commit.into(),
            parent: parent.map(str::to_owned),
            entries,
        })
    }
    pub fn commit_file_diff(
        &self,
        root: &Path,
        comparison: &CommitFiles,
        entry: &Change,
        cancel: &Cancellation,
    ) -> Result<FileDiff, Error> {
        if !object_id(&comparison.commit)
            || comparison.parent.as_deref().is_some_and(|p| !object_id(p))
            || !comparison.entries.contains(entry)
        {
            return Err(Error::Invalid);
        }
        let mut args = [
            "--literal-pathspecs",
            "-c",
            "core.quotepath=false",
            "diff-tree",
            "--root",
            "--no-commit-id",
            "-r",
        ]
        .map(OsString::from)
        .to_vec();
        args.extend(common_args());
        args.extend(comparison.parent.iter().map(OsString::from));
        args.push(comparison.commit.clone().into());
        let mut paths = vec![entry.path.clone()];
        paths.extend(entry.original.iter().cloned());
        let old = if entry.head_mode == "160000" {
            Summary::default()
        } else {
            self.blob_summary(root, &entry.head_object, cancel)?
        };
        let new = if entry.index_mode == "160000" {
            Summary::default()
        } else {
            self.blob_summary(root, &entry.index_object, cancel)?
        };
        self.read_comparison(
            root,
            args,
            paths,
            old,
            new,
            entry.head_mode == "160000" || entry.index_mode == "160000",
            Options::default(),
            false,
            cancel,
        )
    }
    fn blob_summary(
        &self,
        root: &Path,
        object: &str,
        cancel: &Cancellation,
    ) -> Result<Summary, Error> {
        if object.is_empty() {
            return Ok(Summary::default());
        }
        if !object_id(object) {
            return Err(Error::Invalid);
        }
        if object.bytes().all(|b| b == b'0') {
            return Ok(Summary {
                bytes: Some(0),
                absent: true,
                ..Default::default()
            });
        }
        let result = self.read(root, ["cat-file", "-s", object], cancel, 128)?;
        let size = std::str::from_utf8(&result.stdout)
            .map_err(|_| Error::Invalid)?
            .trim()
            .parse::<u64>()
            .map_err(|_| Error::Invalid)?;
        let mut summary = Summary {
            bytes: Some(size),
            ..Default::default()
        };
        if size > 0 && size < 1024 {
            let result = self.read(root, ["cat-file", "blob", object], cancel, 1024)?;
            let logical = lfs_size(&result.stdout);
            summary.lfs = logical.is_some();
            summary.binary = summary.lfs || result.stdout.contains(&0);
            summary.bytes = Some(logical.unwrap_or(size));
        }
        Ok(summary)
    }
    pub(crate) fn hunk_attributes(
        &self,
        root: &Path,
        relative: &str,
        cancel: &Cancellation,
    ) -> Result<(bool, String), Error> {
        let output = self
            .read(
                root,
                [
                    "--literal-pathspecs",
                    "check-attr",
                    "-z",
                    "filter",
                    "working-tree-encoding",
                    "ident",
                    "text",
                    "eol",
                    "--",
                    relative,
                ],
                cancel,
                256 * 1024,
            )?
            .stdout;
        let text = std::str::from_utf8(&output).map_err(|_| Error::Invalid)?;
        let fields = text.split('\0').collect::<Vec<_>>();
        if fields.len() != 16 || !fields[15].is_empty() {
            return Err(Error::Invalid);
        }
        let mut supported = true;
        for triple in fields[..15].as_chunks::<3>().0 {
            if triple[0] != relative {
                return Err(Error::Invalid);
            }
            if matches!(triple[1], "filter" | "working-tree-encoding" | "ident")
                && !matches!(triple[2], "unspecified" | "unset")
            {
                supported = false;
            }
        }
        Ok((supported, hash(&output)))
    }
    pub fn file_diff(
        &self,
        context: &Context,
        entry: &Change,
        options: Options,
        cancel: &Cancellation,
    ) -> Result<FileDiff, Error> {
        if !context.entries.contains(entry) {
            return Err(Error::Stale);
        }
        let root = &context.repository.worktree;
        let file = path(root, &entry.path)?;
        let staged = entry.group == Group::Staged;
        let new = matches!(entry.group, Group::Untracked | Group::Ignored);
        let submodule = entry.head_mode == "160000" || entry.index_mode == "160000";
        if !staged && has_reparse(root, &file)? {
            return Ok(FileDiff::empty(
                Kind::UnsupportedPath,
                Summary::default(),
                Summary::default(),
            ));
        }
        if !staged && file.is_dir() && !submodule {
            return Ok(FileDiff::empty(
                Kind::Directory,
                Summary::default(),
                Summary::default(),
            ));
        }
        if self.semantic_index(&context.repository, cancel)? != context.index_stamp {
            return Err(Error::Stale);
        }
        let before = file_stamp(&file)?;
        let old = if submodule {
            Summary::default()
        } else if new {
            Summary {
                bytes: Some(0),
                absent: true,
                ..Default::default()
            }
        } else {
            self.blob_summary(
                root,
                if staged {
                    &entry.head_object
                } else {
                    &entry.index_object
                },
                cancel,
            )?
        };
        let mut new_info = if submodule {
            Summary::default()
        } else if staged {
            self.blob_summary(root, &entry.index_object, cancel)?
        } else {
            worktree_summary(&file)?
        };
        if !staged && old.lfs && !new_info.absent {
            new_info.lfs = true;
        }
        let hunk_candidate = candidate(entry) && !options.ignore_whitespace && !options.whole_file;
        let content = if hunk_candidate {
            content_stamp(root, &entry.path, cancel)?
        } else {
            String::new()
        };
        let mut result = if entry.group != Group::Conflicted && (old.binary || new_info.binary) {
            FileDiff::empty(Kind::Binary, old, new_info)
        } else {
            let mut args = context.filters.clone();
            args.extend(
                ["--literal-pathspecs", "-c", "core.quotepath=false", "diff"].map(OsString::from),
            );
            args.extend(common_args());
            if new {
                args.push("--no-index".into());
            } else if staged {
                args.push("--cached".into());
                args.extend(context.repository.head.iter().map(OsString::from));
            }
            if options.ignore_whitespace {
                args.push("--ignore-all-space".into());
            }
            let mut paths = vec![];
            if new {
                paths.push("/dev/null".into());
            }
            paths.push(entry.path.clone());
            if staged && let Some(old) = &entry.original {
                paths.push(old.clone());
            }
            self.read_comparison(
                root, args, paths, old, new_info, submodule, options, new, cancel,
            )?
        };
        if self.semantic_index(&context.repository, cancel)? != context.index_stamp
            || !staged && file_stamp(&file)? != before
        {
            return Err(Error::Stale);
        }
        result.availability = if result.kind != Kind::Text {
            HunkAvailability::ContentType
        } else if !candidate(entry) {
            HunkAvailability::ChangeType
        } else if options.ignore_whitespace || options.whole_file {
            HunkAvailability::PreviewOptions
        } else {
            HunkAvailability::NoChanges
        };
        if hunk_candidate && result.kind == Kind::Text && !result.patch.is_empty() {
            if content_stamp(root, &entry.path, cancel)? != content {
                return Err(Error::HunkStale);
            }
            let (supported, attribute_stamp) = self.hunk_attributes(root, &entry.path, cancel)?;
            let hunks = parse_hunks(&result.patch);
            result.availability = if !supported {
                HunkAvailability::Filtered
            } else if hunks.is_none() {
                HunkAvailability::ChangeType
            } else if hunks.as_ref().unwrap().is_empty() {
                HunkAvailability::NoChanges
            } else {
                HunkAvailability::Available
            };
            if result.availability == HunkAvailability::Available {
                result.hunks = Some(Arc::new(HunkPreview {
                    context: context.clone(),
                    change: entry.clone(),
                    patch: result.patch.clone(),
                    content_stamp: content,
                    attribute_stamp,
                    hunks: hunks.unwrap(),
                }));
            }
        }
        Ok(result)
    }
    #[allow(clippy::too_many_arguments)]
    fn read_comparison(
        &self,
        root: &Path,
        args: Vec<OsString>,
        paths: Vec<String>,
        old: Summary,
        new: Summary,
        submodule: bool,
        options: Options,
        allow_one: bool,
        cancel: &Cancellation,
    ) -> Result<FileDiff, Error> {
        let mut stat = args.clone();
        stat.extend(["--numstat", "-z", "--"].map(OsString::from));
        stat.extend(paths.iter().map(OsString::from));
        let output = process::run(
            &self.request(root, stat, Lifetime::Read, MAX_BYTES),
            cancel,
            None,
        )?;
        let stat = diff_require(output, allow_one)?;
        let (binary, added, removed) = parse_stats(&stat.stdout)?;
        let mut diff = FileDiff::empty(
            if binary || old.lfs || new.lfs {
                Kind::Binary
            } else if submodule {
                Kind::Submodule
            } else {
                Kind::Text
            },
            old,
            new,
        );
        diff.added = added;
        diff.removed = removed;
        if diff.kind == Kind::Binary {
            return Ok(diff);
        }
        let mut patch = args;
        patch.push("--patch".into());
        patch.push(
            if options.whole_file {
                "--unified=20000"
            } else {
                "--unified=3"
            }
            .into(),
        );
        patch.push("--".into());
        patch.extend(paths.into_iter().map(OsString::from));
        let output = process::run(
            &self.request(root, patch, Lifetime::Read, MAX_BYTES),
            cancel,
            None,
        )?;
        if output.outcome == process::Outcome::OutputLimit {
            diff.kind = Kind::TooLarge;
            return Ok(diff);
        }
        let output = diff_require(output, allow_one)?;
        match String::from_utf8(output.stdout) {
            Ok(patch) => match parse_lines(&patch) {
                Ok(lines) => {
                    diff.patch = patch;
                    diff.lines = lines;
                }
                Err(Error::Limit) => diff.kind = Kind::TooLarge,
                Err(e) => return Err(e),
            },
            Err(_) => diff.kind = Kind::UnsupportedEncoding,
        }
        Ok(diff)
    }
}
fn diff_require(result: process::Result, allow_one: bool) -> Result<process::Result, Error> {
    if result.success()
        || allow_one && result.outcome == process::Outcome::Exited && result.exit_code == 1
    {
        Ok(result)
    } else {
        Ok(git::require(result)?)
    }
}
fn common_args() -> Vec<OsString> {
    [
        "--no-ext-diff",
        "--no-textconv",
        "--no-color",
        "--find-renames",
        "--submodule=short",
        "--ignore-submodules=dirty",
        "--src-prefix=a/",
        "--dst-prefix=b/",
        "--no-relative",
        "--inter-hunk-context=0",
        "--output-indicator-new=+",
        "--output-indicator-old=-",
        "--output-indicator-context= ",
    ]
    .map(OsString::from)
    .to_vec()
}
fn worktree_summary(file: &Path) -> Result<Summary, Error> {
    let mut stream = match OpenOptions::new()
        .read(true)
        .share_mode(1 | 2 | 4)
        .open(file)
    {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(Summary {
                bytes: Some(0),
                absent: true,
                ..Default::default()
            });
        }
        Err(e) => return Err(e.into()),
    };
    let size = stream.metadata()?.len();
    let mut prefix = vec![0u8; size.min(8192) as usize];
    stream.read_exact(&mut prefix)?;
    let logical = if size < 1024 { lfs_size(&prefix) } else { None };
    Ok(Summary {
        bytes: Some(logical.unwrap_or(size)),
        lfs: logical.is_some(),
        binary: logical.is_some() || prefix.contains(&0),
        absent: false,
    })
}
fn parse_stats(bytes: &[u8]) -> Result<(bool, u32, u32), Error> {
    let text = std::str::from_utf8(bytes).map_err(|_| Error::Invalid)?;
    if !text.is_empty() && !text.ends_with('\0') {
        return Err(Error::Invalid);
    }
    let records = text.split_terminator('\0').collect::<Vec<_>>();
    let (mut binary, mut added, mut removed, mut i) = (false, 0u32, 0u32, 0usize);
    while i < records.len() {
        let fields = records[i].splitn(3, '\t').collect::<Vec<_>>();
        if fields.len() != 3 {
            return Err(Error::Invalid);
        }
        if fields[0] == "-" && fields[1] == "-" {
            binary = true;
        } else {
            added = added
                .checked_add(fields[0].parse::<u32>().map_err(|_| Error::Invalid)?)
                .ok_or(Error::Limit)?;
            removed = removed
                .checked_add(fields[1].parse::<u32>().map_err(|_| Error::Invalid)?)
                .ok_or(Error::Limit)?;
        }
        i += 1;
        if fields[2].is_empty() {
            if i + 1 >= records.len() {
                return Err(Error::Invalid);
            }
            i += 2;
        }
    }
    Ok((binary, added, removed))
}
fn range(value: &str, prefix: char) -> Option<(u32, u32)> {
    let value = value.strip_prefix(prefix)?;
    let (start, count) = value.split_once(',').unwrap_or((value, "1"));
    Some((start.parse().ok()?, count.parse().ok()?))
}
fn header(line: &str) -> Option<((u32, u32), (u32, u32))> {
    let tail = line.strip_prefix("@@ ")?;
    let (numbers, _) = tail.split_once(" @@")?;
    let (old, new) = numbers.split_once(' ')?;
    Some((range(old, '-')?, range(new, '+')?))
}
pub fn parse_lines(patch: &str) -> Result<Vec<Line>, Error> {
    if patch.len() > MAX_BYTES {
        return Err(Error::Limit);
    }
    let (mut old, mut new) = (None, None);
    let mut lines = vec![];
    for text in patch.split('\n') {
        if lines.len() >= 20000 || text.encode_utf16().count() > 16384 {
            return Err(Error::Limit);
        }
        let mut row = Line {
            kind: LineKind::Metadata,
            text: text.into(),
            old: None,
            new: None,
        };
        if text.starts_with("@@") {
            let counts = header(text);
            old = counts.map(|p| p.0.0);
            new = counts.map(|p| p.1.0);
            row.kind = LineKind::Hunk;
        } else if old.is_some() && new.is_some() {
            match text.as_bytes().first() {
                Some(b'+') => {
                    row.kind = LineKind::Added;
                    row.new = new;
                    new = new.and_then(|n| n.checked_add(1));
                    row.text = text[1..].into();
                }
                Some(b'-') => {
                    row.kind = LineKind::Removed;
                    row.old = old;
                    old = old.and_then(|n| n.checked_add(1));
                    row.text = text[1..].into();
                }
                Some(b' ') => {
                    row.kind = LineKind::Context;
                    row.old = old;
                    row.new = new;
                    old = old.and_then(|n| n.checked_add(1));
                    new = new.and_then(|n| n.checked_add(1));
                    row.text = text[1..].into();
                }
                _ => {
                    if text.starts_with("diff ") {
                        old = None;
                        new = None;
                    }
                }
            }
        }
        lines.push(row);
    }
    Ok(lines)
}
fn parse_hunks(patch: &str) -> Option<Vec<Hunk>> {
    let lines = patch.split('\n').collect::<Vec<_>>();
    if lines.len() < 6 || lines.last() != Some(&"") || !lines[0].starts_with("diff --git ") {
        return None;
    }
    let fields = lines[1].split(' ').collect::<Vec<_>>();
    if fields.len() != 3 || fields[0] != "index" || !matches!(fields[2], "100644" | "100755") {
        return None;
    }
    let (old, new) = fields[1].split_once("..")?;
    if old.is_empty()
        || new.is_empty()
        || !old
            .bytes()
            .chain(new.bytes())
            .all(|b| b.is_ascii_hexdigit())
    {
        return None;
    }
    if !(lines[2].starts_with("--- a/") || lines[2].starts_with("--- \"a/"))
        || !(lines[3].starts_with("+++ b/") || lines[3].starts_with("+++ \"b/"))
    {
        return None;
    }
    let mut hunks = vec![];
    let mut i = 4;
    while i < lines.len() - 1 {
        let start = i;
        let expected = header(lines[i])?;
        i += 1;
        let (mut old, mut new, mut changed) = (0u32, 0u32, false);
        while i < lines.len() - 1 && !lines[i].starts_with("@@") {
            match lines[i].as_bytes().first() {
                Some(b' ') => {
                    old += 1;
                    new += 1;
                }
                Some(b'+') => {
                    new += 1;
                    changed = true;
                }
                Some(b'-') => {
                    old += 1;
                    changed = true;
                }
                Some(b'\\') if lines[i] == "\\ No newline at end of file" => {}
                _ => return None,
            }
            i += 1;
        }
        if !changed || old != expected.0.1 || new != expected.1.1 {
            return None;
        }
        hunks.push(Hunk {
            id: hunks.len(),
            start,
            count: i - start,
            header: lines[start].into(),
        });
    }
    Some(hunks)
}
