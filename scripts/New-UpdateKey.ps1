#requires -Version 7.4
param([Parameter(Mandatory)][string]$PrivateKeyPath, [Parameter(Mandatory)][string]$PublicKeyPath)
$ErrorActionPreference = 'Stop'
foreach ($path in @($PrivateKeyPath, $PublicKeyPath)) {
    if (-not [IO.Path]::IsPathFullyQualified($path) -or (Test-Path -LiteralPath $path)) { throw 'Use new absolute paths; existing keys are never overwritten.' }
    if (-not (Test-Path -LiteralPath (Split-Path -Parent $path) -PathType Container)) { throw 'Create the destination directory first.' }
}
$rsa = [Security.Cryptography.RSA]::Create(3072)
try {
    # Private file is created with inheritance disabled and access for the current Windows user only.
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent().User
    $acl = [Security.AccessControl.FileSecurity]::new()
    $acl.SetAccessRuleProtection($true, $false)
    $acl.AddAccessRule([Security.AccessControl.FileSystemAccessRule]::new($identity, 'FullControl', 'Allow'))
    $stream = [IO.FileSystemAclExtensions]::Create([IO.FileInfo]::new($PrivateKeyPath), [IO.FileMode]::CreateNew,
        [Security.AccessControl.FileSystemRights]::FullControl, [IO.FileShare]::None, 4096, [IO.FileOptions]::None, $acl)
    try { $bytes = [Text.Encoding]::UTF8.GetBytes($rsa.ExportPkcs8PrivateKeyPem()); $stream.Write($bytes); [Array]::Clear($bytes) }
    finally { $stream.Dispose() }
    [IO.File]::WriteAllText($PublicKeyPath, $rsa.ExportSubjectPublicKeyInfoPem())
} finally { $rsa.Dispose() }
Write-Output 'Update signing key created. Back up the private key securely; distribute only the public key.'
