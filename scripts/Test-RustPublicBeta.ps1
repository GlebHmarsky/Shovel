#requires -Version 7.4
param(
    [Parameter(Mandatory)][string]$Version,
    [Parameter(Mandatory)][string]$SourceCommit,
    [Parameter(Mandatory)][long]$ActionsRun,
    [Parameter(Mandatory)][string]$Verifier,
    [securestring]$MetadataToken
)
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'Get-RustReleasePlan.ps1')
. (Join-Path $PSScriptRoot 'Get-RustReleaseTag.ps1')
. (Join-Path $PSScriptRoot 'Assert-RustReleaseArtifacts.ps1')
. (Join-Path $PSScriptRoot 'Assert-RustPublicationAssets.ps1')

$root = Split-Path -Parent $PSScriptRoot
$verifierPath = (Resolve-Path -LiteralPath $Verifier).Path
$plan = Get-RustReleasePlan win-rust-beta $Version $SourceCommit master
$repositoryUrl = "https://github.com/$($plan.Repository)"
$api = "https://api.github.com/repos/$($plan.Repository)"
$metadataAuth = @{}
if ($MetadataToken) {
    $metadataAuth.Authentication = 'Bearer'
    $metadataAuth.Token = $MetadataToken
}
$run = Invoke-RestMethod "$api/actions/runs/$ActionsRun" -TimeoutSec 30 @metadataAuth
if ($run.path -cne '.github/workflows/rust-windows.yml' -or
    $run.head_branch -cne 'master' -or $run.head_sha -cne $SourceCommit -or
    $run.status -cne 'completed' -or $run.conclusion -cne 'success') {
    throw 'The requested master Windows Beta workflow did not succeed for this source.'
}
if (!(Get-RustReleaseTag $root "$repositoryUrl.git" $plan)) {
    throw 'The immutable Beta tag is missing.'
}

$release = Invoke-RestMethod "$api/releases/tags/$($plan.Tag)" -TimeoutSec 30 @metadataAuth
$assets = Get-RustReleaseAssetMap $release $plan
if ($release.draft -or $assets.Count -ne 7) {
    throw 'The public Windows Beta release is incomplete.'
}
# A fresh directory prevents a failed retry from recording old or partial bytes.
$attempt = [Guid]::NewGuid().ToString('N')
$directory = Join-Path $root "Build/rust-$Version-public-verification/$attempt"
New-Item -ItemType Directory -Path $directory | Out-Null
$names = Get-RustPublicAssetNames $plan
foreach ($name in $names) {
    Invoke-WebRequest "$repositoryUrl/releases/download/$($plan.Tag)/$name" `
        -OutFile (Join-Path $directory $name) -TimeoutSec 120
}
$info = Get-Content -Raw -LiteralPath (Join-Path $directory 'release-info.json') | ConvertFrom-Json
Assert-RustPublishedBundle $release $plan $directory $info.SourceFingerprint $verifierPath

$hashes = [ordered]@{}
foreach ($name in $names) {
    $hashes[$name] = (Get-FileHash -LiteralPath (Join-Path $directory $name)).Hash
}
foreach ($name in @($plan.Feed, "$($plan.Feed).sig")) {
    $alias = Join-Path $directory "alias-$name"
    Invoke-WebRequest "$repositoryUrl/releases/download/$($plan.FeedTag)/$name" -OutFile $alias -TimeoutSec 90
    if ((Get-FileHash -LiteralPath $alias).Hash -cne $hashes[$name]) {
        throw "Live Beta $name differs from the verified release."
    }
}
$installer = Get-Item -LiteralPath (Join-Path $directory $plan.Installer)
if ($installer.VersionInfo.ProductVersion -cne $Version) {
    throw 'The downloaded installer reports a different product version.'
}
$receipt = [ordered]@{
    Version = $Version
    Channel = $plan.Channel
    Commit = $SourceCommit
    ActionsRun = $ActionsRun
    Release = $release.html_url
    VerifiedUTC = [DateTimeOffset]::UtcNow.ToString('o')
    AssetSHA256 = $hashes
    LiveFeedMatchesVerifiedSignature = $true
    InstallerSize = $installer.Length
    InstallerProductVersion = $installer.VersionInfo.ProductVersion
    InstallerAuthenticode = (Get-AuthenticodeSignature -FilePath $installer.FullName).Status.ToString()
    HostedSourceFingerprint = $info.SourceFingerprint
}
$path = Join-Path $directory 'PUBLICATION.json'
$receipt | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath $path -Encoding utf8NoBOM
Write-Output $path
