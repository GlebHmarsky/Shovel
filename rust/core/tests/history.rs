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
fn history_waits_for_git_to_confirm_the_captured_worktree() {
    let (temp, git) = fixture();
    let root = temp.path();
    command(&git, root, &["commit", "--allow-empty", "-m", "first"], b"");
    let identity = shovel_core::identity::read(root).unwrap();
    let other = root.join("alternate-worktree");
    fs::create_dir(&other).unwrap();
    command(
        &git,
        root,
        &["config", "--local", "core.worktree", other.to_str().unwrap()],
        b"",
    );
    let cancel = Cancellation::default();
    // Git history itself still reads successfully with this configuration.
    assert_eq!(
        command(&git, root, &["log", "-1", "--format=%s"], b"").trim(),
        "first"
    );
    assert!(matches!(
        git.read_history(
            root,
            &identity,
            None,
            Some(&|_| panic!("the worktree must be confirmed before displaying history")),
            &cancel,
        ),
        Err(git::Error::NotWorktree)
    ));
}

#[test]
fn initial_history_does_not_pass_validation_after_refs_change() {
    let (temp, git) = fixture();
    let root = temp.path();
    command(&git, root, &["commit", "--allow-empty", "-m", "first"], b"");
    let identity = shovel_core::identity::read(root).unwrap();
    let publish = |page: &git::Page| {
        assert_eq!(page.entries.len(), 1);
        assert_eq!(page.entries[0].commit.subject(), "first");
        command(&git, root, &["commit", "--allow-empty", "-m", "second"], b"");
    };
    assert!(matches!(
        git.read_history(root, &identity, None, Some(&publish), &Cancellation::default()),
        Err(git::Error::Changed)
    ));
}

#[test]
fn unusual_stash_stays_available_without_adding_a_history_root() {
    let (temp, git) = fixture();
    let root = temp.path();
    let tree = command(&git, root, &["mktree"], b"");
    let unusual = command(
        &git,
        root,
        &["commit-tree", tree.trim(), "-m", "Unusual stash entry"],
        b"",
    );
    let unusual = unusual.trim();
    command(
        &git,
        root,
        &[
            "update-ref",
            "--create-reflog",
            "-m",
            "Fixture",
            "refs/stash",
            unusual,
        ],
        b"",
    );

    let cancel = Cancellation::default();
    let identity = shovel_core::identity::read(root).unwrap();
    let git::HistoryRead::Page(history, page) = git
        .read_history(root, &identity, None, None, &cancel)
        .unwrap()
    else {
        panic!("expected an empty history page");
    };
    assert!(page.entries.is_empty());
    let list = git.stashes(&history.snapshot, &cancel).unwrap();
    assert_eq!(list.entries[0].hash, unusual);

    command(&git, root, &["tag", "recovered", unusual], b"");
    let git::HistoryRead::Page(_, recovered) = git
        .read_history(root, &identity, None, None, &cancel)
        .unwrap()
    else {
        panic!("expected the recovered history page");
    };
    assert_eq!(recovered.entries.len(), 1);
    assert_eq!(recovered.entries[0].commit.hash, unusual);
    assert_eq!(recovered.entries[0].references.len(), 1);
    assert_eq!(recovered.entries[0].references[0].name, "refs/tags/recovered");
}

#[test]
fn reachable_stashes_annotate_their_actual_base_across_history_pages() {
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
    command(&git, root, &["checkout", "--detach", &baseline[199]], b"");
    fs::write(root.join("tracked.txt"), "changed at boundary\n").unwrap();
    command(&git, root, &["stash", "push", "-m", "Boundary stash"], b"");
    command(&git, root, &["checkout", "main"], b"");
    let cancel = Cancellation::default();
    let snapshot = git.inspect(root, &cancel).unwrap();
    assert_eq!(snapshot.branch.as_deref(), Some("main"));
    let git::HistoryRead::Page(history, first) = git
        .read_history(root, &snapshot.identity, None, None, &cancel)
        .unwrap()
    else {
        panic!("a cold read should return the first page");
    };
    assert_eq!(history.snapshot.fingerprint, snapshot.fingerprint);
    assert_eq!(first.entries.len(), 200);
    let second = git
        .page(&history, first.next.as_ref().unwrap(), &cancel)
        .unwrap();
    assert_eq!(second.entries.len(), 7);
    assert_eq!(second.cursor.offset, 200);
    assert!(second.next.is_none());
    assert_eq!(history.stashes.len(), 2);
    let entries = first.entries.iter().chain(&second.entries).collect::<Vec<_>>();
    assert_eq!(
        entries.iter().filter(|entry| !entry.graph.is_stash()).map(|entry| entry.commit.hash.clone()).collect::<Vec<_>>(),
        baseline
    );
    for stash in &history.stashes {
        let leaf = entries.iter().position(|entry| entry.commit.hash == stash.hash).unwrap();
        assert!(entries[leaf].graph.is_stash());
        assert_eq!(entries[leaf + 1].commit.hash, stash.parents[0]);
        assert_eq!(entries[leaf].graph.node, entries[leaf + 1].graph.node);
        assert!(!entries.iter().any(|entry| entry.commit.hash == stash.parents[1]));
    }
    assert!(entries[0].commit.message.contains("complete body שלום"));
    command(&git, root, &["branch", "new-reference", "HEAD~3"], b"");
    assert!(matches!(
        git.page(&history, &Cursor::default(), &cancel),
        Err(git::Error::Changed)
    ));
    assert!(matches!(
        git.history_and_first_page(history.snapshot, &cancel),
        Err(git::Error::Changed)
    ));
}

#[test]
fn detached_autostash_does_not_pull_its_detached_ancestry_into_history() {
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
        &["commit", "--allow-empty", "-m", "Previous ancestor"],
        b"",
    );
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
    let [tip, common] = page.entries.as_slice() else {
        panic!("expected only the two commits reachable from main");
    };
    assert_eq!(tip.commit.subject(), "Current tip");
    assert_eq!(common.commit.subject(), "Common base");
    assert_eq!(tip.commit.parents, std::slice::from_ref(&common.commit.hash));
    assert_eq!(history.stashes.len(), 1);
    assert!(history.stashes[0].subject().contains("Pull autostash"));
    assert_eq!(tip.graph.node, common.graph.node);
    assert_eq!(tip.graph.color, common.graph.color);

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
