#![cfg(windows)]
use shovel_core::{
    diff::{HunkAvailability, Kind, Options},
    git::Git,
    process::{self, Cancellation, Lifetime},
    working::{Action, Error, Group},
};
use std::{fs, path::Path};
fn command(git: &Git, root: &Path, args: &[&str]) -> String {
    let output = process::run(
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
    assert!(output.success(), "{output:?}");
    String::from_utf8(output.stdout).unwrap()
}
fn fixture() -> (tempfile::TempDir, Git, String) {
    let temp = tempfile::tempdir().unwrap();
    let mut git = Git::discover(None, &Cancellation::default()).unwrap();
    let config = temp.path().join("isolated.config");
    fs::write(&config,"[user]\nname = Fixture\nemail = fixture@example.invalid\n[core]\nautocrlf = false\n[commit]\ngpgsign = false\n").unwrap();
    git.environment
        .insert("GIT_CONFIG_GLOBAL".into(), Some(config.into_os_string()));
    git.environment
        .insert("GIT_CONFIG_NOSYSTEM".into(), Some("1".into()));
    command(&git, temp.path(), &["init", "--initial-branch=main"]);
    let original = (1..=60)
        .map(|n| format!("line {n}\r\n"))
        .collect::<String>();
    fs::write(temp.path().join("file.txt"), &original).unwrap();
    command(&git, temp.path(), &["add", "--", "file.txt"]);
    command(&git, temp.path(), &["commit", "-m", "base"]);
    (temp, git, original)
}
#[test]
fn selected_crlf_hunks_stage_and_unstage_without_touching_other_changes() {
    let (temp, git, original) = fixture();
    let root = temp.path();
    let cancel = Cancellation::default();
    let modified = original
        .replace("line 3\r\n", "first addition\r\nsecond addition\r\n")
        .replace("line 30\r\n", "middle changed\r\n")
        .replace("line 60\r\n", "last without newline");
    fs::write(root.join("file.txt"), &modified).unwrap();
    let context = git.working_copy(root, &cancel).unwrap();
    let entry = context
        .entries
        .iter()
        .find(|e| e.path == "file.txt" && e.group == Group::Unstaged)
        .unwrap();
    let diff = git
        .file_diff(&context, entry, Options::default(), &cancel)
        .unwrap();
    assert_eq!(diff.availability, HunkAvailability::Available);
    let preview = diff.hunks.unwrap();
    assert_eq!(preview.hunks().len(), 3);
    let result = git
        .execute_working(
            &context,
            &Action::Hunks {
                preview,
                ids: vec![0, 2],
            },
            None,
        )
        .unwrap();
    assert!(result.succeeded());
    assert_eq!(fs::read_to_string(root.join("file.txt")).unwrap(), modified);
    let staged = command(&git, root, &["show", ":file.txt"]);
    assert!(staged.contains("first addition\r\n"));
    assert!(staged.contains("line 30\r\n"));
    assert!(staged.ends_with("last without newline"));
    let context = result.updated.unwrap();
    let entry = context
        .entries
        .iter()
        .find(|e| e.path == "file.txt" && e.group == Group::Staged)
        .unwrap();
    let preview = git
        .file_diff(&context, entry, Options::default(), &cancel)
        .unwrap()
        .hunks
        .unwrap();
    assert_eq!(preview.hunks().len(), 2);
    let result = git
        .execute_working(
            &context,
            &Action::Hunks {
                preview,
                ids: vec![0],
            },
            None,
        )
        .unwrap();
    assert!(result.succeeded());
    let staged = command(&git, root, &["show", ":file.txt"]);
    assert!(staged.contains("line 3\r\n"));
    assert!(staged.ends_with("last without newline"));
    assert_eq!(fs::read_to_string(root.join("file.txt")).unwrap(), modified);
}
#[test]
fn stale_bytes_attributes_and_preview_options_never_apply_a_patch() {
    let (temp, git, original) = fixture();
    let root = temp.path();
    let cancel = Cancellation::default();
    let modified = original.replace("line 3", "new 03");
    fs::write(root.join("file.txt"), &modified).unwrap();
    let context = git.working_copy(root, &cancel).unwrap();
    let entry = context
        .entries
        .iter()
        .find(|e| e.path == "file.txt")
        .unwrap();
    let diff = git
        .file_diff(
            &context,
            entry,
            Options {
                ignore_whitespace: true,
                whole_file: false,
            },
            &cancel,
        )
        .unwrap();
    assert_eq!(diff.availability, HunkAvailability::PreviewOptions);
    assert!(diff.hunks.is_none());
    let preview = git
        .file_diff(&context, entry, Options::default(), &cancel)
        .unwrap()
        .hunks
        .unwrap();
    fs::write(root.join("file.txt"), modified.replace("new 03", "new 04")).unwrap();
    assert!(matches!(
        git.execute_working(
            &context,
            &Action::Hunks {
                preview,
                ids: vec![0]
            },
            None
        ),
        Err(Error::HunkStale)
    ));
    assert_eq!(command(&git, root, &["show", ":file.txt"]), original);
    fs::write(root.join("file.txt"), &modified).unwrap();
    let preview = git
        .file_diff(&context, entry, Options::default(), &cancel)
        .unwrap()
        .hunks
        .unwrap();
    fs::write(root.join(".gitattributes"), "file.txt ident\n").unwrap();
    assert!(matches!(
        git.execute_working(
            &context,
            &Action::Hunks {
                preview,
                ids: vec![0]
            },
            None
        ),
        Err(Error::HunkStale)
    ));
    assert_eq!(command(&git, root, &["show", ":file.txt"]), original);
    let diff = git
        .file_diff(&context, entry, Options::default(), &cancel)
        .unwrap();
    assert_eq!(diff.availability, HunkAvailability::Filtered);
    assert!(diff.hunks.is_none());
}
#[test]
fn binary_and_lfs_sizes_are_local_and_absent_is_distinct_from_empty() {
    let (temp, git, _) = fixture();
    let root = temp.path();
    let cancel = Cancellation::default();
    fs::write(root.join("binary.dat"), [1, 0, 2, 3]).unwrap();
    fs::write(root.join("empty.dat"), []).unwrap();
    fs::write(
        root.join("pointer.asset"),
        format!(
            "version https://git-lfs.github.com/spec/v1\noid sha256:{}\nsize 987654321\n",
            "a".repeat(64)
        ),
    )
    .unwrap();
    let context = git.working_copy(root, &cancel).unwrap();
    for name in ["binary.dat", "empty.dat", "pointer.asset"] {
        let entry = context.entries.iter().find(|e| e.path == name).unwrap();
        let diff = git
            .file_diff(&context, entry, Options::default(), &cancel)
            .unwrap();
        assert!(diff.old.absent);
        assert!(!diff.new.absent);
        match name {
            "binary.dat" => {
                assert_eq!(diff.kind, Kind::Binary);
                assert_eq!(diff.new.bytes, Some(4));
            }
            "empty.dat" => assert_eq!(diff.new.bytes, Some(0)),
            _ => {
                assert_eq!(diff.kind, Kind::Binary);
                assert!(diff.new.lfs);
                assert_eq!(diff.new.bytes, Some(987654321));
                assert!(diff.lines.is_empty());
            }
        }
    }
}

#[test]
fn historical_comparison_uses_immutable_parent_and_commit_objects() {
    let (temp, git, original) = fixture();
    let root = temp.path();
    let cancel = Cancellation::default();
    let parent = command(&git, root, &["rev-parse", "HEAD"])
        .trim()
        .to_owned();
    fs::rename(root.join("file.txt"), root.join("renamed.txt")).unwrap();
    let changed = original.replace("line 10", "renamed content");
    fs::write(root.join("renamed.txt"), &changed).unwrap();
    command(&git, root, &["add", "--", "file.txt", "renamed.txt"]);
    command(&git, root, &["commit", "-m", "rename"]);
    let commit = command(&git, root, &["rev-parse", "HEAD"])
        .trim()
        .to_owned();
    let comparison = git
        .commit_files(root, &commit, Some(&parent), &cancel)
        .unwrap();
    assert_eq!(comparison.entries.len(), 1);
    let row = &comparison.entries[0];
    assert_eq!(row.original.as_deref(), Some("file.txt"));
    assert_eq!(row.path, "renamed.txt");
    fs::write(root.join("renamed.txt"), "unrelated worktree bytes\n").unwrap();
    command(&git, root, &["add", "--all"]);
    let diff = git
        .commit_file_diff(root, &comparison, row, &cancel)
        .unwrap();
    assert_eq!(diff.kind, Kind::Text);
    assert!(
        diff.lines
            .iter()
            .any(|l| l.text.contains("renamed content"))
    );
    assert!(
        !diff
            .lines
            .iter()
            .any(|l| l.text.contains("unrelated worktree"))
    );
    assert!(diff.hunks.is_none());
    assert!(git.commit_files(root, &commit, None, &cancel).is_err());
    let initial = git.commit_files(root, &parent, None, &cancel).unwrap();
    assert_eq!(initial.entries.len(), 1);
    assert_eq!(initial.entries[0].code, 'A');
}
