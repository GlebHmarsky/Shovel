#requires -Version 7.4
# Select the source for this Actions run. Only an unpublished draft may need
# its original installer archive. The verifier always comes from the fresh build.
param([Parameter(Mandatory)][string]$Version)
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'Assert-RustPublicationAssets.ps1')
$repository = 'GlebHmarsky/Shovel'
$tag = "rust-v$Version-win-rust-beta"
$releases = gh api --paginate "repos/$repository/releases" --jq '.[] | [.tag_name, .draft, .id] | @tsv'
if ($LASTEXITCODE) { throw 'Cannot inspect published Rust versions.' }
$existing = @($releases | Where-Object { ($_ -split "`t", 3)[0] -ceq $tag })
if ($existing.Count -gt 1) { throw 'Conflicting versioned release identities.' }
if ($existing.Count) {
    $fields = $existing[0] -split "`t", 3
    $commit = git rev-parse --verify "refs/tags/$tag^{commit}"
    if ($LASTEXITCODE) { throw 'The versioned release is missing its immutable source tag.' }
    if ($commit -cne $env:GITHUB_SHA) {
        if ($fields[1] -cne 'false') { throw 'Finish the existing draft from its original tagged source.' }
        Write-Host "Immutable version $tag shipped from $commit. This master build remains available as an artifact."
        'publish=false' >> $env:GITHUB_OUTPUT
        return
    }
    if ($fields[1] -ceq 'true') {
        $draft = gh api "repos/$repository/releases/$($fields[2])" | ConvertFrom-Json
        if ($LASTEXITCODE -or !$draft.draft -or $draft.tag_name -cne $tag -or $draft.target_commitish -cne $commit) { throw 'Draft identity changed.' }
        $id = Get-RustDraftArtifactId $draft.body
        if ($id) {
            $artifact = gh api "repos/$repository/actions/artifacts/$id" | ConvertFrom-Json
            if ($LASTEXITCODE) { throw 'Cannot retrieve the original Windows build artifact.' }
            $run = gh api "repos/$repository/actions/runs/$($artifact.workflow_run.id)" | ConvertFrom-Json
            if ($LASTEXITCODE) { throw 'Cannot verify the original Windows build run.' }
            Assert-RustRecoveryRun $artifact $run $commit $id $repository
            "archive=$id" >> $env:GITHUB_OUTPUT
            "run=$($run.id)" >> $env:GITHUB_OUTPUT
        }
    }
}
'publish=true' >> $env:GITHUB_OUTPUT
