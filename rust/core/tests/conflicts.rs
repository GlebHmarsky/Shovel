#![cfg(windows)]
use shovel_core::{
    conflict_text::{self, Encoding},
    conflicts::{Choice, Preview, Tool, ToolSettings},
    git::Git,
    process::{self, Cancellation, Lifetime},
    working::Group,
};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};
fn raw(git: &Git, root: &Path, args: &[&str]) -> process::Result {
    process::run(
        &git.request(
            root,
            args.iter().copied(),
            Lifetime::AcceptedCommand,
            1024 * 1024,
        ),
        &Cancellation::default(),
        None,
    )
    .unwrap()
}
fn command(git: &Git, root: &Path, args: &[&str]) -> String {
    let result = raw(git, root, args);
    assert!(result.success(), "{result:?}");
    String::from_utf8(result.stdout).unwrap().trim_end().into()
}
struct Fixture {
    _temp: tempfile::TempDir,
    root: PathBuf,
    git: Git,
}

#[test]
fn rebase_preview_names_rebased_checkout_and_replayed_commit_without_swapping_stages() {
    let f = Fixture::conflict(Encoding::Utf8, false);
    f.cmd(&["merge", "--abort"]);
    let replayed = f.cmd(&["rev-parse", "main"]);
    let onto = f.cmd(&["rev-parse", "side"]);
    assert!(!raw(&f.git, &f.root, &["rebase", "side"]).success());
    let preview = f.preview();
    assert_eq!(preview.current_source, onto);
    assert_eq!(preview.incoming_source, format!("main · {replayed}"));
    assert_eq!(
        preview.sides[1]
            .as_ref()
            .unwrap()
            .content
            .as_ref()
            .unwrap()
            .text,
        "incoming\r\n"
    );
    assert_eq!(
        preview.sides[2]
            .as_ref()
            .unwrap()
            .content
            .as_ref()
            .unwrap()
            .text,
        "current\r\n"
    );
    assert!(
        f.git
            .resolve_conflict(&preview, &Choice::Incoming)
            .unwrap()
            .resolved
    );
    assert_eq!(
        fs::read(f.root.join("conflict.txt")).unwrap(),
        b"current\r\n"
    );
    assert!(f.root.join(".git/rebase-merge").is_dir());
}

#[test]
fn binary_conflicts_reject_text_editing_but_native_side_selection_remains_available() {
    let f = Fixture::conflict(Encoding::Utf8, false);
    f.cmd(&["merge", "--abort"]);
    f.cmd(&["switch", "side"]);
    fs::write(f.root.join("conflict.txt"), b"incoming\0bytes\xff").unwrap();
    f.cmd(&["commit", "-am", "binary incoming"]);
    f.cmd(&["switch", "main"]);
    fs::write(f.root.join("conflict.txt"), b"current\0bytes\xfe").unwrap();
    f.cmd(&["commit", "-am", "binary current"]);
    assert!(!raw(&f.git, &f.root, &["merge", "--no-edit", "side"]).success());
    let preview = f.preview();
    assert!(!preview.can_edit);
    assert!(preview.sides[1].as_ref().unwrap().content.is_none());
    assert_eq!(
        f.git
            .resolve_conflict(&preview, &Choice::Edited("text".into()))
            .err()
            .unwrap()
            .code(),
        "ConflictTextLimit"
    );
    assert_eq!(
        fs::read(f.root.join("conflict.txt")).unwrap(),
        b"current\0bytes\xfe"
    );
    assert!(
        f.git
            .resolve_conflict(&preview, &Choice::Incoming)
            .unwrap()
            .resolved
    );
    assert_eq!(
        fs::read(f.root.join("conflict.txt")).unwrap(),
        b"incoming\0bytes\xff"
    );
    assert!(
        f.cmd(&["ls-files", "--unmerged", "--", "conflict.txt"])
            .is_empty()
    );
}
impl Fixture {
    fn conflict(encoding: Encoding, deletion: bool) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("repo");
        fs::create_dir(&root).unwrap();
        let config = temp.path().join("isolated.config");
        fs::write(&config, "[user]\nname=Fixture\nemail=fixture@example.invalid\n[commit]\ngpgsign=false\n[core]\nautocrlf=false\n").unwrap();
        let mut git = Git::discover(None, &Cancellation::default()).unwrap();
        git.environment
            .insert("GIT_CONFIG_GLOBAL".into(), Some(config.into_os_string()));
        git.environment
            .insert("GIT_CONFIG_NOSYSTEM".into(), Some("1".into()));
        command(&git, &root, &["init", "--initial-branch=main"]);
        fs::write(
            root.join("conflict.txt"),
            conflict_text::encode("base\r\n", encoding).unwrap(),
        )
        .unwrap();
        fs::write(root.join("unrelated.txt"), "keep\n").unwrap();
        command(&git, &root, &["add", "conflict.txt", "unrelated.txt"]);
        command(&git, &root, &["commit", "-m", "base"]);
        command(&git, &root, &["switch", "--create", "side"]);
        fs::write(
            root.join("conflict.txt"),
            conflict_text::encode("incoming\r\n", encoding).unwrap(),
        )
        .unwrap();
        command(&git, &root, &["commit", "-am", "incoming"]);
        command(&git, &root, &["switch", "main"]);
        if deletion {
            fs::remove_file(root.join("conflict.txt")).unwrap();
        } else {
            fs::write(
                root.join("conflict.txt"),
                conflict_text::encode("current\r\n", encoding).unwrap(),
            )
            .unwrap();
        }
        command(&git, &root, &["commit", "-am", "current"]);
        let merge = raw(&git, &root, &["merge", "--no-edit", "side"]);
        assert_ne!(merge.exit_code, 0);
        Self {
            _temp: temp,
            root,
            git,
        }
    }
    fn preview(&self) -> Preview {
        let cancel = Cancellation::default();
        let context = Arc::new(self.git.working_copy(&self.root, &cancel).unwrap());
        let change = context
            .entries
            .iter()
            .find(|e| e.path == "conflict.txt" && e.group == Group::Conflicted)
            .unwrap()
            .clone();
        self.git.read_conflict(context, &change, &cancel).unwrap()
    }
    fn cmd(&self, args: &[&str]) -> String {
        command(&self.git, &self.root, args)
    }
}
#[test]
fn native_side_selection_runs_hooks_stages_one_file_and_keeps_merge_pending() {
    let f = Fixture::conflict(Encoding::Utf8, false);
    let preview = f.preview();
    assert!(preview.can_edit);
    assert_eq!(
        preview.sides[0]
            .as_ref()
            .unwrap()
            .content
            .as_ref()
            .unwrap()
            .text,
        "base\r\n"
    );
    assert_eq!(preview.current_source, "main");
    assert!(preview.incoming_source.starts_with("side · "));
    fs::write(
        f.root.join(".git/hooks/post-checkout"),
        "#!/bin/sh\necho native-checkout >> hook-evidence\n",
    )
    .unwrap();
    let result = f.git.resolve_conflict(&preview, &Choice::Incoming).unwrap();
    assert!(
        result.resolved,
        "{} {:?}",
        result.operation.output, result.operation.message
    );
    assert_eq!(
        fs::read(f.root.join("conflict.txt")).unwrap(),
        b"incoming\r\n"
    );
    assert_eq!(
        fs::read_to_string(f.root.join("hook-evidence")).unwrap(),
        "native-checkout\n"
    );
    assert!(f.root.join(".git/MERGE_HEAD").exists());
    assert_eq!(f.cmd(&["show", ":conflict.txt"]), "incoming");
    assert_eq!(
        fs::read_to_string(f.root.join("unrelated.txt")).unwrap(),
        "keep\n"
    );
    assert!(
        !result
            .updated
            .unwrap()
            .entries
            .iter()
            .any(|e| e.group == Group::Conflicted)
    );
}
#[test]
fn deleted_named_side_removes_and_stages_only_the_reviewed_file() {
    let f = Fixture::conflict(Encoding::Utf8, true);
    let preview = f.preview();
    assert!(preview.sides[1].is_none());
    let result = f.git.resolve_conflict(&preview, &Choice::Current).unwrap();
    assert!(result.resolved);
    assert!(!f.root.join("conflict.txt").exists());
    assert!(f.cmd(&["ls-files", "--", "conflict.txt"]).is_empty());
    assert!(f.root.join(".git/MERGE_HEAD").exists());
    assert_eq!(
        fs::read_to_string(f.root.join("unrelated.txt")).unwrap(),
        "keep\n"
    );
}
#[test]
fn edited_bom_utf16_results_preserve_encoding_bom_unicode_and_line_endings() {
    for encoding in [Encoding::Utf16LittleEndianBom, Encoding::Utf16BigEndianBom] {
        let f = Fixture::conflict(encoding, false);
        let preview = f.preview();
        assert!(preview.can_edit);
        assert_eq!(preview.working.as_ref().unwrap().encoding, encoding);
        let text = "שלום 😀\r\nmerged\nlast";
        let encoded = conflict_text::encode(text, encoding).unwrap();
        let result = f
            .git
            .resolve_conflict(&preview, &Choice::Edited(text.into()))
            .unwrap();
        assert!(result.resolved);
        assert_eq!(fs::read(f.root.join("conflict.txt")).unwrap(), encoded);
        assert_eq!(
            raw(&f.git, &f.root, &["show", ":conflict.txt"]).stdout,
            encoded
        );
    }
}
#[test]
fn unresolved_markers_stale_bytes_index_and_attributes_reject_without_overwrite() {
    let f = Fixture::conflict(Encoding::Utf8, false);
    let preview = f.preview();
    let original = fs::read(f.root.join("conflict.txt")).unwrap();
    assert_eq!(
        f.git
            .resolve_conflict(&preview, &Choice::Edited("<<<<<<< remaining\n".into()))
            .err()
            .unwrap()
            .code(),
        "ConflictMarkersRemain"
    );
    assert_eq!(fs::read(f.root.join("conflict.txt")).unwrap(), original);
    fs::write(f.root.join("conflict.txt"), "external\n").unwrap();
    assert_eq!(
        f.git
            .resolve_conflict(&preview, &Choice::Current)
            .err()
            .unwrap()
            .code(),
        "ConflictEditedOutside"
    );
    fs::write(f.root.join("conflict.txt"), &original).unwrap();
    fs::write(
        f.root.join(".gitattributes"),
        "conflict.txt filter=custom\n",
    )
    .unwrap();
    assert_eq!(
        f.git
            .resolve_conflict(&preview, &Choice::Incoming)
            .err()
            .unwrap()
            .code(),
        "CommitStale"
    );
    fs::remove_file(f.root.join(".gitattributes")).unwrap();
    f.cmd(&["update-index", "--chmod=+x", "unrelated.txt"]);
    assert_eq!(
        f.git
            .resolve_conflict(&preview, &Choice::Incoming)
            .err()
            .unwrap()
            .code(),
        "CommitStale"
    );
    assert_eq!(fs::read(f.root.join("conflict.txt")).unwrap(), original);
}
#[test]
fn failed_staging_keeps_the_saved_result_and_unmerged_index_for_recovery() {
    let f = Fixture::conflict(Encoding::Utf8Bom, false);
    // Git's text merge places the side BOM inside conflict markers. Review an
    // explicitly BOM-marked working result to test preservation on failed add.
    fs::write(
        f.root.join("conflict.txt"),
        conflict_text::encode(
            "<<<<<<< current\ncurrent\n=======\nincoming\n>>>>>>> side\n",
            Encoding::Utf8Bom,
        )
        .unwrap(),
    )
    .unwrap();
    let preview = f.preview();
    let old_index = f.cmd(&["ls-files", "--unmerged", "-z"]);
    fs::write(f.root.join(".git/index.lock"), "external lock").unwrap();
    let value = "saved even though staging fails\r\n";
    let result = f
        .git
        .resolve_conflict(&preview, &Choice::Edited(value.into()))
        .unwrap();
    assert!(!result.resolved);
    assert_eq!(
        fs::read(f.root.join("conflict.txt")).unwrap(),
        conflict_text::encode(value, Encoding::Utf8Bom).unwrap()
    );
    assert_eq!(f.cmd(&["ls-files", "--unmerged", "-z"]), old_index);
    assert!(!result.operation.commands.last().unwrap().succeeded());
}
#[test]
fn configured_and_custom_mergetools_stage_native_results_without_config_changes() {
    for saved in [false, true] {
        let mut f = Fixture::conflict(Encoding::Utf8, false);
        f.cmd(&[
            "config",
            "mergetool.fixture.cmd",
            "printf 'configured result\\n' > \"$MERGED\"",
        ]);
        f.cmd(&["config", "mergetool.fixture.trustExitCode", "true"]);
        f.cmd(&["config", "merge.tool", "fixture"]);
        f.git
            .environment
            .insert("MERGETOOL_PROMPT".into(), Some("false".into()));
        let preview = f.preview();
        assert!(preview.tools.contains(&"fixture".into()));
        let configuration = fs::read(f.root.join(".git/config")).unwrap();
        let tool = if saved {
            Tool::Saved(ToolSettings { mode: 2, command: "printf '%s\\n' 'custom  spacing' > \"$MERGED\"\r\nprintf '%s' 'second line' >> \"$MERGED\"".into(), ..Default::default() })
        } else {
            Tool::Configured
        };
        let result = f
            .git
            .resolve_conflict(&preview, &Choice::External(tool))
            .unwrap();
        assert!(
            result.resolved,
            "{} {:?}",
            result.operation.output, result.operation.message
        );
        assert_eq!(
            fs::read_to_string(f.root.join("conflict.txt")).unwrap(),
            if saved {
                "custom  spacing\nsecond line"
            } else {
                "configured result\n"
            }
        );
        assert_eq!(fs::read(f.root.join(".git/config")).unwrap(), configuration);
        assert!(f.cmd(&["ls-files", "--unmerged"]).is_empty());
    }
}
