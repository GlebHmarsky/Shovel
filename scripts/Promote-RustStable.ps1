#requires -Version 7.4
# One-time owner-approved promotion. No app rebuild, version change or parity claim.
[CmdletBinding()]
param([Parameter(Mandatory)][string]$VerifierPath,
      [string]$PrivateKeyPath=(Join-Path $env:USERPROFILE '.shovel-signing/shovel-update.key'),
      [switch]$Publish)
$ErrorActionPreference='Stop'
$root=Split-Path -Parent $PSScriptRoot
if(git -C $root status --porcelain){throw 'Commit promotion tooling and approval first.'}
$approval=Get-Content "$root/distribution/rust-stable-0.50.3-approval.json" -Raw | ConvertFrom-Json
if($approval.Version -ne '0.50.3' -or $approval.SourceCommit -ne '9d75ec962d714646276f9c2ca25619767437fe5d' -or !$approval.OwnerApproval){throw 'This approval covers only Rust 0.50.3.'}
$version=$approval.Version;$source=$approval.SourceCommit
$channel='win-rust-stable';$tag="rust-v$version-$channel";$feed="releases.$channel.json";$package="ShovelRust-$version-$channel-full.nupkg"
$work=Join-Path $root 'Build/stable-promotion-0.50.3';$inputDirectory=Join-Path $work 'input';$output=Join-Path $work 'stable'
$base="https://github.com/GlebHmarsky/Shovel/releases/download/rust-v$version-win-rust-beta"
New-Item -ItemType Directory -Force $inputDirectory | Out-Null
foreach($name in @('releases.win-rust-beta.json','releases.win-rust-beta.json.sig',"ShovelRust-$version-win-rust-beta-full.nupkg")){
    $file=Join-Path $inputDirectory $name
    if(!(Test-Path -LiteralPath $file)){Invoke-WebRequest "$base/$name" -OutFile $file -TimeoutSec 120}
}
$betaPackage=Join-Path $inputDirectory "ShovelRust-$version-win-rust-beta-full.nupkg"
if((Get-FileHash $betaPackage).Hash -ne $approval.BetaPackageSHA256){throw 'Beta package differs from owner-approved release.'}
& $VerifierPath win-rust-beta $version $source "$inputDirectory/releases.win-rust-beta.json" "$inputDirectory/releases.win-rust-beta.json.sig" $betaPackage
if($LASTEXITCODE -ne 0){throw 'Production verifier rejected Beta input.'}
Add-Type -AssemblyName System.IO.Compression.FileSystem
function PayloadHashes([string]$path){
    $zip=[IO.Compression.ZipFile]::OpenRead($path);$hashes=@{}
    try {foreach($entry in $zip.Entries){
        if(!$entry.FullName.StartsWith('lib/app/') -or !$entry.Name){continue}
        $name=$entry.FullName.Substring(8)
        if($name -in @('sq.version','Squirrel.exe','Shovel Rust_ExecutionStub.exe')){continue}
        if($hashes.ContainsKey($name)){throw 'Duplicate package path.'}
        $stream=$entry.Open();try{$hashes[$name]=[Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($stream))}finally{$stream.Dispose()}
    }}finally{$zip.Dispose()}
    return $hashes
}
$expected=PayloadHashes $betaPackage
if(!(Test-Path -LiteralPath $output)){
    $app=Join-Path $work 'app';New-Item -ItemType Directory -Force $app | Out-Null
    $zip=[IO.Compression.ZipFile]::OpenRead($betaPackage)
    try {foreach($entry in $zip.Entries){
        if(!$entry.FullName.StartsWith('lib/app/')){continue}
        $name=$entry.FullName.Substring(8);if(!$expected.ContainsKey($name)){continue}
        $destination=[IO.Path]::GetFullPath((Join-Path $app $name))
        if(!$destination.StartsWith($app+'\',[StringComparison]::OrdinalIgnoreCase)){throw 'Unsafe archive path.'}
        New-Item -ItemType Directory -Force (Split-Path -Parent $destination) | Out-Null
        [IO.Compression.ZipFileExtensions]::ExtractToFile($entry,$destination,$true)
    }}finally{$zip.Dispose()}
    New-Item -ItemType Directory $output | Out-Null
    $notes='Shovel Rust 0.50.3 Stable. Owner-approved promotion of the existing Beta application on 2026-11-17; application files are unchanged. Full parity remains unfinished. Select Rust Stable in update settings; fresh 0.50.3 profiles still default to Rust Beta. No installation/update rehearsal or Authenticode signing is claimed.'
    [IO.File]::WriteAllText("$output/release-notes.md",$notes)
    Push-Location $root
    try {
        dotnet tool restore;if($LASTEXITCODE -ne 0){throw 'Velopack restore failed.'}
        dotnet vpk pack --packId ShovelRust --packVersion $version --packDir $app --mainExe ShovelRust.exe --packTitle 'Shovel Rust' --packAuthors 'Shovel contributors' --channel $channel --runtime win-x64 --delta None --icon "$root/rust/assets/Shovel.ico" --outputDir $output --releaseNotes "$output/release-notes.md"
        if($LASTEXITCODE -ne 0){throw 'Stable packaging failed.'}
    }finally{Pop-Location}
    $setups=@(Get-ChildItem $output -Filter '*Setup.exe');if($setups.Count -ne 1){throw 'Expected one installer.'}
    if($setups[0].Name -ne 'ShovelRust-Setup.exe'){Move-Item -LiteralPath $setups[0].FullName -Destination "$output/ShovelRust-Setup.exe"}
    $assets=@((Get-Content "$output/$feed" -Raw | ConvertFrom-Json).Assets | Where-Object {$_.Type -eq 'Full' -and $_.Version -eq $version -and $_.PackageId -eq 'ShovelRust'})
    if($assets.Count -ne 1 -or $assets[0].FileName -ne $package){throw 'Unexpected Stable package identity.'}
    [ordered]@{SchemaVersion=1;Product='ShovelRust';Channel=$channel;Architecture='win-x64';SourceCommit=$source;Assets=$assets} | ConvertTo-Json -Depth 12 | Set-Content "$output/$feed" -Encoding utf8NoBOM
    $key=[Security.Cryptography.RSA]::Create();$public=[Security.Cryptography.RSA]::Create()
    try {
        $key.ImportFromPem([IO.File]::ReadAllText($PrivateKeyPath));$public.ImportFromPem([IO.File]::ReadAllText("$root/distribution/update-public.pem"))
        if($key.ExportSubjectPublicKeyInfoPem() -ne $public.ExportSubjectPublicKeyInfoPem()){throw 'Existing signing key mismatch.'}
        [IO.File]::WriteAllBytes("$output/$feed.sig",$key.SignData([IO.File]::ReadAllBytes("$output/$feed"),[Security.Cryptography.HashAlgorithmName]::SHA256,[Security.Cryptography.RSASignaturePadding]::Pss))
    }finally{$key.Dispose();$public.Dispose()}
    [ordered]@{Version=$version;Product='ShovelRust';Channel=$channel;SourceCommit=$source;PromotionCommit=(git -C $root rev-parse HEAD);OwnerApproval=$approval.OwnerApproval;BetaPackageSHA256=$approval.BetaPackageSHA256;ParityVerified=$false;Authenticode=$false} | ConvertTo-Json | Set-Content "$output/release-info.json" -Encoding utf8NoBOM
}
$actual=PayloadHashes "$output/$package"
if($actual.Count -ne $expected.Count){throw 'Application payload count changed.'}
foreach($name in $expected.Keys){if($actual[$name] -ne $expected[$name]){throw "Application payload changed: $name"}}
& $VerifierPath $channel $version $source "$output/$feed" "$output/$feed.sig" "$output/$package"
if($LASTEXITCODE -ne 0){throw 'Production verifier rejected Stable package.'}
$names=@('ShovelRust-Setup.exe',$package,$feed,"$feed.sig",'release-info.json','release-notes.md')
$names | ForEach-Object {(Get-FileHash "$output/$_").Hash.ToLowerInvariant()+'  '+$_} | Set-Content "$output/SHA256SUMS.txt" -Encoding utf8NoBOM
Write-Host "Verified $($expected.Count) unchanged application files and signed Stable package."
if(!$Publish){Write-Output $output;return}
$env:GIT_TERMINAL_PROMPT='0';$env:GCM_INTERACTIVE='Never'
$credential="protocol=https`nhost=github.com`n`n" | git credential fill
if($LASTEXITCODE -ne 0){throw 'GitHub credential unavailable.'}
$token=($credential | Where-Object {$_.StartsWith('password=')}).Substring(9)
$headers=@{Authorization="Bearer $token";Accept='application/vnd.github+json';'X-GitHub-Api-Version'='2022-11-28'}
$api='https://api.github.com/repos/GlebHmarsky/Shovel'
function OptionalRelease($name){try{Invoke-RestMethod "$api/releases/tags/$name" -Headers $headers}catch{if([int]$_.Exception.Response.StatusCode -ne 404){throw}}}
function ProtectedState {
    $state=[ordered]@{}
    foreach($name in @('beta','rust-beta','rust-v0.50.3-win-rust-beta')){$r=OptionalRelease $name;$state[$name]=@{id=$r.id;assets=@($r.assets | Sort-Object name | Select-Object id,name,digest,size,updated_at)}}
    $latest=Invoke-RestMethod "$api/releases/latest" -Headers $headers;$state['latest']=@{id=$latest.id;tag=$latest.tag_name}
    $state | ConvertTo-Json -Depth 8 -Compress
}
function Upload($release,$name){Invoke-RestMethod "https://uploads.github.com/repos/GlebHmarsky/Shovel/releases/$($release.id)/assets?name=$name" -Headers $headers -Method Post -ContentType application/octet-stream -InFile "$output/$name" | Out-Null}
try {
    if((OptionalRelease $tag) -or (OptionalRelease 'rust-stable')){throw 'Stable release/feed exists; never overwrite this one-time promotion.'}
    $before=ProtectedState
    git -C $root push origin "${source}:refs/heads/codex/rust-stable"
    if($LASTEXITCODE -ne 0){throw 'Stable branch push failed.'}
    $release=Invoke-RestMethod "$api/releases" -Headers $headers -Method Post -ContentType application/json -Body (@{tag_name=$tag;target_commitish=$source;name='Shovel Rust 0.50.3 - Stable';body=(Get-Content "$output/release-notes.md" -Raw);draft=$true;prerelease=$false;make_latest='false'} | ConvertTo-Json)
    foreach($name in ($names+@('SHA256SUMS.txt'))){Upload $release $name}
    Invoke-RestMethod "$api/releases/$($release.id)" -Headers $headers -Method Patch -ContentType application/json -Body '{"draft":false,"prerelease":false,"make_latest":"false"}' | Out-Null
    $probe=Join-Path $work 'anonymous';New-Item -ItemType Directory $probe | Out-Null
    foreach($name in @('ShovelRust-Setup.exe',$package,$feed,"$feed.sig")){
        Invoke-WebRequest "https://github.com/GlebHmarsky/Shovel/releases/download/$tag/$name" -OutFile "$probe/$name" -TimeoutSec 120
        if((Get-FileHash "$probe/$name").Hash -ne (Get-FileHash "$output/$name").Hash){throw 'Anonymous download hash mismatch.'}
    }
    & $VerifierPath $channel $version $source "$probe/$feed" "$probe/$feed.sig" "$probe/$package"
    if($LASTEXITCODE -ne 0){throw 'Anonymous Stable package verification failed.'}
    $alias=Invoke-RestMethod "$api/releases" -Headers $headers -Method Post -ContentType application/json -Body (@{tag_name='rust-stable';target_commitish=$source;name='Shovel Rust Stable update feed';body='Signed Rust Stable feed. Owner-approved 0.50.3 promotion; see versioned release for limits.';draft=$true;prerelease=$true;make_latest='false'} | ConvertTo-Json)
    foreach($name in @($feed,"$feed.sig")){Upload $alias $name}
    Invoke-RestMethod "$api/releases/$($alias.id)" -Headers $headers -Method Patch -ContentType application/json -Body '{"draft":false,"prerelease":true,"make_latest":"false"}' | Out-Null
    foreach($name in @($feed,"$feed.sig")){Invoke-WebRequest "https://github.com/GlebHmarsky/Shovel/releases/download/rust-stable/$name" -OutFile "$probe/$name" -TimeoutSec 120}
    & $VerifierPath $channel $version $source "$probe/$feed" "$probe/$feed.sig" "$probe/$package"
    if($LASTEXITCODE -ne 0){throw 'Stable feed alias verification failed.'}
    if((ProtectedState) -ne $before){throw 'Protected Beta/C# release state changed.'}
    [ordered]@{Published=$true;Version=$version;SourceCommit=$source;ApplicationFilesUnchanged=$expected.Count;AnonymousInstallerVerified=$true;SignedStableFeedVerified=$true;ProtectedReleaseStateUnchanged=$true;ParityVerified=$false;Installed=$false} | ConvertTo-Json | Set-Content "$work/PUBLICATION.json" -Encoding utf8NoBOM
    Write-Output "Published https://github.com/GlebHmarsky/Shovel/releases/tag/$tag"
}finally{$headers=$null;$token=$null;$credential=$null}
