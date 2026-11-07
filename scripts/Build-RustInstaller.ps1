#requires -Version 7.4
[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$PayloadDirectory,
    [ValidateSet('win-rust-beta','win-rust-stable')][string]$Channel='win-rust-beta',
    [string]$PrivateKeyPath=(Join-Path $env:USERPROFILE '.shovel-signing/shovel-update.key'),
    [string]$StableApproval
)
$ErrorActionPreference='Stop'
$root=Split-Path -Parent $PSScriptRoot
. (Join-Path $PSScriptRoot 'Get-RustReleasePlan.ps1')
. (Join-Path $PSScriptRoot 'Get-RustSourceFingerprint.ps1')
$branch=git -C $root branch --show-current
$commit=git -C $root rev-parse HEAD
$pending=git -C $root status --porcelain
if($LASTEXITCODE -ne 0 -or $pending){throw 'Commit intended Rust source before packaging.'}
$version=Get-RustVersion $root
$plan=Get-RustReleasePlan -Channel $Channel -Version $version -SourceCommit $commit -SourceBranch $branch -StableApproval $StableApproval
$payload=[IO.Path]::GetFullPath($PayloadDirectory)
$evidence=Get-Content (Join-Path $payload 'VERIFICATION.json') -Raw | ConvertFrom-Json
if($evidence.Dirty -or $evidence.Product -ne 'ShovelRust' -or $evidence.Version -ne $version -or
   $evidence.SourceCommit -ne $commit -or $evidence.SourceFingerprint -ne (Get-RustSourceFingerprint $root)){
    throw 'Installer requires a matching clean native Rust payload.'
}
foreach($file in $evidence.Files){
    $path=Join-Path (Join-Path $payload 'app') $file.Name
    if((Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant() -ne $file.SHA256){
        throw 'Verified Rust payload changed before packaging.'
    }
}
$output=Join-Path $root "Build/rust-$version-installer-$Channel"
if(Test-Path -LiteralPath $output){throw 'Installer already exists. Increment the version instead of overwriting it.'}
if(!(Test-Path -LiteralPath $PrivateKeyPath)){throw 'The existing signing key is required. Do not create a replacement.'}
$key=[Security.Cryptography.RSA]::Create()
$public=[Security.Cryptography.RSA]::Create()
try{
    $key.ImportFromPem([IO.File]::ReadAllText($PrivateKeyPath))
    $public.ImportFromPem([IO.File]::ReadAllText((Join-Path $root 'distribution/update-public.pem')))
    if($key.ExportSubjectPublicKeyInfoPem() -ne $public.ExportSubjectPublicKeyInfoPem()){throw 'Signing key does not match committed verifier.'}
    New-Item -ItemType Directory -Path $output | Out-Null
    Push-Location $root
    try{
        & dotnet tool restore
        if($LASTEXITCODE -ne 0){throw 'Pinned Velopack tool restore failed.'}
        $notes=Join-Path $output 'release-notes.md'
        [IO.File]::WriteAllText($notes,"Shovel Rust $version Beta. Native Rust workspace preview. Full C# parity is still in progress. Independent Rust-only updates. See TODO and roadmap for verification limits.")
        & dotnet vpk pack --packId ShovelRust --packVersion $version --packDir (Join-Path $payload 'app') --mainExe ShovelRust.exe --packTitle 'Shovel Rust' --packAuthors 'Shovel contributors' --channel $Channel --runtime win-x64 --delta None --icon (Join-Path $root 'src/GitGui.App/Assets/Shovel.ico') --outputDir $output --releaseNotes $notes
        if($LASTEXITCODE -ne 0){throw 'Rust installer packaging failed.'}
    }finally{Pop-Location}
    $setups=@(Get-ChildItem -LiteralPath $output -Filter '*Setup.exe' -File)
    if($setups.Count -ne 1){throw 'Expected one Rust installer.'}
    $setup=Join-Path $output $plan.Installer
    if($setups[0].FullName -ne $setup){Move-Item -LiteralPath $setups[0].FullName -Destination $setup}
    $feed=Join-Path $output $plan.Feed
    $metadata=Get-Content -LiteralPath $feed -Raw | ConvertFrom-Json
    $assets=@($metadata.Assets | Where-Object {$_.Type -eq 'Full' -and $_.Version -eq $version -and $_.PackageId -eq 'ShovelRust'})
    if($assets.Count -ne 1 -or $assets[0].FileName -ne $plan.Package){throw 'Unexpected Rust package identity.'}
    $signed=[ordered]@{SchemaVersion=1;Product='ShovelRust';Channel=$Channel;Architecture='win-x64';SourceCommit=$commit;Assets=$assets}
    $signed | ConvertTo-Json -Depth 12 | Set-Content -LiteralPath $feed -Encoding utf8NoBOM
    $signature=$key.SignData([IO.File]::ReadAllBytes($feed),[Security.Cryptography.HashAlgorithmName]::SHA256,[Security.Cryptography.RSASignaturePadding]::Pss)
    [IO.File]::WriteAllBytes("$feed.sig",$signature)
    & (Join-Path $payload 'verify_update_bundle.exe') $Channel $version $commit $feed "$feed.sig" (Join-Path $output $plan.Package)
    if($LASTEXITCODE -ne 0){throw 'Production Rust signature/package verification failed.'}
    [ordered]@{Version=$version;Product='ShovelRust';Channel=$Channel;Repository=$plan.Repository;Tag=$plan.Tag;FeedTag=$plan.FeedTag;SourceCommit=$commit;SourceFingerprint=$evidence.SourceFingerprint;Published=$false;Authenticode=$false;ParityVerified=$false} |
        ConvertTo-Json | Set-Content -LiteralPath (Join-Path $output 'release-info.json') -Encoding utf8NoBOM
    Get-ChildItem -LiteralPath $output -File | Sort-Object Name | ForEach-Object {
        (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant()+'  '+$_.Name
    } | Set-Content -LiteralPath (Join-Path $output 'SHA256SUMS.txt') -Encoding utf8NoBOM
    Write-Output $output
}finally{$key.Dispose();$public.Dispose()}
