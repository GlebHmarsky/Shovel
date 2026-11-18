# Check the bytes on disk before reusing a build or publishing its installer.
function Assert-RustPayload {
    param([string]$Directory, $Plan, [string]$Fingerprint)
    $receipt = Get-Content -LiteralPath (Join-Path $Directory 'VERIFICATION.json') -Raw | ConvertFrom-Json
    if ($receipt.Dirty -ne $false -or $receipt.SmokeVerified -ne $true -or
        $receipt.Product -cne $Plan.Product -or $receipt.Version -cne $Plan.Version -or
        $receipt.SourceCommit -cne $Plan.SourceCommit -or $receipt.SourceFingerprint -cne $Fingerprint) {
        throw 'Payload requires a clean matching source commit, version and smoke receipt.'
    }

    $files = @{}
    foreach ($entry in $receipt.Files) {
        if (!$entry.Name -or $files.ContainsKey($entry.Name)) { throw 'Invalid payload file inventory.' }
        $files[$entry.Name] = $entry
    }
    $app = Join-Path $Directory 'app'
    if ((Get-Item -LiteralPath $app).Attributes -band [IO.FileAttributes]::ReparsePoint) {
        throw 'Unexpected link in Rust application payload.'
    }
    $actual = @(foreach ($item in Get-ChildItem -LiteralPath $app -Recurse -Force) {
        if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'Unexpected link in Rust application payload.' }
        if (!$item.PSIsContainer) { $item }
    })
    if ($files.Count -ne $actual.Count -or !$files.ContainsKey($Plan.Executable) -or !$files.ContainsKey('ShovelAskPass.exe')) {
        throw 'Payload files differ from the recorded inventory.'
    }
    foreach ($file in $actual) {
        $relative = [IO.Path]::GetRelativePath($app, $file.FullName).Replace('\','/')
        $entry = $files[$relative]
        if (!$entry -or $entry.Name -cne $relative -or $entry.Size -ne $file.Length -or
            $entry.SHA256 -ne (Get-FileHash -LiteralPath $file.FullName).Hash) {
            throw "Payload file differs from receipt: $relative"
        }
    }
    if ((Get-FileHash -LiteralPath (Join-Path $Directory 'verify_update_bundle.exe')).Hash -ne $receipt.VerifierSHA256) {
        throw 'Package verifier changed.'
    }
}

function Get-RustPublicAssetNames($Plan) {
    @($Plan.Installer, $Plan.Package, $Plan.Feed, "$($Plan.Feed).sig", 'release-info.json', 'release-notes.md', 'SHA256SUMS.txt')
}

function Get-RustInstallerAssetNames($Plan) {
    # Velopack also produces these three channel-specific files for the CI archive.
    @(Get-RustPublicAssetNames $Plan) + @(
        "assets.$($Plan.Channel).json", "RELEASES-$($Plan.Channel)", "$($Plan.Product)-$($Plan.Channel)-Portable.zip"
    )
}

function Get-RustInstallerFiles([string]$Directory, $Plan) {
    $allowed = @(Get-RustInstallerAssetNames $Plan)
    foreach ($item in Get-ChildItem -LiteralPath $Directory -Force) {
        if ($item.PSIsContainer -or ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -or $item.Name -cnotin $allowed) {
            throw 'Unexpected file or directory in Rust installer output.'
        }
        if ($item.Name -cne 'SHA256SUMS.txt') { $item }
    }
}

function Read-RustChecksumMap([string]$Directory, [string[]]$AllowedNames) {
    $hashes = @{}
    foreach ($line in Get-Content -LiteralPath (Join-Path $Directory 'SHA256SUMS.txt')) {
        if ($line -cnotmatch '^([a-f0-9]{64})  ([A-Za-z0-9_.-]+)$') { throw 'Invalid checksum entry.' }
        $hash = $Matches[1]
        $name = $Matches[2]
        if ($hashes.ContainsKey($name) -or $name -ceq 'SHA256SUMS.txt' -or $name -cnotin $AllowedNames) { throw 'Unexpected release checksum entry.' }
        $hashes[$name] = $hash
    }
    return $hashes
}

function Get-RustInstallerHashes([string]$Directory, $Plan) {
    $files = @(Get-RustInstallerFiles $Directory $Plan)
    $hashes = Read-RustChecksumMap $Directory $files.Name
    foreach ($name in @(Get-RustPublicAssetNames $Plan) + $files.Name) {
        if ($name -cne 'SHA256SUMS.txt' -and !$hashes.ContainsKey($name)) { throw "Missing verified artifact: $name" }
    }
    foreach ($file in $files) {
        if ((Get-FileHash -LiteralPath $file.FullName).Hash -ne $hashes[$file.Name]) { throw "Changed release artifact: $($file.Name)" }
    }
    return $hashes
}

function Assert-RustInstallerReceipt([string]$Directory, $Plan, [string]$Fingerprint) {
    $info = Get-Content -LiteralPath (Join-Path $Directory 'release-info.json') -Raw | ConvertFrom-Json
    foreach ($field in @('Product','Version','Channel','SourceCommit','Repository','Tag','FeedTag')) {
        if ($info.$field -cne $Plan.$field) { throw "Installer receipt differs from release plan: $field" }
    }
    if ($info.SourceFingerprint -cne $Fingerprint) { throw 'Installer source fingerprint differs.' }
}

function Get-VerifiedRustReleaseHashes {
    param([string]$Directory, [string]$PayloadDirectory, $Plan, [string]$Fingerprint)
    Assert-RustPayload $PayloadDirectory $Plan $Fingerprint
    Assert-RustInstallerReceipt $Directory $Plan $Fingerprint
    $hashes = Get-RustInstallerHashes $Directory $Plan
    & (Join-Path $PayloadDirectory 'verify_update_bundle.exe') $Plan.Channel $Plan.Version $Plan.SourceCommit `
        (Join-Path $Directory $Plan.Feed) (Join-Path $Directory "$($Plan.Feed).sig") (Join-Path $Directory $Plan.Package) | Out-Host
    if ($LASTEXITCODE -ne 0) { throw 'Production Rust verification failed.' }
    return $hashes
}
