# Architecture and distribution

Updated 2026-10-23. **Confirmed stack: C# + WPF**, selected by the user. **Implemented through F1b:** .NET 10 LTS, MVVM shell, local settings/resources/diagnostics, read-only Git metadata and commit history. Other designs below remain proposals unless identified as implemented. User obligations live in [requirements](requirements.md); tasks in [TODO](../TODO.md); evidence and unresolved decisions in [roadmap](roadmap.md).

## Stack and boundaries

C# / WPF is confirmed; .NET 10 LTS was selected using the installed SDK 10.0.401 and Windows Desktop pack 10.0.12. The official policy lists .NET 10 support through 2028-11-14; refresh servicing patches before distribution. [Microsoft support policy](https://dotnet.microsoft.com/en-us/platform/support/policy/dotnet-core). Git for Windows CLI is the implemented metadata adapter; no libgit2 dependency. TypeScript is excluded. [Stack research](research/desktop-stack-reassessment.md) preserves alternatives and rationale, not measured performance claims.

Proposed baseline: x64 Windows 10 22H2 and Windows 11; ARM64 later. Confirm actual fleet before claiming support. Runtime compatibility and Microsoft's supported OS editions are separate: [current .NET Windows support](https://learn.microsoft.com/en-us/dotnet/core/install/windows) limits Windows 10 support to particular LTSC/Enterprise editions.

The following are **future logical boundaries**, not six projects already created:

| Boundary | Owns |
| --- | --- |
| WPF UI/view models | Views, command bindings, focus, accessible graph/diff presentation; no Git orchestration |
| Application core | Typed operations, previews, safety/idle gates, immutable snapshots, workflow state |
| Git adapter | Capability checks, command construction, byte parsers, discovery and state queries |
| Windows infrastructure | Process host, prompt bridge, owned descendants, watchers, external-tool tracking |
| Persistence/resources | Versioned settings/session state, caches, diagnostics, localized messages |
| Update coordinator | Scheduling/status, verifier/updater adapter, shutdown handshake; cannot bypass core idle gate |

Keep one primary language, minimal dependencies, pinned SDK/packages, reproducible build/test/publish commands. No embedded web server, plugin framework, or database initially.

### Implemented structure (F0 + F1a/F1b)

```text
GitGui.slnx / global.json / Directory.Build.props
src/GitGui.App/                 WPF executable, composition and lifetime owner
  App.xaml(.cs)                Startup/shutdown, profile mutex, guarded error handling
  Views/MainWindow.xaml(.cs)   Layout and view-only geometry handling
  Views/CommitGraphCell.cs     Visible-row drawing only; no Git/graph computation
  ViewModels/                  ShellViewModel, RepositoryTab, CommitRow and small command/observable bases
  Services/UserInteraction.cs  Owned folder/executable selection and session-trust dialogs
  Localization/UiText.cs       Resource lookup, formatting, XAML markup extension
  Resources/Strings.resx       All app-owned English messages/labels
src/GitGui.Core/                UI-independent net10.0 library
  Settings/                    Schema-2 layout/tab model and bounded JSON store
  Diagnostics/                 Allowlisted events and bounded asynchronous log writer
  Git/RepositoryReader.cs      Discovery, audited metadata commands and typed snapshots
  Git/CommitHistory.cs         Bounded HEAD history reader and NUL-field parser
  Git/CommitGraph.cs           UI-independent deterministic parent-lane layout
  Processes/                   Requests/results and asynchronous runner contract
src/GitGui.ProcessHost/         Windows runner/job ownership and gated child-launch helper
tests/GitGui.FoundationChecks/  Disposable-file/Git, process, view-model and headless WPF checks
scripts/Build-Preview.ps1       Verified local publish into ignored Build/<version>-win-x64
```

App references Core and ProcessHost; Core never references WPF or Windows process APIs. There are no external NuGet packages. Small INotifyPropertyChanged/ICommand implementations suffice; no command framework, DI container or generic repository/service scaffolding. Add mature dependencies only when a concrete need outweighs owning a small amount of code. Standard WPF controls preserve native keyboard/automation behavior; the graph cell draws already-computed real parent connections.

App creates services explicitly, shows a loading shell, then loads settings on a worker task. Closing cancels startup, stops navigation, captures a plain layout snapshot on the dispatcher and saves it off-thread. Saving failure offers stay/retry or explicit exit without saving; normal exit drains diagnostics with a bounded wait. A per-profile mutex prevents two app writers. Dispatcher/startup failures show a resource-backed generic error and exit without saving potentially invalid state; fatal process logging is best effort. No tray, startup network work or background polling exists.

Closing also prevents new inspections, cancels the current read and awaits owned-process cleanup before saving. This gate covers read-only metadata/history only. Replace it with the E2-E4/U3 workflow gate **before** adding mutations, prompts, tracked external tools or updates. The settings mutex is not a repository or linked-worktree lock. No updater service or fake always-idle implementation exists.

## Implemented Git metadata and process execution

Discovery checks standard Git for Windows installation paths or an explicitly confirmed executable, never PATH/current-directory git.exe. Each repository directory needs session trust before discovery/inspection; remembered tabs do not imply trust. This is not a secure sandbox for hostile repositories or executables. Version floor is 2.47.0 for the audited invocation controls; only installed 2.51.1 has runtime coverage. Configured executable and trust are session-only. Git configuration is preserved; inherited repository/trace overrides are removed, with explicit global/system config locations retained. No safe.directory changes are made.

Core builds metadata commands and parses results on workers. It uses rev-parse/symbolic-ref for worktree, Git/common directory and HEAD identity, then observes operation markers (merge/rebase/am/cherry-pick/revert/sequencer). HEAD/markers are checked twice to reject detected races. Snapshots are timestamped observations, not transactions or authoritative safety gates. Bare repositories are rejected; a selected subdirectory may resolve to its containing worktree. No status, file-content, mutation or network commands exist. [Git rev-parse](https://git-scm.com/docs/git-rev-parse), [worktree semantics](https://git-scm.com/docs/git-worktree).

The Windows runner uses ArgumentList, no shell, explicit cwd/environment, closed stdin, concurrent byte-stream drains and optional worker callbacks (consumers must not block). Default limit is 256 KiB per stream and 15 seconds per process; two concurrent processes per runner, one inspection in this UI, 45-second inspection deadline. Output overflow, timeout, cancellation and launch failures have distinct results. Git error text is displayed in memory, never logged. UI elapsed-time ticks run only during an active inspection. Pagers, optional locks, lazy fetch, fsmonitor and automatic maintenance are disabled for these reads. No hooks/filters/helpers are intentionally invoked and no invisible input is supported. [Structured arguments](https://learn.microsoft.com/en-us/dotnet/api/system.diagnostics.processstartinfo.argumentlist?view=net-10.0), [Git invocation controls](https://git-scm.com/docs/git).

An owned helper waits for a bounded JSON request on stdin. The parent assigns it to a kill-on-close Windows Job Object **before** sending the request, so Git cannot spawn before ownership is established. The helper launches the child and relays both output streams; exit 125 is reserved for host/protocol failure. Closing the job stops only its owned descendants. Timeout also bounds descendants retaining pipe handles after their direct parent exits. The helper EXE/DLL/runtime files must ship together with the app. This small separate executable avoids a custom suspended-process/native pipe launcher. Job assignment failure fails closed. [Job Objects](https://learn.microsoft.com/en-us/windows/win32/procthread/job-objects), [assignment semantics](https://learn.microsoft.com/en-us/windows/win32/api/jobapi2/nf-jobapi2-assignprocesstojobobject).

UI generation checks discard obsolete results; beginning a refresh clears the old snapshot/history. Cancel/failed history reads clear rows and preserve any already-inspected metadata; retry is explicit. Cancellation is not rollback. Manual refresh is the external-change mechanism. No polling/watchers, cross-instance repository locks or mutation coordination exist yet.

### Commit history and graph (F1b, 0.3.0)

The user prioritized history before status. Read `git log --topo-order` from the inspected HEAD hash, with NUL-delimited fields/records, UTF-8 output, no patches, external diff, textconv, notes, mailmap, decoration or signature verification. Git's configured `log.showSignature` must not launch gpg. Parse complete commit identities, original-offset dates (including UTC `Z`), parent hashes and messages on workers. Hash widths are validated without assuming SHA-1. [Git log options](https://git-scm.com/docs/git-log), [pretty-format placeholders/terminators](https://git-scm.com/docs/pretty-formats) (official documentation rechecked 2026-10-23).

Load a 200-row prefix with one-row lookahead, increasing by 200 up to 2,000; each expansion re-reads that bounded prefix from the same hash. Compare prior hashes/parents before replacing rows, retain selection by hash and reject obsolete tab generations. This trades repeated bounded traversal for simpler stable loading; deeper cursor-based history remains deferred. Shallow state is queried, but missing objects fail without lazy fetch. Per-command output is capped at 4 MiB; pathological history fails visibly rather than allocating without bounds. Only the active tab holds history; switching tabs clears it and requires explicit refresh. Nothing is persisted or automatically fetched.

`CommitGraph` maintains pending-parent lanes in topological order, merges shared destinations, and produces per-row line segments plus a node. It validates order/duplicates, caps at 64 lanes and does no drawing. WPF uses a fixed-order, read-only DataGrid with row recycling and finite viewport; only visible graph cells draw curves/nodes. Merge nodes have an outer ring; accessible row text includes parent hashes. Graph column minimum width follows computed lanes, allowing horizontal scrolling. Dates use local culture; bottom details preserve original offsets. Message display is capped at 65,536 characters with notice, and long subject/identity prefixes are bounded. Full parsed text stays in memory within the read cap. [WPF virtualization guidance](https://learn.microsoft.com/en-us/dotnet/desktop/wpf/advanced/optimizing-performance-controls).

The graph shows actual ancestry, including merges, from one observed HEAD. All-ref selection, ref badges, search/filter, continuous cursor loading, diff/file lists and more complete accessibility/RTL certification remain deferred. No fake rows or simulated Git interactions ship.

### Deferred execution boundaries

Before status/diffs: add porcelain-v2/NUL path parsers, literal pathspecs, bounded content loading, no external diff/textconv/lazy fetch, and refresh reconciliation. Before mutations: serialize by common-directory identity across instances, recheck prerequisites and allow valid reads only; shared-state identity is already captured. External clients remain outside app coordination. Before network/hooks/tools: explicit authenticated askpass/editor or visible terminal routes, prompt/tool lifecycle, cooperative cancellation policy and recovery reinspection. The current closed-stdin/forced-stop read runner alone is insufficient for these workflows. Never disable hooks silently to fit the runner.

## Settings, storage, localization

**Implemented:** schema-2 settings under `%LOCALAPPDATA%\GitGui\Foundation` (or an explicit absolute test-profile path), separate from binaries. Stores layout/navigation and up to 20 absolute repository paths in tab order with selection; paths are local private data. Schema 1 migrates in memory; saving preserves previous bytes in the backup. Reads/writes are limited to 64 KiB and values normalized. Saves use a same-directory temporary file, cancellation check, atomic move/replace and previous-file backup; changed input since load blocks saving. This detects external edits but is not an atomic transaction with arbitrary editors. Invalid/newer/unsupported schemas and unavailable files remain untouched. [README](../README.md#local-data-and-recovery) owns file names and recovery instructions.

Diagnostics accept only typed event/failure codes and UTC timestamps, through a 128-entry bounded queue. Two rotating files retain roughly 512 KiB total, with at most one event of threshold overshoot; overload drops events. No exception text, stack traces, credentials or repository content enter the log. Worker-task logging failure is nonfatal. This is a safe foundation, not comprehensive crash reporting.

**Deferred settings fields:** drafts, tool definitions, persistent trust, update preferences and redacted recovery markers. No credentials. Future caches must be bounded and rebuildable. On restart rediscover Git state before enabling mutations; no automatic command replay.

**Migration policy:** validate before writing; preserve pre-migration bytes as the backup during atomic replacement. Never overwrite a newer/invalid schema. Only schema 1-to-2 migration exists; no general migration engine or in-app recovery/export UI. Version 0.1.0 opens schema 2 read-only; reverting binaries does not restore data automatically. Separate channel profiles if pilot schemas diverge. Installation identity/product name and the permanent data path remain undecided; the current Foundation profile is provisional.

**Implemented localization:** neutral English `.resx` messages, ResourceManager lookup, `{loc:Text Key=...}` in XAML and UiText lookup/formatting in view models/lifecycle. Whole messages replace concatenation; formatting uses the user's CurrentCulture while UI language is currently English. Window flow direction derives from UI culture; the version is direction-isolated. Accessibility names use localized labels, not model ToString output. Built-in OS dialog button captions are supplied by Windows.

**Deferred:** language selection/satellite resources, a plural-aware formatter before the first count message, pseudo-localization/RTL verification, theme selection and installer/updater resources. Inspection dates/durations use CurrentCulture; repository paths/hashes and raw output have LTR fields. Complete bidi isolation of mixed branch/path messages still needs verification. Do not use the Framework-only LocBaml workflow for modern WPF. [WPF localization](https://learn.microsoft.com/en-us/dotnet/desktop/wpf/advanced/wpf-globalization-and-localization-overview).

## Local preview delivery (implemented)

Version comes from Directory.Build.props; each delivered update gets one CHANGELOG entry and a commit. Minor versions add capabilities, patches fix behavior/docs; incompatible released contracts require a deliberate major bump. Build-Preview.ps1 restores/builds/checks, publishes a framework-dependent x64-host preview to unique staging, verifies required helper files, then moves it into ignored Build/<version>-win-x64 without replacing older versions. SHA256SUMS detects accidental changes only. This is an unsigned local folder requiring the Desktop Runtime, with no install/update service or distribution trust guarantee; signed per-user distribution remains deferred below.

## Updater comparison: current official evidence

WPF has no built-in universal updater. A compatible third-party framework is distinct from Microsoft platform support. Final choice requires installed upgrade, idle-gate, signature and localization tests; documentation alone is not proof.

| Option | Fit, signing and installation | Tradeoff / decision status |
| --- | --- | --- |
| **NetSparkle + per-user installer** | .NET/WPF support; static appcast; strict Ed25519 verification for both feed and package. Separate installer handles replacement. | Strong candidate for exact consent flow and signed manifest requirement. App owns scheduling/idle coordination; installer must prove safe in-place repair and failure handling. No automatic rollback assumed. |
| **Velopack** | C# SDK, static hosting, integrated packaging/update lifecycle, LocalAppData installation; code-signing support. | Strong candidate for less installer machinery. Must verify complete feed/package authenticity and signer policy in pinned version, not assume hashes or signed EXEs authenticate every package byte. Automatic pending-update application on startup is documented as enabled by default and must be disabled. |
| **MSIX / App Installer** | Windows-managed signed packages and update deployment; package certificate must be trusted on each device. Supports packaged desktop apps. Per-user deployment is feasible under device policy. | Attractive with company-managed certificate trust. App Installer scheduling is not our Git-aware idle gate; automatic background/opportune application may conflict with explicit restart consent. Programmatic deployment needs a proven integration; no forced shutdown/activation blocking. |

NetSparkle's documented strict mode requires a detached appcast signature and a signature on the downloaded package; it does not supply the installer. Proposed packaging companion: NSIS, user execution level, offline self-contained runtime, fixed application identity, no forced closing of tracked processes. This is a candidate, not a selected bundle. [NetSparkle official README](https://github.com/NetSparkleUpdater/NetSparkle), [NSIS user privilege option](https://nsis.sourceforge.io/Reference/RequestExecutionLevel).

Velopack supports static file distribution and C# integration. Its installation directory is replaced during updating, so user data must live elsewhere. Inspect all auto-apply/exit behaviors and preserve explicit consent. [Introduction](https://docs.velopack.io/), [integration/default auto-apply](https://docs.velopack.io/integrating/overview), [installer](https://docs.velopack.io/packaging/installer), [signing](https://docs.velopack.io/packaging/signing), [preserving data](https://docs.velopack.io/integrating/preserved-files).

App Installer options have different minimum Windows versions; its independent background check runs every eight hours rather than an arbitrary in-app interval. Do not enable UpdateBlocksActivation. OS package-in-use detection cannot identify a Git conflict workflow after processes exit. Programmatic non-Store updates are possible, but compatibility with the full consent/idle flow must be tested. [Update settings](https://learn.microsoft.com/en-us/windows/msix/app-installer/update-settings), [programmatic updates](https://learn.microsoft.com/en-us/windows/msix/non-store-developer-updates), [servicing while in use](https://devblogs.microsoft.com/insidemsix/msix-servicing-while-in-use/).

MSIX signing requires device trust; self-signed/internal certificates need provisioning, potentially IT/admin involvement. Publisher identity and certificate rotation need a deployment plan. App Installer availability, sideload policy and target Windows build must be checked. Windows code signing does not guarantee no SmartScreen prompt. [Microsoft signing guidance](https://learn.microsoft.com/en-us/windows/msix/package/signing-package-overview), [App Installer troubleshooting](https://learn.microsoft.com/en-us/windows/msix/app-installer/troubleshoot-appinstaller-issues).

## Proposed update mechanics

Use the selected framework's manifest format (for example signed appcast), not an invented wire protocol. Required fields are U2; propose channel, OS/architecture, minimum version/schema compatibility, package size/hash, signer/key identifier, and release-note content covered by verification. Allow only supported schema/HTTPS locations; reject unexpected redirects/auth pages and downgrade/replay below installed or persisted trusted version. Signature verification must precede executable use; checksums additionally detect corruption. Do not render active HTML from release notes.

Check after shell startup, with timeout/backoff; optional periodic interval never overlaps a check. Download to bounded staging storage and re-verify completed bytes after resume. Cancellation discards or quarantines partial data. Do not install on download completion, ordinary close, or next startup.

On explicit Restart and update: acquire an app-wide shutdown/update gate across instances, stop admission of new work, recheck every repository/operation/tool/prompt and unresolved sequence, persist session safely, then hand off to the updater only if idle. Unknown tool lifecycle blocks until completion is verified or acknowledged. The installer must not force-kill an instance or race a new launch. Revalidate package identity/signature at handoff. Startup recovery still blocks installation when Git has unresolved work. Failure to persist state cancels shutdown.

### Signing and bad-release recovery

Updater signature (e.g. Ed25519) authenticates downloaded feed/package against a bundled public trust key. Authenticode/MSIX signing establishes Windows publisher trust; HTTPS authenticates transport. These are separate controls; neither a hash nor an updater signature replaces Windows certificate trust.

Keep private keys outside repository, app, artifacts and ordinary logs; restrict release access, use protected signing/CI or hardware-backed storage, and document backup/rotation/compromise handling. Public verification keys may ship. No keys/providers are selected or generated; signing-service eligibility/budget and company certificate trust remain open.

Retain known-good artifacts and schema backups. Pilot first; if bad, withdraw manifest, keep clients operational, and publish a higher-version corrective release compatible with affected schemas. Maintain a separately reachable signed repair installer for an app that cannot start. Repair must not require routine uninstall/reinstall; exceptional downgrade/data restore is an explicit documented procedure. No automatic rollback or power-loss atomicity claim without implementation evidence.

## Hosting and release operations

| Hosting | Availability/maintenance | Outside office and access |
| --- | --- | --- |
| Personal computer | Least dependable: sleep, restart, changing address, bandwidth and owner availability; TLS/backups/security upkeep required | Office/VPN-only unless intentionally exposed through managed HTTPS networking. NAT/firewall/DNS and a reachable trusted certificate are prerequisites, not something to configure silently. |
| Company server | Good if already monitored, backed up and maintained; still an outage dependency for new downloads | Internal endpoint requires corporate network/VPN and DNS/certificate trust. External users need approved public HTTPS access or VPN; browser login alone does not authorize an updater. |
| Managed static/object hosting | Preferred proposal for low operational effort; storage/egress cost and provider policy remain | Public HTTPS supports offsite access without VPN. Private packages require supported per-user authentication/access gateway or approved private networking; test proxy and token expiry behavior. |

Do not equate unlisted URLs with private distribution. Some static website products are public-only: Azure Storage's static website endpoint does not support authentication/authorization; private deployment needs a suitable endpoint/service. [Official Azure hosting limitations](https://learn.microsoft.com/en-us/azure/storage/blobs/storage-blob-static-website). Reuse existing company identity where possible; never embed a PAT/shared password. Exact host/access integration is open. Unavailable hosting affects only update discovery/download, never installed local functionality.

Proposed release sequence: build/test -> sign immutable versioned packages -> upload packages and signed notes/metadata -> verify downloads from user-facing endpoint -> test-channel promotion/pilot -> atomically publish stable manifest and matching signature. If atomic pair publication is unavailable, mismatch must fail closed and retry later; never relax verification. Use short manifest cache lifetime, immutable artifact URLs, retain prior packages. Prepare a release checklist, licenses, migration/recovery notes and standard-user upgrade report before distribution.
