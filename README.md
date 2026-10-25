# Shovel

A C# / WPF application for Windows 10/11, targeting .NET 10 LTS. Version **0.5.0** defaults to All Commits across local branches, remote-tracking branches and tags, with branch/tag labels, a connected graph and complete selected-commit details. Older/newer pages replace the previous 2,000-commit ceiling. File status, diffs, mutations, network access, installer and updater are not implemented. The product name is confirmed; publisher and permanent installation identity remain open.

Start with [AGENTS.md](AGENTS.md), [TODO.md](TODO.md) and the [roadmap](docs/roadmap.md). [Architecture](docs/architecture.md) describes implemented boundaries and future proposals separately.

## Prerequisites

- Windows with the .NET 10 SDK, including the Windows Desktop targeting pack. `global.json` pins SDK 10.0.401 with patch roll-forward. Visual Studio is optional; a C++ toolchain is unnecessary.
- For running the build without the SDK: .NET 10 **Desktop Runtime** of the matching architecture. This development build is framework-dependent; self-contained distribution is deferred.
- Git for Windows 2.47.0 or newer for repository inspection/checks; only 2.51.1 is runtime-verified here. The app checks standard install locations; use **Git executable…** to select and confirm a nonstandard installation. It does not search PATH or execute repository-local Git automatically.
- Installed here: SDK 10.0.401, Desktop Runtime/targeting pack 10.0.12, Git for Windows 2.51.1. No dependencies were installed.
- Update the development/runtime servicing patch before distribution: official support information checked on 2026-10-23 lists 10.0.13. .NET 10 LTS support runs through 2028-11-14. [Microsoft support policy](https://dotnet.microsoft.com/en-us/platform/support/policy/dotnet-core).
- Windows 10 runtime compatibility does not imply support for every Windows 10 edition. Confirm the fleet's editions/builds and test them; Microsoft's current list covers specific Windows 10 LTSC/Enterprise variants. [Supported Windows versions](https://learn.microsoft.com/en-us/dotnet/core/install/windows).

## Restore, build and check

From this directory in PowerShell:

```powershell
$env:DOTNET_CLI_TELEMETRY_OPTOUT = '1'
$env:DOTNET_NOLOGO = '1'
$env:DOTNET_CLI_HOME = Join-Path (Get-Location) 'artifacts/dotnet-home'
$env:DOTNET_ADD_GLOBAL_TOOLS_TO_PATH = 'false'
$env:DOTNET_CLI_WORKLOAD_UPDATE_NOTIFY_DISABLE = 'true'
$env:MSBuildEnableWorkloadResolver = 'false'
dotnet restore GitGui.slnx -m:1
dotnet build GitGui.slnx --no-restore -c Release -m:1 -p:UseSharedCompilation=false -nodeReuse:false
dotnet run --project tests/GitGui.FoundationChecks -c Release --no-build
```

These environment variables affect only this shell. Single-process MSBuild and disabled compiler sharing avoid restrictions observed in the Codex execution environment. WPF needs no optional .NET workload here. Check each command's exit code; there are no third-party NuGet packages. The console checks return a nonzero exit code on failure; they do not use `dotnet test` or a test-framework package.

Checks write inspectable, isolated fixtures under ignored `artifacts/foundation-checks/`; one empty non-repository fixture uses the temporary directory. Git mutations use disposable repositories and isolated configuration, never working repositories. Headless WPF checks also save history-table PNGs without opening a window or controlling the desktop. [Roadmap evidence](docs/roadmap.md#all-commits-verification-2026-10-24) distinguishes these checks from interactive GUI verification.

## Run

```powershell
dotnet run --project src/GitGui.App -c Release --no-build
```

For an isolated manual test profile:

```powershell
dotnet run --project src/GitGui.App -c Release --no-build -- --data-directory "C:\Users\hmars\Documents\GitGUI\artifacts\manual-profile"
```

`--data-directory` accepts an absolute path; omit it for normal use. Default data remains `%LOCALAPPDATA%\GitGui\Foundation`, separate from binaries, so the Shovel rename preserves existing tabs/layout and profile locking. Internal `GitGui` solution/project names are retained; no data migration is needed. Closing cancels/drains read-only inspection, then saves layout/tabs; there is no tray or autostart. One instance may own a profile at a time. Different test profiles are independent.

Choose **Repository → Open…**, select a local worktree root and approve automatic read-only history once. Approval is remembered and can be revoked from **Repository → Repository approval**. Existing pre-0.5 saved tabs need one explicit approval; no old path is silently trusted. Subsequent startups load only the selected approved tab automatically. **All Commits** is the default scope; it includes local branch, remote-tracking branch and tag ancestry. Reflog-only, unreachable, stash-only and detached-only commits are outside this view. Choose **Current branch / HEAD** for the inspected HEAD's ancestry, including detached HEAD. The scope selector's tooltip/accessibility help explains these boundaries; changing scope never checks out a branch. No filters are active.

The table has no visible column-heading row and shows graph nodes, ref labels, description, author, local date and short hash. Select a row for its complete message, author/committer, dates, hash, parents and full labels below. **Load older commits** / **Newer commits** navigate 200-row pages with the visible range shown; there is no total commit-count ceiling. Up to three visited tabs cache one page each within a character-data budget; back navigation retains 128 cursors and Repository → First history page returns to the start. Long row labels have full text in details/tooltips.

Switching tabs shows its cached page immediately while identity is checked off-thread. Snapshots older than 30 seconds refresh on tab/window activation; use **Repository → Refresh history** for immediate external-change inspection. No polling or preload of every tab. Scope persists across restart; page, selection and scroll persist only in cached session tabs. An evicted tab reloads its first page. Refresh retains rows; failed/cancelled reads mark them stale. Ref changes block paging until refresh; changed refs restart at page one, preserving the selected hash when present.

**Repository** also contains Cancel read, Git executable selection, Diagnostics, approval management and tab actions. Diagnostics shows in-memory Git stderr and bounded stage timings, not a persistent content log. Choosing a custom Git executable is still session-only; standard installations are discovered automatically after restart. The history surface uses white text and light-yellow author peers; high contrast switches to system colors. See [product](docs/product.md) for the exact author matching rule.

Automatic approval currently requires a root on a local fixed drive with no reparse/symlink/junction components; UNC/removable paths are not supported by this approval model. Moving/replacing a repository or its Git directories requires fresh approval. In-place hostile changes and atomic filesystem races are outside its guarantees; see [identity limits and read audit](docs/architecture.md#remembered-read-approval). No global Git/safe.directory changes, helper scripts or implicit network fetch.

## Headless timing reproduction

After building, run `dotnet run --project tests/GitGui.FoundationChecks -c Release --no-build -- --measure-history`. This creates an isolated 2,500-commit fixture and three samples under ignored `artifacts/history-benchmarks/`, with separate settings/discovery/inspection/ref/page/graph/formatting and process-launch durations. It also measures first/warm offscreen shell construction/layout. The regular checks record automatic startup, tab-switch and refresh timings and normal/minimum renders. These are not cold-boot, desktop-input, Windows 10 or p95 benchmarks. See the roadmap for before/after evidence.

## Build a local preview

```powershell
powershell -NoProfile -File .\scripts\Build-Preview.ps1
```

The script restores/builds/runs checks and publishes to `Build/0.5.0-win-x64/`. Double-click **Shovel.exe** there; keep the helper and all other files together. No SDK is needed to run, but the matching .NET 10 Desktop Runtime is required. No additional prerequisite is missing on this machine.

`Build/` is ignored and must never be committed. On 2026-10-24, the user requested removal of the old Build previews before rebuilding 0.3.1; those old package paths no longer exist. Cleanup is not automatic on subsequent builds. The script refuses to replace an existing version folder and keeps incomplete publishes in ignored `artifacts/`. Set the next delivery version in `Directory.Build.props` and update `CHANGELOG.md` once per delivery; see AGENTS.md for bump rules. The folder includes launch notes and SHA256 checksums. It is an unsigned local preview, not an installer or automatic update package. No hosting, keys or remote push are configured by packaging.

## Local data and recovery

- `settings.json`: schema 3; layout, up to 20 repository paths/order/selection, per-tab history scope and revocable approval fingerprints. Paths and approval are local private data; no credentials or history content. Schemas 1/2 migrate without approval; backups preserve previous bytes. Approval/revocation changes save immediately; save failures remain visible and are not durable approval/revocation claims.
- `settings.json.bak`: previous valid settings on replacement, including pre-migration bytes. Invalid, inaccessible, unsupported or newer-schema settings load defaults without overwriting the file. Existing external edits also block saving.
- `diagnostics.jsonl` and `.1`: UTC timestamps and allowlisted event/failure codes only; each rotates at roughly 256 KiB, with a bounded event queue. No raw exception messages/stack traces or command output. Logging is best effort, including on fatal termination.
- With the app closed, copy the settings files before recovery. Inspect a backup's schema before restoring it, or move the invalid file aside to use defaults. Migration supports schemas 1/2-to-3; there is no automatic backup restoration or rollback. Older previews treat schema 3 as read-only; an intentional downgrade can use a separate profile or a retained schema-1 backup. Never edit files while the app is saving; external-edit detection is not a transactional lock against arbitrary editors.

## Current limits

Tabs support selection, left/right movement and closing. Inspection is bounded and cancellable; one operation chain runs at a time, with at most two child commands in parallel. Activation-driven refresh retains the prior page on failure/cancellation. Identity mismatch/revocation clears cached content. Bare repositories are rejected; linked worktree identity is read but no shared-state mutation coordinator exists. Shallow history stops at local boundaries; missing partial-clone objects fail without fetching. Marker detection is observational, not a validated recovery workflow.

History limits: 4 MiB per Git/ref/boundary read, 64 concurrent graph lanes, 15 seconds per command and 45 seconds per operation. These limits cause explicit errors, not silent truncation or a false end-of-history. Pages re-walk the unchanged ref snapshot with an offset; very deep walks may time out. Navigation cursors are capped at 128 per cached tab. Full large-repository performance budgets are unverified. Descriptions/labels use bounded row previews; selected details keep complete parsed values. Additional locales, RTL/accessibility/DPI, dark theme and performance work remain in [TODO.md](TODO.md).
