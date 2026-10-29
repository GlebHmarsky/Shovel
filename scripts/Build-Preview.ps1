param()
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
$properties = [xml](Get-Content -LiteralPath (Join-Path $projectRoot 'Directory.Build.props') -Raw)
$version = [string]$properties.Project.PropertyGroup.Version
if ($version -notmatch '^\d+\.\d+\.\d+$') { throw 'Invalid project version.' }
$output = Join-Path $projectRoot ('Build/' + $version + '-win-x64')
if (Test-Path -LiteralPath $output) { throw 'This preview version already exists. Increment the project version before publishing another package.' }
$staging = Join-Path $projectRoot ('artifacts/package-' + [Guid]::NewGuid().ToString('N'))
if ([Runtime.InteropServices.RuntimeInformation]::OSArchitecture -ne [Runtime.InteropServices.Architecture]::X64) {
    throw 'This preview script is validated for a Windows x64 SDK host only.'
}
$env:DOTNET_CLI_TELEMETRY_OPTOUT = '1'
$env:DOTNET_NOLOGO = '1'
$env:DOTNET_CLI_HOME = Join-Path $projectRoot 'artifacts/dotnet-home'
$env:DOTNET_ADD_GLOBAL_TOOLS_TO_PATH = 'false'
$env:DOTNET_CLI_WORKLOAD_UPDATE_NOTIFY_DISABLE = 'true'
$env:MSBuildEnableWorkloadResolver = 'false'
Push-Location $projectRoot
try {
    dotnet restore GitGui.slnx -m:1
    if ($LASTEXITCODE -ne 0) { throw 'Restore failed.' }
    dotnet build GitGui.slnx --no-restore -c Release -m:1 -p:UseSharedCompilation=false -nodeReuse:false
    if ($LASTEXITCODE -ne 0) { throw 'Build failed.' }
    dotnet run --project tests/GitGui.FoundationChecks -c Release --no-build -- --smoke
    if ($LASTEXITCODE -ne 0) { throw 'Verification failed; preview was not produced.' }
    dotnet publish src/GitGui.App --no-restore -c Release --self-contained false -o $staging -m:1 -p:UseSharedCompilation=false -nodeReuse:false
    if ($LASTEXITCODE -ne 0) { throw 'Publish failed. The partial output is not a usable preview.' }
    $required = @('Shovel.exe', 'Shovel.dll', 'Shovel.runtimeconfig.json', 'GitGui.Core.dll',
        'GitGui.ProcessHost.dll', 'Shovel.AskPass.exe', 'Shovel.AskPass.dll',
        'Shovel.AskPass.runtimeconfig.json', 'Shovel.AskPass.deps.json')
    foreach ($file in $required) {
        if (-not (Test-Path -LiteralPath (Join-Path $staging $file))) { throw ('Missing preview file: ' + $file) }
    }
    $instructions = @"
Shovel $version - local Windows x64 preview

Double-click Shovel.exe. Keep all files in this folder together.
Appearance switches the shared Dark/Light palette and saves the choice; Windows high contrast uses system colors. Shovel title bars match dark chrome using native DWM (exact colors on Windows 11, best effort on Windows 10); high contrast restores system defaults. Pickers/message boxes remain OS-owned; native title bars follow the selected palette where supported.
Requires the .NET 10 Desktop Runtime and Git for Windows 2.47.0 or newer.
Use Repository > Open to select a local worktree root; history loads without an approval prompt.
History row context menu: Checkout (Detached HEAD), Reset branch to here (Soft/Mixed/Hard), Copy full SHA and Revert. Resets confirm actual targets and consequences; merge Revert explicitly chooses a mainline parent. Progress, output and conflict recovery remain in the shell.
Fetch and Push main buttons run default targets; arrows open existing options without preparation. Push respects supported single-branch push configuration; ambiguous or multi-ref defaults require explicit options. Commit always opens its workspace.
Branch menu includes Delete local branch and Set/Unset upstream. Deletion confirms the exact local branch and uses Git's normal merged/worktree protections; no force deletion. Commit drafts persist per worktree under %LOCALAPPDATA%/GitGui/CommitDrafts. Storage errors preserve recovery information; unsaved debounce-window edits can still be lost on a crash.
File / Repository > New repository creates a local repository in a new or populated folder. Choose an absolute destination and optional initial branch; blank uses Git's default. Opening options performs no preparation. Create closes the dialog and shows progress/output in the shell, then opens the repository automatically. Existing files stay unstaged; no commit or remote is added. Existing roots open without reinitializing. Nested/bare/metadata/reparse destinations are unsupported. Failure/cancellation preserves partial files; inspect the output before retrying.
The selected saved tab loads automatically at startup; switching tabs reuses bounded cached pages. The expandable sidebar shows branches with local upstream counts, fetched remote branches, tags, stashes and submodules. Single-click refs/stashes to locate their commit; double-click a branch or press Enter to switch. Right-click for branch management, Fetch on a configured remote, or stash Apply/Pop/confirmed Drop. Click a submodule to open a child tab; its menu offers Initialize / Update to its recorded commit and Sync URL, without force or recursion. Secondary lists read after first history, with no polling or menu-opening preparation. Large ref navigation has explicit output/time limits and retains history on failure.
Repository tabs share the strip equally. Same-author history peers use muted yellow backgrounds with ordinary text; graph lanes have compact straight runs and rounded joins. Top-right Open in offers Explorer, Rider and VS Code; Console opens Git Bash (Ctrl+Alt+T). Rider prefers a root-level .uproject, then solution/C# project, otherwise the folder. Discovery happens after selection; missing apps can be selected and remembered. No Work button. External tools manage their own lifetime. Select a row for its message, author/committer, dates, hash and parents below. Open Changes for committed files/diffs; choose a parent for merges. M/+/minus mark modified/added/deleted files. Local Changes uses the same markers; its output section shows validation, Git and refresh timings.
Toolbar Commit (count) or sidebar Local changes opens the modal over retained history. The count scans the selected repository only after history is published. Opening binds cached lists/draft before background reads; Refresh/Cancel and retained output are in the modal.
Select files to Stage/Unstage; hold Shift for all listed files. Enter subject/body to Commit, or use Amend with actual-tip confirmation. Hold Shift for Commit and Push to the configured upstream; a failed push retains the successful commit. Drafts persist locally per worktree; failed commits retain text, successful commits clear the submitted text.
Right-click a Local Changes row (or Shift+F10/Menu key) for selected/all Stage or Unstage, Open, Show in File Explorer and relative/full path copying. The menu preserves a containing multiselection and captures its targets; reopen it after refresh. Explicit menu actions ignore Shift-to-all. File-grid shortcuts: Enter selected Stage/Unstage; Delete Discard confirmation; Ctrl+Shift+Alt+S all; Ctrl+C relative paths; Ctrl+Shift+C full paths; Ctrl+Shift+Alt+O Open. Enter inside a menu activates the highlighted item. Open/Explorer requires one file and uses current working content, even for staged rows; missing files use an existing containing folder. Refresh after external editing.
Discard confirms exact targets: Unstaged restores index content; Staged restores both index and worktree to HEAD, losing unstaged edits there too. Selected untracked/unborn-added files are permanently removed. No recycle bin or guaranteed undo; partial failure/cancellation is reinspected. Selected Stash saves both sides plus selected untracked files while preserving unrelated work; use Stashes to inspect/apply/pop/drop. A saved stash survives cancelled final index cleanup; do not blindly repeat the action.
External Diff uses one configured Git tool, with the selected staged/unstaged comparison and tracked progress/errors. File History/Blame opens before background reads: first-parent ancestry with rename-following 200-row pages, and committed blame at captured HEAD. Reopen for newer HEAD; limits 4 MiB/read, 128 back cursors plus Newest, 20k blame lines. Main All Commits scope is unchanged. Ignore appends literal selected paths or extension rules to root .gitignore for untracked files. LFS edits exact-path root .gitattributes rules; requires configured LFS for tracking, with no migration/restaging/install/download. Review and stage rule files; nested overrides are reported. Save as Patch exports selected text/binary changes up to 16 MiB; configured filters are suppressed, so LFS/filter-managed content can differ from transformed Git content. Rule editing and patch export limit selection to 128 paths / roughly 16k argument characters. No menu-opening Git or filesystem preparation; no automatic rollback.
Single-file actions publish real affected rows before a wider background scan. Later actions do not wait for obsolete diff/status reads. Background failure retains successful rows but marks them stale. Bottom activity bars/ticking footer are removed; static results, Cancel and top-center duration remain; selected history/refresh reads also report timing.
Diffs show colored numbered unified text, whitespace/whole-file options and hunk navigation; binaries show Old/New file cards, extension, formatted size and exact bytes. Recognized LFS pointers show asset size without downloading content. Limits: 2 MiB/20,000 lines/16,384 characters per line; unsupported encodings/linked paths have notices. Select lines in complete change blocks, Ctrl/Shift for multiple blocks, then Stage/Unstage hunk(s). Ordinary tracked text is supported; filtered/binary/added/deleted/renamed content and whitespace/whole-file preview modes use whole-file actions. Working files, unrelated staged edits and drafts are retained; stale previews fail and refresh. Ignored files, image previews, arbitrary-line staging and conflict editing are not included. Resolve conflicts externally, then stage in Local Changes, close the modal and Continue from the sidebar or Repository > Recovery. Skip and Abort confirm actual targets and can discard resolution changes.
Status suppresses clean/process filters (results may differ for filter-managed content) and reports submodule pointers only. Limits: 4 MiB / 20,000 entries, with explicit failure rather than truncation.
The sidebar lists stashes directly; Repository > Stashes opens the detailed inspector with cached entries/message before background reads. Create optionally includes untracked files; ignored files and submodule contents are excluded. Inspect saved tracked/staged/unstaged/untracked files and real text/binary diffs. Apply keeps the stash; Pop applies once then removes the entry only after success and unchanged stash list. Restore staged state is optional. Drop confirms the exact saved target and may make it unrecoverable. Conflicts/cancellation/uncertain removal preserve/report partial results; do not blindly apply again. Resolve externally and Stage in Local Changes; stash conflicts have no Continue/Abort sequence. Follow command duration at top center and retained output/Cancel in the modal. List bound: 1,000 entries / 1 MiB, explicit failure. Avoid concurrent stash mutations in other clients; their final reflog changes are not atomically covered by Shovel's gate.
All Commits includes local branches, remote-tracking branches, tags, current HEAD and saved stashes. Each ordinary stash appears once; private index/untracked parent commits are folded, with original metadata/inspector content retained. Independently reachable objects remain visible. No scope/filter selector. Saved-stash history currently supports up to 256 entries, otherwise it reports a read-limit error. Other reflog-only/unreachable commits remain excluded.
Use View > Older / Newer, Ctrl+Page Up/Down or scroll past a page edge to navigate 200-row pages without a total commit-count limit.
View shows range/status; Diagnostics exposes limits. The main footer is removed; Help retains the version. Detached HEAD is included.
Use Repository > Refresh after external changes, or Cancel for a read.
Activation refreshes cached snapshots older than 30 seconds. Right-click a tab for Refresh, Move, Close or Close others; middle click closes it.
Pull shows behind and Push shows ahead of the configured upstream's local snapshot; no remote request is made.
A dash means counts are pending/unavailable/failed; tooltips explain. Fetch/Push open explicit operation dialogs. Main Pull immediately uses the configured upstream with Rebase and auto-stash off; its adjacent arrow opens the existing Pull options modal. Modal choices do not change main-button defaults.
Fixed-size close-only options (including the Pull arrow) use cached local targets immediately, with no Git queries on opening. Fetch discovers new remote branches; Refresh updates external local changes. Submitting validates and uses the named remote's current configuration.
Repository > Add remote saves a name and URL/local path without fetching. Use Fetch, then Repository > Set upstream for a fetched branch; Unset upstream is also available. Remote edit/delete is deferred.
Branch opens create/switch dialogs. Pull defaults to Rebase with a Merge alternative and optional tracked auto-stash; normal Push supports optional tags/upstream. Review targets and click the operation button to execute, including configured hooks/helpers; there is no extra trust checkbox. Unfinished actions/options are omitted; disabled working actions explain what is missing.
Submitting closes the options window. Follow progress/elapsed time at top center, Cancel there or under Repository, and use Details (ellipsis) to see the retained Operation result/output. History reads and refreshes also report elapsed duration; capsules fade in briefly. The fixed command duration capsule includes validation/execution/final refresh; completion holds three seconds then fades over two, retaining outcome text and errors. Colors and symbols distinguish success, conflict, failure and cancellation; Windows animation/high-contrast preferences are honored. Pull updates local files/history/counts; divergent branches integrate using the selected strategy. Conflicts retain Git recovery state with Continue/Skip/Abort controls; auto-stash can conflict and staged changes can return unstaged. Repository navigation/exit waits for completion or cancellation and reinspection.
Pull respects existing sparse patterns and updates superproject submodule pointers only; submodule files are not updated or auto-stashed.
Git/OpenSSH askpass prompts are available during explicit network actions. Keep Shovel.AskPass.exe and its companion files here. Verify SSH host fingerprints independently before accepting; Shovel does not save credentials, but configured Git helpers may do so.
Credential Manager browser sign-in, interactive signing/hooks, custom SSH commands, conflict editing, force-push, search/filter, installer and updates remain deferred. Configured hooks/signing remain enabled; no bypass. Use existing terminal tools for unsupported interaction; cancellation is not undo. Local history needs no approval.

This is an unsigned, framework-dependent local preview, not the signed team distribution.
Settings are outside this folder: %LOCALAPPDATA%\GitGui\Foundation.
Checksums detect accidental changes; they are NOT signatures or proof of authenticity.
"@
    [IO.File]::WriteAllText((Join-Path $staging 'START-HERE.txt'), $instructions)
    Copy-Item -LiteralPath (Join-Path $projectRoot 'CHANGELOG.md') -Destination $staging
    $hashes = Get-ChildItem -LiteralPath $staging -File | Sort-Object Name | ForEach-Object {
        $hash = Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256
        $hash.Hash.ToLowerInvariant() + '  ' + $_.Name
    }
    $hashes | Set-Content -LiteralPath (Join-Path $staging 'SHA256SUMS.txt') -Encoding utf8
    # Validate both absolute paths before moving a directory. Never replace an existing version.
    $workspacePrefix = [IO.Path]::GetFullPath($projectRoot).TrimEnd('\') + '\'
    foreach ($candidate in @($staging, $output)) {
        if (-not [IO.Path]::GetFullPath($candidate).StartsWith($workspacePrefix, [StringComparison]::OrdinalIgnoreCase)) {
            throw 'Package path is outside the project workspace.'
        }
    }
    New-Item -ItemType Directory -Path (Split-Path -Parent $output) -Force | Out-Null
    if (Test-Path -LiteralPath $output) { throw 'A preview with this version was created during the build.' }
    [IO.Directory]::Move($staging, $output)
    Write-Output ('Preview: ' + (Join-Path $output 'Shovel.exe'))
}
finally { Pop-Location }
