using System.Globalization;
using GitGui.App.Localization;
using GitGui.Core.Settings;

namespace GitGui.App.ViewModels;

public sealed class SettingsViewModel : ObservableObject
{
    private readonly bool _canSave;
    private readonly Func<bool, int, Task<bool>> _save;
    private bool _autoFetchEnabled, _saving;
    private string _intervalSeconds, _saveError = "";
    public bool AutoFetchEnabled { get => _autoFetchEnabled; set { if (Set(ref _autoFetchEnabled, value)) Refresh(); } }
    public string IntervalSeconds { get => _intervalSeconds; set { if (Set(ref _intervalSeconds, value)) Refresh(); } }
    public bool IsSaving => _saving;
    public bool CanEdit => !_saving;
    private bool ValidInterval => int.TryParse(IntervalSeconds, NumberStyles.Integer, CultureInfo.CurrentCulture, out var seconds) &&
        seconds >= AppSettings.MinimumAutoFetchIntervalSeconds && seconds <= AppSettings.MaximumAutoFetchIntervalSeconds;
    public string Error => !_canSave ? UiText.Get("SettingsReadOnly") : !ValidInterval ? UiText.Get("SettingsFetchIntervalInvalid") : _saveError;
    public string Status => _saving ? UiText.Get("SettingsSaving") : Error;
    public RelayCommand SaveCommand { get; }
    public Task SaveTask { get; private set; } = Task.CompletedTask;
    public event EventHandler? Saved;
    public SettingsViewModel(bool enabled, int intervalSeconds, bool canSave, Func<bool, int, Task<bool>> save)
    {
        _autoFetchEnabled = enabled; _intervalSeconds = intervalSeconds.ToString(CultureInfo.CurrentCulture);
        _canSave = canSave; _save = save;
        SaveCommand = new(() => SaveTask = SaveAsync(), () => _canSave && !_saving && ValidInterval);
    }
    private void Refresh()
    {
        _saveError = ""; SaveCommand.Refresh(); Changed(nameof(Error)); Changed(nameof(Status));
    }
    private async Task SaveAsync()
    {
        var enabled = AutoFetchEnabled;
        var interval = int.Parse(IntervalSeconds, CultureInfo.CurrentCulture);
        _saving = true; Refresh(); Changed(nameof(IsSaving)); Changed(nameof(CanEdit));
        var saved = false;
        try { saved = await _save(enabled, interval); }
        catch (Exception) { /* Keep edits and offer retry without exposing arbitrary exceptions. */ }
        finally
        {
            _saving = false; _saveError = saved ? "" : UiText.Get("SettingsSaveFailed");
            SaveCommand.Refresh(); Changed(nameof(IsSaving)); Changed(nameof(CanEdit)); Changed(nameof(Error)); Changed(nameof(Status));
        }
        if (saved) Saved?.Invoke(this, EventArgs.Empty);
    }
}
