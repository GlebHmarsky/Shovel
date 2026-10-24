# Product

Status: confirmed intent with explicitly marked UX proposals. Updated 2026-10-24.
Read [requirements](requirements.md) for obligations/tests, [architecture](architecture.md) for mechanisms, and [roadmap](roadmap.md) for phase scope.

## Purpose and audience

**Shovel** is the confirmed product name (2026-10-24). Build a reliable, lightweight Windows 10/11 Git desktop client for approximately 30 initial users working with development repositories, including TypeScript and C++. Combine Fork's restrained interface with Git Extensions' practical capabilities, using original assets and branding. AI will maintain the implementation; personal familiarity with a language does not determine the stack. The application itself does not require AI services.

English ships first; future localization is a foundational requirement. Users install once and receive in-place updates. Measured responsiveness and trustworthy Git behavior take priority over feature count.

**Implemented through F1c (0.4.0):** open an existing worktree after session trust; inspect branch/unborn/detached HEAD, worktree/common directory, interrupted-operation markers and real named-ref and HEAD commit history. Tabs can be selected, reordered and closed; layout and tab order/selection survive restart. Restored tabs require trust again and run no commands automatically. Trust / Refresh reinspects external changes; Cancel stops the read and leaves refresh available. A failed initial/refresh read leaves an empty table with an explanation; failed older/newer reads retain the existing page. Failures show an explanation and original Git error output in memory. Git can be selected explicitly when standard discovery fails.

**History workflow:** default to a clearly labeled **All Commits** selector: ancestry reachable from local branches, remote-tracking branches and tags. It excludes reflog-only/unreachable commits, stash-only history and detached-only tips. **Current branch / HEAD** separately follows the inspected HEAD, including detached HEAD; changing scope never checks out a branch. An unborn HEAD does not hide other branches from All Commits. No filters are active; that is stated beside the scope.

The virtualized table places connected graph nodes on the left and branch/tag labels with description, author, local commit date and short hash on the right. Select a row for its complete message, identities, original-offset dates, full hash, parents and full ref names in the bottom panel. Long row labels are abbreviated with full text available in details/tooltips; messages within the read bound are not shortened. Merge nodes have a second ring and accessible parent text; column sorting stays disabled to preserve topology.

Load older commits / Newer commits navigate 200-row pages with a visible row range. There is no total commit-count ceiling; only the current page is retained as rows/messages. Scope, filters, loading, end/shallow boundary, empty and errors are distinct; read/graph limits remain visible. Failed/cancelled paging retains the previous page. Ref or traversal-boundary changes require explicit refresh before paging continues. Tab changes clear history; restored tabs still require trust and refresh. Missing objects are never fetched implicitly.

Individual branch selection, search/filter, file status, diffs, mutation/tool buttons and sequence recovery actions remain unavailable. No network requests are made. Remaining workflows below describe the target product; [roadmap](roadmap.md) owns delivery evidence.

## Interface

Confirmed layout:
- Repository tabs across the top, preserving order and selection.
- Fetch, Pull, Push, Stash at top left; repository, branch, and active operation near the center.
- Explorer, terminal, editor, and configured external-tool actions at top right.
- Sidebar: local changes, history, branches, remotes, tags, stashes, submodules.
- Main history area: readable graph, search, and filters.
- Resizable lower details: message, author/committer, dates, hashes, parents, changed files, diffs, file tree.
- Restrained context menus, themes, keyboard access, scalable layout, accessible statuses.

**Proposed refinement:** retain this frame while switching between History and Changes workspaces. Changes gets separate staged/unstaged lists, diff, and commit editor instead of forcing staging into the history layout. Tabs/details retain independent view state per worktree.

Reference boundary: the user referred to an attached screenshot, but it was not available to inspect. [Fork UI research](research/fork-ui-observations.md) records official Windows screenshots/animations, their age, and observed versus inferred behavior. Exact matching to the attachment remains open. Do not copy proprietary assets or assume historical dialog behavior is current.

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
