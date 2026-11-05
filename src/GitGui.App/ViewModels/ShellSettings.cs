using GitGui.Core.Git;
using GitGui.App.Services;
using GitGui.Core.Settings;

namespace GitGui.App.ViewModels;

public sealed partial class ShellViewModel
{
    private readonly SemaphoreSlim _preferenceWrites = new(1, 1);
    private AppSettings? _pendingSettings;
    private string _savedUpdateChannel = "win-stable";
    private bool _savedPeriodicUpdates = true;
    public WindowLayouts WindowLayouts { get; } = new();
    public AvatarService AvatarImages { get; } = new();
    public AvatarSettings AvatarPreferences { get; private set; } = new();
    public string AvatarRemote => SelectedRepository?.RemoteConfiguration?.Remotes.OrderBy(remote => remote.Name == "origin" ? 0 : 1).FirstOrDefault()?.Location ?? "";
    public GlobalIdentityViewModel CreateIdentitySettings()
    {
        var git = _git; var actions = _actions;
        if (git is null || actions is null) return new();
        var directory = Environment.GetFolderPath(Environment.SpecialFolder.UserProfile);
        return new(token => actions.ReadGlobalIdentityAsync(git, directory, token),
            (reviewed, name, email, token) => actions.WriteGlobalIdentityAsync(git, directory, reviewed, name, email, token));
    }
    public AppSettings Preferences => CaptureSession(LoadedSettings?.Settings ?? new());
    public bool HistoryRefsOnRight { get; private set; }
    public ExternalMergeToolSettings ExternalMergeTool { get; private set; } = new();
    public Task<bool> SaveAutoFetchSettingsAsync(bool enabled, int intervalSeconds) => SaveSettingsAsync(enabled, intervalSeconds, ExternalMergeTool);

    public Task<bool> SaveSettingsAsync(bool enabled, int intervalSeconds, ExternalMergeToolSettings tool)
        => SaveSettingsAsync(enabled, intervalSeconds, tool, HistoryRefsOnRight);

    public Task<bool> SaveSettingsAsync(bool enabled, int intervalSeconds, ExternalMergeToolSettings tool, bool refsOnRight)
        => SaveSettingsAsync(Preferences with { AutoFetchEnabled = enabled, AutoFetchIntervalSeconds = intervalSeconds, ExternalMergeTool = tool, HistoryRefsOnRight = refsOnRight });

    public Task<bool> SaveSettingsAsync(AppSettings settings)
    {
        if (settings.ExternalMergeTool?.ValidationCode is not null || settings.Avatars?.ValidationCode is not null ||
            _pendingSettings is not null || !IsReady || _stopping || LoadedSettings?.CanSave != true ||
            settings.AutoFetchIntervalSeconds < AppSettings.MinimumAutoFetchIntervalSeconds || settings.AutoFetchIntervalSeconds > AppSettings.MaximumAutoFetchIntervalSeconds)
            return Task.FromResult(false);
        Task drain = Task.CompletedTask;
        var task = SaveAsync(); PreferenceTask = FinishAsync(); return task;
        async Task FinishAsync() { await task; await drain; }
        async Task<bool> SaveAsync()
        {
            await _preferenceWrites.WaitAsync();
            _pendingSettings = settings.Normalize();
            try
            {
                if (_persist is not null) await _persist(CaptureSession(LoadedSettings.Settings));
                var saved = _pendingSettings;
                ExternalMergeTool = saved.ExternalMergeTool; Changed(nameof(ExternalMergeTool));
                HistoryRefsOnRight = saved.HistoryRefsOnRight; Changed(nameof(HistoryRefsOnRight));
                if (AvatarPreferences != saved.Avatars) AvatarImages.Reset();
                AvatarPreferences = saved.Avatars; Changed(nameof(AvatarPreferences));
                DiffPreferences.Apply(saved); if (CommitFiles is { } files) files.SideBySide = saved.DiffSideBySide;
                UiScale.Set(saved.UiScale); SetAppearance(saved.Appearance);
                _riderExecutable = saved.RiderExecutable; _codeExecutable = saved.CodeExecutable; _bashExecutable = saved.BashExecutable;
                if (Updates is { } updates) await updates.ApplyPreferencesAsync(saved.CheckUpdatesPeriodically, saved.UpdateChannel);
                _savedUpdateChannel = saved.UpdateChannel; _savedPeriodicUpdates = saved.CheckUpdatesPeriodically;
                drain = ConfigureAutoFetchAsync(saved.AutoFetchEnabled, saved.AutoFetchIntervalSeconds);
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
