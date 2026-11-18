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
    $actual = @(Get-ChildItem -LiteralPath $app -Recurse -File -Force)
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

function Get-VerifiedRustReleaseHashes {
    param([string]$Directory, [string]$PayloadDirectory, $Plan, [string]$Fingerprint)
    Assert-RustPayload $PayloadDirectory $Plan $Fingerprint
    $info = Get-Content -LiteralPath (Join-Path $Directory 'release-info.json') -Raw | ConvertFrom-Json
    foreach ($field in @('Product','Version','Channel','SourceCommit','Repository','Tag','FeedTag')) {
        if ($info.$field -cne $Plan.$field) { throw "Installer receipt differs from release plan: $field" }
    }
    if ($info.SourceFingerprint -cne $Fingerprint) { throw 'Installer source fingerprint differs.' }
    $hashes = @{}
    foreach ($line in Get-Content -LiteralPath (Join-Path $Directory 'SHA256SUMS.txt')) {
        if ($line -cnotmatch '^([a-f0-9]{64})  ([A-Za-z0-9_.-]+)$') { throw 'Invalid checksum entry.' }
        $hash = $Matches[1]
        $name = $Matches[2]
        if ($hashes.ContainsKey($name) -or $name -match '\.(key|pem|pfx|p12)$') { throw 'Unexpected release asset.' }
        if ((Get-FileHash -LiteralPath (Join-Path $Directory $name)).Hash -ne $hash) { throw "Changed release artifact: $name" }
        $hashes[$name] = $hash
    }
    foreach ($name in @($Plan.Installer,$Plan.Package,$Plan.Feed,"$($Plan.Feed).sig",'release-info.json','release-notes.md')) {
        if (!$hashes.ContainsKey($name)) { throw "Missing verified artifact: $name" }
    }
    & (Join-Path $PayloadDirectory 'verify_update_bundle.exe') $Plan.Channel $Plan.Version $Plan.SourceCommit `
        (Join-Path $Directory $Plan.Feed) (Join-Path $Directory "$($Plan.Feed).sig") (Join-Path $Directory $Plan.Package) | Out-Host
    if ($LASTEXITCODE -ne 0) { throw 'Production Rust verification failed.' }
    return $hashes
}
