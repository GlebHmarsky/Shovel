# Fork UI observations

Historical research reference. Current scope and UX proposals are owned by [product](../product.md); obligations by [requirements](../requirements.md). Observations below are not additional approved requirements.

Research date: 2026-10-23. Research only; no application implementation.

## Evidence boundaries

Inspected official Windows screenshots in a browser and sampled multiple frames of official animated GIF demonstrations. Read Windows release notes and feature articles. This was not a hands-on test of the current installed Fork application. Many screenshots demonstrate old versions; publication date and current release-note confirmation are different kinds of evidence. A separate window in a screenshot does not prove that it blocks its owner (modal).

The X profile loaded in the browser, but opening Media presented a login prompt. The web retrieval tool returned HTTP 403. No X videos were reviewed; do not later describe this research as watching them. Official website animations supplied the observed motion examples.

## Visually observed layout

- Persistent application frame: menu bar, compact toolbar, repository tabs, left navigation. Toolbar places common Git operations on the left, repository/branch status centrally, and external destinations on the right.
- History: compact graph/table with reference labels alongside subjects; selected commit details occupy the lower area. Main window screenshot includes Commit, Changes, and File Tree tabs.
- Changes: main area changes arrangement. Separate unstaged/staged file trees occupy an inner left column, diff occupies the right, and commit subject/body, amend toggle, and commit action are below the diff. Selected hunks expose nearby Stage and Discard actions.
- Restraint comes from compact rows, separators, aligned columns, subdued backgrounds, and limited accent colors. It does not require excessive empty space, large cards, or ornamental animation.

Source screenshots:
- https://git-fork.com/images/image1Win.jpg
- https://git-fork.com/images/carousel/carousel_commitviewWin1.jpg

## Interaction evidence

| Trigger | Observed or documented response | Evidence type |
| --- | --- | --- |
| Right-click commit | Menu offers branch/tag creation, interactive rebase, reset, checkout, cherry-pick, revert, patch export, and copying. Separators organize related actions. | Viewed Windows screenshot |
| Interactive Rebase submenu | Full rebase entry plus quick reword, squash, fixup, drop actions; destination submenu opens beside the parent menu. | Viewed screenshot |
| Start interactive rebase | Dedicated editor with commit rows and action selectors, selection details/diff, named branch/base, backup-tag option, Rebase/Cancel. Dropped rows use strike-through as well as color. | Viewed screenshots; opening trigger documented |
| Drag branch onto another | A compact menu offers merge, rebase, or interactive rebase with both branch names. Dragging selects an operation context rather than immediately performing it. | Observed different GIF frames plus article |
| Right-click selected changed files | Partial stash available for the selection. Multi-file selection is shown in the animation. | Article confirms command; sampled GIF frames confirm selection, not entire menu-to-completion sequence |
| Edit remote | Compact window with remote name, URL, protocol choice, Test Connection, Edit/Cancel. Connection result appears inline in the same window. | Observed before/result GIF frames |
| Select conflicted file | Inline conflict explanation and alternatives in Changes view; operation banner persists above the working area. | Viewed screenshot |
| Open merge editor | Two inputs above a result area; previous/next conflict controls and Resolve/Cancel. | Viewed screenshot; modality not established |
| Ctrl+P Quick Launch | Command selection with branch/file completion. | Official article; not visually exercised |
| Collapse graph merges | Click merge tips or use arrow keys; context menu can collapse all. | Official article |
| Space on commit | Commit-detail popup. | Current Windows release notes |
| Tab context menu | Explorer and terminal actions. | Current Windows release notes |

Specific sources:
- Commit menu: https://fork.dev/blog/posts/forkwin-1.38/ir-quick-actions.jpg
- Rebase: https://fork.dev/blog/posts/forkwin-1.23/ and https://git-fork.com/images/interactiveRebaseWin.jpg
- Branch drag: https://fork.dev/blog/posts/forkwin-1.48/drag-and-drop-merge.gif
- Partial stash: https://fork.dev/blog/posts/forkwin-1.48/partial-stash.gif
- Remote dialog: https://fork.dev/blog/posts/forkwin-1.48/test-connection.gif
- Article explaining the three animations: https://fork.dev/blog/posts/forkwin-1.48/
- Conflict overview: https://git-fork.com/images/mergeConflictWin1.jpg
- Merge editor: https://git-fork.com/images/mergeConflictWin2.jpg
- Quick Launch: https://fork.dev/blog/posts/quick-launch/
- Graph: https://fork.dev/blog/
- Current Windows changes: https://fork.dev/releasenoteswin

## Modality: what is established

The Windows 1.38 article explicitly says the Blame window became non-modal. Do not treat every detached utility window as a modal dialog. Static images do not establish focus trapping, owner-window disabling, Esc behavior, precise placement on multiple monitors, or the latest confirmation sequence.

Sources: https://fork.dev/blog/posts/forkwin-1.38/ and https://fork.dev/releasenoteswin

## Proposed behavior for our application (design decisions, not Fork facts)

1. Keep history and changes as distinct workspaces inside one stable frame.
2. Single selection inspects; right-click addresses the clicked object. Preserve multi-selection when right-clicking inside it; select a newly clicked row otherwise. Freeze the target identity while a menu is open.
3. Anchor menus to the object/pointer, keep within the active monitor, and expose keyboard equivalents. Group inspection/copy, normal operations, and destructive operations. Destructive commands must name the branch, remote, or affected selection.
4. Use compact owned dialogs for branch creation, remote configuration, pull/push options, and destructive confirmation. Position within the owner monitor and restore focus on dismissal. Validate asynchronously without changing the target silently.
5. Use a larger operation editor for interactive rebase. Preview a plan before executing; do not rewrite history on an accidental drag in the main graph.
6. Keep ongoing operation/conflict status visible inline. Continuing or aborting is an explicit command, not a side effect of dismissing a window.
7. Use non-modal views for file history, blame, and output where practical. Never require a modal window simply to inspect a commit.
8. For conflicts, show real source refs/commits and operation context. Do not always equate ours/theirs with local/remote; rebase changes the interpretation.
9. Share one command definition across toolbar, menus, shortcuts, and palette, including availability, scope, localization key, and confirmation policy.
10. Make popup focus, keyboard dismissal, DPI/monitor placement, screen-reader labels, long translations, and RTL explicit UI acceptance tests.

## Still unverified

Current-version exact menu order and wording; timing of hover controls; fetch/pull/push option defaults; reset confirmation details; all dialog modality and placement rules; current keyboard focus behavior; X video content. Do not invent these details. Our requirements and safety rules take precedence over historical Fork behavior.
