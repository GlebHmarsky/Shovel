[CmdletBinding()]
param()
$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
Push-Location $repo
$oldOutput = $env:SHOVEL_DIVERGENCE_PARITY_OUTPUT
try {
    & git diff --quiet 2175458 -- src/GitGui.Core src/GitGui.ProcessHost
    if ($LASTEXITCODE -ne 0) { throw 'Pinned C# core/process reference changed.' }
    $run = Join-Path $repo ('Build/parity-divergence/' + [guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Path $run | Out-Null
    $fixtures = Join-Path ([IO.Path]::GetTempPath()) ('ShovelDivergenceParity-' + [guid]::NewGuid().ToString('N'))
    & dotnet run --project tests/RustParityBaseline/RustParityBaseline.csproj --configuration Release -- $fixtures $run --divergence 2>&1 |
        Tee-Object -FilePath (Join-Path $run 'csharp.log')
    if ($LASTEXITCODE -ne 0) { throw 'Pinned C# divergence probe failed.' }
    . ./scripts/Use-RustToolchain.ps1
    $env:SHOVEL_DIVERGENCE_PARITY_OUTPUT = Join-Path $run 'rust.json'
    & cargo test -j 1 -p shovel-core --test divergence --locked -- --test-threads=1 2>&1 |
        Tee-Object -FilePath (Join-Path $run 'rust.log')
    if ($LASTEXITCODE -ne 0) { throw 'Rust divergence check failed.' }
    $expected = @(Get-Content -Raw -LiteralPath (Join-Path $run 'divergence.json') | ConvertFrom-Json -AsHashtable)
    $actual = @(Get-Content -Raw -LiteralPath (Join-Path $run 'rust.json') | ConvertFrom-Json -AsHashtable)
    if ($actual.Count -ne $expected.Count) { throw 'Different case count.' }
    for ($i = 0; $i -lt $expected.Count; $i++) {
        if ((($expected[$i].Keys | Sort-Object) -join '|') -cne (($actual[$i].Keys | Sort-Object) -join '|')) { throw 'Different fields.' }
        foreach ($key in $expected[$i].Keys) {
            if ($expected[$i][$key] -cne $actual[$i][$key]) { throw "Parity mismatch: $($expected[$i].case)/$key" }
        }
        Write-Output "PASS exact C#/Rust divergence outcome: $($expected[$i].case)"
    }
    $sources = [ordered]@{}
    foreach ($file in @('rust/core/src/divergence.rs', 'rust/core/tests/divergence.rs',
        'rust/desktop/src/divergence.rs', 'rust/desktop/src/main.rs', 'rust/desktop/src/operations.rs',
        'rust/desktop/src/workspace_style.rs', 'tests/RustParityBaseline/Program.cs', 'scripts/Test-RustDivergenceParity.ps1')) {
        $sources[$file] = (Get-FileHash -Algorithm SHA256 -LiteralPath $file).Hash
    }
    $report = [ordered]@{
        SchemaVersion = 1; DateUtc = [DateTime]::UtcNow.ToString('o')
        BaselineCommit = (& git rev-parse 2175458); RustBaseCommit = (& git rev-parse HEAD)
        GitVersion = (& git --version); SourceSha256 = $sources
        Scope = 'Local immutable Pull-upstream ancestry and unavailable states. Both native readers preserve exact HEAD/config/status and index absence. Separate Push destination is configured and never used for these counts.'
        FixtureClock = 'Both drivers fix author/committer dates to 2026-01-01T00:00:00+00:00.'
        DesktopCapture = $false; Equal = $true; Baseline = $expected; Rust = $actual
    }
    $report | ConvertTo-Json -Depth 12 | Set-Content -LiteralPath (Join-Path $run 'comparison.json') -Encoding utf8
    Write-Output "Evidence: $(Join-Path $run 'comparison.json')"
} finally {
    $env:SHOVEL_DIVERGENCE_PARITY_OUTPUT = $oldOutput
    Pop-Location
}
