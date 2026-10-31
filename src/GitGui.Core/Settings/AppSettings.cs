using GitGui.Core.Git;

namespace GitGui.Core.Settings;

public sealed record RepositoryPreference(string Path, HistoryScope Scope);
public enum AppAppearance { Dark, Light }
public sealed record CommitLayout
{
    public double Width { get; init; } = 1200;
    public double Height { get; init; } = 800;
    public bool Maximized { get; init; }
    public double FilesWidth { get; init; } = 380;
    public double UnstagedFraction { get; init; } = .5;
    public double MessageHeight { get; init; } = 220;
    public double ConflictSourcesFraction { get; init; } = .55;
    public CommitLayout Normalize() => this with
    {
        Width = Bound(Width, 1200, 900, 7680), Height = Bound(Height, 800, 620, 4320),
        FilesWidth = Bound(FilesWidth, 380, 250, 3000), UnstagedFraction = Bound(UnstagedFraction, .5, .1, .9),
        MessageHeight = Bound(MessageHeight, 220, 180, 2000), ConflictSourcesFraction = Bound(ConflictSourcesFraction, .55, .2, .8)
    };
    private static double Bound(double value, double fallback, double min, double max) => double.IsFinite(value) ? Math.Clamp(value, min, max) : fallback;
}

public enum NavigationSection
{
    LocalChanges, History, Branches, Remotes, Tags, Stashes, Submodules
}

public sealed record AppSettings
{
    public const int CurrentSchemaVersion = 6;
    public int SchemaVersion { get; init; } = CurrentSchemaVersion;
    public double WindowWidth { get; init; } = 1200;
    public double WindowHeight { get; init; } = 800;
    public bool IsMaximized { get; init; }
    public double SidebarWidth { get; init; } = 210;
    public double DetailsHeight { get; init; } = 180;
    public double HistoryFilesWidth { get; init; } = 340;
    public CommitLayout CommitLayout { get; init; } = new();
    public NavigationSection SelectedSection { get; init; } = NavigationSection.History;
    public string[] RepositoryPaths { get; init; } = [];
    public RepositoryIdentityStamp[] RepositoryIdentities { get; init; } = [];
    public RepositoryPreference[] RepositoryPreferences { get; init; } = [];
    public string? SelectedRepositoryPath { get; init; }
    public bool CheckUpdatesPeriodically { get; init; } = true;
    public string UpdateChannel { get; init; } = "win-stable";
    public AppAppearance Appearance { get; init; } = AppAppearance.Dark;
    public string? RiderExecutable { get; init; }
    public string? CodeExecutable { get; init; }
    public string? BashExecutable { get; init; }

    public AppSettings Normalize() => this with
    {
        UpdateChannel = UpdateChannel == "win-beta" ? "win-beta" : "win-stable",
        WindowWidth = Clamp(WindowWidth, 1200, 900, 3840),
        WindowHeight = Clamp(WindowHeight, 800, 620, 2160),
        SidebarWidth = double.IsFinite(SidebarWidth) ? Math.Max(160, SidebarWidth) : 210,
        DetailsHeight = Clamp(DetailsHeight, 180, 80, 3000),
        HistoryFilesWidth = Clamp(HistoryFilesWidth, 340, 160, 3000),
        CommitLayout = (CommitLayout ?? new()).Normalize(),
        SelectedSection = Enum.IsDefined(SelectedSection) ? SelectedSection : NavigationSection.History,
        Appearance = Enum.IsDefined(Appearance) ? Appearance : AppAppearance.Dark,
        RiderExecutable = Executable(RiderExecutable), CodeExecutable = Executable(CodeExecutable), BashExecutable = Executable(BashExecutable),
        RepositoryIdentities = (RepositoryIdentities ?? []).Where(item => item is not null && !string.IsNullOrWhiteSpace(item.Path) && Path.IsPathFullyQualified(item.Path) &&
            item.Fingerprint is { Length: 64 } && item.Fingerprint.All(char.IsAsciiHexDigit)).DistinctBy(item => item.Path, StringComparer.OrdinalIgnoreCase).Take(20).ToArray(),
        RepositoryPreferences = (RepositoryPreferences ?? []).Where(item => item is not null && !string.IsNullOrWhiteSpace(item.Path) && Path.IsPathFullyQualified(item.Path) && Enum.IsDefined(item.Scope)).Take(20).ToArray(),
        RepositoryPaths = (RepositoryPaths ?? []).Where(path => !string.IsNullOrWhiteSpace(path) &&
            !path.Contains('\0') && Path.IsPathFullyQualified(path)).Distinct(StringComparer.OrdinalIgnoreCase).Take(20).ToArray()
    };

    private static string? Executable(string? value) => value is { Length: > 0 and <= 32767 } &&
        !value.Contains('\0') && Path.IsPathFullyQualified(value) && Path.GetExtension(value).Equals(".exe", StringComparison.OrdinalIgnoreCase) ? value : null;

    private static double Clamp(double value, double fallback, double minimum, double maximum) =>
        double.IsFinite(value) ? Math.Clamp(value, minimum, maximum) : fallback;
}
