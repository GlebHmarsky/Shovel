# Implementation checklist

Updated 2026-10-27. Single prioritized task list; [roadmap](docs/roadmap.md) holds completed milestones/evidence. P0 = correctness prerequisite; P1 = next delivery; P2 = later. Later user requests can change order without reapproving a whole historical phase.

## Next priorities

- [ ] **P1 — Apply the 2026-10-27 interaction policy in code.** Remove separate repository-program trust approval from ordinary Branch/Fetch/Pull/Push execution. Normal action buttons authorize their operation; keep required auth/destructive confirmations. Implement or omit unfinished toolbar/sidebar entry points as those areas are delivered. Verify real execution, errors/cancellation and no routine trust modal; documentation-only 0.9.2 does not implement this.
- [ ] **P1 — Complete authentication routes before broader network rollout.** Tracked GCM/browser, signing and interactive-hook interaction; preserve shared agents/browsers and hooks/host verification. Askpass exists. Real HTTPS/SSH, unknown/rejected host key and live provider tests require separate authorization; they do not block unrelated local features.
- [ ] **P1 — Implement Local Changes/status.** Real porcelain-v2/NUL staged, unstaged, untracked, ignored/conflicted data; preserve history responsiveness. Disposable fixtures; staging/commit is separate scope.
- [ ] **P1 — Measure representative startup.** With launch authorization: cold/warm visible rows, keyboard/tabs, Windows 10/11, custom Git, profile restart/replacement. No interactive launch currently authorized. This validation is not a blanket blocker for independent features.
- [ ] **P1 — Extend performance fixtures.** Merge-heavy/million-commit cases, cancellation under load, app/child peak memory, five-minute idle CPU; retain F1e baseline.
- [ ] **P1 — Validate accessibility/layout.** Mixed DPI, OS high contrast/screen reader, keyboard/focus, +40%/RTL, themes. Offscreen colors do not certify OS accessibility.
- [ ] **P2 — Selected-commit files/diffs.** Bounded real data, exact paths, binary/EOL/large-content cases.
- [ ] **P2 — External reconciliation/tools.** Focus/resume/external-client races and tracked tool sessions.
- [ ] **P2 — Filtering when requested.** Ref/search ancestry, default All Commits, retained selection/scroll.

## Backlog

- [ ] **P1 — Local MVP.** Init, file/hunk stage/unstage, commit/amend, branch deletion/tracking, stashes; real hooks/signing and failure recovery.
- [ ] **P1 — Network MVP.** Clone/remote editing, broader pull strategies, valid sequence recovery and visible auth/input.
- [ ] **P1 — Pilot distribution.** Select updater/installer/hosting/signing, per-user install, channels/pilot/repair, migration/localization; pass U1–U7/D1 on installed Windows.
- [ ] **P2 — Advanced workflows.** Merge/rebase/interactive rebase/cherry-pick/revert/reset/tags/reflog/force-with-lease/conflict editor; disposable destructive fixtures.
- [ ] **P2 — Extended compatibility.** Submodule/worktree/LFS/shallow/partial/sparse management, tray and large-repo optimization.

## Decisions before rollout

- [ ] **Q2:** coworker Windows editions/builds, architectures, IT/proxy/no-admin constraints. Local SDK prerequisites are present.
- [ ] **Q3/Q4:** private/offsite/VPN access, host, release/signing owner, certificate/key custody, updater/installer; architecture compares candidates.
- [ ] Refresh pinned .NET 10 servicing patch before distribution; reverify.
- [ ] Finalize publisher/install/data identity before first installer; plan migration from Foundation profile. Shovel name is confirmed.

## Completed

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
