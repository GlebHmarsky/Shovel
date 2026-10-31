#requires -Version 7.4
param([Parameter(Mandatory)][string]$ArtifactDirectory)
$ErrorActionPreference = 'Stop'
$directory = (Resolve-Path -LiteralPath $ArtifactDirectory).Path
$info = Get-Content -LiteralPath (Join-Path $directory 'release-info.json') -Raw | ConvertFrom-Json
if ($info.Repository -ne 'GlebHmarsky/Shovel') { throw 'All future releases belong in GlebHmarsky/Shovel.' }
if ($info.Channel -eq 'win-stable') {
    & (Join-Path $PSScriptRoot 'Publish-StableRelease.ps1') -ArtifactDirectory $directory
    return
}
if ($info.LocalOnly -or $info.Repository -notmatch '^[A-Za-z0-9][A-Za-z0-9-]*/[A-Za-z0-9_.-]+$' -or $info.Version -notmatch '^\d+\.\d+\.\d+$' -or $info.Channel -notin @('win-stable','win-test')) { throw 'Not a configured release package.' }
foreach ($line in (Get-Content -LiteralPath (Join-Path $directory 'SHA256SUMS.txt'))) {
    if ($line -notmatch '^([a-f0-9]{64})  ([A-Za-z0-9_.-]+)$') { throw 'Invalid checksum list.' }
    if ((Get-FileHash -LiteralPath (Join-Path $directory $Matches[2]) -Algorithm SHA256).Hash -ne $Matches[1]) { throw 'Release artifact changed after packaging.' }
}
$feed = Join-Path $directory "releases.$($info.Channel).json"
if (-not (Test-Path -LiteralPath "$feed.sig")) { throw 'Missing signed feed.' }
$tag = "v$($info.Version)-$($info.Channel)"
$assets = @(Get-ChildItem -LiteralPath $directory -File | Select-Object -ExpandProperty FullName)
# No token is stored in Shovel. gh uses the publisher's local login.
# Upload to a draft first; latest stable switches only after all assets are uploaded.
& gh release create $tag @assets --repo $info.Repository --draft --title "Shovel $($info.Version)" --notes-file (Join-Path $directory 'release-notes.md')
if ($LASTEXITCODE -ne 0) { throw 'Draft creation/upload failed. Inspect the draft on GitHub; do not overwrite an existing release.' }
if ($info.Channel -eq 'win-test') {
    # Test feed uses a fixed prerelease tag. Its packages remain at their immutable versioned release.
    & gh release edit $tag --repo $info.Repository --draft=false --prerelease --latest=false
    if ($LASTEXITCODE -ne 0) { throw 'Cannot publish versioned test release.' }
    & gh release view test --repo $info.Repository *> $null
    if ($LASTEXITCODE -ne 0) {
        & gh release create test $feed "$feed.sig" --repo $info.Repository --prerelease --latest=false --title 'Shovel test feed' --notes 'Signed feed for the test channel.'
    } else {
        & gh release upload test $feed "$feed.sig" --repo $info.Repository --clobber
    }
} else { & gh release edit $tag --repo $info.Repository --draft=false --latest }
if ($LASTEXITCODE -ne 0) { throw 'Publishing failed. Existing installations remain usable; inspect release state before retrying.' }
Write-Output "Published: https://github.com/$($info.Repository)/releases/tag/$tag"
