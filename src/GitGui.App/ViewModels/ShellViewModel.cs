using GitGui.App.Localization;
using GitGui.Core.Settings;

namespace GitGui.App.ViewModels;

public sealed record NavigationItem(NavigationSection Section, string Label);

public sealed class ShellViewModel : ObservableObject
{
    public IReadOnlyList<NavigationItem> Sections { get; } = Enum.GetValues<NavigationSection>()
        .Select(section => new NavigationItem(section, UiText.Get(section.ToString()))).ToArray();
    public string Version => UiText.Format("VersionFormat", typeof(ShellViewModel).Assembly.GetName().Version?.ToString(3) ?? "0.1.0");
    private bool _isReady;
    public bool IsReady { get => _isReady; private set => Set(ref _isReady, value); }
    private string _status = UiText.Get("LoadingSettings");
    public string Status { get => _status; private set => Set(ref _status, value); }
    private NavigationItem _selectedSection;
    public NavigationItem SelectedSection
    {
        get => _selectedSection;
        set { if (value is not null && Set(ref _selectedSection, value)) Changed(nameof(AreaTitle)); }
    }
    public string AreaTitle => SelectedSection.Label;
    public SettingsLoadResult? LoadedSettings { get; private set; }

    public ShellViewModel() => _selectedSection = Sections.Single(item => item.Section == NavigationSection.History);

    public void CompleteStartup(SettingsLoadResult result)
    {
        LoadedSettings = result;
        SelectedSection = Sections.Single(item => item.Section == result.Settings.SelectedSection);
        IsReady = true;
        Status = UiText.Get(result.Status switch
        {
            SettingsLoadStatus.Defaults or SettingsLoadStatus.Loaded => "FoundationReady",
            SettingsLoadStatus.NewerSchema => "SettingsNewer",
            SettingsLoadStatus.UnsupportedSchema => "SettingsUnsupported",
            SettingsLoadStatus.Invalid => "SettingsInvalid",
            _ => "SettingsUnavailable"
        });
    }

    public void BeginShutdown() { IsReady = false; Status = UiText.Get("SavingSettings"); }
    public void ResumeAfterSaveFailure() { IsReady = true; Status = UiText.Get("SettingsSaveFailedStatus"); }
}
