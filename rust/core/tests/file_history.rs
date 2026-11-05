#![cfg(windows)]
use shovel_core::{
    file_history,
    git::Git,
    process::{self, Cancellation, Lifetime},
    working::{Change, Context, Group},
};
use std::{fs, path::PathBuf};
struct Fixture {
    _temp: tempfile::TempDir,
    root: PathBuf,
    git: Git,
}
impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("repo");
        fs::create_dir(&root).unwrap();
        let config = temp.path().join("isolated.config");
        fs::write(&config,"[user]\nname=Fixture\nemail=fixture@example.invalid\n[commit]\ngpgsign=false\n[core]\nautocrlf=false\n").unwrap();
        let mut git = Git::discover(None, &Cancellation::default()).unwrap();
        git.environment
            .insert("GIT_CONFIG_GLOBAL".into(), Some(config.into_os_string()));
        git.environment
            .insert("GIT_CONFIG_NOSYSTEM".into(), Some("1".into()));
        let f = Self { _temp: temp, root, git };
        f.cmd(&["init", "--initial-branch=main"], &[]);
        f
    }
    fn cmd(&self, args: &[&str], input: &[u8]) -> String {
        let mut request = self.git.request(
            &self.root,
            args.iter().copied(),
            Lifetime::AcceptedCommand,
            4 * 1024 * 1024,
        );
        request.input = input.to_vec();
        let result = process::run(&request, &Cancellation::default(), None).unwrap();
        assert!(result.success(), "{result:?}");
        String::from_utf8(result.stdout).unwrap().trim_end().into()
    }
    fn context(&self) -> Context {
        self.git
            .working_copy(&self.root, &Cancellation::default())
            .unwrap()
    }
}
fn selected(context: &Context, path: &str) -> Change {
    context
        .entries
        .iter()
        .find(|e| e.path == path)
        .unwrap()
        .clone()
}

#[test]
fn rename_history_pages_keep_captured_tip_and_per_commit_paths_with_committed_blame() {
    let f = Fixture::new();
    let mut stream = String::new();
    for i in 1..=205 {
        let message = format!("Revision {i}\n\nfull body שלום\n");
        let content = format!("line {i}\n");
        stream.push_str(&format!("commit refs/heads/main\nmark :{i}\ncommitter Author <a@example.invalid> {} +0230\ndata {}\n{message}",1700000000+i,message.len()));
        if i > 1 {
            stream.push_str(&format!("from :{}\n", i - 1));
        }
        stream.push_str(&format!(
            "M 100644 inline old.txt\ndata {}\n{content}\n",
            content.len()
        ));
    }
    stream.push_str("done\n");
    f.cmd(&["fast-import", "--quiet"], stream.as_bytes());
    f.cmd(&["reset", "--hard", "HEAD"], &[]);
    f.cmd(&["mv", "old.txt", "new.txt"], &[]);
    f.cmd(&["commit", "-m", "rename"], &[]);
    fs::write(f.root.join("new.txt"), "uncommitted\n").unwrap();
    let c = f.context();
    let entry = selected(&c, "new.txt");
    let cancel = Cancellation::default();
    let index = fs::read(f.root.join(".git/index")).unwrap();
    let first = f.git.file_history(&c, &entry, None, &cancel).unwrap();
    let next = f
        .git
        .file_history(&c, &entry, first.next.as_ref(), &cancel)
        .unwrap();
    assert_eq!(first.entries.len(), 200);
    assert_eq!(next.entries.len(), 6);
    assert!(next.next.is_none());
    assert_eq!(first.entries[0].path, "new.txt");
    assert!(
        first.entries[1..]
            .iter()
            .chain(&next.entries)
            .all(|e| e.path == "old.txt")
    );
    assert_eq!(first.next.as_ref().unwrap().path, "old.txt");
    assert_eq!(first.next.as_ref().unwrap().offset, 200);
    assert_eq!(
        first
            .entries
            .iter()
            .chain(&next.entries)
            .map(|e| &e.commit.hash)
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        206
    );
    assert!(next.entries[0].commit.message.contains("full body שלום"));
    let blame = f.git.file_blame(&c, &entry, &cancel).unwrap();
    assert_eq!(blame[0].text, "line 205");
    assert_eq!(blame[0].email, "a@example.invalid");
    assert_eq!(blame[0].number, 1);
    assert_eq!(fs::read(f.root.join(".git/index")).unwrap(), index);
    f.cmd(&["commit", "-am", "later tip"], &[]);
    let pinned = f.git.file_history(&c, &entry, None, &cancel).unwrap();
    assert_eq!(pinned.entries[0].commit.hash, first.entries[0].commit.hash);
    assert_eq!(f.git.file_blame(&c, &entry, &cancel).unwrap(), blame);
    let mut bad = first.next.unwrap();
    bad.offset = 199;
    assert!(f.git.file_history(&c, &entry, Some(&bad), &cancel).is_err());
    bad.offset = 200;
    bad.path = "../outside".into();
    assert!(f.git.file_history(&c, &entry, Some(&bad), &cancel).is_err());
}

#[test]
fn first_parent_merge_scope_and_staged_rename_blame_exclude_working_changes() {
    let f = Fixture::new();
    fs::write(f.root.join("file.txt"), "base\n").unwrap();
    fs::write(f.root.join("other.txt"), "base\n").unwrap();
    f.cmd(&["add", "file.txt", "other.txt"], &[]);
    f.cmd(&["commit", "-m", "root"], &[]);
    f.cmd(&["switch", "-c", "side"], &[]);
    fs::write(f.root.join("file.txt"), "side\n").unwrap();
    f.cmd(&["commit", "-am", "side change"], &[]);
    f.cmd(&["switch", "main"], &[]);
    fs::write(f.root.join("other.txt"), "main\n").unwrap();
    f.cmd(&["commit", "-am", "main change"], &[]);
    f.cmd(&["merge", "--no-ff", "side", "-m", "integration"], &[]);
    f.cmd(&["mv", "--", "file.txt", "-literal[1] שלום.txt"], &[]);
    let c = f.context();
    let entry = selected(&c, "-literal[1] שלום.txt");
    assert_eq!(entry.group, Group::Staged);
    assert_eq!(entry.code, 'R');
    let page = f
        .git
        .file_history(&c, &entry, None, &Cancellation::default())
        .unwrap();
    assert_eq!(page.entries.len(), 2);
    assert_eq!(page.entries[0].commit.parents.len(), 2);
    assert!(
        !page
            .entries
            .iter()
            .any(|e| e.commit.subject() == "side change")
    );
    let lines = f
        .git
        .file_blame(&c, &entry, &Cancellation::default())
        .unwrap();
    assert_eq!(lines[0].text, "side");
    assert_eq!(lines[0].summary, "side change");
    let cancelled = Cancellation::default();
    cancelled.cancel();
    assert_eq!(
        f.git
            .file_history(&c, &entry, None, &cancelled)
            .unwrap_err()
            .code(),
        "ActionResultCancelled"
    );
}

#[test]
fn empty_untracked_unborn_and_binary_blame_states_are_explicit() {
    let f = Fixture::new();
    fs::write(f.root.join("new.txt"), "first").unwrap();
    let c = f.context();
    let entry = selected(&c, "new.txt");
    assert!(
        f.git
            .file_history(&c, &entry, None, &Cancellation::default())
            .unwrap()
            .entries
            .is_empty()
    );
    assert_eq!(
        f.git
            .file_blame(&c, &entry, &Cancellation::default())
            .unwrap_err()
            .code(),
        "FileBlameNoHead"
    );
    f.cmd(&["add", "new.txt"], &[]);
    f.cmd(&["commit", "-m", "first"], &[]);
    fs::write(f.root.join("new.txt"), "working").unwrap();
    let c = f.context();
    let entry = selected(&c, "new.txt");
    assert_eq!(
        f.git
            .file_blame(&c, &entry, &Cancellation::default())
            .unwrap()[0]
            .text,
        "first"
    );
    fs::write(f.root.join("binary.dat"), [0, 1, 2, 3]).unwrap();
    f.cmd(&["add", "binary.dat"], &[]);
    f.cmd(&["commit", "-m", "binary"], &[]);
    fs::write(f.root.join("binary.dat"), [0, 4, 5]).unwrap();
    let c = f.context();
    let entry = selected(&c, "binary.dat");
    assert_eq!(
        f.git
            .file_blame(&c, &entry, &Cancellation::default())
            .unwrap_err()
            .code(),
        "FileBlameBinary"
    );
    fs::write(f.root.join("never.txt"), "new").unwrap();
    let c = f.context();
    let entry = selected(&c, "never.txt");
    assert!(
        f.git
            .file_history(&c, &entry, None, &Cancellation::default())
            .unwrap()
            .entries
            .is_empty()
    );
    assert!(
        f.git
            .file_blame(&c, &entry, &Cancellation::default())
            .is_err()
    );
}

#[test]
fn blame_parser_rejects_binary_malformed_truncated_and_overlong_lines() {
    for bytes in [b"\0".as_slice(), &[255]] {
        assert_eq!(
            file_history::parse_blame(bytes).unwrap_err().code(),
            "FileBlameBinary"
        );
    }
    assert!(file_history::parse_blame(b"not a valid header\n").is_err());
    let prefix = format!(
        "{} 1 1 1\nauthor Name\nauthor-mail <a@example.invalid>\nauthor-time 1700000000\nsummary subject\n",
        "1".repeat(40)
    );
    assert!(file_history::parse_blame(prefix.as_bytes()).is_err());
    assert!(file_history::parse_blame((prefix.clone() + "\tmissing newline").as_bytes()).is_err());
    let lines = file_history::parse_blame((prefix.clone() + "\tשלום\r\n").as_bytes()).unwrap();
    assert_eq!(lines[0].text, "שלום\r");
    assert_eq!(
        file_history::parse_blame((prefix + "\t" + &"x".repeat(16384) + "\n").as_bytes())
            .unwrap_err()
            .code(),
        "GitErrorOutputLimit"
    );
}
