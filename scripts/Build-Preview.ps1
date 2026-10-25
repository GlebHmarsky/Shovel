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
    dotnet run --project tests/GitGui.FoundationChecks -c Release --no-build
    if ($LASTEXITCODE -ne 0) { throw 'Verification failed; preview was not produced.' }
    dotnet publish src/GitGui.App --no-restore -c Release --self-contained false -o $staging -m:1 -p:UseSharedCompilation=false -nodeReuse:false
    if ($LASTEXITCODE -ne 0) { throw 'Publish failed. The partial output is not a usable preview.' }
    $required = @('Shovel.exe', 'Shovel.dll', 'Shovel.runtimeconfig.json', 'GitGui.Core.dll',
        'GitGui.ProcessHost.dll')
    foreach ($file in $required) {
        if (-not (Test-Path -LiteralPath (Join-Path $staging $file))) { throw ('Missing preview file: ' + $file) }
    }
    $instructions = @"
Shovel $version - local Windows x64 preview

Double-click Shovel.exe. Keep all files in this folder together.
Requires the .NET 10 Desktop Runtime and Git for Windows 2.47.0 or newer.
Use Repository > Open to select a local worktree root; history loads without an approval prompt.
The selected saved tab loads automatically at startup; switching tabs reuses bounded cached pages.
Select a row for its message, author/committer, dates, hash and parents below.
All Commits includes local branches, remote-tracking branches, tags and current HEAD. No scope/filter selector.
Use Older / Newer to navigate 200-row pages without a total commit-count limit.
The footer shows row range/status; tooltips expose read/graph limits. Detached HEAD is included.
Use Repository > Refresh after external changes, or Cancel for a read.
Activation refreshes cached snapshots older than 30 seconds. Right-click a tab for Refresh, Move, Close or Close others; middle click closes it.
Pull shows behind and Push shows ahead of the configured upstream's local snapshot; no remote request is made.
A dash means counts are pending/unavailable/failed; tooltips explain. Pull/Push do not execute yet.
Menus, top toolbar and sidebar prototypes are restored; unavailable actions are disabled.
Search/filter, file status, diffs, mutations, installer and automatic updates are not implemented.

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
