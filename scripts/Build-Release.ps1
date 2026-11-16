#requires -Version 7.4
[CmdletBinding()]
param([ValidateSet('win-rust-beta','win-rust-stable')][string]$Channel='win-rust-beta',
    [string]$PrivateKeyPath=(Join-Path $env:USERPROFILE '.shovel-signing/shovel-update.key'),
    [switch]$PackageOnly,[string]$StableApproval)
& (Join-Path $PSScriptRoot 'Build-RustRelease.ps1') @PSBoundParameters
