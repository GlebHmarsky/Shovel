#requires -Version 7.4
param([string]$PrivateKeyPath=(Join-Path $env:USERPROFILE '.shovel-signing/shovel-update.key'))
$ErrorActionPreference='Stop'
$root=Split-Path -Parent $PSScriptRoot
$branch=git -C $root branch --show-current
if($LASTEXITCODE -ne 0 -or $branch -ne 'master'){throw 'Stable deliveries must be built from master.'}
$pending=git -C $root status --porcelain
if($LASTEXITCODE -ne 0 -or $pending){throw 'Commit the intended release changes before running Build-Release.'}
$sourceCommit=git -C $root rev-parse HEAD
$origin=git -C $root remote get-url origin
if($LASTEXITCODE -ne 0 -or $origin -notin @('https://github.com/GlebHmarsky/Shovel.git','https://github.com/GlebHmarsky/Shovel','git@github.com:GlebHmarsky/Shovel.git')){throw 'Origin must be the Shovel source/release repository.'}
. (Join-Path $PSScriptRoot 'Get-SourceFingerprint.ps1')
$version=[string]([xml](Get-Content (Join-Path $root 'Directory.Build.props') -Raw)).Project.PropertyGroup.Version
if($version -notmatch '^\d+\.\d+\.\d+$'){throw 'Invalid release version.'}
if(!(Test-Path -LiteralPath $PrivateKeyPath)){throw 'Restore the existing release-signing key before delivery; never generate a replacement for a routine release.'}
$preview=Join-Path $root "Build/$version-win-x64"
$installer=Join-Path $root "Build/$version-installer-win-stable"
if(!(Test-Path $preview)){ & (Join-Path $PSScriptRoot 'Build-Preview.ps1') }
$evidence=Get-Content (Join-Path $preview 'VERIFICATION.json') -Raw | ConvertFrom-Json
if($evidence.Version -ne $version -or $evidence.SourceFingerprint -ne (Get-SourceFingerprint $root)){throw 'Preview does not match current source. Increment the version; do not overwrite a previous build.'}
if(!(Test-Path $installer)){ & (Join-Path $PSScriptRoot 'Build-Installer.ps1') -PrivateKeyPath $PrivateKeyPath }
$info=Get-Content (Join-Path $installer 'release-info.json') -Raw | ConvertFrom-Json
if($info.SourceFingerprint -ne $evidence.SourceFingerprint -or $info.Version -ne $version -or $info.SourceCommit -ne $sourceCommit){throw 'Installer does not match verified source commit.'}
$env:GIT_TERMINAL_PROMPT='0'; $env:GCM_INTERACTIVE='Never'
git -C $root push origin master
if($LASTEXITCODE -ne 0){throw 'Source push failed; no release was published.'}
& (Join-Path $PSScriptRoot 'Publish-Release.ps1') -ArtifactDirectory $installer
& (Join-Path $PSScriptRoot 'Test-PublishedUpdate.ps1') -ExpectedVersion $version
Write-Output "Delivery complete: Shovel $version is available through Help > Shovel updates."
