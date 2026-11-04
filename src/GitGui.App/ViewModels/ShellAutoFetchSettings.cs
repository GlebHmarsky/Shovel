using GitGui.Core.Settings;

namespace GitGui.App.ViewModels;

public sealed partial class ShellViewModel
{
    public bool AutoFetchEnabled { get; private set; } = true;
    public int AutoFetchIntervalSeconds { get; private set; } = AppSettings.DefaultAutoFetchIntervalSeconds;
    private DateTimeOffset _autoFetchNotBefore;

    private void RestoreAutoFetchSettings(AppSettings settings)
    {
        AutoFetchEnabled = settings.AutoFetchEnabled;
        AutoFetchIntervalSeconds = Math.Clamp(settings.AutoFetchIntervalSeconds,
            AppSettings.MinimumAutoFetchIntervalSeconds, AppSettings.MaximumAutoFetchIntervalSeconds);
    }

    // Persistence belongs to the caller: failed settings saves must not change the scheduler.
    public async Task ConfigureAutoFetchAsync(bool enabled, int seconds)
    {
        if (seconds is < AppSettings.MinimumAutoFetchIntervalSeconds or > AppSettings.MaximumAutoFetchIntervalSeconds)
            throw new ArgumentOutOfRangeException(nameof(seconds));
        if (AutoFetchEnabled == enabled && AutoFetchIntervalSeconds == seconds) return;

        AutoFetchEnabled = enabled;
        AutoFetchIntervalSeconds = seconds;
        _autoFetchNotBefore = DateTimeOffset.UtcNow + TimeSpan.FromSeconds(seconds);
        _nextAutoFetch.Clear();
        foreach (var tab in Repositories) _nextAutoFetch[tab] = _autoFetchNotBefore;
        Changed(nameof(AutoFetchEnabled)); Changed(nameof(AutoFetchIntervalSeconds));

        // Settings never start a timer in a headless model; production startup owns that.
        if (_autoFetchTimer is not null)
        {
            _autoFetchTimer.Stop();
            _autoFetchTimer.Interval = TimeSpan.FromSeconds(seconds);
            if (enabled && !_stopping) _autoFetchTimer.Start();
        }
        if (!enabled) await StopAutoFetchAsync();
    }
}
