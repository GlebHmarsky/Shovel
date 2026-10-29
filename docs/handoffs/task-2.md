# Task 2 — Commit draft persistence handoff

2026-10-29. Branch: codex/task-2-commit-drafts. Base: 146b0f3b278c12dc53e732e50d51e93e6d12f685. Worker implementation only; no release or integration claim.

## Implementation

Core/Drafts/CommitDraftStore.cs stores only commit subject/description, schema, opaque identity key and revision. App/ViewModels/CommitDraftCoordinator.cs owns post-display restoration, coalescing, revision tracking and lifetime; CommitDraftPersistence.cs connects it to the existing CommitViewModel. CommitWorkspaceState remains the single mutable subject/body state. Its added metadata survives cache eviction/modal reopen; no parallel editor text state or settings migration was introduced.

ShellChanges supplies the existing identity provider and lazily resolved default store, with an injectable DraftStore seam for isolated fixtures. Construction/history/menu opening does not read draft files or resolve the default storage directory. The existing CommitWindow Loaded → StartLoading route starts restoration after a dispatcher yield, independently of the Git/status scan. A saved draft takes precedence over an automatic merge-message suggestion. Typing before restoration preserves the new text and any different unseen durable draft.

One writer per modal coalesces edits with a 600 ms trailing debounce and a two-second maximum scheduling delay during continuous edits, excluding an in-flight I/O operation. There is one latest pending snapshot, not a per-keystroke task queue. Each storage operation has a five-second cancellation deadline; the cross-profile file lock waits at most two seconds. Commit success acknowledges the captured subject/body and edit generation, clears only that submission, and flushes the state before optional Push. Newer text survives stale success/save/clear completion. Failed/cancelled commits retain the message; successful commit plus failed Push keeps the draft cleared. Successful Git exit with an unknown resulting tip follows the existing success semantics.

The post-commit flush reports the localized “Saving the commit draft state” phase. Closing the modal drains its load/writer and bypasses debounce. The shell already excludes tab/app closure while the modal is active, so normal tab/app shutdown follows that drain without changing App or ShellViewModel shutdown. Storage failure does not disable staging/committing. The existing modal status exposes localized problems; Refresh retries, and an unrecovered close-time error is reported once through the existing interaction boundary. No draft text or exception is logged.

## Storage and retention

Default local directory: %LOCALAPPDATA%\GitGui\CommitDrafts, normally C:\Users\hmars\AppData\Local\GitGui\CommitDrafts for this account. It is shared across Shovel profiles, independent of --data-directory and of repository metadata. It contains plaintext user-authored commit text, protected by the user's ordinary local filesystem permissions; no passwords, authentication state, output, diffs or settings snapshots are inputs to this store.

The canonical file is <key>.json, where key is SHA-256 of the uppercase existing 64-character repository/worktree identity fingerprint. That existing fingerprint includes physical worktree/metadata identities and linked-worktree pointers. Linked worktrees have separate keys; moving/replacing metadata produces a different key rather than inheriting old text. The old identity's record remains available for explicit local recovery.

Writes validate identity, acquire <key>.lock with exclusive file sharing, reread the observed revision, and refuse stale revisions. Each replace uses an owned unique same-directory .tmp file, WriteThrough, an explicit disk flush, identity/cancellation revalidation and atomic File.Replace (or no-overwrite File.Move for the first record). Readers allow delete sharing, preserving an old complete snapshot during replacement. Per-file input/output is capped at 512 KiB; subject/body match the existing 16,384/65,536 character editor bounds, including messages not yet valid for Git's smaller UTF-8 limit.

A successful clear replaces canonical text with a new revision and empty fields. This small tombstone prevents an older profile with an absent-file revision from recreating a cleared draft. No backup containing the submitted canonical message is created. Normal temporary files are removed in finally; an abrupt process/power failure can leave an orphan .tmp, which is never automatically restored or removed. This is not secure erasure.

Corrupt/newer canonical records are never overwritten. A concurrent profile, unreadable canonical record, or different unseen draft can require <key>.<session-copy-id>.recovery.json instead. Each workspace has one stable recovery slot per identity; repeated edits update that slot atomically rather than growing a file per keystroke. Canonical and conflicting text both survive. Restart restores the canonical draft and reports that alternative local copies exist. There is no merge/chooser UI: inspect/copy Subject and Description from the recovery JSON manually. This is an explicit limitation, not an automatic winner selection.

There is no expiry or background cleanup of user drafts, old identity records or other sessions' recovery copies. A writer removes its own recovery copy after a successful canonical save or successful commit of its current copy. Other recovery copies remain. To remove retained text manually, close all Shovel profiles first, copy any needed text, then delete the chosen canonical/recovery/orphan temporary files (or the dedicated draft directory). Empty .lock files contain no text; their handles, not file presence, provide exclusion. Do not remove locks while profiles are running. Corrupt/newer files can be moved aside before Refresh; copying a recovery message into the editor after reopening is the manual recovery path.

## Written checks and registration

tests/GitGui.FoundationChecks/CommitDraftChecks.cs is included by SDK source globbing but intentionally NOT dispatched on this worker branch. RunCommitDraftChecksAsync combines isolated storage/lifetime cases with disposable real-Git and headless modal/shell cases. No tests or fixtures were executed/generated here.

The source covers exact Unicode/line-ending reopen, data-field bounds/privacy, empty revision tombstones, two simultaneous store instances, corrupt/newer/malformed/oversized input, locked canonical/lock files, cancellation after the temporary-file disk flush, unavailable directories, invalid UTF-16, construction without I/O, modal/tab/new-shell restore, early typing and delayed restoration, stale saves/clears, failed/successful/cancelled commits, conflict recovery copies, clean cached reopen after another profile's save, retry, coalescing, linked worktrees, physical metadata replacement and merge-message ordering. Gates and real-hook waits are bounded. All mutation fixtures use the existing isolated-config dispatcher harness and owned temporary paths.

Integrator registration in tests/GitGui.FoundationChecks/ViewModelChecks.cs:
- Add optional bool draftsOnly = false to RunViewModelChecks.
- Include draftsOnly in the existing focused/watchdog expression.
- At the beginning of its nested RunAsync add: if (draftsOnly) { await RunCommitDraftChecksAsync(); return; }
- In the full-suite path add await RunCommitDraftChecksAsync(); next to RunCommitWorkspaceChecksAsync().

Integrator dispatch in Program.cs, matching the existing focused flags:

    if (args.FirstOrDefault() == "--check-drafts")
    {
        var app = new HeadlessApplication(); app.InitializeComponent();
        RunViewModelChecks(draftsOnly: true);
        Console.WriteLine($"{_passed} passed; {_failed} failed. Fixtures: {_root}");
        return _failed == 0 ? 0 : 1;
    }

After authorized integration restore/build, focused command:

    dotnet run --project tests/GitGui.FoundationChecks -c Release --no-build -- --check-drafts

Retain the plain HeadlessApplication/shared resources; never instantiate production App or Show a window. The new shell checks inject DraftStore explicitly. Also inject a fixture CommitDraftStore into the existing ShellViewModel in CommitWorkspaceChecks.cs around line 208, so that older shell/modal coverage never touches the user's default draft directory. Other existing direct CommitViewModel fixtures remain session-only unless they explicitly supply draftStore and identity.

## Static review and integration limits

Static commands: git diff --check passed, and Strings.resx parsed as well-formed XML with no duplicate data keys. No executable code checks ran.

Reviewed identity binding, atomic replacement, revision exclusion, coalescing/lifetime ownership, delayed completion ordering, exact-submission clearing, error localization, and the narrow production call sites. Review also corrected the merge-message race, clean-cache cross-profile restore, malformed schema typing, and unseen-draft replacement during an early edit/commit.

Only CommitViewModel.cs, ShellChanges.cs and appended CommitDraft-prefixed Strings.resx entries are shared production edits. Dedicated source/test/handoff files contain the remaining work. No TODO, owning product/requirements/architecture/roadmap, settings/profile identity, menus/history/branch/release module or version was edited.

Integration still owns dispatch registration, review with the other worker commits, shared resource reconciliation, owning-doc/TODO updates, one version/changelog delivery, restore/build/full and focused checks, and matching preview packaging. Preserve the existing commit/amend/Commit-and-Push regressions, including Push rejection and unknown-tip handling. No push or merge was performed or authorized here.

Explicitly unverified: compilation, test results, physical crash/power-loss durability, disk-full/ACL denial and cross-process profile behavior on Windows, visible modal timing/focus/status/layout/accessibility, live auth, signing, installed migration/update and performance on real repositories. The directory-blocker and typed-failure cases are not physical disk-full/ACL certification. The coordinator's cancellation deadlines cannot forcibly interrupt an unresponsive OS filesystem call; drain can outlast its token. Atomic file replacement is not a transaction with Git commit: process failure between Git success and the draft-clear replace may restore the old message. A crash during the debounce window can lose the last unsaved edits. Identity observation and revision locking coordinate ordinary Shovel instances; they do not provide a transaction against arbitrary external file editors or hostile filesystem replacement.
