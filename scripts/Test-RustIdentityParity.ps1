[CmdletBinding()]
param()
$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
Push-Location $repo
$oldEvidence = $env:SHOVEL_IDENTITY_PARITY
$oldProbe = $env:SHOVEL_IDENTITY_BASELINE_DLL
try {
    & git diff --quiet 2175458 -- src/GitGui.Core src/GitGui.ProcessHost
    if ($LASTEXITCODE -ne 0) { throw 'Pinned C# reference changed.' }
    $run = Join-Path $repo ('Build/parity-identity/' + [guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Path $run | Out-Null
    $fixtures = Join-Path ([IO.Path]::GetTempPath()) ('ShovelIdentityParity-' + [guid]::NewGuid().ToString('N'))
    & dotnet run --project tests/RustParityBaseline/RustParityBaseline.csproj --configuration Release -- $fixtures $run --identity 2>&1 |
        Tee-Object -FilePath (Join-Path $run 'csharp.log')
    if ($LASTEXITCODE -ne 0) { throw 'C# identity probe failed.' }
    . ./scripts/Use-RustToolchain.ps1
    $env:SHOVEL_IDENTITY_PARITY = $run
    $env:SHOVEL_IDENTITY_BASELINE_DLL = Join-Path $repo 'tests/RustParityBaseline/bin/Release/net10.0-windows/RustParityBaseline.dll'
    & cargo test -j 1 -p shovel-core --test identity --locked native_identity_and_gate_match_pinned_csharp -- --nocapture 2>&1 |
        Tee-Object -FilePath (Join-Path $run 'rust.log')
    if ($LASTEXITCODE -ne 0) { throw 'Rust identity/gate parity failed.' }
    Write-Output "Evidence: $run"
} finally {
    $env:SHOVEL_IDENTITY_PARITY = $oldEvidence
    $env:SHOVEL_IDENTITY_BASELINE_DLL = $oldProbe
    Pop-Location
}
