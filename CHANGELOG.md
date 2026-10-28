# Changelog

## 0.17.0 — 2026-10-28

- Recover interrupted merges, rebases, cherry-picks and reverts through Continue, Skip and Abort in the sidebar and Repository menu. Reopen rediscovers state; no opening-time preparation or remote queries. Resolve/stage files through Local Changes, then continue with Git's existing message.
- Confirm actual Abort/Skip targets and consequences; reject unresolved Continue and changed operation state. Reuse shared mutation coordination, top-center progress/cancellation and retained output. Reinspect after failure/cancellation; preserve subsequent conflicts and auto-stash recovery without automatic cleanup or rollback.

## 0.16.1 — 2026-10-28

- Match native title bars to the dark toolbar on the main, commit, branch/network and authentication windows. Preserve native caption controls; restore system defaults in high contrast. Windows 11 supports exact palette colors; Windows 10 uses a best-effort native dark request.

## 0.16.0 — 2026-10-28

- Show old/new binary file cards with extension, readable size, exact bytes and LFS labels in Local Changes and historical Changes. Read LFS pointer sizes locally without downloading assets or invoking filters.
- Return file Stage/Unstage results from path-scoped status, then coalesce a full refresh in the background. Cancel obsolete reads without waiting for them before a write; retain generation checks, index/config/HEAD validation, real Git results and failures. Unstage uses index-only, literal path-limited reset with no worktree refresh.
- Remove bottom activity bars and the commit window's ticking footer. Keep cancellation, static results/errors and diagnostic phase timings; retain useful content during background refresh and selection across staged/unstaged moves.

## 0.15.0 — 2026-10-28

- Add a Changes tab for the selected history commit: real changed files, numbered colored diffs, rename paths and binary size changes. Compare merge commits against a selectable parent; initial commits compare with an empty tree. Reads stay bounded, cancellable and independent of working files.
- Share amber M, green + and red − file markers between history changes and Local Changes, with text/accessibility meanings and system high-contrast colors.
- Remove full-history ref enumeration from working-copy validation and refresh. Run independent reads within the existing two-process bound; retain index/HEAD/config/identity checks and reconcile real Git outcomes. Display read-drain, validation, Git-command and refresh timings in the commit window output section.

## 0.14.0 — 2026-10-28

- Stage or unstage selected complete change blocks from the commit modal's diff. Ctrl/Shift selects multiple blocks; one context-aware button shows the action and count, with accessible block descriptions.
- Apply only the selected hunks to the index, preserving working files and unrelated staged edits. Revalidate the exact preview, file content, index, attributes, configuration and repository state; report failures/cancellation and refresh without a whole-file fallback.
- Keep drafts and selected file after hunk actions. Explain unavailable actions for binary/filter-managed content, additions/deletions/renames and display-only comparison options. Ordinary tracked UTF-8 text modifications support CRLF normalization and missing final newlines; arbitrary line selection remains deferred.

## 0.13.0 — 2026-10-28

- Open Local Changes as a resizable modal commit workspace over retained history. Add a Commit count after the selected tab's history is visible; opening binds available rows before asynchronous inspection, without polling other repositories.
- Show unstaged/untracked/conflicted and staged lists beside a virtualized, numbered, colored unified diff. Include whitespace comparison, whole-file context within limits, hunk navigation and binary before/after byte sizes.
- Implement selected-file stage/unstage and commit/amend. Hold Shift on the same buttons for Stage all, Unstage all and Commit and Push to the configured upstream. Preserve hooks, confirm amend's actual target, retain drafts on failure and report a successful commit separately from a failed push.
- Revalidate identity/HEAD/config/index before writes, serialize related operations, drain cancellation and reinspect results. Keep bounded session drafts/output, stale rows, clear limits and shared dark/high-contrast styles. Hunk staging, image previews and an interactive conflict editor remain deferred.

## 0.12.1 — 2026-10-28

- Apply a restrained dark client-area theme to menus/toolbar, repository tabs, sidebar, history/Local Changes, lower details tabs and existing operation/authentication dialogs. Distinct charcoal surfaces and orange lower-tab accents follow the supplied reference without copying assets.
- Centralize semantic colors and hover/pressed/disabled/selection/focus styles; preserve native control input, history author highlighting and virtualization. Map colors to Windows high contrast and document the project-wide pattern.
- Keep further theme polish low-priority behind file diffs/staging. Native OS title bars/pickers/message boxes remain system-owned; light-mode selection is deferred.

## 0.12.0 — 2026-10-28

- Add read-only Local Changes with real staged, unstaged, untracked and conflicted entries; ignored paths are opt-in. Preserve rename origins and show both index/worktree changes for partially staged files.
- Switch views immediately; scan asynchronously with cancellation, cached per-tab rows/selection, explicit stale/errors/limits, virtualization and no polling or startup preloading. Preserve history while navigating.
- Suppress configured clean/process filters and fsmonitor during status reads without changing Git configuration or index bytes. Submodule status is pointer-only; content diffs and staging remain next.


## 0.11.1 — 2026-10-28

- Pull/Fetch/Push/branch dialogs use cached local targets immediately; removed opening-time Git preparation and loading/error shells. Fresh validation runs after submission with main-window progress and retained errors.
- All operation options are fixed-size with Close only in the title bar; content remains scrollable. Pull keeps default Rebase, optional Merge and auto-stash.
- Cache configured remote names during local inspection, including unfetched remotes. No opening-time network checks; Fetch discovers new remote branches. Document prompt UI behavior as a project requirement.


Historical entries describe their release, not current policy. From 0.9.2, working features/normal operation consent supersede older prototype/trust requirements.

## 0.11.0 — 2026-10-28

- Open operation dialogs before Git preparation; show loading/errors in place, cancel dismissed preparation and reject late results. Reuse inspected refs and overlap bounded URL reads; add repeatable dialog/preparation timings.
- Pull defaults to real rebase with a merge alternative and optional tracked auto-stash. Integrate divergent branches; permit unrelated untracked files and protect ignored collisions. Explicit strategy/stash choices override conflicting Git defaults without changing configuration.
- Allow Pull in existing sparse worktrees and superprojects, updating submodule pointers without touching submodule worktrees. Branch-switch restrictions remain separate.
- Preserve conflicts and Git recovery stashes, including auto-stash conflicts despite zero exit; show recovery instructions and top-center resolution state. Keep hooks, output, cancellation/reinspection and existing operation admission.
- Cover disposable local remotes, divergent file changes, staged/unstaged restoration, conflicts/reopen/abort, dialog preparation and offscreen layouts. Final checks suppress production startup; desktop interaction and live accounts remain unverified.
- Isolate the headless WPF harness from production startup/profile composition by loading shared resources in a test-only Application.

## 0.10.0 — 2026-10-27

- Submitting Pull/Fetch/Push/branch options closes the modal and runs the operation under main-window ownership. Top-center phase, elapsed time, progress and Cancel remain visible through completion/reinspection.
- Retain success/error/cancellation and bounded Git output in an Operation details tab; distinguish refresh failure, prevent duplicate/concurrent actions and keep results scoped to their repository. Authentication prompts use the main window after options close.
- Verify real incoming-commit/file Pull behavior, refreshed history/counts, dirty/unavailable-remote errors, cancellation and offscreen layouts. Prioritize Local Changes/status before broader authentication expansion; Pull remains clean-worktree fast-forward only.

## 0.9.3 — 2026-10-27

- Normal Branch/Fetch/Pull/Push actions execute without a separate configured-program consent checkbox or execution flag. Required authentication, hooks, Git protections and cancellation/reinspection remain.
- Omit unfinished Stash/external-tool/sidebar entry points and rebase/autostash/force options. Keep the functional menus, branch controls, history and details; explain unavailable action targets/states with localized accessible help.
- Exercise all five ordinary action dialogs against disposable repositories/local bare remotes, alongside failure/cancellation/authentication regressions and normal/minimum offscreen layouts. No interactive launch or live provider testing.

## 0.9.2 — 2026-10-27

- Compact all ten maintained Markdown files; consolidate UX, acceptance, mechanisms, evidence and tasks while retaining requirement IDs, release history and research references.
- Require functional features end to end; remove UI-first/placeholder-retention mandates and phase-order blockers for newly requested work.
- Prohibit routine repo-trust/first-use/program-consent layers and extra safety measures without concrete need. Retain normal Git protections, authentication and destructive-action confirmations.
- Track remaining approval/prototype code changes in TODO. Runtime behavior unchanged; versioned local preview only.

## 0.9.1 — 2026-10-26

- Owned Git/OpenSSH askpass for explicit Fetch/Pull/Push: masked replies/confirmation, known waiting state, cancel/reinspection.
- Packaged helper: operation-only current-user pipe/nonce/server checks, bounded frames/requests/deadlines, cleared reply buffers; no history helper.
- Cached GCM/agents/SSH host verification retained; browser/signing/custom SSH/interactive hooks pending, no live-account/desktop certification. Real disposable credential/helper tests and offscreen prompts; no dependency/installer/updater.

## 0.8.0 — 2026-10-25

- Real Branch create/switch and Fetch/FF Pull/normal Push, optional fetch-all/tags/upstream; original local/remote/tag/current/same-tip-upstream badges.
- Added then-required program approval, identity/ref/config revalidation, shared-worktree lock, modal/shutdown admission, bounded output/cancel/reinspection. Automatic history remained prompt-free.
- Preserved hooks/cached auth; interactive/custom SSH and rebase/autostash/force pending. Local bare-remote/failure/cancel/offscreen checks, no desktop/live auth/distribution.

## 0.7.0 — 2026-10-25

- History directly below tabs, compact footer; removed selector, fixed local/remote/tag/current-HEAD scope, ignored older narrower settings.
- Restored menus/toolbar/sidebar/center repo-branch-timer; unfinished controls were disabled prototypes (historical policy).
- Real local-upstream behind/ahead after rows, dash for unavailable; no network/action implementation then. Disposable divergence/cancel/offscreen checks; cache/author/read behavior retained.

## 0.6.0 — 2026-10-25

- Removed local-history consent/management; opened/restored reads automatic, schema-4 old fingerprints become replacement stamps.
- Direct native Git in atomic Job Object; no per-command .NET helper, prefer native Git, combine HEAD/refs/reuse observations. Preserved limits/cancel/ownership/post-validation.
- Tab menu refresh/move/close/close others, MMB close; active cancels/inactive preserves read. Native/process/tab checks, packed 100k benchmark; no interactive/network/distribution work.

## 0.5.0 — 2026-10-25

- Selected approved startup repo/cache restoration; bounded page/scope/selection/scroll, async refresh retaining rows.
- Then-current remembered/revocable identity approval/migration/replacement checks (superseded 0.6.0).
- Combined queries/parallel HEAD pairs/reused refs/source-generated protocol; measured timing/diagnostics.
- Secondary actions in Repository; removed prototypes then; yellow exact-author peers/high contrast/accessibility. Read-only, no desktop/network/mutations/updater.

## 0.4.2 — 2026-10-25

- Selected front-facing silver shovel/blue circle as EXE/window icon; source PNG and multi-size transparent ICO.

## 0.4.1 — 2026-10-25

- Headerless history per user screenshot, aligned columns/localized help/row descriptions; no header drag, description adapts. Scope/paging/status retained, Git unchanged.

## 0.4.0 — 2026-10-24

- Default local/remote/tag All Commits plus then-separate current-HEAD scope/labels.
- Replaced 2,000 cap with 200-row older/newer/range/limits, cross-page graph cursors; minimum layout reserves rows, resizable details.
- Complete parsed details; failure/cancel retains page, ref/boundary changes require refresh. Real multi-branch/merge/nested-tag/packed/shallow/detached/unborn/2,005 fixtures headlessly; no desktop/mutations/network.

## 0.3.1 — 2026-10-24

- Shovel localized title/EXE/product; compatible GitGui profile/mutex/internal names.
- Requested old Build cleanup/preview rebuild/docs; Git scope unchanged.

## 0.3.0 — 2026-10-23

- Real virtualized HEAD graph/refs-free description-author-date-hash table, merge second ring; full selected message/identities/dates/hash/parents.
- 200 initial rows, bounded older expansion to then-2,000 cap/pinned tip, explicit shallow/empty/errors/manual refresh.
- Real linear/merge/shallow/signature-suppression/stale/binding/virtualization checks; no desktop. All-ref/filter/status/files/diffs/mutations pending then.

## 0.2.0 — 2026-10-23

- Then-trusted local metadata: branch/HEAD/worktree/common identity/markers/manual refresh.
- Persistent tab order/selection (no restored trust), reorder/close; schema 1 → 2/backup.
- Async streams/caps/deadlines/cancel/owned jobs through waiting helper; isolated fixtures/ignored versioned preview. History/status/diffs/mutations/auth/distribution pending then.

## 0.1.0 — 2026-10-23

- C#/WPF/.NET 10 MVVM shell, English resources, settings/diagnostics/lifetime.
- 17 checks and actual Windows 11 GUI smoke.
