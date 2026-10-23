namespace GitGui.Core.Settings;

public enum NavigationSection
{
    LocalChanges, History, Branches, Remotes, Tags, Stashes, Submodules
}

public sealed record AppSettings
{
    public const int CurrentSchemaVersion = 1;
    public int SchemaVersion { get; init; } = CurrentSchemaVersion;
    public double WindowWidth { get; init; } = 1200;
    public double WindowHeight { get; init; } = 800;
    public bool IsMaximized { get; init; }
    public double SidebarWidth { get; init; } = 210;
    public double DetailsHeight { get; init; } = 240;
    public NavigationSection SelectedSection { get; init; } = NavigationSection.History;

    public AppSettings Normalize() => this with
    {
        WindowWidth = Clamp(WindowWidth, 1200, 900, 3840),
        WindowHeight = Clamp(WindowHeight, 800, 620, 2160),
        SidebarWidth = Clamp(SidebarWidth, 210, 160, 320),
        DetailsHeight = Clamp(DetailsHeight, 240, 140, 360),
        SelectedSection = Enum.IsDefined(SelectedSection) ? SelectedSection : NavigationSection.History
    };

    private static double Clamp(double value, double fallback, double minimum, double maximum) =>
        double.IsFinite(value) ? Math.Clamp(value, minimum, maximum) : fallback;
}
