[CmdletBinding()]
param()
$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
Push-Location $repo
$oldOutput = $env:SHOVEL_CLONE_PARITY_OUTPUT
$oldSource = $env:SHOVEL_CLONE_PARITY_SOURCE
try {
    & git diff --quiet 2175458 -- src/GitGui.Core src/GitGui.ProcessHost
    if ($LASTEXITCODE -ne 0) { throw 'Pinned C# core/process reference changed.' }
    $run = Join-Path $repo ('Build/parity-clone/' + [guid]::NewGuid().ToString('N'))
    $baseline = Join-Path $run 'csharp'
    $rust = Join-Path $run 'rust'
    New-Item -ItemType Directory -Path $run | Out-Null
    # Use one shared, read-only source repository so both native clones write
    # the identical origin URL into config; preserve partial output for inspection.
    $fixtures = Join-Path ([IO.Path]::GetTempPath()) ('ShovelCloneParity-' + [guid]::NewGuid().ToString('N'))
    & dotnet run --project tests/RustParityBaseline/RustParityBaseline.csproj --configuration Release -- $fixtures $baseline --clone 2>&1 |
        Tee-Object -FilePath (Join-Path $run 'csharp.log')
    if ($LASTEXITCODE -ne 0) { throw 'Pinned C# clone probe failed.' }
    . ./scripts/Use-RustToolchain.ps1
    $env:SHOVEL_CLONE_PARITY_OUTPUT = $rust
    $env:SHOVEL_CLONE_PARITY_SOURCE = (Get-Content -Raw -LiteralPath (Join-Path $baseline "source-path.txt")).Trim()
    & cargo test -j 1 -p shovel-core --test cloning --locked -- --test-threads=1 2>&1 |
        Tee-Object -FilePath (Join-Path $run 'rust.log')
    if ($LASTEXITCODE -ne 0) { throw 'Rust clone checks failed.' }
    $cases = @()
    foreach ($scenario in @('default', 'branch', 'tag', 'hook-failure')) {
        $expected = Get-Content -Raw -LiteralPath (Join-Path $baseline "$scenario.json") | ConvertFrom-Json -AsHashtable
        $actual = Get-Content -Raw -LiteralPath (Join-Path $rust "$scenario.json") | ConvertFrom-Json -AsHashtable
        foreach ($field in @('message', 'headFile', 'head', 'index', 'status', 'config')) {
            if ($expected[$field] -cne $actual[$field]) { throw "Parity mismatch: $scenario/$field" }
        }
        $expectedNames = @($expected.worktree.Keys | Sort-Object)
        $actualNames = @($actual.worktree.Keys | Sort-Object)
        if (($expectedNames -join '|') -cne ($actualNames -join '|')) { throw "Parity mismatch: $scenario paths" }
        foreach ($name in $expectedNames) {
            if ($expected.worktree[$name] -cne $actual.worktree[$name]) { throw "Parity mismatch: $scenario/$name" }
        }
        $cases += [ordered]@{ Scenario = $scenario; Equal = $true; Baseline = $expected; Rust = $actual }
        Write-Output "PASS exact C#/Rust clone outcome: $scenario"
    }
    $sources = [ordered]@{}
    foreach ($file in @('rust/core/src/cloning.rs', 'rust/core/src/initialization.rs',
        'rust/core/tests/cloning.rs', 'rust/desktop/src/repository_init.rs',
        'rust/desktop/src/repository_clone_tests.rs', 'rust/desktop/src/main.rs',
        'tests/RustParityBaseline/Program.cs', 'scripts/Test-RustCloneParity.ps1')) {
        $sources[$file] = (Get-FileHash -Algorithm SHA256 -LiteralPath $file).Hash
    }
    $report = [ordered]@{
        SchemaVersion = 1; DateUtc = [DateTime]::UtcNow.ToString('o')
        BaselineCommit = (& git rev-parse 2175458)
        RustBaseCommit = (& git rev-parse HEAD)
        GitVersion = (& git --version)
        SourceSha256 = $sources
        Scope = 'Four native clone outcomes from one shared disposable source: default branch, explicit branch, detached tag and failed checkout hook with retained repository. Headless Rust menu, lifetime, authentication and automatic opening checks are separate.'
        FixtureClock = 'Both drivers fix only GIT_AUTHOR_DATE and GIT_COMMITTER_DATE to 2026-01-01T00:00:00+00:00 for exact commit hashes.'
        IndexComparison = 'Exact staged path/mode/blob/stage entries. Raw index stat cache and timestamps are not compared across fixtures.'
        DesktopCapture = $false
        Cases = $cases
    }
    $report | ConvertTo-Json -Depth 12 | Set-Content -LiteralPath (Join-Path $run 'comparison.json') -Encoding utf8
    Write-Output "Evidence: $(Join-Path $run 'comparison.json')"
} finally {
    $env:SHOVEL_CLONE_PARITY_OUTPUT = $oldOutput
    $env:SHOVEL_CLONE_PARITY_SOURCE = $oldSource
    Pop-Location
}
