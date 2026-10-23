# Roadmap and decision register

Updated 2026-10-23. [Product](product.md) owns experience; [requirements](requirements.md) owns scope/safety/tests; [architecture](architecture.md) separates implemented choices from proposals. [TODO.md](../TODO.md) is the single prioritized implementation checklist; this file owns milestones, decisions and verification evidence.

## Current status

- **F0 foundation delivered; stop before Git features.** C# + WPF is confirmed by the user; .NET 10 LTS is selected. Solution, MVVM shell, English resources, lifecycle, local settings and safe diagnostics are implemented. Build/run instructions are in [README](../README.md).
- Existing instructions/documents were read and preserved; Git was initialized as explicitly authorized, with `.gitignore`. No commit or remote was created. SDK 10.0.401/Desktop Runtime 10.0.12 were already installed; no dependencies were installed.
- Official Fork screenshots/animations were inspected previously; user's attachment unavailable. X media could not be reviewed without login. Historical observations are not current runtime verification.
- No Git functionality, process runner, credentials/trust flow, installer/updater, hosting, signing keys or packages have been implemented. Git CLI remains the integration proposal. Updater/installer/host/signing remain unselected.
- New startup updater requirement supersedes old research advice to disable update checks. Other network privacy restrictions remain.

## Proposed delivery sequence

Each phase is a working increment with evidence and limitations. The user authorized the small F0 increment only; the next increment awaits instruction. Dependencies below are gates, not calendar estimates. Break each Git milestone into small reviewable deliveries rather than implementing a full row at once.

| Phase | Deliverable and boundary | Dependency / exit gate |
| --- | --- | --- |
| F0: Application foundation — complete | C#/WPF solution, MVVM empty shell, settings/resources/logging, startup/shutdown and checks | Build + 17 checks + Windows 11 GUI smoke verification below; no Git integration |
| 1: Read-only Git — next | Robust runner first; then real open, trust/Git detection, tabs, status/history/graph, metadata/tree/diff and interrupted-state detection | F0; G2-G7 read cases, E/S/L/A gates; no mocked functionality; baseline performance measurements |
| 2: Local MVP | Init, file/text-hunk staging, commit/amend, branch create/switch/delete/tracking, stash operations | Phase 1; mutation safety, hooks/signing, stale hunk tests |
| 3: Network MVP | Clone, remotes, fetch, push, upstream setup, proposed fast-forward-only pull default; external conflict resolution and valid continue/skip/abort for existing sequences | Phase 2; explicit interactive/auth routes, failures, external races and recovery |
| 3R: Pilot distribution | Install once/update in place, stable/test feeds, version/status UI, settings migration and bad-release repair procedure | Updater/IT/signing selection + phase 3; all U/D acceptance tests on actual standard-user Windows installs before initial 30-user rollout |
| 4: Advanced workflows | Start merge/rebase/interactive rebase/cherry-pick/revert, reset modes, tags, reflog/recovery, force-with-lease; built-in conflict editor | MVP; sequence/destructive/recovery integration tests |
| 5: Extended repository/lifecycle features | Submodule/worktree management, dedicated LFS workflows, shallow/partial/sparse management, optional tray, further large-repo optimization | Proven compatibility fixtures; idle/tray and performance regression gates |

**MVP means F0 and phases 1-3 plus 3R for distribution**, not complete Git feature parity. F0 alone is not a usable Git client. Prototype sharing must not be confused with the supported user rollout. Localization, accessible architecture and safety start in F0.

## Foundation verification 2026-10-23

Host: Windows 11 build 26200, x64; SDK 10.0.401, Windows Desktop Runtime/targeting pack 10.0.12. Commands: [README](../README.md#restore-build-and-check). Outputs/fixtures are ignored under `artifacts/`; no working repository was used for tests.

| Evidence | Result and limit |
| --- | --- |
| Restore and Release build | Passed; zero warnings/errors, no external package dependencies. Restricted execution required single-process MSBuild; disabled compiler sharing avoids server delays. |
| Automated foundation checks | 17 passed. Settings round-trip/backup, invalid/older/newer schemas, bounded input, geometry normalization, file locks, cancellation, external edits; diagnostic privacy/rotation/failure; headless WPF resource/binding and localized accessibility-name checks. |
| Actual GUI | Launched and visually inspected on Windows 11 at 1200×800 and 900×620 DIP. Empty frame, disabled Git/tools and readable details rendered. Sidebar click/arrow navigation, details tab selection and both splitters worked; normal close exited with code 0. Relaunch restored sidebar 256 DIP, details 300 DIP and selected section at the larger size. |
| Inspection fixes | Removed a foreground override that hid disabled styling; localized navigation AutomationProperties.Name rather than exposing record ToString text. Minimum-size inspection also caught clipping with a tall restored details panel: its maximum now adapts to available height and constrained main content can scroll. Rebuilt, reran all 17 checks and inspected the corrected minimum-size window. |
| Preliminary idle sample | Empty Release shell, 900×620 DIP, 24 logical CPUs: 138.77 MiB private bytes, 154.43 MiB working set; 0.0 accumulated CPU seconds over 30.01 seconds. Saved in `artifacts/gui-minimum/idle-sample.json`. Single developer-machine sample before the minimum-layout correction, not a benchmark or budget pass; no repository/child-process load, cold-start/p95 or five-minute measurement. |
| Preservation | GUI fixture settings and safe event logs confirmed normal save/close; newer/corrupt preservation is covered by automated fixtures, not a claim of installed-update recovery. |

**Not verified:** Windows 10 runtime, additional architectures, mixed-monitor/DPI/screen-reader matrix, translated/RTL UI, full performance budgets, fatal crash UX, disk-full behavior or installer/update behavior. No Git semantic tests are applicable yet. Standard WPF/system colors are used; dark theme and plural formatting are deferred. See TODO for the next bounded work, not a duplicate phase checklist here.

## Initial compatibility boundaries

These are phase-1-and-later requirements; F0 opens no repositories. Detection is required even when mutation is deferred. Initially open linked worktrees with common-state coordination, show submodule gitlinks/open separately, recognize LFS pointers/missing tooling, and explain shallow/partial/sparse limits. Block operations not yet validated for these configurations. Do not recursively update submodules, implicitly fetch missing objects, deepen history, expand sparse checkout, or bypass filters.

Phase 1 detects interrupted operations; phase 3 provides validated recovery actions. Starting advanced history operations waits for phase 4. Binary files get summaries rather than invented text diffs; hunk mutation initially covers validated text changes. Bare-repository editing, WSL repositories and network-share mutation are not promised by MVP; determine demand before adding support. ARM64 is proposed later; x86 is not currently planned.

## Material unresolved decisions

| ID | Decision | Why it matters / current proposal |
| --- | --- | --- |
| Q2 | Actual Windows editions/builds and CPU architectures; IT restrictions | Proposed x64 Windows 10 22H2/Windows 11; establish runtime support, App Installer availability, proxy and no-admin constraints |
| Q3 | Must binaries/notes be access-restricted, and must offsite coworkers update without VPN? | Determines static host/access method; prefer managed hosting or an existing maintained company endpoint |
| Q4 | Release/signing owner, available company certificate/identity infrastructure and budget | Determines publisher trust, key custody and supported updater/installer bundle; compare NetSparkle, Velopack, MSIX before selection |

**Q1 resolved:** user confirmed C# + WPF and authorized the small foundation on 2026-10-23; .NET 10 LTS selected after tooling inspection. This does not approve all future architecture proposals or deployment work.

Nonblocking details: product name/app identity must be fixed before first distributed install; screenshot must be available before claiming visual parity; exact update interval, Git floor, cache sizes and plural library should be validated when needed. English ResourceManager/.resx is implemented; the permanent data/install identity is still open.

## Documentation maintenance

Record decisions/date/rationale in architecture and resolve Q entries here. Keep requirement IDs stable. Maintain TODO.md after every implementation change and update this file's milestone/evidence status when delivering a phase. Research files remain historical references; do not let them override these documents or later user instructions.
