# Requirements and acceptance

Updated 2026-10-28. Confirmed unless marked proposed. IDs remain stable. [Product](product.md) owns UX; [architecture](architecture.md) mechanisms; [roadmap](roadmap.md) evidence. Deferred scope may advance when requested; phase labels do not veto independent feature work.

## Git functionality and compatibility

| ID | Requirement |
| --- | --- |
| G1 | Deliver real clone/init/open, status, file/hunk stage/unstage, commit/amend, branches/tracking, fetch/pull/push, tags, stashes, merge/rebase/cherry-pick/revert/reset/reflog/conflicts in usable increments. Every requested feature includes working controls and behavior; no placeholder/no-op UI or mock completion. Implement or omit unfinished entry points; state-dependent disabling needs a localized reason. |
| G2 | Preserve Git for Windows configuration, credentials, hooks, signing, filters/tools. Explain missing/unsupported Git; no automatic install/global config edits. Ordinary use needs no extra repository/program-trust consent. |
| G3 | Model unborn/detached HEAD, dirty index/worktree, untracked/ignored, rename/delete/binary/EOL. No fixed 40-character hashes or loose-ref assumptions. |
| G4 | Detect existing merge/rebase/cherry-pick/revert; implement valid continue/skip/abort with recovery work. Reopen observes state, never replays old intent. |
| G5 | Handle multiple remotes, missing/deleted upstreams, ahead/behind/divergence, rejected pushes, rewritten history and branches occupied in other worktrees. |
| G6 | Detect submodule/linked-worktree/LFS/shallow/partial/sparse/large-repo cases. Explain unsupported operations; no silent sparse expansion, lazy fetch, LFS bypass or recursive submodule mutation. |
| G7 | Preserve exact Unicode/space/dash/unusual/long/case-only paths; show unsupported Windows tree names without promising checkout. Handle permissions/locks/encoding/external edits. |
| G8 | Fixed All Commits: local/remote/tag/current-HEAD ancestry, including detached; excludes reflog-only/unreachable/stash-only. Ignore legacy narrow scopes. No scope/filter selector until revisited. Accessible scope/limits, ref labels/range/loading, complete selected metadata within read limits; no silent paging truncation. |
| G9 | Automatically load selected startup tab; publish cached tab page immediately, refresh asynchronously when stale. Retain page/selection/scroll and useful rows on failure; reject outdated results. Bound cache/concurrency, no preload/polling; measure full path before optimizing. |
| G10 | Exact ordinal raw author name/email peers: nonblank email, blank-name equality, no committer/mailmap. Light-yellow peer text/accessibility relationship; normal selection, graph/ref colors, virtualization/keyboard/high contrast preserved. |
| G11 | Tab menu targets placement tab: refresh/move/close/close others; MMB closes clicked tab. Active close cancels/drains/rejects late reads; inactive close preserves active work. Current Git action blocks repository navigation until completion/cancel+reinspection, even after its options modal closes; closing never deletes data. |
| G12 | History directly below tabs; standard menus/toolbar, centered repo/branch/timer. Older prototype-retention mandate is superseded by G1. Pull/Push badges show real local-upstream behind/ahead after rows, handling unavailable/stale/cancelled states; never fetch implicitly or substitute zero. |
| G13 | Real Branch create/start/checkout and switch; target-specific Fetch/Pull/Push options. Open fixed-size close-only dialogs from cached local targets, with zero opening/dismissal Git calls or remote checks. Validate after submission, retaining errors in Operation details. Explain/discuss unavoidable UI waits; ordinary interaction must not depend on data preparation. Normal Execute closes options and starts the action, no separate trust checkbox; opening/dismissing does not execute. Top-center phase/progress/duration/cancel and retained output survive closure; refresh history/counts after Pull. Pull defaults to rebase with merge alternative and optional tracked auto-stash (2026-10-28). Preserve Git protections and recoverable conflicts; zero exit with unmerged files is not success. Force remains deferred. |
| G14 | Original local/tag/remote badges, current-tip mark, grouping only configured same-tip upstream; full accessible meaning. Local tracking tip is not a live-server guarantee. |
| G15 | Modal Local Changes/Commit over retained history: separate staged and unstaged/untracked/conflicted lists, real numbered/colored text diffs and binary sizes, rename/path help, subject/body/amend. File stage/unstage and commit work end to end; Shift changes the same buttons to all-files and Commit and Push. Revalidate writes, confirm amend target, retain drafts on failure and separate commit success from push failure. Paint cached content before scans; cancel/drain/reject obsolete results and retain stale rows on failure. A selected-tab count may run after history; no history-blocking scan, inactive preload or polling. Ignored files excluded; expose submodule/filter/preview limits. Supersedes read-only embedded G15 (2026-10-28). |

**Stashes (0.18.0):** create with optional untracked files; inspect saved tracked/index/untracked content; apply/pop with explicit staged-state restoration; confirmed exact-target drop. Cached opening precedes local reads, failure retains useful rows, no implicit network or polling. Conflicts/cancelled or uncertain removal keep/report the stash and partial results; no automatic retry or pretend recovery sequence. [Product](product.md#stashes-0180) owns interaction, [architecture](architecture.md#stashes-0180) bounds/race limits.

## Execution and data handling

The established work environment is trusted. **Do not add extra safety measures, repository-trust modals, first-use approval layers or repeated consent for routine Git actions.** Use normal operation controls; confirm actual destructive scope and honor required authentication/host verification. This replaces older routine trust-gate requirements.

| ID | Requirement |
| --- | --- |
| E1 | Git/hooks/scripts/filesystem/parsing off UI thread; async processes, bounded streams/concurrency, incremental reads, virtualized views. |
| E2 | Coordinate related mutations/shared worktrees; reject stale results/recheck prerequisites. External clients are independent; app locks are not global transactions. |
| E3 | Show command progress/duration/output and known running/input/conflict states. Local Changes background reads have no bottom activity bar or ticking footer; retain static results/errors and cancellation, with command duration at top center. Provide visible routes for SSH/host keys/GCM/browser/signing/hooks/editor/pager/rebase input; silence alone never proves waiting. |
| E4 | Cancel owned descendants, preserve shared agents/browsers, reinspect after failure/cancel/crash/resume. Cancellation is not rollback; handle network/disk/permissions/locks without automatic destructive cleanup/retries. |
| S1 | No silent discard/clean/reset/rewrite/force push/hook bypass/global-config change. Confirm destructive targets/consequences; prefer force-with-lease. Reflog does not protect all untracked/uncommitted data. |
| S2 | Opened/restored history reads automatically with no trust/consent dialog or setting. Normal action button authorizes ordinary mutations/configured programs. Identity checks detect replacement/errors internally, not permissions; retain Git ownership checks without changing safe.directory. No unsolicited network actions. |
| S3 | Render repo/output/release-note text as data; structured arguments, literal paths, machine parsers; no shell interpolation. |
| S4 | Established credential storage; no plaintext secrets/shared embedded tokens. Redact sensitive output before export/logging; bounded diagnostics, arbitrary hook output can contain secrets. |
| S5 | No default telemetry/uploads/avatar requests/auto-fetch/unsolicited network. Confirmed exception: disclosed startup updater endpoint; configurable periodic checks, no repo content. Disclose normal HTTP metadata/access logs. Supersedes historical advice to disable update checks. |

## Localization, accessibility, and lifecycle

| ID | Requirement |
| --- | --- |
| L1 | Resource-backed complete menus/dialogs/errors/help/accessibility/install/update messages; plural rules, cultural dates/numbers, text expansion/RTL, direction-isolated code/paths/hashes, no sentence concatenation. Git output remains original. |
| A1 | Keyboard/context-menu/dialog access, restored visible focus, screen-reader semantics for custom graph/diff, noncolor status, light/dark/high contrast, DPI scaling. |
| A2 | Shared dark surface/state conventions from the supplied reference; semantic brushes, readable normal/selected text, disabled reasons and visible focus. Respect Windows high contrast; no theme-driven Git work. [Theme](theme.md) owns palette rules. First pass now; further polish/light-mode selection deferred. |
| B1 | Persist tab order/selection/settings; optional explicit tray/close/exit, no surprise autostart, minimal hidden/idle work. Local Git needs no hosted service. |

## Updates and distribution

Deferred implementation; these define the requested install-once experience, not prerequisites for unrelated Git features.

| ID | Requirement |
| --- | --- |
| U1 | In-place updates for ~30 Windows 10/11 coworkers; mature stack-compatible updater/static HTTPS, preferably per-user/no admin. No custom backend/service without need. [Flow](product.md#update-experience-confirmed). |
| U2 | Version/package/verification/notes in manifest; stable/test channels, small pilot, manual check, installed version/status/postpone. |
| U3 | Install only on explicit restart after checking all instances/sessions for Git/hooks/auth/signing/tracked tools/conflicts. Stop new admission before final check; unknown/busy blocks. No implicit next-launch/exit application. |
| U4 | Verify feed/package authenticity before execution; updater signatures differ from Windows signing. Protect keys; match package/metadata/channel/architecture/version; reject invalid signatures. HTTPS/hash alone is not publisher authentication. |
| U5 | Recoverable offline/timeout/proxy/auth/interruption/space/install/crash status. Failed check/download leaves installed app usable; reject HTML/login responses, never disable verification. |
| U6 | Preserve settings/tabs/drafts/recovery through schema migration/backups; document bad-release repair. Claim rollback only after updater and data-compatibility testing. |
| U7 | Approved private/offsite access without embedded shared secrets. Publish immutable packages before manifest; outage never disables installed app. |
| D1 | Clean uninstall/license notices/signed distribution options; preserve repos/Git/shared credentials/tools, explicit app-data retention choice. |

## Verification and acceptance

Build/headless evidence is distinct from desktop/live-auth/installed verification. Use meaningful parser/process tests and disposable real-Git integration fixtures with isolated config/test hooks; never destructive tests on working repositories. Live accounts/desktop launches require task authorization. Record date, OS/build/architecture/Git/runtime/fixture/results/limits. **A feature is complete only when its real action works within stated scope.** Mock/layout checks cannot replace Git semantics.

Historical phase gates below preserve regression coverage; old prototype/trust behavior is evidence only. Update assertions when implementing G1/G13/S2. Unrelated runtime matrices do not block independent requested functionality.

| Gate | Acceptance |
| --- | --- |
| F0 foundation | Clean restore/build; resizable shell, resources, async profile I/O, bounded diagnostics; invalid/newer schemas and failed/cancelled saves preserve bytes. Actual launch/layout/save/relaunch separately verified. |
| F1a metadata | Argument/stream caps, timeout/cancellation/descendant/inherited-pipe handling; valid Git discovery. Real unborn/detached/linked/conflicted metadata; automatic restored/open reads, replacement errors, drained shutdown, no stale tab completions. Published files/launch checked separately. |
| F1b history | CLI-equal topology/parent graph/columns/details, pinned bounded expansion/selection; empty/shallow/failure/malformed cases. No signature helpers/stale rows. Headless graph/virtualization plus separate desktop review. |
| F1c All Commits | Multi-ref/merge/annotated/nested-tag/packed-ref fixtures; all 2,005 commits reachable without duplicates/broken page edges. Detached/unborn/empty distinctions, no checkout from scope; changed refs require refresh; failed pages retain rows; full details/labels. |
| F1d automatic | Startup selected repo without clicks; fresh cached tab has zero Git calls. Retain view state/rows, bounded eviction, obsolete-result rejection and replacement checks. Author relation/selection/high contrast, normal/minimum layouts, measured timings. |
| F1e fast history | No history consent UI; native launch preserves args/env/exit/ownership. Custom wrapper works. Menu/MMB targets correct tab even active/last; no stale leaks. Equal scope/page benchmarks and packed 100k first/last pages. |
| F1f workspace/counts | No selector; history flush below tabs at normal/minimum sizes. Fixed scope restored from old preferences with detached HEAD. Real upstream synchronized/ahead/behind/diverged/unavailable cases; comparison never delays rows/leaks across tabs. Historical disabled controls are debt, not required acceptance. |
| F2a branch/network | Real create/switch/fetch/push/tags/upstream/fast-forward and rejected divergence on disposable remotes. Invalid/duplicate/dirty/sequence/stale refs/config/identity, hooks/remote failure, cancellation/shared exclusion. Exercise each dialog's normal Execute without a trust checkbox; missing targets have localized disabled-action help and unfinished options are absent. Reinspection/badges/headless layouts; live auth/Windows 10/accessibility separate. |
| F2b askpass | Operation-only packaged helper; real credential round-trip incl. Unicode/spaces. Endpoint/nonce/server validation, oversized/multiline rejection, bounded lifetime/count, cleared replies/no persistence. Cancel/exit/timeout closes prompts/reconciles. Masked accessible prompts and offscreen review; real provider/host/signing/focus separate. |
| F2c workspace actions | Pull through the actual bound dialog command closes options, shows main progress and imports a real peer commit/file; history/upstream counts update. Retain dirty/unavailable-remote errors and cancellation; no duplicate launch, premature admission or other-tab result leakage. Refresh failure distinguished; auth survives options closure; normal/minimum offscreen layouts. |
| F2d prompt dialogs/Pull | Measure subprocess preparation separately from first/warm offscreen layout. Open immediately editable from cached targets; opening/dismissal start zero processes, including dispatcher continuations. Configured unfetched remotes appear; stale refs fail after submission without network work. Submission validation failures/cancellation remain inspectable. Real divergent rebase and merge, tracked staged/unstaged auto-stash restoration, untracked/ignored protection, config overrides, conflicts surviving reinspection/reopen, zero-exit stash conflicts and recovery guidance. Verify superproject pointer updates preserve dirty submodule worktrees and existing cone sparse patterns remain unchanged. Preserve F2c cancellation/admission checks; no desktop timing claim from headless evidence. |
| Reads | CLI-matching status/index/worktree/files/diffs: empty/detached/binary/rename/delete/ignored/EOL/unusual paths. |
| Mutations/recovery | Expected refs/index/files, unrelated data intact; stale hunks/external edits; conflicts/interrupted sequences survive reopen with valid actions. Owned cancellation/reinspection. |
| Sequence recovery (0.17.0) | Real merge/rebase/cherry-pick/revert Continue/Skip/Abort; unresolved Continue refuses, merge has no Skip. Reopen detects state, including sequencer-only stops; stale HEAD/markers fail rather than operating on a replacement sequence. Confirm Abort/Skip targets; cancellation/failed hooks/locks retain recoverable state and raw output. Verify later conflicts, auto-stash conflicts after exit zero, actual index/refs/files and unrelated content. Cached controls/render/declined confirmation start zero Git calls; command validation has no ref enumeration/remote query/worktree scan. Offscreen normal/minimum review; desktop latency/accessibility remains separate. |
| Stashes (0.18.0) | Disposable create/inspect/apply/pop/drop, split staged/unstaged state, optional untracked/binary/Unicode and ignored preservation; conflict/collision failures retain stash and files. Reject replaced identity/stale list; cancellation/concurrent insertion after apply never retries or removes shifted entries. Exact-target Drop confirmation can be declined without Git. Cached/warm modal layout starts zero queries; background generations/close reject late results, failed refresh retains rows, bound commands and shell reconciliation work. Review normal/minimum offscreen renders; desktop opening/p95 remain separate. |
| Local Changes / commit | Disposable unborn/partially staged/rename/delete/binary/Unicode/dash/whitespace/conflict fixtures; correct index vs worktree diffs and byte sizes, no read-time filter/external-diff helpers or index writes. File/all stage/unstage, initial/ordinary/amended/merge commit, failed hooks, stale index, lock preservation, active cancellation and local-bare Commit and Push/rejection. Modal construction makes no Git calls; cached rows/draft/history survive opening, failed refresh retains rows and disables stale writes, late diffs cannot replace new selection. Verify modifier commands/actual amend target, unique count, large-list/diff virtualization and normal/minimum renders; desktop timing/keyboard remains separate. |
| Local Changes menus (0.19.0) | Right-click keeps containing multiselection or selects the clicked row; blank background has no file actions. Menu opening initiates zero Git/file handoff, deferring new selection diff; captured selected/all index actions remain independent of Shift and reject refreshed contexts. Real stage/unstage preserves files/draft. Verify relative/full copying, exact working-file/Explorer dispatch, deleted-file fallback, localized errors, close cancellation and normal/minimum/menu offscreen renders. External app/clipboard/physical keyboard behavior requires separate authorized desktop verification. No unfinished entries. |
| Selected hunks | Apply only chosen complete blocks to the index in either direction, preserving working-file bytes and unrelated staged content. Exercise partially staged files, disjoint/offset blocks, Unicode/dash paths, CRLF/EOF, config overrides, stale index/file/HEAD/attributes/config, lock/precheck failure and cancellation. No whole-file fallback; unsupported types/options have localized explanations. Bound command/selection/draft/reinspection, accessible block identity, virtualization and normal/minimum offscreen layouts; desktop interaction separately verified. |
| Committed files | Root/ordinary/empty/merge-parent comparisons; text/binary/rename/add/delete/Unicode/EOF/large/missing objects and configured submodule pointer handling. Never include dirty index/worktree bytes or treat a shallow boundary as a root. Real bound selection, parent changes, stale cancellation, retained refresh errors, no preloading before Changes is opened and normal/minimum renders. Shared M/+/− markers preserve selected/high-contrast/accessibility meanings; historical files must not be described as staged. |
| Local action latency | Repeatable isolated before/after measurements of validation/execution/fresh target rows/diff, not just Git process time. Hold obsolete diff/full-status reads and verify next file action still completes; reject their late results. Reconcile actual target status before coalesced background full scan; preserve unrelated rows and same-path selection. Failed background reads retain successful target rows/output but mark stale. Keep index/HEAD/worktree preservation, locks, cancellation and stale-state tests. No unrelated ref/remote enumeration in index-action preflight. Distinguish target-row readiness from full scan and desktop p95. |
| Binary comparisons | Old/New extension cards with formatted size and exact bytes in working-copy and committed comparisons; explicit absent vs zero-byte files. Recognized v1 LFS pointers show logical size and LFS meaning without downloading or running helpers. Test pointer/index/worktree/parent sizes, stale selection and localized accessible meanings. Review normal/minimum renders with no bottom activity bar; native keyboard/high contrast remain separate. |
| Failures | Hook/signing/auth/network/disk/permission/lock failures preserve usable UI/diagnostics without destructive retry. |
| Routine consent/privacy | No repository-trust/first-use/program-consent layer for ordinary actions. Background reads request no network/helper; secret fixtures absent from retained logs. Destructive/auth prompts remain targeted. |
| Dark theme | Verify shared resource loading, normal/secondary/selected/accent contrast, live system-color mapping and dark restoration; selected/disabled/editable controls keep real state and bindings. Review history/Local Changes/action dialogs at normal/minimum sizes offscreen. Desktop hover/keyboard, OS high contrast, native chrome and mixed DPI remain separately verified. |
| Accessibility/localization | Windows 10/11 keyboard/screen reader; 100/150/200%/mixed DPI; +40% pseudo-localization/RTL without inaccessible/clipped actions. |

### Proposed performance budgets (not measurements)

Document 4-core/8-GB/SSD reference host, antivirus/cache conditions and child peaks; >=20 runs for p95. Cold after reboot, warm after prior launch; distinguish shell from Git load.

| Metric | Proposed limit |
| --- | --- |
| Interactive shell | Warm p95 <=1 s; cold <=2.5 s |
| Status + 200 rows | p95 <=2 s on 10k-file/100k-commit fixture |
| Idle private bytes | <=180 MiB one repo; <=300 MiB ten tabs/inactive views unloaded |
| Idle CPU | <=0.2% total average/5 min; zero periodic Git launches unchanged |
| Input | p95 <=100 ms; no app UI-thread stall >200 ms |
| Single-file Stage/Unstage (2k files, 1,001 refs, 256 MiB unchanged assets) | Median affected rows <=750 ms, selected diff <=1.75 s; report full scan separately. Three-sample regression budget, not desktop p95 or a guarantee for selected large files/filters/storage |
| Operation options (2,500 commits/one remote) | Warm offscreen open <=100 ms, first construction <=1 s, local targets editable on first layout with zero opening-time Git calls; desktop/p95 validation still required |
| Installer | Self-contained compressed x64 <=100 MiB, excluding Git |
| Scaling | Interactive million commits, bounded caches/no full-history UI allocation |

### Update acceptance matrix

Real signed N → N+1, agreed Windows 10/11 builds, standard user, both channels, disposable profiles/repos and controlled endpoint.

| Case | Required result |
| --- | --- |
| Success | Explicit restart installs verified N+1 in place, correct version/status, no routine uninstall/elevation where supported. |
| Postpone | N usable; no later idle/exit/startup install without explicit action. |
| Hosting | Offline/DNS/timeout/5xx/proxy/401/403 leaves startup/Git usable; manual retry, no tight loop. |
| Invalid content | Reject altered feed/package, wrong signer/architecture/channel, truncation/login HTML before execution. |
| Interrupted download | Cancel/loss/crash leaves N usable; resumed/restarted bytes fully reverified. |
| Data | Preserve settings/tabs/drafts/recovery; migration backup; invalid/newer schema not overwritten. |
| Busy/race | Git/hook/auth/tool/conflict blocks, including work starting before shutdown or in another instance. |
| Resource/install failure | Low disk/locked files/permissions/installer/app crash preserves data; documented repair, no untested rollback claim. |
| Bad release/channel | Test pilot isolated from stable; withdraw manifest, higher corrective release and offline repair tested. |
| Publication | Package verifies before manifest; signature mismatch rejects, later retry recovers; installed version usable. |
