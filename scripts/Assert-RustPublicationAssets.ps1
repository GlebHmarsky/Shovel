# Validate the immutable GitHub release before publishing its feed or resuming an interrupted publication.
# Source this file together with Assert-RustReleaseArtifacts.ps1.
function Get-RustReleaseAssetMap($Release, $Plan) {
    if ($Release.tag_name -cne $Plan.Tag -or $Release.target_commitish -cne $Plan.SourceCommit -or
        $Release.prerelease -ne $Plan.GitHubPrerelease -or $Release.draft -isnot [bool]) {
        throw 'Versioned release does not belong to the requested source and channel.'
    }
    $allowed = @(Get-RustPublicAssetNames $Plan)
    $assets = @{}
    foreach ($asset in $Release.assets) {
        $unfinished = $Release.draft -and $asset.state -ceq 'starter' -and $asset.size -eq 0 -and $asset.id -gt 0
        if ($asset.name -cnotin $allowed -or $assets.ContainsKey($asset.name) -or
            (!$unfinished -and ($asset.state -cne 'uploaded' -or $asset.size -le 0 -or $asset.digest -cnotmatch '^sha256:[0-9a-f]{64}$'))) {
            throw 'Unexpected or incomplete versioned release asset.'
        }
        $assets[$asset.name] = $asset
    }
    if (!$Release.draft -and $assets.Count -ne $allowed.Count) { throw 'Incomplete public release. Immutable assets cannot be replaced.' }
    return $assets
}

function Test-RustAssetBytes($Asset, [IO.FileInfo]$File) {
    if (!$Asset -or $Asset.state -cne 'uploaded' -or $Asset.size -ne $File.Length) { return $false }
    return $Asset.digest -ceq ('sha256:' + (Get-FileHash -LiteralPath $File.FullName).Hash.ToLowerInvariant())
}

function Get-RustDraftRepairs($Release, $Plan, [string]$Directory) {
    if (!$Release.draft) { throw 'Cannot upload to a public versioned release.' }
    $assets = Get-RustReleaseAssetMap $Release $Plan
    # Check all existing bytes before uploading anything else. A rebuilt package
    # may have a different signature or installer. Never mix two artifact sets.
    foreach ($asset in $assets.Values) {
        if ($asset.state -ceq 'uploaded' -and !(Test-RustAssetBytes $asset (Get-Item -LiteralPath (Join-Path $Directory $asset.name)))) {
            throw 'Draft release contains different bytes. Resume using its original Windows build artifact.'
        }
    }
    foreach ($name in Get-RustPublicAssetNames $Plan) {
        $asset = $assets[$name]
        if (!$asset -or $asset.state -ceq 'starter') { [pscustomobject]@{Name=$name; EmptyUploadId=$asset.id} }
    }
}

function Get-RustDraftArtifactId([string]$Body) {
    $marker = [regex]::Matches($Body, '(?m)^<!-- Shovel Windows artifact ([1-9][0-9]*) -->\r?$')
    if ($marker.Count -gt 1) { throw 'Ambiguous Windows build artifact in release draft.' }
    if ($marker.Count) { $marker[0].Groups[1].Value }
}

function Assert-RustRecoveryRun($Artifact, $Run, [string]$Commit, [string]$ArtifactId, [string]$Repository) {
    if ($Artifact.expired -ne $false -or [string]$Artifact.id -cne $ArtifactId -or $Artifact.workflow_run.head_sha -cne $Commit -or
        $Run.head_sha -cne $Commit -or $Run.id -ne $Artifact.workflow_run.id -or $Run.head_branch -cne 'master' -or
        $Run.event -cnotin @('push','workflow_dispatch') -or $Run.path -cne '.github/workflows/rust-windows.yml' -or
        $Run.repository.full_name -cne $Repository -or $Run.head_repository.full_name -cne $Repository -or
        $Artifact.name -cnotmatch ('^Shovel-Windows-' + [regex]::Escape($Commit) + '-' + [regex]::Escape([string]$Run.id) + '-[1-9][0-9]*$')) {
        throw 'The original archive must be an unexpired Shovel Windows master build of this exact source.'
    }
}

function Get-RustFeedRepairs($Alias, $Plan, [string]$Directory) {
    $names = @($Plan.Feed, "$($Plan.Feed).sig")
    if ($Alias.tag_name -cne $Plan.FeedTag -or !$Alias.prerelease) { throw 'Unexpected Rust feed identity.' }
    $current = @{}
    foreach ($asset in $Alias.assets) {
        if ($asset.name -cnotin $names -or $current.ContainsKey($asset.name) -or $asset.id -le 0) {
            throw 'Unexpected asset in Rust feed alias.'
        }
        $current[$asset.name] = $asset
    }
    foreach ($name in $names) {
        $old = $current[$name]
        if (!(Test-RustAssetBytes $old (Get-Item -LiteralPath (Join-Path $Directory $name)))) {
            [pscustomobject]@{Name=$name; Existing=$old}
        }
    }
}

function Assert-RustPublishedBundle($Release, $Plan, [string]$Directory, [string]$Fingerprint, [string]$Verifier) {
    if ($Release.draft) { throw 'Versioned Rust release is still a draft.' }
    $assets = Get-RustReleaseAssetMap $Release $Plan
    $hashes = Read-RustChecksumMap $Directory @(Get-RustInstallerAssetNames $Plan)
    foreach ($name in Get-RustPublicAssetNames $Plan) {
        $asset = $assets[$name]
        if (!(Test-RustAssetBytes $asset (Get-Item -LiteralPath (Join-Path $Directory $name))) -or
            ($name -cne 'SHA256SUMS.txt' -and $asset.digest -cne ('sha256:' + $hashes[$name]))) {
            throw "Immutable Rust download differs from the release digest or checksum: $name"
        }
    }
    Assert-RustInstallerReceipt $Directory $Plan $Fingerprint
    & $Verifier $Plan.Channel $Plan.Version $Plan.SourceCommit (Join-Path $Directory $Plan.Feed) (Join-Path $Directory "$($Plan.Feed).sig") (Join-Path $Directory $Plan.Package) | Out-Host
    if ($LASTEXITCODE -ne 0) { throw 'Immutable public Rust feed or package failed production verification.' }
}
