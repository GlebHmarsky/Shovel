#![cfg(windows)]
use shovel_core::{
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
