# Pure release identity/policy; no Git, credentials, network or mutation here.
function Get-RustReleasePlan {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][ValidateSet('win-rust-beta','win-rust-stable')][string]$Channel,
        [Parameter(Mandatory)][string]$Version,
        [Parameter(Mandatory)][string]$SourceCommit,
        [Parameter(Mandatory)][string]$SourceBranch,
        [string]$StableApproval
    )
    if ($Version -notmatch '^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$') { throw 'Rust release version must be three canonical numeric components.' }
    if ($SourceCommit -notmatch '^([a-f0-9]{40}|[a-f0-9]{64})$') { throw 'An exact source commit is required.' }
    if ($SourceBranch -ne 'codex/rust-rewrite') { throw 'Rust release preparation must start on codex/rust-rewrite; C# master is never a Rust source branch.' }
    if ($Channel -eq 'win-rust-stable' -and [string]::IsNullOrWhiteSpace($StableApproval)) { throw 'Rust Stable requires explicit owner approval for this version; record that approval before publication.' }
    $suffix=if ($Channel -eq 'win-rust-beta') {'beta'} else {'stable'}
    [pscustomobject]@{
        Product='ShovelRust'; Repository='GlebHmarsky/Shovel'; Architecture='win-x64'
        Channel=$Channel; Version=$Version; SourceCommit=$SourceCommit; SourceBranch=$SourceBranch
        ReleaseBranch="codex/rust-$suffix"; FeedTag="rust-$suffix"
        Tag="rust-v$Version-$Channel"; Feed="releases.$Channel.json"
        Package="ShovelRust-$Version-$Channel-full.nupkg"; Installer='ShovelRust-Setup.exe'
        Executable='ShovelRust.exe'; GitHubPrerelease=$true; MakeLatest='false'
        StableApproval=$(if ($Channel -eq 'win-rust-stable') {$StableApproval} else {$null})
    }
}
