param(
    [ValidateSet('merge-heavy', 'large', 'million')]
    [string]$Profile = 'merge-heavy',
    [ValidateRange(1, 20)]
    [int]$Samples = 3,
    [ValidateRange(1, 5000)]
    [int]$CancelAfterMs = 50,
    [switch]$IdleFiveMinutes
)
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
$entryPoint = Join-Path $projectRoot 'tests/GitGui.FoundationChecks/Program.cs'
if ((Get-Content -LiteralPath $entryPoint -Raw) -notmatch 'HistoryPerformanceHarness\.Run\(args\)') {
    throw 'Integrate the opt-in dispatch from docs/handoffs/task-3.md and build first. This script never restores or builds.'
}
$executable = Join-Path $projectRoot 'tests/GitGui.FoundationChecks/bin/Release/net10.0-windows/GitGui.FoundationChecks.exe'
if (-not (Test-Path -LiteralPath $executable -PathType Leaf)) {
    throw 'The Release foundation executable is missing. Integration must restore/build separately before measuring.'
}
$sourceFiles = @(Get-Item -LiteralPath $entryPoint) + @(Get-ChildItem -LiteralPath (Split-Path -Parent $entryPoint) -Filter 'HistoryPerformance*.cs')
if (($sourceFiles | Sort-Object LastWriteTimeUtc -Descending | Select-Object -First 1).LastWriteTimeUtc -gt (Get-Item -LiteralPath $executable).LastWriteTimeUtc) {
    throw 'The foundation executable predates the harness/dispatch source. Build the integrated sources separately before measuring.'
}
$benchmarkArguments = @('--measure-history-performance', $Profile.ToLowerInvariant(), ('--samples=' + $Samples), ('--cancel-after-ms=' + $CancelAfterMs))
if ($IdleFiveMinutes) { $benchmarkArguments += '--idle-five-minutes' }
Push-Location -LiteralPath $projectRoot
try {
    & $executable @benchmarkArguments
    if ($LASTEXITCODE -ne 0) { throw ('History measurement exited with code ' + $LASTEXITCODE + '. Inspect artifacts/history-performance for partial evidence.') }
}
finally { Pop-Location }
