# Implementation checklist

Single prioritized task list. Updated 2026-10-23. P0 = prerequisite/safety, P1 = next delivery, P2 = later. [Roadmap](docs/roadmap.md) owns milestone scope and evidence; [requirements](docs/requirements.md) owns acceptance obligations. Check items only when their stated scope is verified.

## Current phase

Foundation F0 is complete. Stop here; further implementation needs the next user instruction. See Completed work and the roadmap's explicit verification limits.

## Next priorities

- [ ] **P0 — Agree the next small read-only increment.** Start with a robust asynchronous process runner and trusted Git discovery, then real repository open; no mutations. Validate structured arguments, concurrent output drains, cancellation and owned-child cleanup in disposable fixtures.
- [ ] **P0 — Introduce operation/state ownership before Git commands.** Common-directory/worktree identity, stale-result rejection, explicit prompt states and shutdown admission control; do not reuse the settings-profile mutex as a Git lock.
- [ ] **P1 — Add real open/status, then history and diffs in separate increments.** Persist real tab order/selection; distinguish empty, loading, missing Git, denied trust, interrupted sequences and errors. Enable controls only when their functions exist.
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
- [ ] **P1 before first installer — Identity:** finalize product name, publisher and permanent settings/install identity; plan migration from the provisional Foundation profile.

## Completed work

- [x] **P0 — Confirm C# + WPF; choose .NET 10 LTS.** Existing SDK/targeting pack sufficient; no third-party packages or installations.
- [x] **P0 — Initialize Git safely.** Existing documents preserved, `.gitignore` added; no commits or remote.
- [x] **P1 — Create solution and MVVM shell.** Separate WPF presentation from UI-independent settings/diagnostics; navigation and details are real layout interactions, Git/tool actions disabled.
- [x] **P0 — Establish lifecycle and persistence.** Async settings load/save, profile ownership, bounded safe diagnostics, guarded error exit, invalid/newer-schema preservation, atomic replacement/last-good backup and cancellation checks.
- [x] **P0 — Resource-back English text.** Dialogs, empty/loading/error states, tooltips and accessible labels; standard controls and named splitters. Further locale certification is not implied.
- [x] **P1 — Verify F0 and update references.** Restore/Release build: zero warnings/errors; 17 automated checks; actual Windows 11 GUI inspection at default/minimum size, navigation, splitters, detail tabs, saved layout/relaunch and normal exit. Short idle sample recorded; full budgets unverified. See roadmap for limits.
