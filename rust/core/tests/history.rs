#![cfg(windows)]
use shovel_core::{
    git::{self, Cursor, Git},
    process::{self, Cancellation, Lifetime},
};
use std::{fs, path::Path};

fn command(git: &Git, root: &Path, args: &[&str], input: &[u8]) -> String {
    let mut request = git.request(
        root,
        args.iter().copied(),
        Lifetime::AcceptedCommand,
        4 * 1024 * 1024,
    );
    request.input = input.to_vec();
    let result = process::run(&request, &Cancellation::default(), None).unwrap();
    assert!(result.success(), "{result:?}");
    String::from_utf8(result.stdout).unwrap()
}
fn fixture() -> (tempfile::TempDir, Git) {
    let temp = tempfile::tempdir().unwrap();
    let mut git = Git::discover(None, &Cancellation::default()).unwrap();
    let config = temp.path().join("isolated.gitconfig");
    fs::write(
        &config,
        b"[user]\nname = Fixture\nemail = fixture@example.invalid\n[commit]\ngpgsign = false\n",
    )
    .unwrap();
    git.environment
        .insert("GIT_CONFIG_GLOBAL".into(), Some(config.into_os_string()));
    git.environment
        .insert("GIT_CONFIG_NOSYSTEM".into(), Some("1".into()));
    command(&git, temp.path(), &["init", "--initial-branch=main"], b"");
    (temp, git)
}

#[test]
fn real_git_history_pages_include_old_base_stash_without_duplicates() {
    let (temp, git) = fixture();
    let root = temp.path();
    let mut stream = String::new();
    for index in 1..=205 {
        let message = format!("Commit {index}\n\ncomplete body שלום\n");
        stream.push_str(&format!("commit refs/heads/main\nmark :{index}\ncommitter Fixture <fixture@example.invalid> {} +0000\ndata {}\n{}",1600000000+index,message.len(),message));
        if index == 1 {
            stream.push_str("M 100644 inline tracked.txt\ndata 5\nbase\n");
        }
        stream.push('\n');
    }
    stream.push_str("done\n");
    command(&git, root, &["fast-import", "--quiet"], stream.as_bytes());
    command(&git, root, &["reset", "--hard", "HEAD"], b"");
    let baseline = command(
        &git,
        root,
        &[
            "log",
            "--topo-order",
            "--format=%H",
            "--branches",
            "--remotes",
            "--tags",
            "HEAD",
            "--",
        ],
        b"",
    )
    .lines()
    .map(str::to_owned)
    .collect::<Vec<_>>();
    assert_eq!(baseline.len(), 205);
    command(
        &git,
        root,
        &["checkout", "--detach", baseline.last().unwrap()],
        b"",
    );
    fs::write(root.join("tracked.txt"), "changed\n").unwrap();
    command(&git, root, &["stash", "push", "-m", "Old base stash"], b"");
    command(&git, root, &["checkout", "main"], b"");
    let cancel = Cancellation::default();
    let snapshot = git.inspect(root, &cancel).unwrap();
    assert_eq!(snapshot.branch.as_deref(), Some("main"));
    let history = git.history(snapshot, &cancel).unwrap();
    let first = git.page(&history, &Cursor::default(), &cancel).unwrap();
    assert_eq!(first.entries.len(), 200);
    let second = git
        .page(&history, first.next.as_ref().unwrap(), &cancel)
        .unwrap();
    assert_eq!(second.entries.len(), 6);
    assert!(second.next.is_none());
    let entries = first
        .entries
        .iter()
        .chain(&second.entries)
        .collect::<Vec<_>>();
    let ordinary = entries
        .iter()
        .filter(|e| !e.graph.is_stash)
        .map(|e| e.commit.hash.clone())
        .collect::<Vec<_>>();
    assert_eq!(ordinary, baseline);
    assert!(entries[204].graph.is_stash);
    assert_eq!(entries[204].commit.parents[0], entries[205].commit.hash);
    assert_eq!(entries[204].commit.parents.len(), 2);
    assert_ne!(entries[204].graph.node, entries[205].graph.node);
    assert_eq!(entries[203].graph.node, entries[205].graph.node);
    assert_eq!(entries[203].graph.color, entries[205].graph.color);
    assert!(entries[0].commit.message.contains("complete body שלום"));
    command(&git, root, &["branch", "new-reference", "HEAD~3"], b"");
    assert!(matches!(
        git.page(&history, &Cursor::default(), &cancel),
        Err(git::Error::Changed)
    ));
}

#[test]
fn autostash_and_detached_previous_tip_stay_on_the_main_lane() {
    let (temp, mut git) = fixture();
    let root = temp.path();
    fs::write(root.join("tracked.txt"), "base\n").unwrap();
    command(&git, root, &["add", "tracked.txt"], b"");
    command(&git, root, &["commit", "-m", "Common base"], b"");
    command(&git, root, &["checkout", "--detach", "HEAD"], b"");
    git.environment
        .insert("GIT_COMMITTER_DATE".into(), Some("1600000000 +0000".into()));
    command(
        &git,
        root,
        &["commit", "--allow-empty", "-m", "Previous tip"],
        b"",
    );
    git.environment
        .remove(std::ffi::OsStr::new("GIT_COMMITTER_DATE"));
    fs::write(root.join("tracked.txt"), "changed\n").unwrap();
    command(&git, root, &["stash", "push", "-m", "Pull autostash"], b"");
    command(&git, root, &["checkout", "main"], b"");
    command(
        &git,
        root,
        &["commit", "--allow-empty", "-m", "Current tip"],
        b"",
    );

    let cancel = Cancellation::default();
    let snapshot = git.inspect(root, &cancel).unwrap();
    let history = git.history(snapshot, &cancel).unwrap();
    let page = git.page(&history, &Cursor::default(), &cancel).unwrap();
    let [tip, stash, base, common] = page.entries.as_slice() else {
        panic!("expected four history rows");
    };
    assert!(stash.graph.is_stash);
    assert_eq!(stash.commit.parents[0], base.commit.hash);
    let common_parent = std::slice::from_ref(&common.commit.hash);
    assert_eq!(tip.commit.parents, common_parent);
    assert_eq!(base.commit.parents, common_parent);
    assert_eq!(stash.graph.node, base.graph.node);
    assert_eq!(stash.graph.width, 2);
    assert!(stash.graph.lines.iter().any(|line| {
        line.from == stash.graph.node
            && line.from_y == 0.5
            && line.to == base.graph.node
            && line.to_y == 1.
    }));
    assert_eq!(base.graph.node, 0);
    assert_eq!(base.graph.node, tip.graph.node);
    assert_eq!(base.graph.color, tip.graph.color);
    assert_eq!(tip.graph.node, common.graph.node);
    let bypass = 1;
    assert!(stash.graph.lines.iter().any(|line| {
        line.from == tip.graph.node && line.from_y == 0. && line.to == bypass && line.to_y == 1.
    }));
    assert!(base.graph.lines.iter().any(|line| {
        line.from == bypass && line.from_y == 0. && line.to == bypass && line.to_y == 1.
    }));
    assert!(common.graph.lines.iter().any(|line| {
        line.from == bypass && line.from_y == 0. && line.to == common.graph.node && line.to_y == 0.5
    }));
}

#[test]
fn unborn_and_detached_metadata_are_distinct() {
    let (temp, git) = fixture();
    let cancel = Cancellation::default();
    let unborn = git.inspect(temp.path(), &cancel).unwrap();
    assert_eq!(unborn.branch.as_deref(), Some("main"));
    assert!(unborn.head.is_none());
    let history = git.history(unborn, &cancel).unwrap();
    assert!(
        git.page(&history, &Cursor::default(), &cancel)
            .unwrap()
            .entries
            .is_empty()
    );
    command(
        &git,
        temp.path(),
        &["commit", "--allow-empty", "-m", "root"],
        b"",
    );
    command(&git, temp.path(), &["checkout", "--detach", "HEAD"], b"");
    let detached = git.inspect(temp.path(), &cancel).unwrap();
    assert!(detached.branch.is_none());
    assert!(detached.head.is_some());
}
