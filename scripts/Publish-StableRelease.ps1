#requires -Version 7.4
param([Parameter(Mandatory)][string]$ArtifactDirectory)
$ErrorActionPreference='Stop'
$directory=(Resolve-Path -LiteralPath $ArtifactDirectory).Path
$info=Get-Content (Join-Path $directory 'release-info.json') -Raw | ConvertFrom-Json
if($info.LocalOnly -or $info.Channel -notin @('win-stable','win-beta') -or $info.Repository -ne 'GlebHmarsky/Shovel' -or $info.Version -notmatch '^\d+\.\d+\.\d+$') { throw 'Not a configured Shovel stable release.' }
if($info.SourceCommit -notmatch '^[a-f0-9]{40}$'){throw 'Release source commit is missing or invalid.'}
$hashes=@{}
foreach($line in Get-Content (Join-Path $directory 'SHA256SUMS.txt')) {
 if($line -notmatch '^([a-f0-9]{64})  ([A-Za-z0-9_.-]+)$'){throw 'Invalid checksum entry.'}
 $hash=$Matches[1]; $name=$Matches[2]
 if($hashes.ContainsKey($name) -or $name -match '\.(key|pem|pfx|p12)$'){throw 'Unexpected release asset.'}
 if((Get-FileHash -LiteralPath (Join-Path $directory $name)).Hash -ne $hash){throw "Changed release artifact: $name"}
 $hashes[$name]=$hash
}
$files=@(Get-ChildItem -LiteralPath $directory -File)
if($files | Where-Object {$_.Name -ne 'SHA256SUMS.txt' -and !$hashes.ContainsKey($_.Name)}){throw 'Unverified files in release directory.'}
$feedPath=Join-Path $directory "releases.$($info.Channel).json"
$key=[Security.Cryptography.RSA]::Create()
try {
 $key.ImportFromPem([IO.File]::ReadAllText((Join-Path $PSScriptRoot '../distribution/update-public.pem')))
 if(!$key.VerifyData([IO.File]::ReadAllBytes($feedPath),[IO.File]::ReadAllBytes("$feedPath.sig"),[Security.Cryptography.HashAlgorithmName]::SHA256,[Security.Cryptography.RSASignaturePadding]::Pss)){throw 'Invalid release signature.'}
}finally{$key.Dispose()}
$feed=Get-Content $feedPath -Raw | ConvertFrom-Json
$package="Shovel-$($info.Version)-$($info.Channel)-full.nupkg"
if($feed.Assets.Count -ne 1 -or $feed.Assets[0].PackageId -ne 'Shovel' -or $feed.Assets[0].Version -ne $info.Version -or $feed.Assets[0].Type -ne 'Full' -or $feed.Assets[0].FileName -ne $package -or $feed.Assets[0].SHA256 -ne $hashes[$package] -or $feed.Assets[0].Size -ne (Get-Item (Join-Path $directory $package)).Length){throw 'Signed feed does not match the release package.'}
foreach($required in @($package,'Shovel-Setup.exe','release-notes.md','release-info.json',"releases.$($info.Channel).json","releases.$($info.Channel).json.sig")) { if(!$hashes.ContainsKey($required)){throw "Missing required asset: $required"} }
# Reuse the publisher's existing Git Credential Manager login. Never echo or persist it.
$env:GIT_TERMINAL_PROMPT='0'; $env:GCM_INTERACTIVE='Never'
$credential=("protocol=https`nhost=github.com`n`n" | git credential fill)
if($LASTEXITCODE -ne 0){throw 'GitHub login unavailable. Sign in with Git Credential Manager first.'}
$password=@($credential | Where-Object {$_.StartsWith('password=')})
if($password.Count -ne 1){throw 'GitHub login unavailable.'}
$token=$password[0].Substring(9)
$headers=@{Authorization="Bearer $token";Accept='application/vnd.github+json';'X-GitHub-Api-Version'='2022-11-28'}
$api="https://api.github.com/repos/$($info.Repository)"
try {
 $repo=Invoke-RestMethod $api -Headers $headers
 if($repo.private){throw 'Stable update downloads require a public releases repository. This script never changes visibility.'}
 $source=Invoke-RestMethod "$api/commits/master" -Headers $headers
 if($source.sha -ne $info.SourceCommit){throw 'Published master does not match this installer source commit. Reconcile source before publishing.'}
 $latest=$null
 try {$latest=Invoke-RestMethod "$api/releases/latest" -Headers $headers}
 catch {if([int]$_.Exception.Response.StatusCode -ne 404){throw}}
 if($latest -and $info.Channel -eq 'win-stable') {
  if($latest.tag_name -notmatch '^v(\d+\.\d+\.\d+)-win-stable$'){throw 'Latest release has an unexpected tag; inspect it before publishing.'}
  if([version]$info.Version -le [version]$Matches[1]){throw 'Increment the version: stable releases are immutable and must advance the latest version.'}
 }
 $tag="v$($info.Version)-$($info.Channel)"
 if($info.Channel -eq 'win-beta') {
  $history=Invoke-RestMethod "$api/releases?per_page=100" -Headers $headers
  foreach($prior in $history) {
   if($prior.tag_name -match '^v(\d+\.\d+\.\d+)-win-beta$' -and [version]$info.Version -le [version]$Matches[1]) {throw 'Beta versions must increase; never replace a published build.'}
  }
 }
 $existing=$null
 try {$existing=Invoke-RestMethod "$api/releases/tags/$tag" -Headers $headers}
 catch {if([int]$_.Exception.Response.StatusCode -ne 404){throw}}
 if($existing){throw "Release $tag already exists. Inspect any incomplete draft; never overwrite a published version."}
 $body=@{tag_name=$tag;target_commitish=$info.SourceCommit;name="Shovel $($info.Version) — $($info.Channel.Substring(4))";body=(Get-Content (Join-Path $directory 'release-notes.md') -Raw);draft=$true;prerelease=($info.Channel -eq 'win-beta')}|ConvertTo-Json
 $release=Invoke-RestMethod "$api/releases" -Headers $headers -Method Post -ContentType 'application/json' -Body $body
 Write-Output "Uploading draft $tag (ID $($release.id)). Failures leave it unpublished for inspection."
 foreach($file in $files) {
  $url="https://uploads.github.com/repos/$($info.Repository)/releases/$($release.id)/assets?name=$([Uri]::EscapeDataString($file.Name))"
  $uploaded=Invoke-RestMethod $url -Headers $headers -Method Post -ContentType 'application/octet-stream' -InFile $file.FullName -TimeoutSec 600
  if($uploaded.state -ne 'uploaded' -or $uploaded.size -ne $file.Length){throw 'Incomplete release upload.'}
  if($uploaded.digest -and $uploaded.digest -ne ('sha256:'+(Get-FileHash $file.FullName).Hash.ToLowerInvariant())){throw 'GitHub upload digest mismatch.'}
  Write-Output "Uploaded: $($file.Name)"
 }
 $published=Invoke-RestMethod "$api/releases/$($release.id)" -Headers $headers -Method Patch -ContentType 'application/json' -Body (@{draft=$false;make_latest=$(if($info.Channel -eq "win-stable"){ "true" }else{ "false" })}|ConvertTo-Json)
 if($info.Channel -eq 'win-beta') {
  $alias=$null
  try {$alias=Invoke-RestMethod "$api/releases/tags/beta" -Headers $headers} catch {if([int]$_.Exception.Response.StatusCode -ne 404){throw}}
  if(!$alias){$alias=Invoke-RestMethod "$api/releases" -Headers $headers -Method Post -ContentType 'application/json' -Body (@{tag_name='beta';target_commitish=$info.SourceCommit;name='Shovel Beta update feed';body='Signed Beta channel feed. Installers are in the versioned prereleases.';prerelease=$true;make_latest='false'}|ConvertTo-Json)}
  foreach($name in @('releases.win-beta.json','releases.win-beta.json.sig')) {
   foreach($old in @($alias.assets | Where-Object name -eq $name)) {Invoke-RestMethod "$api/releases/assets/$($old.id)" -Headers $headers -Method Delete | Out-Null}
   Invoke-RestMethod "https://uploads.github.com/repos/$($info.Repository)/releases/$($alias.id)/assets?name=$name" -Headers $headers -Method Post -ContentType 'application/octet-stream' -InFile (Join-Path $directory $name) | Out-Null
  }
  $stableAfter=Invoke-RestMethod "$api/releases/latest" -Headers $headers
  if($latest -and $stableAfter.tag_name -ne $latest.tag_name){throw 'Stable latest changed during Beta publication; inspect release state.'}
 }
 Write-Output "Published: $($published.html_url)"
}finally{$token=$null;$password=$null;$credential=$null;$headers=$null}
