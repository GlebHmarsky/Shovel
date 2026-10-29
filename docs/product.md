# Product

Updated 2026-10-29. [Requirements](requirements.md) owns acceptance; [architecture](architecture.md) owns mechanisms; [roadmap](roadmap.md) owns delivery evidence.

## Purpose and delivery

**Shovel** is a lightweight Windows 10/11 Git client for about 30 initial coworkers using development repositories, including C++/TypeScript. Aim for Fork's restrained layout and Git Extensions' practical capabilities, with original assets. AI maintains the code; the shipped app needs no AI service. English first, localization-ready; install once with in-place updates when distribution is implemented.

**Confirmed 2026-10-27:** deliver usable functionality with its UI. Every requested button/feature must perform its Git/tool action and expose real results, progress and errors. No new placeholder/no-op controls or UI-only completion claims. Existing disabled prototypes are debt to implement or omit as their area is revised, not a permanent design. Temporary disabling is for actual unavailable state/dependencies.

The work environment is already trusted. No repository-trust modal on open, restore, first operation or ordinary dialog opening; no repeated consent checkbox for configured Git programs. Selecting the normal action authorizes it. **Do not introduce extra safety measures or approval layers without a concrete requirement.** Retain normal Git data-loss protections, destructive-action confirmation and required authentication/SSH host verification. This replaces older prototype-retention and routine trust-gate requirements; runtime gaps are tracked in TODO.

## Current workspace

Implemented through **0.24.0**; ordinary action buttons authorize their configured Git behavior without an extra consent step.

Branch menu: delete local branch, set upstream to an existing remote-tracking branch, or unset upstream. Options use cached refs; submission validates targets. Deletion confirms branch/tip and retains Git's merged and checked-out-worktree protections. No force deletion or remote operation is implied.

Commit drafts now survive closing tabs/restarting. Restore happens after the modal appears; newer typing wins. Successful commit clears only its submitted text, while failed commit retains it. Storage errors remain visible without blocking Git; conflicting copies can require manual recovery. See the persistence limits in architecture.

- Open/restored tabs read local history automatically, without consent settings. Read does not mean fetch. Selected saved tab loads first; others load on selection.
- Tabs immediately show cached rows, then validate identity off-thread. Snapshots older than 30 seconds refresh on tab/window activation; Repository → Refresh is manual. No polling/preload. Loading/failure keeps rows with stale/error status; missing/replaced/inaccessible identity clears rows. Explicit reopen binds a changed location.
- **All Commits:** ancestry of local branches, remote-tracking branches, tags, current HEAD and saved stashes, including detached HEAD. Other reflog-only and unreachable history is excluded. Unborn HEAD does not hide other branches. No scope/filter selector; legacy narrower settings are ignored; scope help is accessible.
- Headerless virtualized history begins directly below repository tabs. Graph, ref badges, description, author, local date and short hash occupy fixed topology order; sorting/header-drag resizing are unavailable. Description adapts to width. Merge nodes have a hollow marker and parent help.
- Selecting a row exposes full message, author/committer, original-offset dates, hash, parents and full refs below. Row labels are bounded previews with full tooltip/details; parsed messages within the read limit remain complete.
- Older/Newer show 200-row pages and range; no total commit ceiling. Cache: three visited tabs, one page each, character budget; 128 back cursors plus First history page. Page/selection/scroll are session-only. Eviction reloads page one. Refresh preserves page for unchanged refs/HEAD; otherwise restarts, retaining the selected hash if present.
- Loading, end/shallow boundary, empty and errors are distinct; footer help exposes limits. Failed/cancelled paging retains rows; ref/boundary changes require refresh. Missing objects are never fetched implicitly. Original Git stderr remains in session Diagnostics.
- Tab context menu: Refresh, Move left/right, Close, Close others; acts on clicked tab. Middle-click closes it. Active close cancels its read/selects a neighbor; inactive close preserves selected work. Closing never deletes repositories.

Pull shows **behind**, Push **ahead**, comparing the configured upstream from the local snapshot, even with a separate push destination. Counts load after rows. Zero is measured; dash means pending/unavailable/cancelled/failed. Help includes upstream/time/reason. Detached/unborn/no-upstream/missing-upstream/shallow cannot imply synchronization. Failed refresh retains stale counts; no server freshness claim without fetch.

Same-author peers use a muted yellow **row background**, retaining ordinary text color. Matching remains exact raw name/email ordinally and case-sensitively, without mailmap/committer; nonblank email required. Selected rows keep normal selection styling. High contrast suppresses the peer fill; accessible text explains the relation. Graph lanes use compact straight runs/rounded joins and hollow merge markers; selection preserves lane colors. Topology and pagination are unchanged.

## Workspace toolbar and appearance (0.24.0)

Repository tabs share the entire strip equally (one fills it, two split it, etc.); long names truncate with full-path tooltips. Existing keyboard selection, reorder/close menus and middle-click close remain.

Top-right **Open in** offers File Explorer, Rider and VS Code; **Console** opens Git Bash in the selected repository (also Ctrl+Alt+T; Explorer Ctrl+Alt+O). **Appearance** switches Dark/Light immediately and remembers it. Native title bars follow the palette where supported; Windows high contrast takes precedence. No Work/environment button is added.

Opening these menus performs no Git, tool discovery or filesystem preparation. Selecting a tool captures that repository, discovers a supported executable off-thread, then hands it the folder/project using literal arguments. Rider prefers a single root-level `.uproject`, then `.slnx`, `.sln`, `.csproj`; ambiguous/no matches open the folder. Nested projects are not searched. A missing tool prompts for its `.exe`, remembered in the profile; no dependency is installed. Loading/failure appears in the status line without clearing history. External tools control their own lifetime; Shovel reports launch dispatch, not successful project loading, and never kills them on exit. Refresh/activation observes subsequent repository changes through the existing workflow.

## Repository sidebar (0.23.0)

Expandable **Branches**, **Remotes**, **Tags**, **Stashes** and **Submodules** use local snapshots. Lists refresh after first history, on stale activation or explicit Refresh; there is no automatic fetch, polling or inactive-tab preload. Empty groups are honest empty states; pending/failure text is explicit and failed refresh retains cached entries. Menus open from available data without preparation. The sidebar resizes and scrolls.

- **Branches:** current checkmark and local upstream counts (↑ ahead / ↓ behind). Single click locates its commit; double click or Enter switches. Right-click offers Switch, Show in history, Delete, Set upstream and Unset upstream. Existing clean-state/worktree/deletion protections apply. Counts reflect fetched local refs, not live server state; no upstream has no count, pending shows an ellipsis and removed upstream is identified.
- **Remotes:** configured remotes and their fetched remote-tracking branches. Click a branch to locate it; remote context menu Fetch performs the explicit fetch. Remote creation/editing/deletion remains deferred.
- **Tags:** click to locate the tagged commit, including older pages; no checkout or filtering. Tags pointing to non-commit objects have no history row and are omitted.
- **Stashes:** inline saved messages/selectors; click selects the stash commit with its label in history. Its saved index/untracked parents can also appear in the graph. Right-click Apply/Pop/Drop uses real existing actions; Drop confirms the actual saved target. These quick actions restore worktree content without restoring staged state. Repository → Stashes retains detailed inspection/Create and the Restore staged state option.
- **Submodules:** indexed gitlinks, including uninitialized ones. Click opens an initialized child as another repository tab. Right-click **Initialize / Update** checks out its recorded commit (may download); **Sync URL** copies configured URL changes. No force, recursive update, implicit stage or commit. Missing/uninitialized paths explain how to initialize; dirty collisions/active child operations fail with retained output. Add/remove/deinit/recursive/remote-latest management is deferred.

Navigation retains useful history while locating an uncached commit asynchronously, selects it and scrolls into view. All Commits returns to the first page after a jump. Read/time limits report failure rather than silently omit data; [architecture](architecture.md#repository-sidebar-0230) owns limits, including the current 256-saved-stash bound. A jump starts a fresh graph lane layout for that page; normal Older paging carries it forward.

## Committed changes (0.15.0)

Select a history commit and open the lower **Changes** tab. Its file list and colored/numbered unified diff load asynchronously, starting with the first file. The selected commit's metadata remains in **Commit**. Compare with parent 1 by default; merge commits offer each parent explicitly. Initial commits compare with the empty tree, while incomplete/shallow ancestry reports a limitation rather than pretending to be an initial commit. Empty commits show zero changed files. Binary files show stored old/new byte sizes; submodules remain pointer-only.

Amber **M**, green **+**, red **−** mark modified, added and deleted files here and in Local Changes; rename arrows and tooltips expose original names. Selection uses normal selection text; high contrast uses system colors and retains symbols/accessibility labels. Working files/index do not supply committed content. Raw patches/errors remain in the output expander. Changed commit/parent/tab cancels obsolete reads; same-comparison refresh failure retains useful content with an error. No preload/polling or startup history delay. Resize the lower panel/file-list divider; Changes reserves usable space at minimum window size.

## New local repository (0.21.0)

File or Repository → **New repository…** opens a fixed-size, close-only dialog immediately. Enter an absolute destination or Browse to an existing folder, and optionally enter the initial branch. Blank uses Git's default. Opening/editing options performs no Git or filesystem preparation; Create closes the dialog and starts validation/execution with top-center progress, Cancel and retained Operation output.

Create supports missing or populated ordinary folders, preserving files without staging, committing or adding a remote. The created worktree opens automatically with an honest no-commits state and subsequent local file count. An existing worktree root opens unchanged; the initial-branch option does not rename it. Failure retains the previous tab and explains the cause. Cancellation/partial failure keeps created files and opens the result only when fresh inspection confirms it is usable; it is not rollback. [Supported destinations and limits](architecture.md#local-repository-initialization-0210).

## Local Changes / Commit (0.13.0)

**Confirmed:** the toolbar's **Commit (count)** and sidebar Local changes open the same resizable owned modal, leaving history/page/selection visible behind it. A partially staged path counts once. Count is a local snapshot, read only for the selected tab after history is published, then refreshed with history or after actions; it can become stale after external edits. A pending/failed count is unknown, never a fabricated zero. No polling or inactive-tab preload. This replaces the embedded Local Changes workspace and its no-startup-status rule.

Left: unstaged/untracked/conflicted files above staged files, each with one action button. Select files (Ctrl/Shift multiselect) for **Stage** or **Unstage**; hold Shift while activating those same buttons for **Stage all** / **Unstage all**. Ignored files are excluded; there is no force-add/discard control. Partially staged files appear in both lists with independent diffs. Rename origins and exact paths remain in help; control characters are escaped in visible rows.

Right: numbered colored unified diff above subject/description and **Amend**. Added/removed markers and accessible descriptions supplement colors; selection retains normal styling. Ignore whitespace, bounded whole-file context and previous/next hunk navigation are real read options. Binary files show old/new byte sizes; submodules show pointers only. Oversized, non-UTF-8, linked paths, nested directories, unchanged content and missing objects have explicit notices; no implicit network retrieval. Image previews, text side-by-side diff and conflict editing remain deferred. Automatic reads suppress filters/external diff/textconv; filter/LFS output can differ from normal Git. Explicit whole-file staging honors configured clean filters.

**Selected hunks (0.14.0):** select any line inside a change block, or Ctrl/Shift-select several blocks, then use the single **Stage hunk(s)** / **Unstage hunk(s)** button beside the diff. This selects complete blocks, not individual lines; accessible row descriptions identify each block. Ordinary tracked text modifications are supported with normal diff options; unavailable types/options explain why and retain whole-file actions. The command preserves working files, unrelated index edits, draft and selected file. It clears block selection after refresh; stale previews fail visibly and reinspect without retrying. [Architecture](architecture.md#selected-hunks-0140) owns exact restrictions and race limits.

**Commit** records the staged index with the entered message. **Amend** can preload the previous message when the draft is empty; execution confirms the actual branch/hash because it rewrites that tip. Hold Shift for **Commit and Push**, using the attached branch's configured upstream; a missing upstream disables that variant with a reason. No force push. Push failure retains the new commit and reports the two outcomes separately. Conflicted index entries block commit; after resolving files externally and staging them, a pending merge can be committed here. Other sequence recovery uses the sidebar/Repository controls described below.

Opening binds cached rows/draft immediately, then reads after layout; it never fetches. First load, clean lists, active reads/actions, stale rows, failures and conflicts remain distinct. Refresh/Cancel is one modal control. Failed/cancelled status reads keep useful rows but disable writes until a fresh context is available. New selection cancels obsolete diff work. Static action results and retained output stay in the modal; command duration stays at top center. Closing cancels/drains/reinspects, without claiming undo. History refreshes after the modal closes following a mutation.

**0.16.0 binary comparison:** old/new cards show file extension, a generic file icon and size below in binary units plus exact bytes. Added/deleted sides say “No file,” distinct from an empty file. LFS labels and logical asset sizes come from bounded local pointer metadata; no asset download or helper is needed. Text files keep the normal numbered diff; in-app image preview remains deferred. The same cards serve historical Changes.

**0.16.0 file-action feedback:** file Stage/Unstage updates affected rows from real Git status before a wider background scan. Keep selection on the same path when its side changes. A short trailing delay coalesces successive actions; cached rows remain usable, newer generations supersede old reads and a failed full refresh visibly marks retained rows stale. Bottom activity bars and the ticking commit footer are removed. Static results/errors, Refresh/Cancel, in-memory phase timings and command-only top-center duration remain. Ordinary background work adds no throbber or polling.

File selection and last output remain session-only. Draft persistence in 0.22.0 supersedes the old session-only draft policy; successful commit clears the submitted draft, failed commit retains it.

## Local Changes file menus (0.19.0)

Right-click a staged or unstaged/untracked/conflicted row for **Stage/Unstage**, **Stage all/Unstage all**, **Open**, **Show in File Explorer**, **Copy path** and **Copy full path**. A selected row keeps its multiselection; a different row becomes the single target. Blank space has no file menu. The menu captures its targets, so refresh cannot silently redirect a command; reopen a stale menu. Opening starts no Git/filesystem preparation. A right-click selection's diff waits until the menu closes; existing useful content remains visible.

Stage/Unstage operates on the captured selection; the explicit All entry uses all rows on that side. Shift does not expand these menu commands. In a file grid, **Enter** stages/unstages selected files, **Ctrl+Shift+Alt+S** acts on all listed files, **Ctrl+C / Ctrl+Shift+C** copies relative/full paths, and **Ctrl+Shift+Alt+O** opens one file. **Shift+F10 / Menu key** uses the native context menu; Enter inside the popup activates its highlighted item. Shortcuts are scoped to file grids/menus, leaving message editing and diff selection alone.

Open/Explorer require one selected row. Open uses the **current working file** and its Windows default application, even from the staged list; it does not export staged content. A deleted row explains why Open is unavailable; Explorer selects an existing file or opens the nearest existing containing folder. Copies preserve exact paths, one per line. Clipboard/default-app/permission failures are localized without logging contents. File handoff is asynchronous and bounded, but an already sent Windows request cannot be retracted. External application lifetime is not tracked; refresh after editing.

### Selected-file actions (0.20.0)

**Discard changes…** (Delete in a file grid) opens immediately with the repository, HEAD and exact selected paths/rename endpoints. Cancel is the default. Unstaged tracked files restore from the index, preserving staged content; staged selections restore **both index and working files to HEAD**, including unstaged edits at those paths. Selected untracked files and unborn staged additions are permanently removed, without a recycle bin or promise of undo. Conflicted files, submodule directories and linked paths are excluded. Submission rechecks state; cancellation/failure can leave partial results, which are reinspected without automatic cleanup.

**Stash selected files…** shows the selected paths and optional message. It saves both staged and unstaged content for those paths, plus selected untracked files; unrelated index/worktree content stays in place. Inspect/apply/pop/drop the result in Stashes. Git can reject applying into a dirty index. A stash saved before cancellation remains available; partial index cleanup is reported separately, never retried automatically.

**External Diff** runs the configured Git diff tool for one file and the selected staged/unstaged side; an untracked file compares against empty content. Missing configuration and tool failures are explained after submission. The command remains tracked until its process exits or is cancelled; a tool that detaches itself cannot be tracked afterward.

**History… / Blame / Timeline…** opens a resizable window before reading. History follows the captured HEAD's **first-parent file ancestry and renames**, with 200-row pages and commit metadata/real changed-file diffs; it is distinct from main-window All Commits. Blame shows line, author, date, hash and committed text at that tip, excluding working edits. Refresh rereads the captured tip; reopen for a new HEAD. Loading retains existing rows; no history, missing objects, unsupported/binary blame and bounded-read failures are explicit. No automatic fetch.

**Ignore** offers literal selected paths or their extensions throughout the repository, for untracked files only. It appends root `.gitignore` rules without removing files or untracking content; nested overrides are reported. **LFS** adds exact-path tracking/reset rules to root `.gitattributes`, preserving other rules. Tracking needs existing Git LFS/filter setup; neither action installs hooks, migrates history, downloads or restages content. Review/stage the rule file and affected files yourself; more-specific attribute overrides are reported.

**Save as Patch…** exports selected staged or unstaged/untracked changes, including binary patches, through a save picker with overwrite confirmation. It never stages/discards source files; failed/cancelled writes preserve an existing destination. Limits and configured-filter differences are described in [architecture](architecture.md#selected-file-actions-0200). Menu actions retain captured targets and Shift does not expand their scope.

## Stashes (0.18.0)

Repository → **Stashes** opens a resizable owned window over retained history; the sidebar now lists entries directly. Opening uses the tab's cached list/message immediately, then refreshes local stashes in the background; no remote queries, initial Git prerequisite or polling. Select an entry for its full hash/message and stored files/diffs: all tracked changes, saved staged changes, saved unstaged changes, or saved untracked files. Text/binary/LFS views are shared with historical Changes; current working files never supply saved content.

**Create stash** accepts an optional message and unchecked **Include untracked files**. It saves staged/unstaged changes and removes those saved changes from the worktree; ignored files and submodule contents are excluded. **Apply** restores the selected entry and keeps it. **Pop** applies first and removes that entry only after successful application with no detected conflicts and an unchanged stash list. Unchecked **Restore staged state** controls both Apply and Pop. **Drop** confirms the exact repository, stash selector/hash/message and loss of the saved entry; it does not change working files and offers no guaranteed undo.

Empty lists, loading, stale refresh failures and command results are explicit. Selection follows the saved hash across refreshed ordinal numbers; useful list rows/draft survive failures and modal reopening during the session. Failed list reads disable selected-entry writes until refresh succeeds. Commands expose top-center duration/Cancel and retained in-memory output in the modal. Close cancels/drains work; history/counts refresh afterward. No automatic retry or rollback.

Conflicts or incomplete restoration keep the stash; partial Pop success explicitly says when changes were applied but removal was not confirmed. Do not blindly apply again. Resolve files externally and stage with Local Changes; stash conflicts have no Continue/Skip/Abort sequence. [Architecture](architecture.md#stashes-0180) owns limits and external-client races.

## Conflict recovery (0.17.0)

Detected merge, rebase, cherry-pick and revert operations expose **Continue**, **Abort**, and **Skip** when a current replayed commit is known. Controls appear in the sidebar and Repository → Recovery from the cached snapshot; opening them performs no Git or filesystem work. Reopening a repository rediscovers Git's state rather than replaying a saved action.

Open **Local Changes**, resolve files in your editor and Stage the resolutions, then close that modal and choose **Continue**. Continue uses Git's existing message; it refuses unresolved index entries. A subsequent conflict keeps recovery controls available. Abort confirms the repository, branch and observed original tip; Skip also identifies the current commit and warns that resolution edits will be discarded. Git may fail to restore all earlier uncommitted changes. Neither action promises complete undo.

Commands use top-center progress/Cancel and retained Operation output. Existing history remains visible. Failures/cancellation re-read state; no automatic abort, stash deletion, lock cleanup or retry. A conflict while restoring an auto-stash remains a conflict even after the sequence ends: use Local Changes and inspect the preserved entry in Stashes. Mailbox (`git am`), ambiguous/oversized state and interactive todo/message editing remain unsupported. Hooks, signing and existing rebase exec steps remain enabled; unsupported input still requires existing tools. Recovery is not a built-in conflict editor.

## Explicit branch and remote workflows (0.8.0)

Branch split button defaults to New branch (name, start reference, optional checkout); arrow lists local branches for switching. Start points bind observed commits. New branches do not inherit tracking; Push can set upstream. The commit editor can create an initial commit in an unborn repository.

**Confirmed 2026-10-28:** operation dialogs open fully editable from the tab's local snapshot, with no Git queries, remote checks or preparation on opening. Pull, Fetch, Push and branch options are fixed-size, with only Close in the title bar (no resizing/minimize/maximize); long content scrolls. Remote names include configured but unfetched remotes; branch choices reflect locally known refs. Use explicit Fetch to discover new remote branches, or Refresh after external local configuration changes. Target help explains that execution uses the named remote's current configuration. Fetch chooses one/all remotes; Push chooses source, remote and destination with unchecked all-tags/set-upstream options.

Ordinary UI interactions must respond promptly, without a loading prerequisite. Necessary command validation/execution runs after submission with visible progress. Explain and discuss any unavoidable new UI wait rather than silently introducing one. Pull still fetches its selected branch before integrating; merely opening any dialog never fetches.

**Confirmed 2026-10-28:** Pull defaults to **Rebase**, with **Merge** as the alternative. Rebase replays local commits and changes their hashes; Merge preserves them and creates a merge commit for divergence. An initially unchecked **Automatically stash and reapply local changes** checkbox uses Git's tracked-file auto-stash. Untracked files remain in place; staged content can return unstaged. Git protects ordinary collisions/dirty state; Shovel no longer rejects all untracked files. Conflicts retain Git recovery state and original output, with in-app Continue/Skip/Abort and retained output; a conflict during stash restoration must not appear as successful completion. See Conflict recovery above; conflict editing remains deferred.

Pull preserves existing sparse-checkout patterns and only updates superproject references to submodules. Submodule contents are neither updated nor included in auto-stash; use existing tools for those workflows. Branch switching in these repository modes remains deferred.

Normal action buttons execute without an additional trust checkbox. **Submitting Pull closes the options modal immediately**; the same shared flow applies to Fetch/Push/branch actions. The main window shows the actual phase (checking targets, fetching, checking fetched commit, updating local branch), elapsed time and indeterminate progress at top center. Existing history stays visible/selectable. Cancel is available there and in Repository; cancellation is unavailable during fresh inspection. Missing targets/dependencies disable actions with localized help.

Operation details opens below with the repository, full result explanation and bounded original/redacted Git output. Success/failure/cancellation persists at top center and in details after reinspection. A failed refresh is distinct from Git success. One last result is kept in memory, only shown on its repository tab; cancelling a later options dialog preserves it. No percentage is invented. Authentication prompts are owned by the main window after options close. Options/execution/reconciliation still blocks tab navigation/new Git actions/window close; attempting close shows operation details. Cancellation stops owned work and reinspects; it does not undo changes. Related caches become stale after every attempt; retry uses fresh targets. No automatic retries/abort/lock removal; auto-stash occurs only when selected for Pull.

Badges distinguish local/tag/remote refs with original drawings. Check marks current tip, including detached HEAD. Only a local branch and its configured same-tip upstream share a local-name badge with remote icon; unrelated remotes stay separate. Full refs and relationship appear in help/accessibility. Remote icons describe local tracking tips.

## Authentication prompts (0.9.1)

Explicit Fetch/Pull/Push can open owned Git authentication prompts through Git/bundled OpenSSH askpass. Show operation/repository, plain request and Waiting for authentication input. Replies are masked, including usernames/host-key answers; host acceptance is explicit after fingerprint verification. OpenSSH confirmation has Approve request; Enter defaults to Cancel. Informational prompts last until helper exit/cancel.

Closing/cancelling stops and reconciles the operation; timeout is two minutes, with bounded requests and connection failures shown. Shovel saves no credentials/prompts; configured Git helpers may store credentials and OpenSSH may save accepted keys. Browser/GCM sign-in, signing, custom SSH and interactive hooks still need existing tools. Live provider/focus/input behavior is unverified. History never authenticates.

## Interface and remaining workflows

**Confirmed 2026-10-28:** default dark client area inspired by the supplied screenshot: distinct chrome, repository tabs, sidebar, history and details surfaces. Shared hover/pressed/disabled/selection/focus states extend to existing dialogs; lower tabs use an orange underline. [Theme](theme.md) owns colors and conventions. Shovel title bars now match the dark chrome through native Windows styling, with system high-contrast fallback; pickers/message boxes remain OS-owned. 0.24.0 adds live Dark/Light selection and matching native chrome; further polish remains deferred.

Current frame: File/View/Repository/Window/Help; Open/Commit/Fetch/Pull/Push; repository/branch/operation timer and Branch menu; repository tabs; Local changes modal entry/All Commits sidebar; history; resizable details. Repository owns secondary actions, Git selection and Diagnostics. Icon: selected front-facing silver shovel, cropped shaft, blue circle.

Unfinished Explorer/Terminal/Editor/Tools buttons, sidebar destinations and force options are omitted. Stashes is available in the sidebar and Repository menu. Historical file-tree browsing, other mutations/tools and conflict editing remain pending. Future controls ship with functionality. Filtering is revisited only when requested.

**Proposed refinements:** context menus target the clicked object while preserving a containing multiselection; shared commands serve menus/toolbar/shortcuts. Large rebase/merge editors show real plans/context. Dragging proposes an operation, never rewrites immediately. The previously proposed separate embedded Changes workspace is superseded by the confirmed modal design above.

| Target workflow | Behavior and failure states |
| --- | --- |
| Open/init/clone | Path/remote choice; async progress/cancel; distinct missing Git/invalid path/empty state. Preserve partial clone destinations for explicit recovery. |
| History/files/diffs | Incremental real metadata/tree/content; distinguish no matches, shallow/missing objects, binary and oversized content from errors. |
| Stage/commit/amend | Real file/hunk/index changes; preserve draft/selection; show stale-hunk, lock, hook/signing failures with current Git state. |
| Branch/network/stash | Concrete refs/remotes/scope; explain divergence, rejection, auth/network failure/conflicts; no destructive retry. |
| Conflict/sequence | Persistent context banner; valid resolve/continue/skip/abort. Reopen reconstructs observed state; dismissing a dialog never ends/replays a sequence. |
| External tools | Track sessions, reconcile edits and retain selection; launcher exit alone may not mean editor completion. |
| Close/tray | Optional explicit close-versus-exit setting; no surprise autostart; hidden does not mean idle. |

Reference: original attachment unavailable; 2026-10-25 screenshot confirmed headerless history. [Fork research](research/fork-ui-observations.md) separates old official screenshots/animations from inference. No proprietary assets or broader parity claim.

## Update experience (confirmed)

Deferred implementation: asynchronous startup check, optional configurable periodic checks and manual check; visible installed/available version, channel, notes and status. Download/verify with progress/failures; allow postponement or explicit **Restart and update**. Download never authorizes installation. Apply only after final idle check; preserve settings/tabs/drafts/recovery data. Unavailable server must not block startup/Git.

States: checking, current, available, downloading, verifying, ready, postponed, blocked by work, installing, failed, recovered. [Update requirements](requirements.md#updates-and-distribution) owns install/authenticity/failure acceptance.
