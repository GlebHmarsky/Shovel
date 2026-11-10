[CmdletBinding()]
param()
$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
Push-Location $repo
$oldOutput = $env:SHOVEL_HISTORY_PARITY_OUTPUT
try {
    & git diff --quiet 2175458 -- src/GitGui.Core src/GitGui.ProcessHost
    if ($LASTEXITCODE -ne 0) { throw 'Pinned C# core/process reference changed.' }
    $run = Join-Path $repo ('Build/parity-history/' + [guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Path $run | Out-Null
    $fixtures = Join-Path ([IO.Path]::GetTempPath()) ('ShovelHistoryParity-' + [guid]::NewGuid().ToString('N'))
    & dotnet run --project tests/RustParityBaseline/RustParityBaseline.csproj --configuration Release -- $fixtures $run --history 2>&1 |
        Tee-Object -FilePath (Join-Path $run 'csharp.log')
    if ($LASTEXITCODE -ne 0) { throw 'Pinned C# history probe failed.' }
    . ./scripts/Use-RustToolchain.ps1
    $env:SHOVEL_HISTORY_PARITY_OUTPUT = Join-Path $run 'rust.json'
    & cargo test -j 1 -p shovel-desktop --locked native_page_results_match_pinned_baseline -- --test-threads=1 2>&1 |
        Tee-Object -FilePath (Join-Path $run 'rust.log')
    if ($LASTEXITCODE -ne 0) { throw 'Rust history check failed.' }
    $expected = @(Get-Content -Raw -LiteralPath (Join-Path $run 'history.json') | ConvertFrom-Json -AsHashtable)
    $actual = @(Get-Content -Raw -LiteralPath (Join-Path $run 'rust.json') | ConvertFrom-Json -AsHashtable)
    if ($actual.Count -ne $expected.Count) { throw 'Different case count.' }
    for ($i = 0; $i -lt $expected.Count; $i++) {
        if ((($expected[$i].Keys | Sort-Object) -join '|') -cne (($actual[$i].Keys | Sort-Object) -join '|')) { throw 'Different fields.' }
        foreach ($key in $expected[$i].Keys) {
            if ($expected[$i][$key] -cne $actual[$i][$key]) { throw "Parity mismatch: $($expected[$i].case)/$key" }
        }
        Write-Output "PASS exact C#/Rust history outcome: $($expected[$i].case)"
    }
    $sources = [ordered]@{}
    foreach ($file in @('rust/core/src/git.rs', 'rust/desktop/src/history_view.rs', 'rust/desktop/src/history_view_tests.rs',
        'rust/desktop/src/main.rs', 'tests/RustParityBaseline/Program.cs', 'scripts/Test-RustHistoryParity.ps1')) {
        $sources[$file] = (Get-FileHash -Algorithm SHA256 -LiteralPath $file).Hash
    }
    $report = [ordered]@{
        SchemaVersion = 1; DateUtc = [DateTime]::UtcNow.ToString('o')
        BaselineCommit = (& git rev-parse 2175458); RustBaseCommit = (& git rev-parse HEAD)
        GitVersion = (& git --version); SourceSha256 = $sources
        Scope = 'Six complete history-page results from native initial/older/unchanged refresh/end/newer/changed refresh reads. Both readers reject changed boundaries and preserve exact config/HEAD/status and index absence. Separate Rust interaction tests cover selection, scroll, focused row allocation and identity invalidation.'
        FixtureClock = 'Identical 610-commit fast-import streams with literal per-commit dates; changed-ref case moves to an existing immutable object.'
        DesktopCapture = $false; Equal = $true; Baseline = $expected; Rust = $actual
    }
    $report | ConvertTo-Json -Depth 12 | Set-Content -LiteralPath (Join-Path $run 'comparison.json') -Encoding utf8
    Write-Output "Evidence: $(Join-Path $run 'comparison.json')"
} finally {
    $env:SHOVEL_HISTORY_PARITY_OUTPUT = $oldOutput
    Pop-Location
}
