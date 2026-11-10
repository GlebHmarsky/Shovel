[CmdletBinding()]
param()
$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
Push-Location $repo
$oldOutput = $env:SHOVEL_QUEUE_PARITY_OUTPUT
try {
    # Compare the actual reference sources, not a mutable version label.
    & git diff --quiet 2175458 -- src/GitGui.Core src/GitGui.ProcessHost
    if ($LASTEXITCODE -ne 0) { throw 'Pinned C# core/process reference changed.' }
    $run = Join-Path $repo ('Build/parity-queue/' + [guid]::NewGuid().ToString('N'))
    $baseline = Join-Path $run 'csharp'
    $rust = Join-Path $run 'rust'
    New-Item -ItemType Directory -Path $run | Out-Null
    & dotnet run --project tests/RustParityBaseline/RustParityBaseline.csproj --configuration Release -- (Join-Path $run 'fixtures') $baseline 2>&1 |
        Tee-Object -FilePath (Join-Path $run 'csharp.log')
    if ($LASTEXITCODE -ne 0) { throw 'Pinned C# outcome probe failed.' }
    . ./scripts/Use-RustToolchain.ps1
    $env:SHOVEL_QUEUE_PARITY_OUTPUT = $rust
    & cargo test -j 1 -p shovel-desktop commit::queue_tests --locked -- --test-threads=1 2>&1 |
        Tee-Object -FilePath (Join-Path $run 'rust.log')
    if ($LASTEXITCODE -ne 0) { throw 'Rust queue checks failed.' }
    $cases = @()
    foreach ($scenario in @('stage-commit', 'removed-commit', 'external-index', 'unstage')) {
        $expected = Get-Content -Raw -LiteralPath (Join-Path $baseline "$scenario.json") | ConvertFrom-Json -AsHashtable
        $actual = Get-Content -Raw -LiteralPath (Join-Path $rust "$scenario.json") | ConvertFrom-Json -AsHashtable
        foreach ($field in @('head', 'index', 'status', 'config')) {
            if ($expected[$field] -cne $actual[$field]) { throw "Parity mismatch: $scenario/$field" }
        }
        $expectedNames = @($expected.worktree.Keys | Sort-Object)
        $actualNames = @($actual.worktree.Keys | Sort-Object)
        if (($expectedNames -join '|') -cne ($actualNames -join '|')) { throw "Parity mismatch: $scenario paths" }
        foreach ($name in $expectedNames) {
            if ($expected.worktree[$name] -cne $actual.worktree[$name]) { throw "Parity mismatch: $scenario/$name" }
        }
        $cases += [ordered]@{ Scenario = $scenario; Equal = $true; Baseline = $expected; Rust = $actual }
        Write-Output "PASS exact C#/Rust native outcome: $scenario"
    }
    $sources = [ordered]@{}
    foreach ($file in @('rust/desktop/src/commit.rs', 'rust/desktop/src/commit_queue.rs',
        'rust/desktop/src/commit_queue_tests.rs', 'rust/desktop/src/operations.rs',
        'rust/desktop/src/main.rs', 'rust/core/src/working.rs', 'rust/core/src/scheduler.rs',
        'tests/RustParityBaseline/Program.cs', 'scripts/Test-RustWorkingQueueParity.ps1')) {
        $sources[$file] = (Get-FileHash -Algorithm SHA256 -LiteralPath $file).Hash
    }
    $report = [ordered]@{
        SchemaVersion = 1; DateUtc = [DateTime]::UtcNow.ToString('o')
        BaselineCommit = (& git rev-parse 2175458)
        RustBaseCommit = (& git rev-parse HEAD)
        GitVersion = (& git --version)
        SourceSha256 = $sources
        Scope = 'Four native working-copy outcomes. Rust uses actual editor input, scheduler and Activity removal; C# probe uses pinned native actions sequentially. Pinned C# UI queue behavior is checked separately by --check-operation-lifetimes.'
        FixtureClock = 'Both drivers fix only GIT_AUTHOR_DATE and GIT_COMMITTER_DATE to 2026-01-01T00:00:00+00:00 for exact commit hashes.'
        IndexComparison = 'Exact staged path/mode/blob/stage entries. Raw index stat cache and filesystem timestamps are intentionally not compared across separate fixtures.'
        DesktopCapture = $false
        Cases = $cases
    }
    $report | ConvertTo-Json -Depth 12 | Set-Content -LiteralPath (Join-Path $run 'comparison.json') -Encoding utf8
    Write-Output "Evidence: $(Join-Path $run 'comparison.json')"
} finally {
    $env:SHOVEL_QUEUE_PARITY_OUTPUT = $oldOutput
    Pop-Location
}
