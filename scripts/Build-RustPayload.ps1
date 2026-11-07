#requires -Version 7.4
# Native optimized payload and packaging evidence; not a smoke/parity certificate.
[CmdletBinding()]
param([string]$OutputDirectory)
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
. (Join-Path $PSScriptRoot 'Use-RustToolchain.ps1')
. (Join-Path $PSScriptRoot 'Get-RustSourceFingerprint.ps1')
. (Join-Path $PSScriptRoot 'Test-RustNativeExecutable.ps1')
$version = Get-RustVersion $root
$fingerprint = Get-RustSourceFingerprint $root
$commit = git -C $root rev-parse HEAD
if ($LASTEXITCODE -ne 0) { throw 'Cannot identify Rust source commit.' }
$pending = @(git -C $root status --porcelain)
if ($LASTEXITCODE -ne 0) { throw 'Cannot identify Rust source changes.' }
if (!$OutputDirectory) { $OutputDirectory = Join-Path $root ('artifacts/rust-payload-' + [Guid]::NewGuid().ToString('N')) }
$output = [IO.Path]::GetFullPath($OutputDirectory)
if (!$output.StartsWith($root.TrimEnd('\')+'\',[StringComparison]::OrdinalIgnoreCase)) { throw 'Rust payload must be built inside this worktree.' }
if (Test-Path -LiteralPath $output) { throw 'Native payload output already exists; do not overwrite verified builds.' }
$app = Join-Path $output 'app'
New-Item -ItemType Directory -Path $app -Force | Out-Null
$previousFlags = $env:CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS
Push-Location $root
try {
    # Explicit target keeps proc-macro/build host crates separate from static CRT target crates.
    $env:CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS = '-C target-feature=+crt-static'
    & cargo build --locked --release --target x86_64-pc-windows-msvc -j 1 -p shovel-desktop -p shovel-askpass -p shovel-core --bins --example verify_update_bundle
    if ($LASTEXITCODE -ne 0) { throw 'Native Rust payload compilation failed.' }
    $binaries = foreach ($executable in @('ShovelRust.exe','ShovelRustAskPass.exe')) {
        $path = Join-Path $root "target/x86_64-pc-windows-msvc/release/$executable"
        $verified = Test-RustNativeExecutable -Path $path -Version $version -Executable $executable
        Copy-Item -LiteralPath $path -Destination $app
        $verified
    }
    $probe = Join-Path $root 'target/x86_64-pc-windows-msvc/release/examples/verify_update_bundle.exe'
    Copy-Item -LiteralPath $probe -Destination $output
    $metadata = & cargo metadata --locked --format-version 1 --filter-platform x86_64-pc-windows-msvc
    if ($LASTEXITCODE -ne 0) { throw 'Locked Rust dependency metadata failed.' }
    $metadataPath = Join-Path $output 'cargo-metadata.json'
    $metadata | Set-Content -LiteralPath $metadataPath -Encoding utf8NoBOM
    $toolchain = & rustc --print sysroot
    if ($LASTEXITCODE -ne 0) { throw 'Pinned Rust toolchain metadata failed.' }
    $notices = & (Join-Path $PSScriptRoot 'Export-RustNotices.ps1') -MetadataPath $metadataPath -OutputDirectory $app -ToolchainDirectory $toolchain
    Copy-Item -LiteralPath (Join-Path $root 'LICENSE'),(Join-Path $root 'third-party/VELOPACK-LICENSE.txt') -Destination $app
    if ((Get-RustSourceFingerprint $root) -ne $fingerprint -or (git -C $root rev-parse HEAD) -ne $commit) { throw 'Source inputs changed during native payload compilation.' }
    $files = @(Get-ChildItem -LiteralPath $app -Recurse -File | Sort-Object FullName | ForEach-Object {
        [ordered]@{Name=[IO.Path]::GetRelativePath($app,$_.FullName).Replace('\','/');Size=$_.Length;SHA256=(Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant()}
    })
    $smokePath=Join-Path $root 'Build/rust-smoke.json'
    $smoke=if(Test-Path -LiteralPath $smokePath){Get-Content -LiteralPath $smokePath -Raw | ConvertFrom-Json}else{$null}
    $smokeVerified=$smoke -and $smoke.Passed -and $smoke.SourceFingerprint -eq $fingerprint
    [ordered]@{SchemaVersion=1;Product='ShovelRust';Version=$version;SourceCommit=$commit;SourceFingerprint=$fingerprint;Dirty=($pending.Count -gt 0);Target='x86_64-pc-windows-msvc';Profile='release';StaticCrt=$true;CompiledAt=[DateTimeOffset]::UtcNow.ToString('O');ParityVerified=$false;SmokeVerified=[bool]$smokeVerified;Binaries=@($binaries);Notices=$notices;Files=$files;VerifierSHA256=(Get-FileHash -LiteralPath (Join-Path $output 'verify_update_bundle.exe') -Algorithm SHA256).Hash.ToLowerInvariant()} | ConvertTo-Json -Depth 10 | Set-Content -LiteralPath (Join-Path $output 'VERIFICATION.json') -Encoding utf8NoBOM
    Write-Output $output
} finally {
    $env:CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS = $previousFlags
    Pop-Location
}
