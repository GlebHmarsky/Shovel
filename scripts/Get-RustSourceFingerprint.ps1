# Package inputs, including native resources, signing policy and delivery tooling.
# Documentation is excluded; packaged release notes are hashed separately.
function Get-RustSourceFingerprint {
    param([Parameter(Mandatory)][string]$Root)
    $files = git -C $Root -c core.quotepath=false ls-files --cached --others --exclude-standard
    if ($LASTEXITCODE -ne 0) { throw 'Cannot enumerate Rust source inputs.' }
    $entries = foreach ($file in ($files | Sort-Object -Unique -CaseSensitive)) {
        if ($file -match '^(rust/|scripts/|distribution/|third-party/rust/)' -or
            $file -in @('Cargo.toml','Cargo.lock','rust-toolchain.toml','.config/dotnet-tools.json','LICENSE','third-party/VELOPACK-LICENSE.txt','src/GitGui.App/Assets/Shovel.ico','src/GitGui.App/Assets/Shovel.png','src/GitGui.App/Resources/Strings.resx')) {
            $path = Join-Path $Root $file
            $hash = if (Test-Path -LiteralPath $path -PathType Leaf) { (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash } else { 'DELETED' }
            "$file=$hash"
        }
    }
    [Convert]::ToHexString([Security.Cryptography.SHA256]::HashData([Text.Encoding]::UTF8.GetBytes(($entries -join "`n"))))
}

function Get-RustVersion {
    param([Parameter(Mandatory)][string]$Root)
    $manifest = [IO.File]::ReadAllText((Join-Path $Root 'Cargo.toml'))
    $section = [regex]::Match($manifest, '(?ms)^\[workspace\.package\]\s*\r?\n(.*?)(?=^\[|\z)').Groups[1].Value
    $match = [regex]::Match($section, '(?m)^version\s*=\s*"((?:0|[1-9]\d*)\.(?:0|[1-9]\d*)\.(?:0|[1-9]\d*))"\s*$')
    if (!$match.Success) { throw 'Cargo workspace version must have three canonical numeric components.' }
    $match.Groups[1].Value
}
