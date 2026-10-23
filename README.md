# Git GUI preview

A C# / WPF application for Windows 10/11, targeting .NET 10 LTS. Version **0.3.0** adds real commit history: connected graph nodes with descriptions to the right, author/date/hash columns, and selected-commit details below. File status, diffs, mutations, network access, installer and updater are not implemented. Product name and installation identity remain provisional.

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

Checks write inspectable, isolated fixtures under ignored `artifacts/foundation-checks/`; one empty non-repository fixture uses the temporary directory. Git mutations use disposable repositories and isolated configuration, never working repositories. Headless WPF checks also save history-table PNGs without opening a window or controlling the desktop. [Roadmap evidence](docs/roadmap.md#history-preview-verification-2026-10-23) distinguishes these checks from interactive GUI verification.

## Run

```powershell
dotnet run --project src/GitGui.App -c Release --no-build
```

For an isolated manual test profile:

```powershell
dotnet run --project src/GitGui.App -c Release --no-build -- --data-directory "C:\Users\hmars\Documents\GitGUI\artifacts\manual-profile"
```

`--data-directory` accepts an absolute path; omit it for normal use. Default data is `%LOCALAPPDATA%\GitGui\Foundation`, separate from binaries. Closing cancels/drains read-only inspection, then saves layout/tabs; there is no tray or autostart. One instance may own a profile at a time. Different test profiles are independent.

Choose **Open…**, select a folder and review the session-trust prompt. The History table displays the inspected HEAD's ancestry, including merged branches. Select a row for its message and metadata below; **Load older commits** expands from 200 rows up to 2,000. Branch tips outside that ancestry, ref badges, searching and filtering are deferred. Open **Repository** details for worktree/common-directory identity and last-inspection time; interrupted-operation notices also appear above the table.

Use **Trust / Refresh** after external changes, switching repository tabs or restarting; restored tabs perform no automatic reads. Refresh replaces the observed history; loading older commits keeps the original tip. Errors retain original Git stderr in **Git output** for this session. The app does not repair, continue or abort interrupted Git operations.

## Build a local preview

```powershell
powershell -NoProfile -File .\scripts\Build-Preview.ps1
```

The script restores/builds/runs checks and publishes to `Build/0.3.0-win-x64/`. Double-click **GitGui.App.exe** there; keep the helper and all other files together. No SDK is needed to run, but the matching .NET 10 Desktop Runtime is required. No additional prerequisite is missing on this machine.

`Build/` is ignored and must never be committed. The script refuses to replace an existing version folder and keeps incomplete publishes in ignored `artifacts/`. Set the next delivery version in `Directory.Build.props` and update `CHANGELOG.md` once per delivery; see AGENTS.md for bump rules. The folder includes launch notes and SHA256 checksums. It is an unsigned local preview, not an installer or automatic update package. No hosting, keys or remote push are configured by packaging.

## Local data and recovery

- `settings.json`: schema 2; layout/navigation plus up to 20 repository paths in tab order and selection. Paths are local private data; no secrets, trust decisions or credentials are stored. Schema 1 layout settings migrate in memory and are saved as schema 2 on normal exit.
- `settings.json.bak`: previous valid settings on replacement, including pre-migration bytes. Invalid, inaccessible, unsupported or newer-schema settings load defaults without overwriting the file. Existing external edits also block saving.
- `diagnostics.jsonl` and `.1`: UTC timestamps and allowlisted event/failure codes only; each rotates at roughly 256 KiB, with a bounded event queue. No raw exception messages/stack traces or command output. Logging is best effort, including on fatal termination.
- With the app closed, copy the settings files before recovery. Inspect a backup's schema before restoring it, or move the invalid file aside to use defaults. Migration is limited to schema 1-to-2; there is no automatic backup restoration or rollback. Version 0.1.0 treats schema 2 as read-only; an intentional downgrade can use a separate profile or a retained schema-1 backup. Never edit files while the app is saving; external-edit detection is not a transactional lock against arbitrary editors.

## Current limits

Tabs support selection, left/right movement and closing. Inspection is bounded and cancellable; one runs at a time, with no background refresh. Failed/cancelled history loads clear rows and retain any already-inspected repository metadata. Bare repositories are rejected; linked worktree identity is read but no shared-state mutation coordinator exists. Shallow history stops at local boundaries; missing partial-clone objects fail without fetching. Marker detection is observational, not a validated recovery workflow.

History limits: 4 MiB Git output, 2,000 displayed commits, 64 concurrent graph lanes, 65,536 characters in a message preview (explicit truncation notice). Descriptions and identities have bounded display prefixes; original parsed text remains in memory. Graph continuations at the bottom are not invented parent rows. Additional locales, plural messages, RTL certification, dark theme, full accessibility/DPI testing and measured performance budgets remain in [TODO.md](TODO.md).
