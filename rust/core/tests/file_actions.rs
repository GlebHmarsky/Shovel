#![cfg(windows)]
use shovel_core::{
    git::Git,
    process::{self, Cancellation, Lifetime},
    working::{Change, Context, Group},
};
use std::{fs, path::PathBuf};
fn apply_export(
    f: &Fixture,
    patch: &std::path::Path,
    prepare: impl FnOnce(&std::path::Path),
) -> PathBuf {
    let target = f._temp.path().join("apply");
    f.cmd(&["clone", "--no-hardlinks", ".", target.to_str().unwrap()]);
    prepare(&target);
    let r = process::run(
        &f.git.request(
            &target,
            ["apply", "--binary", patch.to_str().unwrap()],
            Lifetime::AcceptedCommand,
            65536,
        ),
        &Cancellation::default(),
        None,
    )
    .unwrap();
    assert!(r.success(), "{r:?}");
    target
}

#[test]
fn patch_unstaged_binary_and_literal_untracked_additions_apply_without_staging() {
    let f = Fixture::new();
    f.base();
    fs::write(f.root.join("binary.dat"), [0, 1, 2, 3, 4, 5]).unwrap();
    f.cmd(&["add", "binary.dat"]);
    f.cmd(&["commit", "-m", "binary"]);
    fs::write(f.root.join("file.txt"), "staged\n").unwrap();
    f.cmd(&["add", "file.txt"]);
    fs::write(f.root.join("file.txt"), "worktree\n").unwrap();
    fs::write(f.root.join("binary.dat"), [0, 7, 8, 9, 3, 2, 1, 0]).unwrap();
    fs::write(f.root.join("-literal[1] שלום.txt"), "literal addition\n").unwrap();
    fs::write(f.root.join("other.txt"), "unrelated\n").unwrap();
    let c = f.context();
    let mut entries = selected(&c, Group::Unstaged, &["file.txt", "binary.dat"]);
    entries.extend(selected(&c, Group::Untracked, &["-literal[1] שלום.txt"]));
    let output = f._temp.path().join("selected.patch");
    fs::write(&output, "replace this only on success").unwrap();
    let index = fs::read(f.root.join(".git/index")).unwrap();
    let size = f.git.export_selected_patch(&c, &entries, &output).unwrap();
    assert_eq!(size, fs::metadata(&output).unwrap().len() as usize);
    assert_eq!(fs::read(f.root.join(".git/index")).unwrap(), index);
    let bytes = fs::read(&output).unwrap();
    assert!(String::from_utf8_lossy(&bytes).contains("GIT binary patch"));
    let applied = apply_export(&f, &output, |root| {
        fs::write(root.join("file.txt"), "staged\n").unwrap()
    });
    assert_eq!(fs::read(applied.join("file.txt")).unwrap(), b"worktree\n");
    assert_eq!(
        fs::read(applied.join("binary.dat")).unwrap(),
        [0, 7, 8, 9, 3, 2, 1, 0]
    );
    assert_eq!(
        fs::read(applied.join("-literal[1] שלום.txt")).unwrap(),
        b"literal addition\n"
    );
    assert_eq!(fs::read(applied.join("other.txt")).unwrap(), b"other\n");
    assert_eq!(fs::read(f.root.join("other.txt")).unwrap(), b"unrelated\n");
    assert_eq!(f.cmd(&["show", ":file.txt"]), "staged");
}

#[test]
fn patch_staged_rename_delete_and_addition_ignore_later_worktree_edits() {
    let f = Fixture::new();
    f.base();
    f.cmd(&["mv", "file.txt", "renamed.txt"]);
    f.cmd(&["rm", "other.txt"]);
    fs::write(f.root.join("new.txt"), "staged addition\n").unwrap();
    f.cmd(&["add", "new.txt"]);
    fs::write(f.root.join("renamed.txt"), "later edit\n").unwrap();
    let c = f.context();
    let entries = selected(&c, Group::Staged, &["renamed.txt", "other.txt", "new.txt"]);
    let output = f._temp.path().join("staged.patch");
    f.git.export_selected_patch(&c, &entries, &output).unwrap();
    let applied = apply_export(&f, &output, |_| {});
    assert!(!applied.join("file.txt").exists() && !applied.join("other.txt").exists());
    assert_eq!(fs::read(applied.join("renamed.txt")).unwrap(), b"base\n");
    assert_eq!(
        fs::read(applied.join("new.txt")).unwrap(),
        b"staged addition\n"
    );
    assert_eq!(
        fs::read(f.root.join("renamed.txt")).unwrap(),
        b"later edit\n"
    );
    assert_eq!(f.context().index_stamp, c.index_stamp);
}

#[test]
fn patch_unborn_staged_export_applies_to_an_empty_worktree() {
    let f = Fixture::new();
    fs::write(f.root.join("new.txt"), "first addition\n").unwrap();
    f.cmd(&["add", "new.txt"]);
    let c = f.context();
    let entries = selected(&c, Group::Staged, &["new.txt"]);
    let output = f._temp.path().join("unborn.patch");
    f.git.export_selected_patch(&c, &entries, &output).unwrap();
    let applied = apply_export(&f, &output, |_| {});
    assert_eq!(
        fs::read(applied.join("new.txt")).unwrap(),
        b"first addition\n"
    );
    assert!(f.context().repository.head.is_none());
}

#[test]
fn patch_rejects_metadata_sources_mixed_lists_and_stale_state_preserving_output() {
    let f = Fixture::new();
    f.base();
    fs::write(f.root.join("file.txt"), "staged\n").unwrap();
    f.cmd(&["add", "file.txt"]);
    fs::write(f.root.join("file.txt"), "worktree\n").unwrap();
    let c = f.context();
    let entries = selected(&c, Group::Unstaged, &["file.txt"]);
    for target in [
        PathBuf::from("relative.patch"),
        f.root.join(".git/config"),
        f.root.join("file.txt"),
    ] {
        assert_eq!(
            f.git
                .export_selected_patch(&c, &entries, &target)
                .unwrap_err()
                .code(),
            "FilePatchDestination"
        );
    }
    let output = f._temp.path().join("keep.patch");
    fs::write(&output, "keep existing").unwrap();
    let mut mixed = entries.clone();
    mixed.extend(selected(&c, Group::Staged, &["file.txt"]));
    assert_eq!(
        f.git
            .export_selected_patch(&c, &mixed, &output)
            .unwrap_err()
            .code(),
        "CommitStale"
    );
    f.cmd(&["add", "file.txt"]);
    assert!(f.git.export_selected_patch(&c, &entries, &output).is_err());
    assert_eq!(fs::read(&output).unwrap(), b"keep existing");
    assert!(!fs::read_dir(f._temp.path()).unwrap().any(|e| {
        e.unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".shovel-patch-")
    }));
}

#[test]
fn patch_output_limit_preserves_destination_without_running_configured_diff_helpers() {
    let f = Fixture::new();
    f.base();
    fs::write(f.root.join("file.txt"), "changed\n").unwrap();
    fs::write(
        f.root.join(".gitattributes"),
        "file.txt diff=spy filter=spy\n",
    )
    .unwrap();
    f.cmd(&[
        "config",
        "diff.external",
        "sh -c 'echo wrong > external-ran; exit 1'",
    ]);
    f.cmd(&[
        "config",
        "diff.spy.textconv",
        "sh -c 'echo wrong > textconv-ran; exit 1'",
    ]);
    f.cmd(&[
        "config",
        "filter.spy.clean",
        "sh -c 'echo wrong > clean-ran; cat'",
    ]);
    let c = f.context();
    let entries = selected(&c, Group::Unstaged, &["file.txt"]);
    let patch = f.git.selected_patch(&c, &entries).unwrap();
    assert!(String::from_utf8_lossy(&patch).contains("+changed"));
    for marker in ["external-ran", "textconv-ran", "clean-ran"] {
        assert!(!f.root.join(marker).exists());
    }
    fs::write(
        f.root.join("large.txt"),
        vec![b'x'; shovel_core::patches::MAX_BYTES + 1],
    )
    .unwrap();
    let c = f.context();
    let entries = selected(&c, Group::Untracked, &["large.txt"]);
    let output = f._temp.path().join("keep.patch");
    fs::write(&output, "keep existing").unwrap();
    assert_eq!(
        f.git
            .export_selected_patch(&c, &entries, &output)
            .unwrap_err()
            .code(),
        "GitErrorOutputLimit"
    );
    assert_eq!(fs::read(&output).unwrap(), b"keep existing");
}
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
        let f = Self {
            _temp: temp,
            root,
            git,
        };
        f.cmd(&["init", "--initial-branch=main"]);
        f
    }
    fn cmd(&self, args: &[&str]) -> String {
        let r = process::run(
            &self.git.request(
                &self.root,
                args.iter().copied(),
                Lifetime::AcceptedCommand,
                1048576,
            ),
            &Cancellation::default(),
            None,
        )
        .unwrap();
        assert!(r.success(), "{r:?}");
        String::from_utf8(r.stdout).unwrap().trim_end().into()
    }
    fn context(&self) -> Context {
        self.git
            .working_copy(&self.root, &Cancellation::default())
            .unwrap()
    }
    fn base(&self) {
        fs::write(self.root.join("file.txt"), "base\n").unwrap();
        fs::write(self.root.join("other.txt"), "other\n").unwrap();
        self.cmd(&["add", "file.txt", "other.txt"]);
        self.cmd(&["commit", "-m", "base"]);
    }
}
fn selected(context: &Context, group: Group, paths: &[&str]) -> Vec<Change> {
    paths
        .iter()
        .map(|path| {
            context
                .entries
                .iter()
                .find(|e| e.group == group && e.path == *path)
                .unwrap_or_else(|| panic!("missing {group:?} {path} in {:?}", context.entries))
                .clone()
        })
        .collect()
}
#[test]
fn unstaged_discard_restores_exact_index_preserves_staging_and_uses_only_index_hook() {
    let f = Fixture::new();
    f.base();
    fs::write(f.root.join("file.txt"), "staged\r\n").unwrap();
    f.cmd(&["add", "file.txt"]);
    fs::write(f.root.join("file.txt"), "later worktree\n").unwrap();
    fs::write(f.root.join("other.txt"), "keep unrelated\n").unwrap();
    fs::write(
        f.root.join(".git/hooks/post-checkout"),
        "#!/bin/sh\necho wrong >> checkout-hook\n",
    )
    .unwrap();
    fs::write(
        f.root.join(".git/hooks/post-index-change"),
        "#!/bin/sh\necho native >> index-hook\n",
    )
    .unwrap();
    let c = f.context();
    let entries = selected(&c, Group::Unstaged, &["file.txt"]);
    let r = f.git.discard_files(&c, &entries).unwrap();
    assert!(r.succeeded(), "{:?} {}", r.message, r.output);
    assert_eq!(fs::read(f.root.join("file.txt")).unwrap(), b"staged\r\n");
    assert_eq!(f.cmd(&["show", ":file.txt"]), "staged");
    assert_eq!(r.updated.unwrap().index_stamp, c.index_stamp);
    assert_eq!(
        fs::read_to_string(f.root.join("other.txt")).unwrap(),
        "keep unrelated\n"
    );
    assert!(!f.root.join("checkout-hook").exists());
    assert!(f.root.join("index-hook").exists());
}
#[test]
fn staged_rename_addition_and_deletion_restore_head_only_for_selected_paths() {
    let f = Fixture::new();
    f.base();
    f.cmd(&["mv", "file.txt", "renamed.txt"]);
    fs::remove_file(f.root.join("other.txt")).unwrap();
    fs::write(f.root.join("-literal[1] שלום.txt"), "new\n").unwrap();
    f.cmd(&["add", "--all"]);
    fs::write(f.root.join("keep.txt"), "untouched\n").unwrap();
    let c = f.context();
    let entries = selected(
        &c,
        Group::Staged,
        &["renamed.txt", "other.txt", "-literal[1] שלום.txt"],
    );
    assert_eq!(entries[0].original.as_deref(), Some("file.txt"));
    let r = f.git.discard_files(&c, &entries).unwrap();
    assert!(r.succeeded(), "{}", r.output);
    assert_eq!(fs::read(f.root.join("file.txt")).unwrap(), b"base\n");
    assert_eq!(fs::read(f.root.join("other.txt")).unwrap(), b"other\n");
    assert!(!f.root.join("renamed.txt").exists());
    assert!(!f.root.join("-literal[1] שלום.txt").exists());
    assert_eq!(fs::read(f.root.join("keep.txt")).unwrap(), b"untouched\n");
    assert!(f.cmd(&["diff", "--cached", "--name-only"]).is_empty());
}
#[test]
fn unborn_and_untracked_discard_delete_only_reviewed_regular_files() {
    let f = Fixture::new();
    fs::write(f.root.join("staged.txt"), "staged\n").unwrap();
    f.cmd(&["add", "staged.txt"]);
    fs::write(f.root.join("staged.txt"), "unstaged too\n").unwrap();
    fs::write(f.root.join("new.txt"), "remove\n").unwrap();
    fs::write(f.root.join("keep.txt"), "keep\n").unwrap();
    let c = f.context();
    let r = f
        .git
        .discard_files(&c, &selected(&c, Group::Staged, &["staged.txt"]))
        .unwrap();
    assert!(r.succeeded());
    assert!(!f.root.join("staged.txt").exists());
    assert!(f.cmd(&["ls-files"]).is_empty());
    let c = r.updated.unwrap();
    let r = f
        .git
        .discard_files(&c, &selected(&c, Group::Untracked, &["new.txt"]))
        .unwrap();
    assert!(r.succeeded());
    assert!(r.commands.is_empty());
    assert_eq!(r.filesystem_changes, 1);
    assert!(!f.root.join("new.txt").exists());
    assert_eq!(fs::read(f.root.join("keep.txt")).unwrap(), b"keep\n");
}
#[test]
fn stale_index_directory_replacement_and_native_lock_failure_do_not_delete_targets() {
    let f = Fixture::new();
    f.base();
    fs::write(f.root.join("file.txt"), "worktree\n").unwrap();
    let c = f.context();
    f.cmd(&["add", "file.txt"]);
    assert_eq!(
        f.git
            .discard_files(&c, &selected(&c, Group::Unstaged, &["file.txt"]))
            .err()
            .unwrap()
            .code(),
        "CommitStale"
    );
    let c = f.context();
    fs::write(f.root.join(".git/index.lock"), "external lock").unwrap();
    let r = f
        .git
        .discard_files(&c, &selected(&c, Group::Staged, &["file.txt"]))
        .unwrap();
    assert!(!r.succeeded());
    assert_eq!(fs::read(f.root.join("file.txt")).unwrap(), b"worktree\n");
    assert_eq!(
        fs::read(f.root.join(".git/index.lock")).unwrap(),
        b"external lock"
    );
    fs::remove_file(f.root.join(".git/index.lock")).unwrap();
    fs::write(f.root.join("target"), "old file").unwrap();
    let c = f.context();
    fs::remove_file(f.root.join("target")).unwrap();
    fs::create_dir(f.root.join("target")).unwrap();
    fs::write(f.root.join("target/child"), "keep child").unwrap();
    assert_eq!(
        f.git
            .discard_files(&c, &selected(&c, Group::Untracked, &["target"]))
            .err()
            .unwrap()
            .code(),
        "FileActionUnsupported"
    );
    assert_eq!(
        fs::read(f.root.join("target/child")).unwrap(),
        b"keep child"
    );
}

#[test]
fn lfs_discard_hydrates_native_objects_and_missing_content_is_not_reported_clean() {
    let f = Fixture::new();
    f.cmd(&["lfs", "version"]);
    f.cmd(&["config", "filter.lfs.clean", "git-lfs clean -- %f"]);
    f.cmd(&["config", "filter.lfs.smudge", "git-lfs smudge -- %f"]);
    f.cmd(&["config", "filter.lfs.process", "git-lfs filter-process"]);
    f.cmd(&["config", "filter.lfs.required", "true"]);
    f.cmd(&["config", "lfs.url", "http://127.0.0.1:9/fixture"]);
    f.cmd(&["config", "lfs.transfer.maxretries", "0"]);
    f.cmd(&["config", "lfs.dialtimeout", "2"]);
    f.cmd(&["config", "lfs.activitytimeout", "2"]);
    fs::write(
        f.root.join(".gitattributes"),
        "*.asset filter=lfs diff=lfs merge=lfs -text\n",
    )
    .unwrap();
    let original = vec![0x41; 128 * 1024];
    fs::write(f.root.join("art.asset"), &original).unwrap();
    f.cmd(&["add", ".gitattributes", "art.asset"]);
    f.cmd(&["commit", "-m", "local LFS asset"]);
    let pointer = f.cmd(&["show", ":art.asset"]);
    let oid = pointer
        .lines()
        .find_map(|s| s.strip_prefix("oid sha256:"))
        .unwrap();
    let mut changed = original.clone();
    changed[100] = 0x42;
    fs::write(f.root.join("art.asset"), &changed).unwrap();
    let c = f.context();
    let r = f
        .git
        .discard_files(&c, &selected(&c, Group::Unstaged, &["art.asset"]))
        .unwrap();
    assert!(r.succeeded(), "{:?} {}", r.message, r.output);
    assert!(
        fs::read(f.root.join("art.asset")).unwrap() == original,
        "native discard did not restore the exact hydrated bytes"
    );
    assert!(
        r.updated
            .unwrap()
            .entries
            .iter()
            .all(|e| e.path != "art.asset")
    );
    assert_eq!(f.cmd(&["show", ":art.asset"]), pointer);
    // Native post-index-change runs after checkout has cached the file's stat.
    // A same-size/same-timestamp edit must remain visible in reconciliation.
    fs::write(f.root.join("art.asset"), &changed).unwrap();
    let c = f.context();
    fs::write(f.root.join(".git/hooks/post-index-change"), r#"#!/bin/sh
powershell.exe -NoProfile -NonInteractive -Command '$p="art.asset"; $t=[IO.File]::GetLastWriteTimeUtc($p); $b=[IO.File]::ReadAllBytes($p); $b[100]=67; [IO.File]::WriteAllBytes($p,$b); [IO.File]::SetLastWriteTimeUtc($p,$t)'
"#).unwrap();
    let r = f
        .git
        .discard_files(&c, &selected(&c, Group::Unstaged, &["art.asset"]))
        .unwrap();
    assert!(r.succeeded(), "{}", r.output);
    assert_eq!(
        fs::read(f.root.join("art.asset")).unwrap()[100],
        67,
        "{}",
        r.output
    );
    assert!(
        r.updated
            .unwrap()
            .entries
            .iter()
            .any(|e| e.path == "art.asset" && e.group == Group::Unstaged)
    );
    fs::remove_file(f.root.join(".git/hooks/post-index-change")).unwrap();
    fs::write(f.root.join("art.asset"), &changed).unwrap();
    let c = f.context();
    fs::remove_file(f.root.join(format!(
        ".git/lfs/objects/{}/{}/{oid}",
        &oid[..2],
        &oid[2..4]
    )))
    .unwrap();
    let r = f
        .git
        .discard_files(&c, &selected(&c, Group::Unstaged, &["art.asset"]))
        .unwrap();
    assert!(!r.succeeded());
    assert!(!r.output.is_empty());
    assert!(
        r.updated
            .unwrap()
            .entries
            .iter()
            .any(|e| e.path == "art.asset")
    );
    assert_eq!(f.cmd(&["show", ":art.asset"]), pointer);
}

#[test]
#[allow(clippy::permissions_set_readonly_false)] // Windows-only fixture restores its own attribute.
fn filesystem_partial_failure_retains_removed_count_and_remaining_readonly_file() {
    let f = Fixture::new();
    fs::write(f.root.join("first"), "remove first").unwrap();
    fs::write(f.root.join("second"), "keep readonly").unwrap();
    let mut permissions = fs::metadata(f.root.join("second")).unwrap().permissions();
    permissions.set_readonly(true);
    fs::set_permissions(f.root.join("second"), permissions).unwrap();
    let c = f.context();
    let r = f
        .git
        .discard_files(&c, &selected(&c, Group::Untracked, &["first", "second"]))
        .unwrap();
    // Restore the fixture's permission before assertions/temporary cleanup.
    let mut permissions = fs::metadata(f.root.join("second")).unwrap().permissions();
    permissions.set_readonly(false);
    fs::set_permissions(f.root.join("second"), permissions).unwrap();
    assert!(!r.succeeded());
    assert_eq!(r.filesystem_changes, 1);
    assert!(r.commands.is_empty());
    assert!(!f.root.join("first").exists());
    assert_eq!(fs::read(f.root.join("second")).unwrap(), b"keep readonly");
    assert!(
        r.updated
            .unwrap()
            .entries
            .iter()
            .any(|e| e.path == "second")
    );
}

#[test]
fn selected_stash_preserves_unrelated_index_and_restores_only_selected_index_content() {
    let f = Fixture::new();
    f.base();
    fs::write(f.root.join("file.txt"), "selected staged\n").unwrap();
    fs::write(f.root.join("other.txt"), "unrelated staged\n").unwrap();
    f.cmd(&["add", "file.txt", "other.txt"]);
    f.cmd(&["update-index", "--split-index"]);
    fs::write(f.root.join("file.txt"), "selected working\n").unwrap();
    fs::write(f.root.join("other.txt"), "unrelated working\n").unwrap();
    fs::write(f.root.join("-new שלום[1].txt"), "selected untracked\n").unwrap();
    fs::write(f.root.join("keep.txt"), "unrelated untracked\n").unwrap();
    let c = f.context();
    let mut entries = selected(&c, Group::Unstaged, &["file.txt"]);
    entries.extend(selected(&c, Group::Untracked, &["-new שלום[1].txt"]));
    let r = f.git.stash_files(&c, &entries, "selected שלום").unwrap();
    assert!(r.succeeded(), "{:?} {}", r.message, r.output);
    let saved = r.saved_stash.unwrap();
    assert_eq!(f.cmd(&["rev-parse", "refs/stash"]), saved);
    assert_eq!(
        f.cmd(&["show", &format!("{saved}^2:file.txt")]),
        "selected staged"
    );
    assert_eq!(f.cmd(&["show", &format!("{saved}^2:other.txt")]), "other");
    assert_eq!(
        f.cmd(&["show", &format!("{saved}:file.txt")]),
        "selected working"
    );
    assert_eq!(f.cmd(&["show", ":other.txt"]), "unrelated staged");
    assert_eq!(
        fs::read(f.root.join("other.txt")).unwrap(),
        b"unrelated working\n"
    );
    assert_eq!(
        fs::read(f.root.join("keep.txt")).unwrap(),
        b"unrelated untracked\n"
    );
    assert_eq!(fs::read(f.root.join("file.txt")).unwrap(), b"base\n");
    assert_eq!(f.cmd(&["show", ":file.txt"]), "base");
    assert!(!f.root.join("-new שלום[1].txt").exists());
    let replay = f._temp.path().join("replay");
    f.cmd(&[
        "worktree",
        "add",
        "--detach",
        replay.to_str().unwrap(),
        "HEAD",
    ]);
    let r = process::run(
        &f.git.request(
            &replay,
            ["stash", "apply", "--index", &saved],
            Lifetime::AcceptedCommand,
            65536,
        ),
        &Cancellation::default(),
        None,
    )
    .unwrap();
    assert!(r.success(), "{r:?}");
    let state = f
        .git
        .working_copy(&replay, &Cancellation::default())
        .unwrap();
    assert!(state.entries.iter().all(|e| e.path != "other.txt"));
    assert!(
        state
            .entries
            .iter()
            .any(|e| e.path == "file.txt" && e.group == Group::Staged)
    );
    assert!(
        state
            .entries
            .iter()
            .any(|e| e.path == "file.txt" && e.group == Group::Unstaged)
    );
    assert_eq!(
        fs::read(replay.join("-new שלום[1].txt")).unwrap(),
        b"selected untracked\n"
    );
    assert!(!replay.join("keep.txt").exists());
    assert!(fs::read_dir(c.repository.git_directory).unwrap().all(|e| {
        !e.unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with("shovel-rust-index-")
    }));
}

#[test]
fn selected_stash_keeps_saved_entry_and_real_index_when_native_cleanup_is_locked() {
    let f = Fixture::new();
    f.base();
    fs::write(f.root.join("file.txt"), "staged\n").unwrap();
    f.cmd(&["add", "file.txt"]);
    fs::write(f.root.join("file.txt"), "working\n").unwrap();
    let c = f.context();
    let index = fs::read(c.repository.git_directory.join("index")).unwrap();
    fs::write(
        c.repository.git_directory.join("index.lock"),
        "existing lock",
    )
    .unwrap();
    let r = f
        .git
        .stash_files(
            &c,
            &selected(&c, Group::Unstaged, &["file.txt"]),
            "saved despite lock",
        )
        .unwrap();
    assert!(!r.succeeded());
    assert_eq!(r.message.as_deref(), Some("FileStashIndexRetained"));
    let hash = r.saved_stash.unwrap();
    assert_eq!(f.cmd(&["rev-parse", "refs/stash"]), hash);
    assert_eq!(
        fs::read(c.repository.git_directory.join("index")).unwrap(),
        index
    );
    assert_eq!(
        fs::read(c.repository.git_directory.join("index.lock")).unwrap(),
        b"existing lock"
    );
    assert_eq!(fs::read(f.root.join("file.txt")).unwrap(), b"base\n");
    assert!(
        r.updated
            .unwrap()
            .entries
            .iter()
            .any(|e| e.group == Group::Staged && e.path == "file.txt")
    );
}

#[test]
fn selected_stash_rejects_stale_index_and_invalid_message_without_saving_or_cleaning() {
    let f = Fixture::new();
    f.base();
    fs::write(f.root.join("file.txt"), "keep selected\n").unwrap();
    let c = f.context();
    let entries = selected(&c, Group::Unstaged, &["file.txt"]);
    let r = f.git.stash_files(&c, &entries, &"ש".repeat(2049));
    assert!(matches!(
        r,
        Err(shovel_core::operation::Error::Code("StashMessageLimit"))
    ));
    fs::write(f.root.join("other.txt"), "new index\n").unwrap();
    f.cmd(&["add", "other.txt"]);
    let r = f.git.stash_files(&c, &entries, "stale");
    assert!(matches!(
        r,
        Err(shovel_core::operation::Error::Code("CommitStale"))
    ));
    assert!(f.cmd(&["stash", "list"]).is_empty());
    assert_eq!(
        fs::read(f.root.join("file.txt")).unwrap(),
        b"keep selected\n"
    );
    assert_eq!(f.cmd(&["show", ":other.txt"]), "new index");
}

#[test]
fn selected_stash_retains_saved_rename_and_index_after_native_path_cleanup_failure() {
    let f = Fixture::new();
    f.base();
    f.cmd(&["mv", "--", "file.txt", "-renamed שלום[1].txt"]);
    fs::remove_file(f.root.join("other.txt")).unwrap();
    f.cmd(&["add", "--all"]);
    fs::write(f.root.join("-renamed שלום[1].txt"), "renamed working\n").unwrap();
    let c = f.context();
    let entries = selected(&c, Group::Staged, &["-renamed שלום[1].txt", "other.txt"]);
    let index = fs::read(c.repository.git_directory.join("index")).unwrap();
    let r = f.git.stash_files(&c, &entries, "rename/delete").unwrap();
    // Git 2.51.1 stores the stash, then its internal path-limited `add`
    // rejects the now-absent staged-deletion endpoint. Preserve that native
    // partial failure; do not retry, drop the saved entry or reset real files.
    assert!(!r.succeeded());
    assert!(r.output.contains("pathspec 'file.txt'"), "{}", r.output);
    let hash = r.saved_stash.unwrap();
    assert_eq!(f.cmd(&["rev-parse", "refs/stash"]), hash);
    assert_eq!(
        fs::read(c.repository.git_directory.join("index")).unwrap(),
        index
    );
    assert!(!f.root.join("file.txt").exists() && !f.root.join("other.txt").exists());
    assert_eq!(
        fs::read(f.root.join("-renamed שלום[1].txt")).unwrap(),
        b"renamed working\n"
    );
    assert_eq!(f.cmd(&["show", ":-renamed שלום[1].txt"]), "base");
    assert_eq!(
        f.cmd(&["show", &format!("{hash}^2:-renamed שלום[1].txt")]),
        "base"
    );
    assert_eq!(
        f.cmd(&["show", &format!("{hash}:-renamed שלום[1].txt")]),
        "renamed working"
    );
    assert!(!r.updated.unwrap().entries.is_empty());
}

#[test]
fn selected_stash_observes_real_index_changes_made_by_native_hook_after_saving() {
    let f = Fixture::new();
    f.base();
    fs::write(f.root.join("file.txt"), "selected staged\n").unwrap();
    f.cmd(&["add", "file.txt"]);
    fs::write(f.root.join("file.txt"), "selected working\n").unwrap();
    fs::write(f.root.join(".git/hooks/post-index-change"), "#!/bin/sh\nif test ! -e hook-mutated && git rev-parse --verify refs/stash >/dev/null 2>&1; then\n touch hook-mutated\n printf 'hook staged\\n' > other.txt\n GIT_INDEX_FILE=\"$PWD/.git/index\" git add -- other.txt\nfi\n").unwrap();
    let c = f.context();
    let r = f
        .git
        .stash_files(
            &c,
            &selected(&c, Group::Unstaged, &["file.txt"]),
            "hook changed index",
        )
        .unwrap();
    assert!(!r.succeeded());
    assert_eq!(r.message.as_deref(), Some("FileStashIndexRetained"));
    let saved = r.saved_stash.unwrap();
    assert_eq!(f.cmd(&["rev-parse", "refs/stash"]), saved);
    assert_eq!(f.cmd(&["show", ":file.txt"]), "selected staged");
    assert_eq!(f.cmd(&["show", ":other.txt"]), "hook staged");
    assert_eq!(fs::read(f.root.join("file.txt")).unwrap(), b"base\n");
}

#[test]
fn external_diff_uses_native_staged_unstaged_and_untracked_sides_without_real_index_writes() {
    let f = Fixture::new();
    f.base();
    fs::write(f.root.join("file.txt"), "staged\n").unwrap();
    f.cmd(&["add", "file.txt"]);
    fs::write(f.root.join("file.txt"), "working\n").unwrap();
    fs::write(f.root.join("-new שלום[1].txt"), "untracked\n").unwrap();
    f.cmd(&["config", "diff.tool", "wrong"]);
    f.cmd(&["config", "difftool.wrong.cmd", "exit 42"]);
    f.cmd(&["config", "merge.guitool", "fixture"]);
    f.cmd(&[
        "config",
        "difftool.fixture.cmd",
        "cat -- \"$LOCAL\" > old-side; cat -- \"$REMOTE\" > new-side",
    ]);
    let config = fs::read(f.root.join(".git/config")).unwrap();
    let index = fs::read(f.root.join(".git/index")).unwrap();
    for (group, path, old, new) in [
        (Group::Staged, "file.txt", "base\n", "staged\n"),
        (Group::Unstaged, "file.txt", "staged\n", "working\n"),
        (Group::Untracked, "-new שלום[1].txt", "", "untracked\n"),
    ] {
        let c = f.context();
        let r = f
            .git
            .external_file_diff(&c, &selected(&c, group, &[path]))
            .unwrap();
        assert!(r.succeeded(), "{:?} {}", r.message, r.output);
        assert_eq!(fs::read(f.root.join("old-side")).unwrap(), old.as_bytes());
        assert_eq!(fs::read(f.root.join("new-side")).unwrap(), new.as_bytes());
        assert_eq!(fs::read(f.root.join(".git/index")).unwrap(), index);
        assert_eq!(fs::read(f.root.join(".git/config")).unwrap(), config);
    }
    assert!(fs::read_dir(f.root.join(".git")).unwrap().all(|e| {
        !e.unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with("shovel-rust-index-")
    }));
}

#[test]
fn external_diff_untracked_exit_one_is_failure_and_stale_configuration_never_launches() {
    let f = Fixture::new();
    f.base();
    fs::write(f.root.join("new.txt"), "untracked\n").unwrap();
    let c = f.context();
    let entries = selected(&c, Group::Untracked, &["new.txt"]);
    assert!(matches!(
        f.git.external_file_diff(&c, &entries),
        Err(shovel_core::operation::Error::Code("FileDiffToolMissing"))
    ));
    f.cmd(&["config", "diff.guitool", "fixture"]);
    f.cmd(&[
        "config",
        "difftool.fixture.cmd",
        "touch invoked; echo tool-failure >&2; exit 1",
    ]);
    assert!(matches!(
        f.git.external_file_diff(&c, &entries),
        Err(shovel_core::operation::Error::Code("CommitStale"))
    ));
    assert!(!f.root.join("invoked").exists());
    let c = f.context();
    let index = fs::read(f.root.join(".git/index")).unwrap();
    let r = f.git.external_file_diff(&c, &entries).unwrap();
    assert!(!r.succeeded());
    // This Git version wraps an external-tool exit 1 as Git exit 128.
    assert_ne!(r.commands.last().unwrap().exit_code, 0);
    assert!(r.output.contains("tool-failure"));
    assert!(f.root.join("invoked").exists());
    assert_eq!(fs::read(f.root.join(".git/index")).unwrap(), index);
    assert_eq!(fs::read(f.root.join("new.txt")).unwrap(), b"untracked\n");
}

#[test]
fn external_diff_unborn_staged_file_compares_empty_to_index_and_preserves_working_edits() {
    let f = Fixture::new();
    fs::write(f.root.join("first.txt"), "first staged\n").unwrap();
    f.cmd(&["add", "first.txt"]);
    fs::write(f.root.join("first.txt"), "later working\n").unwrap();
    f.cmd(&["config", "diff.guitool", "fixture"]);
    f.cmd(&[
        "config",
        "difftool.fixture.cmd",
        "cat -- \"$LOCAL\" > old-side; cat -- \"$REMOTE\" > new-side",
    ]);
    let c = f.context();
    let r = f
        .git
        .external_file_diff(&c, &selected(&c, Group::Staged, &["first.txt"]))
        .unwrap();
    assert!(r.succeeded(), "{:?} {}", r.message, r.output);
    assert_eq!(fs::read(f.root.join("old-side")).unwrap(), b"");
    assert_eq!(
        fs::read(f.root.join("new-side")).unwrap(),
        b"first staged\n"
    );
    assert_eq!(
        fs::read(f.root.join("first.txt")).unwrap(),
        b"later working\n"
    );
}

#[test]
fn ignore_literal_paths_preserves_rule_bytes_index_and_files_without_matching_globs() {
    use shovel_core::file_rules::Action;
    let f = Fixture::new();
    f.base();
    let target = "space [literal] שלום.dat";
    fs::write(f.root.join(target), "keep selected").unwrap();
    fs::write(f.root.join("space l שלום.dat"), "keep unrelated").unwrap();
    let original = b"\xef\xbb\xbf# original\r\n*.dat\r\n!*.dat";
    fs::write(f.root.join(".gitignore"), original).unwrap();
    let c = f.context();
    let index = fs::read(f.root.join(".git/index")).unwrap();
    let r = f
        .git
        .edit_file_rules(
            &c,
            &selected(&c, Group::Untracked, &[target]),
            Action::IgnorePaths,
        )
        .unwrap();
    assert!(r.succeeded(), "{:?} {}", r.message, r.output);
    assert_eq!(r.warning, None);
    assert!(r.commands.is_empty());
    assert_eq!(r.filesystem_changes, 1);
    let bytes = fs::read(f.root.join(".gitignore")).unwrap();
    assert!(bytes.starts_with(original));
    assert_eq!(
        &bytes[original.len()..],
        "\r\n/space\\ \\[literal\\]\\ שלום.dat\r\n".as_bytes()
    );
    assert_eq!(fs::read(f.root.join(".git/index")).unwrap(), index);
    assert_eq!(fs::read(f.root.join(target)).unwrap(), b"keep selected");
    let updated = r.updated.unwrap();
    assert!(!updated.entries.iter().any(|e| e.path == target));
    assert!(updated.entries.iter().any(|e| e.path == "space l שלום.dat"));
}

#[test]
fn ignore_extensions_apply_repository_wide_and_nested_override_is_reported() {
    use shovel_core::file_rules::Action;
    let f = Fixture::new();
    f.base();
    fs::create_dir(f.root.join("sub")).unwrap();
    fs::write(f.root.join("first.asset"), "first").unwrap();
    fs::write(f.root.join("sub/second.asset"), "second").unwrap();
    fs::write(f.root.join("sub/keep.tmp"), "keep").unwrap();
    fs::write(f.root.join("sub/.gitignore"), "!keep.tmp\n").unwrap();
    let c = f.context();
    let r = f
        .git
        .edit_file_rules(
            &c,
            &selected(&c, Group::Untracked, &["first.asset"]),
            Action::IgnoreExtensions,
        )
        .unwrap();
    assert!(r.succeeded());
    assert_eq!(r.warning, None);
    assert_eq!(fs::read(f.root.join(".gitignore")).unwrap(), b"*.asset\n");
    let c = r.updated.unwrap();
    assert!(!c.entries.iter().any(|e| e.path.ends_with(".asset")));
    let r = f
        .git
        .edit_file_rules(
            &c,
            &selected(&c, Group::Untracked, &["sub/keep.tmp"]),
            Action::IgnorePaths,
        )
        .unwrap();
    assert!(r.succeeded());
    assert_eq!(r.warning, Some("FileIgnoreInherited"));
    assert!(
        r.updated
            .unwrap()
            .entries
            .iter()
            .any(|e| e.path == "sub/keep.tmp")
    );
    assert_eq!(fs::read(f.root.join("sub/keep.tmp")).unwrap(), b"keep");
}

#[test]
fn rule_edit_rejects_locked_invalid_encoding_and_oversized_files_without_rewrite() {
    use shovel_core::file_rules::Action;
    use std::os::windows::fs::OpenOptionsExt;
    let f = Fixture::new();
    f.base();
    fs::write(f.root.join("new.tmp"), "keep").unwrap();
    fs::write(f.root.join(".gitignore"), b"# original\n").unwrap();
    let c = f.context();
    let entries = selected(&c, Group::Untracked, &["new.tmp"]);
    let lock = fs::OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(f.root.join(".gitignore"))
        .unwrap();
    let r = f.git.edit_file_rules(&c, &entries, Action::IgnorePaths);
    assert!(r.is_err() || r.as_ref().is_ok_and(|r| !r.succeeded()));
    drop(lock);
    assert_eq!(
        fs::read(f.root.join(".gitignore")).unwrap(),
        b"# original\n"
    );
    for (bytes, code) in [
        (vec![0xff, 0xfe, 0x41], "FileRulesEncoding"),
        (vec![b'#'; 1024 * 1024 + 1], "FileRulesTooLarge"),
    ] {
        fs::write(f.root.join(".gitignore"), &bytes).unwrap();
        let c = f.context();
        let r = f
            .git
            .edit_file_rules(&c, &entries, Action::IgnorePaths)
            .unwrap();
        assert!(!r.succeeded());
        assert_eq!(r.message.as_deref(), Some(code));
        assert_eq!(r.filesystem_changes, 0);
        assert!(fs::read(f.root.join(".gitignore")).unwrap() == bytes);
    }
}

#[test]
fn lfs_rule_edit_checks_setup_preserves_bytes_and_reports_nested_override() {
    use shovel_core::file_rules::Action;
    let f = Fixture::new();
    f.base();
    let target = "space [literal] שלום.dat";
    fs::write(f.root.join(target), [0, 1, 2, 3, 0, 255]).unwrap();
    fs::write(
        f.root.join(".gitattributes"),
        "# existing\r\n*.txt text\r\n",
    )
    .unwrap();
    let c = f.context();
    let entries = selected(&c, Group::Untracked, &[target]);
    let missing = f.git.edit_file_rules(&c, &entries, Action::LfsTrack);
    assert!(matches!(
        missing,
        Err(shovel_core::operation::Error::Code("FileLfsSetup"))
    ));
    f.cmd(&["config", "filter.lfs.clean", "git-lfs clean -- %f"]);
    f.cmd(&["config", "filter.lfs.process", "git-lfs filter-process"]);
    let c = f.context();
    let index = fs::read(f.root.join(".git/index")).unwrap();
    let config = fs::read(f.root.join(".git/config")).unwrap();
    let r = f
        .git
        .edit_file_rules(&c, &entries, Action::LfsTrack)
        .unwrap();
    assert!(r.succeeded(), "{:?} {}", r.message, r.output);
    assert_eq!(r.warning, None);
    assert!(
        f.cmd(&["check-attr", "filter", "--", target])
            .ends_with(": lfs")
    );
    assert_eq!(fs::read(f.root.join(target)).unwrap(), [0, 1, 2, 3, 0, 255]);
    let c = r.updated.unwrap();
    let r = f
        .git
        .edit_file_rules(&c, &entries, Action::LfsUntrack)
        .unwrap();
    assert!(r.succeeded());
    assert_eq!(r.warning, None);
    assert!(
        f.cmd(&["check-attr", "filter", "--", target])
            .ends_with(": unspecified")
    );
    assert!(
        fs::read(f.root.join(".gitattributes"))
            .unwrap()
            .starts_with(b"# existing\r\n*.txt text\r\n")
    );
    fs::create_dir(f.root.join("sub")).unwrap();
    fs::write(f.root.join("sub/file.dat"), "nested").unwrap();
    fs::write(f.root.join("sub/.gitattributes"), "file.dat !filter\n").unwrap();
    let c = f.context();
    let r = f
        .git
        .edit_file_rules(
            &c,
            &selected(&c, Group::Untracked, &["sub/file.dat"]),
            Action::LfsTrack,
        )
        .unwrap();
    assert!(r.succeeded());
    assert_eq!(r.warning, Some("FileLfsInherited"));
    assert_eq!(fs::read(f.root.join(".git/index")).unwrap(), index);
    assert_eq!(fs::read(f.root.join(".git/config")).unwrap(), config);
    assert!(!f.root.join(".git/hooks/pre-push").exists());
}

#[test]
fn rule_edits_reject_tracked_ignore_missing_extensions_and_stale_index_before_writing() {
    use shovel_core::{file_rules::Action, operation::Error};
    let f = Fixture::new();
    f.base();
    fs::write(f.root.join("file.txt"), "tracked edit\n").unwrap();
    fs::write(f.root.join("no-extension"), "untracked\n").unwrap();
    let c = f.context();
    assert!(matches!(
        f.git.edit_file_rules(
            &c,
            &selected(&c, Group::Unstaged, &["file.txt"]),
            Action::IgnorePaths
        ),
        Err(Error::Code("FileIgnoreTracked"))
    ));
    let selected = selected(&c, Group::Untracked, &["no-extension"]);
    assert!(matches!(
        f.git
            .edit_file_rules(&c, &selected, Action::IgnoreExtensions),
        Err(Error::Code("FileIgnoreNoExtension"))
    ));
    f.cmd(&["add", "file.txt"]);
    assert!(matches!(
        f.git.edit_file_rules(&c, &selected, Action::IgnorePaths),
        Err(Error::Code("CommitStale"))
    ));
    assert!(!f.root.join(".gitignore").exists());
    assert!(!f.root.join(".gitattributes").exists());
    assert_eq!(f.cmd(&["show", ":file.txt"]), "tracked edit");
}
