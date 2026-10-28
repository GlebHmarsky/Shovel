# Roadmap and decision register

Updated 2026-10-28. [Product](product.md): UX; [requirements](requirements.md): acceptance; [architecture](architecture.md): implementation; [TODO](../TODO.md): only task checklist. This file holds milestone/evidence history and unresolved decisions.

## Current status

**0.15.0: committed files/diffs and local-action latency.** The selected history commit has a parent-aware Changes panel; historical and local file rows share M/+/− markers. Working-copy actions avoid unrelated history ref enumeration and expose phase timings. Further theme polish stays P2; arbitrary-line staging and side-by-side/image previews remain future work. Verification and measured limits are recorded below.

**Confirmed 2026-10-27:** deliver working features with UI; no new placeholders or permanent disabled prototypes. No repo-trust/first-use/program-consent layer for ordinary actions; normal Execute authorizes the operation. Do not add extra safety measures without concrete need. Retain normal Git data-loss handling, auth/SSH verification and destructive confirmation. Supersedes historical UI-first/prototype-retention/trust rules. 0.9.2 recorded the policy; 0.9.3 applies it in code. The user also explicitly reconfirmed removing the gate for configured hooks/helpers during this delivery.

No historical phase order or incomplete unrelated desktop matrix may veto a newly requested feature. Requirements for the feature's own correctness still apply. Startup updater checks supersede old research advice; no startup fetch.

## Delivery sequence

Proposed remaining order; requests can reprioritize. Each increment must work end to end with stated evidence. Commits/versioned local previews authorized; launch/live accounts/deployment remain separate.

| Phase | Delivered or intended scope | Evidence/dependency |
| --- | --- | --- |
| F0 / 0.1.0 | C#/WPF shell/settings/resources/diagnostics/lifetime | 17 checks + Windows 11 smoke; historical foundation, no Git |
| F1a / 0.2.0 | Real metadata/processes/tabs/identity/markers | 35 checks + limited published-window inspection; old session trust superseded |
| F1b / 0.3.0 | HEAD ancestry/graph/details/bounded older loading | 45 checks/offscreen review |
| 0.3.1 | Shovel rename/compatible profile; requested old-preview cleanup | 45 checks/package metadata |
| F1c / 0.4.0 | All Commits/refs/200-row older-newer pages/no total cap | 49 checks/multiref/paging/offscreen |
| 0.4.1 / 0.4.2 | Headerless history / selected icon | 49 checks each/offscreen or extracted icon |
| F1d / 0.5.0 | Automatic history/bounded cache/author relation | 58 checks/timings; old remembered consent superseded |
| F1e / 0.6.0 | Direct contained native Git/no consent/tab menus/MMB | 61 checks/packed 100k benchmark |
| F1f / 0.7.0 | Fixed All Commits below tabs/local upstream counts/frame | 65 checks/offscreen/timings; disabled prototypes now debt |
| F2a / 0.8.0 | Real branch create/switch/Fetch/FF Pull/Push/tags/upstream/badges | 77 checks/local bare remotes/offscreen |
| F2b / 0.9.1 | Packaged Git/OpenSSH askpass/owned prompts/cancel/reinspection | 87 checks/local credential protocol/offscreen; live providers unverified |
| 0.9.2 | Compact docs/working-feature and normal-consent policy | Documentation-only; verification below |
| 0.9.3 | Normal action execution/no routine consent; implemented controls only | Local Git workflow and headless verification below |
| F2c / 0.10.0 | Shell-owned Pull/action execution after options close | Real incoming-commit/error/cancellation and offscreen verification below |
| F2d / 0.11.0 | Immediate options shell; default rebase/merge/auto-stash Pull | Disposable real-Git integration and headless timing/layout evidence below |
| 0.12.1 | Requested first dark-theme pass/shared state conventions | 119 checks/offscreen review; further polish P2 |
| 0.13.0 | Modal Local Changes/Commit; real file diffs, file/all stage/unstage, commit/amend/Shift push | Disposable Git, modifier/failure/cancellation and offscreen evidence below |
| 0.14.0 | Selected whole-hunk stage/unstage for tracked text modifications | Exact-preview validation, disposable Git and bound UI/offscreen evidence below |
| 0.15.0 | Selected-commit files/diffs, file markers, narrower local-action metadata | Immutable tree comparisons and repeatable before/after measurements below |
| F1g onward | Historical file tree; filtering when requested | Bounded content and relevant read tests |
| 2: Local MVP remainder | Init/branch delete/tracking/stashes | Signing/recovery coverage |
| 3: Network MVP | Clone/remotes/broader network/recovery continue/skip/abort | Visible auth/input, failures/external races |
| 3R: Pilot distribution | Install once/update/channels/pilot/migration/repair | Updater/IT/signing decisions and installed U/D tests before ~30-user rollout |
| 4: Advanced | Merge/rebase/interactive rebase/cherry-pick/revert/reset/tags/reflog/force-with-lease/conflict editor | Relevant sequence/destructive/recovery tests |
| 5: Extended | Submodule/worktree/LFS/shallow/partial/sparse/tray/large-repo work | Compatibility and lifecycle/performance evidence |

MVP = F0 + phases 1–3, plus 3R for supported rollout. Foundation/prototype packages alone never meant complete Git functionality. Localization/accessibility/correctness accompany features.

## Verification scope

Historical baseline host unless noted: **Windows 11 x64 build 26200; .NET SDK 10.0.401, Desktop Runtime/pack 10.0.12; Git for Windows 2.51.1** (Git from F1a onward). Restore/Release build/publish passed without warnings/errors for each packaged delivery. No dependency installation/remote push. Git mutations use disposable repos/local bare remotes and isolated config. Artifacts/Build ignored; package checksum/assembly comparisons do not establish signing.

Actual GUI: F0 smoke and limited 0.2.0 window inspection only. From 0.3.0 onward, views were rendered offscreen without Show/desktop input; build/headless results do not certify desktop behavior. Shared limits: Windows 10/additional architectures, live providers/signing, keyboard/focus/screen reader/mixed DPI/RTL/themes, crash/sleep/disk-full, cold/p95/memory/idle budgets and installed updates remain unverified unless explicitly measured below.

Original research attachment unavailable; official old Fork screenshots/animations inspected, X Media blocked by login/403. User screenshot 2026-10-25 informed headerless history only. Research is not runtime parity evidence.

## Foundation verification 2026-10-23

17 checks: settings round-trip/backup/invalid/older/newer schema/input/geometry/locks/cancel/external edits, diagnostic privacy/rotation/failure, headless resource/binding/accessibility names. Existing docs preserved, Git/ignore initialized, foundation commit `fff8f12` after authorization.

Actual GUI at 1200×800 and 900×620 DIP: frame/disabled historical controls/details, sidebar click/arrows, details tabs, both splitters, exit 0. Relaunch restored sidebar 256/details 300 DIP/section. Fixed disabled-foreground override, record ToString accessible names and tall-details minimum clipping; rebuilt/rechecked/reviewed.

Pre-correction idle sample: 900×620, 24 logical CPUs; private 138.77 MiB, working set 154.43 MiB, 0.0 CPU seconds over 30.01 s. Single empty-shell sample, not five-minute/p95/repo-load pass. Saved layout/event logs confirm normal save; not update recovery proof. Fatal crash/disk-full/plural/dark theme not verified.

## Read-only preview verification 2026-10-23

0.2.0: **35 passed/0 failed**. Added argument fidelity, both streams/caps/closed stdin, exit/start errors, timeout/cancel/descendants/inherited pipes/queued cancel/consumer failure, migration/tab order, missing/older Git, unborn/detached/linked/bare/nonrepo/conflicted merge/helper suppression/external branch changes. Model checks covered then-current consent, shutdown and obsolete results. Fixed duplicate helper EXE publish item; failed staging excluded.

Published 1200×800 window with isolated profile showed version/two Unicode tabs/details/old trust state; no automatic restore reads then. User pressed Escape to stop desktop control; no further input. Manual open/trust/refresh/cancel/tab/relaunch never verified. Those old consent expectations are superseded.

## History preview verification 2026-10-23

0.3.0: **45 passed/0 failed**, including final parent-count guard. Linear/merge/offset-date/multiline/2,005-commit fixtures match CLI; pinned lookahead/caps/ref movement/shallow boundaries; configured signed fixture proves no verifier helper. Malformed fields/variable-width hashes; merge/octopus/convergence/page edges, cycle/overwide rejection; bounded row previews preserve source text.

Headless real-history binding at 1200×760/900×580: graph/columns/details; 400 loaded rows realize <60. Fixed background/inactive selection/narrow clipping; reject stale history. All 14 hashes/published App/Core/ProcessHost match; overwrite guard passed. Only HEAD then, schema 2, 2,000 cap; not scalability proof.

## Shovel preview verification 2026-10-24

0.3.1: requested old 0.2.0/0.3.0 Build removal and Release clean; preserved settings/source/ignored evidence. Rename only; **45 checks**, 14 hashes/tested assemblies, Shovel product/file version 0.3.1.0. Historical removed package paths below are evidence, not retained previews.

## All Commits verification 2026-10-24

0.4.0: **49 checks**, 14 hashes/assembly and metadata match. Old HEAD-only/2,000 cap caused missing commits, no active filter. Added All Commits/current-HEAD scope (selector later removed), refs/paging/details. Minimum-layout crowding caught/fixed before delivery.

Real fixtures: all 2,005 rows reachable in CLI order/no duplicates/continuous parent edges; branches/merges/remote-only/tag-only/lightweight/annotated/nested/noncommit/packed refs; then-current reflog/stash/detached exclusions, explicit detached/unborn/shallow and external ref change cases. 0.7.0 later includes detached HEAD in fixed scope. Headless details include >100,000-character message; limits/cancel/timeout/ref changes retain rows. Reviewed 1200×760/900×580; deep traversal still bounded, not large-repo certification.

## Headerless history refinement 2026-10-25

0.4.1: visible headings removed per supplied screenshot, Header resources retained for cell help, no drag resizing, adaptable description. **49 checks**, normal/minimum offscreen review; no Git change.

## Application icon verification 2026-10-25

0.4.2: selected PNG/nine-size ICO embedded in EXE/window. Initial test caught unqualified resource URI resolving to test assembly; qualified URI fixed it. **49 checks**, 14 hashes/tested assemblies, extracted small icon inspected, frame sizes/transparency checked. Explorer/taskbar cache/DPI remains unverified.

## Automatic history performance and verification 2026-10-25

0.5.0: **58 checks**, 14 hashes/tested assemblies/product 0.5.0.0. Reported 10 s not reproduced. 0.4.2 restored tabs never read, tab switch discarded rows, session approval recurred; first page launched 14 sequential helper/Git processes (eight metadata/four ref enumerations). Already only 201 log rows; topo-order still traverses ancestry. Graph/formatting negligible.

Changes: combined locations, two bounded parallel HEAD-query pairs, reused initial refs, source-generated protocol JSON, selected-tab chain/cache. Three pages/32 MiB estimate/128 back cursors, no preload/polling/service/dependency. Old remembered approval now superseded by 0.6.0.

Protocol: 24 logical processors, Intel64 family 6/model 183; CIM RAM/disk unavailable. Three samples/version, fresh isolated 2,500-commit linear loose-object/one-branch/short-message fixture. Caches not flushed, antivirus unchanged; exported 0.4.2 with same harness. Ranges/medians only, no p95/significance.

| Headless stage | 0.4.2 | 0.5.0 |
| --- | --- | --- |
| Settings | 0.42–9.77 ms | 0.55–5.54 ms |
| Git discovery | 238–268 ms | 172–318 ms |
| Inspection | 1,951–2,216 ms | 762–972 ms |
| Opening refs | 493–547 ms | 251–259 ms |
| Page/validation/parser/graph | 980–1,192 ms | 773–916 ms |
| Independent graph | 0.08–0.21 ms | 0.11–0.65 ms |
| Formatting | 0.27–1.16 ms | 0.45–3.20 ms |
| Discovery → 200 rows | 3.66–4.20 s; median 3.89 | 2.02–2.32 s; median 2.29 |
| Launches | 14 sequential | 9, <=2 concurrent |
| First offscreen shell | 448 ms | 501 ms |
| Warm offscreen shell | 24–26 ms | 21–25 ms |

Initial baseline 4.36–4.72 s illustrates host variation. No-op direct Git 97–139 ms vs helper+Git 212–239 ms; JSON alone no distinguishable improvement, ownership/cancel retained.

Final automatic-flow: settings 3.99 ms, first repo 2.52 s; five cache model publications 0.055–0.127 ms/identity 7.18–14.99 ms, zero Git; three refreshes 1.77–2.00 s. Populated warm construction 8.19 ms/layout 62.17 ms. Independent run: first page 2.23–2.35 s, offset 2,400 1.05–1.17 s, first shell 504 ms. Cold process-to-visible/large repos/p95 unmeasured; no shell improvement demonstrated.

Checks: startup/cache/eviction/page-selection retention/failure/cancel/obsolete results, then-current approval-save/revocation, missing/replaced identity/migration/exact author. Multi-ref/tag/graph/helper checks retained. Normal/minimum renders: yellow peers/white unrelated/normal selection, narrow trailing columns horizontal scroll. Forced high-contrast PNG/assertions only, not OS pass.

Historical proposed small-fixture targets: rows <=3 s, cache binding/identity each <=100 ms, refresh <=3 s, first offscreen shell <=750 ms; replaced by F1e targets below.

## Fast local history verification 2026-10-25

0.6.0: **61 checks**, 11 hashes/tested assemblies/version, obsolete helper EXE absent. Native direct atomic Job launch/native Git/combined HEAD-refs reduces first page nine → five calls, refresh four. Same All Commits/200 rows/validation/limits/cache, no repository maintenance. Schema 4 removes approval; stamps detect replacement, explicit reopen; tab menus/MMB target correct tab.

Same three-sample 2,500 fixture/protocol; accounting changed: refs now inside inspection, refsMs later validation; hostedMs JSON retains old name.

| Headless stage | 0.5.0 baseline | 0.6.0 final |
| --- | --- | --- |
| Discovery → 200 rows | 1.98–2.23 s; median 2.14 | 0.53–0.55 s; median 0.54 |
| Discovery | 225–253 ms | 42–48 ms |
| Inspection | 758–904 ms | 108–123 ms incl. refs |
| Remaining refs/page | 982–1,155 ms | 361–394 ms |
| Independent graph/format | Negligible | 0.07–0.20 / 0.22–1.10 ms |
| Calls | 9 | 5 |

Median ~75% lower; earlier 0.6.0 0.54–0.68 s/median 0.58. Native no-op/contained runner both 42–51 ms. First offscreen shell 440 ms, warm 24–28 ms; not desktop startup.

100,000 packed linear commits, 1,201 refs; three samples/variant, final offset 99,800/end verified. Only fixture receives maintenance:

| Fixture | Discovery/inspection/first | Final page |
| --- | --- | --- |
| Loose refs/no commit graph | 1.08–1.31 s | 1.01–1.12 s |
| Loose refs/commit graph | 0.90–1.09 s | 0.78–0.89 s |
| Packed refs/commit graph | 0.38–0.43 s | 0.25–0.32 s |

Loose refs material; no app rewrites to improve claim. Synthetic linear, not merge-heavy/million/cold-cache proof.

Checks: args/env/exit/descendants/timeouts/cancel/wrapper, auto read/migration/stale/failure/replacement/cache/author/tab events. Final flow: settings 2.54 ms, first repo 543 ms, cache 0.050–0.057 ms/zero Git, identity 4.98–5.45 ms, refresh 213–244 ms, populated warm layout 30 ms. Preceding preview run first 479 ms/refresh 271–312 ms. Normal/minimum/tab-menu renders reviewed; narrow trailing columns scroll.

Proposed local targets: 2,500 rows-first-page <=1 s; 100k/1,201 loose-ref <=1.5 s; small refresh <=1 s; cache/identity each <=100 ms; first offscreen shell <=750 ms. Developer regression targets, not fleet budgets.

## Workspace and upstream counts verification 2026-10-25

0.7.0: **65 checks**, 11 hashes/tested assemblies/version. No selector/heading; history flush below tabs; frame/center timer; then-requested prototypes restored (now implementation debt). Fixed scope includes detached HEAD/old saved narrow scopes ignored. Local upstream badges after rows, no network; actions implemented later.

Real synchronized/ahead/behind/diverged, non-origin upstream/separate push target, moved/deleted/missing/detached/unborn/shallow cases. Controlled slow/failing/cancelled/closed-tab comparisons prove rows first/no count leak; badge render synthetic data, separate real-Git semantics. Layout 1200×760/900×580, no combo/top gap, compact footer/full help/horizontal scroll; existing cache/author/virtualization retained.

2,500 fixture: 0.513–0.553 s/median 0.522, five calls vs 0.6.0 0.53–0.55; no demonstrated first-page regression. Shell 522 ms, warm 31–37; no upstream configured, large divergent comparison latency uncharacterized. Final flow: settings 2.99 ms, repo 436 ms, cache 0.039–0.057 ms/zero Git, identity 4.45–5.56, refresh 217–240, populated layout 35 ms. Synthetic/no cache flush, not desktop/p95.

## Branch and network actions verification (2026-10-25)

0.8.0: **77 checks**, 11 hashes/tested App/Core/ProcessHost/version. Actual disposable branches/Unicode/invalid/duplicate names, fetch/push/tags/upstream, FF pull, rejected divergent pull/non-FF push. Merge options rejected pre-fetch. Dirty/untracked/ignored collisions, sequences/ref/config/identity changes, failed hooks/remotes/linked exclusion; waiting hook cancelled/gate released. Auth failure controlled result, not sign-in.

Headless modal admission/then-current approval/branch targets/one attempt/reinspection/accessibility/badges plus history regressions. No added startup query/performance claim. Reviewed 1200×760/900×580 workspace and 650×520/520×440 dialogs; fixed background, buttons outside options scroll, current badge priority/full help.

Restricted sandbox prevented MSYS named-object creation; suite rerun with permission. No working repo/live remote used. Pull FF/clean only; rebase/autostash/force, sparse/submodule mutation/custom SSH/broader recovery pending. Partial operations/cancellation are not rollback.

## Authentication bridge verification (2026-10-26)

0.9.1: **87 checks**, 16 hashes; seven tested/published App/Core/ProcessHost/helper binary/runtime files match, app/helper 0.9.1.0. Internal 0.9.0 candidate (86 checks) retained; final patch moves helper file/pipe setup off dispatcher. History five-call path unchanged, no speed claim.

Real credential fill in disposable pre-push hook/local bare remote, isolated config, Unicode/spaced helper path, no server/store. Prompt cancellation stops owned Git/hook, inspection usable. Checks: missing helper, Unicode/cleared reply, wrong nonce/server/oversized/multiline/timeout/16 limit/stale endpoints/informational exit; worker setup/wait state/cancel/reinspection and prior regressions.

Final 620×440/480×340 masked/confirmation/notification offscreen prompts reviewed; long text scrolls, input/buttons outside scroll. No focus/keyboard/live HTTPS/SSH/agent/GCM/signing certification; browser/interactive hooks still pending. No shared agent/browser killed/global config edit.

## Documentation verification (2026-10-27)

0.9.2 compacts all ten maintained Markdown files by about 46% in word count. All 34 requirement IDs, 41 local links/anchors, 68 external references and 40 historical evidence paths retained/checked. Generated ignored Build/artifacts snapshots remain historical outputs. Application behavior unchanged; separate checkbox/prototypes still need code work.

Restore/Release build/publish passed with zero warnings/errors; **87 checks passed, 0 failed**. Restricted run stopped at local push/clone fixtures; permitted rerun passed. Log: `artifacts/preview-0.9.2-verification.txt`; fixture: `artifacts/foundation-checks/20261027-161840-12950ac5a7ed4e9b8e473067430e2188`. Preview: `Build/0.9.2-win-x64/Shovel.exe`; 16 checksums/seven tested-published files verified, app/helper file versions 0.9.2.0. No interactive launch/live authentication; existing runtime verification limits remain. Intended docs/version committed; no push.

## Interaction-policy verification (2026-10-27)

0.9.3 removes the configured-program approval flag/checkbox and unfinished toolbar/sidebar/dialog controls. Actual targets/state determine availability, with localized tooltip/accessibility help. Automatic reads, Git protections, hooks, askpass and shared-worktree/cancel/reinspection boundaries remain. No new startup queries or performance claim.

Restore/Release build/publish passed with zero warnings/errors; **89 passed, 0 failed**. Added real dialog-command create/checkout → push/set-upstream → fetch → pull → switch against disposable local repositories, plus missing-target help checks. Existing failed hooks, ignored-file collisions, divergence, stale context, authentication/cancellation, modal admission and automatic-history regressions pass. No live remotes used.

Reviewed offscreen workspace at 1200×760/900×580 and action dialogs at 650×520/520×440. Primary actions remain outside the scrolling options; history keeps horizontal scrolling at minimum width. This is not desktop focus/input, live-provider, Windows 10, OS accessibility or performance certification.

Log: `artifacts/preview-0.9.3-verification.txt`; fixtures/renders: `artifacts/foundation-checks/20261027-164923-14c646bd549d4cab9e9489fb853efbcf`. Preview: `Build/0.9.3-win-x64/Shovel.exe`; all 16 checksums and seven tested/published app/helper files match, both executable file versions 0.9.3.0. Build remains ignored/untracked; earlier previews retained. No interactive launch, push, dependency installation or distribution work. Next: remaining authentication routes, then Local Changes/status per TODO.

## Workspace Pull verification (2026-10-27)

0.10.0 fixes the reported interaction gap: 0.9.3 already invoked real fetch/fast-forward commands, but execution/results stayed inside the modal and the shell only reported an open dialog. The user's particular remote failure was not reproduced or diagnosed. Submitting now closes options and transfers lifetime/progress/results to the shell; required authentication prompts use MainWindow. No Git strategy or authentication restriction was silently relaxed.

Restore/Release build/publish passed with zero warnings/errors; **93 passed, 0 failed**. New real local bare-remote/peer fixture invokes the bound Pull command, verifies immediate options closure, actual incoming commit/file, refreshed history/behind count and retained output. A controlled pre-fetch pause exposes running-state layout and duplicate-action/navigation guards; it is not a performance measurement. Real dirty/unavailable-remote failures, cancellation, cancelled options retaining the previous result, other-tab isolation, failed reinspection and askpass after options closure are covered alongside existing regressions.

Reviewed final offscreen running/completed workspace at 1200×760/900×580 and retained the normal/minimum action-dialog checks. Log: `artifacts/preview-0.10.0-verification.txt`; fixtures/renders: `artifacts/foundation-checks/20261027-211310-1127f0a6756f4f7d9dc644e81c8b3b9d`. Initial run also exited successfully with 93 checks after delayed completion; a persistent runner hang was not established. Preview: `Build/0.10.0-win-x64/Shovel.exe`; 16 checksums/seven tested-published files match, both executable versions 0.10.0.0. Build stays ignored/untracked; earlier versions retained.

No interactive launch, desktop input/focus, live HTTPS/SSH/GCM, Windows 10 or new performance certification. Pull still requires a clean worktree and fast-forward; no merge/rebase/autostash. No dependency installation, push or distribution work. Next implementation: Local Changes/status, before broader authentication expansion.

## Selected hunks (2026-10-28)

0.14.0 adds complete-block stage/unstage from the existing diff, including multiple disjoint hunks and partially staged files. One localized action changes with the selected side/count; draft/path remain, block selection resets after reinspection. [Architecture](architecture.md#selected-hunks-0140) owns supported content and external-client race limits.

Focused real-Git checks cover chosen/reversed blocks and offset changes, preserved worktree/unrelated index content, invalid selection without fallback, same-size/mtime and index/HEAD/config/attribute changes, suppressed filters, custom diff/apply config, CRLF and EOF markers, unsupported types/options, existing locks, rejected validation and cancellation. Bound WPF commands exercise multi-block selection, draft/path retention and stale-action recovery. Reviewed offscreen 1200×760 and 900×580 client areas; controls remain available at minimum size. No interactive launch, live authentication/network, OS accessibility/DPI or desktop timing claim.

Restore/Release build/publish passed with zero warnings/errors; **140 passed, 0 failed**, including 11 new hunk checks and an external edit injected during patch validation before the cached write. Existing history, local/network actions, hooks/authentication, cancellation, modal responsiveness and virtualization regressions passed. The first restricted run failed in an existing local Push fixture; direct reproduction showed MSYS `NtCreateDirectoryObject` access denied (0xC0000022). That run is retained as failed evidence; the authorized rerun allowed the required local test subprocesses.

Log: `artifacts/preview-0.14.0-verification.txt`; final fixtures/renders: `artifacts/foundation-checks/20261028-120923-e2eddc4a61934d6fbac14e458e788980`. Preview: `Build/0.14.0-win-x64/Shovel.exe`; 16 valid checksums/seven matching tested-published files, app/helper versions 0.14.0.0. Build remains ignored/untracked; earlier previews preserved. No dependency install, interactive launch, live remote or deployment. The next independent functional item is selected-commit files/diffs; conditional authentication and desktop validation priorities remain in TODO.

## Modal commit workspace (2026-10-28)

0.13.0 replaces the embedded read-only workspace with the user-requested modal layout, real file diffs and file/all stage/unstage/commit/amend/Shift Commit and Push. [Product](product.md#local-changes--commit-0130) owns interaction; [architecture](architecture.md#local-changes-and-commit-0130) owns the new execution/read boundaries. History remains mounted; one selected-tab count after history explicitly supersedes the older no-startup-status policy. No dependency or settings schema change.

Restore/Release build/publish succeeded with zero warnings/errors; **129 checks passed**. New disposable fixtures cover unborn partially staged index removal, Unicode/dash paths, independent HEAD/index/worktree diffs, binary byte sizes, rename/delete/whitespace/oversized text, suppressed read-time helpers versus real staging filters, stale index/locks, Shift actions, failed hooks/draft retention, actual-tip amend, successful/local rejected push, cancelled commit hooks and resolved merge commit. The successful commit survives failed Push. Prior status/parser, history, branch/Pull and askpass coverage remains; old embedded-view assertions were replaced with modal/cache/lifetime assertions.

Headless UI checks verify no construction-time Git calls, cached rows/draft/history, failed/cancelled refresh retaining rows and removing write readiness, late diff rejection, and virtualization for 1,500+ files/3,000+ diff rows. Normal 1200×760 and minimum 900×580 client-area renders reviewed; colors/line numbers/buttons/editor remain visible. Shared diff contrast/system-brush mapping checked. The tests exposed and fixed synchronous reentrant close and repeated layout-triggered diff reads. Cached modal construction plus offscreen layout: **16.70 ms**, one sample with Git excluded; not cold startup, desktop responsiveness or p95 evidence.

Evidence: `artifacts/preview-0.13.0-verification.txt`; fixtures/renders/timing: `artifacts/foundation-checks/20261028-031130-a7647ae17615417199ede7e8e0454bff`. Preview: `Build/0.13.0-win-x64/Shovel.exe`; 16 checksum entries and seven tested/published app/helper files match; both executable versions 0.13.0.0. Build/artifacts ignored, earlier previews preserved. No interactive launch, live authentication/remote, install or deployment. Keyboard modifier/focus/OS high contrast/DPI/live signing remain unverified. Unified UTF-8 text/binary sizes ship; hunk staging, image/side-by-side previews, persistent drafts and conflict editor remain deferred. Next functional priority: selected-hunk stage/unstage with stale-patch verification.

## Dark theme first pass (2026-10-28)

0.12.1 applies the supplied screenshot's surface hierarchy to the client area: dark chrome/inactive tabs, charcoal history, lighter sidebar/details, orange lower-tab underline. [Theme](theme.md) owns the selected approximate palette/state rules and implementation boundaries. Existing Git workflows/loading/virtualization remain; no dependency or settings migration. Native OS title bars/pickers/message boxes remain system-owned; light mode/theme selection and further polish stay P2.

Final restore/Release build/publish passed with zero warnings/errors; **119 passed, 0 failed**. Added contrast checks (normal/secondary/selection/accent >=4.5:1 on intended surfaces), live high-contrast resource mapping/restoration without changing OS settings, disabled/selected states and editable ComboBox value propagation. Existing real disposable-Git, cached automatic history, Local Changes, author relationship/virtualization and operation-dialog checks passed. Offscreen review caught and fixed selected-tab accent leaking into content and a native light menu gutter; a disabled selected tab also overrides its header accent. These are final-source results, not the earlier interrupted verification runs.

Reviewed history and Local Changes at 1200×760 and 900×580, Pull at 650×520 and 520×440, authentication at minimum size, plus menu/control renders. Details retain normal text; primary dialog actions remain outside scrolling content. The headless harness never shows windows. This does not verify desktop pointer hover, keyboard focus, popup positioning, Windows 10, actual OS high contrast/screen readers, mixed DPI/RTL or performance budgets. No interactive launch/control, live network/auth, installation or system setting change was performed.

Log: `artifacts/preview-0.12.1-verification.txt`; final fixtures/renders: `artifacts/foundation-checks/20261028-015204-cb3a4ac761094f1eab596f21aee4ca15`. Preview: `Build/0.12.1-win-x64/Shovel.exe`; 16 checksums and seven tested/published app/helper files match; both executable versions 0.12.1.0. Preview ignored/untracked; earlier versions preserved. Next functional task: selected-file working-tree/index diffs, then staging/unstaging.

## Local Changes (2026-10-28)

Implemented porcelain-v2/NUL status, separate index/worktree rows, conflicts, rename origins and optional ignored paths. The sidebar switches before scanning; three nonempty tab caches, stale/error retention, cancellation/generation rejection and virtualization keep scans away from UI interaction. No history preload delay, constant polling, mutation or content-diff controls. Configured clean/process filters and fsmonitor are suppressed for read safety; disclose filter-sensitive differences and pointer-only submodules.

Final restore/Release build/publish passed with zero warnings/errors; **117 passed, 0 failed**. New checks cover unborn/clean/partially staged/renamed/deleted/Unicode/binary/untracked/ignored/conflicted data, unchanged index bytes, filter/fsmonitor suppression, malformed/invalid-UTF-8/truncated/oversized/cancelled parser input, view-before-scan, warm cache/selection/history retention, failed/cancelled refreshes, external edits and stale-tab rejection. A 1,500-file fixture remains complete with fewer than 80 realized grid rows. Two old tests now explicitly select the history grid instead of assuming one grid in the window; their original history/author assertions pass.

Final headless measurement: view-switch handler **3.46 ms**, warm **0.50 ms**; 1,500-file refresh including metadata **612.86 ms**. Earlier runs: switches 1.96–2.05 ms / warm 0.17–0.57 ms, refresh 431–505 ms. These are small disposable-fixture observations, excluding visible click-to-paint and reboot-cold startup; no p95/desktop guarantee. Reviewed normal 1200×760 and minimum 900×580 offscreen layouts, including final refreshed selected-file details and status notices.

Evidence: `artifacts/preview-0.12.0-verification.txt`; final fixtures/renders/timings: `artifacts/foundation-checks/20261028-011810-5712e20e270b46468c7c3c328433187f`. Preview: `Build/0.12.0-win-x64/Shovel.exe`; 16 valid checksums, seven matching tested/published files, app/helper versions 0.12.0.0. Build remains ignored/untracked and earlier previews retained. No interactive launch, live remotes, dependency installation or deployment. Desktop keyboard/focus/accessibility/DPI and representative large-worktree performance remain unverified. Next: working-tree/index file diffs, then staging/unstaging.

## Cached operation dialogs (2026-10-28)

0.11.0 displayed a dialog quickly but left options waiting for eight local Git queries (no network check), measured at 818–1,073 ms in its final run. 0.11.1 removes that opening dependency entirely across Branch/create/switch, Fetch, Pull and Push. Opening/dismissing now uses the existing tab snapshot only. Local remote names are cached by one bounded query alongside ref inspection, without URL resolution, network, polling or other-tab preloading. Names include configured unfetched remotes; explicit Fetch discovers branches.

All shared options windows are fixed-size/Close-only, retaining scrollable options and accessible primary buttons. Fresh preparation and stale identity/HEAD/ref validation happen after submission with shell progress, cancellation, retained errors and reinspection. Named remotes resolve against current Git configuration at execution; the dialog explicitly describes that policy. Pull itself still fetches before integration. This does not remove command execution time or bypass data-loss protections.

Verification: restore/Release build/publish passed with zero warnings/errors; **109 passed, 0 failed**. Checks prove zero opening/dismissal Git calls for all five actions, immediate local targets including unfetched remotes, stale refs rejected before fetch, cancellable post-submit validation, retained preparation failures and real Pull through the bound dialog command. Existing rebase/merge/auto-stash, conflict recovery-state, hooks/authentication, history/cache and cancellation regressions pass. Reviewed offscreen cached Pull plus normal/reduced-size operation content; shared `ResizeMode.NoResize` is asserted. Actual desktop title-bar/input/paint and live authentication remain unverified.

Final seven-sample 2,500-commit/one-remote headless benchmark: cached editable dialog first construction **570 ms**, warm **8–31 ms** (median **11 ms**), **zero Git calls**. Required preparation still costs **767–953 ms**, now exclusively after submission. This removes the prior options-readiness dependency, not all first-use WPF/JIT cost. No universal instant/p95/cold-desktop claim. Three history samples with unchanged scope/page size measured inspection **273–349 ms**, page component **393–403 ms**; the added local remote-name query overlaps refs, but no paired startup-regression claim is made. Timing files: `artifacts/action-benchmarks/20261028-003248-ce59dcae48a3401e99de604d5718b09b/action-timings.json` and `artifacts/history-benchmarks/20261028-003259-1f712b87956d4d78bae3981eba0d8b29/history-timings.json`.

Log: `artifacts/preview-0.11.1-verification.txt`; fixtures/renders: `artifacts/foundation-checks/20261028-002717-041acb271b844328aa5261a2a876e96f`. Preview: `Build/0.11.1-win-x64/Shovel.exe`, 16 valid checksums/seven tested-published file matches; app/helper versions 0.11.1.0. Build ignored/untracked, older versions retained. Ordinary UI responsiveness is now a confirmed requirement in product/G13 and AGENTS; unavoidable new waits must be explained/discussed. No interactive launch, desktop control, live remote, dependency install or deployment. Next: Local Changes/status.

## Prompt dialogs and Pull strategies (2026-10-28)

Diagnosis: 0.10.0 awaited nine sequential Git subprocesses before constructing the options window, including a duplicated ref enumeration. Pull separately rejected divergence, any untracked/dirty state and repositories with submodule/sparse metadata. These restrictions explain reproducible failures; the user's particular remote/authentication error has not been observed.

0.11.0 opens the loading/options shell before preparation, reuses inspected refs and overlaps the two URL reads within the existing concurrency bound (eight subprocesses for one remote). Preparation errors stay in place; dismissal cancels/drains and rejects late data. Pull defaults to Rebase with Merge/optional tracked auto-stash; native Git handles dirty/conflicting state, with additional ignored-collision protection. Conflicted sequences/stash restoration retain recovery state and terminal guidance. Basic sparse pulls and superproject pointer updates no longer hit a blanket unsupported gate; submodule worktrees remain unchanged.

Seven-sample headless benchmark, isolated 2,500 commits/one local remote, Git 2.51.1 and existing .NET 10 SDK/runtime. Original preparation: 498–706 ms (median 626), nine launches; first preparation + layout 1,179 ms, later 509–733 ms. First revised run: preparation 357–414 ms (median 391), first loading layout 314 ms, warm 11–27 ms. A later repeat varied to preparation 799–1,063 ms and first layout 800 ms, warm 12–32 ms. Final default-radio regression benchmark: first loading layout 511 ms, warm 17–35 ms, background preparation 818–1,073 ms. This variation prevents a universal preparation-speed claim; the removed blocking dependency and launch count are verified. No run measured desktop click-to-paint, reboot-cold startup, a real server or p95. [Requirements](requirements.md#proposed-performance-budgets-not-measurements) owns proposed budgets; verify on representative Windows machines before adopting guarantees.

Timing evidence: `artifacts/action-benchmarks-before-0.11.0/20261027-221550-a974991cb4b445c8ba5d992b80e564fc/action-timings.json`; revised `artifacts/action-benchmarks/20261027-222456-8e75324eb7284a8ab4647d2f605737ea/action-timings.json` and `artifacts/action-benchmarks/20261027-223943-90ed839f51a6460fbbdcd117ef2476af/action-timings.json`.

Final dialog/timing images: `artifacts/action-benchmarks/20261027-224531-c84e0cad648d4fe1932f56160e26b2fa`. Offscreen review caught and fixed radio initialization changing the default to Merge; the final view/model tests verify Rebase is visibly selected and both choices switch the actual strategy.

Verification uncovered a pre-existing harness defect: constructing the production WPF App queues its Startup even without Run/Show, potentially entering normal profile/window startup. The final harness uses a plain test Application/shared styles, asserts suppressed production startup/no visible windows and bounds its dispatcher lifetime. Earlier aborted/failed attempts are not acceptance evidence. No deliberate desktop interaction or live-provider test was performed; desktop behavior remains unverified.

Final restore/Release build/publish passed with zero warnings/errors; **106 passed, 0 failed**. Disposable Git checks cover real divergent file integration for both strategies, staged/unstaged auto-stash restoration, zero-exit stash conflicts, sequence reopen/terminal abort, unrelated untracked files/ignored collisions, sparse pattern preservation and dirty submodule contents. Existing hook/authentication/cancellation/stale-state/admission/history checks also pass. WPF checks cover opening before preparation, dismissal/late completion, errors, radio bindings and normal/minimum layouts. Reviewed final Pull 650×520/520×440 and running/completed workspace 1200×760/900×580 offscreen; options scroll at minimum size, primary buttons stay outside the scroll region. This does not establish desktop focus/input, live HTTPS/SSH/GCM, Windows 10 or OS accessibility behavior.

Log: `artifacts/preview-0.11.0-verification.txt`; final fixtures/renders: `artifacts/foundation-checks/20261027-224618-6b0649aa2e654eba924bacf3245cc953`. Preview: `Build/0.11.0-win-x64/Shovel.exe`; all 16 checksums and seven tested/published app/helper files match, both executable versions 0.11.0.0. Build is ignored/untracked and earlier previews retained. No dependencies installed, push or distribution work. Source/docs committed as requested. Next: Local Changes/status, then in-app recovery actions; broader auth is not a prerequisite for independent features.

## Committed files and local-action latency 2026-10-28

0.15.0 adds real historical file lists and selected text/binary diffs, initial/merge-parent comparisons and shared semantic M/+/− markers. It reads stored objects; dirty working files/index are excluded. A shallow boundary cannot pretend to be an initial commit. Submodule changes stay pointer-only even with configured expanded/ignored diffs. Limits and ownership are in [architecture](architecture.md#committed-files-and-local-action-latency-0150).

The measured bottleneck was repeated **full ref enumeration** during working-copy preflight and again during reconciliation, plus independent serialized reads. Baseline `for-each-ref` calls took 534–1,106 ms on the fixture; the actual unstage command took 76–85 ms. Direct HEAD/branch/upstream reads replace those enumerations; status/config/message and preflight metadata/config overlap within the existing two-process limit. Index/HEAD/config/identity checks and real-result reconciliation remain. More narrow processes can be faster than fewer expensive ones: measured totals were 18→22 for Stage and 17→21 for Unstage, including the selected diff.

Repeatable baseline 0.14.0 versus optimized implementation: same host, three samples/version, disposable **2,000 tracked files / 1,001 loose local branch refs**, one modified file, first process sample plus two warm samples. Fresh fixtures, unchanged machine configuration, no cache flush/reboot or user repositories. Read/validation work is timed separately from the final diff; no WPF/desktop input in this benchmark. Reproduce with README's `--measure-working-copy` command. Values below are medians:

| Measured interval from submission | 0.14.0 | Optimized | Improvement |
| --- | --- | --- | --- |
| Stage validation + command | 996 ms | 462 ms | 54% |
| Stage + refreshed rows | 2,214 ms | 1,113 ms | 50% |
| Stage + selected diff | 2,622 ms | 1,594 ms | 39% |
| Unstage validation + command | 953 ms | 450 ms | 53% |
| Unstage + refreshed rows | 2,672 ms | 1,261 ms | 53% |
| Unstage + selected diff | 3,151 ms | 1,661 ms | 47% |

Fresh-row ranges: Stage 2,010–2,900→992–1,514 ms; Unstage 2,120–2,677→978–1,325 ms. Timing logs: `artifacts/working-copy-before.txt`, `artifacts/working-copy-after.txt`; fixture roots `artifacts/working-copy-benchmarks/20261028-124650-068f08089455479e88ef8e2bf4909bcd` and `20261028-125956-d00aa67adc0f41b8b89fb52b5a720583`. These samples preceded only final timing-display/submodule/layout-check refinements, not another local-action algorithm change.

**Proposed regression budgets for this fixture:** median fresh rows ≤1.5 s, selected diff ≤2 s, with command time reported separately. This is not a p95/large-repository or desktop guarantee; the user's 10-second unstage was not reproduced. Index size/status scans, filesystem/security software and configured filters/hooks can still dominate other repositories. The commit window's output section now exposes wait/validation/command/refresh milliseconds in memory to distinguish those cases; diff follows separately. No speculative successful row movement was added.

Final restore/Release build/publish passed with **zero warnings/errors; 150 checks passed, 0 failed**. Disposable integration covers the new root/empty/merge-parent, text/binary/rename/delete/Unicode/EOF/large/missing-object/submodule cases, immutable content despite dirty index/worktree, helper suppression, obsolete parent results, failed refresh retention and minimal working-copy metadata for attached/detached/unborn HEAD. Existing file/hunk/commit/network failures, locks, cancellation and conflict cases still pass. Marker contrast and system-color mapping are checked headlessly; historical accessibility text does not mislabel files as staged.

Reviewed final offscreen main window at 1200×760/900×580 and Changes pane at 950×360/650×260, plus the shared-marker commit modal. Review caught and fixed minimum-size details overflowing the footer; a bound-layout assertion now covers it. Changes expands its details area and leaving it restores the earlier height. Final log: `artifacts/preview-0.15.0-release.txt`; fixtures/renders: `artifacts/foundation-checks/20261028-133159-7e8ba5b14ddc4840ba10845d859f7ded`. Earlier interrupted runs are not release verification.

Preview: `Build/0.15.0-win-x64/Shovel.exe`; all **16 checksums**, nine published/build files and four tested assemblies match; both executable versions are 0.15.0.0. Evidence: `artifacts/preview-0.15.0-package-check.txt`. Build remains ignored/untracked and earlier previews remain. No interactive launch, live network/auth, user-repository mutation or dependency installation performed; desktop speed, keyboard/screen reader, OS high contrast and Windows 10 remain unverified. Next: in-app sequence recovery, with any remaining reported stage/unstage delay isolated using the new phase timings.

## Initial compatibility boundaries

Ordinary/linked metadata, markers and named-ref/HEAD history/shallow boundaries exist. Bare editing/WSL/network-share mutation not promised. Submodule/LFS/partial/sparse/file content not comprehensively validated; missing objects fail without fetch, no recursion/deepen/expand/filter bypass. Identity alone is not a shared-state lock. Recovery commands/advanced operations ship with their own validated functionality; binaries get summaries, hunk changes initially validated text.

## Material unresolved decisions

| ID | Decision |
| --- | --- |
| Q2 | Actual Windows editions/builds/architectures, supported runtime, App Installer, IT/proxy/no-admin constraints. Historical proposal x64 Windows 10 22H2/11; verify fleet. |
| Q3 | Private binaries/notes? Offsite without VPN? Prefer managed/static or maintained company endpoint with suitable auth. |
| Q4 | Release/signing owner, company certificate/identity/budget/key custody; compare NetSparkle/Velopack/MSIX. |

Q1 resolved 2026-10-23: C#/WPF, .NET 10 after tooling inspection. Name confirmed; publisher/install/data identity before distribution. Exact update interval/cache/plural library when needed. Git floor 2.47, tested 2.51.1. No provider/install/hosting/signing chosen.

## Evidence paths

All paths below are historical ignored outputs, relative to repository root. Earlier 0.2.0/0.3.0 Build packages were deleted on request for 0.3.1; subsequent previews preserved. These pointers retain reproduction detail without duplicating implementation guidance.

- `artifacts/gui-minimum/idle-sample.json`
- `artifacts/foundation-checks/20261023-092921-9525076807c64317b3b9fb6aa716bc2f`
- `Build/0.2.0-win-x64/GitGui.App.exe`
- `artifacts/gui-v020`
- `artifacts/foundation-checks/20261023-140125-40cd3a52a608469fb58f8df98623891d`
- `Build/0.3.0-win-x64/GitGui.App.exe`
- `artifacts/preview-0.3.0-before-final-hardening`
- `artifacts/foundation-checks/20261024-185313-4fe1042215ed4fb985c8d7266b9dc1a7`
- `Build/0.3.1-win-x64/Shovel.exe`
- `artifacts/foundation-checks/20261024-202512-c208c14830e249aeb7988d350675a7f1`
- `Build/0.4.0-win-x64/Shovel.exe`
- `artifacts/preview-0.4.0-before-layout-review`
- `artifacts/foundation-checks/20261024-222743-b5427f8ca010488182df81cd412f74b3`
- `Build/0.4.1-win-x64/Shovel.exe`
- `artifacts/foundation-checks/20261025-014650-48835f984c0745538acfcc77aa4d75d6`
- `Build/0.4.2-win-x64/Shovel.exe`
- `artifacts/startup-baseline-0.4.2`
- `artifacts/startup-baseline-0.4.2/artifacts/history-benchmarks/20261025-021345-ceb46ef4529a4fec8ca61d4ab1a53add`
- `artifacts/history-benchmarks/20261025-021511-fc7dacb7219f497b8bd8e41e03e46cd7`
- `artifacts/foundation-checks/20261025-022247-c226902f231d45c9a3dd1a22ce5434f3`
- `artifacts/history-benchmarks/20261025-022634-0b501db34d58437fb70643916844955f`
- `Build/0.5.0-win-x64/Shovel.exe`
- `artifacts/history-benchmarks/20261025-024208-59b7aa7069334f23817c73f9efa6883e`
- `artifacts/history-benchmarks/20261025-150553-4a92143ed38f4ba8a9a812ebe10c713a`
- `artifacts/history-benchmarks/20261025-150610-b4fc11569bed4f3ebad70224a8e4af50`
- `artifacts/verification-0.6.0.txt`
- `artifacts/foundation-checks/20261025-150655-7ee35d2d9bef46aea8e968dfe5cf9f24`
- `artifacts/foundation-checks/20261025-150158-3798953ca10245468da39fb7fda581d2`
- `Build/0.6.0-win-x64/Shovel.exe`
- `artifacts/preview-0.7.0-verification.txt`
- `artifacts/foundation-checks/20261025-155102-b6692b85bcf04edc96b52d7d27c4c493`
- `artifacts/foundation-checks/20261025-154740-64cb62fd192a4f23a1d9e3fb97b3d452`
- `artifacts/history-benchmarks/20261025-154850-6ca5d33a091747af862cd09e3ff684ff`
- `Build/0.7.0-win-x64/Shovel.exe`
- `artifacts/preview-0.8.0-verification.txt`
- `artifacts/foundation-checks/20261025-203618-eb20b7e9232d46df85fefa0b42beb2c2`
- `Build/0.8.0-win-x64/Shovel.exe`
- `artifacts/preview-0.9.1-verification.txt`
- `artifacts/foundation-checks/20261026-151432-7f167dee62bc4163ba6c75183c7be306`
- `Build/0.9.1-win-x64/Shovel.exe`
