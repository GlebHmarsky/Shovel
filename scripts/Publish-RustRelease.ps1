#requires -Version 7.4
[CmdletBinding()]
param([Parameter(Mandatory)][string]$ArtifactDirectory,[Parameter(Mandatory)][string]$PayloadDirectory,[string]$StableApproval)
$ErrorActionPreference='Stop'
$root=Split-Path -Parent $PSScriptRoot
. (Join-Path $PSScriptRoot 'Get-RustReleasePlan.ps1')
. (Join-Path $PSScriptRoot 'Get-RustSourceFingerprint.ps1')
$info=Get-Content (Join-Path $ArtifactDirectory 'release-info.json') -Raw | ConvertFrom-Json
$commit=git -C $root rev-parse HEAD
$branch=git -C $root branch --show-current
$pending=git -C $root status --porcelain
if($LASTEXITCODE -ne 0 -or $pending -or $commit -ne $info.SourceCommit -or $info.SourceFingerprint -ne (Get-RustSourceFingerprint $root)){throw 'Rust source does not match packaged artifacts.'}
$plan=Get-RustReleasePlan -Channel $info.Channel -Version $info.Version -SourceCommit $commit -SourceBranch $branch -StableApproval $StableApproval
if(@(Select-String -LiteralPath (Join-Path $root 'TODO.md') -Pattern '^- \[ \] \*\*R\d+').Count){throw 'Complete recorded parity/update acceptance before public Rust delivery.'}
$origin=git -C $root remote get-url origin
if($origin -notin @('https://github.com/GlebHmarsky/Shovel.git','https://github.com/GlebHmarsky/Shovel','git@github.com:GlebHmarsky/Shovel.git')){throw 'Unexpected publishing repository.'}
$feed=Join-Path $ArtifactDirectory $plan.Feed
$package=Join-Path $ArtifactDirectory $plan.Package
$installer=Join-Path $ArtifactDirectory $plan.Installer
$verifier=Join-Path $PayloadDirectory 'verify_update_bundle.exe'
& $verifier $plan.Channel $plan.Version $commit $feed "$feed.sig" $package
if($LASTEXITCODE -ne 0){throw 'Production Rust verification failed.'}
& gh release view $plan.Tag --repo $plan.Repository 2>$null
if($LASTEXITCODE -eq 0){throw 'Rust version already published. Never overwrite immutable releases.'}
git -C $root push origin "HEAD:refs/heads/codex/rust-rewrite" "HEAD:refs/heads/$($plan.ReleaseBranch)"
if($LASTEXITCODE -ne 0){throw 'Rust branch push failed. No release published.'}
$assets=@($installer,$package,$feed,"$feed.sig",(Join-Path $ArtifactDirectory 'SHA256SUMS.txt'),(Join-Path $ArtifactDirectory 'release-info.json'))
& gh release create $plan.Tag @assets --repo $plan.Repository --target $commit --prerelease --latest=false --title "Shovel Rust $($plan.Version) ($($plan.Channel))" --notes-file (Join-Path $ArtifactDirectory 'release-notes.md')
if($LASTEXITCODE -ne 0){throw 'Immutable Rust release upload failed. Feed was not advanced.'}
# Check anonymous immutable downloads before advertising the signed feed.
$probe=Join-Path $root ('artifacts/rust-release-probe-'+[guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $probe | Out-Null
$base="https://github.com/$($plan.Repository)/releases/download/$($plan.Tag)"
foreach($name in @($plan.Package,$plan.Installer)){
    Invoke-WebRequest -Uri "$base/$name" -OutFile (Join-Path $probe $name) -TimeoutSec 300
    if((Get-FileHash (Join-Path $probe $name)).Hash -ne (Get-FileHash (Join-Path $ArtifactDirectory $name)).Hash){throw 'Anonymous Rust download differs from built artifact.'}
}
& gh release view $plan.FeedTag --repo $plan.Repository 2>$null
if($LASTEXITCODE -ne 0){
    & gh release create $plan.FeedTag $feed "$feed.sig" --repo $plan.Repository --target $commit --prerelease --latest=false --title $plan.FeedTag --notes 'Signed Shovel Rust update metadata. Rust-only channel.'
}else{
    & gh release upload $plan.FeedTag $feed "$feed.sig" --repo $plan.Repository --clobber
}
if($LASTEXITCODE -ne 0){throw 'Rust feed publication failed.'}
$base="https://github.com/$($plan.Repository)/releases/download/$($plan.FeedTag)"
Invoke-WebRequest -Uri "$base/$($plan.Feed)" -OutFile (Join-Path $probe $plan.Feed) -TimeoutSec 60
Invoke-WebRequest -Uri "$base/$($plan.Feed).sig" -OutFile (Join-Path $probe "$($plan.Feed).sig") -TimeoutSec 60
& $verifier $plan.Channel $plan.Version $commit (Join-Path $probe $plan.Feed) (Join-Path $probe "$($plan.Feed).sig") (Join-Path $probe $plan.Package)
if($LASTEXITCODE -ne 0){throw 'Anonymous signed Rust feed/package probe failed.'}
Write-Output "Published $($plan.Tag) and verified the isolated Rust channel."
