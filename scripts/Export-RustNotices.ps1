#requires -Version 7.4
[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$MetadataPath,
    [Parameter(Mandatory)][string]$OutputDirectory,
    [Parameter(Mandatory)][string]$ToolchainDirectory
)
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$metadata = Get-Content -LiteralPath $MetadataPath -Raw | ConvertFrom-Json
$overrides = Get-Content -LiteralPath (Join-Path $root 'distribution/rust-license-overrides.json') -Raw | ConvertFrom-Json
if ($overrides.SchemaVersion -ne 1 -or !$metadata.resolve.nodes) { throw 'Expected resolved Cargo metadata and versioned license overrides.' }
$resolved = [Collections.Generic.HashSet[string]]::new([StringComparer]::Ordinal)
foreach ($node in $metadata.resolve.nodes) { [void]$resolved.Add($node.id) }
$components = [Collections.Generic.List[object]]::new()
$texts = @{}
$utf8 = [Text.UTF8Encoding]::new($false,$true)
$rustDocs = Join-Path $ToolchainDirectory 'share/doc/rust'
$standardNotices = Join-Path $rustDocs 'COPYRIGHT-library.html'
if (!(Test-Path -LiteralPath $standardNotices) -or !(Test-Path -LiteralPath (Join-Path $rustDocs 'licenses/MIT.txt'))) { throw 'Pinned Rust standard-library notices are required.' }
foreach ($package in ($metadata.packages | Where-Object { $_.source -and $resolved.Contains($_.id) } | Sort-Object name,version)) {
    if ([string]::IsNullOrWhiteSpace($package.license) -and !$package.license_file) { throw "No license declaration for $($package.name) $($package.version)." }
    $directory = Split-Path -Parent $package.manifest_path
    $files = [Collections.Generic.List[object]]::new()
    foreach ($file in (Get-ChildItem -LiteralPath $directory -File -Recurse | Where-Object {
        $_.Name -match '(?i)(license|licence|copying|notice)' -or
        ($_.Extension -eq '.txt' -and $_.DirectoryName -match '(?i)[\\/]fonts$')
    })) {
        $files.Add(@{Path=$file.FullName; Attribution=[IO.Path]::GetRelativePath($directory,$file.FullName).Replace('\','/'); Source='Cargo.lock registry package'})
    }
    if ($package.license_file) {
        $licenseFile = [IO.Path]::GetFullPath((Join-Path $directory $package.license_file))
        if (!$licenseFile.StartsWith($directory.TrimEnd('\')+'\',[StringComparison]::OrdinalIgnoreCase)) { throw 'Crate license path escapes its package.' }
        $files.Add(@{Path=$licenseFile;Attribution=$package.license_file;Source='Cargo.lock registry package'})
    }
    foreach ($extra in @($overrides.Packages | Where-Object { $_.Package -eq $package.name -and $_.Version -eq $package.version })) {
        foreach ($file in $extra.Files) {
            $path = [IO.Path]::GetFullPath((Join-Path $root $file.Path))
            if (!$path.StartsWith($root.TrimEnd('\')+'\',[StringComparison]::OrdinalIgnoreCase) -or
                (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash -ne $file.SHA256) { throw 'Supplemental license hash/path does not match reviewed provenance.' }
            $files.Add(@{Path=$path;Attribution=$file.Path;Source=$file.Source})
        }
    }
    if (!$files.Count) { throw "License text missing for $($package.name) $($package.version); add exact-version upstream provenance." }
    $licenses = @($files | Sort-Object Path -Unique | ForEach-Object {
        $bytes = [IO.File]::ReadAllBytes($_.Path)
        if (!$bytes.Length -or $bytes.Length -gt 2MB) { throw 'License text is empty or exceeds its bound.' }
        $hash = [Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($bytes)).ToLowerInvariant()
        $texts[$hash] = $utf8.GetString($bytes)
        [ordered]@{File=$_.Attribution;SHA256=$hash;Source=$_.Source}
    })
    $components.Add([ordered]@{Name=$package.name;Version=$package.version;License=$package.license;Repository=$package.repository;Licenses=$licenses})
}
$builder = [Text.StringBuilder]::new()
[void]$builder.AppendLine('Shovel Rust — third-party notices')
[void]$builder.AppendLine('This conservative inventory includes the Windows target dependency graph, including build and test dependencies. Their inclusion does not imply that every component is present in the installed executable. Rust code and fonts are compiled into the native executables; the Velopack installer/updater is distributed with its own upstream notices.')
[void]$builder.AppendLine('Full license texts follow the component index. Repeated texts are included once and identified by their SHA-256. Upstream attributions and license terms are preserved.')
[void]$builder.AppendLine('The Rust standard-library copyright and license notices are included separately in rust-standard-library/COPYRIGHT-library.html and its licenses directory, copied from the pinned toolchain distribution.')
foreach ($component in $components) {
    [void]$builder.AppendLine("`n$($component.Name) $($component.Version) — $($component.License)")
    [void]$builder.AppendLine($component.Repository)
    foreach ($license in $component.Licenses) { [void]$builder.AppendLine("  $($license.File): $($license.SHA256)`n  $($license.Source)") }
}
foreach ($hash in ($texts.Keys | Sort-Object -CaseSensitive)) {
    [void]$builder.AppendLine("`n================================================================`nLicense text SHA-256: $hash`n================================================================")
    [void]$builder.AppendLine($texts[$hash])
}
New-Item -ItemType Directory -Path $OutputDirectory -Force | Out-Null
$textPath = Join-Path $OutputDirectory 'THIRD-PARTY-NOTICES.txt'
$jsonPath = Join-Path $OutputDirectory 'THIRD-PARTY-COMPONENTS.json'
if ((Test-Path -LiteralPath $textPath) -or (Test-Path -LiteralPath $jsonPath)) { throw 'Do not overwrite previously generated notices.' }
$standardOutput = Join-Path $OutputDirectory 'rust-standard-library'
if (Test-Path -LiteralPath $standardOutput) { throw 'Do not overwrite previously generated standard-library notices.' }
New-Item -ItemType Directory -Path $standardOutput | Out-Null
Copy-Item -LiteralPath $standardNotices -Destination $standardOutput
Copy-Item -LiteralPath (Join-Path $rustDocs 'licenses') -Destination $standardOutput -Recurse
[IO.File]::WriteAllText($textPath,$builder.ToString(),$utf8)
[ordered]@{SchemaVersion=1;Target='x86_64-pc-windows-msvc';CargoLockSHA256=(Get-FileHash -LiteralPath (Join-Path $root 'Cargo.lock') -Algorithm SHA256).Hash.ToLowerInvariant();Components=@($components)} | ConvertTo-Json -Depth 10 | Set-Content -LiteralPath $jsonPath -Encoding utf8NoBOM
[pscustomobject]@{Components=$components.Count;UniqueTexts=$texts.Count;NoticesSHA256=(Get-FileHash -LiteralPath $textPath -Algorithm SHA256).Hash.ToLowerInvariant()}
