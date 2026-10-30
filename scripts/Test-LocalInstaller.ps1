#requires -Version 7.4
$ErrorActionPreference='Stop'
$root=Split-Path -Parent $PSScriptRoot
$kit=Join-Path $root 'Build/Shovel-Local-Test-0.30.101-to-0.30.102'
$install=Join-Path $env:LOCALAPPDATA 'ShovelLocalTest'
$data=Join-Path $env:LOCALAPPDATA 'ShovelLocalTestData'
$registration='HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\ShovelLocalTest'
if ((Test-Path $install) -or (Test-Path $data) -or (Test-Path $registration)) { throw 'Existing local test found. Refusing to replace it.' }
$evidence=[ordered]@{StartedUtc=[DateTime]::UtcNow; InstallDirectory=$install; DataDirectory=$data; Versions=@('0.30.101','0.30.102'); Published=$false}
function Get-Version {
 $path=Join-Path $install 'current/sq.version'
 if(Test-Path -LiteralPath $path){ return ([xml](Get-Content -LiteralPath $path -Raw)).package.metadata.version }
 return $null
}
function Wait-Version([string]$expected) {
 for($i=0;$i -lt 60;$i++) { if((Get-Version) -eq $expected){return}; Start-Sleep -Seconds 1 }
 throw "Installed version did not become $expected."
}
try {
 $setup=Join-Path $kit 'Shovel-Local-Test-0.30.101-Setup.exe'
 $p=Start-Process -FilePath $setup -ArgumentList '--silent' -WindowStyle Hidden -Wait -PassThru
 $evidence.InstallExitCode=$p.ExitCode
 if($p.ExitCode -ne 0){throw 'Installation failed.'}
 Wait-Version '0.30.101'
 Start-Sleep -Seconds 3
 foreach($app in @(Get-Process Shovel -ErrorAction SilentlyContinue | Where-Object {$_.Path -eq (Join-Path $install 'current/Shovel.exe')})) {
  if (!$app.CloseMainWindow() -or !$app.WaitForExit(15000)) { throw 'Close the isolated test app before continuing.' }
 }
 $evidence.InstalledVersion=Get-Version
 $entry=Get-ItemProperty -LiteralPath $registration
 $evidence.UninstallDisplayName=$entry.DisplayName
 $evidence.UninstallCommand=$entry.UninstallString
 $evidence.RegisteredInstallLocation=$entry.InstallLocation
 New-Item -ItemType Directory -Path (Join-Path $data 'Profile'),(Join-Path $data 'Drafts') | Out-Null
 $sentinel=Join-Path $data 'Profile/lifecycle-sentinel.txt'
 [IO.File]::WriteAllText($sentinel,'Preserve isolated profile through update.')
 $hash=(Get-FileHash $sentinel).Hash
 $p=Start-Process -FilePath (Join-Path $install 'current/Shovel.exe') -ArgumentList '--local-update-probe' -WindowStyle Hidden -Wait -PassThru
 $evidence.UpdateProbeExitCode=$p.ExitCode
 if($p.ExitCode -ne 0){throw 'Signed update probe failed.'}
 Wait-Version '0.30.102'
 $evidence.UpdatedVersion=Get-Version
 $evidence.ProfilePreserved=((Get-FileHash $sentinel).Hash -eq $hash)
 if(!$evidence.ProfilePreserved){throw 'Test profile changed.'}
 $p=Start-Process -FilePath (Join-Path $install 'current/Shovel.exe') -ArgumentList '--local-install-probe' -WindowStyle Hidden -Wait -PassThru
 $evidence.UpdatedLaunchExitCode=$p.ExitCode
 if($p.ExitCode -ne 0){throw 'Updated bootstrap failed.'}
 # Native uninstall first; verify it retains settings before explicit clean sweep.
 $p=Start-Process -FilePath (Join-Path $install 'Update.exe') -ArgumentList 'uninstall','--silent' -WindowStyle Hidden -Wait -PassThru
 $evidence.UninstallExitCode=$p.ExitCode
 if($p.ExitCode -ne 0){throw 'Uninstall failed.'}
 Start-Sleep -Seconds 2
 $evidence.RegistrationRemoved=!(Test-Path -LiteralPath $registration)
 $evidence.ApplicationRemoved=!(Test-Path -LiteralPath (Join-Path $install 'current/Shovel.exe'))
 $evidence.ProfileRetainedByUninstall=Test-Path -LiteralPath $sentinel
 if(!$evidence.RegistrationRemoved -or !$evidence.ApplicationRemoved){throw 'Uninstall left application or registration.'}
 & (Join-Path $PSScriptRoot 'Remove-LocalTest.ps1') -RemoveTestData
 $evidence.CleanSweepRemovedInstall=!(Test-Path -LiteralPath $install)
 $evidence.CleanSweepRemovedData=!(Test-Path -LiteralPath $data)
 $evidence.Passed=$evidence.CleanSweepRemovedInstall -and $evidence.CleanSweepRemovedData
} catch {
 $evidence.Passed=$false
 $evidence.Failure=$_.Exception.Message
 throw
} finally {
 $evidence.FinishedUtc=[DateTime]::UtcNow
 $evidence | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $kit 'LOCAL-TEST-RESULT.json')
}
$evidence | ConvertTo-Json -Depth 5
