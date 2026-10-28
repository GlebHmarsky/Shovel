# Changelog

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
