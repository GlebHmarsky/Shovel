#requires -Version 7.4
param([Parameter(Mandatory)][string]$ArtifactDirectory)
$ErrorActionPreference='Stop'
& (Join-Path $PSScriptRoot 'Publish-StableRelease.ps1') -ArtifactDirectory $ArtifactDirectory