# Generates test-only signatures; no private key is persisted or used for releases.
$ErrorActionPreference = 'Stop'
$directory = Join-Path $PSScriptRoot '../rust/core/tests/fixtures'
New-Item -ItemType Directory -Path $directory -Force | Out-Null
$key = [Security.Cryptography.RSA]::Create(3072)
try {
    [IO.File]::WriteAllText((Join-Path $directory 'update-public.pem'), $key.ExportSubjectPublicKeyInfoPem())
    $package = [Text.Encoding]::UTF8.GetBytes("Rust package fixture`n")
    foreach ($case in @('beta','csharp','traversal','wrong-architecture')) {
        $feed = [ordered]@{
            SchemaVersion=1; Product='ShovelRust'; Channel='win-rust-beta'; Architecture='win-x64'
            SourceCommit=('a' * 40)
            Assets=@([ordered]@{
                PackageId='ShovelRust'; Version='0.48.0'; Type='Full'
                FileName='ShovelRust-0.48.0-win-rust-beta-full.nupkg'
                SHA256=[Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($package))
                Size=$package.Length; NotesMarkdown='Test-only update fixture'; NotesHtml=''
            })
        }
        if ($case -eq 'csharp') { $feed.Product='Shovel'; $feed.Assets[0].PackageId='Shovel' }
        if ($case -eq 'traversal') { $feed.Assets[0].FileName='../ShovelRust-0.48.0-win-rust-beta-full.nupkg' }
        if ($case -eq 'wrong-architecture') { $feed.Architecture='win-arm64' }
        $bytes=[Text.Encoding]::UTF8.GetBytes(($feed | ConvertTo-Json -Depth 8))
        [IO.File]::WriteAllBytes((Join-Path $directory "$case.json"), $bytes)
        [IO.File]::WriteAllBytes((Join-Path $directory "$case.json.sig"), $key.SignData($bytes, [Security.Cryptography.HashAlgorithmName]::SHA256, [Security.Cryptography.RSASignaturePadding]::Pss))
    }
} finally { $key.Dispose() }
