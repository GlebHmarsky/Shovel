#![cfg(windows)]
use shovel_core::{
    divergence::State,
    git::Git,
    process::{self, Cancellation, Lifetime},
};
use std::{fs, path::Path};
fn command(git: &Git, root: &Path, args: &[&str]) -> String {
    let result = process::run(
        &git.request(root, args.iter().copied(), Lifetime::AcceptedCommand, 65536),
        &Cancellation::default(),
        None,
    )
    .unwrap();
    assert!(result.success(), "{result:?}");
    String::from_utf8(result.stdout)
        .unwrap()
        .trim_end()
        .to_owned()
}
fn engine(root: &Path) -> Git {
    let config = root.join("isolated.config");
    fs::write(&config, "[user]\nname=Fixture\nemail=fixture@example.invalid\n[commit]\ngpgsign=false\n[core]\nautocrlf=false\n").unwrap();
    let mut git = Git::discover(None, &Cancellation::default()).unwrap();
    git.environment
        .insert("GIT_CONFIG_GLOBAL".into(), Some(config.into_os_string()));
    git.environment
        .insert("GIT_CONFIG_NOSYSTEM".into(), Some("1".into()));
    for key in ["GIT_AUTHOR_DATE", "GIT_COMMITTER_DATE"] {
        git.environment
            .insert(key.into(), Some("2026-01-01T00:00:00+00:00".into()));
    }
    git
}
#[test]
fn native_counts_follow_captured_pull_upstream_and_preserve_repository_state() {
    let temp = tempfile::tempdir().unwrap();
    let git = engine(temp.path());
    let root = temp.path().join("repository");
    fs::create_dir(&root).unwrap();
    command(&git, &root, &["init", "--initial-branch=main"]);
    let tree = command(&git, &root, &["mktree"]);
    let base = command(&git, &root, &["commit-tree", &tree, "-m", "Root"]);
    let left = command(
        &git,
        &root,
        &["commit-tree", &tree, "-p", &base, "-m", "Local one"],
    );
    let tip = command(
        &git,
        &root,
        &["commit-tree", &tree, "-p", &left, "-m", "Local two"],
    );
    let right = command(
        &git,
        &root,
        &["commit-tree", &tree, "-p", &base, "-m", "Upstream one"],
    );
    let unrelated = command(
        &git,
        &root,
        &["commit-tree", &tree, "-p", &base, "-m", "Other branch"],
    );
    command(&git, &root, &["update-ref", "refs/heads/other", &unrelated]);
    for (key, value) in [
        ("remote.team.url", "https://example.invalid/never-contact"),
        ("remote.team.fetch", "+refs/heads/*:refs/remotes/team/*"),
        ("branch.main.remote", "team"),
        ("branch.main.merge", "refs/heads/topic"),
        ("branch.main.pushRemote", "different-destination"),
    ] {
        command(&git, &root, &["config", key, value]);
    }
    let cancel = Cancellation::default();
    let mut records = Vec::new();
    let hex = |bytes: &[u8]| bytes.iter().map(|b| format!("{b:02X}")).collect::<String>();
    for (name, head, upstream, ahead, behind) in [
        ("sync", &base, &base, 0, 0),
        ("ahead", &tip, &base, 2, 0),
        ("behind", &base, &right, 0, 1),
        ("diverged", &tip, &right, 2, 1),
    ] {
        command(&git, &root, &["update-ref", "refs/heads/main", head]);
        command(
            &git,
            &root,
            &["update-ref", "refs/remotes/team/topic", upstream],
        );
        let snapshot = git.inspect(&root, &cancel).unwrap();
        let head_before = fs::read(root.join(".git/HEAD")).unwrap();
        let config = fs::read(root.join(".git/config")).unwrap();
        let status = command(&git, &root, &["status", "--porcelain", "-z"]);
        assert_eq!(
            snapshot.upstream.as_deref(),
            Some("refs/remotes/team/topic")
        );
        let value = git.divergence(&snapshot, &cancel).unwrap();
        assert_eq!(value, State::Available { ahead, behind });
        let (state, hashes) = git.divergence_with_incoming(&snapshot, &cancel).unwrap();
        assert_eq!(state, value);
        assert_eq!(hashes.len(), behind as usize);
        assert_eq!(hashes.contains(&right), behind != 0);
        assert!(!hashes.contains(&tip));
        assert!(!hashes.contains(&left));
        assert!(!hashes.contains(&unrelated));
        assert_eq!(fs::read(root.join(".git/HEAD")).unwrap(), head_before);
        assert_eq!(fs::read(root.join(".git/config")).unwrap(), config);
        assert_eq!(
            command(&git, &root, &["status", "--porcelain", "-z"]),
            status
        );
        assert!(!root.join(".git/index").exists());
        records.push(serde_json::json!({"case":name,"state":"Available","ahead":ahead,"behind":behind,
            "head":head,"upstream":upstream,"config":hex(&config),"headFile":hex(&head_before),"status":status,"indexExists":false}));
    }
    let captured = git.inspect(&root, &cancel).unwrap();
    command(
        &git,
        &root,
        &["update-ref", "refs/remotes/team/topic", &tip],
    );
    assert_eq!(
        git.divergence(&captured, &cancel).unwrap(),
        State::Available {
            ahead: 2,
            behind: 1
        }
    );
    records.push(serde_json::json!({"case":"immutable","state":"Available","ahead":2,"behind":1}));
    fs::write(root.join(".git/shallow"), format!("{tip}\n")).unwrap();
    assert_eq!(
        git.divergence(&git.inspect(&root, &cancel).unwrap(), &cancel)
            .unwrap(),
        State::Shallow
    );
    fs::remove_file(root.join(".git/shallow")).unwrap();
    command(
        &git,
        &root,
        &["update-ref", "-d", "refs/remotes/team/topic"],
    );
    assert_eq!(
        git.divergence(&git.inspect(&root, &cancel).unwrap(), &cancel)
            .unwrap(),
        State::MissingUpstream
    );
    command(&git, &root, &["config", "--unset", "branch.main.remote"]);
    assert_eq!(
        git.divergence(&git.inspect(&root, &cancel).unwrap(), &cancel)
            .unwrap(),
        State::NoUpstream
    );
    command(&git, &root, &["checkout", "--detach", &tip]);
    assert_eq!(
        git.divergence(&git.inspect(&root, &cancel).unwrap(), &cancel)
            .unwrap(),
        State::Detached
    );
    let empty = temp.path().join("empty");
    fs::create_dir(&empty).unwrap();
    command(&git, &empty, &["init", "--initial-branch=main"]);
    assert_eq!(
        git.divergence(&git.inspect(&empty, &cancel).unwrap(), &cancel)
            .unwrap(),
        State::Unborn
    );
    for state in [
        "Shallow",
        "MissingUpstream",
        "NoUpstream",
        "Detached",
        "Unborn",
    ] {
        records.push(serde_json::json!({"case":state,"state":state}));
    }
    if let Some(output) = std::env::var_os("SHOVEL_DIVERGENCE_PARITY_OUTPUT") {
        fs::write(output, serde_json::to_vec_pretty(&records).unwrap()).unwrap();
    }
}
