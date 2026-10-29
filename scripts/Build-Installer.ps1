#requires -Version 7.4
[CmdletBinding(DefaultParameterSetName='Release')]
param(
    [Parameter(Mandatory, ParameterSetName='Release')][ValidatePattern('^[A-Za-z0-9][A-Za-z0-9-]*/[A-Za-z0-9_.-]+$')][string]$Repository,
    [Parameter(Mandatory, ParameterSetName='Release')][string]$PublicKeyPath,
    [Parameter(Mandatory, ParameterSetName='Release')][string]$PrivateKeyPath,
    [Parameter(Mandatory, ParameterSetName='Local')][switch]$LocalOnly,
    [ValidateSet('win-stable','win-test')][string]$Channel = 'win-stable',
    [string]$AuthenticodeSignParameters
)
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
. (Join-Path $PSScriptRoot 'Get-SourceFingerprint.ps1')
$version = [string]([xml](Get-Content -LiteralPath (Join-Path $root 'Directory.Build.props') -Raw)).Project.PropertyGroup.Version
if ($version -notmatch '^\d+\.\d+\.\d+$') { throw 'Invalid project version.' }
$preview = Join-Path $root "Build/$version-win-x64"
$evidencePath = Join-Path $preview 'VERIFICATION.json'
if (-not (Test-Path -LiteralPath $evidencePath)) { throw 'Build the matching preview with Build-Preview.ps1 first.' }
$evidence = Get-Content -LiteralPath $evidencePath -Raw | ConvertFrom-Json
if ($evidence.Version -ne $version -or $evidence.SourceFingerprint -ne (Get-SourceFingerprint $root)) { throw 'Executable source changed since the verified preview. Build a matching preview before packaging.' }
$output = Join-Path $root "Build/$version-installer-$Channel"
if (Test-Path -LiteralPath $output) { throw 'Installer version already exists; earlier packages are never overwritten.' }
$work = Join-Path $root ('artifacts/installer-' + [Guid]::NewGuid().ToString('N'))
$publish = Join-Path $work 'publish'; $releases = Join-Path $work 'releases'
New-Item -ItemType Directory -Path $publish,$releases -Force | Out-Null
$key = $null
try {
    $publicKey = ''
    if (-not $LocalOnly) {
        $key = [Security.Cryptography.RSA]::Create()
        $key.ImportFromPem([IO.File]::ReadAllText((Resolve-Path -LiteralPath $PrivateKeyPath)))
        $publicKey = [IO.File]::ReadAllText((Resolve-Path -LiteralPath $PublicKeyPath))
        if ($key.KeySize -lt 3072 -or $key.ExportSubjectPublicKeyInfoPem().Trim() -ne $publicKey.Trim()) { throw 'Signing key does not match the public verification key.' }
    }
    $config = Join-Path $work 'updates.json'
    @{ Repository = $(if ($LocalOnly) { '' } else { $Repository }); PublicKey = $publicKey; Channel = $Channel } | ConvertTo-Json | Set-Content -LiteralPath $config -Encoding utf8NoBOM
    Push-Location $root
    try {
        dotnet tool restore
        if ($LASTEXITCODE -ne 0) { throw 'Packaging tool restore failed.' }
        dotnet publish src/GitGui.App -c Release -r win-x64 --self-contained true -o $publish -m:1 -p:RuntimeFrameworkVersion=10.0.12 "-p:UpdateConfigurationFile=$config" -p:UseSharedCompilation=false -nodeReuse:false
        if ($LASTEXITCODE -ne 0) { throw 'Self-contained publish failed.' }
        # AskPass must be self-contained as well. Its runtime config shares the packaged runtime.
        dotnet publish src/GitGui.AskPass -c Release -r win-x64 --self-contained true -o $publish -m:1 -p:RuntimeFrameworkVersion=10.0.12 -p:UseSharedCompilation=false -nodeReuse:false
        if ($LASTEXITCODE -ne 0) { throw 'AskPass publish failed.' }
        foreach ($file in @('Shovel.exe','Shovel.AskPass.exe','coreclr.dll','hostfxr.dll','PresentationFramework.dll')) {
            if (-not (Test-Path -LiteralPath (Join-Path $publish $file))) { throw "Missing self-contained file: $file" }
        }
        Copy-Item -LiteralPath (Join-Path $root 'LICENSE'),(Join-Path $root 'THIRD-PARTY-NOTICES.md'),(Join-Path $root 'CHANGELOG.md') -Destination $publish
        $nugetRoot = if ($env:NUGET_PACKAGES) { $env:NUGET_PACKAGES } else { Join-Path $env:USERPROFILE '.nuget/packages' }
        $license = Join-Path $root 'third-party/VELOPACK-LICENSE.txt'
        if (-not (Test-Path -LiteralPath $license)) { throw 'Velopack license missing from restored package.' }
        Copy-Item -LiteralPath $license -Destination (Join-Path $publish 'VELOPACK-LICENSE.txt')
        foreach ($pack in @('microsoft.netcore.app.runtime.win-x64','microsoft.windowsdesktop.app.runtime.win-x64')) {
            foreach ($notice in @('LICENSE.TXT','THIRD-PARTY-NOTICES.TXT')) {
                $path = Join-Path $nugetRoot "$pack/10.0.12/$notice"
                if (Test-Path -LiteralPath $path) { Copy-Item -LiteralPath $path -Destination (Join-Path $publish "$pack-$notice") }
            }
        }
        $changelog = [IO.File]::ReadAllText((Join-Path $root 'CHANGELOG.md'))
        $section = [regex]::Match($changelog, '(?ms)^## ' + [regex]::Escape($version) + '\b[^\r\n]*\r?\n.*?(?=^## |\z)').Value
        if (-not $section -or $section.Length -gt 60000) { throw 'Current version needs bounded release notes in CHANGELOG.md.' }
        $notes = Join-Path $releases 'release-notes.md'
        [IO.File]::WriteAllText($notes, $section)
        $packArgs = @('vpk' ,'pack','--packId','Shovel','--packVersion',$version,'--packDir',$publish,'--mainExe','Shovel.exe','--packTitle','Shovel','--packAuthors','Shovel contributors','--channel',$Channel,'--runtime','win-x64','--delta','None','--icon',(Join-Path $root 'src/GitGui.App/Assets/Shovel.ico'),'--outputDir',$releases,'--releaseNotes',$notes)
        if ($AuthenticodeSignParameters) { $packArgs += @('--signParams',$AuthenticodeSignParameters) }
        & dotnet @packArgs
        if ($LASTEXITCODE -ne 0) { throw 'Installer packaging failed.' }
        $setups = @(Get-ChildItem -LiteralPath $releases -Filter '*Setup.exe' -File)
        if ($setups.Count -ne 1) { throw 'Expected exactly one Windows setup executable.' }
        $setup = Join-Path $releases 'Shovel-Setup.exe'
        if ($setups[0].FullName -ne $setup) { [IO.File]::Move($setups[0].FullName, $setup) }
        $feed = Join-Path $releases "releases.$Channel.json"
        $package = Join-Path $releases "Shovel-$version-$Channel-full.nupkg"
        foreach ($path in @($feed,$package,$setup)) {
            if (-not (Test-Path -LiteralPath $path)) { throw "Expected release artifact missing: $path" }
        }
        # Publish only the current full package, retaining the framework's feed format.
        $metadata = Get-Content -LiteralPath $feed -Raw | ConvertFrom-Json
        $metadata.Assets = @($metadata.Assets | Where-Object { $_.Type -eq 'Full' -and $_.Version -eq $version })
        if ($metadata.Assets.Count -ne 1) { throw 'Expected one full package in release feed.' }
        if ($metadata.Assets[0].FileName -ne [IO.Path]::GetFileName($package)) { throw 'Unexpected package filename.' }
        $metadata | ConvertTo-Json -Depth 12 | Set-Content -LiteralPath $feed -Encoding utf8NoBOM
        if ($key) {
            $signature = $key.SignData([IO.File]::ReadAllBytes($feed), [Security.Cryptography.HashAlgorithmName]::SHA256, [Security.Cryptography.RSASignaturePadding]::Pss)
            [IO.File]::WriteAllBytes("$feed.sig", $signature)
        }
        @{ Version=$version; Repository=$(if ($LocalOnly) { '' } else { $Repository }); Channel=$Channel; SourceFingerprint=$evidence.SourceFingerprint; LocalOnly=[bool]$LocalOnly; Authenticode=[bool]$AuthenticodeSignParameters } | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $releases 'release-info.json') -Encoding utf8NoBOM
        Get-ChildItem -LiteralPath $releases -File | Sort-Object Name | ForEach-Object { (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant() + '  ' + $_.Name } | Set-Content -LiteralPath (Join-Path $releases 'SHA256SUMS.txt') -Encoding utf8NoBOM
        $prefix = [IO.Path]::GetFullPath($root).TrimEnd('\') + '\'
        foreach ($candidate in @($releases,$output)) {
            if (-not [IO.Path]::GetFullPath($candidate).StartsWith($prefix, [StringComparison]::OrdinalIgnoreCase)) { throw 'Installer output outside workspace.' }
        }
        [IO.Directory]::Move($releases, $output)
        Write-Output "Installer artifacts: $output"
        if ($LocalOnly) { Write-Warning 'Local-only installer: update hosting/key unset. Do not publish this as an auto-updating release.' }
        if (-not $AuthenticodeSignParameters) { Write-Warning 'Windows binaries are unsigned; Windows may display publisher/SmartScreen warnings.' }
    } finally { Pop-Location }
} finally { if ($key) { $key.Dispose() } }
