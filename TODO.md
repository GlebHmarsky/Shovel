# Implementation checklist

Updated 2026-10-28. Single prioritized task list; [roadmap](docs/roadmap.md) holds completed milestones/evidence. P0 = correctness prerequisite; P1 = next delivery; P2 = later. Later user requests can change order without reapproving a whole historical phase.

## Current phase

0.11.1 complete: fixed-size close-only operation dialogs, immediately editable from cached local targets with zero opening-time Git work. Post-submission validation retains progress/errors and stale-context protection. [Evidence](docs/roadmap.md#cached-operation-dialogs-2026-10-28): 109 passing checks, headless timings/layouts and matching ignored preview. Next feature: Local Changes/status; desktop/live-provider validation remains separate.

## Next priorities

- [ ] **P1 — Implement Local Changes/status.** Real porcelain-v2/NUL staged, unstaged, untracked, ignored/conflicted data; preserve history responsiveness. Disposable fixtures; staging/commit is separate scope.
- [ ] **P1 — Extend authentication when needed for network rollout.** Tracked GCM/browser, signing and interactive-hook interaction; preserve shared agents/browsers and hooks/host verification. Askpass exists. Prioritize working Git features; auth expansion is not a blanket prerequisite. Live provider/desktop tests require separate authorization.
- [ ] **P1 — Measure representative startup.** With launch authorization: cold/warm visible rows, keyboard/tabs, Windows 10/11, custom Git, profile restart/replacement. No interactive launch currently authorized. This validation is not a blanket blocker for independent features.
- [ ] **P1 — Extend performance fixtures.** Merge-heavy/million-commit cases, cancellation under load, app/child peak memory, five-minute idle CPU; retain F1e baseline.
- [ ] **P1 — Validate accessibility/layout.** Mixed DPI, OS high contrast/screen reader, keyboard/focus, +40%/RTL, themes. Offscreen colors do not certify OS accessibility.
- [ ] **P2 — Selected-commit files/diffs.** Bounded real data, exact paths, binary/EOL/large-content cases.
- [ ] **P2 — External reconciliation/tools.** Focus/resume/external-client races and tracked tool sessions.
- [ ] **P2 — Filtering when requested.** Ref/search ancestry, default All Commits, retained selection/scroll.

## Backlog

- [ ] **P1 — Local MVP.** Init, file/hunk stage/unstage, commit/amend, branch deletion/tracking, stashes; real hooks/signing and failure recovery.
- [ ] **P1 — Network MVP.** Clone/remote editing, valid sequence recovery and visible auth/input; Pull rebase/merge/auto-stash is in 0.11.0.
- [ ] **P1 — Pilot distribution.** Select updater/installer/hosting/signing, per-user install, channels/pilot/repair, migration/localization; pass U1–U7/D1 on installed Windows.
- [ ] **P2 — Advanced workflows.** Merge/rebase/interactive rebase/cherry-pick/revert/reset/tags/reflog/force-with-lease/conflict editor; disposable destructive fixtures.
- [ ] **P2 — Extended compatibility.** Submodule/worktree/LFS/shallow/partial/sparse management, tray and large-repo optimization.

## Decisions before rollout

- [ ] **Q2:** coworker Windows editions/builds, architectures, IT/proxy/no-admin constraints. Local SDK prerequisites are present.
- [ ] **Q3/Q4:** private/offsite/VPN access, host, release/signing owner, certificate/key custody, updater/installer; architecture compares candidates.
- [ ] Refresh pinned .NET 10 servicing patch before distribution; reverify.
- [ ] Finalize publisher/install/data identity before first installer; plan migration from Foundation profile. Shovel name is confirmed.

## Completed

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
