using GitGui.App.Localization;
using GitGui.Core.Git;
using System.Windows.Threading;

namespace GitGui.App.ViewModels;

public sealed partial class ShellViewModel
{
    private sealed record QueuedAction(RepositoryTab Tab, string Key, string Title, Func<Task> Run,
        string? Identity, string? Branch, bool IsFetch)
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
    });

    private Task QueueSelectedAction(string key, string title, Func<RepositoryTab, Task> run)
    {
        var tab = SelectedRepository!;
        return QueueAction(tab, key, title, () => run(tab));
    }

    private Task QueueAction(RepositoryTab tab, string key, string title, Func<Task> run, bool isFetch = false)
    {
        var existing = _queuedActions.FirstOrDefault(item => item.Tab == tab && item.Key == key);
        if (existing is not null) return existing.Completion.Task;
        var request = new QueuedAction(tab, key, title, run, _knownIdentities.GetValueOrDefault(tab.Directory)?.Fingerprint, tab.Snapshot?.Branch, isFetch);
        _queuedActions.Add(request);
        if (key != "Commit") PreemptBackgroundFetch(tab);
        Changed(nameof(QueueStatus)); Changed(nameof(ActionOpen)); RefreshCommands();
        DrainActionQueue(defer: false);
        return request.Completion.Task;
    }

    private void DrainActionQueue(bool defer = true)
    {
        if (_stopping || _commitOpen || _actionOpen) return;
        foreach (var request in _queuedActions.ToArray())
        {
            if (request.Running || (request.Key == "Commit" ? RepositoryBusy(request.Tab) : RepositoryOperationBusy(request.Tab)) ||
                _queuedActions.Any(other => other.Running && (request.Key != "Commit" || !other.IsFetch) && SameRepository(other.Tab, request.Tab)) ||
                request.Key != "Commit" && SelectedRepository == request.Tab && IsBusy) continue;
            request.Running = true;
            _ = RunQueuedAsync(request, defer);
            break;
        }
    }

    private static bool SameRepository(RepositoryTab a, RepositoryTab b) => a == b ||
        a.Snapshot?.CommonDirectory is { } common && string.Equals(common, b.Snapshot?.CommonDirectory, StringComparison.OrdinalIgnoreCase);

    private async Task RunQueuedAsync(QueuedAction request, bool defer)
    {
        // Never open a modal or execute Git inside a command-state/property notification.
        if (defer) await Dispatcher.Yield(DispatcherPriority.Background);
        if (_commitOpen || _actionOpen) { request.Running = false; return; }
        try
        {
            if (_stopping || !_queuedActions.Contains(request)) return;
            var tab = request.Tab;
            if (request.Key == "Refresh") { request.Started = true; await request.Run(); return; }
            if (!Repositories.Contains(tab) || tab.History.StartupPreview || tab.Snapshot is null ||
                !_knownIdentities.TryGetValue(tab.Directory, out var identity)) throw new GitActionException("ActionInspectionNeeded");
            if (request.Identity is not null && request.Identity != identity.Fingerprint) throw new GitActionException("ActionIdentityChanged");
            if (request.Key != "Refresh" && request.Branch is not null && request.Branch != tab.Snapshot.Branch) throw new GitActionException("ActionChanged");
            request.Started = true;
            await request.Run();
        }
        catch (GitActionException error) { _interaction?.ShowActionError(UiText.Get(error.Code)); }
        catch (Exception) { _interaction?.ShowActionError(UiText.Get("ActionReadFailed")); }
        finally
        {
            _queuedActions.Remove(request); request.Completion.TrySetResult();
            Changed(nameof(QueueStatus)); Changed(nameof(ActionOpen)); RefreshCommands(); DrainActionQueue();
        }
    }

    private void CancelQueuedActions(RepositoryTab? tab)
    {
        foreach (var item in _queuedActions.Where(item => item.Tab == tab && !item.Started).ToArray())
        { _queuedActions.Remove(item); item.Completion.TrySetResult(); }
        Changed(nameof(QueueStatus)); Changed(nameof(ActionOpen)); RefreshCommands();
    }
}
