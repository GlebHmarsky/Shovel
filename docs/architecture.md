# Architecture and distribution

Updated 2026-10-23. **Confirmed stack: C# + WPF**, selected by the user. **Implemented foundation:** .NET 10 LTS, MVVM shell, local settings/resources/diagnostics. Other designs below remain proposals unless identified as implemented. User obligations live in [requirements](requirements.md); tasks in [TODO](../TODO.md); evidence and unresolved decisions in [roadmap](roadmap.md).

## Stack and boundaries

C# / WPF is confirmed; .NET 10 LTS was selected using the installed SDK 10.0.401 and Windows Desktop pack 10.0.12. The official policy lists .NET 10 support through 2028-11-14; refresh servicing patches before distribution. [Microsoft support policy](https://dotnet.microsoft.com/en-us/platform/support/policy/dotnet-core). Git for Windows CLI remains the proposed Git adapter; no Git execution code exists yet. TypeScript is excluded. [Stack research](research/desktop-stack-reassessment.md) preserves alternatives and rationale, not measured performance claims.

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

### Implemented foundation (F0)

```text
GitGui.slnx / global.json / Directory.Build.props
src/GitGui.App/                 WPF executable, composition and lifetime owner
  App.xaml(.cs)                Startup/shutdown, profile mutex, guarded error handling
  Views/MainWindow.xaml(.cs)   Layout and view-only geometry handling
  ViewModels/                  ObservableObject and ShellViewModel
  Localization/UiText.cs       Resource lookup, formatting, XAML markup extension
  Resources/Strings.resx       All app-owned English messages/labels
src/GitGui.Core/                UI-independent net10.0 library
  Settings/                    Plain schema-1 layout model and bounded JSON store
  Diagnostics/                 Allowlisted events and bounded asynchronous log writer
tests/GitGui.FoundationChecks/  Disposable-file and headless WPF checks
```

App references Core; Core never references WPF. There are no external NuGet packages. A small INotifyPropertyChanged base is sufficient for the current presentation state; no command framework, DI container or generic repository/service scaffolding is added. Add mature dependencies only when a concrete need outweighs owning a small amount of code. Standard WPF controls preserve native keyboard/automation behavior; custom graph/diff controls wait for real data.

App creates services explicitly, shows a loading shell, then loads settings on a worker task. Closing cancels startup, stops navigation, captures a plain layout snapshot on the dispatcher and saves it off-thread. Saving failure offers stay/retry or explicit exit without saving; normal exit drains diagnostics with a bounded wait. A per-profile mutex prevents two app writers. Dispatcher/startup failures show a resource-backed generic error and exit without saving potentially invalid state; fatal process logging is best effort. No tray, startup network work or background polling exists.

Future process work belongs in a Git/Windows adapter, with CancellationToken-aware asynchronous methods called by core operation ownership. UI receives typed snapshots/progress rather than executing commands. Introduce those boundaries alongside the first runner; no fake Git/updater interfaces or always-idle services exist in F0. Replace the current simple close gate with the E2-E4/U3 workflow gate **before** adding active Git/tool/update workflows. The settings mutex is not a repository or linked-worktree lock. Update handoff must use that future gate and cannot directly call Application.Shutdown.

## Proposed Git execution and state (not implemented)

Discover a trusted Git installation or explicit executable path; never execute repository-local git.exe. Probe version/capabilities; minimum supported Git is an implementation validation decision. CLI integration preserves native configuration/tools; libgit2 or hybrid reads require a measured need and compatibility proof.

Use ProcessStartInfo.ArgumentList, no shell interpolation, explicit working directory/environment, concurrent stdout/stderr drains, byte-oriented NUL parsers and bounded output. Parse porcelain v2 status and structured refs/object data. Use literal pathspecs; never split paths on whitespace. Poll/parse/render off UI thread; incremental results marshal only small changes to UI.

Gate repository-aware commands on trust initially rather than claiming a secure untrusted Git sandbox. Background reads use audited command-local controls: no optional locks/lazy fetch/external diff/textconv; control helpers such as fsmonitor. Never change global settings or silently suppress hooks in normal mutations. Sources: [Git invocation controls](https://git-scm.com/docs/git), [status](https://git-scm.com/docs/git-status), [diff](https://git-scm.com/docs/git-diff).

Resolve worktree and common Git directories through Git, not assumptions about .git layout. Serialize mutations by common-directory identity with cross-instance coordination. Initial limits: two reads/repository, four Git processes globally. Block incompatible reads during mutation; generation IDs discard stale results. External tools remain outside this lock: reconcile watchers, focus/resume and operation completion; watcher overflow causes rescan. Git is authoritative, never a UI cache. [Worktree semantics](https://git-scm.com/docs/git-worktree).

Reuse credential helpers/agents. Supported askpass/editor requests use a local authenticated bridge with no secret persistence; arbitrary interactive hooks require an explicit visible terminal route or actionable failure. Disable pagers per command; never infer input waits from silence. Sequence editor support is deferred with interactive rebase. Windows Job Objects manage owned subprocesses with cooperative cancellation then explicit forced stop; shared browser/SSH agents are excluded. [Windows Job Objects](https://learn.microsoft.com/en-us/windows/win32/procthread/job-objects).

## Settings, storage, localization

**Implemented:** schema-1 settings under `%LOCALAPPDATA%\GitGui\Foundation` (or an explicit absolute test-profile path), separate from binaries. Only window dimensions/maximized state, panel sizes and navigation selection are stored. Reads are limited to 64 KiB and geometry is normalized. Saves use a same-directory temporary file, cancellation check, atomic move/replace and previous-file backup; changed input since load blocks saving. This detects external edits but is not an atomic transaction with arbitrary editors. Invalid/newer/older schemas and unavailable files remain untouched. [README](../README.md#local-data-and-recovery) owns file names and manual recovery instructions.

Diagnostics accept only typed event/failure codes and UTC timestamps, through a 128-entry bounded queue. Two rotating files retain roughly 512 KiB total, with at most one event of threshold overshoot; overload drops events. No exception text, stack traces, credentials or repository content enter the log. Worker-task logging failure is nonfatal. This is a safe foundation, not comprehensive crash reporting.

**Deferred settings fields:** repository tab order/selection, paths, drafts, tool definitions, explicit trust, update preferences and redacted recovery markers. No credentials. Future caches must be bounded and rebuildable. On restart rediscover Git state before enabling mutations; no automatic command replay.

**Future migration policy:** back up before migration; validate and commit migrated data atomically. Never overwrite a newer/invalid schema; offer recovery/export. No migration engine or in-app recovery/export UI exists yet. Keep a documented compatibility window and separate channel profiles if pilot schemas diverge. Installation identity/product name and the permanent data path remain undecided; the current Foundation profile is provisional.

**Implemented localization:** neutral English `.resx` messages, ResourceManager lookup, `{loc:Text Key=...}` in XAML and UiText lookup/formatting in view models/lifecycle. Whole messages replace concatenation; formatting uses the user's CurrentCulture while UI language is currently English. Window flow direction derives from UI culture; the version is direction-isolated. Accessibility names use localized labels, not model ToString output. Built-in OS dialog button captions are supplied by Windows.

**Deferred:** language selection/satellite resources, a plural-aware formatter before the first count message, pseudo-localization/RTL verification, theme selection and installer/updater resources. There are no plural/date-bearing messages in F0. Add translator comments for placeholders, test long translations and isolate future code/paths/hashes. Do not use the Framework-only LocBaml workflow for modern WPF. [WPF localization](https://learn.microsoft.com/en-us/dotnet/desktop/wpf/advanced/wpf-globalization-and-localization-overview).

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
