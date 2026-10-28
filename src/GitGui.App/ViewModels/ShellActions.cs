using GitGui.App.Localization;
using GitGui.Core.Git;

namespace GitGui.App.ViewModels;

public sealed record BranchMenuItem(string Name, RelayCommand<string> Command);

public sealed partial class ShellViewModel
{
    private GitActions? _actions;
    private bool _actionOpen, _preparingAction;
    public bool ActionOpen => _actionOpen || _commitOpen;
    private GitActionViewModel? _lastAction;
    public GitActionViewModel? CurrentAction => string.Equals(_lastAction?.CurrentRepository, SelectedRepository?.Directory, StringComparison.OrdinalIgnoreCase) ? _lastAction : null;
    public bool HasAction => CurrentAction?.HasAttempt == true;
    public bool IsActionRunning => CurrentAction?.IsRunning == true;
    public Task ActionTask { get; private set; } = Task.CompletedTask;
    public RelayCommand ShowActionDetailsCommand { get; private set; } = null!;
    public RelayCommand NewBranchCommand { get; private set; } = null!;
    public RelayCommand FetchCommand { get; private set; } = null!;
    public RelayCommand PullCommand { get; private set; } = null!;
    public RelayCommand PushCommand { get; private set; } = null!;
    public RelayCommand ContinueCommand { get; private set; } = null!;
    public RelayCommand SkipCommand { get; private set; } = null!;
    public RelayCommand AbortCommand { get; private set; } = null!;
    public bool HasRecovery => SelectedRepository?.Snapshot?.Recovery is not null;
    public bool CanSkipRecovery => SelectedRepository?.Snapshot?.Recovery?.CanSkip == true;
    public string RecoveryHelp => SelectedRepository?.Snapshot?.Recovery is { } recovery
        ? UiText.Format("RecoveryHelp", UiText.Get("Operation" + recovery.Kind)) : UiText.Get("RecoveryUnavailable");
    private bool CanRecover => CanNavigate && !IsBusy && _interaction is not null && _actions is not null && _git is not null && HasRecovery && State?.Page is not null;
    public RelayCommand<string> SwitchBranchCommand { get; private set; } = null!;
    public IReadOnlyList<string> LocalBranches => State?.Page?.Session.References.Where(item => item.Name.StartsWith("refs/heads/", StringComparison.Ordinal)).Select(item => item.Name[11..]).ToArray() ?? [];
    public IReadOnlyList<BranchMenuItem> BranchMenuItems => LocalBranches.Select(name => new BranchMenuItem(name, SwitchBranchCommand)).ToArray();
    private bool CanAct => CanNavigate && !IsBusy && _interaction is not null && _actions is not null && _git is not null && State?.Page is not null && SelectedRepository?.Snapshot is { Operation: RepositoryOperation.None };
    public bool CanOpenBranchMenu => CanAct;
    private string? ActionUnavailableKey => !IsReady || _stopping ? "ActionWorkspaceStarting" :
        ActionOpen ? "ActionWorkspaceBusy" : SelectedRepository is null ? "ActionOpenRepository" :
        IsBusy ? "ActionWaitForRead" : _git is null ? "ActionSelectGit" :
        _interaction is null || _actions is null || SelectedRepository.Snapshot is null || State?.Page is null ? "ActionInspectionNeeded" :
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
        ContinueCommand = new(() => OpenAction(GitAction.Continue), () => CanRecover);
        SkipCommand = new(() => OpenAction(GitAction.Skip), () => CanRecover && CanSkipRecovery);
        AbortCommand = new(() => OpenAction(GitAction.Abort), () => CanRecover);
        ShowActionDetailsCommand = new(() => RevealDetails(3), () => HasAction);
        NewBranchCommand = new(() => OpenAction(GitAction.NewBranch), () => CanAct);
        FetchCommand = new(() => OpenAction(GitAction.Fetch), () => CanAct);
        PullCommand = new(() => OpenAction(GitAction.Pull), () => CanAct && SelectedRepository?.Snapshot?.HeadKind == HeadKind.Branch);
        PushCommand = new(() => OpenAction(GitAction.Push), () => CanAct && LocalBranches.Count != 0);
        SwitchBranchCommand = new(branch => OpenAction(GitAction.SwitchBranch, branch), branch => CanAct && LocalBranches.Contains(branch));
    }
    private void OpenAction(GitAction action, string? branch = null) => ActionTask = OpenActionAsync(action, branch);
    private async Task OpenActionAsync(GitAction action, string? branch)
    {
        var tab = SelectedRepository!;
        // A modal confirmation pumps the dispatcher. Freeze its target before it opens;
        // activation/refresh must not silently replace the operation the user reviewed.
        var reviewedSnapshot = tab.Snapshot!;
        var reviewedIdentity = _knownIdentities.GetValueOrDefault(tab.Directory);
        var recoveryAction = action is GitAction.Continue or GitAction.Skip or GitAction.Abort;
        if (recoveryAction && action != GitAction.Continue && reviewedSnapshot.Recovery is { } recovery &&
            !_interaction!.ConfirmRecovery(UiText.Format("RecoveryConfirm" + action, tab.Directory,
                UiText.Get("Operation" + recovery.Kind), recovery.Branch.Length > 0 ? recovery.Branch : reviewedSnapshot.Branch ?? UiText.Get("NoBranch"),
                recovery.Target.Length > 0 ? recovery.Target : reviewedSnapshot.Commit ?? UiText.Get("NoBranch"), recovery.CurrentCommit), UiText.Get("ActionTitle" + action))) return;
        if (SelectedRepository != tab) return;
        CancelCountRead();
        var attempted = false;
        SetActionOpen(true); Status = UiText.Get("ActionPreparing");
        try
        {
            var identity = recoveryAction ? reviewedIdentity : _knownIdentities.GetValueOrDefault(tab.Directory);
            if (tab.History.Page is not { } page || identity is null)
                throw new GitActionException("ActionInspectionNeeded");
            var snapshot = recoveryAction ? reviewedSnapshot : tab.Snapshot!;
            var cached = new ActionContext(_git!, new RepositoryIdentity(snapshot.Worktree, snapshot.GitDirectory,
                snapshot.CommonDirectory, identity.Fingerprint), snapshot, page.Session,
                snapshot.RemoteNames.Select(name => new GitRemote(name, name, name)).ToArray(), "");
            var model = new GitActionViewModel(cached, action, _actions!, async () =>
            {
                foreach (var related in Repositories.Where(item => string.Equals(item.Snapshot?.CommonDirectory, tab.Snapshot?.CommonDirectory, StringComparison.OrdinalIgnoreCase)))
                { related.History.ValidatedAt = default; related.History.NeedsRefresh = true; related.Changes.ReadAt = default; }
                // Keep navigation blocked through reconciliation. Internal refresh explicitly bypasses that UI gate.
                BeginInspection(tab, force: true, actionReconcile: true);
                await WaitForIdleAsync();
                if (tab.History.ValidatedAt == default || tab.History.Error.Length != 0)
                    throw new System.IO.IOException(); // Localized separately from the Git outcome; no raw exception logging.
            }, branch, _interaction, recoveryAction ? null : async token =>
            {
                var context = await _actions!.PrepareAsync(_git!, tab.Directory, token);
                if (!_knownIdentities.TryGetValue(tab.Directory, out var known) || known.Fingerprint != context.Identity.Fingerprint)
                    throw new GitActionException("ActionIdentityChanged");
                if (context.Repository.Commit != cached.Repository.Commit || context.Repository.Branch != cached.Repository.Branch ||
                    context.History.Fingerprint != cached.History.Fingerprint)
                    throw new GitActionException("ActionChanged");
                return context;
            });
            var previous = _lastAction;
            _preparingAction = false;
            SetLastAction(model);
            if (recoveryAction) model.RunCommand.Execute(null);
            else _interaction!.ShowGitAction(model);
            model.DismissOptions(); attempted = model.HasAttempt;
            if (!model.HasAttempt) SetLastAction(previous);
            else
            {
                DetailsIndex = 3;
                Status = UiText.Get("ActionRunning");
                // The options window is gone; keep owning the operation through fresh inspection.
                await model.RunningTask;
                Status = model.Summary;
            }
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
            if (attempted) BeginCountRead(tab);
        }
    }
    private void SetActionOpen(bool value)
    {
        _actionOpen = value; _preparingAction = value; Changed(nameof(ActionOpen)); Changed(nameof(CanNavigate)); Changed(nameof(OperationStatus)); RefreshCommands();
    }
    private void SetLastAction(GitActionViewModel? model)
    {
        if (_lastAction is not null) _lastAction.PropertyChanged -= OnActionChanged;
        _lastAction = model;
        if (model is not null) model.PropertyChanged += OnActionChanged;
        NotifyActionContext();
    }
    private void OnActionChanged(object? sender, System.ComponentModel.PropertyChangedEventArgs e)
    {
        if (e.PropertyName == nameof(GitActionViewModel.Summary)) Changed(nameof(OperationStatus));
        if (e.PropertyName is nameof(GitActionViewModel.IsRunning) or nameof(GitActionViewModel.HasAttempt) or nameof(GitActionViewModel.CanCancel))
            NotifyActionContext();
    }
    private void NotifyActionContext()
    {
        if (!HasAction && DetailsIndex == 3) DetailsIndex = 0;
        Changed(nameof(CurrentAction)); Changed(nameof(HasAction)); Changed(nameof(IsActionRunning)); Changed(nameof(OperationStatus));
        ShowActionDetailsCommand?.Refresh(); CancelCommand?.Refresh();
    }
}
