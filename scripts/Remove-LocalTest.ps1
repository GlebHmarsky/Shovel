#requires -Version 7.4
[CmdletBinding(SupportsShouldProcess)]
param([switch]$RemoveTestData)
$ErrorActionPreference='Stop'
$base=[IO.Path]::GetFullPath([Environment]::GetFolderPath('LocalApplicationData'))
$install=Join-Path $base 'ShovelLocalTest'
$data=Join-Path $base 'ShovelLocalTestData'
function Assert-SafeDirectory([string]$path) {
 if ([IO.Path]::GetFullPath($path) -notin @($install,$data)) { throw 'Unexpected cleanup target.' }
 if (!(Test-Path -LiteralPath $path)) { return }
 $items=@(Get-Item -LiteralPath $path)+@(Get-ChildItem -LiteralPath $path -Recurse -Force)
 if ($items | Where-Object { $_.Attributes -band [IO.FileAttributes]::ReparsePoint }) { throw "Refusing cleanup through a junction or symbolic link: $path" }
}
Assert-SafeDirectory $install
Assert-SafeDirectory $data
$running=Get-Process | Where-Object { $_.Path -and $_.Path.StartsWith($install+'\',[StringComparison]::OrdinalIgnoreCase) }
if ($running) { throw 'Close Shovel Local Test and wait for any update to finish, then run cleanup again.' }
$updater=Join-Path $install 'Update.exe'
if (Test-Path -LiteralPath $updater) {
 $manifest=Join-Path $install 'current/sq.version'
 if (!(Test-Path -LiteralPath $manifest) -or (Get-Content -LiteralPath $manifest -Raw) -notmatch '<id>ShovelLocalTest</id>') { throw 'Installer identity could not be verified. No files removed.' }
 if ($PSCmdlet.ShouldProcess($install,'Uninstall Shovel Local Test')) {
  $p=Start-Process -FilePath $updater -ArgumentList 'uninstall','--silent' -WindowStyle Hidden -Wait -PassThru
  if ($p.ExitCode -ne 0) { throw "Uninstaller returned $($p.ExitCode)." }
  for($i=0;$i -lt 30 -and (Test-Path -LiteralPath (Join-Path $install 'current'));$i++) { Start-Sleep -Seconds 1 }
 }
}
if (Test-Path -LiteralPath 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\ShovelLocalTest') {
 if (!$WhatIfPreference) { throw 'Windows uninstall registration remains. No forced removal was attempted.' }
}
foreach($path in @($install)+@($(if($RemoveTestData){$data}))) {
 if (!$path -or !(Test-Path -LiteralPath $path)) { continue }
 Assert-SafeDirectory $path
 if ($PSCmdlet.ShouldProcess($path,'Remove isolated test files')) { Remove-Item -LiteralPath $path -Recurse -Force }
}
if ($WhatIfPreference) { Write-Output 'Cleanup preview completed; no files were removed.' }
else { Write-Output 'Cleanup completed. Normal Shovel data and repositories were not targeted.' }
