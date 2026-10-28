# Product

Updated 2026-10-28. [Requirements](requirements.md) owns acceptance; [architecture](architecture.md) owns mechanisms; [roadmap](roadmap.md) owns delivery evidence.

## Purpose and delivery

**Shovel** is a lightweight Windows 10/11 Git client for about 30 initial coworkers using development repositories, including C++/TypeScript. Aim for Fork's restrained layout and Git Extensions' practical capabilities, with original assets. AI maintains the code; the shipped app needs no AI service. English first, localization-ready; install once with in-place updates when distribution is implemented.

**Confirmed 2026-10-27:** deliver usable functionality with its UI. Every requested button/feature must perform its Git/tool action and expose real results, progress and errors. No new placeholder/no-op controls or UI-only completion claims. Existing disabled prototypes are debt to implement or omit as their area is revised, not a permanent design. Temporary disabling is for actual unavailable state/dependencies.

The work environment is already trusted. No repository-trust modal on open, restore, first operation or ordinary dialog opening; no repeated consent checkbox for configured Git programs. Selecting the normal action authorizes it. **Do not introduce extra safety measures or approval layers without a concrete requirement.** Retain normal Git data-loss protections, destructive-action confirmation and required authentication/SSH host verification. This replaces older prototype-retention and routine trust-gate requirements; runtime gaps are tracked in TODO.

## Current workspace

Implemented through **0.12.1**; ordinary action buttons authorize their configured Git behavior without an extra consent step.

- Open/restored tabs read local history automatically, without consent settings. Read does not mean fetch. Selected saved tab loads first; others load on selection.
- Tabs immediately show cached rows, then validate identity off-thread. Snapshots older than 30 seconds refresh on tab/window activation; Repository → Refresh is manual. No polling/preload. Loading/failure keeps rows with stale/error status; missing/replaced/inaccessible identity clears rows. Explicit reopen binds a changed location.
- **All Commits:** ancestry of local branches, remote-tracking branches, tags and current HEAD, including detached HEAD. Excludes reflog-only, unreachable and stash-only history. Unborn HEAD does not hide other branches. No scope/filter selector; legacy narrower settings are ignored; scope help is accessible.
- Headerless virtualized history begins directly below repository tabs. Graph, ref badges, description, author, local date and short hash occupy fixed topology order; sorting/header-drag resizing are unavailable. Description adapts to width. Merge nodes have a second ring and parent help.
- Selecting a row exposes full message, author/committer, original-offset dates, hash, parents and full refs below. Row labels are bounded previews with full tooltip/details; parsed messages within the read limit remain complete.
- Older/Newer show 200-row pages and range; no total commit ceiling. Cache: three visited tabs, one page each, character budget; 128 back cursors plus First history page. Page/selection/scroll are session-only. Eviction reloads page one. Refresh preserves page for unchanged refs/HEAD; otherwise restarts, retaining the selected hash if present.
- Loading, end/shallow boundary, empty and errors are distinct; footer help exposes limits. Failed/cancelled paging retains rows; ref/boundary changes require refresh. Missing objects are never fetched implicitly. Original Git stderr remains in session Diagnostics.
- Tab context menu: Refresh, Move left/right, Close, Close others; acts on clicked tab. Middle-click closes it. Active close cancels its read/selects a neighbor; inactive close preserves selected work. Closing never deletes repositories.

Pull shows **behind**, Push **ahead**, comparing the configured upstream from the local snapshot, even with a separate push destination. Counts load after rows. Zero is measured; dash means pending/unavailable/cancelled/failed. Help includes upstream/time/reason. Detached/unborn/no-upstream/missing-upstream/shallow cannot imply synchronization. Failed refresh retains stale counts; no server freshness claim without fetch.

Same-author peers use light-yellow text, matching exact raw name/email ordinally and case-sensitively, without mailmap/committer. Nonblank email required; blank name matches only blank name. Selected row keeps selection colors; unrelated text stays white on dark history, graph/ref colors independent. High contrast uses system colors; accessible text explains the relation.

## Local Changes (0.12.0)

Select Local changes in the sidebar for a read-only table ordered by Conflicted, Staged, Unstaged, Untracked and optional Ignored. A partially staged path appears independently in both groups. Rows show change kind and exact relative path; selected-file details retain rename origins. Control characters are escaped in rows and preserved in details. No staging, commit, discard or diff controls are claimed in this increment.

The view switches synchronously, then scans off-thread after dispatcher layout. First load shows a loading state; later visits show the tab's cached rows/selection immediately. Repository → Refresh rereads after external edits; Cancel stops the read, retaining explicitly stale rows. Tab/window activation revalidates snapshots older than 30 seconds; there is no polling, startup status scan or preload of other tabs. Workspace choice is per-tab for the session; restart still opens history. Empty, loading, failed/cancelled/stale and conflicted states are explicit; Read output preserves Git errors. Limits fail visibly rather than silently truncating.

Show ignored paths opts into ignored entries (Git may collapse ignored directories). Submodules show changed pointers only, not nested modified/untracked files. Configured clean/process filters are suppressed for automatic read safety, so filter-managed content can differ from ordinary Git status; this is not full LFS/filter parity. [Architecture](architecture.md#local-changes-0120) owns read safeguards and bounds. File details are metadata, not content diffs.

## Explicit branch and remote workflows (0.8.0)

Branch split button defaults to New branch (name, start reference, optional checkout); arrow lists local branches for switching. Start points bind observed commits. New branches do not inherit tracking; Push can set upstream. Unborn repositories need an initial commit through existing tools.

**Confirmed 2026-10-28:** operation dialogs open fully editable from the tab's local snapshot, with no Git queries, remote checks or preparation on opening. Pull, Fetch, Push and branch options are fixed-size, with only Close in the title bar (no resizing/minimize/maximize); long content scrolls. Remote names include configured but unfetched remotes; branch choices reflect locally known refs. Use explicit Fetch to discover new remote branches, or Refresh after external local configuration changes. Target help explains that execution uses the named remote's current configuration. Fetch chooses one/all remotes; Push chooses source, remote and destination with unchecked all-tags/set-upstream options.

Ordinary UI interactions must respond promptly, without a loading prerequisite. Necessary command validation/execution runs after submission with visible progress. Explain and discuss any unavoidable new UI wait rather than silently introducing one. Pull still fetches its selected branch before integrating; merely opening any dialog never fetches.

**Confirmed 2026-10-28:** Pull defaults to **Rebase**, with **Merge** as the alternative. Rebase replays local commits and changes their hashes; Merge preserves them and creates a merge commit for divergence. An initially unchecked **Automatically stash and reapply local changes** checkbox uses Git's tracked-file auto-stash. Untracked files remain in place; staged content can return unstaged. Git protects ordinary collisions/dirty state; Shovel no longer rejects all untracked files. Conflicts retain Git recovery state and original output, with terminal continue/abort guidance; a conflict during stash restoration must not appear as successful completion. In-app conflict recovery remains deferred.

Pull preserves existing sparse-checkout patterns and only updates superproject references to submodules. Submodule contents are neither updated nor included in auto-stash; use existing tools for those workflows. Branch switching in these repository modes remains deferred.

Normal action buttons execute without an additional trust checkbox. **Submitting Pull closes the options modal immediately**; the same shared flow applies to Fetch/Push/branch actions. The main window shows the actual phase (checking targets, fetching, checking fetched commit, updating local branch), elapsed time and indeterminate progress at top center. Existing history stays visible/selectable. Cancel is available there and in Repository; cancellation is unavailable during fresh inspection. Missing targets/dependencies disable actions with localized help.

Operation details opens below with the repository, full result explanation and bounded original/redacted Git output. Success/failure/cancellation persists at top center and in details after reinspection. A failed refresh is distinct from Git success. One last result is kept in memory, only shown on its repository tab; cancelling a later options dialog preserves it. No percentage is invented. Authentication prompts are owned by the main window after options close. Options/execution/reconciliation still blocks tab navigation/new Git actions/window close; attempting close shows operation details. Cancellation stops owned work and reinspects; it does not undo changes. Related caches become stale after every attempt; retry uses fresh targets. No automatic retries/abort/lock removal; auto-stash occurs only when selected for Pull.

Badges distinguish local/tag/remote refs with original drawings. Check marks current tip, including detached HEAD. Only a local branch and its configured same-tip upstream share a local-name badge with remote icon; unrelated remotes stay separate. Full refs and relationship appear in help/accessibility. Remote icons describe local tracking tips.

## Authentication prompts (0.9.1)

Explicit Fetch/Pull/Push can open owned Git authentication prompts through Git/bundled OpenSSH askpass. Show operation/repository, plain request and Waiting for authentication input. Replies are masked, including usernames/host-key answers; host acceptance is explicit after fingerprint verification. OpenSSH confirmation has Approve request; Enter defaults to Cancel. Informational prompts last until helper exit/cancel.

Closing/cancelling stops and reconciles the operation; timeout is two minutes, with bounded requests and connection failures shown. Shovel saves no credentials/prompts; configured Git helpers may store credentials and OpenSSH may save accepted keys. Browser/GCM sign-in, signing, custom SSH and interactive hooks still need existing tools. Live provider/focus/input behavior is unverified. History never authenticates.

## Interface and remaining workflows

**Confirmed 2026-10-28:** default dark client area inspired by the supplied screenshot: distinct chrome, repository tabs, sidebar, history and details surfaces. Shared hover/pressed/disabled/selection/focus states extend to existing dialogs; lower tabs use an orange underline. [Theme](theme.md) owns colors and conventions. Further polish is low-priority; OS title bars/pickers/message boxes remain native and light-mode selection is deferred.

Current frame: File/View/Repository/Window/Help; Open/Fetch/Pull/Push; repository/branch/operation timer and Branch menu; repository tabs; Local changes/All Commits sidebar navigation; history; resizable details. Repository owns secondary actions, Git selection and Diagnostics. Icon: selected front-facing silver shovel, cropped shaft, blue circle.

Unfinished standalone Stash/Explorer/Terminal/Editor/Tools buttons, sidebar destinations and force options are omitted. Pull rebase/merge/auto-stash ship in 0.11.0. Diffs/staging, other mutations/tools and in-app sequence recovery remain pending. Future controls ship with functionality. Filtering is revisited only when requested.

**Proposed refinements:** separate History/Changes workspaces within this frame; Changes has staged/unstaged lists, diff and commit editor, with independent worktree view state. Single selection inspects; right-click targets clicked object while preserving a containing multiselection. Shared commands serve menus/toolbar/shortcuts. Compact owned operation dialogs restore focus; inspection/output can be non-modal; large rebase/merge editors show real plans/context. Dragging proposes an operation, never rewrites immediately.

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
