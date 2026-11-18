$ErrorActionPreference='Stop'
. (Join-Path $PSScriptRoot 'Get-RustReleasePlan.ps1')
$arguments=@{Version='0.48.0';SourceCommit=('a'*40);SourceBranch='master'}
$beta=Get-RustReleasePlan @arguments -Channel win-rust-beta
if ($beta.FeedTag -ne 'rust-beta' -or $beta.ReleaseBranch -ne 'codex/rust-beta' -or $beta.Product -ne 'ShovelRust' -or !$beta.GitHubPrerelease -or $beta.MakeLatest -ne 'false') {throw 'Rust Beta identity isolation failed'}
$stable=Get-RustReleasePlan @arguments -Channel win-rust-stable -StableApproval 'TEST ONLY: no publication is performed'
if ($stable.FeedTag -ne 'rust-stable' -or $stable.ReleaseBranch -ne 'codex/rust-stable' -or $stable.GitHubPrerelease -or $stable.MakeLatest -ne 'false') {throw 'Rust Stable must not replace C# latest'}
foreach ($case in @('stable-without-approval','csharp-channel','foreign-source','invalid-version','invalid-commit')) {
    $inputArguments=$arguments.Clone(); $inputArguments.Channel='win-rust-beta'
    switch ($case) {
        'stable-without-approval' {$inputArguments.Channel='win-rust-stable'}
        'csharp-channel' {$inputArguments.Channel='win-beta'}
        'foreign-source' {$inputArguments.SourceBranch='codex/csharp-archive-0.47.0'}
        'invalid-version' {$inputArguments.Version='0.48.0/../other'}
        'invalid-commit' {$inputArguments.SourceCommit='master'}
    }
    $rejected=$false
    try {Get-RustReleasePlan @inputArguments | Out-Null} catch {$rejected=$true}
    if (!$rejected) {throw "Release policy admitted $case"}
}
$config=Get-Content (Join-Path $PSScriptRoot '../distribution/rust-channels.json') -Raw | ConvertFrom-Json
if ($config.Product -ne $beta.Product -or $config.Executable -cne 'Shovel.exe' -or $config.Executable -cne $beta.Executable -or $beta.Installer -cne 'Shovel-Setup.exe') {throw 'Windows installer naming or package identity differs from release policy'}
foreach ($plan in @($beta,$stable)) {
    $entry=@($config.Channels | Where-Object Channel -eq $plan.Channel)
    if ($entry.Count -ne 1 -or $entry[0].Branch -ne $plan.ReleaseBranch -or $entry[0].FeedTag -ne $plan.FeedTag -or $entry[0].Feed -ne $plan.Feed -or $entry[0].GitHubPrerelease -ne $plan.GitHubPrerelease -or $entry[0].MakeLatest) {throw 'Channel manifest differs from release policy'}
}
Write-Output 'PASS Rust release identity, Beta/Stable branch policy, Stable approval requirement and C# latest isolation (8 checks). No publication.'
& (Join-Path $PSScriptRoot 'Test-RustReleaseArtifacts.ps1')
