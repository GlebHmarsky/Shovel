# Roadmap and decision register

Updated 2026-10-25. [Product](product.md) owns experience; [requirements](requirements.md) owns scope/safety/tests; [architecture](architecture.md) separates implemented choices from proposals. [TODO.md](../TODO.md) is the single prioritized implementation checklist; this file owns milestones, decisions and verification evidence.

## Current status

- **Fast local history / F1e / 0.6.0 is delivered.** The user prioritized the main history table/graph ahead of status and requested no desktop control. C# + WPF / .NET 10 LTS is confirmed. Verification limits are below; build/run/package instructions are in [README](../README.md).
- Existing instructions/documents were read and preserved; Git was initialized with `.gitignore`. Foundation commit `fff8f12` was created after the user's commit authorization. SDK 10.0.401/Desktop Runtime 10.0.12 were already installed; no dependencies were installed. No remote/push.
- Official Fork screenshots/animations were inspected previously; the original attachment was unavailable. A new user screenshot was inspected on 2026-10-25 for the headerless history refinement. X media could not be reviewed without login. Historical observations are not current runtime verification.
- Git CLI is implemented for repository metadata, all-branch/tag and HEAD ancestry. No individual-ref selection/search/filter, status/diffs, mutation, interactive credentials, installer/updater, hosting or signing is implemented. Local preview folders are distinct from distribution packages; updater/installer/host/signing remain unselected.
- New startup updater requirement supersedes old research advice to disable update checks. Other network privacy restrictions remain.

## Proposed delivery sequence

Each phase is a working increment with evidence and limitations. The user authorized commits, version increments and local previews; on 2026-10-23 they prioritized history before status. The latest request prioritizes automatic startup history, bounded tab caching, responsiveness, prompt-free history, lower read latency and tab context/middle-click actions. Stop after this increment; new Git features remain deferred. Dependencies below are gates, not calendar estimates; break Git milestones into small deliveries.

| Phase | Deliverable and boundary | Dependency / exit gate |
| --- | --- | --- |
| F0: Application foundation — complete | C#/WPF solution, MVVM empty shell, settings/resources/logging, startup/shutdown and checks | Build + 17 checks + Windows 11 GUI smoke verification below; no Git integration |
| 1a: Metadata preview — delivered | Runner, real open, session trust/Git detection, persistent tabs, HEAD/common-directory identity and interrupted markers | Build/publish + 35 checks + published-window inspection; manual interaction gates remain open; local package only |
| 1b: History preview — delivered | Real HEAD ancestry, graph/description table, selected-commit metadata, bounded older-history loading | Build/publish + 45 automated checks and headless render review; no desktop control or interactive launch |
| 1c: All Commits — delivered | Local/remote/tag history, scope/labels, bounded older/newer pages without a total count ceiling | Headless multi-ref/pagination/failure checks and preview; manual visual confirmation remains open |
| 1d: Automatic history — delivered | Remembered approval, startup/activation reads, bounded tab caches, menu cleanup, author highlighting and measured startup path | Restore/build/publish, 58 checks, headless timing and render evidence below; no interactive launch |
| 1e: Fast local history — delivered | Direct job-contained Git, native entry point, combined HEAD/refs, no consent question, tab menus/middle-click | Build/publish, 61 checks, measured 0.5.0 comparison and packed 100k fixture; no interactive launch |
| 1f onward: Read-only Git — deferred until responsiveness review | Individual ref selection/search; status and changed-file/tree/diff reads in bounded increments; external reconciliation | F1c; G2-G8 read cases, E/S/L/A gates; baseline performance measurements |
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

## Headerless history refinement 2026-10-25

Version 0.4.1 removes visible column headings based on the newly supplied Fork screenshot. Column resources remain as cell automation help; row descriptions remain named. Header drag resizing is unavailable, while description width adapts to the window. Scope, status and paging remain visible; no Git behavior changes.

Restore/Release build/publish succeeded with zero warnings/errors; all 49 checks passed. Reviewed normal/minimum offscreen renders in `artifacts/foundation-checks/20261024-222743-b5427f8ca010488182df81cd412f74b3` (fixture names use UTC). Preview: `Build/0.4.1-win-x64/Shovel.exe`; earlier previews retained. No interactive launch, desktop control or screen-reader test; the existing runtime/accessibility limits remain.

## Application icon verification 2026-10-25

Version 0.4.2 adds the user-selected front-facing silver shovel on a blue circle to the executable and main window, with the source PNG retained and nine transparent ICO sizes (16–256 pixels). An initial headless run caught a window resource URI resolving against the test executable; an assembly-qualified URI fixed it.

Restore/Release build/publish succeeded with zero warnings/errors; all 49 checks passed in `artifacts/foundation-checks/20261025-014650-48835f984c0745538acfcc77aa4d75d6`. The executable icon was extracted and inspected at small size; ICO frame sizes and transparent corners were checked. Preview: `Build/0.4.2-win-x64/Shovel.exe`; all 14 checksums verified and published assemblies match the tested build. Earlier previews remain untouched; Build stays ignored. No interactive launch or desktop control; actual taskbar/Explorer caching and DPI appearance remain unverified.

## Automatic history performance and verification 2026-10-25

The reported ten-second delay was not reproduced on the synthetic fixture. Confirmed causes in 0.4.2: restored paths never triggered a read, tab changes discarded history, session approval repeatedly gated progress, and first-page loading ran 14 sequential helper/Git processes (eight metadata calls and four ref enumerations). The graph and row formatting were negligible relative to those processes. The app already requested only 201 log records, not full-history materialization; `--topo-order` can still inspect ancestry internally and deep `--skip` walks remain costly.

0.5.0 uses one combined location query, two bounded parallel HEAD-query pairs, a reused initial ref snapshot, source-generated protocol JSON, and a selected-tab read chain. It retains up to three current pages with a 32 MiB character-data estimate and 128 backwards cursors per cached tab. No persistent helper/service, new dependency, preloading or polling. Remembered identity approval, automatic loading and menu/author behavior are owned by product/architecture; these supersede the historical session-only/manual-refresh behavior above.

**Repeatable protocol:** Release, Windows 11 x64 build 26200, SDK 10.0.401/Desktop Runtime 10.0.12/Git 2.51.1, 24 logical processors (Intel64 family 6/model 183). CIM hardware queries were unavailable; RAM/physical disk were not characterized. Three samples per version, isolated config, fresh disposable 2,500-commit linear loose-object repo with short messages and one branch. OS/file caches were not flushed and antivirus settings were not changed. Baseline source was exported from 0.4.2 into ignored `artifacts/startup-baseline-0.4.2`; the same benchmark harness was applied there without optimizing it. Compare ranges and medians, not p95/statistical significance.

| Headless stage | 0.4.2 before | 0.5.0 after |
| --- | --- | --- |
| Settings read/parse | 0.42–9.77 ms | 0.55–5.54 ms |
| Git discovery/version | 238–268 ms | 172–318 ms |
| Repository inspection | 1,951–2,216 ms | 762–972 ms |
| Opening ref snapshot | 493–547 ms | 251–259 ms |
| First page including validation/parsing/graph | 980–1,192 ms | 773–916 ms |
| Independent graph construction | 0.08–0.21 ms | 0.11–0.65 ms |
| Row formatting | 0.27–1.16 ms | 0.45–3.20 ms |
| Discovery through first 200 formatted rows | **3.66–4.20 s (median 3.89 s)** | **2.02–2.32 s (median 2.29 s)** |
| Git/helper process launches | 14, sequential | 9, at most 2 concurrently |
| First offscreen application + window construction/layout | 448 ms | 501 ms |
| Subsequent offscreen window construction/layout | 24–26 ms | 21–25 ms |

Paired sources: `artifacts/startup-baseline-0.4.2/artifacts/history-benchmarks/20261025-021345-ceb46ef4529a4fec8ca61d4ab1a53add` and `artifacts/history-benchmarks/20261025-021511-fc7dacb7219f497b8bd8e41e03e46cd7`. The initial diagnostic baseline also ranged 4.36–4.72 s, illustrating host variability. A separate three-run no-op comparison measured direct Git startup at 97–139 ms versus helper+Git at 212–239 ms. Helper startup overhead is real; source-generated JSON alone did not produce a distinguishable no-op improvement in this sample. Keeping ownership/cancellation safety was preferred to replacing the runner without evidence.

Warm cached switches publish model state synchronously and validate identity asynchronously with zero Git launches. Final automatic-flow checks measured startup restoration/settings at 3.99 ms, first selected repository at 2.52 s, five cached switches at 0.055–0.127 ms for model publication / 7.18–14.99 ms through identity validation, and three background refreshes at 1.77–2.00 s. A warm offscreen window with populated history took 8.19 ms to construct and 62.17 ms to bind/layout. These are separate from actual desktop rendering/input. Cold boot, real app process startup-to-visible-window, real large repositories and p95 remain **unmeasured**; shell construction has no demonstrated improvement.

Final verification: `scripts/Build-Preview.ps1` restored, built and published with zero warnings/errors; **58 checks passed, 0 failed**. Fixture: `artifacts/foundation-checks/20261025-022247-c226902f231d45c9a3dd1a22ce5434f3`, including `automatic-history-timings.json`. Checks cover startup selection/scope, automatic reads, warm tabs, cache eviction, retained page/selection after refresh, failure/cancellation, cancellation-ignoring obsolete results, approval-save failure, persisted revocation, missing/replaced identities, legacy schema migration and exact author matching. Existing multi-branch/merge/tag/pagination and helper-suppression regressions remain green.

Reviewed `automatic-history.png` at 1200×760 and `automatic-history-minimum.png` at 900×580: selected text retains normal styling, same-author peers are yellow, unrelated rows white, details and menu remain usable offscreen. At minimum width the trailing columns require horizontal scrolling. `automatic-history-high-contrast.png` and color assertions verify the forced high-contrast code path, not actual Windows theme/screen-reader behavior. No interactive app launch or desktop control occurred.

A final independent benchmark in `artifacts/history-benchmarks/20261025-022634-0b501db34d58437fb70643916844955f` measured first-page paths at 2.23–2.35 s and offset-2,400 pages at 1.05–1.17 s on the same 2,500-commit fixture. First offscreen shell construction/layout was 504 ms. This modest depth does not establish large-repository scalability or eliminate `--skip` traversal cost.

Preview: `Build/0.5.0-win-x64/Shovel.exe`, framework-dependent and unsigned; earlier version folders retained. All 14 package checksums verified; published App/Core/ProcessHost assemblies match the tested assemblies, and product/file version is Shovel / 0.5.0.0. Build/artifacts stay ignored and untracked. No new dependency, installer, updater, hosting, signing or network operation was introduced. Actual scroll restoration/keyboard input, profile restart in a desktop session, Windows 10, mixed DPI/accessibility, larger packed/many-ref repositories and memory/idle budgets remain open.

**Proposed developer-fixture gates:** first 200 rows <=3 s, retained-cache binding <=100 ms and identity validation <=100 ms, background refresh <=3 s while preserving rows, first offscreen empty shell <=750 ms. These are local regression targets grounded in the small fixture, not fleet guarantees or replacements for the broader performance budgets in requirements. Deep paging/packed many-ref fixtures and actual cold/warm desktop validation are the next priorities before more Git features.

## Fast local history verification 2026-10-25

**Diagnosis and change:** the remaining startup cost was dominated by repeated process launches, including a .NET helper and Git for Windows' wrapper, plus repeated HEAD/ref observations. Graph construction and formatting were already below a few milliseconds. Version 0.6.0 uses direct, atomically job-contained native Git launches and one combined HEAD/ref observation reused by the page read. First load uses five Git calls instead of nine; refresh uses four. Post-read validation, 200 rows, All Commits scope, output/time limits and bounded tab caches remain unchanged. No preload, polling, repository maintenance or new dependency.

The user explicitly removed local-history approval questions/settings. Schema 4 migrates prior fingerprints into replacement-detection stamps; opened/restored tabs load without a consent question. Ownership checks and helper suppression remain. Missing/replaced roots stop safely; explicit reopen binds the new identity. The [read audit and identity limits](architecture.md#automatic-local-history) define this policy; it grants no permission for network access or future executable Git workflows. Tab context menus now target the clicked tab for Refresh, Move and Close/Close others; middle click closes it, including during a read.

**Repeated small-fixture measurements:** same Windows 11 host/toolchain and isolated 2,500-commit loose-object protocol as F1d; three samples per run, caches not flushed. Fresh 0.5.0 baseline: `artifacts/history-benchmarks/20261025-024208-59b7aa7069334f23817c73f9efa6883e`. Final 0.6.0: `artifacts/history-benchmarks/20261025-150553-4a92143ed38f4ba8a9a812ebe10c713a`.

| Headless stage | 0.5.0 | 0.6.0 |
| --- | --- | --- |
| Discovery through 200 formatted rows | **1.98–2.23 s; median 2.14 s** | **0.53–0.55 s; median 0.54 s** |
| Git discovery/version | 225–253 ms | 42–48 ms |
| Repository inspection | 758–904 ms | 108–123 ms, including initial refs |
| Remaining refs/page read and validation | 982–1,155 ms | 361–394 ms |
| Independent graph construction / formatting | Negligible relative to launch cost | 0.07–0.20 ms / 0.22–1.10 ms |
| Git command count | 9 | 5 |

The final median is about 75% lower. An earlier 0.6.0 run measured 0.54–0.68 s (median 0.58 s), showing host variation rather than an instant-load guarantee. Stage accounting changed: initial ref enumeration is now inside inspection; `refsMs` in the benchmark is subsequent validation. Do not double-count nested stage timings. Raw native Git startup and the contained runner both measured about 42–51 ms in the final no-op comparison (`hostedMs` retains its old JSON field name). First offscreen application/window construction/layout was 440 ms, then 24–28 ms warm; this is not actual process startup or desktop presentation time.

**Packed large fixture:** `artifacts/history-benchmarks/20261025-150610-b4fc11569bed4f3ebad70224a8e4af50` contains 100,000 linear commits in packed objects and 1,201 local/remote/tag refs. Three samples per storage variant check the first 200 rows and final page at offset 99,800, including true end-of-history. Only this disposable fixture receives maintenance commands.

| Fixture state | Discovery/inspection/first page | Final page |
| --- | --- | --- |
| Loose refs, no commit-graph file | 1.08–1.31 s | 1.01–1.12 s |
| Loose refs, commit-graph file | 0.90–1.09 s | 0.78–0.89 s |
| Packed refs and commit-graph file | 0.38–0.43 s | 0.25–0.32 s |

Many loose refs remain a material cost; the app does not rewrite them to improve a timing claim. This is a synthetic linear history, not a merge-heavy/million-commit or cold-cache certification. Deep `--skip`/topological traversal can still cost more on other repositories.

**Final verification:** clean Release restore/build/publish succeeded; preview creation passed all checks. A separately captured final run reports **61 passed, 0 failed** in `artifacts/verification-0.6.0.txt`, fixtures `artifacts/foundation-checks/20261025-150655-7ee35d2d9bef46aea8e968dfe5cf9f24`. It covers native argument/environment/exit-code fidelity, descendant cleanup, timeout/cancellation, explicit wrapper compatibility, automatic startup and settings migration, stale results, failed refresh retaining rows, replacement detection, cache bounds, author highlighting and tab menu/middle-click behavior. These include real disposable Git reads and headless routed-event/command checks, not desktop mouse input.

That run measured settings restoration at 2.54 ms, the first small selected repository at 543 ms, warm cached model publication at 0.050–0.057 ms with zero Git calls, identity validation at 4.98–5.45 ms, and background refresh at 213–244 ms. Warm populated offscreen construction/layout took 30 ms. The preceding preview check run measured 479 ms first repository and 271–312 ms refresh. Reviewed its version-0.6.0 normal/minimum history renders and tab menu in `artifacts/foundation-checks/20261025-150158-3798953ca10245468da39fb7fda581d2`; labels and details remain readable, with horizontal scrolling for trailing columns at minimum width.

Preview: `Build/0.6.0-win-x64/Shovel.exe`. All 11 checksums verified; published App/Core/ProcessHost assemblies match the tested assemblies, version is 0.6.0, and the legacy helper EXE is absent. Previous versions remain intact; Build/artifacts stay ignored and untracked. No interactive launch, desktop control, install, signing, updater or network work occurred. Actual cold/warm app startup-to-visible-rows, input responsiveness, Windows 10, custom/newer Git variants, accessibility/DPI and memory/idle budgets remain unverified.

**Proposed local regression targets:** first 200 formatted rows within 1 s on the 2,500-commit fixture; within 1.5 s on the 100k/1,201-loose-ref fixture; small-fixture refresh within 1 s while retaining rows; cached binding/identity validation each within 100 ms; first offscreen shell within 750 ms. These replace F1d's developer-fixture targets, not broader fleet acceptance budgets. Next priority is representative startup validation with launch authorization and merge-heavy performance/memory fixtures before further Git features.

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

Nonblocking details: product name Shovel is confirmed; publisher/install identity must be fixed before first distributed install; the supplied screenshot informs the headerless layout, without a broader visual-parity claim; exact update interval, cache sizes and plural library should be validated when needed. Git floor is 2.47.0 for metadata reads, tested here only on 2.51.1. English ResourceManager/.resx is implemented; permanent data/install identity remains open.

## Documentation maintenance

Record decisions/date/rationale in architecture and resolve Q entries here. Keep requirement IDs stable. Maintain TODO.md after every implementation change and update this file's milestone/evidence status when delivering a phase. Research files remain historical references; do not let them override these documents or later user instructions.
