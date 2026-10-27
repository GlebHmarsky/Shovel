# Architecture and distribution

Updated 2026-10-27. C# / WPF / .NET 10 LTS confirmed. Runtime through 0.9.3, applying the interaction policy to F2b actions. [Requirements](requirements.md) owns obligations, [TODO](../TODO.md) tasks, [roadmap](roadmap.md) decisions/evidence. Future designs below are proposals.

## Product identity

Shovel confirmed 2026-10-24; `Shovel.exe/.dll`, product metadata/resource title since 0.3.1. Keep internal GitGui projects/namespaces/resource base/manifest/infrastructure identity and profile mutex/path for compatibility. Publisher/permanent install/data identity remain open; rename introduced no installer/migration.

Selected icon (2026-10-25, shipped 0.4.2): front-facing silver shovel on blue circle; PNG source and ICO in App/Assets. ICO has transparent 16/20/24/32/40/48/64/128/256 frames; proportional scaling/padding. ApplicationIcon embeds it; main-window WPF resource uses assembly-qualified `/Shovel;component/Assets/Shovel.ico`.

## Stack and boundaries

SDK 10.0.401/Desktop pack 10.0.12 were installed; no external NuGet packages. Git for Windows CLI, no libgit2/TypeScript/web server/plugin framework/database. Keep explicit composition, small command/observable bases, typed operations/immutable snapshots; add mature dependencies only for concrete need. Core stays independent of WPF/Windows process APIs.

Historical platform baseline: x64 Windows 10 22H2/11, ARM64 later, no x86 plan; validate fleet/editions before support claims. .NET 10 LTS policy recorded through 2028-11-14; refresh servicing before distribution. [Support policy](https://dotnet.microsoft.com/en-us/platform/support/policy/dotnet-core), [Windows editions](https://learn.microsoft.com/en-us/dotnet/core/install/windows). [Stack research](research/desktop-stack-reassessment.md) holds alternatives, not measurements.

| Implemented location | Responsibility |
| --- | --- |
| GitGui.slnx / global.json / Directory.Build.props | Solution, SDK pin, version/build defaults |
| App/App.xaml(.cs) | Composition/startup/shutdown/profile mutex/error exit |
| App/Views/MainWindow.xaml(.cs) | Layout/view geometry/MMB dispatch |
| App/Views/GitActionWindow.xaml; ViewModels/GitActionViewModel.cs | Owned modal/options/progress/one action attempt |
| App/Views/AuthenticationWindow.xaml | Masked/confirm/informational owned prompts |
| App/ViewModels/ShellActions.cs | Preparation/admission/reconciliation |
| App/ViewModels/ | Shell/tab/row presentation and command/observable bases |
| App/Views/CommitGraphCell.cs | Draw precomputed segments for realized rows |
| App/Services/UserInteraction.cs | Owned folder/executable selection/confirmation |
| App/Localization/UiText.cs; Resources/Strings.resx | Resource lookup/formatting/XAML extension/English text |
| Core/Settings; Diagnostics | Schema-4 store; typed bounded events/stage timings |
| Core/Git/RepositoryReader.cs; CommitHistory.cs | Discovery/metadata; ref snapshots/paged NUL parser |
| Core/Git/GitActions.cs; CommitGraph.cs | Command plans/revalidation/shared lock; parent lanes/cursors |
| Core/Processes/ | Typed runner request/result/contracts |
| ProcessHost/ | Native Windows runner/jobs/filesystem identity; AskPassSession/Client/Protocol |
| AskPass/ | Console-free Shovel.AskPass.exe protocol helper |
| tests/GitGui.FoundationChecks/ | Disposable-file/Git/process/view-model/headless WPF checks |
| scripts/Build-Preview.ps1 | Checked publish into ignored Build/<version>-win-x64 |

Project paths above are under `src/GitGui.*` unless shown otherwise. App references Core/ProcessHost and packages AskPass.

Future logical boundaries, not extra existing projects: UI/view models; application workflows; Git adapter/parsers; Windows process/prompt/tool services; persistence/resources/cache; updater coordinator. Do not add abstraction layers without use.

### Delivery and interaction policy

2026-10-27 replaces UI-first/prototype-retention and routine trust requirements. Implement each requested action through Git/tool execution and results; no new no-op controls. Existing prototypes must be wired or omitted as their area is delivered; real state-dependent unavailability remains valid.

Established work environment: no repository-trust dialog, first-use modal or repeated program-approval checkbox. Ordinary Execute authorizes the selected action/configured Git behavior. **Do not add extra safety measures or approval layers without a concrete requirement.** Keep normal Git data-loss protections, required auth/SSH host verification, structured arguments, cancellation/state handling and targeted destructive confirmation.

0.9.3 removes the approval flag from ActionOptions and the consent checkbox/view-model state. RunCommand validates actual targets only; execution still happens solely after the normal action. Shell/action view models expose localized reasons through disabled-control tooltips/accessibility help. Unfinished toolbar/sidebar/dialog controls and the no-op navigation setter are removed; the legacy settings field remains readable and saves History.

## Startup, lifetime and settings

App shows loading shell, loads settings off-thread and automatically reads selected restored tab. Schema-4 identity stamps are replacement detection, not consent. 0.7.0 ignores legacy narrower scopes and writes All Commits. Identity writes serialize with settings; failure is visible without hiding history.

Closing stops new inspections/navigation, cancels/drains startup/reads, captures plain layout on dispatcher, saves off-thread and bounds diagnostic drain. Save failure offers stay/retry or explicit exit without saving. Dispatcher/startup failure shows localized generic error, exits without saving invalid state; fatal logging best effort. Per-profile mutex excludes app writers. No tray/autostart/polling/startup fetch exists.

Current explicit-modal preparation/execution/reconciliation blocks main close/tab navigation. Running modal cannot close; cancellation drains then reinspects. Common-directory mutex coordinates linked-worktree actions. This is not the future updater/external-tool idle coordinator.

Settings: `%LOCALAPPDATA%\GitGui\Foundation`, or explicit absolute `--data-directory`; separate from binaries. Stores layout/navigation, up to 20 tab paths/order/selection, legacy scope and identity stamps; no history/credentials. 64 KiB input/output, normalized values. Schema 1/2/3 → 4; previous bytes backed up on atomic replace. Same-directory temp, cancellation check, move/replace; external change since load blocks saving. Invalid/newer/unsupported/inaccessible files stay untouched. No general migration engine/recovery/export UI or automatic backup restore; older previews open newer schemas read-only. Separate profiles for divergent pilot schemas. Future drafts/tools/update/recovery fields/caches need versioning and bounded rebuildable data. Rediscover state after restart; never replay commands. [Recovery instructions](../README.md#local-data-and-recovery).

Diagnostics: typed event/failure codes + UTC only; 128-entry queue, overload drops; two files ~256 KiB each (~512 KiB total, one-event overshoot). No arbitrary exception/stack/path/argument/credential/repo/output logging. Worker logging failure is nonfatal. Stage timings and original Git stderr are bounded, in-memory session data.

Localization: neutral English .resx/ResourceManager, `{loc:Text Key=...}`, UiText lookup/whole-message formatting; CurrentCulture for dates/numbers, UI language English. FlowDirection follows UI culture; version isolated, raw paths/hashes/output LTR. Localized accessible names; OS supplies system button captions. Pending: language selector/satellites, plural-aware formatter before first plural message, pseudo-localization/RTL/themes/installer text and complete mixed-text bidi verification. Modern WPF cannot reuse Framework-only LocBaml unchanged. [WPF localization](https://learn.microsoft.com/en-us/dotnet/desktop/wpf/advanced/wpf-globalization-and-localization-overview).

## Implemented Git metadata and process execution

Discovery: standard Git for Windows roots or exact user-selected executable, not PATH/current-directory Git. Prefer `ucrt64/bin/git.exe`, then mingw64/mingw32, fallback cmd/git.exe; custom choice session-only. Floor 2.47.0; tested 2.51.1/mingw64 only. No HOME/PATH/global-config/install edits. Official sources checked 2026-10-25: [wrapper](https://gitforwindows.org/git-wrapper.html), [path transition](https://github.com/git-for-windows/build-extra/blob/main/ReleaseNotes.md).

### Automatic local history

Current `WindowsRepositoryIdentity` validates before Git: root on local fixed drive, no reparse components in root/.git/resolved Git/common directories. UNC/removable/reparse/subfolder opening currently unsupported; these are existing restrictions, not a mandate to add barriers. Pointer files (.git/commondir) <=8 KiB. Stamp = normalized paths + volume serial/128-bit file IDs/creation times + linked pointer contents. Detects ordinary replacement/redirection, not hostile in-place changes/reused IDs/profile tampering/atomic swaps; not a sandbox. [File identity](https://learn.microsoft.com/en-us/windows/win32/api/winbase/ns-winbase-file_id_info), checked 2026-10-25.

Open/restored tabs read without consent. Schema-3 approval fingerprints become stamps; schemas 1/2 and formerly unapproved tabs load automatically. Save stamp after successful read. Tab activation validates off-thread; missing/inaccessible/mismatched clears rows/stops. Explicit reopen binds replacement without another question; closing removes stamp/tab only. Check Git-reported directories against identity and validate after read. No earlier stamp exists on first read.

Read audit: version/rev-parse/symbolic-ref/for-each-ref/log/rev-list only. Structured arguments, fixed executable, closed stdin, no pager/optional locks/lazy fetch/fsmonitor/auto-maintenance. Suppress patch/external diff/textconv/signatures/notes/mailmap; request no hooks/filters/credential helpers/editors/transports. Config/includes can be read; strip inherited redirects/traces except deliberate config locations. Git ownership checks remain, no safe.directory edits. Marker helpers/missing partial-clone objects tested; no implicit fetch/repair. Sources checked 2026-10-25: [Git invocation](https://git-scm.com/docs/git), [rev-parse](https://git-scm.com/docs/git-rev-parse).

`rev-parse` reads locations once; `for-each-ref --include-root-refs` observes HEAD/full symbolic branch/upstream/named refs together. Unborn HEAD falls back to symbolic-ref. Operation markers before/after enter fingerprint. Initial/refresh reuse initial refs, validate HEAD/refs/boundaries/markers after log. Normal first page: five Git launches including discovery; refresh four; available upstream comparison adds one after rows. Nested tags/unborn add calls. No loose/packed/reftable parser. Bare repos rejected. Snapshots detect observed changes, not ABA/atomic transactions. [2.47 include-root-refs](https://git-scm.com/docs/git-for-each-ref/2.47.0).

`WindowsProcessRunner`: direct CreateProcessW/STARTUPINFOEX; atomically attach kill-on-close per-command Job Object through JOB_LIST; HANDLE_LIST inherits stdin/stdout/stderr only. Ownership setup failure stops launch. ProcessHost is a library, no per-command .NET helper/service. Windows CRT argument encoding, explicit executable/cwd/Unicode environment, no shell; tested empty/quotes/backslash/Unicode/metacharacters/env removal. Closed stdin; concurrent worker pipe reads; registered exit wait. Concurrency two, one UI read chain. Default caps 256 KiB/stream, history 4 MiB; 15 s/command, 45 s/read chain. Overflow/cancel closes job, waits/drains; normal exit removes leftover owned descendants. Raw errors stay in memory. Sources checked 2026-10-25: [attributes](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-updateprocthreadattribute), [creation](https://learn.microsoft.com/en-us/windows/win32/procthread/creating-processes), [pipes](https://learn.microsoft.com/en-us/windows/win32/ipc/pipe-handle-inheritance).

Newest read cancels/drains obsolete chain; navigation allowed during reads, generation/cancellation/identity rejects late results. `HistoryViewState`: page/scope/selection/cursors/scroll/time/error/output. Cache three visited pages, conservative 32 MiB character estimate, 128 back cursors/tab; active page bounded by read cap. First history page always available; eviction clears/reloads first page, no preload. Tab activation validates identity then skips Git if <30 s; stale window reactivation refreshes, manual refresh overrides. No watcher/timer; external edits in continuously active window need Refresh. Refresh retains page for unchanged refs/HEAD, else page one/selected hash if present. History state session-only; failure retains stale rows. Nested stage timings overlap; do not double-count.

### Local upstream comparison (F1f, 0.7.0)

Existing ref observation records full symbolic HEAD and `%(upstream)` in fingerprint. After rows: `rev-list --left-right --count <HEAD-hash>...<upstream-hash> --`; parse two nonnegative invariant integers as ahead/behind. Both buttons use configured upstream, never guessed origin or separate push target. 4 KiB/stream, existing deadlines; no fetch. Immutable hashes bind counts to rows; shallow deliberately unavailable, missing ancestry fails without lazy fetch. Detached/unborn/missing upstream skips command. One session comparison/page, reused for paging/warm tabs; refresh recomputes. Generation/cancel rejects obsolete results; failure retains rows/raw stderr, cancellation becomes unavailable. Timer distinguishes comparison. Config/boundaries can race externally; no transaction/server freshness. Sources checked 2026-10-25: [upstream](https://git-scm.com/docs/git-for-each-ref#Documentation/git-for-each-ref.txt-upstream), [count](https://git-scm.com/docs/git-rev-list#Documentation/git-rev-list.txt---count).

### Commit history and graph (through F1f, 0.7.0)

0.3.x walked HEAD/capped 2,000; no filter hid commits. F1c added All Commits; 0.7.0 uses `log --topo-order --branches --remotes --tags` plus inspected HEAD hash. Avoid `--all` (includes stash refs). UI has no selector; Core retains current-HEAD traversal for tests/future filtering. No reflog/unreachable/stash-only tips. [Git log](https://git-scm.com/docs/git-log), checked 2026-10-24.

`OpenHistoryAsync` fingerprints sorted HEAD/heads/remotes/tags/replacement refs and bounded shallow/graft files. Peel annotated/nested tags recursively using validated hashes; non-commit tags have no row. Full labels in details/help. `ReadInitialHistoryAsync` reuses pre-read observation then validates after; `ReadHistoryPageAsync` checks before/after, reads 200+lookahead with --skip/expected boundary hash. Changed snapshot requires refresh. HEAD pinned, labels from refresh; I/O/parsing/layout/formatting on workers. [Ref formatting](https://git-scm.com/docs/git-for-each-ref).

No total count cap; one message page, compact offset/lane cursors (128 back states). Deep --skip rewalks can hit deadlines. Ref/output/boundary caps 4 MiB, width 64 lanes; visible range/help, error retains page rather than false end. Missing objects fail; shallow boundary distinct.

NUL UTF-8 fields retain full message/parents/offset dates. `CommitGraph.BuildPage` carries parent lanes/colors and validates batches. Headerless fixed-order recycling DataGrid; Header resources become cell help, graph cells draw only. Bounded row subjects/labels, complete parsed selected details, finite message scroll viewport. History reserves 300 DIP; details default 180/minimum 80 DIP, old layout preserved in schema 4. Simple original XAML paths; no Fork assets. Dark history/white text/yellow exact-author peers with system high-contrast fallback; selected/ref/graph colors independent. Keyboard/DPI/RTL/high contrast still need actual certification.

Tab context menu passes placement RepositoryTab to parameterized commands. Code-behind delegates MMB close; active close changes generation/cancels/selects neighbor, inactive preserves selected read. Empty selection clears busy while owned work drains. Page back restores hash/scroll. Filtering/search/files/diffs/broader mutations pending.

### Deferred execution work

Status/diffs need porcelain-v2/NUL/literal pathspecs and bounded content without external diff/textconv/lazy fetch. Broader browser/signing/interactive-hook/editor/tools need visible input and tracked lifecycle; do not silently bypass hooks. F2a provides serialization/revalidation/reinspection, F2b askpass. These dependencies apply to those workflows, not a blanket prohibition on other requests.

## Authentication bridge (F2b, 0.9.1)

App composes `AskPassSession` for explicit network execution; Core receives IAuthenticationSession, no WPF. Packaged Shovel.AskPass.exe forwards prompt/reply over operation pipe/stdout; never history/preparation, no permanent listener/dependency. Existing credential helpers/agents remain and may store credentials. [Git protocol](https://git-scm.com/docs/gitcredentials), checked 2026-10-26.

Pipe: CurrentUserOnly/FirstPipeInstance, random name/256-bit nonce/server PID/account-elevation checks. Length-check before allocation: prompt 16 KiB, reply 4 KiB; UI <=1,024 UTF-16 units, rejects NUL/newline. One prompt/connection at a time, <=16/operation; connect 5 s, prompt 2 min, command 5 min. Sources checked 2026-10-26: [pipe options](https://learn.microsoft.com/en-us/dotnet/api/system.io.pipes.pipeoptions?view=net-10.0), [PID](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-getnamedpipeserverprocessid).

Endpoint/nonce inherited only by action tree, expires on disposal; not a sandbox against same-user/inheriting helpers/replaced binary. Plain redacted request cannot authenticate remote identity. PasswordBox/SecurePassword/disposable UTF-8 buffers cleared after send; no response string binding/settings/logging. Cannot guarantee erasure of OS/native/managed/Git copies. Raw helper output in memory only.

Bundled OpenSSH with broker: BatchMode=no, StrictHostKeyChecking=ask, SSH_ASKPASS_REQUIRE=force. No automatic host acceptance/known_hosts removal. Unknown-host text has no structured type: display/typed answer, no translated-text guessing. SSH_ASKPASS_PROMPT=confirm: explicit Approve; none: notification dismissed on helper exit without cancelling SSH (key touch). Other helper exits/cancel stop operation. Signing/custom SSH pending. Sources checked 2026-10-26: [environment](https://man.openbsd.org/ssh.1), [hints](https://github.com/openssh/openssh-portable/blob/master/readpass.c), [host confirmation](https://github.com/openssh/openssh-portable/blob/master/sshconnect.c).

Git stdin closed, GIT_TERMINAL_PROMPT=0; GCM_INTERACTIVE=never allows cached use while suppressing GUI/browser. Credential debug/traces suppressed. Browser route must track external lifecycle so job cleanup cannot kill browser; GCM interactive/modal-parent options are future candidates, no global config changes. [GCM environment](https://github.com/git-ecosystem/git-credential-manager/blob/main/docs/environment.md), checked 2026-10-26.

Input-wait state exists only while prompt callback active. Prompt cancel/expiry cancels operation; broker disposal completes before reconciliation/navigation. Localized pipe/timeout/limit failures; helper file/pipe setup off dispatcher. Provider/desktop interoperability remains unverified.

## Explicit Git actions (F2a, 0.8.0)

`GitActions` prepares immutable identity/HEAD/refs/effective redacted fetch-push addresses/config hash off-thread on dialog opening, not startup. <=32 remotes, single fetch/push URL each; unsupported config explained. GitActionViewModel edits options/one attempt; ShellActions blocks navigation, invalidates related caches and reinspects after every outcome.

Normal Execute authorizes the selected operation and configured hooks/helpers; no separate approval flag, checkbox or replacement trust modal. Identity/paths/HEAD/refs/config revalidation and sequence checks remain correctness handling. Common-directory named mutex coordinates Shovel in one Windows login; acquire/release same worker, abandoned lock requires fresh validation. Other sessions/clients/aliases/launch races outside lock; Git locks authoritative, never deleted. Hashes do not attest helper contents.

| Action | Current command behavior |
| --- | --- |
| New branch | branch --no-track or switch --create --no-track; validated name/immutable start hash, duplicates fail. |
| Switch | --no-guess, no force/discard; --no-overwrite-ignore. |
| Checkout/Pull | Require clean index/worktree incl. untracked; protect ignored collisions, reject submodule/sparse; Git enforces other-worktree occupancy. |
| Fetch | Explicit remote and +refs/heads/*:refs/remotes/<remote>/*, empty --refmap=, no prune/submodule recursion; rewritten tips update tracking only. |
| Pull | Fetch selected branch, recheck HEAD/config/identity/clean state, cross-check FETCH_HEAD/tracking tip, merge --ff-only --no-autostash --no-edit. Reject nonempty branch.mergeOptions before fetch (could change semantics). |
| Push | Explicit source/destination refs, no implicit mirror/follow-tags/submodule push/force. Tags/upstream opt-in; hooks/filters preserved. |

Pull's two commands are not transactional; failed merge may leave fetched objects/refs. Sequential fetch-all/multiple push refs can partially succeed. Git 2.51.1 local remotes tested; sources checked 2026-10-25: [fetch](https://git-scm.com/docs/git-fetch), [merge](https://git-scm.com/docs/git-merge), [push](https://git-scm.com/docs/git-push), [switch](https://git-scm.com/docs/git-switch).

Writes: existing runner, 5 min/command, 1 MiB output; decode/redact off dispatcher, modal memory only. HTTP(S) userinfo/query secrets redacted; arbitrary helper output may still contain secrets. Show running/duration/cancel; no wait inference from silence, automatic retry or rollback claim.

F2b supersedes cached-only auth. Core without broker stays noninteractive/strict known-host; custom core.sshCommand/unrecognized bundled layout rejected. Default transports file/http/https/ssh/git allowed, others denied unless explicit Git per-protocol permission overrides. Startup requests none. RefBadge reuses CommitRef.Upstream, no extra startup query; original vectors inherit row/system colors, preserve virtualization. No provider/avatar network assets.

## Local preview delivery (implemented)

Central version + one changelog/commit per delivery; minor capability, patch fix/docs, deliberate major incompatible released contract. Script restores/builds/checks/publishes framework-dependent x64 to unique staging, verifies required files, moves to ignored Build/<version>-win-x64 without overwrite. START-HERE/checksums included; hashes detect accidental changes, not signatures. Historical 0.3.1 cleanup was one-time. Runtime required; no installer/update service/hosting/signing. No distribution claims from preview packaging.

## Updater comparison: current official evidence

Historical research, not a selected provider. WPF has no universal updater; library documentation cannot prove installed lifecycle/signature/localization behavior. Recheck pinned APIs before implementation.

| Candidate | Fit | Open integration work |
| --- | --- | --- |
| NetSparkle + per-user installer | .NET/WPF/static appcast; strict Ed25519 feed/package verification | Own scheduling/idle gate; separate installer must prove repair/failure/consent. NSIS user-level/offline self-contained is proposed, no forced closes or assumed rollback. |
| Velopack | C# SDK/static hosting/LocalAppData/package lifecycle/signing | Verify whole-feed/package authenticity/signer policy; disable documented default pending-update application on startup. Store user data outside replaced install directory. |
| MSIX / App Installer | Windows signed packages/per-user deployment under policy | Device certificate trust/rotation, sideload/App Installer/build compatibility; scheduling differs from Git-aware idle gate. No activation block/forced shutdown; prove programmatic consent integration. |

Sources: [NetSparkle](https://github.com/NetSparkleUpdater/NetSparkle), [NSIS privilege](https://nsis.sourceforge.io/Reference/RequestExecutionLevel); Velopack [intro](https://docs.velopack.io/), [integration](https://docs.velopack.io/integrating/overview), [installer](https://docs.velopack.io/packaging/installer), [signing](https://docs.velopack.io/packaging/signing), [data](https://docs.velopack.io/integrating/preserved-files); Microsoft [update options](https://learn.microsoft.com/en-us/windows/msix/app-installer/update-settings), [programmatic deployment](https://learn.microsoft.com/en-us/windows/msix/non-store-developer-updates), [in-use servicing](https://devblogs.microsoft.com/insidemsix/msix-servicing-while-in-use/), [signing](https://learn.microsoft.com/en-us/windows/msix/package/signing-package-overview), [troubleshooting](https://learn.microsoft.com/en-us/windows/msix/app-installer/troubleshoot-appinstaller-issues).

App Installer background check documented at eight hours; option minimum OS varies. Do not enable UpdateBlocksActivation. OS package-in-use cannot identify conflicts after commands exit. Internal/self-signed certificate provisioning may need IT/admin; code signing does not guarantee no SmartScreen prompt.

## Proposed update mechanics

Use selected framework manifest, not custom protocol. U2 plus proposed channel/OS/arch/minimum version-schema/size/hash/signer/verified notes. Supported schemas/HTTPS only; reject redirects/login pages/downgrades/replay below installed or trusted version. Verify before executing; render notes as plain/inactive text.

Check after shell startup with timeout/backoff, no overlapping periodic checks. Bounded staging, full reverify after resume; cancel discards/quarantines partials. No auto-install on download/exit/startup.

Explicit Restart and update: cross-instance shutdown gate stops new work, rechecks all repos/commands/prompts/tools/sequences, persists session, revalidates package and hands off only if idle. Unknown tool completion needs verification/acknowledgment. Installer cannot force-kill/race new launch; save failure cancels shutdown. Unresolved startup recovery blocks install.

### Signing and bad-release recovery

Updater signatures (e.g. Ed25519) authenticate feed/package via bundled public key; Authenticode/MSIX establishes Windows publisher trust; HTTPS is transport. Keep private keys out of repo/app/artifacts/logs, with restricted CI/hardware storage and backup/rotation/compromise procedure; providers/budget/IT trust open.

Retain known-good artifacts/schema backups, pilot first. Withdraw bad manifest, keep clients usable, issue higher-version corrective release compatible with affected schema. Separate reachable signed repair installer for nonstarting app; no routine uninstall. Explicit downgrade/data restoration only; no unverified automatic rollback/power-loss atomicity.

## Hosting and release operations

| Host | Operation/access tradeoff |
| --- | --- |
| Personal PC | Sleep/restart/address/bandwidth/owner dependency; TLS/backups upkeep. Office/VPN unless managed public HTTPS/DNS/firewall is deliberately configured. |
| Company server | Existing monitoring/backups preferred; internal DNS/certificate/network/VPN dependency; approved public access or VPN offsite. Browser login alone may not authorize updater. |
| Managed static/object | Proposed low-maintenance choice; cost/provider policy. Public HTTPS offsite; private needs supported user auth/gateway/network, tested proxy/token expiry. |

Unlisted URLs are not private. Azure static website endpoint lacks auth; private hosting needs different service. [Azure limitations](https://learn.microsoft.com/en-us/azure/storage/blobs/storage-blob-static-website). Reuse company identity, no embedded PAT/password. Host/access undecided; outages affect update discovery/download only.

Proposed release: build/test → sign immutable packages → upload packages/verified notes/metadata → verify public-facing download → test pilot → publish stable manifest/signature atomically. If pair cannot be atomic, reject mismatch/retry later. Short feed cache, immutable artifact URLs, retained earlier packages; licenses/migration/repair/standard-user upgrade evidence before distribution.
