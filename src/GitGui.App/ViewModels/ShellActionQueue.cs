using GitGui.App.Localization;
using GitGui.Core.Git;
using System.Windows.Threading;

namespace GitGui.App.ViewModels;

public sealed partial class ShellViewModel
{
    private sealed record QueuedAction(RepositoryTab Tab, string Key, string Title, Func<Task> Run,
        string? Identity, string? Branch, bool IsFetch, bool WorkingCopy, bool Independent)
    {
        public TaskCompletionSource Completion { get; } = new(TaskCreationOptions.RunContinuationsAsynchronously);
        public bool Running { get; set; }
        public bool Started { get; set; }
    }
    private readonly List<QueuedAction> _queuedActions = [];
    private bool HasQueuedAction(RepositoryTab? tab) => _queuedActions.Any(item => item.Tab == tab);
    private bool CanRequestAction => CanNavigate && SelectedRepository is not null && _reader is not null && _actions is not null && _interaction is not null;
    public string QueueStatus => SelectedRepository is { } tab && _queuedActions.FirstOrDefault(item => item.Tab == tab && !item.Running) is { } next
        ? UiText.Format("ActionQueued", next.Title) : "";

    private void QueueRefresh(RepositoryTab tab) => ActionTask = QueueAction(tab, "Refresh", UiText.Get("RefreshRepository"), async () =>
    {
        if (SelectedRepository == tab) { BeginCurrentRead(tab, force: true); await _activeOperation; }
        else await ReconcileActionAsync(tab);
    }, independent: true);

    private Task QueueSelectedAction(string key, string title, Func<RepositoryTab, Task> run)
    {
        var tab = SelectedRepository!;
        return QueueAction(tab, key, title, () => run(tab));
    }

    private Task QueueAction(RepositoryTab tab, string key, string title, Func<Task> run, bool isFetch = false, bool workingCopy = false, bool independent = false)
    {
        var existing = _queuedActions.FirstOrDefault(item => item.Tab == tab && item.Key == key);
        if (existing is not null) return existing.Completion.Task;
        var request = new QueuedAction(tab, key, title, run, _knownIdentities.GetValueOrDefault(tab.Directory)?.Fingerprint, tab.Snapshot?.Branch, isFetch, workingCopy, independent);
        _queuedActions.Add(request);
        if (key != "Commit") PreemptBackgroundFetch(tab);
        Journal.Record(new(request, title, UiText.Get("OperationWaiting"), 0, OperationOutcome.Running), tab.Directory, false);
        Journal.Entries.First(entry => ReferenceEquals(entry.Attempt, request)).CancelWaiting = new(() => CancelQueuedAction(request), () => !request.Started && _queuedActions.Contains(request));
        Changed(nameof(QueueStatus)); Changed(nameof(ActionOpen)); RefreshCommands();
        DrainActionQueue(defer: false);
        return request.Completion.Task;
    }

    private void DrainActionQueue(bool defer = true)
    {
        if (_stopping) return;
        foreach (var request in _queuedActions.ToArray())
        {
            if (request.Running) continue;
            if (request.Tab.Snapshot is null || !_knownIdentities.ContainsKey(request.Tab.Directory))
            {
                if (!request.Tab.InspectionTask.IsCompleted) continue;
                // Let execution report a failed initialization, never start against guessed state.
            }
            else if (request.Tab.History.Page is null && !request.WorkingCopy && !request.Tab.InspectionTask.IsCompleted) continue;
            var fetching = _fetchOwners.Any(owner => SameRepository(owner, request.Tab));
            if (!request.Independent && (request.IsFetch ? fetching : RepositoryBusy(request.Tab) || !request.WorkingCopy && fetching)) continue;
            if (_queuedActions.TakeWhile(other => other != request).Any(other =>
                SameRepository(other.Tab, request.Tab) && Conflicts(other, request))) continue;
            request.Running = true;
            _ = RunQueuedAsync(request, defer);
        }
    }
    private static bool Conflicts(QueuedAction first, QueuedAction second) =>
        !first.Independent && !second.Independent && !(first.IsFetch && second.WorkingCopy || second.IsFetch && first.WorkingCopy);

    private static bool SameRepository(RepositoryTab a, RepositoryTab b) => a == b ||
        a.Snapshot?.CommonDirectory is { } common && string.Equals(common, b.Snapshot?.CommonDirectory, StringComparison.OrdinalIgnoreCase);

    private async Task RunQueuedAsync(QueuedAction request, bool defer)
    {
        // Never open a modal or execute Git inside a command-state/property notification.
        if (defer) await Dispatcher.Yield(DispatcherPriority.Background);
        try
        {
            if (_stopping || !_queuedActions.Contains(request)) return;
            var tab = request.Tab;
            if (request.Key == "Refresh") { request.Started = true; Journal.Remove(request); await request.Run(); return; }
            if (tab.History.StartupPreview || tab.Snapshot is null ||
                !_knownIdentities.TryGetValue(tab.Directory, out var identity)) throw new GitActionException("ActionInspectionNeeded");
            if (request.Identity is not null && request.Identity != identity.Fingerprint) throw new GitActionException("ActionIdentityChanged");
            if (request.Key != "Refresh" && request.Branch is not null && request.Branch != tab.Snapshot.Branch) throw new GitActionException("ActionChanged");
            request.Started = true;
            Journal.Remove(request);
            var previous = _repositoryActions.GetValueOrDefault(tab.Directory);
            await request.Run();
            var result = _repositoryActions.GetValueOrDefault(tab.Directory);
            if (result != previous && result is { HasAttempt: true, Succeeded: false }) FailWaitingRequests(tab);
        }
        catch (GitActionException error) { ReportFailure(UiText.Get(error.Code)); }
        catch (Exception) { ReportFailure(UiText.Get("ActionReadFailed")); }
        finally
        {
            _queuedActions.Remove(request); request.Completion.TrySetResult();
            Journal.Entries.FirstOrDefault(entry => ReferenceEquals(entry.Attempt, request))?.CancelWaiting?.Refresh();
            Changed(nameof(QueueStatus)); Changed(nameof(ActionOpen)); RefreshCommands(); DrainActionQueue();
            PruneRetainedRepositories();
        }
        void ReportFailure(string message)
        {
            FailWaitingRequests(request.Tab);
            Journal.Record(new(request, request.Title, message, 0, OperationOutcome.Failed), request.Tab.Directory, false);
            _interaction?.ShowActionError(message);
        }
    }

    private void CancelQueuedActions(RepositoryTab? tab)
    {
        foreach (var item in _queuedActions.Where(item => item.Tab == tab && !item.Started).ToArray())
        { _queuedActions.Remove(item); item.Completion.TrySetResult(); }
        Changed(nameof(QueueStatus)); Changed(nameof(ActionOpen)); RefreshCommands();
    }
}
