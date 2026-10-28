# Implementation checklist

Updated 2026-10-28. Single prioritized task list; [roadmap](docs/roadmap.md) holds completed milestones/evidence. P0 = correctness prerequisite; P1 = next delivery; P2 = later. Later user requests can change order without reapproving a whole historical phase.

## Current phase

0.15.0 delivered: selected-commit files/diffs, shared colored file markers and measured stage/unstage latency improvements. See the completed milestone below for verification; next priorities follow.

- [x] **P1 — Committed content.** Real parent-aware files/diffs, binary sizes, cancellation and bounded reads.
- [x] **P1 — File markers.** Shared modified/added/deleted symbols, accessible meanings and theme colors.
- [x] **P1 — Diagnose and reduce local-action delay.** Repeatable before/after fixture, narrower metadata reads and visible phase timings; preserve mutation checks.
- [x] **P1 — Deliver.** Complete regressions/offscreen review, documentation, matching preview and commit.


## Next priorities

- [ ] **P1 — In-app sequence recovery.** Show valid Continue/Skip/Abort for merge/rebase/cherry-pick/revert state; preserve conflict details after reopen, confirm destructive abort targets and verify disposable interrupted/conflicted cases. Keep command progress/cancellation visible.
- [ ] **P1 — Remaining stage/unstage latency.** Use the new phase timings if the reported 10-second case persists; then reproduce its dominant phase in a disposable fixture. Keep working Git validations and the current measured baseline.
- [ ] **P1 — Extend authentication when needed for network rollout.** Tracked GCM/browser, signing and interactive-hook interaction; preserve shared agents/browsers and hooks/host verification. Askpass exists. Prioritize working Git features; auth expansion is not a blanket prerequisite. Live provider/desktop tests require separate authorization.
- [ ] **P1 — Measure representative startup.** With launch authorization: cold/warm visible rows, keyboard/tabs, Windows 10/11, custom Git, profile restart/replacement. No interactive launch currently authorized. This validation is not a blanket blocker for independent features.
- [ ] **P1 — Extend performance fixtures.** Merge-heavy/million-commit cases, cancellation under load, app/child peak memory, five-minute idle CPU; retain F1e baseline.
- [ ] **P1 — Validate accessibility/layout.** Mixed DPI, OS high contrast/screen reader, keyboard/focus, +40%/RTL, themes. Offscreen colors do not certify OS accessibility.
- [ ] **P2 — External reconciliation/tools.** Focus/resume/external-client races and tracked tool sessions.
- [ ] **P2 — Filtering when requested.** Ref/search ancestry, default All Commits, retained selection/scroll.

## Backlog

- [ ] **P2 — Theme polish (low priority).** Extend the [shared pattern](docs/theme.md): light/theme selection, native title-bar integration where supported, popup/focus/hover/mixed-DPI/RTL and OS high-contrast desktop review. Preserve fast views and native control behavior; do not delay Git functionality.
- [ ] **P2 — Diff/editor extensions.** Historical file-tree browsing, individual-line staging, broader hunk file types, side-by-side/image previews, chosen encodings, ignored-file inspection and crash-safe per-repository draft storage; preserve bounded reads and clear unsupported states.

- [ ] **P1 — Local MVP remainder.** Init, branch deletion/tracking and stashes; broaden signing and failure recovery. Whole-hunk staging is delivered in 0.14.0.
- [ ] **P1 — Network MVP.** Clone/remote editing and visible auth/input; sequence recovery is prioritized above. Pull rebase/merge/auto-stash is in 0.11.0.
- [ ] **P1 — Pilot distribution.** Select updater/installer/hosting/signing, per-user install, channels/pilot/repair, migration/localization; pass U1–U7/D1 on installed Windows.
- [ ] **P2 — Advanced workflows.** Merge/rebase/interactive rebase/cherry-pick/revert/reset/tags/reflog/force-with-lease/conflict editor; disposable destructive fixtures.
- [ ] **P2 — Extended compatibility.** Submodule/worktree/LFS/shallow/partial/sparse management, tray and large-repo optimization.

## Decisions before rollout

- [ ] **Q2:** coworker Windows editions/builds, architectures, IT/proxy/no-admin constraints. Local SDK prerequisites are present.
- [ ] **Q3/Q4:** private/offsite/VPN access, host, release/signing owner, certificate/key custody, updater/installer; architecture compares candidates.
- [ ] Refresh pinned .NET 10 servicing patch before distribution; reverify.
- [ ] Finalize publisher/install/data identity before first installer; plan migration from Foundation profile. Shovel name is confirmed.

## Completed

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
