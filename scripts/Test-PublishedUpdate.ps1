#requires -Version 7.4
param([ValidateSet('win-stable','win-beta')][string]$Channel='win-stable', [Parameter(Mandatory)][ValidatePattern('^\d+\.\d+\.\d+$')][string]$ExpectedVersion,
    [ValidatePattern('^\d+\.\d+\.\d+$')][string]$ClientVersion=$ExpectedVersion)
$ErrorActionPreference='Stop'
$root=Split-Path -Parent $PSScriptRoot
$preview=Join-Path $root "Build/$ClientVersion-win-x64"
foreach($name in @('Shovel.dll','Velopack.dll')) { if(!(Test-Path (Join-Path $preview $name))){throw 'Build the matching preview first.'} }
$work=Join-Path $root ('artifacts/published-update-probe-'+[Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory $work | Out-Null
$shovel=[Security.SecurityElement]::Escape((Join-Path $preview 'Shovel.dll'))
$velopack=[Security.SecurityElement]::Escape((Join-Path $preview 'Velopack.dll'))
@"
<Project Sdk="Microsoft.NET.Sdk">
 <PropertyGroup><OutputType>Exe</OutputType><TargetFramework>net10.0-windows</TargetFramework><UseWPF>true</UseWPF></PropertyGroup>
 <ItemGroup>
  <Reference Include="Shovel"><HintPath>$shovel</HintPath></Reference>
  <Reference Include="Velopack"><HintPath>$velopack</HintPath></Reference>
 </ItemGroup>
</Project>
"@ | Set-Content (Join-Path $work 'OnlineProbe.csproj')
@'
using System.IO;
using System.Net.Http;
using System.Text.Json;
using GitGui.App.Services;

var expected = args[0];
var output = args[1];
Directory.CreateDirectory(output);
using var http = new HttpClient { Timeout = TimeSpan.FromMinutes(10) };
http.DefaultRequestHeaders.UserAgent.ParseAdd("Shovel-Updater/1.0");
// Deliberately no Authorization header, cookies or GitHub credentials.
var config = UpdateConfiguration.Load() with { Channel = args[2] };
var source = new SignedReleaseSource(config, http, output);
var feed = await source.GetReleaseFeed(QuietUpdateLogger.Instance, "Shovel", config.Channel);
var asset = feed.Assets.Single(a => a.Version.ToString() == expected);
var package = Path.Combine(output, asset.FileName);
await source.DownloadReleaseEntry(QuietUpdateLogger.Instance, asset, package, _ => { });
var final = await source.GetReleaseFeed(QuietUpdateLogger.Instance, "Shovel", config.Channel);
if (!final.Assets.Any(a => a.Version == asset.Version && a.SHA256 == asset.SHA256 && a.Size == asset.Size))
    throw new InvalidDataException("Release changed during verification.");
await SignedReleaseSource.VerifyPackageAsync(asset, package, CancellationToken.None);
var result = JsonSerializer.Serialize(new {
    Passed = true, Version = expected, ClientVersion = typeof(UpdateConfiguration).Assembly.GetName().Version?.ToString(3),
    Repository = config.Repository, Feed = source.FeedUri, Package = source.PackageUri(asset),
    asset.Size, asset.SHA256, Anonymous = true, Installed = false,
    VerifiedUtc = DateTime.UtcNow
}, new JsonSerializerOptions { WriteIndented = true });
await File.WriteAllTextAsync(Path.Combine(output, "RESULT.json"), result);
Console.WriteLine(result);
'@ | Set-Content (Join-Path $work 'Program.cs')
dotnet run --project (Join-Path $work 'OnlineProbe.csproj') -c Release -p:UseSharedCompilation=false -- $ExpectedVersion (Join-Path $work 'download') $Channel
if($LASTEXITCODE -ne 0){throw 'Published update download verification failed.'}
Write-Output "Evidence: $work/download/RESULT.json"
