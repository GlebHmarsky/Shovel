# Shovel

Windows 10/11 Git client, C# / WPF / .NET 10 LTS. **0.14.0** adds selected-hunk staging/unstaging to the modal Local Changes/Commit workspace with file diffs, file/all stage/unstage, commit/amend and Shift Commit and Push. History remains visible behind it. Shared dark styling, branch/network actions and Pull-Rebase/Merge/auto-stash remain.

[AGENTS](AGENTS.md) owns session guidance; [product](docs/product.md) UX; [requirements](docs/requirements.md) acceptance; [architecture](docs/architecture.md) mechanisms/limits; [roadmap](docs/roadmap.md) evidence; [TODO](TODO.md) implementation priorities.

**Delivery policy:** requested controls must perform real actions; no new placeholders or UI-only completion. Established work environment: no repo-trust/first-use/program-consent layer for ordinary Git actions, and no extra safety measures without concrete need. Normal action buttons authorize the operation and configured hooks/helpers; required auth/destructive confirmations remain. Unfinished entry points are omitted; disabled working actions explain the missing target/dependency or active operation.

## Prerequisites

- Windows x64, SDK **10.0.401** (global.json, patch roll-forward) with Desktop targeting pack for development; Visual Studio optional, no C++ toolchain/workload/third-party NuGet packages.
- Run-only machines need matching .NET 10 **Desktop Runtime**; preview is framework-dependent.
- Git for Windows >=2.47.0; tested here 2.51.1. Standard roots discovered automatically, preferring native ucrt64/mingw64/mingw32 over cmd/git.exe. Repository → Git executable selects a custom installation for the session; no PATH/repository-local discovery.
- Recorded host: SDK 10.0.401/Desktop Runtime+pack 10.0.12/Git 2.51.1, already installed. Historical 2026-10-23 support check listed .NET 10.0.13; refresh servicing before distribution. .NET 10 support recorded through 2028-11-14. [Policy](https://dotnet.microsoft.com/en-us/platform/support/policy/dotnet-core).
- Verify coworker Windows editions/builds; Windows 10 runtime compatibility is not support for every edition. [Windows support](https://learn.microsoft.com/en-us/dotnet/core/install/windows). ARM64/installed distribution unverified.

## Restore, build and check

PowerShell from repository root:

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

Check every exit code. Env applies to current shell; single-process MSBuild/no shared compiler avoids observed execution restrictions. Checks are a console runner, not dotnet test; nonzero means failure.

Fixtures under ignored artifacts/foundation-checks (one empty nonrepo in temp), isolated config/disposable Git repos/local bare remotes/test hooks. Headless WPF uses a plain test Application/shared resources, suppresses production startup and saves PNGs without Show/desktop control. Restricted MSYS named-object creation may need sandbox permission; that does not authorize live remotes/launches. [Evidence](docs/roadmap.md#selected-hunks-2026-10-28) distinguishes integration/headless from desktop.

Focused theme resource/control/offscreen checks (no window shown): `dotnet run --project tests/GitGui.FoundationChecks -c Release --no-build -- --check-theme`. The full suite also runs these checks.

## Run

Interactive launch requires task authorization for AI sessions. Manual commands:

```powershell
dotnet run --project src/GitGui.App -c Release --no-build
# Optional isolated profile:
dotnet run --project src/GitGui.App -c Release --no-build -- --data-directory "C:\Users\hmars\Documents\GitGUI\artifacts\manual-profile"
```

Data-directory must be absolute; default %LOCALAPPDATA%\GitGui\Foundation preserves old GitGui tabs/layout. One instance/profile; independent profiles can coexist. Closing cancels/drains reads then saves; no tray/autostart.

Keep Shovel.AskPass.exe/.dll/.deps.json/.runtimeconfig.json beside Shovel (build/publish copies them). Askpass handles explicit network credentials/passphrases/host confirmation; browser/GCM/signing/interactive hooks/custom SSH still use existing tools. Shovel saves no prompt/credential data; configured helpers may store credentials. Live sign-in/focus/input unverified.

## Using the workspace

Open a local worktree root; selected saved tab restores automatically, others load on selection, no read consent. Fixed All Commits includes local/remote/tag/current-HEAD ancestry, detached included; no reflog-only/unreachable/stash-only history or scope selector. Legacy narrow scope ignored.

Headerless graph/refs/description/author/date/hash begins below tabs. Select row for full parsed metadata/message/parents/refs. Older/Newer: 200-row pages/no total count cap. Range/status/help exposes limits. Three cached tabs/character budget, 128 back cursors/First history page; page/selection/scroll session-only. Cache eviction reloads first page.

Cached switch publishes immediately/validates identity off-thread. >30 s snapshots refresh on tab/window activation; Refresh forces read. No polling/preload. Refresh retains rows on failure/cancel, changed refs restart page one/retain selected hash if present; changed boundaries block paging. Identity errors clear rows. Right-click tab: refresh/move/close/close others; MMB closes clicked tab, active read cancels/neighbour selected, inactive close preserves active work.

Pull badge = behind, Push = ahead of configured upstream **locally**, loaded after rows; separate push target not compared. Zero measured, dash pending/unavailable/failed; tooltip reason/time/upstream. Missing/detached/unborn/shallow cannot claim synchronized; failed refresh keeps stale counts, no implicit fetch.

Menus, working toolbar actions and center timer remain; unfinished Stash/Explorer/Terminal/Editor/Tools/sidebar controls are omitted. Local changes opens the commit modal; All Commits stays in the main workspace. Repository holds refresh/cancel/Git selection/Diagnostics/tab actions. Diagnostics includes bounded in-memory stderr/timings, not persistent content logs. Exact-author peers yellow, selected row normal/high-contrast system colors; [product](docs/product.md) owns exact matching.

## Local Changes

Choose toolbar **Commit (count)** or sidebar Local changes. The resizable modal binds cached rows/draft before its background scan, while history stays behind it. Select unstaged or staged files for the appropriate diff. Ctrl/Shift selects multiple files; hold Shift while clicking the existing Stage/Unstage buttons for all listed files. Enter subject/body to Commit; Amend can preload the last message and confirms the actual tip. Hold Shift on Commit for Commit and Push to the configured upstream. Push failure retains the successful commit. Drafts survive closing/reopening this modal during the session, but not closing the tab/app.

The right side shows colored, numbered unified text, whitespace/whole-file options and hunk navigation; binary files show before/after bytes. Refresh/Cancel and retained Git output are inside the modal. Failed reads keep rows but disable stale writes. Commit count runs once after selected-tab history, never delays those rows or polls/preloads other tabs. [Product](docs/product.md#local-changes--commit-0130) owns the full workflow; [architecture](docs/architecture.md#local-changes-and-commit-0130) owns limits/filter and submodule restrictions. Image previews, arbitrary-line staging and conflict editing are deferred; resolve files externally, then stage/commit a resolved merge here.

Select a line inside a change block, or Ctrl/Shift-select multiple blocks, then use **Stage hunk(s)** / **Unstage hunk(s)** beside the diff. This stages complete blocks, not individual lines. Ordinary tracked text modifications are supported; unavailable file types/options explain their whole-file alternative. [Hunk implementation and limits](docs/architecture.md#selected-hunks-0140).

Focused headless checks after building: `dotnet run --project tests/GitGui.FoundationChecks -c Release --no-build -- --check-local` for the parser/status, or `--check-commit` for modal/diff/mutation workflows. Use `--check-hunks` for focused hunk integration/UI checks. Full preview checks include all three.

## Branch and remote actions

Branch: create from concrete starting commit/optional checkout; arrow switches local branches. Fetch chooses remote/all; Pull remote branch → current branch defaults to Rebase, with Merge as the alternative. Optional auto-stash saves/reapplies tracked changes (untracked stay in place; staging may change). Push chooses source/remote/destination with optional tags/upstream. Fixed-size close-only options are immediately editable from cached local refs/remote names; opening runs no Git queries. Use Fetch for new remote branches or Refresh after external configuration changes. Execution resolves the named remote using current Git configuration and validates the cached refs. Clicking the normal action closes options and starts Git. Follow phase/time at top center, Cancel there or under Repository, and read results through Details → Operation. No extra approval checkbox; force options remain deferred.

Existing restrictions: Git rejects conflicting dirty/untracked paths; Shovel protects ignored collisions. Branch checkout still requires clean state and excludes sparse/submodule repositories. Pull respects existing sparse patterns and updates superproject pointers without updating/stashing submodule contents. Active sequences, custom SSH/multiple remote URLs and force remain unsupported. Hooks are preserved. [Action details](docs/architecture.md#explicit-git-actions-f2a-080). Rebase/merge/stash conflicts keep recovery state, show terminal instructions and original output; in-app resolve/continue/abort is pending. One last result/output remains in memory on its repository tab, with HTTP(S) credentials redacted. Cancel is not undo; fresh history/count inspection finishes before another action/tab/window close. Main-window auth prompts survive options closure; refresh failure is distinct from Git outcome.

## Headless timing reproduction

After build:

```powershell
dotnet run --project tests/GitGui.FoundationChecks -c Release --no-build -- --measure-history
dotnet run --project tests/GitGui.FoundationChecks -c Release --no-build -- --measure-large-history
dotnet run --project tests/GitGui.FoundationChecks -c Release --no-build -- --measure-actions
```

First: three samples/isolated 2,500 commits, separate settings/discovery/inspection/ref/page/graph/format/process timings, first/warm offscreen construction. Second: packed 100k commits/1,201 refs, first/last page before/after fixture-only commit-graph/packed-ref maintenance. App never maintains user repos for a benchmark. Regular checks cover auto-start/tab/refresh/render timing. Outputs artifacts/history-benchmarks; [roadmap](docs/roadmap.md) records comparisons. Not cold desktop/Windows 10/p95 certification.

Third: seven samples on an isolated 2,500-commit/one-remote fixture; separate cached editable-dialog layout, post-submission preparation/subprocess cost and prepared-dialog layout. Writes artifacts/action-benchmarks. First construction includes WPF/JIT initialization; later samples are warm. No window is shown, no network fetch or user repository is read.

## Build a local preview

```powershell
powershell -NoProfile -File .\scripts\Build-Preview.ps1
```

Restores/builds/checks/publishes **Build/0.14.0-win-x64/Shovel.exe**, with helper/files/START-HERE/SHA256SUMS/changelog. Keep files together; runtime needed, SDK not needed to run. Unsigned local folder, no installer/updater/hosting/keys/push.

Build ignored/untracked; never overwrite an existing version. Partial publish stays in ignored artifacts. Increment Directory.Build.props and changelog once/delivery. User-requested 0.3.1 old-preview cleanup was one-time; subsequent versions retained. No additional prerequisite missing on this development host.

## Local data and recovery

| File | Content/handling |
| --- | --- |
| settings.json | Schema 4: layout/up to 20 tabs/order/selection/legacy scope/identity; All Commits written since 0.7.0. No credentials/history. Schemas 1/2/3 migrate without consent; stamps saved after successful reads. Invalid/inaccessible/newer/unsupported or externally changed file is not overwritten. |
| settings.json.bak | Previous valid bytes, including pre-migration data; no automatic restore. |
| diagnostics.jsonl / .1 | UTC + typed codes; ~256 KiB each, bounded queue/best effort including fatal exit; no raw output/exceptions/paths/content. |

Close app, copy settings/backup first; inspect schema before restoring or move invalid file aside for defaults. Older previews read schema 4 without writing; downgrade via separate profile/compatible backup. External-edit check is not a transaction with arbitrary editors; never edit during save.

## Current limits

Current identity supports root on local fixed drive/no reparse components; UNC/removable/junction/symlink/subfolder opening unsupported. Missing/replaced targets require explicit reopen without trust modal; identity is not an in-place-hostile-change/atomic-race guarantee. [Read audit](docs/architecture.md#automatic-local-history).

History uses one read chain/max two children; the modal has bounded cancelling status/diff chains, and writes wait for both. Bare repos rejected, linked actions coordinated within Shovel, external clients independent. Shallow ends distinct, missing partial objects fail without fetch. Marker observation is not sequence recovery.

History: 4 MiB/read, 64 lanes, 15 s/command, 45 s/read chain, 128 cursors/tab; errors explicit, no false truncation. Deep --skip can time out. Full selected fields within read bound, abbreviated rows. The [shared dark theme](docs/theme.md) applies to the client area; native OS title bars/pickers/message boxes remain system-owned. Light mode and further theme polish are deferred. Pending: hunk staging, image/selected-commit diffs, broader mutations/filtering, auth routes, locales/accessibility/DPI, representative performance and installed updates. See TODO for priorities, roadmap for measured limits.
