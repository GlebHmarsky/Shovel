# Roadmap and decision register

Updated 2026-10-30. [Product](product.md): UX; [requirements](requirements.md): acceptance; [architecture](architecture.md): implementation; [TODO](../TODO.md): only task checklist. This file holds milestone/evidence history and unresolved decisions.

## Current status

**0.30.0 / 2026-10-30 — `codex/windows-installer-updates`, based on `8a9741a`:** self-contained per-user Windows installer and pinned Velopack integration; signed GitHub release-feed checks/downloads, anti-replay floor, plain notes/progress/retry/cancel and explicit idle-gated restart. Public source **Shovel** under MIT and separate public **shovel-releases** confirmed; personal GitHub owner intentionally unset. Source/preview and the local-only installer make no updater request. Other agents' uncommitted master changes were excluded using a separate Git worktree. [Behavior/setup](distribution.md), [mechanism](architecture.md#windows-installer-and-updater-0300).

Evidence: **12 focused checks passed** (`artifacts/update-checks-final.txt`), including RSA verification/tampering, unsafe feeds, exact package bytes, corruption/cancellation cleanup, offline/login/oversized responses, remembered-version replay rejection, staged rehash, explicit restart/blocked retry, settings persistence and final repository readiness. The disposable readiness fixture proves active sequences, unresolved index entries without a sequence, and an unvalidated inactive tab block restart. Dark/Light normal/minimum renders reviewed in `artifacts/update-checks/2dca397b45414cb9af8634d336a5ecab` and regenerated in the final focused run. One early layout check caught a missing Close resource; corrected before final verification.

Final restore/build/publish succeeded with zero warnings/errors; **26 smoke checks passed in 65.3 seconds** (`artifacts/preview-0.30.0-final.txt`). First pre-final smoke passed in 71.5 seconds before final review added the unmerged-index check; that preview is retained as `Build/0.30.0-before-unmerged-fix-win-x64`. Smoke was then run once on final executable code; installer packaging reused its source-fingerprint evidence rather than repeating it. Final `Build/0.30.0-win-x64` has **21 verified checksums**, and four assemblies match the tested binaries.

`Build/0.30.0-installer-win-stable/Shovel-Setup.exe` is **76,303,635 bytes (72.8 MiB)**, self-contained but deliberately unconfigured/unsigned; local-only publication rejection was verified. Eight release-artifact checksums, main/helper included-framework configuration, bundled WPF/runtime/licenses and absence of the fixture endpoint were verified. A separate configured `win-test` build using a disposable RSA-3072 fixture key verified feed signature, signed package SHA-256/size, embedded repository/public key and private-key exclusion; its artifacts are retained only under `artifacts/signing-fixture/release`, not for distribution. Private-key generation was checked for protected current-user ACL. Logs: `artifacts/installer-0.30.0-local.txt`, `artifacts/installer-0.30.0-signing-fixture.txt`, `artifacts/package-validation.txt`.

No interactive launch, system installation, actual GitHub repository/release creation, live update endpoint, Windows publisher signing or installer deployment was performed. Native install/replacement/restart/uninstall, cross-session mutex behavior, power-loss/repair, Windows 10, native maintenance-log contents, accessibility and real GitHub/proxy behavior remain unverified. Uninstall retains separate settings/drafts; an app-data retention-choice UI is not implemented. Key rotation needs a signed bridge/repair procedure, not an automatic rollback claim. [TODO](../TODO.md#next-priorities) owns remaining rollout work.


**0.29.0 / 2026-10-29:** history RMB actions now perform detached Checkout, confirmed Soft/Mixed/Hard Reset, full SHA copying and Revert with explicit merge mainline. Direct Fetch/Push join split Pull; Commit remains modal. Menus use cached rows, defer selection reads and reject stale targets. Shared command execution/reinspection and conflict recovery remain; no new trust layer. [Behavior](product.md#history-actions-and-network-split-buttons-0290), [mechanism/limits](architecture.md#history-context-execution-and-default-network-targets-0290).

Focused evidence: seven history scenarios passed in `artifacts/history-action-checks.txt` (fixture `artifacts/history-action-checks/20261029-203456-bcb6d8f811734991835b696a87043b48`), covering real menu checkout, full-SHA copy, cancelled reset confirmation, stale targets, dirty checkout, each reset mode, normal/merge Revert, conflicted Revert/Abort and cancellation. History menu construction/layout **18 ms, zero Git calls** in one headless fixture; not a desktop latency claim. Reviewed Dark/Light **1200×800 / 900×620** offscreen renders and menu. The first direct-Push check failed because installed Git returned empty push:remoteref for default refspecs; local single-branch config resolution fixed it. Two targeted network checks then passed in **16.769 / 7.868 seconds** (`artifacts/default-network-checks.txt`, fixture `artifacts/history-action-checks/20261029-203933-8189f8216f22461498c81eaaeca93092`): real Fetch/Push using local bare remotes and separate push destination, preparation-free options, modal Commit, simple/upstream mapping and rejected matching scope. These are complete check durations including setup, not command latency. Nine focused scenarios passed across the initial history checks and targeted network rerun; unchanged history checks were not repeated. Final restore/build/publish succeeded with zero warnings/errors; **26 routine checks passed in 62.4 seconds** (`artifacts/preview-0.29.0-final.txt`, fixture `artifacts/smoke-checks/20261029-204215-d5444d8eb4c049ddae27d418c08d3dbb`). Ignored `Build/0.29.0-win-x64` matches 16 checksums, nine build files and four tested assemblies; both executables are 0.29.0.0 and packaged changelog matches (`artifacts/preview-0.29.0-package-check.txt`). Earlier previews retained; no full-suite run.

No interactive launch, live server/auth, native clipboard or desktop keyboard/accessibility validation. Clipboard/confirmation use injected test interactions; offscreen visuals do not prove native popup placement. Broader sparse/linked-worktree/filter/signing behavior and representative asset-repo command timings remain unverified; no absolute instant-operation promise. Named-side conflict editing is the next implementation priority.

**0.28.0 / 2026-10-29:** main Pull executes Rebase/auto-stash off against the configured upstream; adjacent arrow retains the preparation-free options modal and its chosen-settings submission. Shared execution owns progress/output/cancel/reinspection. Missing upstream has a localized disabled reason; changed/custom server mappings fail before fetch. No remembered-default setting or new consent. Confirmed for later work: selected history Checkout enters labelled detached HEAD; conflict Resolve applies and stages the chosen side. [Mechanism and limits](architecture.md#direct-default-pull-0280).

Four focused checks passed (`artifacts/default-pull-checks.txt`, fixture `artifacts/default-pull-checks/20261029-200226-bbe185c23f0f4be688a12e31ce634ba3`): options opening performs zero Git calls; direct Pull really rebases divergent local history, blocks duplicate submissions and refreshes rows; changed upstream mapping fails before fetch, missing upstream retains options and cancellation drains; modal Merge/auto-stash completes and preserves dirty text, while a later default dirty rebase fails without losing files/history. Reviewed Dark/Light **1200×800 / 900×620** offscreen renders. Check durations 1.255 / 9.526 / 8.887 / 11.903 seconds cover fixture operations, not desktop or individual-Git latency. Final restore/build/publish: zero warnings/errors; **26 routine checks passed in 51.6 seconds**, no failures (`artifacts/preview-0.28.0-final.txt`; fixture `artifacts/smoke-checks/20261029-200439-b6467ce6108a42049d9916d5d99e3351`). Ignored `Build/0.28.0-win-x64` matches 16 checksums, nine build files and four tested assemblies; both executables 0.28.0.0, packaged changelog matches (`artifacts/preview-0.28.0-package-check.txt`). Earlier previews retained; no full-suite run. No interactive launch, live server/auth or user-repository mutation; new history actions/conflict editing remain pending.

**0.27.2 / 2026-10-29 — planning only:** user prioritizes personal Fork replacement, inspectable source, routine-click reduction and controllable visuals. Promote direct default Pull, history context actions and the supplied named-side conflict workflow; LFS locks stay low priority with server support untested. [Product](product.md#requested-next-workflows-not-implemented) owns confirmed requests and open choices; [TODO](../TODO.md#next-priorities) replaces the former generic signing-first order. No new Git/UI functionality is claimed. Version/preview changes follow the repository's docs-delivery policy. Restore/build/publish succeeded with zero warnings/errors; 26 routine checks passed in 40.9 seconds (`artifacts/preview-0.27.2-final.txt`). `Build/0.27.2-win-x64` matches 16 checksums, nine build files and four tested assemblies; versions 0.27.2.0, packaged changelog matches, Build ignored (`artifacts/preview-0.27.2-package-check.txt`). No interactive launch, live server test or new feature acceptance claim.

**0.27.1 / 2026-10-29:** focused draft-write diagnosis. The original clean-cache/cross-profile reopen flow passed 12 repetitions with no I/O diagnostics. A controlled temporary reader denying file replacement reproduced `CommitDraftUnavailable`, now identified as `Save / PublishRecord / Win32 32` (`artifacts/draft-io-before-fix.txt`). Existing draft readers already use delete sharing; their mode was not the defect. The old 0.22 failure lacks the underlying OS error, so its exact cause remains unknown.

Add short cancellable retries only for sharing/lock failures, rechecking physical identity and expected revision after releasing each attempt's handles. Another profile's newer revision still wins; conflicting editor text uses the existing recovery copy. Bounded diagnostic events contain operation/stage/numeric error only. [Mechanism and limits](architecture.md#integrated-branch-management-and-drafts-0220).

Four focused checks passed (`artifacts/draft-io-after-fix.txt`, isolated fixture `artifacts/draft-io-checks/20261029-161642-3d5fe73966b44afbbe8d2799af45692f`): 12 reopens **359 ms**, temporary publication-lock recovery plus cancellation **94 ms**, competing-save recovery **124 ms**, persistent/non-sharing failures and diagnostic privacy **341 ms**. Before-fix publication recovery failed after **28 ms**; after-fix completion includes the intentional first 50 ms backoff. These are whole-check headless samples, not per-save latency or desktop responsiveness claims.

Final restore/Release build/publish: **zero warnings/errors; 26 routine checks passed, 0 failed in 54.0 seconds** (`artifacts/preview-0.27.1-final.txt`, fixture `artifacts/smoke-checks/20261029-162000-c26a93d624c849ebb975e15c38bee2e8`). Ignored `Build/0.27.1-win-x64/Shovel.exe`: **16 checksums, nine build files and four tested assemblies match**, both executables 0.27.1.0; packaged changelog matches (`artifacts/preview-0.27.1-package-check.txt`). Earlier previews retained; no full suite or benchmark run.

No UI layout changes or interactive launch; routine resource/model checks are headless. Actual ACL/disk-full/power-loss behavior and the historical failure's origin remain unverified. No user repository or real draft storage was used. If the old failure recurs, new typed events can distinguish the failing stage and OS condition. TODO now returns to the remaining local MVP/signing and failure-recovery work; live authentication and desktop validation remain separately authorized.

**0.27.0 / 2026-10-29:** remove duplicate sidebar title/Local Changes, horizontal sidebar scrolling, the 320-DIP width cap, permanent history paging band and main footer. Enlarge caption glyphs; shorten tooltips, including stale prototype wording. Keep bounded paging through View, Ctrl+Page Up/Down and wheel input past an edge. History/refresh/ref-location reads now show measured completion capsules with an entrance fade; action reconciliation keeps its own timer. Add local remote configuration and expose upstream set/unset under Repository. Stash nodes use a distinct marker and one true-base connection even when an internal object is independently tagged; real stash-only ancestors remain visible. [UX](product.md), [mechanisms and official remote source](architecture.md#workspace-cleanup-and-remote-setup-0270).

Four focused checks passed on final 0.27.0 source (`artifacts/workspace-cleanup-checks.txt`): preparation-free options and real local remote/upstream configuration, duplicate/stale-config/cancel protection; timed refresh/paging, actual wheel routing and failed/cancelled reads retaining rows; a stash with abandoned real base plus independently tagged index and connected graph boundaries; sidebar persistence/layout/caption size and controlled fade clocks. Dark/Light **1200×800 / 900×620** offscreen renders and Add remote dialog reviewed in `artifacts/workspace-cleanup-checks/20261029-144855-0b0f9fa2046d475ca4c994acfa892208`. Test remote targets are unavailable local fixture paths, so no live network occurs. An initial transparent offscreen dialog render was corrected before final verification.

Final restore/Release build/publish: **zero warnings/errors; 26 routine checks passed, 0 failed in 47.4 seconds** (`artifacts/preview-0.27.0-final.txt`; fixture `artifacts/smoke-checks/20261029-145231-cbc597f350b84beeb579a662b0ea75a9`). Matching ignored `Build/0.27.0-win-x64/Shovel.exe`: **16 checksums, nine build files, four tested assemblies**, executable versions 0.27.0.0 (`artifacts/preview-0.27.0-package-check.txt`). Earlier previews retained. No full-suite/benchmark run.

No interactive launch, desktop control, live remote/auth or OS setting change. Native dragging/maximize/DPI/accessibility and actual fade cadence remain unverified. Wider sidebar can require horizontal scrolling in the history table; it never adds a sidebar horizontal scrollbar. History remains bounded 200-row pages, not an infinite in-memory list. Remote edit/delete is deferred. This increment does not claim faster Git reads. The intermittent draft-write investigation remains next in TODO.


**0.26.0 / 2026-10-29:** main menus share a WindowChrome caption with working window commands; toolbar order is Fetch/Pull/Push/Commit after Open. Stashes render as one entry with private index/untracked parents folded, preserving original details and independently reachable commits. Fixed graph columns keep three successive merged branches straight until their common base; turns use broader curves. [UX](product.md#compact-caption-and-history-0260), [mechanisms and limits](architecture.md#caption-and-history-layout-0260).

Four focused checks passed (`artifacts/history-layout-checks.txt`): successive merges and page continuity; octopus/criss-cross/independent roots, slot reuse and width/cycle guards; real tracked/untracked stashes across 200-row pagination/ref navigation with independently tagged parents retained; caption/menu/command wiring and toolbar order. Reviewed **1200×800 / 900×580 Dark/Light** and folded-stash offscreen renders in `artifacts/history-layout-checks/20261029-134823-7ddad9a10b07482a913d610a579b7f97`. An initial run caught the old parser's 201-raw-record cap; it now allows the bounded folding over-read while retaining 200 visible rows and the byte limit. Fixture assertions were corrected for shared identical stash index objects and multiple buttons using the same command.

The small disposable 208-visible-entry/two-stash fixture measured **466 ms** for inspection/session construction and **326 ms** for its first page in the final run. These are single headless fixture samples, not desktop startup/p95 or a before/after speed claim. New stash ancestry exclusion can traverse older/divergent history under existing deadlines; representative large-repository latency remains unmeasured.

Final restore/Release build/publish: **zero warnings/errors; 26 routine checks passed, 0 failed in 41.8 seconds** (`artifacts/preview-0.26.0-final.txt`; fixtures `artifacts/smoke-checks/20261029-135208-6671f47ca8d6484db225240b1f78ed2c`). No full-suite/benchmark rerun. Preview `Build/0.26.0-win-x64/Shovel.exe`: **16 checksums, nine build files and four tested assemblies match**, executable versions 0.26.0.0 (`artifacts/preview-0.26.0-package-check.txt`). Build remains ignored and older previews retained.

No interactive launch, desktop control, live network/auth or system changes. Caption drag/resize/maximize/system menu, native snap behavior, mixed DPI and screen-reader/high-contrast operation still need desktop validation. Existing 64-column/256-stash limits remain explicit; jumped pages start a fresh graph. Draft-write diagnosis remains next in TODO.

**0.25.0 / 2026-10-29:** fixed-slot operation duration capsules with accurate terminal states, three-second hold/two-second fade, retained results and no cross-tab replay. Existing timers cover branch/network/recovery/stash/init actions and Stage/Unstage/Commit; background reads stay quiet. Shared buttons, tabs, navigation, popups and the center panel use consistent small corner radii. Commit guidance now requires descriptive change-focused titles. [UX](product.md#operation-feedback-0250), [implementation](architecture.md#operation-presentation-0250), [visual rules](theme.md#borders-and-operation-feedback-0250).

Four focused checks passed (`artifacts/operation-style-checks.txt`): terminal outcome/reconciliation ordering; real disposable stage/unstage/branch actions forwarded to shell feedback; replacement/tab clearing and controlled WPF fade-clock behavior; palette contrast and **1200×800 / 900×580** Dark/Light offscreen layouts. Reviewed all four renders in `artifacts/operation-style-checks/20261029-040306-a869ce7908454404aaf1c9b7d472dc77`. A fixture initially attempted branch switching with an untracked file and correctly hit the existing dirty-worktree guard; corrected the fixture. Animation verification uses explicit clock seeking because an offscreen tree has no desktop paint loop; it does not measure live smoothness.

Final restore/Release build/publish: **zero warnings/errors; 26 routine checks passed, 0 failed in 32.7 seconds** (`artifacts/preview-0.25.0-final.txt`; fixture `artifacts/smoke-checks/20261029-040557-0af9a2d9ec6d4c438fa931cbbfda0e42`). An earlier restricted-host invocation stopped at temporary Git initialization and produced no preview (`artifacts/preview-0.25.0-sandbox-attempt.txt`); the normal-host retry passed. No full-suite/benchmark run. Matching ignored `Build/0.25.0-win-x64/Shovel.exe`: **16 checksums, nine build files, four tested assemblies**, executable versions 0.25.0.0 (`artifacts/preview-0.25.0-package-check.txt`). Previous previews retained.

No interactive launch, desktop control, live network/auth or OS setting change. Native chrome, live fade cadence, screen reader/high contrast, translated/RTL and mixed-DPI behavior remain desktop-validation limits. This pass adds presentation feedback, not a Git-speed claim. The intermittent draft-write investigation remains next in TODO.

**0.24.0 / 2026-10-29:** same-author row backgrounds with ordinary text, compact straight/rounded graph rendering, equal full-width repository tabs, working Explorer/Rider/VS Code/Git Bash toolbar handoff and live persisted Dark/Light appearance. No Work button. Core history topology/pagination are unchanged. [Workflow](product.md#workspace-toolbar-and-appearance-0240), [tool discovery/schema limits](architecture.md#workspace-presentation-and-tool-handoff-0240), [palette](theme.md).

Four optional focused checks passed (`artifacts/workspace-0.24-checks.txt`): schema 4→5 migration/persistence and palette contrast/high-contrast mapping; literal launch paths and Unreal-project preference/ambiguity; bound explicit tool commands/failure recovery with fake process dispatch; real disposable merge history with background-only peers, selection precedence, 1/2/3 equal tabs and both-theme **1200×800 / 900×580** offscreen renders. Reviewed renders in `artifacts/workspace-checks/20261029-034011-4982bda311ca4625bef6063026f9f999`. Zero tool-discovery calls during construction/render. An initial test-only double/int layout-size compile error was fixed before execution. Focused rendering completed in 1.36 seconds including fixture setup/history read/layout; this is not desktop startup or per-click latency.

Final restore/Release build/publish: **zero warnings/errors, 26 routine checks passed, 0 failed in 32.2 seconds** (`artifacts/preview-0.24.0-final.txt`; fixtures `artifacts/smoke-checks/20261029-034524-7b5faac4ac4947ba8756df39041e78b8`). No full-suite/benchmark rerun. Preview `Build/0.24.0-win-x64/Shovel.exe`: **16 checksums, nine build files and four tested assemblies match**, executable versions 0.24.0.0; changelog matches, Build ignored and earlier versions retained (`artifacts/preview-0.24.0-package-check.txt`).

No interactive app/tool launch, desktop control, live network/authentication, dependency installation or OS theme change. Actual Rider/Unreal project loading, Bash/Explorer/VS Code windows, native light title-bar paint, OS accessibility/Windows 10 and many-tab/large-graph behavior still need authorized desktop validation. Discovery covers known install locations plus manual executable selection, not every vendor install layout. Nested Rider projects are not scanned; external processes own their lifetime. The intermittent draft-write investigation remains the next correctness priority in TODO.

**0.23.0 / 2026-10-29:** functional expandable repository sidebar: local branches/tracking/switch/manage; fetched remote branches/Fetch; tag navigation; inline stashes with Apply/Pop/confirmed Drop and saved stash commits in history; submodule open-as-tab/Initialize-Update/Sync URL. Cached menus do no preparation; secondary lists follow first history. Ref navigation retains rows during bounded background reads. [Behavior](product.md#repository-sidebar-0230), [implementation/limits](architecture.md#repository-sidebar-0230).

Four focused disposable checks passed (`artifacts/sidebar-checks.txt`, fixture `artifacts/sidebar-checks/20261029-031404-56dc6751ee174d5d93cab2cf24b42d3e`): local tracking/refs/stash graph, bound branch and stash mutations, submodule update/sync/open with dirty-content protection, and navigation beyond the first 200 rows. The first run exposed list refresh not being awaited by the action task; corrected before the complete passing run. Subsequent review fixed nested right-click routing, dark selection and post-action sidebar refresh. The focused list/input-binding/render check passed again (`artifacts/sidebar-render-final.txt`, fixture `artifacts/sidebar-checks/20261029-032613-2a8f71d78abf47a98bcf0e4eda319f19`); reviewed **1200×800 / 900×580** offscreen renders. Initial blank captures were a harness layout error, not visual verification. No popup or main window was shown.

Final restore/Release build/publish: **zero warnings/errors, 26 smoke checks passed, 0 failed in 31.5 seconds** (`artifacts/preview-0.23.0-final.txt`; fixture `artifacts/smoke-checks/20261029-032808-37ff976c50bf40a8ad160ab7e8e6ac73`). The routine suite remains 26, maximum 30; no full regression or benchmark rerun. Preview `Build/0.23.0-win-x64/Shovel.exe`: **16 checksums, nine build files and four tested assemblies match**, executable versions 0.23.0.0; changelog matches, Build ignored and previous versions retained (`artifacts/preview-0.23.0-package-check.txt`).

Evidence is headless/local-Git only: no interactive launch, user-repository mutation, live remote/auth, Windows 10 or OS accessibility verification. Individual feature checks include several operations and are not click-to-paint/per-operation speed claims. Current limits: 256 saved stash tips, 16 MiB/30-second ref locating, 4 MiB sidebar metadata, fresh lane layout after jumps, tree expansion reset on refresh. Submodule add/remove/deinit/recursive/remote-latest management is deferred. Next priority remains focused diagnosis of the intermittent draft write, followed by separately authorized representative desktop/large-repository validation.

**0.22.1 / 2026-10-29:** replaced automatic 263-check verification with **26 explicit smoke checks**, maximum 30, fail-fast and measured per-check output. The existing full suite is opt-in through `--full`; focused suites/benchmarks remain available. Final restore/Release build/publish passed with zero warnings/errors; **26 passed, 0 failed in 33.3 seconds**, including real local init/stage/unstage/commit/diffs/branch deletion/stash/push/fetch/pull/merge-abort plus settings/drafts/process and model scheduling checks. This is a single host run of reduced representative coverage, not equivalent to the full regression matrix or a universal timing guarantee. Hard limits: 30 seconds/check and 180 seconds total; target <120 seconds. Log: `artifacts/preview-0.22.1-final.txt`; fixtures: `artifacts/smoke-checks/20261029-025542-5b8263ca941c48789dd70086c2f86f10` (contains the owned temp Git fixture location).

Three isolated runner probes verified first-failure exit 1, rejection before a 31st check, and independent timer exit 124 (`artifacts/smoke-runner-guards.txt`). An initial smoke-fixture error omitted SettingsStore's required Load before Save; it stopped in 0.5 seconds without publishing, then was corrected (`artifacts/preview-0.22.1-smoke-setup-failure.txt`). No production bug or extended-suite rerun is claimed. Preview `Build/0.22.1-win-x64/Shovel.exe`: **16 checksums, nine build files and four tested assemblies match**, executables 0.22.1.0; changelog matches, Build ignored, earlier versions retained (`artifacts/preview-0.22.1-package-check.txt`). No application UI changes or interactive launch; resource/layout smoke is headless, not desktop verification. The prior intermittent draft-write issue remains unresolved and should use focused diagnosis next.

**0.22.0 / 2026-10-29:** merged Task 1 `543595b`, Task 2 `572b2af`, Task 3 `d7aa228` into master with merge commits; original branches/worktrees retained. Resolved the expected additive `Strings.resx` conflict by preserving both resource groups. Registered all new suites and opt-in benchmark dispatch, isolated the legacy commit-modal fixture's draft storage, and corrected test-only span/await comparisons in Tasks 1/2. Application sources required no compiler fix. Focused checks: branches **13/13**, drafts **28/28**, performance harness **6/6** (`artifacts/integrated--check-branches.txt`, `artifacts/integrated-drafts-final.txt`, `artifacts/integrated--check-history-performance.txt`). Review also found and fixed an early-open editor gap: persistence now binds after the first validated repository read when history has not supplied identity yet, preserving text typed before that read. A dedicated regression passes. The first full run was stopped before publishing for the early-open fix (`artifacts/preview-0.22.0-interrupted.txt`). The next run reported 262 pass/1 failure: `CommitDraftUnavailable` on the clean-cache cross-profile replacement check (`artifacts/preview-0.22.0-storage-failure.txt`); its canonical draft still contained the previous text. A fresh focused rerun passed 28/28 (`artifacts/drafts-recheck.txt`); the underlying OS failure was not established. TODO retains investigation rather than claiming a fix. Final fresh restore/Release build/publish passed with zero warnings/errors and **263 checks passed, 0 failed** (`artifacts/preview-0.22.0-final.txt`). The expanded sequential view-model aggregate has a 20-minute harness deadline; individual operation limits are unchanged.

Final fixtures/renders: `artifacts/foundation-checks/20261029-021905-15466acfa4fc4d65976ff6d5db526563`. Reviewed normal **1200×760**, minimum **900×580** workspace and **660×500** branch-deletion content renders; minimum history retains horizontal scrolling. Offscreen content rendering does not certify native window chrome or desktop behavior. Preview `Build/0.22.0-win-x64/Shovel.exe`: **16 checksums, nine build files and four tested assemblies match**, both executables 0.22.0.0 (`artifacts/preview-0.22.0-package-check.txt`); packaged changelog matches, Build is ignored/untracked and previous previews retained. No interactive launch, live network/auth, Windows 10/DPI/accessibility certification, million-scale benchmark or five-minute idle measurement. Next: investigate the intermittent draft write before further persistence claims, then run representative performance profiles.

Agent assessment: Task 1 delivered the requested end-to-end local branch workflows with normal Git protections and substantial mutation/race coverage. Task 2 delivered the more complex storage/lifetime design with revision exclusion, recovery copies and 28 passing focused cases; power-loss, physical disk-full/ACL and visible UX still need separate certification. Task 3 supplied deterministic opt-in fixtures, resource/cancellation reporting and six passing small checks without modifying production algorithms; no million-scale/idle measurement or performance improvement is claimed. All worker handoffs explicitly reported uncompiled source under the no-build assignment; the two compile defects were in unexecuted tests, not production code. Their original handoffs remain historical worker evidence.

**0.21.0 / 2026-10-29:** local repository initialization through File / Repository → New repository. Preparation-free options, optional initial branch, real Git execution and automatic selection/opening; existing files and partial results are retained. [Workflow](product.md#new-local-repository-0210), [implementation and destination limits](architecture.md#local-repository-initialization-0210). Restore/Release build/publish passed with **zero warnings/errors; 216 checks passed, 0 failed**. Log: `artifacts/preview-0.21.0-final.txt`; final full-suite fixtures/renders: `artifacts/foundation-checks/20261028-232619-f0955d42cd624fefbcb742ff0377e799`. Preview `Build/0.21.0-win-x64/Shovel.exe`: **16 checksums, nine build files and four tested assemblies match**, both executables 0.21.0.0 (`artifacts/preview-0.21.0-package-check.txt`); changelog matches, Build ignored/untracked and earlier versions preserved.

Coverage includes populated/missing/Unicode paths, configured branch/templates, real staging, existing/linked roots, invalid/nested/bare/metadata destinations, missing executable/redirection, partial failure, cancellation/reinspection/gate release and bound shell restoration/navigation. Disposable temp roots outside the source checkout are recorded in `initialization-fixtures.txt`; isolated config only. After correcting only focused-test artifact routing, a rebuild and rerun passed **9/9** (`artifacts/init-harness-final.txt`; `artifacts/init-checks/20261028-234127-589bca78ca374160bd348c4896570265`). Production assemblies remained identical to the full-suite/published files.

Reviewed final offscreen options and **1200×760 / 900×580** result layouts. Full-suite warmed options edit/layout: **6 ms, zero Git queries**; separate focused-process layout: **94 ms**. Bound creation through automatic empty history/status: **1,899 / 2,451 ms** respectively. These small-fixture single samples exclude visible input/paint and do not establish startup, p95 or a universal latency guarantee. No interactive launch, live network or system changes; Windows 10, OS keyboard/focus/accessibility and network-share creation remain unverified. Next functionality: branch deletion and tracking.

**0.20.0 / 2026-10-29:** Local Changes menus now perform confirmed selected Discard/Stash, configured External Diff, first-parent file History/committed Blame, Ignore/LFS rule editing and binary patch export. [Scopes and limits](product.md#selected-file-actions-0200), [implementation](architecture.md#selected-file-actions-0200). Restore/Release build/publish passed with **zero warnings/errors; 207 checks passed, 0 failed**, including 15 new file-action checks. Log: `artifacts/preview-0.20.0-final.txt`; final fixtures/renders: `artifacts/foundation-checks/20261028-215415-8c1ed7a073644487860683a100fe376a`. Preview `Build/0.20.0-win-x64/Shovel.exe`: **16 checksums, nine build files and four tested assemblies match**, both executables 0.20.0.0 (`artifacts/preview-0.20.0-package-check.txt`); changelog matches, Build ignored/untracked and previous previews retained.

Evidence includes split staging, staged/unborn/renamed/deleted/binary/untracked discard, preserved unrelated files, stale targets and existing locks; selected stash trees excluding unrelated staged content, split-index and cancellation after saving; patches applied to a separate repository and cancelled output preservation; actual configured fixture diff-tool success/failure, literal Ignore/LFS rules and nested overrides; 206 commits across a rename, explicit merge scope, committed blame, failed-refresh retention and closed/cancelled generations. Bound menu Discard/Stash/History/Patch and Cancel execute real workflows. Late Cancel/Stop is harmless after closing history and its shared file viewer. No user's repository was used for destructive tests.

Reviewed final single/multiple/staged menus, confirmation, blame and **1200×760 / 900×580** workspace/history renders; narrow history columns remain horizontally scrollable. Full-suite warmed construction/layout: menu **10 ms**, file history **23 ms**, each with zero opening-time Git calls. Selected Discard through scoped publication: **667 ms** (validation 355, command 172, target refresh 116 ms). Separate focused run: **15 passed**, Discard **605 ms**, first file-history construction/layout **602 ms** (`artifacts/file-actions-0.20-focused-final.txt`). These are small-fixture samples, excluding visible paint/desktop input and OS-cold startup; not p95 or user-repository guarantees. An earlier full run was stopped before packaging to repair close/cancel lifetime handling; only the final complete log is release evidence. No interactive app/editor/Explorer/clipboard, live remote, Windows 10/accessibility or installation validation. Next functionality: local repository initialization; broader tool/LFS/file-history variants remain in TODO.

**0.19.0:** Local Changes row context menus with selected/all Stage/Unstage, Windows file handoff and path copying. Captured targets, preserved multiselection, explicit Shift-independent commands and deferred selection diffs. [Implementation and limits](architecture.md#local-changes-file-menus-0190). Final restore/Release build/publish passed with **zero warnings/errors; 192 checks passed, 0 failed**, including five file-menu checks. Log: `artifacts/preview-0.19.0-final.txt`; fixtures/renders: `artifacts/foundation-checks/20261028-205707-2b3a1da6d9ef4181815c3ee3b720d0e2`. Preview `Build/0.19.0-win-x64/Shovel.exe`: **16 checksums, nine build files and four tested assemblies match**, both executables 0.19.0.0 (`artifacts/preview-0.19.0-package-check.txt`). Build ignored/untracked; earlier previews preserved.

Menu evidence: routed right-click selection, bound real index mutations, retained drafts/worktree bytes, stale context refusal after refresh, literal Unicode/comma/bracket paths, deleted-file folder planning, localized copy/open failures and cancellation on close. Reviewed final single/multiple/staged menus and **1200×760 / 900×580** offscreen workspace layouts. Warmed menu construction/layout measured **11 ms with zero Git calls**, versus 33 ms in the standalone focused process; these are individual headless samples, not desktop/p95 claims. Existing held-read regression cases published Stage rows in 442 ms and Unstage rows in 533 ms on disposable files. No interactive launch, actual editor/Explorer/clipboard dispatch, live remote, installation or deployment; native keyboard/focus and accessibility remain unverified. Advanced menu entries were deferred at this milestone and are delivered in 0.20.0; local repository initialization remains next.

**0.18.0:** create/inspect/apply/pop/confirmed drop in an owned Stashes modal over retained history. Cached opening, selected-hash/draft retention, real saved tracked/index/untracked diffs, staged-state option and shared progress/cancellation. [Implementation and limits](architecture.md#stashes-0180). Final restore/Release build/publish passed with **zero warnings/errors; 187 checks passed, 0 failed**, including 11 stash checks. Log: `artifacts/preview-0.18.0-final.txt`; fixtures/renders: `artifacts/foundation-checks/20261028-202428-46ce2b54be75456f81ce9a883369fe08`. Preview `Build/0.18.0-win-x64/Shovel.exe`: **16 checksums, nine build files and four tested assemblies match**, both executables 0.18.0.0 (`artifacts/preview-0.18.0-package-check.txt`). Build ignored/untracked and older previews preserved. No interactive launch, live remote, installation or deployment.

Stash evidence: real split staged/unstaged restoration, binary saved-untracked inspection, ignored preservation, clean/locked outcomes, linked-worktree shared entries with changes confined to the selected worktree, conflict/untracked collision retention, stale selector/replaced identity/unborn refusal, concurrent insertion and cancellation after apply. Bound Create/Apply/Drop and shell reconciliation work; late list/detail/close results cannot replace current data, and late Cancel after close is harmless. Reviewed final **1200×760 and 900×580** offscreen renders. Full-suite warmed construction/layout: **19 ms**, then five samples **15/14/7/12/7 ms**, all with zero opening-time Git calls. Separate focused-process first construction was **669 ms**, warmed samples **35/29/29/26/28 ms** (`artifacts/stash-0.18-focused-release.txt`). These small-fixture figures measure construction/layout, excluding background data reads, visible paint, OS-cold startup and p95; Windows 10/desktop/accessibility remain unverified.

Verification history: one full run was stopped before publishing to fix the close/cancel lifetime race; another hit the harness's eight-minute aggregate dispatcher deadline while still passing recovery checks, before stash cases ran. The expanded suite now has a twelve-minute aggregate bound; individual operation/focused-suite timeouts are unchanged. Both incomplete logs remain in artifacts; only the final complete run is release evidence. Next functionality: local repository initialization, then branch deletion/tracking. Representative user-repository/desktop latency validation still requires authorization; advanced stash variants and conflict editing remain deferred.

**0.17.0:** in-app recovery for detected merge/rebase/cherry-pick/revert sequences. Cached sidebar/Repository controls, Continue with existing messages, actual-target Skip/Abort confirmations and shared progress/cancellation/reinspection. [Implementation and limits](architecture.md#sequence-recovery-0170). Final restore/Release build/publish passed with **zero warnings/errors; 176 checks passed, 0 failed**, including 19 recovery checks. Log: `artifacts/preview-0.17.0-final.txt`; fixtures/renders: `artifacts/foundation-checks/20261028-192239-a70b57c225f44aa290efa387dfb9b171`. Preview `Build/0.17.0-win-x64/Shovel.exe`: **16 checksums, nine build files and four tested assemblies match**, both executables 0.17.0.0 (`artifacts/preview-0.17.0-package-check.txt`). Build ignored, older previews preserved; source/docs committed locally. No interactive launch, live remote, installation or deployment.

Recovery evidence: all four operations continue after real file staging and abort to original tips; supported Skip omits the conflicted patch. Unresolved/stale/missing state refuses; failed hooks/locks, subsequent conflicts, sequencer-only continuation after external commit, auto-stash conflicts after exit zero, real hook cancellation/common gate, and failed post-command refresh retain truthful state. The bound shell verifies actual Continue/Abort, cancelled Skip, declined confirmation with zero calls and snapshot changes during confirmation; it never retargets the reviewed sequence. An earlier full run was intentionally stopped before packaging to fix that confirmation race; only the final complete run is release evidence.

Reviewed final offscreen recovery layouts at **1200×760 and 900×580**: controls fit the sidebar and history stays in place. Warmed layout measured **44 ms / zero Git calls**; earlier focused-process construction measured 271 ms. Real Continue validation/command/local-state observation samples: Merge 547 ms, Rebase 924 ms, Cherry-pick 642 ms, Revert 706 ms. These are single samples on small disposable conflicts, excluding subsequent shell history/count reconciliation; not desktop click-to-paint, p95, large-asset or Windows 10 guarantees. Conflict editing, interactive todo/message editing, live signing/auth and OS accessibility remain separately unverified/deferred. Next priority remains representative repository latency validation, with authorization for user-repository/desktop measurements.

**0.16.1:** native title-bar colors now follow the shared dark chrome for the main, commit, branch/network and authentication windows, with high-contrast reset and native controls preserved. [Implementation/platform limits](architecture.md#native-title-bars-0161). Final restore/build/publish: zero warnings/errors, **157 checks passed, 0 failed** on the fresh run, including the merge-abort case below. Log: `artifacts/preview-0.16.1-release.txt`; fixture: `artifacts/foundation-checks/20261028-160901-d8f8c7d2924e43e197b49ed070103e03`. Preview `Build/0.16.1-win-x64/Shovel.exe`: 16 checksums, nine build files and four tested assemblies match, both executable versions 0.16.1.0 (`artifacts/preview-0.16.1-package-check.txt`). Earlier previews preserved; Build ignored; no interactive launch, live network, installation or deployment.

Title-bar evidence: Windows 11 build 26200, hidden native HWND only, no `Show`/desktop interaction. DWM accepted explicit palette attributes and restoration of high-contrast defaults; existing native WindowStyle/ResizeMode were preserved. This is API/lifetime verification, not visible paint, hover, focus or Windows 10 certification. The first full run (`artifacts/preview-0.16.1-final.txt`) passed 156 checks but its disposable `conflict Merge` cleanup failed because `index.lock` existed; replay confirmed that error. The fixture was preserved without lock deletion; origin of the leftover lock is unconfirmed. No title-bar code was changed in response; the fresh complete suite passed.

**0.16.0: fast file actions and binary cards.** Stage/Unstage publishes real affected rows before a coalesced background scan; obsolete reads no longer hold up the write. Binary comparisons show Old/New extensions and sizes, including local LFS metadata. Bottom activity bars/ticking footer are removed; static results/errors/cancellation remain. Further theme polish stays P2; arbitrary-line staging and text side-by-side/image previews remain future work. Verification and measured limits are recorded below.

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
| 0.17.0 | Continue/Skip/Abort for interrupted merge/rebase/cherry-pick/revert | Disposable conflicts, cached controls, cancellation/locks/hooks/stale state; current-status evidence |
| 0.18.0 | Stash create/inspect/apply/pop/drop | Saved objects, conflicts/collisions/stale lists/cancellation, cached modal and partial success; current-status evidence |
| 0.19.0 | Local Changes right-click menus | Captured selection/all scopes, stale menus, file handoff and offscreen evidence; current status |
| 0.20.0 | Selected-file discard/stash and remaining menu workflows | Exact-target confirmation, real tools/rules/patch/history/blame, partial-result/cancellation and offscreen evidence; current status |
| 0.21.0 | Local repository initialization | Preparation-free options, real init and automatic opening; preserved files/partial results, isolated fixtures and offscreen evidence |
| F1g onward | Historical file tree; filtering when requested | Bounded content and relevant read tests |
| 2: Local MVP remainder | Branch delete/tracking | Signing/recovery coverage |
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

## Fast file actions and binary cards 2026-10-28

0.16.0 advances the user's binary comparison/Unstage request ahead of sequence recovery. Confirmed delays in 0.15: file writes waited for obsolete diff/status reads, and action completion included a full repository status scan. Index-action preflight also repeated location/remote/upstream queries. The actual index write was a small fraction of the measured wait. The user's exact **11.5-second** case was not reproduced; no user repository was inspected or mutated for these tests.

The action now cancels stale read generations, validates the current index/HEAD/config/identity, uses literal path-limited `reset --no-refresh` for Unstage, and publishes actual affected-path status. A trailing background full scan refreshes other rows. LFS/binary metadata avoids unnecessary content diffing. [Architecture](architecture.md#fast-file-results-and-binary-metadata-0160) owns bounds, failure handling and external-writer limitations; no synthetic row movement or success is used.

Repeatable isolated fixture: 2,000 text files/1,001 branches plus 64 unchanged 4 MiB binary assets (256 MiB), asset mtimes invalidated each sample, one modified text path. Three Stage/Unstage samples per version, Windows x64/Git 2.51.1/.NET 10.0.12 on this host; no reboot/cache flush, live network or user repository. Medians:

| Elapsed to | 0.15.0 | 0.16.0 final |
| --- | --- | --- |
| Stage: fresh affected rows | 1,880 ms | 503 ms |
| Unstage: fresh affected rows | 1,709 ms | 514 ms |
| Stage: selected text diff | 2,440 ms | 1,200 ms |
| Unstage: selected text diff | 2,178 ms | 999 ms |

The old row metric waited for the **entire scan**; the new metric publishes freshly checked affected paths and retains unrelated cached rows until a subsequent full scan. This measures the user's file-action feedback, not identical whole-repository completion. Baseline full status alone took 811–916 ms; Unstage's actual Git command took 71–94 ms. Final affected-row ranges: Stage 503–552 ms, Unstage 495–602 ms. Intermediate runs ranged lower (about 300 ms), so use the final results rather than the best run. Small samples, process/cache/host-load variance and selected large assets/filters preclude desktop p95 or universal subsecond claims. Proposed fixture budgets: median affected rows ≤750 ms, selected diff ≤1.75 s; representative desktop/asset-repository validation remains open.

Baseline: `artifacts/0.16-assets-before.txt`, fixture `artifacts/working-copy-benchmarks/20261028-150105-d29c23d87811402983e641f499d96ed2`. Final: `artifacts/0.16-assets-release.txt`, fixture `artifacts/working-copy-benchmarks/20261028-153350-0b47ee7c89004e56a122a97042a1656c`. A separate gated view-model check holds obsolete diff/full-status reads despite cancellation, proving new file actions complete independently and late results cannot replace fresh rows.

Final restore/Release build/publish passed with **zero warnings/errors; 156 checks passed, 0 failed**. New checks cover scoped real status, HEAD/worktree/unrelated-file preservation, held obsolete reads, late-result rejection, failed full refresh retaining successful rows, logical LFS/index/worktree/parent sizes with no helper execution, malformed pointers, absent vs empty files, binary→text selection, and errors remaining visible over retained binary cards. Existing hunk/commit/branch/network/authentication/cancellation regressions pass. Full-run gated Stage/Unstage completed in 459/495 ms while the old read was still held; this proves removal of that dependency, not desktop latency.

Reviewed final offscreen binary modal at 1200×760 and 900×580 plus compact cards: Old/New/LFS/extension/size fit and bottom activity bars are absent. Log: `artifacts/preview-0.16.0-final.txt`; fixtures/renders: `artifacts/foundation-checks/20261028-154234-44b64798ea964e7b952a26bdfc1b9912`. Earlier stopped runs are not release verification. Preview: `Build/0.16.0-win-x64/Shovel.exe`; **16 checksums, nine build files and four tested assemblies** match, both executable versions 0.16.0.0 (`artifacts/preview-0.16.0-package-check.txt`). Build/artifacts remain ignored and earlier previews remain intact.

No interactive launch, desktop control, live authentication/network test, user-repository mutation, dependency installation or deployment was performed. Native input/paint/accessibility/Windows 10 and the reported asset-repository delay remain unverified. Hunk/commit/larger-selection reconciliation still takes the full-scan path. Next functional priority: in-app sequence recovery; representative latency validation remains separately authorized work.

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
