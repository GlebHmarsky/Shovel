# Implementation checklist

Updated 2026-11-01. Single prioritized task list; [roadmap](docs/roadmap.md) holds completed milestones/evidence. P0 = correctness prerequisite; P1 = next delivery; P2 = later. Later user requests can change order without reapproving a whole historical phase.

## Current phase

- [x] **P2 — Refactor selected LFS discard verification (0.36.1).** Share exact-byte hashing, remove redundant raw status scans for supported selections, retain native fallback and concurrent edits. Same-host median: one asset 1.215 → 1.104 s; 64 assets 3.691 → 2.009 s. [Evidence and limits](docs/roadmap.md#lfs-discard-refactor-0361-beta).
- [x] **P1 — Deliver 0.36.1 Beta.** 11 LFS and 6 discard checks; 26 smoke checks in 48.6 s, matching preview/installer, signed update feed and anonymous full-package verification. Stable latest remains 0.35.0. [Evidence and limits](docs/roadmap.md#lfs-discard-refactor-0361-beta).

- [x] **P1 — Deliver 0.36.0 Beta.** Inline external merge-tool chooser; 10 focused conflict checks and 26 smoke checks (45.7 s), signed Beta publication and anonymous package verification. Stable remains 0.35.0. [Evidence and limits](docs/roadmap.md#external-merge-tool-chooser-0360-beta).

- [x] **P1 — Stable/Beta channels (0.35.0).** Persisted selector/default Stable, isolated signed feeds and package validation, pending-update invalidation, no cross-channel downgrade; routine Build-Release defaults to Beta. Explicitly publish the common baseline to both; future Stable releases require owner approval.
- [x] **P1 — Deliver common 0.35.0 baseline.** 17 updater checks; 26 smoke checks in 33.1 s; same source/version published as Stable release and Beta prerelease. Both signed anonymous downloads verified; Beta left Stable latest unchanged. [Evidence](docs/roadmap.md#stable-and-beta-channels-0350). Installed channel-switch/restart acceptance remains with the owner.

- [x] **P1 — Automatic launch updates (0.34.0).** Immediately check/download/verify on each installed launch and automatically request the existing guarded restart. No delay, periodic-preference exemption, verification bypass or repeated restart loop. Blocked/cancelled/failed attempts remain usable. Installed restart acceptance remains in the rollout task.
- [x] **P1 — Deliver 0.34.0.** 16 updater checks and 26 smoke checks (32.9 s); Build-Release published the signed feed and installer on Shovel and verified an anonymous full-package download. [Evidence](docs/roadmap.md#automatic-launch-updates-0340).

- [x] **P1 — Clone and remote URL management.** Preparation-free Clone/Manage remotes forms; native checkout with filters/hooks, authentication/cancellation, partial-result retention and automatic history opening; cached fetch/push URL editing and confirmed local remote removal with identity/configuration checks and reconciliation. [Flow and limits](docs/product.md#clone-and-remote-management-0330).
- [x] **P1 — Deliver 0.33.0.** 11 clone/remote and 2 shared network checks passed; Build-Release passed 26 smoke checks in 31.8 s, packaged the exact committed source, published the signed update feed on Shovel and verified an anonymous full-package download. [Release evidence and limits](docs/roadmap.md#clone-and-remote-management-0330).

- [x] **P1 — Consolidate source and updates in public Shovel.** 0.32.1 is published and signed on the main repository; clean-master delivery tags the exact source commit. The old host is archived with one final migration release. Both anonymous download routes were verified; all future publishing is restricted to Shovel.


- [x] **P1 — Merge installer/updater into master and make releases the delivery default.** 0.32.0 retains current UI/Git features and settings compatibility; Build-Release packages, signs, publishes and verifies each delivery. Source and release hosting now share public GlebHmarsky/Shovel; old hosting is only a migration endpoint. See distribution for the command and roadmap for verification evidence.


- [x] **P1 — Daily-use QOL.** Retain file/commit comparisons while loading; preserve unchanged selected rows; navigate history/stashes to Commit changes; embed whole-file conflict selection/editing/staging in Commit; persist pane/window sizes; quiet tabs/arrows/badges and add bounded All/User/Background activity. [Behavior and limits](docs/product.md#daily-use-interactions-0310).
- [x] **P1 — Deliver 0.31.0.** Six QOL, eight conflict, eleven stash and ten commit-file checks passed; 26 smoke checks in 53.0 seconds, zero build warnings/errors, matching ignored preview/checksums and owning docs. Native input coverage, crash-time geometry and advanced merging remain limitations.

- [x] **P1 — Diagnose and fix EOM's slow LFS discard.** Retained Shovel output and Fork Activity Manager identify the command/hook difference; user confirmed Fork's unstaged-discard semantics. Native conversions/index hooks remain; hook files/branch/staged-discard behavior unchanged. Same EOM assets: old restore pipeline 33.4 s, fixed single/two-file selections 1.3–2.1 s, exact hashes and preserved stash. [Evidence and limits](docs/roadmap.md#lfs-discard-0301).
- [x] **P1 — Deliver 0.30.1.** Focused discard/LFS checks; 26 routine checks in 52.8 seconds, zero build warnings/errors, matching ignored preview/checksums and owning docs. New preview launched and restored EOM; desktop discard submission remains an automation limitation. Large-batch/cold-cache limits remain below.
- [x] **P1 — Conflict resolution.** Actual source labels, per-file side checkout/staging and UTF-8 result editing; merge/rebase banner with Resolve/Continue/Abort. Binary/LFS/deletion/rebase/stash and stale/failure paths covered by focused checks. Configured external tool works; tool chooser and additional encodings remain limits.
- [x] **P1 — Startup and daily-use fixes.** Bounded saved last-selected history preview; real stash history Apply/Pop/Drop; themed disabled tab strip and persisted drag reorder. Headless cache publication and offscreen theme checks passed; native desktop input remains unverified.
- [x] **P1 — Profile discard and correct LFS status.** Read-only authorized asset-repository inspection; disposable text/LFS single/batch measurements; overlapping validation and consolidated HEAD reads; restore filters/hooks preserved. Fixed false modified rows after hydrated LFS restores. See [results](docs/roadmap.md#conflicts-and-usability-0300).
- [x] **P1 — Deliver 0.30.0.** Focused conflict/usability/LFS/status/responsiveness checks and offscreen review; 26 routine checks in 55.6 seconds, zero build warnings/errors, matching ignored preview/checksums, owning docs and descriptive source commit. No interactive launch; LFS latency remains open below.

- [x] **P1 — History context actions.** Clicked full SHA; detached Checkout; confirmed Soft/Mixed/Hard Reset; Copy SHA and Revert with explicit merge mainline. Preparation-free menu, shared execution/results/recovery and stale-target rejection.
- [x] **P1 — Direct Fetch/Push with options arrows.** Use supported configured single-branch targets; preserve split Pull and modal Commit. No menu-time preparation.
- [x] **P1 — Deliver 0.29.0.** Nine focused scenarios passed across history checks and the corrected network rerun; Dark/Light normal/minimum offscreen review; 26 routine checks in 62.4 seconds, matching ignored preview/package hashes, owning docs and descriptive commit. No interactive launch or live server test.

0.30.1 addresses the identified EOM discard bottleneck; 0.30.0 adds conflict resolution and startup/tab/stash usability. Routine verification remains 26 checks (maximum 30). Large-batch and cold-cache costs remain below.

- [x] **P1 — Direct default Pull.** Main button uses Rebase/auto-stash off and configured upstream; arrow keeps the existing editable modal. Shared real execution/results/reconciliation; no options-time preparation or remembered-choice change.
- [x] **P1 — Deliver 0.28.0.** Four focused checks/offscreen Dark/Light normal/minimum review; 26 routine checks in 51.6 seconds; matching ignored preview/package hashes, updated docs and descriptive commit. No interactive launch or live server test.

- [x] **P1 — Record daily-use priorities.** Prioritize fewer-click Pull, history context actions and in-app conflict resolution; keep LFS locking and coworker distribution later. Do not mark these as implemented.

- [x] **P1 — Diagnose draft storage and preserve both writers.** Twelve repeats of the original reopen flow passed; a controlled publication lock reproduced `CommitDraftUnavailable` with OS error 32. Add bounded cancellable retries for sharing/lock errors, rechecking identity/revision, and allowlisted diagnostic metadata. Four focused checks pass; the old uninstrumented failure's exact cause remains unknown.
- [x] **P1 — Deliver 0.27.1.** Four focused checks; 26 routine checks in 54.0 seconds; zero build warnings/errors, matching ignored preview/package hashes, updated owning docs and descriptive commit. No interactive launch; original failure attribution and physical crash/ACL/disk-full validation remain limits.

- [x] **P1 — Clean workspace and expose remote setup.** Sidebar has no horizontal scroll or fixed width cap; duplicate title/Local Changes and main footer removed. Page range/navigation move into View with keyboard/edge scrolling. Larger caption icons and shorter tooltips; read/refresh fade feedback; one stash base connection preserves real commits. Add remote locally, then explicit Fetch and Set upstream.
- [x] **P1 — Deliver 0.27.0.** Four focused checks and Dark/Light normal/minimum offscreen review; 26 routine checks in 47.4 seconds; matching ignored preview/package hashes, owning docs and descriptive commit. No interactive launch or live network; see roadmap limits.

- [x] **P1 — Compact caption and stable history.** Menus in draggable title strip; Fetch/Pull/Push/Commit order; one row per stash, fixed lanes and broad graph turns. Original metadata and independent history retained; visible pagination/ref jumps agree.
- [x] **P1 — Deliver 0.26.0.** Four focused topology/stash/page/caption checks, offscreen normal/minimum Dark/Light review; 26 routine checks in 41.8 seconds, matching ignored preview, docs and descriptive commit. Native caption behavior and large-repository stash projection timings remain validation limits.

- [x] **P1 — Stable operation feedback.** Fixed duration capsule for existing timed commands; accurate completion outcome, brief hold/fade, no horizontal text jump. Read feedback added in 0.27.0. Retain results/errors; respect animation preference/high contrast.
- [x] **P1 — Deliver visual pass (0.25.0).** Consistent small radii/dividers in both themes, four focused checks and normal/minimum offscreen review; 26 routine checks in 32.7 seconds, matching ignored preview, owning docs and descriptive local commit. Live animation/desktop accessibility remains unverified; see roadmap.

- [x] **P1 — Workspace presentation and toolbar.** Background-only author peers, straight/rounded graph, evenly filled tabs, Open in Explorer/Rider/VS Code, Bash and persisted Dark/Light appearance. Four focused checks and normal/minimum offscreen review; no live tool launches.
- [x] **P1 — Deliver 0.24.0.** 26 routine checks in 32.2 seconds, matching ignored preview/package hashes, owning docs and local commit.

- [x] **P1 — Make repository sidebar functional.** Branch switch/tracking/context actions; fetched remote refs; tag/stash history navigation; inline Apply/Pop/confirmed Drop; submodule open/update/sync. Async cached lists after history, retained errors, real disposable workflows and normal/minimum offscreen review.
- [x] **P1 — Deliver 0.23.0.** 26 routine checks in 31.5 seconds; four focused sidebar checks, offscreen input/layout review, matching ignored preview/package verification and owning documentation.

- [x] **P1 — Reduce routine verification.** Explicit small default, count/runtime guards, first-failure stop, per-check timings, optional full suite, updated guidance and verified 0.22.1 preview. No automatic suite growth or repeated unchanged checks. [Policy](AGENTS.md), [commands](README.md#restore-build-and-check).

- [x] **P1 — Initialize local repositories.** Fast options, new/existing nonrepo folders, optional initial branch, preserved files, automatic opening and real progress/errors/cancellation. Nine focused checks pass.
- [x] **P1 — Deliver 0.21.0.** 216 full checks and nine focused checks passed; offscreen review, owning docs, matching ignored preview and local commit. No interactive launch. [Evidence](docs/roadmap.md#current-status).


## Parallel assignments — Task 1, Task 2, Task 3

Confirmed 2026-10-29: these are independent implementation branches, not three releases. Start each from the same planning commit containing this section. **Use a separate worktree for each chat**; switching branches in one shared directory is not isolation. Never reset another checkout or copy uncommitted work automatically. Do not start other tasks or spawn agents from an assigned chat.

Worker rules: no dependency installation, restore, compilation, test execution, app launch, packaging, version/changelog edits, push or merge. Write meaningful tests and perform static review only; clearly report them as unexecuted. Preserve all existing protections and preparation-free options. Commit assigned changes locally. Do not edit this checklist or shared product/architecture/roadmap docs on worker branches; write a concise implementation/verification handoff in the assigned `docs/handoffs/task-N.md` instead (not another checklist). The integrator will update owning docs and TODO once.

### Task 1 — Local branch deletion and upstream tracking

- [x] **P1; branch `codex/task-1-branch-management`.** Deliver real branch management from the existing Branch menu: delete a selected local branch, set its upstream to an existing remote-tracking branch, or unset its upstream. No remote deletion, force deletion, fetch or implicit checkout.
- Open options immediately from cached refs. Submission revalidates identity/refs/config and serializes through the existing common-repository mutation gate. Confirm deletion with exact branch/tip; use Git's normal merged-branch protection. Refuse current branches and branches checked out in any linked worktree. Handle stale targets, missing refs, active sequences, locks and cancellation accurately; refresh history/ref labels/ahead-behind after success or failure.
- Own new `Core/Git/BranchManagement.cs`, `App/ViewModels/ShellBranchManagement.cs`, dedicated branch-management view/model files, and necessary narrow wiring in `ShellActions.cs`, `ShellViewModel.cs`, `MainWindow.xaml`, `UserInteraction.cs`. Reuse existing Core gate helpers; any necessary edit to `GitActions.cs` must be small and preserve other commands. Do not edit commit/draft/history-performance modules.
- Add disposable-fixture checks in `tests/GitGui.FoundationChecks/BranchManagementChecks.cs`: merged/unmerged/current/linked-worktree deletion, tracking set/unset, changed target and failed/cancelled execution. Handoff: `docs/handoffs/task-1.md` with exact test registration instructions and known limits. Acceptance is source-complete workflow plus written checks; it is **not runtime-verified** until integration.

### Task 2 — Crash-safe per-repository commit drafts

- [x] **P2, advanced for independent parallel work; branch `codex/task-2-commit-drafts`.** Persist commit subject/body locally so closing a tab or restarting Shovel does not lose the draft. Restore after the commit modal is visible; never delay opening/history. Successful commit clears only the submitted draft; failed/cancelled commit retains it. Do not persist passwords, output, diffs or authentication data.
- Use a small dedicated Core draft store and App coordination module. Debounce/bound writes, use atomic replacement, handle corrupt/newer data, permissions/disk failure, shutdown and late completion. Key drafts by repository/worktree identity so linked worktrees stay separate and replaced repositories cannot silently inherit drafts. Avoid lost updates across profiles; expose a localized recoverable save/load problem without blocking ordinary Git use. Preserve newer text when a delayed save/delete completes.
- Own new draft storage/coordinator files, narrow wiring in `CommitViewModel.cs`, `ShellChanges.cs`, `LocalChangesState.cs` if present, and dedicated draft tests. Do not change shared settings schema/profile identity, branch actions, main-window menus or history algorithms. Inspect the actual state type before editing; do not invent a parallel draft state.
- Add isolated storage/lifetime tests in `tests/GitGui.FoundationChecks/CommitDraftChecks.cs` covering reopen, successful/failed commit, corrupt/locked storage, identity replacement, linked worktrees and stale writes. Handoff: `docs/handoffs/task-2.md` with storage/retention design, test registration and unexecuted checks. Draft files contain user text: document their local location and deletion behavior; never log their content.

### Task 3 — Representative history/performance harness

- [x] **P1; branch `codex/task-3-history-benchmarks`.** Extend the existing disposable performance harness with reproducible merge-heavy and large-history fixtures, including an opt-in million-commit case. Generate locally with isolated config, no network or user's repositories. Keep expensive cases opt-in, outside the default foundation suite.
- Measure first history page, warm tab switch, refresh, later pages and cancellation under load separately. Include app/owned-child memory sampling and an opt-in five-minute idle CPU sample, with clear measurement boundaries and machine/fixture metadata. Preserve the existing 200-row page size and All Commits scope; validate topology/count expectations. Never label headless measurements as visible desktop startup or input latency.
- Own new files under `tests/GitGui.FoundationChecks/` prefixed `HistoryPerformance`, plus an optional dedicated `scripts/Measure-History.ps1`. Do not alter production code, existing benchmark methods, shared test dispatch or release scripts. Provide exact integration commands/dispatch registration in `docs/handoffs/task-3.md`. Add bounded cancellation, cleanup of only owned temporary fixtures, and useful failure output; do not generate/run the fixtures in this assignment.
- Acceptance: runnable harness source and documented reproducible commands/metrics, statically reviewed; **no fabricated measurements**. Any optimization suggested by the harness remains a later evidence-based change.

### Shared files and integration

Task 1 and Task 2 may append localization entries to `Strings.resx` using disjoint `BranchManage*` and `CommitDraft*` prefixes; do not reformat or reorder existing entries. This is an explicit small merge point. All other shared-file ownership follows the boundaries above. If an unforeseen overlap is required, document it in the handoff and stop that portion rather than editing another task's modules.

- [x] **P1 — Integrate the three branches after review.** User-authorized merges preserve all worker branches; additive resources reconciled, tests/opt-in dispatch registered, early-open draft persistence corrected, docs/version/changelog updated. 263 full checks and package verification passed; original worker handoffs remain historical. [Evidence and limits](docs/roadmap.md#current-status).

## Next priorities

- [ ] **P2 — Remaining LFS performance acceptance.** 0.36.1 reduces the 64 × 4 MiB fixture median to 2.01 s while verifying exact bytes; native checkout itself is about 1.01 s. Verify cold remote hydration, larger/mixed selections and precise EOM desktop timings when authorized. Missing local objects without a remote, partial staging, concurrent bytes/index changes and cancellation have focused coverage. Preserve honest completion; do not bypass native LFS or hooks to reach a timing target. [Evidence](docs/roadmap.md#lfs-discard-refactor-0361-beta).
- [x] **P2 — External merge-tool chooser (0.36.0 Beta).** Select cached configured/common Git tool names in the inline conflict pane; explicit native launch, actual staged-result checking, unchanged Git configuration and stale/failure handling.
- [ ] **P2 — Remaining conflict editor extensions.** More encodings, block-level merge controls and executable-path/custom-command configuration UI; named tool selection and configured-tool/UTF-8/whole-side workflows are delivered.

- [ ] **P1 — Finish public installer rollout (0.30.0 branch).** Packaging, signed checks/downloads and explicit restart admission are implemented. GlebHmarsky/Shovel is now the public source/release host; shovel-releases is a legacy migration endpoint with signed 0.30.103 and 0.30.104 releases, and anonymous production feed/package verification passed. Back up the permanent key, choose Windows publisher signing, and verify two installed versions on a disposable Windows user/VM. Prove native install/update/repair/uninstall, cross-session admission, retained settings/drafts and framework maintenance-log contents before coworker rollout. [Setup and limits](docs/distribution.md). Online 0.30.103 → 0.30.104 exposed a schema-6 profile compatibility gap. Repair 0.30.105 supports the saved format and preserves layout; 0.30.106 is its online update target. Fourteen updater regressions and both 26-check smoke runs pass; installed Help-menu/restart acceptance remains pending. Isolated offline ShovelLocalTest 0.30.101 → 0.30.102 native installation/update/uninstall and scoped cleanup passed; interactive installed acceptance remains pending; anonymous production transport verification has passed. See docs/local-installer-test.md.


- [ ] **P2 — Remaining signing/input recovery.** Broaden only for a concrete daily-use failure; not a prerequisite that displaces requested functionality. Existing hooks/signing stay enabled.
- [ ] **P1 — Representative latency validation.** With authorization, measure the user's asset repository and desktop; 0.16 targets scan/read dependencies with disposable fixtures, not an absolute timing guarantee for all disks/filters/repos.
- [ ] **P1 — Extend authentication when needed for network rollout.** Tracked GCM/browser, signing and interactive-hook interaction; preserve shared agents/browsers and hooks/host verification. Askpass exists. Prioritize working Git features; auth expansion is not a blanket prerequisite. Live provider/desktop tests require separate authorization.
- [ ] **P1 — Measure representative startup.** With launch authorization: cold/warm visible rows, keyboard/tabs, Windows 10/11, custom Git, profile restart/replacement. No interactive launch currently authorized. This validation is not a blanket blocker for independent features.
- [ ] **P1 — Run representative performance fixtures.** Use the integrated opt-in merge-heavy/large/million harness and five-minute idle sampling; retain F1e baseline and distinguish harness/child metrics from desktop behavior. Small fixture correctness checks do not establish large-scale latency budgets.
- [ ] **P1 — Validate accessibility/layout.** Mixed DPI, OS high contrast/screen reader, keyboard/focus, +40%/RTL, themes. Offscreen colors do not certify OS accessibility.
- [ ] **P2 — External reconciliation/tools.** Focus/resume/external-client races and tracked tool sessions.
- [ ] **P2 — File-menu extensions.** Optional all-parent file history/extra diff-tool chooser, repository/global ignore editing and LFS migration; current implemented scopes remain visible.
- [ ] **P2 — Filtering when requested.** Ref/search ancestry, default All Commits, retained selection/scroll.

## Backlog

- [ ] **P2 — LFS locks (low priority).** Discuss a separately authorized server-capability/authentication test, then consider owner display and Lock/Unlock. Server support is unknown; no live test or fetch yet.
- [ ] **P2 — Theme polish (low priority).** Extend the [shared pattern](docs/theme.md): popup/focus/hover/mixed-DPI/RTL and OS high-contrast desktop review. Preserve fast views and native control behavior; do not delay Git functionality.
- [ ] **P2 — Diff/editor extensions.** Historical file-tree browsing, individual-line staging, broader hunk file types, text side-by-side/image previews, chosen encodings, ignored-file inspection and further draft recovery UX; preserve bounded reads and clear unsupported states.

- [ ] **P1 — Broader network authentication/input.** Credential Manager/browser sign-in, custom SSH commands and interactive signing/hooks remain. Clone and local remote add/edit/remove, upstream management, Pull rebase/merge/auto-stash and sequence recovery are delivered. Add explicit longer/shallow/bare/recursive clone options only with supported UI and failure coverage; current bounds are documented.
- [ ] **P2 — Installed distribution acceptance.** 0.30.0 selects Velopack/public GitHub Releases; finish the explicit rollout task above and pass U1–U7/D1 on installed Windows, including key-rotation and uninstall-data-choice requirements.
- [ ] **P2 — Advanced workflows.** Merge/rebase/interactive rebase/cherry-pick/tag management/reflog/force-with-lease and advanced stash variants; disposable destructive fixtures. Reset/Revert and per-file conflict editing are delivered; extensions remain above.
- [ ] **P2 — Extended compatibility.** Additional submodule management (add/remove/deinit/recursive/remote-latest), worktree/LFS/shallow/partial/sparse management, tray and large-repo optimization. Selected submodule open/update/sync is delivered.

## Decisions before rollout

- [ ] **Q2:** coworker Windows editions/builds, architectures, IT/proxy/no-admin constraints. Local SDK prerequisites are present.
- [ ] **Q3/Q4:** release publication/access and certificate/key custody; GlebHmarsky repository URLs are configured. Public GitHub Releases, Velopack and MIT are confirmed; no private-host/VPN requirement.
- [ ] Refresh pinned .NET 10 servicing patch before distribution; reverify.
- [ ] Finalize Windows publisher signing before distribution. Per-user Shovel install identity retains existing Foundation settings and CommitDrafts locations. Shovel name is confirmed.

## Completed

- [x] **0.30.0 / 2026-10-30 (installer branch):** Windows self-contained packaging, signed public-GitHub update checks/downloads, restart admission and publishing scripts; MIT source license. 12 focused checks, 26 final smoke checks, local installer and disposable signing-fixture packaging verified. GitHub owner and installed rollout remain in the explicit task above. [Evidence](docs/roadmap.md#current-status).

- [x] **0.24.0 / 2026-10-29:** background-only author peers, rounded graph lines, equal tabs, Explorer/Rider/VS Code/Bash toolbar and persisted Dark/Light appearance. Four optional focused checks, normal/minimum offscreen review; 26 routine checks in 32.2 seconds and verified preview. Actual desktop handoffs remain unverified. [Evidence](docs/roadmap.md#current-status).

- [x] **0.23.0 / 2026-10-29:** branch/remotes/tags/stash/submodule sidebar with real navigation and actions, saved stash history, selected submodule open/update/sync. Default remains 26 checks; four optional feature checks and normal/minimum offscreen review. Matching preview hashes verified. [Evidence and limitations](docs/roadmap.md#current-status).

- [x] **0.22.1 / 2026-10-29:** default/preview reduced to 26 explicit smoke checks; 30-count cap, 30 seconds/check, 180 seconds total. All 26 passed in 33.3 seconds; three guard probes and package hashes verified. Extended suite retained but not rerun for this delivery. [Evidence](docs/roadmap.md#current-status).

- [x] **0.22.0 / 2026-10-29:** integrated branch deletion/upstream management, persistent per-worktree drafts and opt-in performance harness. 263 full checks; focused branches 13/13, drafts 28/28, harness 6/6; offscreen review and verified preview. Original branches retained. One earlier intermittent draft write remains tracked above. [Evidence and limits](docs/roadmap.md#current-status).

- [x] **0.21.0 / 2026-10-29:** local repository initialization with fast options, optional initial branch, preserved files/partial results and automatic opening. 216 full checks; focused 9/9; normal/minimum offscreen review and package hashes verified. Next: branch deletion/tracking. [Evidence and limits](docs/roadmap.md#current-status).

- [x] **0.20.0 / 2026-10-29:** confirmed selected Discard/Stash, configured External Diff, first-parent file History/Blame, Ignore/LFS rules and binary patch export. 207 checks; warmed menu/history layout 10/23 ms with zero opening-time queries, selected Discard 667 ms on a disposable fixture. Normal/minimum/menu review and package hashes verified. [Evidence and limits](docs/roadmap.md#current-status).

- [x] **0.19.0 / 2026-10-28:** Local Changes right-click menus, real selected/all index actions, asynchronous Windows file handoff and path copying. 192 checks; zero opening-time Git calls, 11 ms warmed offscreen menu layout, normal/minimum review and verified package. [Evidence and limits](docs/roadmap.md#current-status).

- [x] **0.18.0 / 2026-10-28:** stash create/inspect/apply/pop/drop, cached opening and real saved-object diffs; exact-target deletion, retained conflicts/partial results and cancellation. 187 checks; zero opening-time Git calls, normal/minimum offscreen review and verified package. [Evidence and timing limits](docs/roadmap.md#current-status).

- [x] **0.17.0 / 2026-10-28:** real Continue/Skip/Abort recovery, cached controls and reviewed-target confirmation, shared progress/cancellation and preserved conflict state. 176 checks; zero opening-time Git calls, 44 ms warmed offscreen layout; normal/minimum review and verified package. [Evidence](docs/roadmap.md#current-status).

- [x] **0.16.1 / 2026-10-28:** native dark title bars on app-owned windows, system high-contrast defaults and Windows 10 fallback. 157 checks passed on fresh rerun; hidden HWND API acceptance verified, visible paint unverified. Preview checksums/build/tested assemblies match. [Evidence](docs/roadmap.md#current-status).

- [x] **0.16.0 / 2026-10-28:** scoped file-action results/quiet background scanning, Old/New binary extension/size cards and local LFS metadata. Fixture median affected rows: Stage 1.88→0.50 s, Unstage 1.71→0.51 s; user's exact 11.5-second case unverified. 156 checks, normal/minimum offscreen review, 16 preview checksums/nine build files/four tested assemblies matched. [Evidence](docs/roadmap.md#fast-file-actions-and-binary-cards-2026-10-28).

- [x] **0.15.0 / 2026-10-28:** historical files/diffs with selectable merge parents, binary sizes and shared M/+/− markers; narrowed working-copy metadata and exposed phase timings. Fixture median fresh rows: Stage 2.21→1.11 s, Unstage 2.67→1.26 s; user's 10-second case not reproduced. 150 checks, normal/minimum offscreen review, 16 preview checksums/nine build files/four tested assemblies matched. [Evidence](docs/roadmap.md#committed-files-and-local-action-latency-2026-10-28).

- [x] **0.14.0 / 2026-10-28:** complete-block stage/unstage, multi-hunk selection, stale preview/file/index/attribute checks, clear unsupported states and retained drafts. 140 checks, normal/minimum offscreen review, 16 preview checksums/seven matching tested files. [Evidence](docs/roadmap.md#selected-hunks-2026-10-28).

- [x] **0.13.0 / 2026-10-28:** modal commit workspace, colored/numbered diffs/binary sizes, file/all stage/unstage, commit/amend/Shift push, retained drafts and honest partial-success recovery. 129 checks, normal/minimum offscreen review, 16 preview checksums/seven matching tested files. [Evidence](docs/roadmap.md#modal-commit-workspace-2026-10-28).

- [x] **0.12.1 / 2026-10-28:** shared dark palette/styles for workspace and existing dialogs, high-contrast mapping, documented state conventions. 119 checks; normal/minimum offscreen review; 16 preview checksums/seven matching tested files. No desktop launch.

- [x] **F1g / 0.12.0 / 2026-10-28:** read-only Local Changes, separate index/worktree/untracked/conflicted rows, optional ignored paths, immediate cached navigation, stale-result/cancellation handling and virtualization. 117 checks, normal/minimum offscreen review; 16 preview checksums/seven matching tested files.

- [x] **0.11.1 / 2026-10-28:** cached editable/close-only operation dialogs, zero opening-time Git calls, post-submit validation/cancellation/stale-ref errors. 109 checks, headless first/warm measurements and offscreen review; 16 preview checksums/seven matching tested files.

- [x] **F2d / 0.11.0 / 2026-10-28:** dialogs open before async target preparation; default Rebase/alternative Merge/optional tracked auto-stash, sparse/superproject Pull, retained conflicts/recovery guidance. 106 checks, normal/minimum offscreen review, seven-sample timing runs, 16 checksums/seven matching tested files. Headless harness isolated from production startup; intended source/docs committed with preview ignored.
- [x] **F2c / 0.10.0 / 2026-10-27:** options close on submission; shell-owned Git execution/top-center phase/cancel/retained output through reinspection. Real incoming-commit/file Pull, refreshed history/counts, failures/cancellation; 93 checks, normal/minimum offscreen review, 16 preview checksums/seven matching tested files.
- [x] **0.9.3 / 2026-10-27:** ordinary branch/network actions without extra program consent; omitted unfinished toolbar/sidebar/dialog controls; localized unavailability help. 89 checks, normal/minimum offscreen review, 16 preview checksums/seven matching tested files.
- [x] **0.9.2 / 2026-10-27:** compact all ten maintained Markdown files; require working features and normal operation consent; remove routine trust/prototype mandates. Documentation/version delivery; verification recorded in roadmap.
- [x] **F2b / 0.9.1 / 2026-10-26:** Git/OpenSSH askpass, bounded operation pipe/lifetime, masked owned prompts/wait state/cancellation/reconciliation; 87 checks, offscreen review, 16 checksums/seven matching tested files.
- [x] **F2a / 0.8.0 / 2026-10-25:** real branch/network actions and ref badges; 77 checks, offscreen review and verified preview.
- [x] **F1f / 0.7.0 / 2026-10-25:** workspace layout/upstream counts; 65 checks, headless layout/timings.
- [x] **F1e / 0.6.0 / 2026-10-25:** direct contained Git, prompt-free history/schema 4, tab menus/MMB, packed 100k fixture; 61 checks.
- [x] **F1d / 0.5.0 / 2026-10-25:** automatic history/cache/author relation; 58 checks; paired median 3.89 → 2.29 s.
- [x] **0.4.2 / 2026-10-25:** selected icon/source/nine ICO sizes; 49 checks, extracted-icon inspection.
- [x] **0.4.1 / 2026-10-25:** headerless history/accessibility help; 49 checks, offscreen review.
- [x] **F1c / 0.4.0 / 2026-10-24:** All Commits, refs, 200-row pages beyond 2,000, full details/failure retention; 49 checks.
- [x] **0.3.1 / 2026-10-24:** Shovel title/executable/metadata, compatible profile; requested old-preview cleanup; 45 checks.
- [x] **F1b / 0.3.0 / 2026-10-23:** real HEAD history/graph/details/bounded loading; 45 checks.
- [x] **F1a / 0.2.0 / 2026-10-23:** async bounded runner, real metadata/worktree identity/markers, persistent tabs/migration, shutdown drain, versioned previews; 35 checks and limited published-window inspection.
- [x] **F0 / 0.1.0 / 2026-10-23:** confirmed C#/WPF/.NET 10, Git initialization/ignore, MVVM settings/resources/diagnostics/lifecycle; 17 checks and Windows 11 layout/navigation/splitter/save/relaunch smoke test.

Completed milestones retain their historical scope; desktop/live auth, Windows 10, full accessibility/performance and installed updates remain separate verification work.
