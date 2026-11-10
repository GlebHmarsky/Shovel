[CmdletBinding()]
param()
$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
Push-Location $repo
$oldOutput = $env:SHOVEL_INIT_PARITY_OUTPUT
try {
    & git diff --quiet 2175458 -- src/GitGui.Core src/GitGui.ProcessHost
    if ($LASTEXITCODE -ne 0) { throw 'Pinned C# core/process reference changed.' }
    $run = Join-Path $repo ('Build/parity-init/' + [guid]::NewGuid().ToString('N'))
    $baseline = Join-Path $run 'csharp'
    $rust = Join-Path $run 'rust'
    New-Item -ItemType Directory -Path $run | Out-Null
    # Initialization rejects nested repositories: the native fixtures must be
    # outside this source checkout. Preserve failed/partial output for inspection.
    $fixtures = Join-Path ([IO.Path]::GetTempPath()) ('ShovelInitParity-' + [guid]::NewGuid().ToString('N'))
    & dotnet run --project tests/RustParityBaseline/RustParityBaseline.csproj --configuration Release -- $fixtures $baseline --init 2>&1 |
        Tee-Object -FilePath (Join-Path $run 'csharp.log')
    if ($LASTEXITCODE -ne 0) { throw 'Pinned C# initialization probe failed.' }
    . ./scripts/Use-RustToolchain.ps1
    $env:SHOVEL_INIT_PARITY_OUTPUT = $rust
    & cargo test -j 1 -p shovel-core --test initialization --locked -- --test-threads=1 2>&1 |
        Tee-Object -FilePath (Join-Path $run 'rust.log')
    if ($LASTEXITCODE -ne 0) { throw 'Rust initialization checks failed.' }
    $cases = @()
    foreach ($scenario in @('configured', 'existing', 'partial')) {
        $expected = Get-Content -Raw -LiteralPath (Join-Path $baseline "$scenario.json") | ConvertFrom-Json -AsHashtable
        $actual = Get-Content -Raw -LiteralPath (Join-Path $rust "$scenario.json") | ConvertFrom-Json -AsHashtable
        foreach ($field in @('message', 'repositoryObserved', 'headFile', 'indexExists', 'index', 'status', 'tip', 'config', 'templateMarker')) {
            if ($expected[$field] -cne $actual[$field]) { throw "Parity mismatch: $scenario/$field" }
        }
        $expectedNames = @($expected.worktree.Keys | Sort-Object)
        $actualNames = @($actual.worktree.Keys | Sort-Object)
        if (($expectedNames -join '|') -cne ($actualNames -join '|')) { throw "Parity mismatch: $scenario paths" }
        foreach ($name in $expectedNames) {
            if ($expected.worktree[$name] -cne $actual.worktree[$name]) { throw "Parity mismatch: $scenario/$name" }
        }
        $cases += [ordered]@{ Scenario = $scenario; Equal = $true; Baseline = $expected; Rust = $actual }
        Write-Output "PASS exact C#/Rust initialization outcome: $scenario"
    }
    $sources = [ordered]@{}
    foreach ($file in @('rust/core/src/initialization.rs', 'rust/core/src/working.rs',
        'rust/core/tests/initialization.rs', 'rust/desktop/src/repository_init.rs',
        'rust/desktop/src/repository_init_tests.rs', 'rust/desktop/src/main.rs',
        'tests/RustParityBaseline/Program.cs', 'scripts/Test-RustInitializationParity.ps1')) {
        $sources[$file] = (Get-FileHash -Algorithm SHA256 -LiteralPath $file).Hash
    }
    $report = [ordered]@{
        SchemaVersion = 1; DateUtc = [DateTime]::UtcNow.ToString('o')
        BaselineCommit = (& git rev-parse 2175458)
        RustBaseCommit = (& git rev-parse HEAD)
        GitVersion = (& git --version)
        SourceSha256 = $sources
        Scope = 'Three native initialization outcomes: configured default/template/populated directory; explicit missing destination and existing-repository no-op; failed native template with retained partial metadata. Headless Rust menu/form/scheduler and persistence checks are separate.'
        FixtureClock = 'Both drivers fix only GIT_AUTHOR_DATE and GIT_COMMITTER_DATE to 2026-01-01T00:00:00+00:00 for exact commit hashes.'
        IndexComparison = 'Exact staged path/mode/blob/stage entries and index absence before first staging. Raw index stat cache and timestamps are not compared across fixtures.'
        DesktopCapture = $false
        Cases = $cases
    }
    $report | ConvertTo-Json -Depth 12 | Set-Content -LiteralPath (Join-Path $run 'comparison.json') -Encoding utf8
    Write-Output "Evidence: $(Join-Path $run 'comparison.json')"
} finally {
    $env:SHOVEL_INIT_PARITY_OUTPUT = $oldOutput
    Pop-Location
}
