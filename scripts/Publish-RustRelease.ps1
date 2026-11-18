#requires -Version 7.4
[CmdletBinding()]
param([Parameter(Mandatory)][string]$ArtifactDirectory,[Parameter(Mandatory)][string]$PayloadDirectory,
    [ValidateSet('win-rust-beta','win-rust-stable')][string]$Channel='win-rust-beta',[string]$StableApproval)
$ErrorActionPreference='Stop'
$root=Split-Path -Parent $PSScriptRoot
. (Join-Path $PSScriptRoot 'Get-RustReleasePlan.ps1')
. (Join-Path $PSScriptRoot 'Get-RustSourceFingerprint.ps1')
. (Join-Path $PSScriptRoot 'Assert-RustReleaseArtifacts.ps1')
. (Join-Path $PSScriptRoot 'Get-RustReleaseTag.ps1')
$directory=(Resolve-Path -LiteralPath $ArtifactDirectory).Path
$commit=git -C $root rev-parse HEAD
$branch=git -C $root branch --show-current
$pending=git -C $root status --porcelain
if($LASTEXITCODE -ne 0 -or $pending){throw 'Commit intended Rust changes before publication.'}
$plan=Get-RustReleasePlan -Channel $Channel -Version (Get-RustVersion $root) -SourceCommit $commit -SourceBranch $branch -StableApproval $StableApproval
if($plan.Channel -eq 'win-rust-stable' -and @(Select-String -LiteralPath (Join-Path $root 'TODO.md') -Pattern '^- \[ \] \*\*R\d+').Count){throw 'Rust Stable requires complete parity acceptance as well as owner approval.'}
$origin=git -C $root remote get-url origin
if($origin -notin @('https://github.com/GlebHmarsky/Shovel.git','https://github.com/GlebHmarsky/Shovel','git@github.com:GlebHmarsky/Shovel.git')){throw 'Unexpected publishing repository.'}
$PayloadDirectory=(Resolve-Path -LiteralPath $PayloadDirectory).Path
$hashes=Get-VerifiedRustReleaseHashes $directory $PayloadDirectory $plan (Get-RustSourceFingerprint $root)
$assetNames=@(Get-RustPublicAssetNames $plan)
$verifier=Join-Path $PayloadDirectory 'verify_update_bundle.exe'
# Reuse the established GCM login without printing, persisting or forwarding it to download hosts.
$priorPrompt=$env:GIT_TERMINAL_PROMPT;$priorInteractive=$env:GCM_INTERACTIVE
$env:GIT_TERMINAL_PROMPT='0';$env:GCM_INTERACTIVE='Never'
$probe=Join-Path $root ('artifacts/rust-release-probe-'+[guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $probe | Out-Null
$api="https://api.github.com/repos/$($plan.Repository)"
function Get-OptionalRelease([string]$tag){
    try {Invoke-RestMethod "$api/releases/tags/$tag" -Headers $headers}
    catch {if([int]$_.Exception.Response.StatusCode -ne 404){throw}}
}
function Upload-Asset($release,[string]$name){
    $file=Get-Item -LiteralPath (Join-Path $directory $name)
    $uploaded=Invoke-RestMethod "https://uploads.github.com/repos/$($plan.Repository)/releases/$($release.id)/assets?name=$name" -Headers $headers -Method Post -ContentType 'application/octet-stream' -InFile $file.FullName
    $hash=(Get-FileHash $file.FullName).Hash.ToLowerInvariant()
    if($uploaded.state -ne 'uploaded' -or $uploaded.size -ne $file.Length -or ($uploaded.digest -and $uploaded.digest -ne "sha256:$hash")){throw "Incomplete or changed upload: $name"}
    Write-Host "Uploaded: $name"
}
function Get-CSharpState {
    $latest=Invoke-RestMethod "$api/releases/latest" -Headers $headers
    $beta=Get-OptionalRelease 'beta'
    $master=Invoke-RestMethod "$api/commits/master" -Headers $headers
    [ordered]@{Master=$master.sha;Latest=$latest.tag_name;LatestId=$latest.id;BetaId=$beta.id;BetaAssets=@($beta.assets | Sort-Object name | Select-Object id,name,size,digest,updated_at)}
}
try {
    if ($env:GH_TOKEN) {
        $token = $env:GH_TOKEN
    } else {
        $credential=("protocol=https"+[char]10+"host=github.com"+[char]10+[char]10 | git credential fill)
        if($LASTEXITCODE -ne 0){throw 'Configured GitHub login unavailable.'}
        $password=@($credential | Where-Object {$_.StartsWith('password=')})
        if($password.Count -ne 1){throw 'Configured GitHub login unavailable.'}
        $token=$password[0].Substring(9)
    }
    $headers=@{Authorization="Bearer $token";Accept='application/vnd.github+json';'X-GitHub-Api-Version'='2022-11-28'}
    $repository=Invoke-RestMethod $api -Headers $headers
    if($repository.private){throw 'Anonymous updates require the existing public repository; visibility is never changed.'}
    $before=Get-CSharpState
    $before | ConvertTo-Json -Depth 6 | Set-Content (Join-Path $probe 'csharp-before.json')
    if(Get-OptionalRelease $plan.Tag){throw 'Rust version already exists. Never overwrite immutable releases.'}
    $alias=Get-OptionalRelease $plan.FeedTag
    if($alias -and (!$alias.prerelease -or @($alias.assets | Where-Object {$_.name -notin @($plan.Feed,"$($plan.Feed).sig")}).Count)){throw 'Unexpected Rust feed identity.'}
    # Examine all pages so a previous newer Rust version cannot be hidden by C# releases.
    for($page=1;;$page++){
        $history=@(Invoke-RestMethod "$api/releases?per_page=100&page=$page" -Headers $headers | ForEach-Object { $_ })
        foreach($prior in $history){
            $versionMatch=[regex]::Match([string]$prior.tag_name,"^rust-v(\d+\.\d+\.\d+)-$([regex]::Escape($plan.Channel))$")
            if($versionMatch.Success -and [version]$plan.Version -le [version]$versionMatch.Groups[1].Value){throw 'Rust versions must increase.'}
        }
        if($history.Count -lt 100){break}
    }
    $tagRef="refs/tags/$($plan.Tag)"
    $refs=@("HEAD:refs/heads/$($plan.SourceBranch)","HEAD:refs/heads/$($plan.ReleaseBranch)")
    if (!(Get-RustReleaseTag $root origin $plan)) { $refs+="HEAD:$tagRef" }
    git -C $root push --atomic origin @refs
    if($LASTEXITCODE -ne 0){throw 'Rust branch push failed. No release published.'}
    $remote=Invoke-RestMethod "$api/commits/$($plan.ReleaseBranch)" -Headers $headers
    if($remote.sha -ne $commit){throw 'Rust release branch differs from packaged commit.'}
    if (!(Get-RustReleaseTag $root origin $plan)) { throw 'Version tag is missing after branch push.' }
    $body=@{tag_name=$plan.Tag;target_commitish=$commit;name="Shovel $($plan.Version) - $($plan.Channel)";body=(Get-Content (Join-Path $directory 'release-notes.md') -Raw);draft=$true;prerelease=$plan.GitHubPrerelease;make_latest='false'} | ConvertTo-Json
    $release=Invoke-RestMethod "$api/releases" -Headers $headers -Method Post -ContentType 'application/json' -Body $body
    foreach($name in $assetNames){Upload-Asset $release $name}
    $published=Invoke-RestMethod "$api/releases/$($release.id)" -Headers $headers -Method Patch -ContentType 'application/json' -Body (@{draft=$false;prerelease=$plan.GitHubPrerelease;make_latest='false'}|ConvertTo-Json)
    # Anonymous immutable downloads must succeed before advertising the feed.
    foreach($name in @($plan.Package,$plan.Installer)){
        Invoke-WebRequest -Uri "https://github.com/$($plan.Repository)/releases/download/$($plan.Tag)/$name" -OutFile (Join-Path $probe $name) -TimeoutSec 300
        if((Get-FileHash (Join-Path $probe $name)).Hash -ne $hashes[$name]){throw 'Anonymous Rust download differs from built artifact.'}
    }
    if(!$alias){
        $alias=Invoke-RestMethod "$api/releases" -Headers $headers -Method Post -ContentType 'application/json' -Body (@{tag_name=$plan.FeedTag;target_commitish=$commit;name="Shovel $($plan.FeedTag) update feed";body='Signed Rust-only update metadata. Partial Beta; see versioned release and TODO for parity limits.';draft=$true;prerelease=$true;make_latest='false'}|ConvertTo-Json)
    }
    foreach($name in @($plan.Feed,"$($plan.Feed).sig")){
        foreach($old in @($alias.assets | Where-Object name -eq $name)){Invoke-RestMethod "$api/releases/assets/$($old.id)" -Headers $headers -Method Delete | Out-Null}
        Upload-Asset $alias $name
    }
    Invoke-RestMethod "$api/releases/$($alias.id)" -Headers $headers -Method Patch -ContentType 'application/json' -Body (@{draft=$false;prerelease=$true;make_latest='false'}|ConvertTo-Json) | Out-Null
    foreach($name in @($plan.Feed,"$($plan.Feed).sig")){
        Invoke-WebRequest -Uri "https://github.com/$($plan.Repository)/releases/download/$($plan.FeedTag)/$name" -OutFile (Join-Path $probe $name) -TimeoutSec 60
    }
    & $verifier $plan.Channel $plan.Version $commit (Join-Path $probe $plan.Feed) (Join-Path $probe "$($plan.Feed).sig") (Join-Path $probe $plan.Package)
    if($LASTEXITCODE -ne 0){throw 'Anonymous signed Rust feed/package probe failed.'}
    $after=Get-CSharpState
    $after | ConvertTo-Json -Depth 6 | Set-Content (Join-Path $probe 'csharp-after.json')
    if(($before|ConvertTo-Json -Depth 6 -Compress) -ne ($after|ConvertTo-Json -Depth 6 -Compress)){throw 'C# source/latest/Beta changed during Rust publication; inspect concurrent state.'}
    [ordered]@{Published=$true;Version=$plan.Version;SourceCommit=$commit;Url=$published.html_url;AnonymousInstallerVerified=$true;AnonymousSignedPackageVerified=$true;CSharpUnchanged=$true;ParityVerified=$false} | ConvertTo-Json | Set-Content (Join-Path $probe 'PUBLICATION.json')
    Write-Output "Published $($published.html_url); anonymous signed Rust update verified. Evidence: $probe"
} finally {
    $headers=$null;$password=$null;$credential=$null;$token=$null
    $env:GIT_TERMINAL_PROMPT=$priorPrompt;$env:GCM_INTERACTIVE=$priorInteractive
}
