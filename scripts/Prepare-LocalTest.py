# Generate isolated offline test copy; production configuration remains unchanged.
from pathlib import Path
import shutil, json, sys, re
root=Path(__file__).resolve().parent.parent
work=Path(sys.argv[1]); kit=Path(sys.argv[2]); public=Path(sys.argv[3]).read_text()
source=work/'source'
shutil.copytree(root, source, ignore=shutil.ignore_patterns('.git','artifacts','Build','bin','obj','.vs'))
for name in ['src/GitGui.App/Services/SignedReleaseSource.cs','src/GitGui.App/Services/AppUpdater.cs','src/GitGui.App/Services/ShovelUpdateLocator.cs']:
 p=source/name; s=p.read_text(encoding='utf-8').replace('"Shovel"','"ShovelLocalTest"').replace('$"Shovel-','$"ShovelLocalTest-'); p.write_text(s,encoding='utf-8')
p=source/'src/GitGui.App/Services/SignedReleaseSource.cs'; s=p.read_text(encoding='utf-8')
s=s.replace('        if (!configuration.IsConfigured', '        _ = client; // Keep constructor compatibility in the offline fixture.\n        if (!configuration.IsConfigured', 1)
a=s.index('    public Uri FeedUri'); b=s.index('\n    public async Task',a)
s=s[:a]+'    public Uri FeedUri => new Uri(Path.Combine('+json.dumps(str(kit/'feed'))+', FeedName));\n'+s[b:]
a=s.index('    public Uri PackageUri'); b=s.index('\n    public async Task',a)
s=s[:a]+'    public Uri PackageUri(VelopackAsset asset) => new Uri(Path.Combine('+json.dumps(str(kit/'feed'))+', asset.FileName));\n'+s[b:]
a=s.index('            using var response ='); b=s.index('            await using (var output',a)
s=s[:a]+'''            await using var input = new FileStream(PackageUri(releaseEntry).LocalPath, FileMode.Open, FileAccess.Read, FileShare.Read, 65536, true);
'''+s[b:]
a=s.index('        using var response =',s.index('private async Task<byte[]> ReadBoundedAsync')); b=s.index('        using var output =',a)
s=s[:a]+'''        if (!uri.IsFile || new FileInfo(uri.LocalPath).Length > maximum) throw new InvalidDataException();
        await using var input = new FileStream(uri.LocalPath, FileMode.Open, FileAccess.Read, FileShare.Read, 8192, true);
'''+s[b:]; p.write_text(s,encoding='utf-8')
p=source/'src/GitGui.App/Services/AppUpdater.cs'; s=p.read_text(encoding='utf-8').replace('restart: true','restart: !Environment.GetCommandLineArgs().Contains("--local-update-probe")'); p.write_text(s,encoding='utf-8')
p=source/'src/GitGui.App/Program.cs'; s=p.read_text(encoding='utf-8'); marker='        try { var app = new App();'
s=s.replace(marker,'''        if (args is ["--local-install-probe"]) { if (_ownsInstance) _installedInstance!.ReleaseMutex(); return; }
        if (args is ["--local-update-probe"])
        {
            using var updater = new Services.AppUpdater(Services.UpdateConfiguration.Load());
            updater.CheckAndDownloadAsync(new Progress<Services.UpdateSnapshot>(), CancellationToken.None).GetAwaiter().GetResult();
            updater.VerifyReadyAsync(CancellationToken.None).GetAwaiter().GetResult();
            updater.Apply();
            if (_ownsInstance) _installedInstance!.ReleaseMutex();
            return;
        }
'''+marker); p.write_text(s,encoding='utf-8')
p=source/'src/GitGui.App/App.xaml.cs'; s=p.read_text(encoding='utf-8').replace('"GitGui", "Foundation"','"ShovelLocalTestData", "Profile"').replace('new GitGui.Core.Drafts.CommitDraftStore(null,','new GitGui.Core.Drafts.CommitDraftStore(Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "ShovelLocalTestData", "Drafts"),'); p.write_text(s,encoding='utf-8')
p=source/'src/GitGui.App/Resources/Strings.resx'; s=p.read_text(encoding='utf-8').replace('<value>Shovel</value>','<value>Shovel Local Test</value>')
s=re.sub(r'(<data name="UpdatePrivacy"[^>]*><value>).*?(</value>)', r'\1Offline test: update checks read the signed local test feed. No GitHub update requests are made. Keep the test kit in its original folder. Installation requires Restart and update.\2', s)
p.write_text(s,encoding='utf-8')
p=source/'src/GitGui.App/ViewModels/UpdateViewModel.cs'; s=p.read_text(encoding='utf-8').replace('$"https://github.com/{Configuration.Repository}/releases"',json.dumps(str(kit/'feed'))); p.write_text(s,encoding='utf-8')
(source/'src/GitGui.App/updates.json').write_text(json.dumps(dict(Repository='GlebHmarsky/shovel-releases',PublicKey=public,Channel='win-test')),encoding='utf-8')
print(source)
