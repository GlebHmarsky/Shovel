using GitGui.Core.Git;

namespace GitGui.Core.Settings;

public sealed record RepositoryPreference(string Path, HistoryScope Scope);
public enum AppAppearance { Dark, Light }

public enum NavigationSection
{
    LocalChanges, History, Branches, Remotes, Tags, Stashes, Submodules
}

public sealed record AppSettings
{
    public const int CurrentSchemaVersion = 5;
    public int SchemaVersion { get; init; } = CurrentSchemaVersion;
    public double WindowWidth { get; init; } = 1200;
    public double WindowHeight { get; init; } = 800;
    public bool IsMaximized { get; init; }
    public double SidebarWidth { get; init; } = 210;
    public double DetailsHeight { get; init; } = 180;
    public NavigationSection SelectedSection { get; init; } = NavigationSection.History;
    public string[] RepositoryPaths { get; init; } = [];
    public RepositoryIdentityStamp[] RepositoryIdentities { get; init; } = [];
    public RepositoryPreference[] RepositoryPreferences { get; init; } = [];
    public string? SelectedRepositoryPath { get; init; }
    public bool CheckUpdatesPeriodically { get; init; } = true;
    public AppAppearance Appearance { get; init; } = AppAppearance.Dark;
    public string? RiderExecutable { get; init; }
    public string? CodeExecutable { get; init; }
    public string? BashExecutable { get; init; }

    public AppSettings Normalize() => this with
    {
        WindowWidth = Clamp(WindowWidth, 1200, 900, 3840),
        WindowHeight = Clamp(WindowHeight, 800, 620, 2160),
        SidebarWidth = double.IsFinite(SidebarWidth) ? Math.Max(160, SidebarWidth) : 210,
        DetailsHeight = Clamp(DetailsHeight, 180, 80, 360),
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
