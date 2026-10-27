# Fork UI observations

Historical research **2026-10-23**; no app implementation/current Fork hands-on test. [Product](../product.md)/[requirements](../requirements.md) own current behavior. Later user requests supersede these proposals; no prototype or extra trust-layer requirement follows from this research.

## Evidence limits

Official Windows screenshots and multiple GIF frames inspected; release notes/articles read. Old publication evidence differs from current behavior; a separate window does not establish modality. X profile loaded, Media required login, retrieval returned 403; **no X videos reviewed**. Official animations supplied motion evidence.

Original user attachment unavailable; a later 2026-10-25 screenshot informed headerless Shovel history, not broad parity. Exact current menu order/wording, hover timing, remote defaults, reset confirmation, focus/monitor/RTL/Esc behavior and modality remain unverified.

## Layout

Stable menu/compact toolbar/repository tabs/sidebar; common Git left, repository/branch center, tools right. Compact graph/ref/subject rows, lower selected details (Commit/Changes/File Tree tabs). Changes rearranges staged/unstaged trees on inner left, diff right, subject/body/amend/commit below; nearby Stage/Discard for hunks. Compact aligned rows/separators/subdued backgrounds/limited accents, not cards/ornament/large gaps.

Sources: [main Windows](https://git-fork.com/images/image1Win.jpg), [commit view](https://git-fork.com/images/carousel/carousel_commitviewWin1.jpg).

## Interactions

| Trigger | Observed/documented behavior | Evidence/source |
| --- | --- | --- |
| Commit right-click | Branch/tag, interactive rebase, reset, checkout, cherry-pick/revert, patch/copy groups; rebase quick reword/squash/fixup/drop and adjacent destination submenu | [Viewed menu](https://fork.dev/blog/posts/forkwin-1.38/ir-quick-actions.jpg) |
| Interactive rebase | Rows/action selectors, details/diff, branch/base, backup-tag, Rebase/Cancel; dropped rows struck out as well as colored | [Article/trigger](https://fork.dev/blog/posts/forkwin-1.23/), [viewed editor](https://git-fork.com/images/interactiveRebaseWin.jpg) |
| Branch drag | Menu offers merge/rebase/interactive rebase with both names; drag chooses context | [Sampled GIF](https://fork.dev/blog/posts/forkwin-1.48/drag-and-drop-merge.gif), [article](https://fork.dev/blog/posts/forkwin-1.48/) |
| Changed-file selection | Partial stash command; multiselect seen, whole menu-to-completion not observed | [GIF](https://fork.dev/blog/posts/forkwin-1.48/partial-stash.gif), same 1.48 article |
| Edit remote | Name/URL/protocol/Test Connection/Edit/Cancel; inline test result | [Before/result frames](https://fork.dev/blog/posts/forkwin-1.48/test-connection.gif), same article |
| Conflict selection | Inline explanation/alternatives, persistent operation banner | [Viewed overview](https://git-fork.com/images/mergeConflictWin1.jpg) |
| Merge editor | Two inputs/result, previous/next conflict, Resolve/Cancel; modality unknown | [Viewed editor](https://git-fork.com/images/mergeConflictWin2.jpg) |
| Ctrl+P | Commands/branch-file completion; not exercised | [Quick Launch article](https://fork.dev/blog/posts/quick-launch/) |
| Graph collapse | Merge tip/arrow keys, collapse-all context command | [Official blog](https://fork.dev/blog/) |
| Space / tab menu | Commit popup / Explorer-terminal | [Windows release notes at research date](https://fork.dev/releasenoteswin) |

[Windows 1.38](https://fork.dev/blog/posts/forkwin-1.38/) explicitly made Blame non-modal; do not infer every utility window is modal.

## Historical Shovel proposals

1. Separate history/changes workspaces in stable frame.
2. Single selection inspects; right-click targets object, preserves containing multiselection, otherwise selects new row; freeze target while menu open.
3. Object-anchored on-screen keyboard menus; group inspection/normal/destructive actions, name destructive targets.
4. Compact owned branch/remote/pull/push dialogs; async validation, fixed targets, owner-monitor placement/focus restore. Normal execution needs no extra trust layer under current policy.
5. Larger rebase plan/editor; drag never rewrites immediately.
6. Persistent operation/conflict status; explicit continue/abort, never dismissal side effects.
7. Non-modal file history/blame/output where practical; no modal just to inspect commit.
8. Real source refs/context; ours/theirs is not always local/remote, especially rebase.
9. Shared command definitions across toolbar/menu/shortcut/palette: scope, availability, resource and destructive confirmation.
10. Verify focus/dismissal/DPI/monitor/accessibility/long translations/RTL with functional UI.

These are design proposals, not Fork facts or approval for UI-only deliveries. Implement requested controls with their real operation.
