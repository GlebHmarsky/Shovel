# Implementation checklist

Single prioritized task list. Updated 2026-10-25. P0 = prerequisite/safety, P1 = next delivery, P2 = later. [Roadmap](docs/roadmap.md) owns milestone scope and evidence; [requirements](docs/requirements.md) owns acceptance obligations. Check items only when their stated scope is verified.

## Current phase

**Current: explicit branch/network actions and ref badges / 0.8.0.** User authorized this next capability increment. No interactive launch, desktop control or live authentication/network tests.

- [x] **P1 — Add Branch split button and operation dialogs.** New/switch branch, Fetch, fast-forward-only Pull, normal Push; explicit targets, optional upstream/tags and disabled advanced options.
- [x] **P0 — Verify execution safety.** Identity/config/ref revalidation, linked-worktree gate, dirty/sequence rejection, hook failures, cancellation and state reconciliation in disposable repositories.
- [x] **P1 — Verify ref badges and layout.** Current branch check, branch/tag/remote icons, combined upstream badge on matching tip, accessible help and normal/minimum offscreen renders.
- [x] **P0 — Deliver.** Restore/build/publish and 77 checks passed; docs/version/changelog updated. Ignored 0.8.0 preview has 11 verified checksums and matching tested assemblies. Intended source/docs committed with this delivery; live authentication and desktop behavior remain unverified.

## Next priorities

- [ ] **P1 — Finish interactive authentication before broader network rollout.** Design an explicit askpass/credential-manager/SSH/signing route; test with separately authorized accounts. Current actions require noninteractive credentials and known hosts. Keep bounded cancellation and do not bypass hooks.

- [ ] **P1 — Implement Local Changes/status after workspace review.** Replace that prototype with real porcelain-v2/NUL data: staged, unstaged, untracked, ignored and conflicted states. Retain automatic history responsiveness and test only disposable repositories; no staging/commit implementation yet.
- [ ] **P0 — Validate startup on representative repositories before more features.** With separate authorization, measure actual cold/warm launch, first visible rows and keyboard/tab responsiveness on Windows 10/11; no interactive launch is authorized now. Include custom Git installations and real profile restart/replacement detection.
- [ ] **P1 — Extend performance fixtures.** Extend the measured packed 100,000-commit/1,201-ref fixture to merge-heavy/million-commit repositories, cancellation under load, app/child peak memory and five-minute idle CPU. Use the F1e baseline before more runner changes.
- [ ] **P1 — Broaden accessibility/layout validation.** Mixed DPI, actual high contrast/screen reader, keyboard focus, +40%/RTL layouts and full theme selection. Headless high-contrast color fallback is not an OS accessibility pass.
- [ ] **P2 — Revisit filtering only when requested.** Individual ref selection/search with truthful ancestry; preserve the default All Commits view, selected hash and scroll.
- [ ] **P2 — Selected-commit file list, then diffs.** Bounded real data, exact paths and binary/EOL/large-content cases; no placeholders.
- [ ] **P2 — External reconciliation.** Extend focus/resume state handling and external-client race coverage beyond the explicit-action reinspection; track external-tool sessions before enabling them.

## Backlog

- [ ] **P1 — Local MVP (phase 2).** Stage/unstage, commit/amend, branch deletion/additional tracking and stashes, with trust, hooks/signing, previews and failure recovery.
- [ ] **P1 — Network MVP (phase 3).** Clone/remote editing, broader pull strategies and validated recovery actions; explicit auth/terminal routes, no invisible waits.
- [ ] **P1 — Pilot distribution (3R).** Select mature updater/installer, signed manifests/packages, per-user installation, channel/pilot/recovery and localization; pass U1-U7/D1 tests before rollout. No implementation in F0.
- [ ] **P2 — Advanced workflows (phase 4).** Start sequences, interactive rebase, reset, tags, reflog, force-with-lease and built-in conflict editor with disposable destructive tests.
- [ ] **P2 — Extended compatibility (phase 5).** Submodule/worktree/LFS/shallow/partial/sparse management, tray lifecycle and large-repository optimization after compatibility gates.

## Blocked items and decisions

- [ ] **P0 before supported rollout — Q2:** confirm coworkers' Windows editions/builds, CPU architectures and IT/proxy/no-admin constraints. No missing SDK prerequisite blocks local development on this machine.
- [ ] **P0 before distribution — Q3/Q4:** decide private/offsite/VPN access, hosting, signing owner/certificate/key custody and updater/installer. Preserve the architecture's comparison; no provider is selected.
- [ ] **P1 before distribution — Toolchain servicing:** refresh the pinned SDK/runtime to the then-current supported .NET 10 patch and reverify. No software installation was performed in F0.
- [ ] **P1 before first installer — Identity:** product name Shovel is confirmed; finalize publisher and permanent settings/install identity; plan migration from the provisional Foundation profile.

## Completed work

- [x] **P1 — Branch/network actions and ref badges / 0.8.0.** F2a delivered on 2026-10-25 with 77 checks, headless dialog/workspace review and verified preview. See roadmap F2a for scope, evidence and limits.

- [x] **P1 — Workspace and upstream counts / 0.7.0.** Current-phase scope completed on 2026-10-25; roadmap F1f records evidence and limits. No interactive launch or actual pull/push.

- [x] **P1 — Fast local history and tab actions / 0.6.0.** F1e completed on 2026-10-25; detailed evidence and remaining performance/desktop limits are in roadmap F1e.

- [x] **P1 — Automatic history / 0.5.0.** Completed F1d on 2026-10-25. Paired first-page median 3.89 → 2.29 seconds; warm cached switches issue no Git commands. Actual desktop launch/input, Windows 10 and memory/idle budgets remain unverified; F1e adds larger synthetic histories.

- [x] **P1 — Add selected application icon / 0.4.2.** Embed the front-facing shovel artwork in the executable and main window; preserve source PNG and nine ICO sizes. Release build/publish and all 49 headless checks passed on 2026-10-25. No interactive launch; desktop/taskbar cache behavior remains unverified.

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
