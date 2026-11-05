# Dot-source this file. Prefer the isolated toolchain; otherwise use installed Cargo.
$rustRoot = Split-Path -Parent $PSScriptRoot
$localTools = Join-Path $rustRoot 'artifacts/rust-tools'
if (Test-Path -LiteralPath (Join-Path $localTools 'cargo/bin/cargo.exe')) {
    $env:CARGO_HOME = Join-Path $localTools 'cargo'
    $env:RUSTUP_HOME = Join-Path $localTools 'rustup'
    $env:PATH = (Join-Path $env:CARGO_HOME 'bin') + [IO.Path]::PathSeparator + $env:PATH
}
if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) { throw 'Rust 1.99.1 MSVC toolchain is required. See rust-toolchain.toml.' }
