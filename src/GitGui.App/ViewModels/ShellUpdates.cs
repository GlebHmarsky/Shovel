using GitGui.App.Services;
using GitGui.Core.Git;

namespace GitGui.App.ViewModels;

public sealed partial class ShellViewModel
{
    public UpdateViewModel? Updates { get; private set; }
    private bool _unverifiedToolHandoff;
    public void ConfigureUpdates(UpdateViewModel updates)
    {
        Updates = updates;
        updates.PropertyChanged += (_, e) =>
        { if (e.PropertyName is nameof(UpdateViewModel.PeriodicChecks) or nameof(UpdateViewModel.SelectedChannel) && IsReady && !_stopping) PreferenceTask = SavePreferencesAsync(); };
        Changed(nameof(Updates));
    }
    public bool ToolsNeedUpdateAcknowledgment => _unverifiedToolHandoff;
    public bool CanPrepareUpdate => IsReady && !_stopping && !ActionOpen && !_launchingTool;
    public async Task<bool> VerifyUpdateIdleAsync(CancellationToken token)
    {
        // Called only after BeginShutdown closes admission to commands/tools/tab changes.
        if (!_stopping || ActionOpen || _launchingTool || !WorkspaceToolTask.IsCompleted || !PreferenceTask.IsCompleted) return false;
        if (LoadedSettings?.CanSave != true) return false;
        foreach (var tab in Repositories)
        {
            if (_reader is null || _git is null || !_knownIdentities.TryGetValue(tab.Directory, out var expected)) return false;
            var identity = await _identities.ReadAsync(tab.Directory, token);
            if (identity.Fingerprint != expected.Fingerprint) return false;
            var snapshot = await _reader.InspectAsync(_git, tab.Directory, token, includeHistory: false);
            if (snapshot.Operation != RepositoryOperation.None || snapshot.Recovery is not null ||
                await _reader.HasUnmergedFilesAsync(_git, tab.Directory, token)) return false;
        }
        return true;
    }
}
