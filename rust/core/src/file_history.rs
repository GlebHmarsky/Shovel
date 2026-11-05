//! Immutable captured-tip file inspection. Reads never consult a newer HEAD,
//! change the index/worktree, run textconv, or fetch missing objects.
use crate::{
    git::{self, Commit, Git},
    identity,
    operation::Error,
    process::Cancellation,
    working::{self, Change, Context},
};
use chrono::{DateTime, Utc};
use std::ffi::OsString;
pub const PAGE_SIZE: usize = 200;
pub const MAX_BYTES: usize = 4 * 1024 * 1024;
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cursor {
    pub hash: String,
    pub path: String,
    pub offset: usize,
}
#[derive(Clone, Debug)]
pub struct Entry {
    pub commit: Commit,
    pub path: String,
}
#[derive(Clone, Debug)]
pub struct Page {
    pub entries: Vec<Entry>,
    pub next: Option<Cursor>,
    pub offset: usize,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlameLine {
    pub hash: String,
    pub author: String,
    pub email: String,
    pub date: DateTime<Utc>,
    pub number: usize,
    pub text: String,
    pub summary: String,
}
fn invalid() -> Error {
    Error::Code("GitErrorInvalidOutput")
}
fn oid(s: &str) -> bool {
    matches!(s.len(), 40 | 64) && s.bytes().all(|b| b.is_ascii_hexdigit())
}
fn scope(entry: &Change) -> &str {
    if entry.code == 'R' {
        entry.original.as_deref().unwrap_or(&entry.path)
    } else {
        &entry.path
    }
}
fn validate(context: &Context, entry: &Change, cancel: &Cancellation) -> Result<(), Error> {
    if cancel.is_cancelled() {
        return Err(Error::Code("ActionResultCancelled"));
    }
    if !context.entries.contains(entry) {
        return Err(Error::Code("CommitStale"));
    }
    working::path(&context.repository.worktree, &entry.path)?;
    working::path(&context.repository.worktree, scope(entry))?;
    if identity::read(&context.repository.worktree)? != context.repository.identity {
        return Err(Error::Code("ActionIdentityChanged"));
    }
    Ok(())
}
impl Git {
    pub fn file_history(
        &self,
        context: &Context,
        entry: &Change,
        cursor: Option<&Cursor>,
        cancel: &Cancellation,
    ) -> Result<Page, Error> {
        validate(context, entry, cancel)?;
        let offset = cursor.map_or(0, |c| c.offset);
        if cursor
            .is_some_and(|c| !oid(&c.hash) || c.offset == 0 || !c.offset.is_multiple_of(PAGE_SIZE))
        {
            return Err(invalid());
        }
        let Some(head) = context.repository.head.as_deref() else {
            return Ok(Page {
                entries: vec![],
                next: None,
                offset,
            });
        };
        let relative = cursor.map_or_else(|| scope(entry), |c| c.path.as_str());
        working::path(&context.repository.worktree, relative)?;
        let mut args = [
            "--literal-pathspecs",
            "-c",
            "core.fsmonitor=false",
            "log",
            "--first-parent",
            "--follow",
            "--no-color",
            "--no-decorate",
            "--no-show-signature",
            "--no-notes",
            "--no-use-mailmap",
            "--name-status",
            "--no-ext-diff",
            "--no-textconv",
            "--encoding=UTF-8",
            "-z",
            "--format=tformat:%H%x00%P%x00%an%x00%ae%x00%aI%x00%cn%x00%ce%x00%cI%x00%B",
            "--max-count=201",
        ]
        .map(OsString::from)
        .to_vec();
        args.extend([
            cursor.map_or(head, |c| c.hash.as_str()).into(),
            "--".into(),
            relative.into(),
        ]);
        let result = self.read(&context.repository.worktree, args, cancel, MAX_BYTES)?;
        let page = parse_history(&result.stdout, relative, offset)?;
        if cursor.is_some_and(|c| {
            page.entries
                .first()
                .is_none_or(|first| first.commit.hash != c.hash)
        }) {
            return Err(invalid());
        }
        validate(context, entry, cancel)?;
        Ok(page)
    }
    pub fn file_blame(
        &self,
        context: &Context,
        entry: &Change,
        cancel: &Cancellation,
    ) -> Result<Vec<BlameLine>, Error> {
        validate(context, entry, cancel)?;
        let head = context
            .repository
            .head
            .as_deref()
            .ok_or(Error::Code("FileBlameNoHead"))?;
        let result = self.read(
            &context.repository.worktree,
            [
                "--literal-pathspecs",
                "-c",
                "core.fsmonitor=false",
                "blame",
                "--line-porcelain",
                "--no-textconv",
                "--encoding=UTF-8",
                "--root",
                head,
                "--",
                scope(entry),
            ],
            cancel,
            MAX_BYTES,
        )?;
        let lines = parse_blame(&result.stdout)?;
        validate(context, entry, cancel)?;
        Ok(lines)
    }
}
fn parse_history(bytes: &[u8], relative: &str, offset: usize) -> Result<Page, Error> {
    if bytes.len() > MAX_BYTES {
        return Err(Error::Code("GitErrorOutputLimit"));
    }
    let text = std::str::from_utf8(bytes).map_err(|_| invalid())?;
    if !text.is_empty() && !text.ends_with('\0') {
        return Err(invalid());
    }
    let fields = text.split_terminator('\0').collect::<Vec<_>>();
    let mut i = 0;
    let mut entries = vec![];
    let mut next = None;
    let mut relative = relative.to_owned();
    while i < fields.len() {
        if fields.len() - i < 9 || entries.len() > PAGE_SIZE {
            return Err(invalid());
        }
        let raw = fields[i..i + 9].join("\0") + "\0";
        let commit = git::parse_commits(raw.as_bytes(), 1)?
            .pop()
            .ok_or_else(invalid)?;
        i += 9;
        if entries.len() == PAGE_SIZE {
            next = Some(Cursor {
                hash: commit.hash.clone(),
                path: relative.clone(),
                offset: offset.checked_add(PAGE_SIZE).ok_or_else(invalid)?,
            });
        }
        entries.push(Entry {
            commit,
            path: relative.clone(),
        });
        while i < fields.len() && !oid(fields[i]) {
            let status = fields[i].trim_start_matches('\n');
            i += 1;
            if status.is_empty() {
                continue;
            }
            let code = status.as_bytes()[0];
            if !b"MADRTCU".contains(&code) || i >= fields.len() {
                return Err(invalid());
            }
            let first = fields[i];
            i += 1;
            if first.is_empty() {
                return Err(invalid());
            }
            if code == b'R' || code == b'C' {
                let second = fields.get(i).ok_or_else(invalid)?;
                i += 1;
                if second.is_empty() {
                    return Err(invalid());
                }
                if relative == *second {
                    relative = first.into();
                }
            }
        }
    }
    entries.truncate(PAGE_SIZE);
    Ok(Page {
        entries,
        next,
        offset,
    })
}
pub fn parse_blame(bytes: &[u8]) -> Result<Vec<BlameLine>, Error> {
    if bytes.len() > MAX_BYTES {
        return Err(Error::Code("GitErrorOutputLimit"));
    }
    let text = std::str::from_utf8(bytes).map_err(|_| Error::Code("FileBlameBinary"))?;
    if text.contains('\0') {
        return Err(Error::Code("FileBlameBinary"));
    }
    if !text.is_empty() && !text.ends_with('\n') {
        return Err(invalid());
    }
    let mut lines = text.split_terminator('\n');
    let mut rows = vec![];
    while let Some(header) = lines.next() {
        let header = header.split(' ').collect::<Vec<_>>();
        if !(3..=4).contains(&header.len()) || !oid(header[0]) {
            return Err(invalid());
        }
        let number = header[2].parse::<usize>().map_err(|_| invalid())?;
        if number == 0
            || header[1].parse::<usize>().ok().filter(|n| *n > 0).is_none()
            || header
                .get(3)
                .is_some_and(|n| n.parse::<usize>().ok().filter(|n| *n > 0).is_none())
        {
            return Err(invalid());
        }
        let (mut author, mut email, mut seconds, mut summary) = (None, None, None, None);
        let mut content = None;
        for line in lines.by_ref() {
            if let Some(value) = line.strip_prefix('\t') {
                content = Some(value);
                break;
            }
            if let Some(value) = line.strip_prefix("author ") {
                author = Some(value);
            } else if let Some(value) = line.strip_prefix("author-mail ") {
                email = Some(value.trim_matches(['<', '>']));
            } else if let Some(value) = line.strip_prefix("author-time ") {
                seconds = Some(value.parse::<i64>().map_err(|_| invalid())?);
            } else if let Some(value) = line.strip_prefix("summary ") {
                summary = Some(value);
            }
        }
        let content = content.ok_or_else(invalid)?;
        if rows.len() >= 20000 || content.encode_utf16().count() + 1 > 16384 {
            return Err(Error::Code("GitErrorOutputLimit"));
        }
        rows.push(BlameLine {
            hash: header[0].into(),
            author: author.ok_or_else(invalid)?.into(),
            email: email.ok_or_else(invalid)?.into(),
            date: DateTime::from_timestamp(seconds.ok_or_else(invalid)?, 0).ok_or_else(invalid)?,
            number,
            text: content.into(),
            summary: summary.ok_or_else(invalid)?.into(),
        });
    }
    Ok(rows)
}
