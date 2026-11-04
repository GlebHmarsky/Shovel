using GitGui.Core.Git;
using GitGui.Core.Settings;

namespace GitGui.App.ViewModels;

public sealed partial class ShellViewModel
{
    private readonly SemaphoreSlim _preferenceWrites = new(1, 1);
    private (bool Enabled, int Seconds, ExternalMergeToolSettings Tool, bool RefsOnRight)? _pendingSettings;
    public bool HistoryRefsOnRight { get; private set; }
    public ExternalMergeToolSettings ExternalMergeTool { get; private set; } = new();
    public Task<bool> SaveAutoFetchSettingsAsync(bool enabled, int intervalSeconds) => SaveSettingsAsync(enabled, intervalSeconds, ExternalMergeTool);

    public Task<bool> SaveSettingsAsync(bool enabled, int intervalSeconds, ExternalMergeToolSettings tool)
        => SaveSettingsAsync(enabled, intervalSeconds, tool, HistoryRefsOnRight);

    public Task<bool> SaveSettingsAsync(bool enabled, int intervalSeconds, ExternalMergeToolSettings tool, bool refsOnRight)
    {
        if (tool is null || tool.ValidationCode is not null || _pendingSettings is not null || !IsReady || _stopping || LoadedSettings?.CanSave != true ||
            intervalSeconds < AppSettings.MinimumAutoFetchIntervalSeconds || intervalSeconds > AppSettings.MaximumAutoFetchIntervalSeconds)
            return Task.FromResult(false);
        Task drain = Task.CompletedTask;
        var task = SaveAsync(); PreferenceTask = FinishAsync(); return task;
        async Task FinishAsync() { await task; await drain; }
        async Task<bool> SaveAsync()
        {
            await _preferenceWrites.WaitAsync();
            _pendingSettings = (enabled, intervalSeconds, tool.Normalize(), refsOnRight);
            try
            {
                if (_persist is not null) await _persist(CaptureSession(LoadedSettings.Settings));
                // Configure applies admission and requests cancellation synchronously. Keep
                // the drain in PreferenceTask without making the settings dialog wait on Git.
                ExternalMergeTool = _pendingSettings.Value.Tool;
                Changed(nameof(ExternalMergeTool));
                HistoryRefsOnRight = refsOnRight; Changed(nameof(HistoryRefsOnRight));
                drain = ConfigureAutoFetchAsync(enabled, intervalSeconds);
                return true;
            }
            catch (Exception) { return false; }
            finally { _pendingSettings = null; _preferenceWrites.Release(); }
        }
    }
    private async Task PersistCurrentPreferencesAsync()
    {
        await _preferenceWrites.WaitAsync();
        try { if (_persist is not null) await _persist(CaptureSession(LoadedSettings?.Settings ?? new())); }
        finally { _preferenceWrites.Release(); }
    }

}
