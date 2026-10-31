# Shared by preview verification and installer packaging. Excludes docs/build outputs.
function Get-SourceFingerprint {
    param([string]$Root)
    $tracked = git -C $Root ls-files --cached --others --exclude-standard
    if ($LASTEXITCODE -ne 0) { throw 'Cannot enumerate source files.' }
    $entries = foreach ($file in ($tracked | Sort-Object -Unique)) {
        if ($file -match '\.(cs|xaml|resx|csproj|props|targets|slnx|json)$') {
            $path = Join-Path $Root $file
            $hash = if (Test-Path -LiteralPath $path -PathType Leaf) { (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash } else { 'DELETED' }
            "$file=$hash"
        }
    }
    $bytes = [Text.Encoding]::UTF8.GetBytes(($entries -join "`n"))
    $sha = [Security.Cryptography.SHA256]::Create()
    try { [BitConverter]::ToString($sha.ComputeHash($bytes)).Replace('-', '') } finally { $sha.Dispose() }
}
