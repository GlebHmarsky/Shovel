# Implementation checklist

Single prioritized task list. Updated 2026-10-25. P0 = prerequisite/safety, P1 = next delivery, P2 = later. [Roadmap](docs/roadmap.md) owns milestone scope and evidence; [requirements](docs/requirements.md) owns acceptance obligations. Check items only when their stated scope is verified.

## Current phase

All Commits / F1c / 0.4.0 is complete. The user prioritized complete named-ref history before other features; no interactive launch is authorized. The 0.4.1 follow-up removes visible column headings based on user screenshot feedback; 49 headless checks and normal/minimum render review passed.

## Next priorities

- [ ] **P1 — Manual preview review when the user is ready.** Inspect trust/open/refresh/cancel, history selection/scrolling, tab reorder/close and normal exit/relaunch. Headless checks are separate evidence; no desktop control in the current delivery.
- [ ] **P1 — Refine history after this increment.** Individual ref selection and search/filter with truthful graph ancestry; preserve per-page selection/scroll through navigation. Profile deep offset walks and compact page-cursor growth before large-repository claims.
- [ ] **P0 — Add read-only status.** Parse porcelain v2 with NUL paths; distinguish staged/unstaged/untracked/ignored, rename/delete and conflict entries. Compare disposable fixtures with CLI; keep optional locks, helpers and lazy fetch controlled.
- [ ] **P0 — Extend refresh/state coordination alongside status.** Reconcile external changes/focus/resume with bounded work and generation checks. Add more interrupted-sequence fixtures. Before mutations, coordinate by common-directory identity across instances; before prompts/tools, extend shutdown admission control.
- [ ] **P1 — Add selected-commit file list, then diffs.** Real bounded data, exact path bytes, binary/EOL/large-content cases; enable controls only when their functions exist.
- [ ] **P1 — Broaden shell validation.** Target Windows 10 plus Windows 11; mixed DPI, high contrast, screen reader, keyboard focus and +40%/RTL layouts at minimum size. Add dark/light theme and a plural-capable formatter before first count-dependent message. Preserve complete resource messages.
- [ ] **P1 — Record repeatable performance baselines.** Use the requirements' fixture/hardware/sample protocol; shell startup, private bytes, idle CPU and responsiveness separately. No p95 or lightweight claim from one launch.

## Backlog

- [ ] **P1 — Local MVP (phase 2).** Stage/unstage, commit/amend, branches/tracking and stashes, with trust, hooks/signing, previews and failure recovery.
- [ ] **P1 — Network MVP (phase 3).** Clone/remotes/fetch/push/pull and validated recovery actions; explicit auth/terminal routes, no invisible waits.
- [ ] **P1 — Pilot distribution (3R).** Select mature updater/installer, signed manifests/packages, per-user installation, channel/pilot/recovery and localization; pass U1-U7/D1 tests before rollout. No implementation in F0.
- [ ] **P2 — Advanced workflows (phase 4).** Start sequences, interactive rebase, reset, tags, reflog, force-with-lease and built-in conflict editor with disposable destructive tests.
- [ ] **P2 — Extended compatibility (phase 5).** Submodule/worktree/LFS/shallow/partial/sparse management, tray lifecycle and large-repository optimization after compatibility gates.

## Blocked items and decisions

- [ ] **P0 before supported rollout — Q2:** confirm coworkers' Windows editions/builds, CPU architectures and IT/proxy/no-admin constraints. No missing SDK prerequisite blocks local development on this machine.
- [ ] **P0 before distribution — Q3/Q4:** decide private/offsite/VPN access, hosting, signing owner/certificate/key custody and updater/installer. Preserve the architecture's comparison; no provider is selected.
- [ ] **P1 before distribution — Toolchain servicing:** refresh the pinned SDK/runtime to the then-current supported .NET 10 patch and reverify. No software installation was performed in F0.
- [ ] **P1 before first installer — Identity:** product name Shovel is confirmed; finalize publisher and permanent settings/install identity; plan migration from the provisional Foundation profile.

## Completed work

- [x] **P1 — Simplify history headings / 0.4.1.** Hide visible column headings per supplied screenshot; retain localized cell help and row descriptions. Restore/build/publish and all 49 checks passed; reviewed two offscreen layouts. No interactive launch or screen-reader verification.

- [x] **P0 — Confirm C# + WPF; choose .NET 10 LTS.** Existing SDK/targeting pack sufficient; no third-party packages or installations.
- [x] **P0 — Initialize Git safely.** Existing documents preserved and `.gitignore` added. Foundation committed after the later user authorization; no remote/push.
- [x] **P1 — Create solution and MVVM shell.** Separate WPF presentation from UI-independent settings/diagnostics; navigation and details are real layout interactions, Git/tool actions disabled.
- [x] **P0 — Establish lifecycle and persistence.** Async settings load/save, profile ownership, bounded safe diagnostics, guarded error exit, invalid/newer-schema preservation, atomic replacement/last-good backup and cancellation checks.
- [x] **P0 — Resource-back English text.** Dialogs, empty/loading/error states, tooltips and accessible labels; standard controls and named splitters. Further locale certification is not implied.
- [x] **P1 — Verify F0 and update references.** Restore/Release build: zero warnings/errors; 17 automated checks; actual Windows 11 GUI inspection at default/minimum size, navigation, splitters, detail tabs, saved layout/relaunch and normal exit. Short idle sample recorded; full budgets unverified. See roadmap for limits.
- [x] **P0 — Implement bounded process execution.** Structured arguments, concurrent byte streams, output/deadline limits, queued cancellation and owned-child job cleanup; no shell or invisible input.
- [x] **P1 — Open trusted worktrees for metadata.** Standard/explicit Git discovery, unborn/detached HEAD, common-directory identity and operation markers; manual refresh, cancellation and stale-result rejection.
- [x] **P1 — Preserve tabs safely.** Order/selection/reordering/close, schema 1-to-2 migration/backup; trust and snapshots never restored. Shutdown cancels/drains reads before save.
- [x] **P1 — Establish versioned delivery.** Central version 0.2.0, changelog and preview script; Build stays ignored and prior version folders cannot be overwritten.
- [x] **P0 — Verify and package F1a.** Restore/Release build/publish succeeded; 35 checks passed. Published 0.2.0 window launched and rendered on Windows 11; Build folder kept outside version control. Manual interaction coverage remains explicitly limited above.
- [x] **P1 — Deliver F1b history / 0.3.0.** Real HEAD ancestry with connected graph nodes, description/author/date/hash columns, selected-commit details and bounded older-history loading. Restore/Release build/publish passed with zero warnings/errors; 45 checks passed, including real disposable histories, stale results and headless row virtualization. Reviewed two offscreen renders; interactive GUI remains unverified. Ignored preview checksums and tested/published assemblies match; prior preview preserved.

- [x] **P1 — Name and rebuild Shovel / 0.3.1.** Resource-backed title, executable and product metadata updated; existing profile preserved. Old Build packages removed on request; clean Release build, all 45 checks and publish passed. Checksums/tested assemblies verified; no interactive launch.

- [x] **P0 — Deliver All Commits / F1c / 0.4.0.** Default local/remote/tag ancestry and explicit current-HEAD scope; ref labels, 200-row older/newer pages beyond 2,000, full selected details and preserved rows on paging failures. Restore/Release build/publish and 49 checks passed; offscreen normal/minimum renders reviewed. Ignored preview checksums/tested assemblies match. No interactive launch; visual keyboard/DPI review remains open.
