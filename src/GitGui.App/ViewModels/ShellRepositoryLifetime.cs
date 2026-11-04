using GitGui.App.Localization;

namespace GitGui.App.ViewModels;

public sealed partial class ShellViewModel
{
    private readonly Dictionary<string, RepositoryTab> _retainedRepositories = new(StringComparer.OrdinalIgnoreCase);
    private IEnumerable<RepositoryTab> AllRepositoryOwners => Repositories.Concat(_retainedRepositories.Values).Distinct();
    private RepositoryTab RetainedRepository(string path) => _retainedRepositories.Remove(path, out var tab) ? tab : new(path);
    private Task _retiredEditors = Task.CompletedTask;
    private void PruneRetainedRepositories()
    {
        if (_stopping) return;
        foreach (var tab in _retainedRepositories.Values.OrderByDescending(tab => tab.History.LastUse).Skip(CachedTabLimit).ToArray())
        {
            if (!tab.InspectionTask.IsCompleted || RepositoryOperationBusy(tab) || HasQueuedAction(tab) ||
                _commitSessions.GetValueOrDefault(tab)?.HasPendingWork == true) continue;
            _retainedRepositories.Remove(tab.Directory); _knownIdentities.Remove(tab.Directory);
            if (_commitSessions.Remove(tab, out var editor)) _retiredEditors = Task.WhenAll(_retiredEditors, editor.CloseAsync());
            if (_repositoryActions.Remove(tab.Directory, out var action)) action.PropertyChanged -= OnActionChanged;
            tab.History.Clear(); tab.Changes.Clear(); tab.Stashes.List = null; tab.Sidebar = null; tab.RemoteConfiguration = null;
        }
    }
    private void CancelNextQueuedAction(RepositoryTab? tab)
    {
        if (_queuedActions.FirstOrDefault(item => item.Tab == tab && !item.Started) is { } item) CancelQueuedAction(item);
    }
    private void CancelQueuedAction(QueuedAction item)
    {
        if (item.Started || !_queuedActions.Remove(item)) return;
        item.Completion.TrySetResult();
        Journal.Record(new(item, item.Title, UiText.Get("QueuedRequestCancelled"), 0, OperationOutcome.Cancelled), item.Tab.Directory, false);
        Changed(nameof(QueueStatus)); Changed(nameof(ActionOpen)); RefreshCommands(); DrainActionQueue();
    }
    private void FailWaitingRequests(RepositoryTab tab)
    {
        foreach (var item in _queuedActions.Where(item => SameRepository(item.Tab, tab) && !item.Started).ToArray())
        {
            _queuedActions.Remove(item); item.Completion.TrySetResult();
            Journal.Record(new(item, item.Title, UiText.Get("QueueStoppedAfterFailure"), 0, OperationOutcome.Failed), item.Tab.Directory, false);
            Journal.Entries.FirstOrDefault(entry => ReferenceEquals(entry.Attempt, item))?.CancelWaiting?.Refresh();
        }
        Changed(nameof(QueueStatus)); RefreshCommands();
    }
}
