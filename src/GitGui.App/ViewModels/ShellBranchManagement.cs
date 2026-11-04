using GitGui.App.Localization;
using GitGui.Core.Git;

namespace GitGui.App.ViewModels;

public sealed partial class ShellViewModel
{
    public RelayCommand DeleteBranchCommand { get; private set; } = null!;
    public RelayCommand SetBranchUpstreamCommand { get; private set; } = null!;
    public RelayCommand UnsetBranchUpstreamCommand { get; private set; } = null!;
    public string BranchManagementHelp => UiText.Get(ActionUnavailableKey ??
        (LocalBranches.Count == 0 ? "BranchManageChooseBranch" : "BranchManageMenuHelp"));

    private void InitializeBranchManagement()
    {
        AddRemoteCommand = new(() => ActionTask = QueueSelectedAction("AddRemote", UiText.Get("RemoteAddTitle"), tab => AddRemoteAsync(tab)), () => CanAct);
        ManageRemotesCommand = new(() => ActionTask = QueueSelectedAction("ManageRemotes", UiText.Get("RemoteManageTitle"), tab => ManageRemotesAsync(targetTab: tab)), () => CanAct && SelectedRepository?.RemoteConfiguration is not null);
        DeleteBranchCommand = new(() => ActionTask = QueueSelectedAction("BranchDelete", UiText.Get("BranchManageTitleDeleteBranch"), tab => ManageBranchAsync(BranchManagementAction.Delete, targetTab: tab)), () => CanAct && LocalBranches.Count != 0);
        SetBranchUpstreamCommand = new(() => ActionTask = QueueSelectedAction("BranchSetUpstream", UiText.Get("BranchManageTitleSetUpstream"), tab => ManageBranchAsync(BranchManagementAction.SetUpstream, targetTab: tab)), () => CanAct && LocalBranches.Count != 0);
        UnsetBranchUpstreamCommand = new(() => ActionTask = QueueSelectedAction("BranchUnsetUpstream", UiText.Get("BranchManageTitleUnsetUpstream"), tab => ManageBranchAsync(BranchManagementAction.UnsetUpstream, targetTab: tab)), () => CanAct && LocalBranches.Count != 0);
    }
    private void RefreshBranchManagement()
    {
        AddRemoteCommand?.Refresh(); ManageRemotesCommand?.Refresh(); Changed(nameof(ManageRemotesHelp)); DeleteBranchCommand?.Refresh(); SetBranchUpstreamCommand?.Refresh(); UnsetBranchUpstreamCommand?.Refresh();
        Changed(nameof(BranchManagementHelp));
    }
    private async Task ManageBranchAsync(BranchManagementAction action, string? selectedBranch = null, RepositoryTab? targetTab = null)
    {
        var tab = targetTab ?? SelectedRepository!;
        // Freeze before the modal pumps the dispatcher; neither confirmation nor a later
        // refresh may substitute the branch/hash that the user actually submitted.
        var snapshot = tab.Snapshot!; var page = tab.History.Page!;
        var known = _knownIdentities.GetValueOrDefault(tab.Directory);
        if (known is null) return;
        var cached = new ActionContext(_git!, new RepositoryIdentity(snapshot.Worktree, snapshot.GitDirectory,
            snapshot.CommonDirectory, known.Fingerprint), snapshot, page.Session, [], "");
        CancelCountRead(); SetActionOpen(true, tab);
        _preparingAction = false; Changed(nameof(OperationStatus));
        var attempted = false;
        try
        {
            var choices = new BranchManagementViewModel(action, snapshot, page.Session);
            if (selectedBranch is not null) choices.Branch = choices.Branches.FirstOrDefault(b => b.Name == selectedBranch);
            var options = _interaction!.ChooseBranchManagement(choices);
            if (options is null) return;
            var kind = action switch
            {
                BranchManagementAction.Delete => GitAction.DeleteBranch,
                BranchManagementAction.SetUpstream => GitAction.SetUpstream,
                _ => GitAction.UnsetUpstream
            };
            ActionContext? prepared = null;
            var operation = new GitActionViewModel(cached, kind, _actions!, () => ReconcileActionAsync(tab), prepareExecution: async token =>
            {
                var context = await _actions!.PrepareBranchManagementAsync(cached.Git, cached.Identity, cached.Repository, cached.History, token);
                prepared = context; return context;
            },
                execute: async (token, progress) =>
                {
                    var result = await _actions!.ExecuteBranchManagementAsync(prepared!, options, token, progress);
                    var target = UiText.Format("BranchManageResultTarget", options.Branch.Name, options.Branch.Hash,
                        options.Upstream?.Name ?? UiText.Get("BranchManageNoUpstream"));
                    return result with { Output = target + Environment.NewLine + result.Output };
                });
            _preparingAction = false; SetLastAction(operation); DetailsIndex = 2;
            operation.RunCommand.Execute(null); attempted = operation.HasAttempt;
            await operation.RunningTask;
            if (SelectedRepository == tab) Status = operation.Summary;
        }
        finally
        {
            SetActionOpen(false, tab);
            if (attempted) { BeginCountRead(tab); BeginSidebarRead(tab); }
        }
    }
}
