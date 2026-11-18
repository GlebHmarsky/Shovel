#![cfg(windows)]
use shovel_core::{
    git::Git,
    process::{self, Cancellation, Lifetime},
    working::{Action, Error, Group, MutationGate},
};
use std::{fs, path::Path};
fn command(git: &Git, root: &Path, args: &[&str]) -> String {
    let result = process::run(
        &git.request(
            root,
            args.iter().copied(),
            Lifetime::AcceptedCommand,
            1024 * 1024,
        ),
        &Cancellation::default(),
        None,
    )
    .unwrap();
    assert!(result.success(), "{result:?}");
    String::from_utf8(result.stdout).unwrap()
}
fn fixture() -> (tempfile::TempDir, Git) {
    let temp = tempfile::tempdir().unwrap();
    let mut git = Git::discover(None, &Cancellation::default()).unwrap();
    let config = temp.path().join("isolated.gitconfig");
    fs::write(
        &config,
        "[user]\nname = Fixture\nemail = fixture@example.invalid\n[commit]\ngpgsign = false\n",
    )
    .unwrap();
    git.environment
        .insert("GIT_CONFIG_GLOBAL".into(), Some(config.into_os_string()));
    git.environment
        .insert("GIT_CONFIG_NOSYSTEM".into(), Some("1".into()));
    command(&git, temp.path(), &["init", "--initial-branch=main"]);
    (temp, git)
}

#[test]
#[ignore = "opt-in three-sample Local Changes latency fixture with 256 MiB of assets"]
fn measure_stage_unstage_rows_and_selected_diff() {
    use std::time::{Duration, Instant, SystemTime};
    let (temp, git) = fixture();
    let root = temp.path();
    command(&git, root, &["config", "core.autocrlf", "false"]);
    for index in 0..2000 {
        fs::write(root.join(format!("file-{index}.txt")), b"baseline\n").unwrap();
    }
    let mut state = 17_u32;
    let mut asset = vec![0; 4 * 1024 * 1024];
    for block in asset.as_chunks_mut::<4>().0 {
        state = state.wrapping_mul(1664525).wrapping_add(1013904223);
        block.copy_from_slice(&state.to_le_bytes());
    }
    asset[0] = 0;
    let assets: Vec<_> = (0..64)
        .map(|index| root.join(format!("asset-{index}.uasset")))
        .collect();
    for path in &assets {
        fs::write(path, &asset).unwrap();
    }
    command(&git, root, &["add", "."]);
    command(&git, root, &["commit", "-m", "Fixture"]);
    let tip = command(&git, root, &["rev-parse", "HEAD"]);
    for index in 0..1000 {
        fs::write(root.join(format!(".git/refs/heads/fixture-{index}")), &tip).unwrap();
    }
    let cancel = Cancellation::default();
    let target = "file-0.txt";
    for sample in 1..=3 {
        let timestamp = SystemTime::now() + Duration::from_secs(sample + 1);
        for path in &assets {
            fs::File::options()
                .write(true)
                .open(path)
                .unwrap()
                .set_modified(timestamp)
                .unwrap();
        }
        fs::write(root.join(target), format!("changed {sample}\n")).unwrap();
        let mut context = git.working_copy(root, &cancel).unwrap();
        assert_eq!(
            context
                .repository
                .references
                .iter()
                .filter(|r| r.name.starts_with("refs/heads/"))
                .count(),
            1001
        );
        for (before, after) in [
            (Group::Unstaged, Group::Staged),
            (Group::Staged, Group::Unstaged),
        ] {
            let selection = vec![context.change(before, target).unwrap().clone()];
            let (name, action) = if before == Group::Unstaged {
                ("stage", Action::Stage(selection))
            } else {
                ("unstage", Action::Unstage(selection))
            };
            let started = Instant::now();
            let result = git
                .execute_working_for_editor(&context, &action, None)
                .unwrap();
            let command_ms = result.command.elapsed.as_secs_f64() * 1000.0;
            let validation_ms = result.validation_elapsed.as_secs_f64() * 1000.0;
            let reconciliation_ms = result.reconciliation_elapsed.as_secs_f64() * 1000.0;
            assert!(result.succeeded());
            assert!(result.needs_full_scan);
            context = result.updated.unwrap();
            let entry = context.change(after, target).unwrap();
            let rows_ms = started.elapsed().as_secs_f64() * 1000.0;
            let diff = git.working_diff(&context, entry, &cancel).unwrap();
            assert!(diff.contains(&format!("+changed {sample}")));
            let diff_ms = started.elapsed().as_secs_f64() * 1000.0;
            let wider = Instant::now();
            let full = git.working_copy(root, &cancel).unwrap();
            let full_ms = wider.elapsed().as_secs_f64() * 1000.0;
            assert_eq!(full.entries, context.entries);
            assert_eq!(full.index_stamp, context.index_stamp);
            println!(
                "sample={sample} action={name} validation_ms={validation_ms:.1} command_ms={command_ms:.1} reconciliation_ms={reconciliation_ms:.1} rows_ms={rows_ms:.1} diff_ms={diff_ms:.1} wider_ms={full_ms:.1}"
            );
        }
    }
    assert_eq!(fs::read(&assets[0]).unwrap(), asset);
    assert_eq!(command(&git, root, &["show", ":file-0.txt"]), "baseline\n");
}
#[test]
fn targeted_index_actions_fall_back_when_a_native_hook_stages_an_unreviewed_file() {
    let (temp, git) = fixture();
    let root = temp.path();
    let cancel = Cancellation::default();
    for file in ["chosen", "other"] {
        fs::write(root.join(file), "base\n").unwrap();
    }
    command(&git, root, &["add", "."]);
    command(&git, root, &["commit", "-m", "base"]);
    for file in ["chosen", "other"] {
        fs::write(root.join(file), "changed\n").unwrap();
    }
    let review = git.working_copy(root, &cancel).unwrap();
    let chosen = review.change(Group::Unstaged, "chosen").unwrap().clone();
    fs::write(
        root.join(".git/hooks/post-index-change"),
        "#!/bin/sh\nif test ! -f .git/ran-hook; then touch .git/ran-hook; git add -- other; fi\n",
    )
    .unwrap();
    let result = git
        .execute_working_for_editor(&review, &Action::Stage(vec![chosen]), None)
        .unwrap();
    assert!(result.succeeded() && !result.needs_full_scan);
    let updated = result.updated.unwrap();
    for file in ["chosen", "other"] {
        assert!(updated.change(Group::Staged, file).is_some());
    }
    assert_eq!(
        updated.entries,
        git.working_copy(root, &cancel).unwrap().entries
    );
}

#[test]
fn initial_stage_unstage_commit_and_partial_staging_preserve_exact_content() {
    let (temp, git) = fixture();
    let root = temp.path();
    let cancel = Cancellation::default();
    let file = "-שלום [literal].txt";
    fs::write(root.join(file), "first\n").unwrap();
    let review = git.working_copy(root, &cancel).unwrap();
    let chosen = review
        .entries
        .iter()
        .find(|e| e.path == file)
        .unwrap()
        .clone();
    let staged = git
        .execute_working(&review, &Action::Stage(vec![chosen]), None)
        .unwrap();
    assert!(staged.command.success());
    let staged = staged.updated.unwrap();
    let row = staged
        .entries
        .iter()
        .find(|e| e.path == file && e.group == Group::Staged)
        .unwrap()
        .clone();
    let unstage = git
        .execute_working(&staged, &Action::Unstage(vec![row]), None)
        .unwrap();
    assert!(unstage.command.success());
    assert_eq!(fs::read_to_string(root.join(file)).unwrap(), "first\n");
    let review = unstage.updated.unwrap();
    let selected = review
        .entries
        .iter()
        .filter(|e| e.path == file)
        .cloned()
        .collect();
    let stage = git
        .execute_working(&review, &Action::Stage(selected), None)
        .unwrap();
    assert!(stage.command.success());
    let review = stage.updated.unwrap();
    let result = git
        .execute_working(
            &review,
            &Action::Commit {
                message: "Initial שלום\n\nbody\n".into(),
                amend: false,
            },
            None,
        )
        .unwrap();
    assert!(result.command.success());
    assert!(result.commit.is_some());
    assert_eq!(
        command(&git, root, &["show", &format!("HEAD:{file}")]),
        "first\n"
    );
    fs::write(root.join(file), "staged\n").unwrap();
    let review = git.working_copy(root, &cancel).unwrap();
    let selected = review
        .entries
        .iter()
        .filter(|e| e.path == file)
        .cloned()
        .collect();
    let result = git
        .execute_working(&review, &Action::Stage(selected), None)
        .unwrap();
    assert!(result.command.success());
    fs::write(root.join(file), "working\n").unwrap();
    let review = git.working_copy(root, &cancel).unwrap();
    assert_eq!(review.entries.iter().filter(|e| e.path == file).count(), 2);
    assert!(
        git.working_diff(
            &review,
            review
                .entries
                .iter()
                .find(|e| e.path == file && e.group == Group::Staged)
                .unwrap(),
            &cancel
        )
        .unwrap()
        .contains("+staged")
    );
    let chosen = review
        .entries
        .iter()
        .find(|e| e.path == file && e.group == Group::Staged)
        .unwrap()
        .clone();
    let result = git
        .execute_working(&review, &Action::Unstage(vec![chosen]), None)
        .unwrap();
    assert!(result.command.success());
    assert_eq!(fs::read_to_string(root.join(file)).unwrap(), "working\n");
    assert_eq!(
        command(&git, root, &["show", &format!(":{file}")]),
        "first\n"
    );
}
#[test]
fn commit_rejects_changed_index_but_accepts_stat_refresh() {
    let (temp, git) = fixture();
    let root = temp.path();
    let cancel = Cancellation::default();
    fs::write(root.join("a"), "one\n").unwrap();
    command(&git, root, &["add", "--", "a"]);
    let review = git.working_copy(root, &cancel).unwrap();
    command(&git, root, &["update-index", "--refresh"]);
    assert_eq!(
        review.index_stamp,
        git.semantic_index(&review.repository, &cancel).unwrap()
    );
    let result = git
        .execute_working(
            &review,
            &Action::Commit {
                message: "one".into(),
                amend: false,
            },
            None,
        )
        .unwrap();
    assert!(result.command.success());
    fs::write(root.join("a"), "two\n").unwrap();
    command(&git, root, &["add", "--", "a"]);
    let review = git.working_copy(root, &cancel).unwrap();
    fs::write(root.join("a"), "three\n").unwrap();
    command(&git, root, &["add", "--", "a"]);
    assert!(matches!(
        git.execute_working(
            &review,
            &Action::Commit {
                message: "must not commit".into(),
                amend: false
            },
            None
        ),
        Err(Error::Stale)
    ));
    assert_eq!(command(&git, root, &["show", ":a"]), "three\n");
    assert_eq!(
        command(&git, root, &["log", "-1", "--format=%s"]).trim(),
        "one"
    );
}
#[test]
fn failed_native_hook_retains_index_and_reports_failure() {
    let (temp, git) = fixture();
    let root = temp.path();
    let cancel = Cancellation::default();
    fs::write(root.join("file"), "payload\n").unwrap();
    command(&git, root, &["add", "--", "file"]);
    fs::write(
        root.join(".git/hooks/pre-commit"),
        "#!/bin/sh\nprintf 'blocked by fixture\\n' >&2\nexit 1\n",
    )
    .unwrap();
    let review = git.working_copy(root, &cancel).unwrap();
    let result = git
        .execute_working(
            &review,
            &Action::Commit {
                message: "retained message".into(),
                amend: false,
            },
            None,
        )
        .unwrap();
    assert!(!result.command.success());
    assert!(
        String::from_utf8(result.command.stderr)
            .unwrap()
            .contains("blocked by fixture")
    );
    assert_eq!(review.index_stamp, result.updated.unwrap().index_stamp);
    assert!(result.commit.is_none());
    assert_eq!(command(&git, root, &["show", ":file"]), "payload\n");
}

#[test]
#[ignore = "opt-in native commit-signing acceptance using Git for Windows OpenSSH"]
fn commit_honors_configured_ssh_signing_and_preserves_the_index_on_failure() {
    let (temp, git) = fixture();
    let root = temp.path();
    let cancel = Cancellation::default();
    let signing = tempfile::tempdir().unwrap();
    let key = signing.path().join("fixture signing key");
    let ssh_keygen = git
        .executable
        .ancestors()
        .map(|path| path.join("usr/bin/ssh-keygen.exe"))
        .find(|path| path.is_file())
        .expect("Git for Windows OpenSSH signer");
    let keygen = process::Request::new(
        ssh_keygen.clone(),
        signing.path().into(),
        ["-q", "-t", "ed25519", "-N", "", "-f", key.to_str().unwrap()],
    );
    assert!(process::run(&keygen, &cancel, None).unwrap().success());
    let signers = signing.path().join("allowed signers");
    let public_key = fs::read_to_string(key.with_extension("pub")).unwrap();
    fs::write(&signers, format!("fixture@example.invalid {public_key}")).unwrap();
    for (name, value) in [
        ("commit.gpgSign", "true"),
        ("gpg.format", "ssh"),
        ("gpg.ssh.program", ssh_keygen.to_str().unwrap()),
        ("gpg.ssh.allowedSignersFile", signers.to_str().unwrap()),
        ("user.signingKey", key.to_str().unwrap()),
    ] {
        command(&git, root, &["config", "--local", name, value]);
    }
    let commit = |message: &str| {
        let review = git.working_copy(root, &cancel).unwrap();
        git.execute_working(
            &review,
            &Action::Commit {
                message: message.into(),
                amend: false,
            },
            None,
        )
        .unwrap()
    };
    fs::write(root.join("file"), "signed contents\n").unwrap();
    command(&git, root, &["add", "--", "file"]);
    let signed = commit("signed through Shovel");
    assert!(signed.command.success(), "{:?}", signed.command);
    let head = command(&git, root, &["rev-parse", "HEAD"]);
    assert_eq!(signed.commit.as_deref(), Some(head.trim()));
    command(&git, root, &["verify-commit", "HEAD"]);

    command(
        &git,
        root,
        &["config", "--local", "user.signingKey", "missing-fixture-key"],
    );
    fs::write(root.join("file"), "retain in index\n").unwrap();
    command(&git, root, &["add", "--", "file"]);
    let index = git.working_copy(root, &cancel).unwrap().index_stamp;
    let failed = commit("signing failure");
    assert!(!failed.command.success());
    assert!(failed.commit.is_none());
    assert_eq!(failed.updated.unwrap().index_stamp, index);
    assert_eq!(command(&git, root, &["rev-parse", "HEAD"]), head);
    assert_eq!(command(&git, root, &["show", ":file"]), "retain in index\n");
}

#[test]
fn commit_reports_the_created_object_when_a_post_commit_hook_moves_head() {
    let (temp, git) = fixture();
    let root = temp.path();
    command(&git, root, &["commit", "--allow-empty", "-m", "base"]);
    let before = command(&git, root, &["rev-parse", "HEAD"])
        .trim()
        .to_owned();
    fs::write(root.join("change"), b"reviewed work").unwrap();
    command(&git, root, &["add", "change"]);
    fs::write(
        root.join(".git/hooks/post-commit"),
        "#!/bin/sh\ngit rev-parse HEAD > .git/fixture-created\ngit update-ref HEAD HEAD^ HEAD\n",
    )
    .unwrap();
    let review = git.working_copy(root, &Cancellation::default()).unwrap();
    let result = git
        .execute_working(
            &review,
            &Action::Commit {
                message: "reviewed commit".into(),
                amend: false,
            },
            None,
        )
        .unwrap();
    assert!(result.succeeded());
    let created = fs::read_to_string(root.join(".git/fixture-created")).unwrap();
    assert_ne!(before, created.trim());
    assert_eq!(result.commit.as_deref(), Some(created.trim()));
    assert_eq!(
        result.updated.unwrap().repository.head.as_deref(),
        Some(before.as_str())
    );
}
#[test]
fn linked_worktree_gate_conflicts_on_another_thread() {
    let (temp, git) = fixture();
    let root = temp.path();
    let snapshot = git.inspect(root, &Cancellation::default()).unwrap();
    let common = snapshot.common_directory;
    let guard = MutationGate::acquire(&common, false).unwrap();
    let other = std::thread::spawn(move || {
        matches!(
            MutationGate::acquire(&common, false),
            Err(Error::OtherInstance)
        )
    });
    assert!(other.join().unwrap());
    drop(guard);
}

#[test]
fn standard_lfs_status_hashes_hydrated_content_without_running_helpers() {
    use sha2::{Digest, Sha256};
    let (temp, git) = fixture();
    let root = temp.path();
    let cancel = Cancellation::default();
    let asset = b"hydrated asset content\n";
    let pointer = format!(
        "version https://git-lfs.github.com/spec/v1\noid sha256:{:x}\nsize {}\n",
        Sha256::digest(asset),
        asset.len()
    );
    fs::write(root.join("art.asset"), pointer).unwrap();
    fs::write(root.join(".gitattributes"), "*.asset filter=lfs\n").unwrap();
    command(&git, root, &["add", "--all"]);
    command(&git, root, &["commit", "-m", "pointer"]);
    fs::write(
        root.join(".git/fixture-lfs"),
        "[filter \"lfs\"]\nclean = git-lfs clean -- %f\nprocess = git-lfs filter-process\nrequired\n",
    )
    .unwrap();
    command(
        &git,
        root,
        &["config", "--add", "include.path", "fixture-lfs"],
    );
    fs::write(root.join("art.asset"), asset).unwrap();
    let review = git.working_copy(root, &cancel).unwrap();
    assert!(!review.entries.iter().any(|e| e.path == "art.asset"));
    assert_eq!(
        review.config_stamp,
        git.config_stamp(root, &cancel).unwrap()
    );
    assert!(
        !git.has_local_changes(root, Some(&review.repository.identity.fingerprint), &cancel)
            .unwrap()
    );
    let mut changed = asset.to_vec();
    changed[0] = b'X';
    fs::write(root.join("art.asset"), changed).unwrap();
    let review = git.working_copy(root, &cancel).unwrap();
    assert!(
        review
            .entries
            .iter()
            .any(|e| e.path == "art.asset" && e.group == Group::Unstaged)
    );
    // A custom clean transform must not be mistaken for standard LFS semantics.
    command(
        &git,
        root,
        &[
            "config",
            "filter.lfs.clean",
            "sh -c 'touch helper-ran; exit 1'",
        ],
    );
    fs::write(root.join("art.asset"), asset).unwrap();
    let review = git.working_copy(root, &cancel).unwrap();
    assert!(review.entries.iter().any(|e| e.path == "art.asset"));
    assert!(!root.join("helper-ran").exists());
}
