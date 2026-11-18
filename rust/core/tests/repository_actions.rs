#![cfg(windows)]
use shovel_core::{
    git::Git,
    process::{self, Cancellation, Lifetime},
    repository_actions::{Action, Bisect},
};
use std::{fs, path::Path};

fn command(git: &Git, root: &Path, arguments: &[&str]) -> Vec<u8> {
    let result = process::run(
        &git.request(
            root,
            arguments.iter().copied(),
            Lifetime::AcceptedCommand,
            1024 * 1024,
        ),
        &Cancellation::default(),
        None,
    )
    .unwrap();
    assert!(result.success(), "{result:?}");
    result.stdout
}

#[test]
fn creates_tags_and_worktrees_applies_patches_and_runs_flow_and_bisect() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("primary");
    fs::create_dir(&root).unwrap();
    let config = directory.path().join("gitconfig");
    fs::write(&config, "[user]\nname=Fixture\nemail=test@example.invalid\n[core]\nautocrlf=false\n[commit]\ngpgsign=false\n").unwrap();
    let cancel = Cancellation::default();
    let mut git = Git::discover(None, &cancel).unwrap();
    git.environment
        .insert("GIT_CONFIG_GLOBAL".into(), Some(config.into_os_string()));
    git.environment
        .insert("GIT_CONFIG_NOSYSTEM".into(), Some("1".into()));
    let git_command = |args: &[&str]| command(&git, &root, args);
    let invoke = |action| {
        let snapshot = git.inspect(&root, &cancel).unwrap();
        let result = git
            .execute_repository_action(&snapshot, &action, None, None)
            .unwrap();
        assert!(
            result.succeeded(),
            "{:?}\n{}",
            result.message,
            result.output
        );
        result.updated.unwrap()
    };
    git_command(&["init", "--initial-branch=main"]);
    let file = root.join("file.txt");
    for (name, contents) in [
        ("base", "base\n"),
        ("second", "second\n"),
        ("third", "third\n"),
    ] {
        fs::write(&file, contents).unwrap();
        git_command(&["add", "--", "file.txt"]);
        git_command(&["commit", "-m", name]);
    }
    let tagged = invoke(Action::Tag {
        name: "menu-tag".into(),
        target: "HEAD~2".into(),
        message: "Annotated fixture tag".into(),
    });
    assert!(
        tagged
            .references
            .iter()
            .any(|reference| reference.name == "refs/tags/menu-tag")
    );
    let linked = directory.path().join("worktree with spaces");
    invoke(Action::Worktree {
        path: linked.clone(),
        branch: "menu-worktree".into(),
        target: "HEAD".into(),
    });
    assert_eq!(
        String::from_utf8_lossy(&command(&git, &linked, &["branch", "--show-current"])).trim(),
        "menu-worktree"
    );
    fs::write(&file, "patched\n").unwrap();
    let patch = directory.path().join("reviewed patch.diff");
    fs::write(&patch, git_command(&["diff", "--binary", "--", "file.txt"])).unwrap();
    git_command(&["restore", "--", "file.txt"]);
    invoke(Action::Patch(patch));
    assert_eq!(fs::read_to_string(&file).unwrap(), "patched\n");
    git_command(&["restore", "--", "file.txt"]);

    invoke(Action::FlowInit {
        production: "main".into(),
        development: "develop".into(),
    });
    let feature = invoke(Action::Feature {
        name: "menu".into(),
        finish: false,
    });
    assert_eq!(feature.branch.as_deref(), Some("feature/menu"));
    fs::write(&file, "feature\n").unwrap();
    git_command(&["add", "--", "file.txt"]);
    git_command(&["commit", "-m", "feature"]);
    let merged = invoke(Action::Feature {
        name: "menu".into(),
        finish: true,
    });
    assert_eq!(merged.branch.as_deref(), Some("develop"));
    assert_eq!(fs::read_to_string(&file).unwrap(), "feature\n");

    let state = invoke(Action::BisectStart {
        good: "main~2".into(),
        bad: "main".into(),
    });
    assert_eq!(state.operations, ["bisect"]);
    invoke(Action::Bisect(Bisect::Good));
    let restored = invoke(Action::Bisect(Bisect::Reset));
    assert_eq!(restored.branch.as_deref(), Some("develop"));
    assert!(restored.operations.is_empty());

    git_command(&["lfs", "version"]);
    invoke(Action::LfsInstall);
    assert_eq!(
        String::from_utf8_lossy(&git_command(&[
            "config",
            "--local",
            "--get",
            "filter.lfs.required"
        ]))
        .trim(),
        "true"
    );
    invoke(Action::LfsPattern {
        pattern: "*.asset".into(),
        track: true,
    });
    let attrs = root.join(".gitattributes");
    assert!(
        fs::read_to_string(&attrs)
            .unwrap()
            .contains("*.asset filter=lfs")
    );
    invoke(Action::LfsPattern {
        pattern: "*.asset".into(),
        track: false,
    });
    assert!(
        !fs::read_to_string(&attrs)
            .unwrap_or_default()
            .contains("*.asset filter=lfs")
    );
}
