using GitGui.App.Localization;
using GitGui.Core.Git;

namespace GitGui.App.ViewModels;

public sealed record BranchMenuItem(string Name, RelayCommand<string> Command);

public sealed partial class ShellViewModel
{
    private GitActions? _actions;
    private bool _actionOpen;
    public bool ActionOpen => _actionOpen;
    public RelayCommand NewBranchCommand { get; private set; } = null!;
    public RelayCommand FetchCommand { get; private set; } = null!;
    public RelayCommand PullCommand { get; private set; } = null!;
    public RelayCommand PushCommand { get; private set; } = null!;
    public RelayCommand<string> SwitchBranchCommand { get; private set; } = null!;
    public IReadOnlyList<string> LocalBranches => State?.Page?.Session.References.Where(item => item.Name.StartsWith("refs/heads/", StringComparison.Ordinal)).Select(item => item.Name[11..]).ToArray() ?? [];
    public IReadOnlyList<BranchMenuItem> BranchMenuItems => LocalBranches.Select(name => new BranchMenuItem(name, SwitchBranchCommand)).ToArray();
    private bool CanAct => CanNavigate && !IsBusy && _interaction is not null && _actions is not null && _git is not null && SelectedRepository?.Snapshot is { Operation: RepositoryOperation.None };
    public bool CanOpenBranchMenu => CanAct;
    private string? ActionUnavailableKey => !IsReady || _stopping ? "ActionWorkspaceStarting" :
        ActionOpen ? "ActionDialogOpen" : SelectedRepository is null ? "ActionOpenRepository" :
        IsBusy ? "ActionWaitForRead" : _git is null ? "ActionSelectGit" :
        _interaction is null || _actions is null || SelectedRepository.Snapshot is null ? "ActionInspectionNeeded" :
        SelectedRepository.Snapshot.Operation != RepositoryOperation.None ? "ActionSequence" : null;
    public string BranchActionHelp => UiText.Get(ActionUnavailableKey ?? "ActionTitleNewBranch");
    public string FetchActionHelp => UiText.Get(ActionUnavailableKey ?? "ActionTitleFetch");
    public string PullActionHelp => ActionUnavailableKey is { } reason ? UiText.Get(reason) :
        SelectedRepository?.Snapshot?.HeadKind != HeadKind.Branch ? UiText.Get("ActionPullNeedsBranch") : DivergenceHelp;
    public string PushActionHelp => ActionUnavailableKey is { } reason ? UiText.Get(reason) :
        LocalBranches.Count == 0 ? UiText.Get("ActionNeedsBranch") : DivergenceHelp;
    public string SwitchActionHelp => UiText.Get(ActionUnavailableKey ?? (LocalBranches.Count == 0 ? "ActionNeedsBranch" : "ActionTitleSwitchBranch"));
    public bool CanSwitchBranch => CanAct && LocalBranches.Count != 0;
    private void InitializeActions(GitActions? actions)
    {
        _actions = actions;
        NewBranchCommand = new(() => OpenAction(GitAction.NewBranch), () => CanAct);
        FetchCommand = new(() => OpenAction(GitAction.Fetch), () => CanAct);
        PullCommand = new(() => OpenAction(GitAction.Pull), () => CanAct && SelectedRepository?.Snapshot?.HeadKind == HeadKind.Branch);
        PushCommand = new(() => OpenAction(GitAction.Push), () => CanAct && LocalBranches.Count != 0);
        SwitchBranchCommand = new(branch => OpenAction(GitAction.SwitchBranch, branch), branch => CanAct && LocalBranches.Contains(branch));
    }
    private async void OpenAction(GitAction action, string? branch = null)
    {
        var tab = SelectedRepository!;
        SetActionOpen(true); Status = UiText.Get("ActionPreparing");
        try
        {
            using var timeout = new CancellationTokenSource(TimeSpan.FromSeconds(45));
            var context = await _actions!.PrepareAsync(_git!, tab.Directory, timeout.Token);
            if (!_knownIdentities.TryGetValue(tab.Directory, out var known) || known.Fingerprint != context.Identity.Fingerprint)
                throw new GitActionException("ActionIdentityChanged");
            var model = new GitActionViewModel(context, action, _actions, async () =>
            {
                foreach (var related in Repositories.Where(item => string.Equals(item.Snapshot?.CommonDirectory, context.Repository.CommonDirectory, StringComparison.OrdinalIgnoreCase)))
                { related.History.ValidatedAt = default; related.History.NeedsRefresh = true; }
                // Keep navigation blocked through reconciliation. Internal refresh explicitly bypasses that UI gate.
                BeginInspection(tab, force: true, actionReconcile: true);
                await WaitForIdleAsync();
            }, branch, _interaction);
            _interaction!.ShowGitAction(model);
        }
        catch (GitActionException exception) { _interaction!.ShowActionError(UiText.Get(exception.Code)); }
        catch (GitReadException exception)
        {
            if (State is { } state) { state.RawOutput = GitActions.Redact(exception.RawOutput); Changed(nameof(RawOutput)); }
            _interaction!.ShowActionError(UiText.Get("ActionReadFailed"));
        }
        catch (OperationCanceledException) { _interaction!.ShowActionError(UiText.Get("ActionReadFailed")); }
        catch (Exception exception) when (exception is System.IO.IOException or UnauthorizedAccessException or System.ComponentModel.Win32Exception)
        { _interaction!.ShowActionError(UiText.Get("ActionReadFailed")); }
        finally
        {
            if (Status == UiText.Get("ActionPreparing")) Status = UiText.Get("NoActiveOperation");
            SetActionOpen(false);
        }
    }
    private void SetActionOpen(bool value)
    {
        _actionOpen = value; Changed(nameof(ActionOpen)); Changed(nameof(CanNavigate)); Changed(nameof(OperationStatus)); RefreshCommands();
    }
}
