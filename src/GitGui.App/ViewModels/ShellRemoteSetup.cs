using GitGui.Core.Git;

namespace GitGui.App.ViewModels;
public sealed partial class ShellViewModel
{
    public RelayCommand AddRemoteCommand { get; private set; } = null!;
    private async Task AddRemoteAsync(RepositoryTab? targetTab = null)
    {
        var tab = targetTab ?? SelectedRepository!; var snapshot = tab.Snapshot!;
        if (!_knownIdentities.TryGetValue(tab.Directory, out var known)) return;
        var cached = new ActionContext(_git!, new(snapshot.Worktree, snapshot.GitDirectory, snapshot.CommonDirectory, known.Fingerprint),
            snapshot, tab.History.Page!.Session, [], "");
        CancelCountRead(); SetActionOpen(true, tab); var attempted = false;
        try
        {
            var options = _interaction!.ChooseRemoteSetup(new(snapshot.RemoteNames));
            if (options is null) return;
            ActionContext? prepared = null;
            var operation = new GitActionViewModel(cached, GitAction.AddRemote, _actions!, () => ReconcileActionAsync(tab), prepareExecution: async token => prepared = await _actions!.PrepareRemoteSetupAsync(cached, token),
                execute: (token, progress) => _actions!.AddRemoteAsync(prepared!, options, token, progress));
            SetLastAction(operation); DetailsIndex = 2; operation.RunCommand.Execute(null); attempted = operation.HasAttempt;
            await operation.RunningTask; if (SelectedRepository == tab) Status = operation.Status;
        }
        finally { SetActionOpen(false, tab); if (attempted) { BeginCountRead(tab); BeginSidebarRead(tab); } }
    }
}
