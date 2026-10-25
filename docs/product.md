# Product

Status: confirmed intent with explicitly marked UX proposals. Updated 2026-10-25.
Read [requirements](requirements.md) for obligations/tests, [architecture](architecture.md) for mechanisms, and [roadmap](roadmap.md) for phase scope.

## Purpose and audience

**Shovel** is the confirmed product name (2026-10-24). Build a reliable, lightweight Windows 10/11 Git desktop client for approximately 30 initial users working with development repositories, including TypeScript and C++. Combine Fork's restrained interface with Git Extensions' practical capabilities, using original assets and branding. AI will maintain the implementation; personal familiarity with a language does not determine the stack. The application itself does not require AI services.

English ships first; future localization is a foundational requirement. Users install once and receive in-place updates. Measured responsiveness and trustworthy Git behavior take priority over feature count.

**Implemented in 0.6.0:** opening a repository or restoring its tab automatically reads local history, with no approval question or read-consent setting. This is the purpose of the history workspace, not an optional feature. “Read history” does not mean network `git fetch`. Remembered filesystem identity detects ordinary replacement; missing/inaccessible/changed targets show a concrete error, with explicit reopen for replacement. [Architecture](architecture.md#automatic-local-history) owns the limits. Future hooks/scripts/mutations still need separate safeguards.

Tabs select immediately and display their cached page, then validate identity off-thread. A snapshot older than 30 seconds refreshes on tab/window activation; manual Repository → Refresh is always available. No polling or preload of every repository. Refresh and failures keep useful rows with a loading/stale/error notice; identity failure clears the cache. Original Git stderr remains in the Diagnostics tab for the session.

**History workflow (0.7.0):** one **All Commits** view shows ancestry reachable from local branches, remote-tracking branches, tags and current HEAD, including detached HEAD. Reflog-only/unreachable commits and stash-only history are excluded. An unborn HEAD does not hide other branches. The user requested removal of the scope/filter controls; filtering is deferred, and saved current-branch preferences no longer restrict startup. Scope help remains accessible on the history table.

The history table starts directly with commit rows, without visible column headings (user feedback, 2026-10-25). Localized column names remain in cell accessibility help. Header drag resizing is unavailable; description width adapts to the window. The virtualized table places connected graph nodes on the left and branch/tag labels with description, author, local commit date and short hash on the right. Select a row for its complete message, identities, original-offset dates, full hash, parents and full ref names in the bottom panel. Long row labels are abbreviated with full text available in details/tooltips; messages within the read bound are not shortened. Merge nodes have a second ring and accessible parent text; column sorting stays disabled to preserve topology.

Older / Newer navigate 200-row pages with a visible row range in a compact footer. There is no total commit-count ceiling; up to three visited tabs retain one page each, subject to a character-data memory budget. Back navigation retains 128 cursors; Repository → First history page remains available. Loading, end/shallow boundary, empty and errors are distinct; footer tooltips expose the full notice and read/graph limits. Failed/cancelled paging retains the previous page. Ref or traversal-boundary changes block further paging until refresh. Each cached tab retains page, selection and scroll; refresh preserves the current page when refs/HEAD are unchanged, otherwise starts at the first page and keeps the selected hash if present. Rows/page/selection/scroll are session-only. Evicted tabs reload the first page. Missing objects are never fetched implicitly.

Individual branch selection, search/filter, file status, diffs, mutations/tools and sequence recovery remain deferred. At the user's request, toolbar actions and sidebar destinations are visible prototypes: dimmed, disabled and explained in localized tooltips/accessibility help. They do not change workspaces or execute Git. No network requests are made. Remaining workflows below describe the target product; [roadmap](roadmap.md) owns delivery evidence.

Pull displays commits **behind**, Push commits **ahead**, comparing the selected branch with its configured upstream from the displayed local snapshot. Both use that upstream even if a separate push destination is configured. Counts load after history; zero means a measured zero, while a dash means pending, unavailable, cancelled or failed. Tooltips identify the upstream, snapshot time and reason. Detached/unborn/no-upstream/missing-upstream/shallow states never imply synchronization. Refresh failure retains prior counts as stale; counts may lag the server because no fetch occurs. These indicators do not enable Pull/Push.

Selecting a commit highlights the text of other rows with the same exact raw author name and email in light yellow. Comparisons are ordinal/case-sensitive, do not apply mailmap, and never use the committer. Blank email never matches; blank name matches only the same blank name with a nonblank matching email. The selected row keeps normal selection styling. Unrelated text stays white on the dark history surface; graph/ref colors remain independent. High contrast uses system colors; the row accessibility name also states “Same author as the selected commit.”

The application icon is the user-selected front-facing silver shovel head on a blue circular badge, with its shaft cropped at the bottom (confirmed 2026-10-25).

## Interface

Confirmed layout (0.7.0 restores requested prototypes without claiming functional Git/tool actions):
- Repository tabs across the top, preserving order and selection. Right-click a tab for Refresh, Move left/right, Close tab and Close other tabs; actions target that tab. Middle-click closes the clicked tab. Closing the selected tab cancels its read and selects its neighbor; closing an inactive tab leaves current work alone. Closing never deletes repository files.
- File/View/Repository/Window/Help menu row, then Open/Fetch/Pull/Push/Stash at top left. Repository, branch, active operation and its elapsed timer occupy the top-center block, above tabs.
- Explorer, terminal, editor, and configured external-tool actions at top right.
- Sidebar: local changes, history, branches, remotes, tags, stashes, submodules.
- Main history area begins directly below the repository tabs without an intervening heading/filter row. A readable graph uses the available width/height; paging/status are below it. Repository owns Open, Refresh, Cancel read, Git selection, Diagnostics, tab movement/closing and First history page. No inspection toolbar or prominent Trust button.
- Resizable lower details: message, author/committer, dates, hashes, parents, changed files, diffs, file tree.
- Restrained context menus, themes, keyboard access, scalable layout, accessible statuses.

**Proposed refinement:** retain this frame while switching between History and Changes workspaces. Changes gets separate staged/unstaged lists, diff, and commit editor instead of forcing staging into the history layout. Tabs/details retain independent view state per worktree.

Reference boundary: the original attachment was unavailable. A Fork screenshot supplied on 2026-10-25 was inspected and confirms the headerless history layout requested for this refinement. [Fork UI research](research/fork-ui-observations.md) records official Windows screenshots/animations, their age, and observed versus inferred behavior. Broader visual parity is not claimed. Do not copy proprietary assets or assume historical dialog behavior is current.

## Workflows and visible states

| Workflow | Normal interaction | Loading, empty, failure, and recovery |
| --- | --- | --- |
| Open/init/clone | Choose path or remote; validate location/Git availability; open tab. Future executable workflows have separate trust gates | Startup shell remains usable; clone progress/cancel. No tabs, invalid directory, missing Git, or cancelled/partial clone have distinct messages; never delete partial destination automatically. |
| Inspect history | Select commit/file, filter graph, inspect metadata/tree/diff | Incremental results; distinguish empty repository, no matches, missing shallow/partial objects, binary preview, and truncated large content from errors. |
| Prepare commit | Review unstaged/staged differences; stage file/hunk; enter message; commit or explicit amend | Preserve draft/selection through refresh. Show stale-hunk, locked-index, hook/signing failure with Git output and current state. |
| Branch/network/stash | Select concrete refs/remotes and scope; preview consequences; execute | Show missing upstream, divergence, rejected push, authentication request, connection failure, and conflicts. No implicit destructive retry. |
| Conflict/sequence | Persistent operation banner; inspect sources; resolve then explicitly continue/skip/abort where valid | Closing a dialog does not end the operation. Reopen reconstructs state and offers valid recovery, without replaying a previous command. |
| External change/tool | Open configured tool; refresh after edits; retain relevant selection | Track launched sessions; show uncertain completion rather than assuming a launcher exiting means the editor closed. Reconcile external repository changes. |
| Close/tray | Explicit close-versus-exit setting, optional tray | No surprise autostart; active work requires deliberate handling. Hidden window does not imply idle. |

**Proposed interaction rules:** single selection inspects; right-click targets the clicked object and preserves a selection containing it. Menus share commands with toolbar/shortcuts. Small owned dialogs configure operations or confirm scope; large editors handle rebase/merging. Inspection/output views are non-modal where practical. Menus stay on-screen, dialogs restore focus, and keyboard activation is equivalent. A drag gesture may propose an operation but never silently rewrite history.

## Update experience (confirmed)

1. Check asynchronously at startup; optionally check at a configurable interval while running. Manual Check for updates remains available.
2. Show available version and release notes without blocking Git work. Installed version/channel and last update status are visible.
3. Download and verify asynchronously, displaying progress and understandable failures.
4. Allow postponement or explicit **Restart and update**. A downloaded package is not permission to install.
5. Apply only when idle, after the final state check; preserve settings, tab order/selection, drafts, and recovery information.

States: checking, current, available, downloading, verifying, ready, postponed, blocked by work, installing, failed, and recovered. Server unavailability does not block startup or normal use. Security, busy-state definition, and failure acceptance are owned by [requirements](requirements.md#updates-and-distribution).
