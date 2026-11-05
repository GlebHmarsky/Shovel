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
    . (Join-Path $PSScriptRoot 'Get-SourceFingerprint.ps1')
    @{ Version=$version; SourceFingerprint=(Get-SourceFingerprint $projectRoot); Suite='smoke' } | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $staging 'VERIFICATION.json') -Encoding utf8
    Copy-Item -LiteralPath (Join-Path $projectRoot 'LICENSE'),(Join-Path $projectRoot 'THIRD-PARTY-NOTICES.md') -Destination $staging
    Copy-Item -LiteralPath (Join-Path $projectRoot 'third-party/VELOPACK-LICENSE.txt') -Destination $staging
    # Keep this launch guide short. Owning docs describe features; duplicated manuals drift.
    $instructions = @"
Shovel $version - local Windows x64 preview

Double-click Shovel.exe and keep the files in this folder together.
Requires the .NET 10 Desktop Runtime and Git for Windows 2.47.0 or newer.
This framework-dependent portable preview does not install or apply updates.
For the self-contained installer, use the matching release at:
https://github.com/GlebHmarsky/Shovel/releases

Open a repository from File / Repository > Open. Saved tabs restore automatically.
File > Settings exposes display, history, Git, tools and update preferences.
Fetch/Pull/Push arrows remember options used by their main buttons.
F5 refreshes repository state. Commit opens Local Changes; Ctrl+Enter submits it.
Submitted user commands continue in the background when their view or tab closes.
Only waiting requests can be removed. Accepted work prevents app exit until it finishes.
The central activity view retains command results and logs; user failures need attention.

Current behavior and limitations: https://github.com/GlebHmarsky/Shovel/blob/master/docs/product.md
Build and verification commands: https://github.com/GlebHmarsky/Shovel/blob/master/README.md
Settings and drafts remain outside this folder under %LOCALAPPDATA%\GitGui.
This preview is not publisher-signed. Checksums detect corruption, not authenticity.
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
