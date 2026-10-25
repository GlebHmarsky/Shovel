# Implementation checklist

Single prioritized task list. Updated 2026-10-25. P0 = prerequisite/safety, P1 = next delivery, P2 = later. [Roadmap](docs/roadmap.md) owns milestone scope and evidence; [requirements](docs/requirements.md) owns acceptance obligations. Check items only when their stated scope is verified.

## Current phase

**Delivered: automatic history and responsiveness / 0.5.0.** Prioritized by the user before further Git features. No interactive launch or desktop control. Next work starts with the validation priorities below.

- [x] **P0 — Measure first.** Repeated headless settings/discovery/inspection/refs/history/graph/UI timings; separate first load, cached tab switches and refresh. Evidence and limits in roadmap.
- [x] **P0 — Remember revocable repository approval safely.** Audited automatic reads, identity checks before commands, migration without granting old paths consent.
- [x] **P1 — Automatic selected-tab history.** Startup/selection loading, bounded per-tab rows/page/scope/selection/scroll; retained rows on refresh/failure and rejected obsolete work.
- [x] **P1 — Simplify history controls and highlight matching authors.** Implemented Repository menu, accessible same-author indication and headless high-contrast fallback checks.
- [x] **P0 — Verify and deliver.** Restore/build/publish, 58 disposable/headless checks, before/after report, normal/minimum offscreen render review and matching ignored 0.5.0 preview. Commit recorded in Git history.

## Next priorities

- [ ] **P0 — Validate startup on representative repositories before more features.** With separate authorization, measure actual cold/warm launch, first visible rows and keyboard/tab responsiveness on Windows 10/11; no interactive launch is authorized now. Include custom Git installations and real profile restart/revocation.
- [ ] **P1 — Extend performance fixtures.** Packed 100,000-commit/many-ref histories, deep-offset paging, cancellation under load, app/child peak memory and five-minute idle CPU; compare the measured F1d baseline before changing the runner further.
- [ ] **P1 — Broaden accessibility/layout validation.** Mixed DPI, actual high contrast/screen reader, keyboard focus, +40%/RTL layouts and full theme selection. Headless high-contrast color fallback is not an OS accessibility pass.
- [ ] **P2 — Resume history improvements after responsiveness review.** Individual ref selection/search with truthful ancestry; keep scope, selected hash and scroll preservation.
- [ ] **P2 — Selected-commit file list, then diffs.** Bounded real data, exact paths and binary/EOL/large-content cases; no placeholders.
- [ ] **P2 — Read-only status and reconciliation.** Porcelain-v2/NUL parser, staged/unstaged/untracked/ignored/conflict distinctions; extend focus/resume state handling before any mutation or external-tool work.

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

- [x] **P1 — Automatic history / 0.5.0.** Completed the current-phase scope above on 2026-10-25. Paired first-page median 3.89 → 2.29 seconds; warm cached switches issue no Git commands. Actual desktop launch/input, Windows 10, larger histories and memory/idle budgets remain unverified.

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
