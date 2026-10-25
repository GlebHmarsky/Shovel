# Product

Status: confirmed intent with explicitly marked UX proposals. Updated 2026-10-25.
Read [requirements](requirements.md) for obligations/tests, [architecture](architecture.md) for mechanisms, and [roadmap](roadmap.md) for phase scope.

## Purpose and audience

**Shovel** is the confirmed product name (2026-10-24). Build a reliable, lightweight Windows 10/11 Git desktop client for approximately 30 initial users working with development repositories, including TypeScript and C++. Combine Fork's restrained interface with Git Extensions' practical capabilities, using original assets and branding. AI will maintain the implementation; personal familiarity with a language does not determine the stack. The application itself does not require AI services.

English ships first; future localization is a foundational requirement. Users install once and receive in-place updates. Measured responsiveness and trustworthy Git behavior take priority over feature count.

**Implemented in 0.5.0:** automatic read-only history for the selected previously approved repository at startup. Explicit Open explains and remembers approval once; older saved paths receive no implicit approval. Repository → Repository approval can approve or revoke. Approval is for audited reads only, not future mutations. Missing/replaced/inaccessible identities stop automatic reading; [architecture](architecture.md#remembered-read-approval) owns the exact identity and limitations.

Tabs select immediately and display their cached page, then validate identity off-thread. A snapshot older than 30 seconds refreshes on tab/window activation; manual Repository → Refresh is always available. No polling or preload of every repository. Refresh and failures keep useful rows with a loading/stale/error notice; identity failure or revocation clears the cache. Original Git stderr remains in the Diagnostics tab for the session.

**History workflow:** default to a clearly labeled **All Commits** selector: ancestry reachable from local branches, remote-tracking branches and tags. It excludes reflog-only/unreachable commits, stash-only history and detached-only tips. **Current branch / HEAD** separately follows the inspected HEAD, including detached HEAD; changing scope never checks out a branch. An unborn HEAD does not hide other branches from All Commits. No filters are active; that is stated beside the scope.

The history table starts directly with commit rows, without visible column headings (user feedback, 2026-10-25). Localized column names remain in cell accessibility help. Header drag resizing is unavailable; description width adapts to the window. The virtualized table places connected graph nodes on the left and branch/tag labels with description, author, local commit date and short hash on the right. Select a row for its complete message, identities, original-offset dates, full hash, parents and full ref names in the bottom panel. Long row labels are abbreviated with full text available in details/tooltips; messages within the read bound are not shortened. Merge nodes have a second ring and accessible parent text; column sorting stays disabled to preserve topology.

Load older commits / Newer commits navigate 200-row pages with a visible row range. There is no total commit-count ceiling; up to three visited tabs retain one page each, subject to a character-data memory budget. Back navigation retains 128 cursors; Repository → First history page remains available. Scope, filters, loading, end/shallow boundary, empty and errors are distinct; read/graph limits remain visible. Failed/cancelled paging retains the previous page. Ref or traversal-boundary changes block further paging until refresh. Each cached tab retains scope, page, selection and scroll; refresh preserves the current page when refs/HEAD are unchanged, otherwise starts at the first page and keeps the selected hash if present. Scope persists across restart; rows/page/selection/scroll do not. Evicted tabs reload the first page. Missing objects are never fetched implicitly.

Individual branch selection, search/filter, file status, diffs, mutations/tools and sequence recovery remain deferred. Unimplemented toolbar buttons, sidebar destinations and details tabs are absent. No network requests are made. Remaining workflows below describe the target product; [roadmap](roadmap.md) owns delivery evidence.

Selecting a commit highlights the text of other rows with the same exact raw author name and email in light yellow. Comparisons are ordinal/case-sensitive, do not apply mailmap, and never use the committer. Blank email never matches; blank name matches only the same blank name with a nonblank matching email. The selected row keeps normal selection styling. Unrelated text stays white on the dark history surface; graph/ref colors remain independent. High contrast uses system colors; the row accessibility name also states “Same author as the selected commit.”

The application icon is the user-selected front-facing silver shovel head on a blue circular badge, with its shaft cropped at the bottom (confirmed 2026-10-25).

## Interface

Confirmed target layout (future Git/tool actions remain absent until implemented):
- Repository tabs across the top, preserving order and selection.
- Fetch, Pull, Push, Stash at top left; repository, branch, and active operation near the center.
- Explorer, terminal, editor, and configured external-tool actions at top right.
- Sidebar: local changes, history, branches, remotes, tags, stashes, submodules.
- Main history area: readable graph; search/filter deferred. A standard top-left Repository menu owns Open, Refresh, Cancel read, Git selection, Diagnostics, approval management, tab movement/closing and First history page. No inspection toolbar or prominent Trust button.
- Resizable lower details: message, author/committer, dates, hashes, parents, changed files, diffs, file tree.
- Restrained context menus, themes, keyboard access, scalable layout, accessible statuses.

**Proposed refinement:** retain this frame while switching between History and Changes workspaces. Changes gets separate staged/unstaged lists, diff, and commit editor instead of forcing staging into the history layout. Tabs/details retain independent view state per worktree.

Reference boundary: the original attachment was unavailable. A Fork screenshot supplied on 2026-10-25 was inspected and confirms the headerless history layout requested for this refinement. [Fork UI research](research/fork-ui-observations.md) records official Windows screenshots/animations, their age, and observed versus inferred behavior. Broader visual parity is not claimed. Do not copy proprietary assets or assume historical dialog behavior is current.

## Workflows and visible states

| Workflow | Normal interaction | Loading, empty, failure, and recovery |
| --- | --- | --- |
| Open/init/clone | Choose path or remote; establish trust and Git availability; open tab | Startup shell remains usable; clone progress/cancel. No tabs, invalid directory, missing Git, or cancelled/partial clone have distinct messages; never delete partial destination automatically. |
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
