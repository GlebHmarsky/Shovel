# Roadmap and decision register

Updated 2026-10-24. [Product](product.md) owns experience; [requirements](requirements.md) owns scope/safety/tests; [architecture](architecture.md) separates implemented choices from proposals. [TODO.md](../TODO.md) is the single prioritized implementation checklist; this file owns milestones, decisions and verification evidence.

## Current status

- **All Commits / F1c / 0.4.0 is complete.** The user prioritized the main history table/graph ahead of status and requested no desktop control. C# + WPF / .NET 10 LTS is confirmed. Verification limits are below; build/run/package instructions are in [README](../README.md).
- Existing instructions/documents were read and preserved; Git was initialized with `.gitignore`. Foundation commit `fff8f12` was created after the user's commit authorization. SDK 10.0.401/Desktop Runtime 10.0.12 were already installed; no dependencies were installed. No remote/push.
- Official Fork screenshots/animations were inspected previously; user's attachment unavailable. X media could not be reviewed without login. Historical observations are not current runtime verification.
- Git CLI is implemented for repository metadata, all-branch/tag and HEAD ancestry. No individual-ref selection/search/filter, status/diffs, mutation, interactive credentials, installer/updater, hosting or signing is implemented. Local preview folders are distinct from distribution packages; updater/installer/host/signing remain unselected.
- New startup updater requirement supersedes old research advice to disable update checks. Other network privacy restrictions remain.

## Proposed delivery sequence

Each phase is a working increment with evidence and limitations. The user authorized commits, version increments and local previews; on 2026-10-23 they prioritized history before status. The latest request prioritizes All Commits visibility, ref labels, complete details and pagination before other features; stop after this history delivery. Dependencies below are gates, not calendar estimates; break Git milestones into small deliveries.

| Phase | Deliverable and boundary | Dependency / exit gate |
| --- | --- | --- |
| F0: Application foundation — complete | C#/WPF solution, MVVM empty shell, settings/resources/logging, startup/shutdown and checks | Build + 17 checks + Windows 11 GUI smoke verification below; no Git integration |
| 1a: Metadata preview — delivered | Runner, real open, session trust/Git detection, persistent tabs, HEAD/common-directory identity and interrupted markers | Build/publish + 35 checks + published-window inspection; manual interaction gates remain open; local package only |
| 1b: History preview — delivered | Real HEAD ancestry, graph/description table, selected-commit metadata, bounded older-history loading | Build/publish + 45 automated checks and headless render review; no desktop control or interactive launch |
| 1c: All Commits — delivered | Local/remote/tag history, scope/labels, bounded older/newer pages without a total count ceiling | Headless multi-ref/pagination/failure checks and preview; manual visual confirmation remains open |
| 1d onward: Read-only Git — next | Individual ref selection/search; status and changed-file/tree/diff reads in bounded increments; external reconciliation | F1c; G2-G8 read cases, E/S/L/A gates; baseline performance measurements |
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

**F0 limits:** Windows 10 runtime, additional architectures, mixed-monitor/DPI/screen-reader matrix, translated/RTL UI, full performance budgets, fatal crash UX, disk-full behavior or installer/update behavior were not verified. Standard WPF/system colors are used; dark theme and plural formatting are deferred. Later evidence below does not retroactively broaden these results.

## Read-only preview verification 2026-10-23

Same Windows 11 x64/SDK/runtime host as F0; Git for Windows 2.51.1. Automated checks use real disposable repository mutations only for fixture setup, not production app capabilities. Test-only view-model transports do not substitute for real Git integration tests.

| Evidence | Result and limit |
| --- | --- |
| Restore / Release build / publish | Passed, zero warnings/errors and no dependency installation. Full preview script passed after fixing a duplicate helper-EXE publish item; failed staging never entered Build. |
| Automated checks | 35 passed, 0 failed. All F0 cases plus argument preservation, simultaneous output/streaming, output cap, closed stdin, exit/start failures, timeout/cancellation/descendants, inherited pipes, queued cancellation and failed-consumer cleanup. Settings migration/backup/tab order; real missing/unsupported Git, unborn/detached/linked/bare/non-repository, conflicted merge preserved across inspections, helper suppression and external branch changes. Model tests verify trust denial/restoration, shutdown drain and obsolete-result rejection. |
| Reproduction | `scripts/Build-Preview.ps1`; final fixture run `artifacts/foundation-checks/20261023-092921-9525076807c64317b3b9fb6aa716bc2f`. All fixture/output folders ignored. |
| Actual published GUI | Launched `Build/0.2.0-win-x64/GitGui.App.exe` with isolated `artifacts/gui-v020` settings. Visually inspected at 1200×800 DIP: version, two restored Unicode-path tabs, disabled future actions, not-inspected/trust state and details rendered. No automatic inspection on restored tabs. User pressed Escape to stop Computer Use before interactive tab/metadata/close tests; no further UI input sent. |
| Local preview | Framework-dependent unsigned folder, helper files, START-HERE and SHA256 checksums; Build ignored. Requires installed .NET 10 Desktop Runtime. No installer, update feed, signing keys, hosting or remote push. |

**Still unverified:** manual trust/open/refresh/cancel/reorder/close/relaunch in the published GUI, more interrupted-sequence types, live authentication/network failure, crash/sleep/disk-full behavior, Windows 10, additional architectures, complete accessibility/DPI/localization and measured performance budgets. F0's idle sample does not measure 0.2.0. Cross-instance mutations and updater idle gates are deferred and must precede those features.

## History preview verification 2026-10-23

Host remains Windows 11 x64 build 26200, SDK 10.0.401, Desktop Runtime 10.0.12 and Git for Windows 2.51.1. No desktop control or interactive app launch was performed, as requested. This delivery does not close the older manual-UI verification gaps.

| Evidence | Result and limit |
| --- | --- |
| Restore / Release build / publish | Passed with zero warnings/errors; final preview script ran all 45 checks successfully, including the early parent-count guard. No dependencies installed. |
| Real Git histories | Disposable Unicode-path repositories with linear, merge, original-offset/UTC dates, multiline messages and a 2,005-commit history. Ordering matches CLI; lookahead/caps/pinned expansion survive ref movement; shallow boundaries are explicit. Signed fixture plus configured verifier proves history does not launch signature helpers. No working repositories or network used. |
| Parser/graph | Malformed field boundaries and invalid hashes fail visibly; variable-width hash fixture supported. Merge/octopus/convergence edges meet adjacent rows, cycles/overwide graphs rejected. Original commit text remains in memory while visible text is bounded. |
| Headless WPF | Actual views bound to real fixture histories, measured and rendered without Show/desktop input. Table has graph then description/author/date/hash; selected row exposes matching message/parents. 400 loaded rows realize fewer than 60 WPF rows; stale history completion cannot populate another tab. Inspected offscreen PNGs at 1200×760 and 900×580 content size; fixed background, inactive-selection graph contrast and narrow-column clipping. |
| Reproduction | `scripts/Build-Preview.ps1`; final fixture run `artifacts/foundation-checks/20261023-140125-40cd3a52a608469fb58f8df98623891d`, with `history-table.png` and `history-minimum.png` offscreen renders. Fixtures and outputs are ignored. |
| Preview | `Build/0.3.0-win-x64/GitGui.App.exe`; all 14 file checksums verified, published App/Core/ProcessHost assemblies match the tested assemblies, existing-version overwrite guard passed. Build/artifacts are ignored and untracked. Initial package retained unchanged under ignored `artifacts/preview-0.3.0-before-final-hardening`; 0.2.0 preview untouched. |

**Limits:** the 2,005-commit fixture is a bounded-loading regression, not a large-repository performance benchmark. No interactive scrolling/keyboard/screen-reader, mixed DPI, high contrast, Windows 10 or published-GUI launch verification. Only HEAD ancestry is shown; all-ref history, search/filter, file status/diffs and all mutations remain deferred. Settings remain schema 2. No installation/update/hosting/signing work.

## Shovel preview verification 2026-10-24

The user confirmed the product name **Shovel** and requested cleanup of old Build outputs before a new preview. Removed the old 0.2.0/0.3.0 Build packages and cleaned Release outputs; historical package links above document earlier verification and no longer point to retained previews. Settings, source and ignored test evidence were preserved. Version 0.3.1 changes title/executable/product metadata only; Git capabilities and settings schema are unchanged.

Verification: Release clean, restore/build and publish succeeded with zero warnings/errors; all 45 disposable/headless checks passed. Fixture: `artifacts/foundation-checks/20261024-185313-4fe1042215ed4fb985c8d7266b9dc1a7`. Preview: `Build/0.3.1-win-x64/Shovel.exe`; 14 checksums verified, published assemblies match tested assemblies, executable product/description is Shovel and file version is 0.3.1.0. Build contains only the new version and remains ignored/untracked. No desktop control or interactive app launch occurred; earlier runtime/Windows 10/accessibility/performance limitations remain.

## All Commits verification 2026-10-24

Diagnosis: 0.3.x showed only HEAD ancestry, initially 200 rows and at most 2,000; no active filters caused the missing branch/tag commits. F1c adds default All Commits, explicit current-HEAD scope, ref labels, older/newer paging and full selected details. A first headless check caught the new controls crowding the table at minimum size; layout was adjusted before delivery.

Same Windows 11 x64 build 26200 / SDK 10.0.401 / Desktop Runtime 10.0.12 / Git 2.51.1 host. Restore, Release build and publish passed with zero warnings/errors; **49 checks passed, 0 failed**. Final fixture: `artifacts/foundation-checks/20261024-202512-c208c14830e249aeb7988d350675a7f1`.

- Real disposable Git: all 2,005 commits traversed in bounded pages, CLI ordering equality/no duplicates, parent-line continuity across page boundaries; multiple branches, merges, remote-only/tag-only ancestry, lightweight/annotated/nested/non-commit tags and packed refs. Reflog/stash/detached-only exclusions, current detached history, empty/unborn, shallow and external ref deletion/movement covered.
- Headless WPF: default/switchable scope, real ref labels and full 100,000-character-plus message selection; older/newer ranges and virtualized rows. Controlled output-limit/cancellation/timeout/ref-change failures retain the prior page. Reviewed `all-commits.png` (1200×760) and `all-commits-minimum.png` (900×580); history now reserves space for several rows at minimum size. These are offscreen renders, not desktop interaction.
- Preview: `Build/0.4.0-win-x64/Shovel.exe`; all 14 checksums verified, published App/Core/ProcessHost assemblies match tested assemblies, product/version metadata verified. Build remains ignored/untracked. Initial pre-layout-review 0.4.0 package retained under ignored `artifacts/preview-0.4.0-before-layout-review`; 0.3.1 remains available.

No interactive launch occurred. Actual keyboard/scrolling/splitter use, mixed DPI/high contrast/screen readers, Windows 10, full performance budgets and installed-update behavior still need verification. The 2,005 fixture is a pagination regression, not a large-repository benchmark; deep offset walks/cursor growth remain known limits. No new Git mutations, network operations or dependencies.

## Initial compatibility boundaries

F1a inspects ordinary and linked worktrees, rejects bare repositories and observes common operation markers. F1c reads named-ref/HEAD ancestry and recognizes shallow boundaries. Submodule/LFS/partial/sparse modes and file content are not comprehensively validated; missing commit objects fail without fetching. No submodule recursion, deepening, sparse expansion or filter bypass is available. Common-directory identity alone is not a shared-state lock.

Phase 1 detects interrupted operations; phase 3 provides validated recovery actions. Starting advanced history operations waits for phase 4. Binary files get summaries rather than invented text diffs; hunk mutation initially covers validated text changes. Bare-repository editing, WSL repositories and network-share mutation are not promised by MVP; determine demand before adding support. ARM64 is proposed later; x86 is not currently planned.

## Material unresolved decisions

| ID | Decision | Why it matters / current proposal |
| --- | --- | --- |
| Q2 | Actual Windows editions/builds and CPU architectures; IT restrictions | Proposed x64 Windows 10 22H2/Windows 11; establish runtime support, App Installer availability, proxy and no-admin constraints |
| Q3 | Must binaries/notes be access-restricted, and must offsite coworkers update without VPN? | Determines static host/access method; prefer managed hosting or an existing maintained company endpoint |
| Q4 | Release/signing owner, available company certificate/identity infrastructure and budget | Determines publisher trust, key custody and supported updater/installer bundle; compare NetSparkle, Velopack, MSIX before selection |

**Q1 resolved:** user confirmed C# + WPF and authorized the small foundation on 2026-10-23; .NET 10 LTS selected after tooling inspection. This does not approve all future architecture proposals or deployment work.

Nonblocking details: product name Shovel is confirmed; publisher/install identity must be fixed before first distributed install; screenshot must be available before claiming visual parity; exact update interval, cache sizes and plural library should be validated when needed. Git floor is 2.47.0 for metadata reads, tested here only on 2.51.1. English ResourceManager/.resx is implemented; permanent data/install identity remains open.

## Documentation maintenance

Record decisions/date/rationale in architecture and resolve Q entries here. Keep requirement IDs stable. Maintain TODO.md after every implementation change and update this file's milestone/evidence status when delivering a phase. Research files remain historical references; do not let them override these documents or later user instructions.
