# Product

Updated 2026-10-28. [Requirements](requirements.md) owns acceptance; [architecture](architecture.md) owns mechanisms; [roadmap](roadmap.md) owns delivery evidence.

## Purpose and delivery

**Shovel** is a lightweight Windows 10/11 Git client for about 30 initial coworkers using development repositories, including C++/TypeScript. Aim for Fork's restrained layout and Git Extensions' practical capabilities, with original assets. AI maintains the code; the shipped app needs no AI service. English first, localization-ready; install once with in-place updates when distribution is implemented.

**Confirmed 2026-10-27:** deliver usable functionality with its UI. Every requested button/feature must perform its Git/tool action and expose real results, progress and errors. No new placeholder/no-op controls or UI-only completion claims. Existing disabled prototypes are debt to implement or omit as their area is revised, not a permanent design. Temporary disabling is for actual unavailable state/dependencies.

The work environment is already trusted. No repository-trust modal on open, restore, first operation or ordinary dialog opening; no repeated consent checkbox for configured Git programs. Selecting the normal action authorizes it. **Do not introduce extra safety measures or approval layers without a concrete requirement.** Retain normal Git data-loss protections, destructive-action confirmation and required authentication/SSH host verification. This replaces older prototype-retention and routine trust-gate requirements; runtime gaps are tracked in TODO.

## Current workspace

Implemented through **0.14.0**; ordinary action buttons authorize their configured Git behavior without an extra consent step.

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

## Local Changes / Commit (0.13.0)

**Confirmed:** the toolbar's **Commit (count)** and sidebar Local changes open the same resizable owned modal, leaving history/page/selection visible behind it. A partially staged path counts once. Count is a local snapshot, read only for the selected tab after history is published, then refreshed with history or after actions; it can become stale after external edits. A pending/failed count is unknown, never a fabricated zero. No polling or inactive-tab preload. This replaces the embedded Local Changes workspace and its no-startup-status rule.

Left: unstaged/untracked/conflicted files above staged files, each with one action button. Select files (Ctrl/Shift multiselect) for **Stage** or **Unstage**; hold Shift while activating those same buttons for **Stage all** / **Unstage all**. Ignored files are excluded; there is no force-add/discard control. Partially staged files appear in both lists with independent diffs. Rename origins and exact paths remain in help; control characters are escaped in visible rows.

Right: numbered colored unified diff above subject/description and **Amend**. Added/removed markers and accessible descriptions supplement colors; selection retains normal styling. Ignore whitespace, bounded whole-file context and previous/next hunk navigation are real read options. Binary files show old/new byte sizes; submodules show pointers only. Oversized, non-UTF-8, linked paths, nested directories, unchanged content and missing objects have explicit notices; no implicit network retrieval. Image previews, side-by-side diff and conflict editing remain deferred. Automatic reads suppress filters/external diff/textconv; filter/LFS output can differ from normal Git. Explicit whole-file staging honors configured clean filters.

**Selected hunks (0.14.0):** select any line inside a change block, or Ctrl/Shift-select several blocks, then use the single **Stage hunk(s)** / **Unstage hunk(s)** button beside the diff. This selects complete blocks, not individual lines; accessible row descriptions identify each block. Ordinary tracked text modifications are supported with normal diff options; unavailable types/options explain why and retain whole-file actions. The command preserves working files, unrelated index edits, draft and selected file. It clears block selection after refresh; stale previews fail visibly and reinspect without retrying. [Architecture](architecture.md#selected-hunks-0140) owns exact restrictions and race limits.

**Commit** records the staged index with the entered message. **Amend** can preload the previous message when the draft is empty; execution confirms the actual branch/hash because it rewrites that tip. Hold Shift for **Commit and Push**, using the attached branch's configured upstream; a missing upstream disables that variant with a reason. No force push. Push failure retains the new commit and reports the two outcomes separately. Conflicted index entries block commit; after resolving files externally and staging them, a pending merge can be committed here. Other sequence recovery still uses existing tools.

Opening binds cached rows/draft immediately, then reads after layout; it never fetches. First load, clean lists, active reads/actions, stale rows, failures and conflicts remain distinct. Refresh/Cancel is one modal control. Failed/cancelled reads keep useful rows but disable writes until a fresh context is available. New selection cancels obsolete diff work. Stage/commit progress, elapsed time and retained output stay in the modal and top-center summary; closing cancels/drains/reinspects, without claiming undo. History refreshes after the modal closes following a mutation.

Draft, file selection and last output are session-only per tab; successful commit clears the draft, failed commit retains it. Closing the tab/app discards that draft; no crash-safe draft persistence yet. [Architecture](architecture.md#local-changes-and-commit-0130) owns bounds and execution details.

## Explicit branch and remote workflows (0.8.0)

Branch split button defaults to New branch (name, start reference, optional checkout); arrow lists local branches for switching. Start points bind observed commits. New branches do not inherit tracking; Push can set upstream. The commit editor can create an initial commit in an unborn repository.

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

Current frame: File/View/Repository/Window/Help; Open/Commit/Fetch/Pull/Push; repository/branch/operation timer and Branch menu; repository tabs; Local changes modal entry/All Commits sidebar; history; resizable details. Repository owns secondary actions, Git selection and Diagnostics. Icon: selected front-facing silver shovel, cropped shaft, blue circle.

Unfinished standalone Stash/Explorer/Terminal/Editor/Tools buttons, sidebar destinations and force options are omitted. Selected-commit files/diffs, other mutations/tools and in-app sequence recovery remain pending. Future controls ship with functionality. Filtering is revisited only when requested.

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
