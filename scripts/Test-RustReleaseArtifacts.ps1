# Offline packaging regressions. Only disposable paths and local Git refs are used.
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'Assert-RustReleaseArtifacts.ps1')
. (Join-Path $PSScriptRoot 'Get-RustReleaseTag.ps1')
. (Join-Path $PSScriptRoot 'Get-RustReleasePlan.ps1')
function Assert-Refusal([scriptblock]$Action, [string]$Expected) {
    try { & $Action } catch {
        if ($_.Exception.Message -like $Expected) { return }
        throw
    }
    throw "Expected release refusal: $Expected"
}
$fixture = [IO.Directory]::CreateTempSubdirectory('shovel-release-test-').FullName
$priorGlobal = $env:GIT_CONFIG_GLOBAL
$priorNoSystem = $env:GIT_CONFIG_NOSYSTEM
try {
    $env:GIT_CONFIG_GLOBAL = Join-Path $fixture 'isolated.gitconfig'
    $env:GIT_CONFIG_NOSYSTEM = '1'
    git -C $fixture init --quiet --initial-branch=master
    if ($LASTEXITCODE) { throw 'Cannot initialize release fixture.' }
    git -C $fixture -c user.name=Fixture -c user.email=local@example.invalid commit -q --allow-empty -m fixture
    if ($LASTEXITCODE) { throw 'Cannot create release fixture commit.' }
    $commit = git -C $fixture rev-parse HEAD
    $plan = Get-RustReleasePlan -Channel win-rust-beta -Version '0.1.0' -SourceCommit $commit -SourceBranch master
    if (Get-RustReleaseTag $fixture . $plan) { throw 'Unexpected fixture tag.' }
    git -C $fixture tag $plan.Tag HEAD
    if ((Get-RustReleaseTag $fixture . $plan) -ne $commit) { throw 'Missing lightweight tag.' }
    git -C $fixture -c user.name=Fixture -c user.email=local@example.invalid tag -fa $plan.Tag -m fixture HEAD
    if ((Get-RustReleaseTag $fixture . $plan) -ne $commit) { throw 'Missing annotated tag.' }
    $other = Get-RustReleasePlan -Channel win-rust-beta -Version '0.1.0' -SourceCommit ('a'*40) -SourceBranch master
    Assert-Refusal { Get-RustReleaseTag $fixture . $other } '*different source commit*'

    $app = [IO.Directory]::CreateDirectory((Join-Path $fixture 'app')).FullName
    $verifier = Join-Path $fixture 'verify_update_bundle.exe'
    [IO.File]::WriteAllText($verifier,'fixture only')
    $files = foreach ($name in @('Shovel.exe','ShovelAskPass.exe')) {
        $path = Join-Path $app $name
        [IO.File]::WriteAllText($path,'fixture')
        @{Name=$name;Size=(Get-Item -LiteralPath $path).Length;SHA256=(Get-FileHash -LiteralPath $path).Hash}
    }
    $fingerprint = 'B'*64
    $receipt = @{Product=$plan.Product;Version=$plan.Version;SourceCommit=$commit;SourceFingerprint=$fingerprint;
        Dirty=$false;SmokeVerified=$true;Files=$files;VerifierSHA256=(Get-FileHash -LiteralPath $verifier).Hash}
    $receipt | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath (Join-Path $fixture 'VERIFICATION.json')
    Assert-RustPayload $fixture $plan $fingerprint
    Assert-Refusal { Assert-RustPayload $fixture $other $fingerprint } '*matching source commit*'
    Assert-Refusal { Assert-RustPayload $fixture $plan ('C'*64) } '*matching source commit*'
    $foreign = Get-RustReleasePlan -Channel win-rust-stable -Version $plan.Version -SourceCommit $commit -SourceBranch master -StableApproval fixture
    $foreign | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $fixture 'release-info.json')
    Assert-Refusal { Get-VerifiedRustReleaseHashes $fixture $fixture $plan $fingerprint } '*release plan: Channel'
    [IO.File]::AppendAllText($verifier,'changed')
    Assert-Refusal { Assert-RustPayload $fixture $plan $fingerprint } 'Package verifier changed.'
    [IO.File]::WriteAllText((Join-Path $app 'Shovel.exe'),'changed')
    Assert-Refusal { Assert-RustPayload $fixture $plan $fingerprint } '*differs from receipt: Shovel.exe'
    [IO.File]::WriteAllText((Join-Path $app 'other.dll'),'unlisted')
    Assert-Refusal { Assert-RustPayload $fixture $plan $fingerprint } '*recorded inventory*'

    $installer = [IO.Directory]::CreateDirectory((Join-Path $fixture 'installer')).FullName
    $checksums = Join-Path $installer 'SHA256SUMS.txt'
    $names = @(Get-RustPublicAssetNames $plan | Where-Object { $_ -cne 'SHA256SUMS.txt' }) +
        @("assets.$($plan.Channel).json", "RELEASES-$($plan.Channel)", "$($plan.Product)-$($plan.Channel)-Portable.zip")
    $lines = foreach ($name in $names) {
        $path = Join-Path $installer $name
        [IO.File]::WriteAllText($path, "fixture $name")
        (Get-FileHash -LiteralPath $path).Hash.ToLowerInvariant() + '  ' + $name
    }
    $lines | Set-Content -LiteralPath $checksums
    $verified = Get-RustInstallerHashes $installer $plan
    if ($verified.Count -ne 9) { throw 'Expected all installer and pinned Velopack outputs.' }
    $lines[0..($lines.Count - 2)] | Set-Content -LiteralPath $checksums
    Assert-Refusal { Get-RustInstallerHashes $installer $plan } 'Missing verified artifact:*'
    @($lines + $lines[0]) | Set-Content -LiteralPath $checksums
    Assert-Refusal { Get-RustInstallerHashes $installer $plan } 'Unexpected release checksum entry.'
    $lines | Set-Content -LiteralPath $checksums
    $unexpected = Join-Path $installer 'unrelated.txt'
    [IO.File]::WriteAllText($unexpected, 'unrelated')
    Assert-Refusal { Get-RustInstallerHashes $installer $plan } 'Unexpected file or directory*'
    Remove-Item -LiteralPath $unexpected
    $unexpected = [IO.Directory]::CreateDirectory((Join-Path $installer 'nested')).FullName
    Assert-Refusal { Get-RustInstallerHashes $installer $plan } 'Unexpected file or directory*'
    [IO.Directory]::Delete($unexpected)
    [IO.File]::AppendAllText((Join-Path $installer $plan.Installer), 'changed')
    Assert-Refusal { Get-RustInstallerHashes $installer $plan } 'Changed release artifact:*'
    $history = @(
        '## Rust 0.1.10 - 2026-11-18', '', '- A different version.', '',
        '## Rust 0.1.0 Stable promotion - 2026-11-18', '', 'Promotion only.', '',
        '## Rust 0.1.0 - 2026-11-17', '', '- First change.', '- Second change.', '',
        '## Rust 0.0.9 - 2026-11-15', '', '- Older.'
    ) -join [Environment]::NewLine
    $notes = Get-RustChangelogNotes $history '0.1.0'
    if ($notes -cne ('- First change.' + [Environment]::NewLine + '- Second change.')) { throw 'Incorrect versioned release notes.' }
    Assert-Refusal { Get-RustChangelogNotes $history '0.2.0' } 'Expected one nonempty*'
    Assert-Refusal { Get-RustChangelogNotes ($history + [Environment]::NewLine + $history) '0.1.0' } 'Expected one nonempty*'
    Assert-Refusal { Get-RustChangelogNotes ("## Rust 0.1.0 - 2026-11-18" + [Environment]::NewLine) '0.1.0' } 'Expected one nonempty*'
    Write-Output 'PASS release provenance, exact installer inventory, versioned changelog and immutable tag fixtures.'
} finally {
    $env:GIT_CONFIG_GLOBAL = $priorGlobal
    $env:GIT_CONFIG_NOSYSTEM = $priorNoSystem
    # Only the directory created for this invocation, directly under the OS temp root.
    $directory = Get-Item -LiteralPath $fixture
    if ($directory.Parent.FullName -eq [IO.Path]::TrimEndingDirectorySeparator([IO.Path]::GetTempPath()) -and
        $directory.Name.StartsWith('shovel-release-test-')) { Remove-Item -LiteralPath $directory.FullName -Recurse -Force }
}
