#requires -Version 7.4
$ErrorActionPreference='Stop'
$root=Split-Path -Parent $PSScriptRoot
$kit=Join-Path $root 'Build/Shovel-Local-Test-0.30.101-to-0.30.102'
if (Test-Path -LiteralPath $kit) { throw 'Test kit already exists; never overwrite it.' }
$work=Join-Path $root ('artifacts/local-test-'+[Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Force $work,$kit,(Join-Path $kit 'feed') | Out-Null
& (Join-Path $PSScriptRoot 'New-UpdateKey.ps1') -PrivateKeyPath (Join-Path $work 'fixture.key') -PublicKeyPath (Join-Path $work 'fixture.pub.pem')
& python (Join-Path $PSScriptRoot 'Prepare-LocalTest.py') $work $kit (Join-Path $work 'fixture.pub.pem')
if ($LASTEXITCODE -ne 0) { throw 'Test copy preparation failed.' }
$rsa=[Security.Cryptography.RSA]::Create()
try {
 $rsa.ImportFromPem([IO.File]::ReadAllText((Join-Path $work 'fixture.key')))
 foreach($version in @('0.30.101','0.30.102')) {
  $source=Join-Path $work 'source'
  $props=Join-Path $source 'Directory.Build.props'
  $xml=[xml](Get-Content $props -Raw); $xml.Project.PropertyGroup.Version=$version; $xml.Save($props)
  $publish=Join-Path $work "publish-$version"
  & dotnet publish (Join-Path $source 'src/GitGui.App') -c Release -r win-x64 --self-contained true -o $publish -p:RuntimeFrameworkVersion=10.0.12 -p:UseSharedCompilation=false -m:1 -nodeReuse:false
  if ($LASTEXITCODE -ne 0) { throw 'Test app publish failed.' }
  & dotnet publish (Join-Path $source 'src/GitGui.AskPass') -c Release -r win-x64 --self-contained true -o $publish -p:RuntimeFrameworkVersion=10.0.12 -p:UseSharedCompilation=false -m:1 -nodeReuse:false
  if ($LASTEXITCODE -ne 0) { throw 'Test helper publish failed.' }
  Copy-Item (Join-Path $root 'LICENSE'),(Join-Path $root 'third-party/VELOPACK-LICENSE.txt') $publish
  $release=Join-Path $work "release-$version"
  Push-Location $root
  try { & dotnet vpk pack --packId ShovelLocalTest --packVersion $version --packDir $publish --mainExe Shovel.exe --packTitle 'Shovel Local Test' --channel win-test --runtime win-x64 --delta None --shortcuts StartMenuRoot --icon (Join-Path $root 'src/GitGui.App/Assets/Shovel.ico') --outputDir $release } finally { Pop-Location }
  if ($LASTEXITCODE -ne 0) { throw 'Test installer packaging failed.' }
  $setup=Get-ChildItem $release -Filter '*Setup.exe' | Select-Object -First 1
  Copy-Item $setup.FullName (Join-Path $kit "Shovel-Local-Test-$version-Setup.exe")
  Copy-Item (Join-Path $release "ShovelLocalTest-$version-win-test-full.nupkg") (Join-Path $kit 'feed')
  if ($version -eq '0.30.102') {
   $feed=Join-Path $kit 'feed/releases.win-test.json'
   Copy-Item (Join-Path $release 'releases.win-test.json') $feed
   [IO.File]::WriteAllBytes("$feed.sig", $rsa.SignData([IO.File]::ReadAllBytes($feed),[Security.Cryptography.HashAlgorithmName]::SHA256,[Security.Cryptography.RSASignaturePadding]::Pss))
  }
 }
} finally { $rsa.Dispose() }
Copy-Item (Join-Path $PSScriptRoot 'Remove-LocalTest.ps1') $kit
Copy-Item (Join-Path $root 'docs/local-installer-test.md') (Join-Path $kit 'README.md')
Get-ChildItem $kit -File -Recurse | Sort-Object FullName | ForEach-Object {
 (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant()+'  '+[IO.Path]::GetRelativePath($kit,$_.FullName)
} | Set-Content (Join-Path $kit 'SHA256SUMS.txt')
Write-Output "Offline kit ready: $kit"
