#requires -Version 7.4
[CmdletBinding()]
param([ValidateSet('win-rust-beta','win-rust-stable')][string]$Channel='win-rust-beta',
    [string]$PrivateKeyPath=(Join-Path $env:USERPROFILE '.shovel-signing/shovel-update.key'),
    [switch]$PackageOnly,[string]$StableApproval)
$ErrorActionPreference='Stop'
$root=Split-Path -Parent $PSScriptRoot
. (Join-Path $PSScriptRoot 'Get-RustSourceFingerprint.ps1')
. (Join-Path $PSScriptRoot 'Get-RustReleasePlan.ps1')
$commit=git -C $root rev-parse HEAD
$branch=git -C $root branch --show-current
$pending=git -C $root status --porcelain
if($LASTEXITCODE -ne 0 -or $pending){throw 'Commit intended Rust changes before delivery.'}
$version=Get-RustVersion $root
$plan=Get-RustReleasePlan -Channel $Channel -Version $version -SourceCommit $commit -SourceBranch $branch -StableApproval $StableApproval
if(-not $PackageOnly -and $Channel -eq 'win-rust-stable'){
    $unchecked=@(Select-String -LiteralPath (Join-Path $root 'TODO.md') -Pattern '^- \[ \] \*\*R\d+')
    if($unchecked.Count){throw 'Rust Stable requires complete parity acceptance and explicit owner approval.'}
}
$smokePath=Join-Path $root 'Build/rust-smoke.json'
$smoke=if(Test-Path -LiteralPath $smokePath){Get-Content -LiteralPath $smokePath -Raw | ConvertFrom-Json}else{$null}
if(!$smoke -or !$smoke.Passed -or $smoke.SourceFingerprint -ne (Get-RustSourceFingerprint $root)){
    & (Join-Path $PSScriptRoot 'Test-RustSmoke.ps1')
}
$payload=Join-Path $root "Build/rust-$version-win-x64"
if(!(Test-Path -LiteralPath $payload)){& (Join-Path $PSScriptRoot 'Build-RustPayload.ps1') -OutputDirectory $payload}
$installer=Join-Path $root "Build/rust-$version-installer-$Channel"
if(!(Test-Path -LiteralPath $installer)){
    & (Join-Path $PSScriptRoot 'Build-RustInstaller.ps1') -PayloadDirectory $payload -Channel $Channel -PrivateKeyPath $PrivateKeyPath -StableApproval $StableApproval
}
if($PackageOnly){Write-Output 'Signed Rust installer built. No branches were merged and no public release or feed was changed.';return}
& (Join-Path $PSScriptRoot 'Publish-RustRelease.ps1') -ArtifactDirectory (Join-Path $root "Build/rust-$version-installer-$Channel") -PayloadDirectory $payload -StableApproval $StableApproval
