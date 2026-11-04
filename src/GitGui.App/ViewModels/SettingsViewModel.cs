using System.Globalization;
using GitGui.App.Localization;
using GitGui.Core.Settings;
using GitGui.Core.Git;

namespace GitGui.App.ViewModels;

public sealed class SettingsViewModel : ObservableObject
{
    private readonly bool _canSave;
    public sealed record ModeOption(ExternalMergeToolMode Value, string Label);
    public IReadOnlyList<ModeOption> Modes { get; } = Enum.GetValues<ExternalMergeToolMode>()
        .Select(mode => new ModeOption(mode, UiText.Get("SettingsMergeMode" + mode))).ToArray();
    private readonly Func<bool, int, ExternalMergeToolSettings, Task<bool>> _save;
    private ExternalMergeToolMode _mergeToolMode;
    private string _mergeToolName, _mergeToolExecutablePath, _mergeToolCommand;
    public ExternalMergeToolMode MergeToolMode { get => _mergeToolMode; set { if (Set(ref _mergeToolMode, value)) { Refresh(); Changed(nameof(IsNamedTool)); Changed(nameof(IsCustomCommand)); } } }
    public string MergeToolName { get => _mergeToolName; set { if (Set(ref _mergeToolName, value ?? "")) Refresh(); } }
    public string MergeToolExecutablePath { get => _mergeToolExecutablePath; set { if (Set(ref _mergeToolExecutablePath, value ?? "")) Refresh(); } }
    public string MergeToolCommand { get => _mergeToolCommand; set { if (Set(ref _mergeToolCommand, value ?? "")) Refresh(); } }
    public bool IsNamedTool => MergeToolMode == ExternalMergeToolMode.NamedTool;
    public bool IsCustomCommand => MergeToolMode == ExternalMergeToolMode.CustomCommand;
    public ExternalMergeToolSettings MergeToolSettings => new()
    { Mode = MergeToolMode, ToolName = MergeToolName, ExecutablePath = MergeToolExecutablePath, Command = MergeToolCommand };
    private bool _autoFetchEnabled, _saving;
    private string _intervalSeconds, _saveError = "";
    public bool AutoFetchEnabled { get => _autoFetchEnabled; set { if (Set(ref _autoFetchEnabled, value)) Refresh(); } }
    public string IntervalSeconds { get => _intervalSeconds; set { if (Set(ref _intervalSeconds, value)) Refresh(); } }
    public bool IsSaving => _saving;
    public bool CanEdit => !_saving;
    private bool ValidInterval => int.TryParse(IntervalSeconds, NumberStyles.Integer, CultureInfo.CurrentCulture, out var seconds) &&
        seconds >= AppSettings.MinimumAutoFetchIntervalSeconds && seconds <= AppSettings.MaximumAutoFetchIntervalSeconds;
    public string Error => !_canSave ? UiText.Get("SettingsReadOnly") : !ValidInterval ? UiText.Get("SettingsFetchIntervalInvalid") :
        MergeToolSettings.ValidationCode is { } code ? UiText.Get(code) : _saveError;
    public string Status => _saving ? UiText.Get("SettingsSaving") : Error;
    public RelayCommand SaveCommand { get; }
    public Task SaveTask { get; private set; } = Task.CompletedTask;
    public event EventHandler? Saved;
    public SettingsViewModel(bool enabled, int intervalSeconds, bool canSave, Func<bool, int, Task<bool>> save)
        : this(enabled, intervalSeconds, canSave, new(), (enabledValue, seconds, _) => save(enabledValue, seconds)) { }

    public SettingsViewModel(bool enabled, int intervalSeconds, bool canSave, ExternalMergeToolSettings mergeTool,
        Func<bool, int, ExternalMergeToolSettings, Task<bool>> save)
    {
        var tool = mergeTool.Normalize();
        _mergeToolMode = tool.Mode; _mergeToolName = tool.ToolName;
        _mergeToolExecutablePath = tool.ExecutablePath; _mergeToolCommand = tool.Command;
        _autoFetchEnabled = enabled; _intervalSeconds = intervalSeconds.ToString(CultureInfo.CurrentCulture);
        _canSave = canSave; _save = save;
        SaveCommand = new(() => SaveTask = SaveAsync(), () => _canSave && !_saving && ValidInterval && MergeToolSettings.ValidationCode is null);
    }
    private void Refresh()
    {
        _saveError = ""; SaveCommand.Refresh(); Changed(nameof(Error)); Changed(nameof(Status));
    }
    private async Task SaveAsync()
    {
        var enabled = AutoFetchEnabled;
        var interval = int.Parse(IntervalSeconds, CultureInfo.CurrentCulture);
        var tool = MergeToolSettings;
        _saving = true; Refresh(); Changed(nameof(IsSaving)); Changed(nameof(CanEdit));
        var saved = false;
        try { saved = await _save(enabled, interval, tool); }
        catch (Exception) { /* Keep edits and offer retry without exposing arbitrary exceptions. */ }
        finally
        {
            _saving = false; _saveError = saved ? "" : UiText.Get("SettingsSaveFailed");
            SaveCommand.Refresh(); Changed(nameof(IsSaving)); Changed(nameof(CanEdit)); Changed(nameof(Error)); Changed(nameof(Status));
        }
        if (saved) Saved?.Invoke(this, EventArgs.Empty);
    }
}
