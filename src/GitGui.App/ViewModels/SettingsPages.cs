using GitGui.App.Localization;
using GitGui.Core.Settings;

namespace GitGui.App.ViewModels;

public sealed partial class SettingsViewModel
{
    public sealed record Choice<T>(T Value, string Label);
    public IReadOnlyList<Choice<int>> Categories { get; } = new[] { "Appearance", "History", "Diff", "Git", "Identity", "Tools", "Updates" }
        .Select((key, index) => new Choice<int>(index, UiText.Get("SettingsPage" + key))).ToArray();
    public IReadOnlyList<Choice<AppAppearance>> Themes { get; } = Enum.GetValues<AppAppearance>().Select(value => new Choice<AppAppearance>(value, UiText.Get("Workspace" + value))).ToArray();
    public IReadOnlyList<Choice<AvatarProvider>> AvatarProviders { get; } = Enum.GetValues<AvatarProvider>().Select(value => new Choice<AvatarProvider>(value, UiText.Get("SettingsAvatar" + value))).ToArray();
    public IReadOnlyList<int> ScaleChoices { get; } = Enumerable.Range(7, 14).Select(value => value * 10).ToArray();
    public IReadOnlyList<int> FontSizes { get; } = Enumerable.Range(9, 16).ToArray();
    public IReadOnlyList<Choice<string>> Channels { get; } = [new("win-stable", UiText.Get("UpdateStable")), new("win-beta", UiText.Get("UpdateBeta"))];
    private int _category, _scale = 100, _fontSize = 12;
    private bool _sideBySide, _periodicUpdates = true;
    private AppAppearance _appearance;
    private AvatarProvider _avatarProvider;
    private string _server = "", _rider = "", _code = "", _bash = "", _channel = "win-stable";
    public GlobalIdentityViewModel Identity { get; private set; } = new();
    public int SelectedCategory { get => _category; set { if (Set(ref _category, value)) foreach (var name in new[] { nameof(IsAppearance), nameof(IsHistory), nameof(IsDiff), nameof(IsGit), nameof(IsIdentity), nameof(IsTools), nameof(IsUpdates) }) Changed(name); } }
    public bool IsAppearance => SelectedCategory == 0;
    public bool IsHistory => SelectedCategory == 1;
    public bool IsDiff => SelectedCategory == 2;
    public bool IsGit => SelectedCategory == 3;
    public bool IsIdentity => SelectedCategory == 4;
    public bool IsTools => SelectedCategory == 5;
    public bool IsUpdates => SelectedCategory == 6;
    public AppAppearance Appearance { get => _appearance; set { if (Set(ref _appearance, value)) Refresh(); } }
    public int ScalePercent { get => _scale; set { if (Set(ref _scale, value)) Refresh(); } }
    public int DiffFontSize { get => _fontSize; set { if (Set(ref _fontSize, value)) Refresh(); } }
    public bool DiffSideBySide { get => _sideBySide; set { if (Set(ref _sideBySide, value)) Refresh(); } }
    public AvatarProvider AvatarProvider { get => _avatarProvider; set { if (Set(ref _avatarProvider, value)) Refresh(); } }
    public string GiteaServer { get => _server; set { if (Set(ref _server, value)) Refresh(); } }
    public string RiderExecutable { get => _rider; set { if (Set(ref _rider, value)) Refresh(); } }
    public string CodeExecutable { get => _code; set { if (Set(ref _code, value)) Refresh(); } }
    public string BashExecutable { get => _bash; set { if (Set(ref _bash, value)) Refresh(); } }
    public string UpdateChannel { get => _channel; set { if (Set(ref _channel, value)) Refresh(); } }
    public bool PeriodicUpdates { get => _periodicUpdates; set { if (Set(ref _periodicUpdates, value)) Refresh(); } }
    private AvatarSettings AvatarSettings => new() { Provider = AvatarProvider, GiteaServer = GiteaServer.Trim() };
    private string? AdditionalError => AvatarSettings.ValidationCode is { } code ? UiText.Get(code) :
        ScalePercent is < 70 or > 200 || ScalePercent % 10 != 0 || DiffFontSize is < 9 or > 24 ? UiText.Get("SettingsDisplayInvalid") :
        new[] { RiderExecutable, CodeExecutable, BashExecutable }.Any(value => value.Length > 0 && (!System.IO.Path.IsPathFullyQualified(value) || value.Contains('\0') || !value.EndsWith(".exe", StringComparison.OrdinalIgnoreCase))) ? UiText.Get("SettingsToolPathInvalid") : null;
    public AppSettings EditedSettings => _snapshot with
    {
        Appearance = Appearance, UiScale = ScalePercent / 100d, DiffFontSize = DiffFontSize, DiffSideBySide = DiffSideBySide,
        Avatars = AvatarSettings, RiderExecutable = RiderExecutable.Length == 0 ? null : RiderExecutable,
        CodeExecutable = CodeExecutable.Length == 0 ? null : CodeExecutable, BashExecutable = BashExecutable.Length == 0 ? null : BashExecutable,
        CheckUpdatesPeriodically = PeriodicUpdates, UpdateChannel = UpdateChannel
    };
    public SettingsViewModel(AppSettings settings, bool canSave, Func<AppSettings, Task<bool>> save, GlobalIdentityViewModel? identity = null)
        : this(settings.AutoFetchEnabled, settings.AutoFetchIntervalSeconds, canSave, settings.ExternalMergeTool, settings.HistoryRefsOnRight,
            (_, _, _, _) => Task.FromResult(false))
    {
        _snapshot = settings; _save = save; Identity = identity ?? new();
        _appearance = settings.Appearance; _scale = (int)Math.Round(settings.UiScale * 100); _fontSize = (int)settings.DiffFontSize;
        _sideBySide = settings.DiffSideBySide; _avatarProvider = settings.Avatars.Provider; _server = settings.Avatars.GiteaServer;
        _rider = settings.RiderExecutable ?? ""; _code = settings.CodeExecutable ?? ""; _bash = settings.BashExecutable ?? "";
        _channel = settings.UpdateChannel; _periodicUpdates = settings.CheckUpdatesPeriodically;
        Identity.PropertyChanged += (_, args) => { if (args.PropertyName == nameof(GlobalIdentityViewModel.Busy)) SaveCommand.Refresh(); };
    }
}
