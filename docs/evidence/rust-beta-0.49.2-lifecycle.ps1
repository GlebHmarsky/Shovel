#requires -Version 7.4
param([string]$Root=(Resolve-Path (Join-Path $PSScriptRoot '../..')).Path)
$ErrorActionPreference='Stop'
if([IO.File]::ReadAllText((Join-Path $root 'Cargo.toml')) -notmatch 'version = "0\.49\.2"'){throw 'This frozen acceptance fixture targets Rust 0.49.2 source.'}
$kit=Join-Path $root ('artifacts/rust-lifecycle-'+[Guid]::NewGuid().ToString('N'))
if(Test-Path $kit){throw 'Fixture already exists; preserve evidence.'}
$source=Join-Path $kit 'source'
$install=Join-Path $kit 'installed'
$feed=Join-Path $kit 'feed'
New-Item -ItemType Directory -Path $kit,$source,$feed | Out-Null
git -C $root archive HEAD -o (Join-Path $kit 'source.zip')
if($LASTEXITCODE -ne 0){throw 'Source export failed'}
Expand-Archive -LiteralPath (Join-Path $kit 'source.zip') -DestinationPath $source
$reg='HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\ShovelRustLifecycle'
if(Test-Path $reg){throw 'Existing fixture registration; stop.'}
$proof=[ordered]@{SourceCommit=(git -C $root rev-parse HEAD);Identity='ShovelRustLifecycle';Channel='win-rust-beta';From='0.49.1';To='0.49.2';ProductionPayload=$false;ProductionKey=$false;Transport='local signed fixture';Limit='Fixture-only local transport and console orchestration, no UI input or public GitHub discovery.'}
# Only the copied source receives test identity, key, local transport and console orchestration.
foreach($file in Get-ChildItem (Join-Path $source 'rust') -Recurse -File | Where-Object {$_.Extension -in '.rs','.toml'}){
 $text=[IO.File]::ReadAllText($file.FullName).Replace('ShovelRust','ShovelRustLifecycle')
 [IO.File]::WriteAllText($file.FullName,$text)
}
$key=[Security.Cryptography.RSA]::Create(3072)
[IO.File]::WriteAllText((Join-Path $source 'distribution/update-public.pem'),$key.ExportSubjectPublicKeyInfoPem())
$updater=Join-Path $source 'rust/desktop/src/updater.rs'
$text=[IO.File]::ReadAllText($updater)
$transport=@'
    if let Some(base) = std::env::var_os("SHOVEL_LIFECYCLE_FEED") {
        let name = url.rsplit('/').next().ok_or_else(||text("RustUpdateInvalid"))?;
        let path = PathBuf::from(base).join(name);
        let mut file = File::open(path).map_err(failure)?;
        let mut total=0;
        let mut buffer=[0u8;65536];
        loop {
            if cancel.is_cancelled(){return Err(text("ActionResultCancelled"));}
            let n=file.read(&mut buffer).map_err(failure)?;
            if n==0 {break;}
            total+=n as u64;
            if total>max{return Err(text("RustUpdateInvalid"));}
            out.write_all(&buffer[..n]).map_err(failure)?;
            if let Some(tx)=progress{let _=tx.send((total*100/max.max(1)) as i16);}
        }
        return Ok(());
    }
'@
$text=$text.Replace('    let agent: ureq::Agent =',$transport+[Environment]::NewLine+'    let agent: ureq::Agent =')
$text += [IO.File]::ReadAllText((Join-Path $PSScriptRoot 'rust-beta-0.49.2-lifecycle-probe.rs'))
[IO.File]::WriteAllText($updater,$text)
$main=Join-Path $source 'rust/desktop/src/main.rs'
$text=[IO.File]::ReadAllText($main).Replace('    let options = eframe::NativeOptions {','    if updater::acceptance_probe() { return Ok(()); }'+[Environment]::NewLine+'    let options = eframe::NativeOptions {')
[IO.File]::WriteAllText($main,$text)
. (Join-Path $root 'scripts/Use-RustToolchain.ps1')
$env:CARGO_TARGET_DIR=Join-Path $root 'target'
$env:CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS='-C target-feature=+crt-static'
$env:SHOVEL_LIFECYCLE_ROOT=$kit
$env:SHOVEL_LIFECYCLE_FEED=$feed
try {
 Push-Location $source
 try {
  foreach($version in @('0.49.1','0.49.2')){
   foreach($name in @('Cargo.toml','Cargo.lock')){
    $path=Join-Path $source $name
    $text=[IO.File]::ReadAllText($path) -replace '(?m)^version = "0\.49\.[12]"\r?$',('version = "'+$version+'"')
    [IO.File]::WriteAllText($path,$text)
   }
   & cargo build --locked --release --target x86_64-pc-windows-msvc -j 1 -p shovel-desktop -p shovel-askpass
   if($LASTEXITCODE -ne 0){throw 'Fixture build failed'}
   $payload=Join-Path $kit "payload-$version"
   $output=Join-Path $kit "installer-$version"
   New-Item -ItemType Directory -Path $payload | Out-Null
   Copy-Item (Join-Path $env:CARGO_TARGET_DIR 'x86_64-pc-windows-msvc/release/ShovelRustLifecycle.exe'),(Join-Path $env:CARGO_TARGET_DIR 'x86_64-pc-windows-msvc/release/ShovelRustLifecycleAskPass.exe') $payload
   & dotnet vpk pack --packId ShovelRustLifecycle --packVersion $version --packDir $payload --mainExe ShovelRustLifecycle.exe --packTitle 'Shovel Rust Lifecycle Test' --channel win-rust-beta --runtime win-x64 --delta None --outputDir $output
   if($LASTEXITCODE -ne 0){throw 'Fixture packaging failed'}
  }
 } finally {Pop-Location}
 $latest=Join-Path $kit 'installer-0.49.2'
 $metadata=Get-Content (Join-Path $latest 'releases.win-rust-beta.json') -Raw | ConvertFrom-Json
 $signed=[ordered]@{SchemaVersion=1;Product='ShovelRustLifecycle';Channel='win-rust-beta';Architecture='win-x64';SourceCommit=$proof.SourceCommit;Assets=@($metadata.Assets)}
 $feedFile=Join-Path $feed 'releases.win-rust-beta.json'
 $signed | ConvertTo-Json -Depth 10 | Set-Content $feedFile -Encoding utf8NoBOM
 [IO.File]::WriteAllBytes("$feedFile.sig",$key.SignData([IO.File]::ReadAllBytes($feedFile),[Security.Cryptography.HashAlgorithmName]::SHA256,[Security.Cryptography.RSASignaturePadding]::Pss))
 Copy-Item (Join-Path $latest 'ShovelRustLifecycle-0.49.2-win-rust-beta-full.nupkg') $feed
 git init -q (Join-Path $kit 'repo')
 if($LASTEXITCODE -ne 0){throw 'Fixture repository initialization failed'}
 function Run-Native([string]$path,[string[]]$arguments){
  $psi=[Diagnostics.ProcessStartInfo]::new($path);$psi.UseShellExecute=$false;$psi.CreateNoWindow=$true
  foreach($argument in $arguments){$psi.ArgumentList.Add($argument)}
  $p=[Diagnostics.Process]::Start($psi)
  if(!$p.WaitForExit(60000)){throw 'Fixture process did not exit; leave it intact for inspection.'}
  if($p.ExitCode -ne 0){throw "Fixture process failed with exit $($p.ExitCode)"}
 }
 function Version {([xml](Get-Content (Join-Path $install 'current/sq.version') -Raw)).package.metadata.version}
 $setup=@(Get-ChildItem (Join-Path $kit 'installer-0.49.1') -Filter '*Setup.exe')
 Run-Native $setup[0].FullName @('--silent','--installto',$install)
 $proof.Installed=Version
 if($proof.Installed -ne '0.49.1'){throw 'Initial native installation failed'}
 $entry=Get-ItemProperty $reg
 if($entry.DisplayName -ne 'Shovel Rust Lifecycle Test' -or $entry.InstallLocation.TrimEnd('\') -ne $install){throw 'Unexpected fixture registration'}
 $proof.Registered=$true
 $exe=Join-Path $install 'current/ShovelRustLifecycle.exe'
 Run-Native $exe @('--lifecycle-initialize')
 $before=@(Get-ChildItem (Join-Path $kit 'Profile'),(Join-Path $kit 'Drafts') -File | ForEach-Object {(Get-FileHash $_.FullName).Hash})
 Run-Native $exe @('--lifecycle-update')
 for($i=0;$i -lt 60 -and !(Test-Path (Join-Path $kit 'restarted.json'));$i++){Start-Sleep -Seconds 1}
 if(!(Test-Path (Join-Path $kit 'restarted.json'))){throw 'Native updated process did not report restart'}
 $proof.Restart=Get-Content (Join-Path $kit 'restarted.json') -Raw | ConvertFrom-Json
 $proof.Updated=Version
 if($proof.Updated -ne '0.49.2' -or $proof.Restart.Version -ne '0.49.2'){throw 'Version did not advance'}
 $after=@(Get-ChildItem (Join-Path $kit 'Profile'),(Join-Path $kit 'Drafts') -File | ForEach-Object {(Get-FileHash $_.FullName).Hash})
 $proof.ProfileAndDraftBytesPreserved=!(Compare-Object $before $after)
 if(!$proof.ProfileAndDraftBytesPreserved){throw 'Profile/draft bytes changed'}
 $setup=@(Get-ChildItem $latest -Filter '*Setup.exe')
 Run-Native $setup[0].FullName @('--silent','--installto',$install)
 $proof.RepairedVersion=Version
 if($proof.RepairedVersion -ne '0.49.2'){throw 'Repair failed'}
 Run-Native (Join-Path $install 'Update.exe') @('uninstall','--silent')
 $proof.Uninstalled=!(Test-Path $exe) -and !(Test-Path $reg)
 $proof.DataRetained=Test-Path (Join-Path $kit 'Profile/settings.json')
 if(!$proof.Uninstalled -or !$proof.DataRetained){throw 'Uninstall verification failed'}
 $proof.Passed=$true
} catch {$proof.Passed=$false;$proof.Error=$_.Exception.Message;throw}
finally{
 $key.Dispose()
 $proof | ConvertTo-Json -Depth 8 | Set-Content (Join-Path $kit 'RESULT.json')
}
$proof | ConvertTo-Json -Depth 8
