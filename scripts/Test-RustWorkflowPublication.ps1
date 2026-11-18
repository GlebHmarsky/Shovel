# Drive the real workflow selection with local Git and in-memory public metadata.
param([string]$Fixture, $Plan)
$ErrorActionPreference = 'Stop'
$repository = 'GlebHmarsky/Shovel'
$commit = $Plan.SourceCommit
$draftBody = "Version notes." + [Environment]::NewLine + "<!-- Shovel Windows artifact 123 -->"
$artifact = @{id=123; expired=$false; name="Shovel-Windows-$commit-900-1"; workflow_run=@{id=900;head_sha=$commit}}
$run = @{id=900; head_sha=$commit; head_branch='master'; event='push'; path='.github/workflows/rust-windows.yml';
    repository=@{full_name=$repository}; head_repository=@{full_name=$repository}}
$listed = @()
function gh {
    if ($args -contains '--paginate') { return $listed }
    switch -Exact ($args[1]) {
        "repos/$repository/releases/71" { return (@{draft=$true;tag_name=$Plan.Tag;target_commitish=$commit;body=$draftBody} | ConvertTo-Json) }
        "repos/$repository/actions/artifacts/123" { return ($artifact | ConvertTo-Json) }
        "repos/$repository/actions/runs/900" { return ($run | ConvertTo-Json) }
        default { throw 'Unexpected GitHub fixture request.' }
    }
}
$priorSha = $env:GITHUB_SHA
$priorOutput = $env:GITHUB_OUTPUT
Push-Location -LiteralPath $Fixture
try {
    $env:GITHUB_SHA = $commit
    $env:GITHUB_OUTPUT = Join-Path $Fixture 'workflow-output'
    function Invoke-Selection {
        [IO.File]::WriteAllText($env:GITHUB_OUTPUT, '')
        & (Join-Path $PSScriptRoot 'Get-RustWorkflowPublication.ps1') -Version $Plan.Version
        @(Get-Content -LiteralPath $env:GITHUB_OUTPUT) -join ','
    }
    if ((Invoke-Selection) -cne 'publish=true') { throw 'New version was skipped.' }
    $listed = @("$($Plan.Tag)`tfalse`t71")
    if ((Invoke-Selection) -cne 'publish=true') { throw 'Public original-source retry was skipped.' }
    $env:GITHUB_SHA = 'c'*40
    if ((Invoke-Selection) -cne 'publish=false') { throw 'Ordinary later build attempted to republish.' }
    $listed = @("$($Plan.Tag)`ttrue`t71")
    Assert-Refusal { Invoke-Selection } '*original tagged source*'
    $env:GITHUB_SHA = $commit
    if ((Invoke-Selection) -cne 'archive=123,run=900,publish=true') { throw 'Original draft archive was not selected.' }
    $run.event='pull_request'
    Assert-Refusal { Invoke-Selection } '*exact source*'
    $run.event='push'
    $run.head_repository.full_name='foreign/repository'
    Assert-Refusal { Invoke-Selection } '*exact source*'
    $run.head_repository.full_name=$repository
    $artifact.expired=$true
    Assert-Refusal { Invoke-Selection } '*exact source*'
    $artifact.expired=$false
    $run.head_sha='d'*40
    Assert-Refusal { Invoke-Selection } '*exact source*'
} finally {
    Pop-Location
    $env:GITHUB_SHA = $priorSha
    $env:GITHUB_OUTPUT = $priorOutput
}
