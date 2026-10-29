using System.IO;
using Velopack;
using Velopack.Locators;
using Velopack.Logging;
using Velopack.NuGet;
namespace GitGui.App.Services;

// Only the modern per-user Windows layout produced by our pinned packager is supported.
// The default locator always writes arbitrary paths/exceptions to its own log; this one does not.
public sealed class ShovelUpdateLocator : IVelopackLocator
{
    private readonly PackageManifest? _manifest;
    public ShovelUpdateLocator()
    {
        AppContentDir = AppContext.BaseDirectory.TrimEnd(Path.DirectorySeparatorChar);
        var root = Directory.GetParent(AppContentDir)!.FullName;
        var manifest = Path.Combine(AppContentDir, "sq.version");
        if (File.Exists(Path.Combine(root, "Update.exe")) && File.Exists(manifest))
        {
            _manifest = PackageManifest.ParseFromFile(manifest);
            if (_manifest.Id != "Shovel") throw new InvalidDataException();
            RootAppDir = root; PackagesDir = Path.Combine(root, "packages");
            Directory.CreateDirectory(PackagesDir);
            UpdateExePath = Path.Combine(root, "Update.exe");
        }
    }
    public string? AppId => _manifest?.Id;
    public string? RootAppDir { get; }
    public string? PackagesDir { get; }
    public string AppContentDir { get; }
    public string? AppTempDir => PackagesDir is null ? null : Directory.CreateDirectory(Path.Combine(PackagesDir, "VelopackTemp")).FullName;
    public string? UpdateExePath { get; }
    public SemanticVersion? CurrentlyInstalledVersion => _manifest?.Version;
    public string ThisExeRelativePath => "Shovel.exe";
    public string? Channel => _manifest?.Channel;
    public string? AppUserModelId => _manifest?.ShortcutAumid ?? "velopack.Shovel";
    public IVelopackLogger Log => QuietUpdateLogger.Instance;
    public bool IsPortable => RootAppDir is null || File.Exists(Path.Combine(RootAppDir, ".portable"));
    public IProcessImpl Process { get; } = new DefaultProcessImpl(QuietUpdateLogger.Instance);
    public void AddLogger(IVelopackLogger logger) { }
    public Guid? GetOrCreateStagedUserId() => null;
    public List<VelopackAsset> GetLocalPackages() => PackagesDir is null ? [] : Directory.EnumerateFiles(PackagesDir, "*.nupkg")
        .Take(100).Select(VelopackAsset.FromNupkgNoChecksum).ToList();
    public VelopackAsset? GetLatestLocalFullPackage() => GetLocalPackages().Where(a => a.Type == VelopackAssetType.Full).MaxBy(a => a.Version);
}
