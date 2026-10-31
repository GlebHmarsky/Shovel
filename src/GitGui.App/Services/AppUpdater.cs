using System.IO;
using System.Net.Http;
using Velopack;
using Velopack.Locators;
using Velopack.Logging;

namespace GitGui.App.Services;

public enum UpdatePhase { Unconfigured, Portable, Idle, Checking, Current, Downloading, Ready, Cancelled, Failed, Blocked, Installing }
public sealed record UpdateSnapshot(UpdatePhase Phase, string Version = "", int Percent = 0, string Notes = "");
public interface IAppUpdater
{
    UpdateConfiguration Configuration { get; }
    bool Available { get; }
    Task CheckAndDownloadAsync(IProgress<UpdateSnapshot> progress, CancellationToken token);
    Task VerifyReadyAsync(CancellationToken token);
    void Apply();
}

public sealed class AppUpdater : IAppUpdater, IDisposable
{
    private readonly HttpClient _http = new() { Timeout = TimeSpan.FromMinutes(10) };
    private readonly UpdateManager? _manager;
    private readonly SignedReleaseSource? _source;
    private VelopackAsset? _ready;
    public UpdateConfiguration Configuration { get; }
    public bool Available => _manager is { IsInstalled: true, IsPortable: false };
    public AppUpdater(UpdateConfiguration configuration)
    {
        Configuration = configuration;
        _http.DefaultRequestHeaders.UserAgent.ParseAdd("Shovel-Updater/1.0");
        if (!configuration.IsConfigured) return;
        _source = new(configuration, _http, VelopackLocator.Current.RootAppDir);
        _manager = new(_source, new UpdateOptions { ExplicitChannel = configuration.Channel }, VelopackLocator.Current);
    }
    public async Task CheckAndDownloadAsync(IProgress<UpdateSnapshot> progress, CancellationToken token)
    {
        if (!Available) { progress.Report(new(Configuration.IsConfigured ? UpdatePhase.Portable : UpdatePhase.Unconfigured)); return; }
        _ready = null;
        progress.Report(new(UpdatePhase.Checking));
        // Full packages only: signed SHA256 covers the exact package handed to the updater.
        var update = await _manager!.CheckForUpdatesAsync();
        token.ThrowIfCancellationRequested();
        if (update is null) { progress.Report(new(UpdatePhase.Current)); return; }
        var asset = update.TargetFullRelease;
        progress.Report(new(UpdatePhase.Downloading, asset.Version.ToString()));
        await _manager.DownloadUpdatesAsync(update, percent => progress.Report(new(UpdatePhase.Downloading,
            asset.Version.ToString(), percent)), token);
        await SignedReleaseSource.VerifyPackageAsync(asset, Path.Combine(VelopackLocator.Current.PackagesDir!, asset.FileName), token);
        _ready = asset;
        progress.Report(new(UpdatePhase.Ready, asset.Version.ToString(), 100, asset.NotesMarkdown ?? ""));
    }
    public async Task VerifyReadyAsync(CancellationToken token)
    {
        if (_ready is null || _source is null) throw new InvalidOperationException();
        // Re-read the signed feed to honor a withdrawn release and re-hash the staged package.
        var feed = await _source.GetReleaseFeed(QuietUpdateLogger.Instance, "Shovel", Configuration.Channel);
        var asset = feed.Assets.SingleOrDefault(a => a.Version == _ready.Version && a.SHA256 == _ready.SHA256 && a.Size == _ready.Size);
        if (asset is null) throw new InvalidDataException();
        await SignedReleaseSource.VerifyPackageAsync(asset, Path.Combine(VelopackLocator.Current.PackagesDir!, asset.FileName), token);
    }
    public void Apply()
    {
        if (_ready is null) throw new InvalidOperationException();
        _manager!.WaitExitThenApplyUpdates(_ready, silent: true, restart: true, restartArgs: Environment.GetCommandLineArgs().Skip(1).ToArray());
    }
    public void Dispose() => _http.Dispose();
}
