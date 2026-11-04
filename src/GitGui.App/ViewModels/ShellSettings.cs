using GitGui.Core.Settings;

namespace GitGui.App.ViewModels;

public sealed partial class ShellViewModel
{
    private readonly SemaphoreSlim _preferenceWrites = new(1, 1);
    private (bool Enabled, int Seconds)? _pendingAutoFetchSettings;
    public Task<bool> SaveAutoFetchSettingsAsync(bool enabled, int intervalSeconds)
    {
        if (_pendingAutoFetchSettings is not null || !IsReady || _stopping || LoadedSettings?.CanSave != true ||
            intervalSeconds < AppSettings.MinimumAutoFetchIntervalSeconds || intervalSeconds > AppSettings.MaximumAutoFetchIntervalSeconds)
            return Task.FromResult(false);
        Task drain = Task.CompletedTask;
        var task = SaveAsync(); PreferenceTask = FinishAsync(); return task;
        async Task FinishAsync() { await task; await drain; }
        async Task<bool> SaveAsync()
        {
            await _preferenceWrites.WaitAsync();
            _pendingAutoFetchSettings = (enabled, intervalSeconds);
            try
            {
                if (_persist is not null) await _persist(CaptureSession(LoadedSettings.Settings));
                // Configure applies admission and requests cancellation synchronously. Keep
                // the drain in PreferenceTask without making the settings dialog wait on Git.
                drain = ConfigureAutoFetchAsync(enabled, intervalSeconds);
                return true;
            }
            catch (Exception) { return false; }
            finally { _pendingAutoFetchSettings = null; _preferenceWrites.Release(); }
        }
    }
    private async Task PersistCurrentPreferencesAsync()
    {
        await _preferenceWrites.WaitAsync();
        try { if (_persist is not null) await _persist(CaptureSession(LoadedSettings?.Settings ?? new())); }
        finally { _preferenceWrites.Release(); }
    }

}
