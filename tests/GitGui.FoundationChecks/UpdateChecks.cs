using System.IO;
using System.Net;
using System.Net.Http;
using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
using GitGui.App.Services;
using GitGui.App.ViewModels;
using Velopack;

namespace GitGui.FoundationChecks;
internal static partial class Program
{
    private static int RunUpdateChecks()
    {
        _routine = true;
        _root = Path.Combine(Path.GetFullPath("artifacts/update-checks"), Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(_root);
        using var deadline = RoutineDeadline(TimeSpan.FromMinutes(2), "Update checks exceeded 120 seconds.");
        try
        {
            var app = new HeadlessApplication(); app.InitializeComponent();
            var dispatcher = System.Windows.Threading.Dispatcher.CurrentDispatcher;
            SynchronizationContext.SetSynchronizationContext(new System.Windows.Threading.DispatcherSynchronizationContext(dispatcher));
            var emptyConfig = Path.Combine(_root, "empty.gitconfig"); File.WriteAllText(emptyConfig, "");
            Environment.SetEnvironmentVariable("GIT_CONFIG_GLOBAL", emptyConfig);
            Environment.SetEnvironmentVariable("GIT_CONFIG_NOSYSTEM", "1");
            var frame = new System.Windows.Threading.DispatcherFrame();
            var task = RunUpdateChecksAsync();
            async Task Finish() { try { await task; } finally { frame.Continue = false; } }
            var finish = Finish();
            System.Windows.Threading.Dispatcher.PushFrame(frame);
            finish.GetAwaiter().GetResult();
            Console.WriteLine($"{_passed} update checks passed; {_failed} failed."); return _failed == 0 ? 0 : 1;
        }
        catch (Exception error) { Console.Error.WriteLine(error); return 1; }
    }
    private static async Task RunUpdateChecksAsync()
    {
        using var key = RSA.Create(3072);
        var config = new UpdateConfiguration("owner/shovel-releases", key.ExportSubjectPublicKeyInfoPem(), "win-stable");
        var package = Encoding.UTF8.GetBytes("fixture package, not executed");
        var assetJson = new { PackageId="Shovel", Version="0.31.0", Type="Full", FileName="Shovel-0.31.0-win-stable-full.nupkg",
            SHA256=Convert.ToHexString(SHA256.HashData(package)), Size=package.Length, NotesMarkdown="Plain **notes** <script>not executable</script>" };
        var feed = JsonSerializer.SerializeToUtf8Bytes(new { Assets=new[] { assetJson } });
        byte[] Sign(byte[] bytes) => key.SignData(bytes, HashAlgorithmName.SHA256, RSASignaturePadding.Pss);
        var signature = Sign(feed);
        var asset = SignedReleaseSource.VerifyFeed(config, feed, signature).Assets.Single();
        await Check("Signed Velopack feed validates and rejects tampering", () =>
        {
            Expect(asset.Version.ToString() == "0.31.0", "Signed version parsed");
            var corrupt = feed.ToArray(); corrupt[10] ^= 1;
            ExpectThrows(() => SignedReleaseSource.VerifyFeed(config, corrupt, signature));
            using var otherKey = RSA.Create(3072);
            ExpectThrows(() => SignedReleaseSource.VerifyFeed(config with { PublicKey=otherKey.ExportSubjectPublicKeyInfoPem() }, feed, signature));
            return Task.CompletedTask;
        });
        await Check("Signed unsupported packages and unsafe filenames are rejected", () =>
        {
            foreach (var replacement in new[] { "../Shovel.nupkg", "https://other.invalid/package", "Shovel-0.31.0-win-beta-full.nupkg" })
            {
                var bad = JsonSerializer.SerializeToUtf8Bytes(new { Assets=new[] { assetJson with { FileName=replacement } } });
                ExpectThrows(() => SignedReleaseSource.VerifyFeed(config, bad, Sign(bad)));
            }
            var delta = JsonSerializer.SerializeToUtf8Bytes(new { Assets=new[] { assetJson with { Type="Delta" } } });
            ExpectThrows(() => SignedReleaseSource.VerifyFeed(config, delta, Sign(delta)));
            return Task.CompletedTask;
        });
        await Check("GitHub feed and immutable package download verify exact bytes", async () =>
        {
            var handler = new FixtureUpdateHttp(uri => uri.AbsolutePath.EndsWith(".sig") ? signature : uri.AbsolutePath.EndsWith(".json") ? feed : package);
            using var client = new HttpClient(handler);
            var source = new SignedReleaseSource(config, client);
            var result = await source.GetReleaseFeed(QuietUpdateLogger.Instance, "Shovel", config.Channel);
            var path = Path.Combine(_root, "download.nupkg");
            await source.DownloadReleaseEntry(QuietUpdateLogger.Instance, result.Assets.Single(), path, _ => { });
            Expect(File.ReadAllBytes(path).SequenceEqual(package), "Exact verified package");
            Expect(handler.Requests.Count == 3 && handler.Requests.All(u => u.Host == "github.com" && u.Query == ""), "No API or device identifier");
            Expect(handler.Requests.Last().AbsolutePath.Contains("/v0.31.0-win-stable/"), "Immutable package URL");
            Expect(new SignedReleaseSource(config with { Channel="win-beta" }, client).FeedUri.AbsolutePath.Contains("/download/beta/"), "Separate test feed");
        });
        await Check("Wrong hash and cancelled downloads leave no partial package", async () =>
        {
            using var client = new HttpClient(new FixtureUpdateHttp(uri => uri.AbsolutePath.EndsWith(".sig") ? signature : uri.AbsolutePath.EndsWith(".json") ? feed : new byte[package.Length]));
            var source = new SignedReleaseSource(config, client);
            await source.GetReleaseFeed(QuietUpdateLogger.Instance, "Shovel", config.Channel);
            var path = Path.Combine(_root, "invalid.nupkg");
            await ExpectAsyncThrows(() => source.DownloadReleaseEntry(QuietUpdateLogger.Instance, asset, path, _ => { }));
            Expect(!File.Exists(path), "Bad bytes deleted");
            using var cancelled = new CancellationTokenSource(); cancelled.Cancel();
            await ExpectAsyncThrows(() => source.DownloadReleaseEntry(QuietUpdateLogger.Instance, asset, path, _ => { }, cancelled.Token));
            Expect(!File.Exists(path), "Cancelled package absent");
        });
        await Check("Offline, login pages and oversized feeds fail without fallback", async () =>
        {
            foreach (var code in new[] { HttpStatusCode.Forbidden, HttpStatusCode.ServiceUnavailable, HttpStatusCode.OK })
            {
                using var client = new HttpClient(new FixtureUpdateHttp(_ => feed, code, "text/html"));
                await ExpectAsyncThrows(() => new SignedReleaseSource(config, client).GetReleaseFeed(QuietUpdateLogger.Instance, "Shovel", config.Channel));
            }
            using var large = new HttpClient(new FixtureUpdateHttp(_ => new byte[SignedReleaseSource.MaximumFeedSize + 1]));
            await ExpectAsyncThrows(() => new SignedReleaseSource(config, large).GetReleaseFeed(QuietUpdateLogger.Instance, "Shovel", config.Channel));
        });
        await Check("Older signed feeds cannot replay below a remembered version", async () =>
        {
            var trust = Path.Combine(_root, "trust"); Directory.CreateDirectory(trust);
            var verified = SignedReleaseSource.VerifyFeed(config, feed, signature);
            await SignedReleaseSource.EnforceVersionFloorAsync(verified, trust, config.Channel, default);
            var olderBytes = JsonSerializer.SerializeToUtf8Bytes(new { Assets=new[] { assetJson with { Version="0.30.0", FileName="Shovel-0.30.0-win-stable-full.nupkg" } } });
            var older = SignedReleaseSource.VerifyFeed(config, olderBytes, Sign(olderBytes));
            await ExpectAsyncThrows(() => SignedReleaseSource.EnforceVersionFloorAsync(older, trust, config.Channel, default));
            Expect(File.ReadAllText(Path.Combine(trust, "shovel-trusted-version-win-stable")) == "0.31.0", "Trusted floor retained");
        });
        await Check("Staged package is rehashed before restart", async () =>
        {
            var path = Path.Combine(_root, "staged.nupkg"); File.WriteAllBytes(path, package);
            await SignedReleaseSource.VerifyPackageAsync(asset, path, default);
            File.WriteAllBytes(path, new byte[package.Length]);
            await ExpectAsyncThrows(() => SignedReleaseSource.VerifyPackageAsync(asset, path, default));
        });
        await Check("Launch checks require approval before download and restart", async () =>
        {
            var fake = new FixtureUpdater(config); var restarts = 0;
            var vm = new UpdateViewModel(fake, () => { restarts++; return Task.FromResult(true); }) { PeriodicChecks = false };
            await vm.CheckOnLaunchAsync(default);
            Expect(fake.Checks == 1 && fake.Downloads == 0 && restarts == 0 && vm.Offered, "Check only at launch");
            await vm.CheckOnLaunchAsync(default);
            Expect(fake.Checks == 1, "Once per launch");
            vm.RestartCommand.Execute(null); await vm.ActiveTask;
            Expect(fake.Downloads == 1 && restarts == 1, "Single approval downloads then restarts");
        });
        await Check("Launch failures, cancellation, current and portable builds never install; blocked restart is retained", async () =>
        {
            var restarts = 0;
            foreach (var phase in new[] { UpdatePhase.Current, UpdatePhase.Failed, UpdatePhase.Cancelled })
            {
                var vm = new UpdateViewModel(new FixtureUpdater(config) { ResultPhase = phase }, () => { restarts++; return Task.FromResult(true); });
                await vm.CheckOnLaunchAsync(default);
            }
            var offline = new UpdateViewModel(new FixtureUpdater(config) { Fail = true }, () => { restarts++; return Task.FromResult(true); });
            await offline.CheckOnLaunchAsync(default);
            var portable = new FixtureUpdater(config) { Available = false };
            await new UpdateViewModel(portable, () => { restarts++; return Task.FromResult(true); }).CheckOnLaunchAsync(default);
            using var cancelled = new CancellationTokenSource(); cancelled.Cancel();
            var skipped = new FixtureUpdater(config);
            await new UpdateViewModel(skipped, () => { restarts++; return Task.FromResult(true); }).CheckOnLaunchAsync(cancelled.Token);
            Expect(restarts == 0 && portable.Checks == 0 && skipped.Checks == 0, "No inappropriate restart or cancelled network work");
            var blocked = new UpdateViewModel(new FixtureUpdater(config), () => { restarts++; return Task.FromResult(false); });
            await blocked.CheckOnLaunchAsync(default);
            blocked.RestartCommand.Execute(null); await blocked.ActiveTask;
            Expect(restarts == 1 && blocked.Ready && blocked.Status.Contains("blocked", StringComparison.OrdinalIgnoreCase) && blocked.RestartCommand.CanExecute(null), "Blocked auto-restart keeps manual retry");
        });
        await Check("Manual download retains explicit restart and blocked state", async () =>
        {
            var fake = new FixtureUpdater(config); var restartCount = 0;
            var vm = new UpdateViewModel(fake, () => { restartCount++; return Task.FromResult(false); });
            await vm.CheckAsync();
            Expect(vm.Offered && fake.Downloads == 0 && restartCount == 0, "Check never downloads or installs");
            vm.RestartCommand.Execute(null); await vm.ActiveTask;
            Expect(vm.Ready && restartCount == 1 && vm.Status.Contains("blocked", StringComparison.OrdinalIgnoreCase), "Busy restart retained");
            vm.Stop(); Expect(!vm.CheckCommand.CanExecute(null) && !vm.RestartCommand.CanExecute(null), "Shutdown stops admission");
            vm.Resume(); Expect(vm.RestartCommand.CanExecute(null), "Failed close resumes commands");
        });
        await Check("Unconfigured builds perform no update work; failures remain retryable", async () =>
        {
            var fake = new FixtureUpdater(config) { Fail=true };
            var vm = new UpdateViewModel(fake, () => Task.FromResult(false));
            await vm.CheckAsync(); Expect(!vm.Ready && vm.CheckCommand.CanExecute(null), "Failed check retryable");
            var empty = new FixtureUpdater(new("", "", "win-stable")) { Available=false };
            var inactive = new UpdateViewModel(empty, () => Task.FromResult(false));
            await inactive.CheckAsync(); Expect(empty.Checks == 0, "Placeholder configuration never contacts a host");
        });
        await Check("Channel selection clears pending updates, persists and isolates signed feeds", async () =>
        {
            var fake = new FixtureUpdater(config);
            var vm = new UpdateViewModel(fake, () => Task.FromResult(false));
            await vm.CheckOnOpenAsync();
            await vm.CheckAsync(download: true); Expect(vm.Ready, "Fixture ready");
            vm.SelectedChannel = 1; await vm.ActiveTask;
            Expect(vm.Channel == "win-beta" && !vm.Ready && vm.Offered && fake.Downloads == 1, "Switch checks but invalidates pending install");
            var store = new GitGui.Core.Settings.SettingsStore(Path.Combine(_root, "channel-settings")); await store.LoadAsync(default);
            await store.SaveAsync(new() { UpdateChannel = vm.Channel }, default);
            Expect((await store.LoadAsync(default)).Settings.UpdateChannel == "win-beta", "Selection persists");
            var betaConfig = config with { Channel = "win-beta" };
            var betaFeed = JsonSerializer.SerializeToUtf8Bytes(new { Assets = new[] { assetJson with { FileName = "Shovel-0.31.0-win-beta-full.nupkg" } } });
            Expect(SignedReleaseSource.VerifyFeed(betaConfig, betaFeed, Sign(betaFeed)).Assets.Count() == 1, "Beta signed package accepted");
            ExpectThrows(() => SignedReleaseSource.VerifyFeed(config, betaFeed, Sign(betaFeed)));
            ExpectThrows(() => SignedReleaseSource.VerifyFeed(betaConfig, feed, signature));
            vm.SelectedChannel = 0; await vm.ActiveTask; Expect(vm.Channel == "win-stable" && !vm.Ready, "Stable switch also clears pending state");
        });
        await Check("Update preferences survive settings round-trip", async () =>
        {
            var store = new GitGui.Core.Settings.SettingsStore(Path.Combine(_root, "settings"));
            await store.LoadAsync(default);
            await store.SaveAsync(new() { CheckUpdatesPeriodically=false }, default);
            Expect(!(await store.LoadAsync(default)).Settings.CheckUpdatesPeriodically, "Periodic preference persisted");
        });
        await Check("Schema 6 profile permits update and preserves saved layout", async () =>
        {
            var directory = Path.Combine(_root, "schema6-profile"); Directory.CreateDirectory(directory);
            var store = new GitGui.Core.Settings.SettingsStore(directory);
            await File.WriteAllTextAsync(store.FilePath, """
                {"SchemaVersion":6,"HistoryFilesWidth":277,"DetailsHeight":900,
                 "CommitLayout":{"Width":1300,"Height":850,"FilesWidth":411,"MessageHeight":234,"ConflictSourcesFraction":0.6},
                 "CheckUpdatesPeriodically":false}
                """);
            var loaded = await store.LoadAsync(default);
            Expect(loaded.CanSave, "Previously rejected schema 6 is writable");
            var model = new ShellViewModel(); model.CompleteStartup(loaded);
            var window = new GitGui.App.Views.MainWindow(model); window.RestoreLayout(loaded.Settings);
            var captured = model.CaptureSession(window.CaptureLayout());
            Expect(captured.CommitLayout == loaded.Settings.CommitLayout && captured.HistoryFilesWidth == 277,
                "Window capture retains fields owned by newer layout views");
            model.BeginShutdown(); await model.WaitForIdleAsync();
            Expect(await model.VerifyUpdateIdleAsync(default), "Compatible empty profile permits update");
            await store.SaveAsync(captured, default);
            var saved = await store.LoadAsync(default);
            Expect(saved.CanSave && saved.Settings.CommitLayout == loaded.Settings.CommitLayout && saved.Settings.HistoryFilesWidth == 277,
                "Saved layout survives restart round-trip");
            Expect(File.Exists(store.FilePath + ".bak"), "Original settings backed up");
        });
        await Check("Future settings remain protected and explain blocked restart", async () =>
        {
            var directory = Path.Combine(_root, "future-profile"); Directory.CreateDirectory(directory);
            var store = new GitGui.Core.Settings.SettingsStore(directory);
            var bytes = Encoding.UTF8.GetBytes("{\"SchemaVersion\":999,\"FutureData\":\"preserve\"}");
            await File.WriteAllBytesAsync(store.FilePath, bytes);
            var loaded = await store.LoadAsync(default);
            var model = new ShellViewModel(); model.CompleteStartup(loaded); model.BeginShutdown();
            Expect(!loaded.CanSave && !await model.VerifyUpdateIdleAsync(default), "Unknown schema does not bypass update gate");
            await ExpectAsyncThrows(() => store.SaveAsync(new(), default));
            Expect(File.ReadAllBytes(store.FilePath).SequenceEqual(bytes), "Future data unchanged");
            var vm = new UpdateViewModel(new FixtureUpdater(config), () => Task.FromResult(false),
                () => GitGui.App.Localization.UiText.Get("UpdateSettingsBlocked"));
            await vm.CheckAsync(); vm.RestartCommand.Execute(null); await vm.ActiveTask;
            Expect(vm.Ready && vm.Status.Contains("settings", StringComparison.OrdinalIgnoreCase) &&
                vm.Status.Contains("installer", StringComparison.OrdinalIgnoreCase), "Specific reason and recovery guidance shown");
        });
        await CheckUpdateGateAndLayoutAsync(config);
    }
    private static async Task CheckUpdateGateAndLayoutAsync(UpdateConfiguration config)
    {
        await Check("Update restart gate checks all repositories and refuses recovery", async () =>
        {
            var runner = new GitGui.ProcessHost.WindowsProcessRunner(); var reader = new GitGui.Core.Git.RepositoryReader(runner);
            var git = await reader.DiscoverAsync(null, default);
            var path = Path.Combine(_root, "gate-repo"); Directory.CreateDirectory(path);
            async Task Git(params string[] args)
            {
                var result = await runner.RunAsync(new(git.Executable, path, args), default);
                Expect(result.ExitCode == 0, "Fixture Git failed");
            }
            await Git("init", "-b", "main"); await File.WriteAllTextAsync(Path.Combine(path, "file.txt"), "data");
            await Git("add", "."); await Git("-c", "user.name=Fixture", "-c", "user.email=test@example.invalid", "commit", "-m", "fixture");
            var model = new ShellViewModel(reader, new ModelInteraction(path) { Executable=git.Executable });
            model.CompleteStartup(new(new() { RepositoryPaths=[path] }, GitGui.Core.Settings.SettingsLoadStatus.Defaults));
            await model.WaitForIdleAsync();
            Expect(model.CanPrepareUpdate, "Open app is initially ready");
            Expect(!await model.VerifyUpdateIdleAsync(default), "Final validation requires closed admission");
            model.BeginShutdown(); await model.WaitForIdleAsync();
            Expect(!model.OpenRepositoryCommand.CanExecute(null), "No new work during final inspection");
            Expect(await model.VerifyUpdateIdleAsync(default), "Clean known repository permits restart");
            var rev = await runner.RunAsync(new(git.Executable, path, ["rev-parse", "HEAD"]), default);
            File.WriteAllBytes(Path.Combine(path, ".git", "MERGE_HEAD"), rev.StandardOutput);
            Expect(!await model.VerifyUpdateIdleAsync(default), "Active merge prevents restart");
            File.Delete(Path.Combine(path, ".git", "MERGE_HEAD"));
            await Git("switch", "-c", "side"); await File.WriteAllTextAsync(Path.Combine(path, "file.txt"), "side");
            await Git("-c", "user.name=Fixture", "-c", "user.email=test@example.invalid", "commit", "-am", "side");
            await Git("switch", "main"); await File.WriteAllTextAsync(Path.Combine(path, "file.txt"), "main");
            await Git("-c", "user.name=Fixture", "-c", "user.email=test@example.invalid", "commit", "-am", "main");
            var conflict = await runner.RunAsync(new(git.Executable, path, ["-c", "user.name=Fixture", "-c", "user.email=test@example.invalid", "merge", "side"]), default);
            Expect(conflict.ExitCode == 1, "Real unmerged index fixture");
            File.Delete(Path.Combine(path, ".git", "MERGE_HEAD"));
            Expect(!await model.VerifyUpdateIdleAsync(default), "Unmerged index without sequence blocks restart");
            await Git("reset", "--hard", "HEAD"); // Owned disposable fixture only.
            model.Repositories.Add(new(Path.Combine(_root, "unvalidated")));
            Expect(!await model.VerifyUpdateIdleAsync(default), "Unvalidated inactive tab blocks restart");
            model.ResumeAfterSaveFailure(); Expect(model.CanNavigate, "Blocked update keeps app usable");
        });
        await Check("Update window renders with shared Dark and Light resources", async () =>
        {
            var vm = new UpdateViewModel(new FixtureUpdater(config), () => Task.FromResult(false)); await vm.CheckAsync();
            var theme = new AppTheme(System.Windows.Application.Current);
            foreach (var appearance in new[] { GitGui.Core.Settings.AppAppearance.Dark, GitGui.Core.Settings.AppAppearance.Light })
            {
                theme.Select(appearance);
                var window = new GitGui.App.Views.UpdateWindow(vm);
                var content = (System.Windows.FrameworkElement)window.Content; content.DataContext = vm;
                Arrange(content, 584, 514); SaveRender(content, $"updates-{appearance}.png", 584, 514);
                Arrange(content, 484, 444); SaveRender(content, $"updates-{appearance}-minimum.png", 484, 444);
                Expect(VisualDescendants<System.Windows.Controls.Button>(content).Count() == 3, "One primary update action plus Cancel and Close");
            }
            theme.Dispose();
        });
    }
    private static void ExpectThrows(Action action)
    { try { action(); } catch (Exception) { return; } throw new Exception("Expected rejection"); }
    private static async Task ExpectAsyncThrows(Func<Task> action)
    { try { await action(); } catch (Exception) { return; } throw new Exception("Expected rejection"); }
    private sealed class FixtureUpdateHttp(Func<Uri,byte[]> bytes, HttpStatusCode code=HttpStatusCode.OK, string media="application/octet-stream") : HttpMessageHandler
    {
        public List<Uri> Requests { get; } = [];
        protected override Task<HttpResponseMessage> SendAsync(HttpRequestMessage request, CancellationToken cancellationToken)
        {
            cancellationToken.ThrowIfCancellationRequested(); Requests.Add(request.RequestUri!);
            var content = new ByteArrayContent(bytes(request.RequestUri!)); content.Headers.ContentType = new(media);
            return Task.FromResult(new HttpResponseMessage(code) { RequestMessage=request, Content=content });
        }
    }
    private sealed class FixtureUpdater(UpdateConfiguration configuration) : IAppUpdater
    {
        public UpdateConfiguration Configuration { get; private set; } = configuration;
        public void SelectChannel(string channel) => Configuration = Configuration with { Channel = channel };
        public bool Available { get; set; } = true;
        public bool Fail { get; set; }
        public UpdatePhase ResultPhase { get; set; } = UpdatePhase.Ready;
        public int Checks { get; private set; }
        public int Downloads { get; private set; }
        public Task CheckAsync(IProgress<UpdateSnapshot> progress, CancellationToken token)
        { token.ThrowIfCancellationRequested(); Checks++; if (Fail) throw new IOException(); progress.Report(new(ResultPhase == UpdatePhase.Ready ? UpdatePhase.Offered : ResultPhase, "0.31.0")); return Task.CompletedTask; }
        public Task DownloadApprovedAsync(IProgress<UpdateSnapshot> progress, CancellationToken token)
        { token.ThrowIfCancellationRequested(); Downloads++; if (Fail) throw new IOException(); progress.Report(new(ResultPhase, "0.31.0")); return Task.CompletedTask; }
        public Task VerifyReadyAsync(CancellationToken token) => Task.CompletedTask;
        public void Apply() => throw new Exception("Must never apply during a check");
    }
}
