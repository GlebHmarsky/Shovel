using GitGui.App.Localization;
using GitGui.Core.Git;

namespace GitGui.App.ViewModels;
public sealed partial class ShellViewModel
{
    public RelayCommand ManageRemotesCommand { get; private set; } = null!;
    public string ManageRemotesHelp => UiText.Get(ActionUnavailableKey ??
        (SelectedRepository?.RemoteConfiguration is null ? "RemoteManageLoading" : "RemoteManageReady"));
    private async Task ManageRemotesAsync(string? selected = null)
    {
        var tab = SelectedRepository!; var snapshot = tab.Snapshot!; var reviewed = tab.RemoteConfiguration;
        if (reviewed is null || !_knownIdentities.TryGetValue(tab.Directory, out var known)) return;
        var context = new ActionContext(_git!, new(snapshot.Worktree, snapshot.GitDirectory, snapshot.CommonDirectory, known.Fingerprint),
            snapshot, tab.History.Page!.Session, [], reviewed.ConfigStamp);
        CancelCountRead(); SetActionOpen(true); var attempted = false;
        try
        {
            var options = _interaction!.ChooseRemoteManagement(new(reviewed.Remotes, selected));
            if (options is null || SelectedRepository != tab) return;
            if (options.Remove && !_interaction.ConfirmRecovery(UiText.Format("RemoteRemoveConfirm", options.Name, tab.Directory), UiText.Get("ActionTitleRemoveRemote"))) return;
            var operation = new GitActionViewModel(context, options.Remove ? GitAction.RemoveRemote : GitAction.EditRemote, _actions!, async () =>
            {
                foreach (var related in Repositories.Where(item => item.Snapshot?.CommonDirectory == snapshot.CommonDirectory))
                { related.History.ValidatedAt = default; related.History.NeedsRefresh = true; related.SidebarReadAt = default; related.RemoteConfiguration = null; }
                BeginInspection(tab, force: true, actionReconcile: true); await WaitForIdleAsync();
                if (tab.History.ValidatedAt == default) throw new System.IO.IOException();
            }, execute: (token, progress) => _actions!.ChangeRemoteAsync(context, reviewed, options, token, progress));
            SetLastAction(operation); DetailsIndex = 2; operation.RunCommand.Execute(null); attempted = operation.HasAttempt;
            await operation.RunningTask; Status = operation.Status;
        }
        finally { SetActionOpen(false); if (attempted) { BeginCountRead(tab); BeginSidebarRead(tab); } }
    }
}
