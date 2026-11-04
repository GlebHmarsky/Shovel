using System.Windows.Threading;
using GitGui.Core.Git;
using GitGui.Core.Settings;

namespace GitGui.App.ViewModels;

public sealed partial class ShellViewModel
{
    // Called on the dispatcher, but all inspection work remains asynchronous. Never refresh
    // an inactive tab through the selected-view read pipeline: it owns a different generation.
    private async Task ReconcileActionAsync(RepositoryTab tab, bool refreshChanges = true)
    {
        foreach (var related in Repositories.Where(item => item == tab ||
            string.Equals(item.Snapshot?.CommonDirectory, tab.Snapshot?.CommonDirectory, StringComparison.OrdinalIgnoreCase)))
        {
            related.History.ValidatedAt = default; related.History.NeedsRefresh = true;
            if (refreshChanges) related.Changes.ReadAt = default;
            related.SidebarReadAt = default; related.RemoteConfiguration = null;
        }
        if (SelectedRepository == tab && !_stopping)
        {
            await RefreshSelectedAsync(tab);
            if (SelectedRepository == tab) return;
        }
        using var timeout = new CancellationTokenSource(TimeSpan.FromSeconds(45));
        var identity = await _identities.ReadAsync(tab.Directory, timeout.Token);
        if (!_knownIdentities.TryGetValue(tab.Directory, out var known) || identity.Fingerprint != known.Fingerprint)
            throw new GitActionException("ActionIdentityChanged");
        var snapshot = await _reader!.InspectAsync(_git!, tab.Directory, timeout.Token, includeHistory: false);
        var current = await _identities.ReadAsync(tab.Directory, timeout.Token);
        if (current.Fingerprint != identity.Fingerprint) throw new GitActionException("ActionIdentityChanged");
        tab.Apply(snapshot);
        if (!_stopping && SelectedRepository is { } relatedTab &&
            string.Equals(relatedTab.Snapshot?.CommonDirectory, snapshot.CommonDirectory, StringComparison.OrdinalIgnoreCase))
            await RefreshSelectedAsync(relatedTab);

        async Task RefreshSelectedAsync(RepositoryTab viewTab)
        {
            BeginInspection(viewTab, force: true, actionReconcile: true, checkChanges: refreshChanges);
            var inspection = _activeOperation;
            await inspection;
            // Switching tabs cancels only this view read, not the completed Git operation.
            if (SelectedRepository == viewTab && (viewTab.History.ValidatedAt == default || viewTab.History.Error.Length != 0))
                throw new System.IO.IOException();
        }
    }

    private readonly HashSet<GitActionViewModel> _reportedOperations = [];
    private void ReportOperationProblem(GitActionViewModel operation)
    {
        if (!_reportedOperations.Add(operation)) return;
        // Bound retained attempt identities to the current per-repository results.
        _reportedOperations.RemoveWhere(item => item != _lastAction && !_repositoryActions.ContainsValue(item));
        if (!operation.NeedsAttention) return;
        if (operation.IsBackground) return;
        _interaction?.ShowOperationProblem(operation.CurrentRepository, operation.Title, operation.Status, operation.RawOutput);
    }

    private void PreemptBackgroundFetch(RepositoryTab tab)
    {
        foreach (var owner in _fetchOwners.Where(owner => SameRepository(owner, tab)).ToArray())
            if (_repositoryActions.GetValueOrDefault(owner.Directory) is { IsBackground: true } action) action.Cancel();
    }

    private Task ReconcileFetchAsync(RepositoryTab tab)
    {
        // Remote refs do not invalidate the worktree/index preview. Let user work
        // own the next refresh instead of starting a blocking read behind it.
        foreach (var related in Repositories.Where(item => SameRepository(item, tab)))
        { related.History.NeedsRefresh = true; related.SidebarReadAt = default; }
        if (_commitOpen || HistoryFileBusy(tab) || _queuedActions.Any(item => !item.Started && SameRepository(item.Tab, tab))) return Task.CompletedTask;
        return ReconcileActionAsync(tab, refreshChanges: false);
    }

    public bool HasUserOperation => HasHistoryFileAction || _queuedActions.Count != 0 || _actionOpen || _commitOpen || _operationOwners.Any(tab =>
        _repositoryActions.GetValueOrDefault(tab.Directory)?.IsBackground != true);
    public void RevealRunningOperation()
    {
        var owner = _operationOwners.FirstOrDefault(tab => _repositoryActions.GetValueOrDefault(tab.Directory)?.IsBackground != true) ?? _historyFileActions.Values.FirstOrDefault() ?? _initializationOwner ?? _queuedActions.FirstOrDefault()?.Tab;
        if (owner is not null && Repositories.Contains(owner)) SelectedRepository = owner;
        ShowActionDetailsCommand.Execute(null);
    }
    public async Task StopAutoFetchAsync(bool stopTimer = true)
    {
        if (stopTimer) _autoFetchTimer?.Stop();
        var work = _autoFetchWork.ToArray();
        var active = _repositoryActions.Values.Where(action => action.IsBackground && action.IsRunning).ToArray();
        foreach (var action in active) action.Cancel();
        // Include the shell ownership/reconciliation lifetime, not just the Git model.
        await Task.WhenAll(work.Concat(active.Select(action => action.RunningTask)));
    }

    public static readonly TimeSpan AutoFetchInterval = TimeSpan.FromSeconds(AppSettings.DefaultAutoFetchIntervalSeconds);
    private readonly Dictionary<RepositoryTab, DateTimeOffset> _nextAutoFetch = [];
    private readonly HashSet<Task> _autoFetchWork = [];
    private DispatcherTimer? _autoFetchTimer;
    public void StartAutoFetch()
    {
        if (_autoFetchTimer is null)
        {
            _autoFetchTimer = new DispatcherTimer();
            _autoFetchTimer.Tick += async (_, _) => await AutoFetchDueAsync(DateTimeOffset.UtcNow);
        }
        _autoFetchTimer.Interval = TimeSpan.FromSeconds(AutoFetchIntervalSeconds);
        if (AutoFetchEnabled && !_stopping) _autoFetchTimer.Start();
        else _autoFetchTimer.Stop();
    }
    public Task AutoFetchDueAsync(DateTimeOffset now)
    {
        if (!AutoFetchEnabled || now < _autoFetchNotBefore || !IsReady || _stopping || _commitOpen || _actionOpen || Updates?.Busy == true || _git is null || _actions is null) return Task.CompletedTask;
        foreach (var old in _nextAutoFetch.Keys.Where(tab => !Repositories.Contains(tab)).ToArray()) _nextAutoFetch.Remove(old);
        var work = new List<Task>();
        var slots = 2 - _repositoryActions.Values.Count(action => action.IsBackground && action.IsRunning);
        foreach (var tab in Repositories.OrderBy(tab => _nextAutoFetch.GetValueOrDefault(tab)).ToArray())
        {
            if (slots <= 0) break;
            if (RepositoryOperationBusy(tab) || _queuedActions.Any(item => SameRepository(item.Tab, tab)) || tab.History.StartupPreview ||
                tab.Snapshot is not { RemoteNames.Count: > 0, Operation: RepositoryOperation.None } ||
                !_knownIdentities.ContainsKey(tab.Directory) || now < _nextAutoFetch.GetValueOrDefault(tab) ||
                SelectedRepository == tab && IsBusy) continue;
            slots--; _nextAutoFetch[tab] = now + TimeSpan.FromSeconds(AutoFetchIntervalSeconds);
            var task = OpenActionAsync(GitAction.Fetch, null, directNetwork: true, targetTab: tab, background: true);
            _autoFetchWork.Add(task);
            work.Add(ObserveAutoFetchAsync(task));
        }
        return Task.WhenAll(work);
    }
    private async Task ObserveAutoFetchAsync(Task task)
    {
        try { await task; }
        finally { _autoFetchWork.Remove(task); }
    }
}
