using GitGui.App.Localization;
using GitGui.Core.Git;

namespace GitGui.App.ViewModels;

public sealed record BranchMenuItem(string Name, RelayCommand<string> Command);

public sealed partial class ShellViewModel
{
    private GitActions? _actions;
    private bool _actionOpen, _preparingAction;
    private readonly HashSet<RepositoryTab> _operationOwners = [];
    private readonly Dictionary<string, GitActionViewModel> _repositoryActions = new(StringComparer.OrdinalIgnoreCase);
    public bool ActionOpen => _actionOpen || _operationOwners.Count != 0 || _queuedActions.Count != 0 || _commitOpen;
    private readonly HashSet<RepositoryTab> _fetchOwners = [];
    private bool RepositoryBusy(RepositoryTab? tab) => tab is not null && (_commitSessions.Any(pair => pair.Value.CommandExecuting && SameRepository(pair.Key, tab)) || HistoryFileBusy(tab) || _actionOpen && tab == _initializationOwner || _operationOwners.Any(owner => !_fetchOwners.Contains(owner) && SameRepository(owner, tab)));
    private bool RepositoryOperationBusy(RepositoryTab? tab) => tab is not null && (_commitSessions.Any(pair => pair.Value.CommandExecuting && SameRepository(pair.Key, tab)) || HistoryFileBusy(tab) || _actionOpen && tab == _initializationOwner || _operationOwners.Any(owner => owner == tab ||
        owner.Snapshot?.CommonDirectory is { } common && string.Equals(common, tab.Snapshot?.CommonDirectory, StringComparison.OrdinalIgnoreCase)));
    private GitActionViewModel? _lastAction;
    public GitActionViewModel? CurrentAction => SelectedRepository is { } tab && _repositoryActions.TryGetValue(tab.Directory, out var action)
        ? action : SelectedRepository == _initializationOwner ? _lastAction : null;
    public bool HasAction => CurrentAction?.HasAttempt == true;
    public bool IsActionRunning => CurrentAction?.IsRunning == true;
    public Task ActionTask { get; private set; } = Task.CompletedTask;
    public RelayCommand ShowActionDetailsCommand { get; private set; } = null!;
    public RelayCommand NewBranchCommand { get; private set; } = null!;
    public RelayCommand FetchCommand { get; private set; } = null!;
    public RelayCommand PullCommand { get; private set; } = null!;
    public RelayCommand DefaultPullCommand { get; private set; } = null!;
    public RelayCommand DefaultFetchCommand { get; private set; } = null!;
    public RelayCommand DefaultPushCommand { get; private set; } = null!;
    public RelayCommand PushCommand { get; private set; } = null!;
    public RelayCommand ContinueCommand { get; private set; } = null!;
    public RelayCommand SkipCommand { get; private set; } = null!;
    public RelayCommand AbortCommand { get; private set; } = null!;
    public bool HasRecovery => SelectedRepository?.Snapshot?.Recovery is not null;
    public bool HasOperationBanner => SelectedRepository?.Snapshot?.Recovery?.Kind is RecoveryKind.Merge or RecoveryKind.Rebase;
    public string OperationBanner => UiText.Format("ConflictOperationBanner", UiText.Get("Operation" + SelectedRepository?.Snapshot?.Recovery?.Kind));
    public bool CanSkipRecovery => SelectedRepository?.Snapshot?.Recovery?.CanSkip == true;
    public string RecoveryHelp => SelectedRepository?.Snapshot?.Recovery is { } recovery
        ? UiText.Format("RecoveryHelp", UiText.Get("Operation" + recovery.Kind)) : UiText.Get("RecoveryUnavailable");
    private bool CanRecover => CanNavigate && !ShowingStartupPreview && _interaction is not null && _actions is not null && _git is not null && HasRecovery && State?.Page is not null;
    public RelayCommand<string> SwitchBranchCommand { get; private set; } = null!;
    public IReadOnlyList<string> LocalBranches => State?.Page?.Session.References.Where(item => item.Name.StartsWith("refs/heads/", StringComparison.Ordinal)).Select(item => item.Name[11..]).ToArray() ?? [];
    public IReadOnlyList<BranchMenuItem> BranchMenuItems => LocalBranches.Select(name => new BranchMenuItem(name, SwitchBranchCommand)).ToArray();
    private bool CanAct => CanNavigate && !ShowingStartupPreview && _interaction is not null && _actions is not null && _git is not null && State?.Page is not null && SelectedRepository?.Snapshot is { Operation: RepositoryOperation.None };
    public bool CanOpenBranchMenu => CanRequestAction;
    private string? ActionUnavailableKey => !IsReady || _stopping ? "ActionWorkspaceStarting" :
        SelectedRepository is null ? "ActionOpenRepository" :
        _git is null ? "ActionSelectGit" :
        _interaction is null || _actions is null || SelectedRepository.Snapshot is null || State?.Page is null ? "ActionInspectionNeeded" :
        SelectedRepository.Snapshot.Operation != RepositoryOperation.None ? "ActionSequence" : null;
    public string BranchActionHelp => UiText.Get(ActionUnavailableKey ?? "ActionTitleNewBranch");
    public string FetchActionHelp => UiText.Get(ActionUnavailableKey ?? "ActionTitleFetch");
    public string PullActionHelp => ActionUnavailableKey is { } reason ? UiText.Get(reason) :
        SelectedRepository?.Snapshot?.HeadKind != HeadKind.Branch ? UiText.Get("ActionPullNeedsBranch") : DivergenceHelp;
    private (string Remote, string Branch)? DefaultPullTarget => PreferredPullTarget(SelectedRepository);
    private static (string Remote, string Branch)? PullTarget(RepositoryTab? tab)
    {
        var snapshot = tab?.Snapshot;
        if (snapshot?.HeadKind != HeadKind.Branch || snapshot.UpstreamRef is not { } upstream) return null;
        var remotes = snapshot.RemoteNames.Where(name => upstream.StartsWith("refs/remotes/" + name + "/", StringComparison.Ordinal)).ToArray();
        if (remotes.Length != 1) return null;
        var branch = upstream[(14 + remotes[0].Length)..];
        return branch.Length > 0 ? (remotes[0], branch) : null;
    }
    public string DefaultPullHelp => ActionUnavailableKey is { } reason ? UiText.Get(reason) :
        SelectedRepository?.Snapshot?.HeadKind != HeadKind.Branch ? UiText.Get("ActionPullNeedsBranch") :
        UiText.Get(DefaultPullTarget is null ? "PullDefaultNeedsUpstream" : "PullDefaultHelp");
    public string DefaultFetchHelp => UiText.Get(ActionUnavailableKey ?? (SelectedRepository?.Snapshot?.RemoteNames.Count > 0 ? "DefaultFetchHelp" : "ActionNoRemote"));
    public string DefaultPushHelp => UiText.Get(ActionUnavailableKey ?? (SelectedRepository?.Snapshot?.HeadKind != HeadKind.Branch ? "ActionAttachedRequired" :
        SelectedRepository.Snapshot.RemoteNames.Count == 0 ? "ActionNoRemote" : "DefaultPushHelp"));
    public string PushActionHelp => ActionUnavailableKey is { } reason ? UiText.Get(reason) :
        LocalBranches.Count == 0 ? UiText.Get("ActionNeedsBranch") : DivergenceHelp;
    public string SwitchActionHelp => UiText.Get(ActionUnavailableKey ?? (LocalBranches.Count == 0 ? "ActionNeedsBranch" : "ActionTitleSwitchBranch"));
    public bool CanSwitchBranch => CanRequestAction && LocalBranches.Count != 0;
    private void InitializeActions(GitActions? actions)
    {
        _actions = actions;
        NewRepositoryCommand = new(() => ActionTask = InitializeRepositoryAsync(), () => CanNavigate && !_actionOpen &&
            _reader is not null && _interaction is not null && _actions is not null && Repositories.Count < 20);
        CloneRepositoryCommand = new(() => ActionTask = CloneRepositoryAsync(), () => NewRepositoryCommand.CanExecute(null));
        InitializeStashes();
        InitializeBranchManagement();
        ContinueCommand = new(() => OpenAction(GitAction.Continue), () => CanRecover);
        SkipCommand = new(() => OpenAction(GitAction.Skip), () => CanRecover && CanSkipRecovery);
        AbortCommand = new(() => OpenAction(GitAction.Abort), () => CanRecover);
        ShowActionDetailsCommand = new(() => RevealDetails(2), () => HasAction);
        NewBranchCommand = new(() => OpenAction(GitAction.NewBranch), () => CanRequestAction);
        FetchCommand = new(() => OpenAction(GitAction.Fetch), () => CanRequestAction);
        PullCommand = new(() => OpenAction(GitAction.Pull), () => CanRequestAction && SelectedRepository?.Snapshot?.HeadKind == HeadKind.Branch);
        DefaultPullCommand = new(() => OpenAction(GitAction.Pull, defaultPull: true), () => CanRequestAction && (SelectedRepository?.Snapshot is null || DefaultPullTarget is not null));
        DefaultFetchCommand = new(() => OpenAction(GitAction.Fetch, directNetwork: true), () => CanRequestAction && (SelectedRepository?.Snapshot is null || SelectedRepository.Snapshot.RemoteNames.Count > 0));
        DefaultPushCommand = new(() => OpenAction(GitAction.Push, directNetwork: true), () => CanRequestAction && (SelectedRepository?.Snapshot is null || SelectedRepository.Snapshot is { HeadKind: HeadKind.Branch, RemoteNames.Count: > 0 }));
        PushCommand = new(() => OpenAction(GitAction.Push), () => CanRequestAction && LocalBranches.Count != 0);
        SwitchBranchCommand = new(branch => OpenAction(GitAction.SwitchBranch, branch), branch => CanRequestAction && LocalBranches.Contains(branch));
    }
    private void OpenAction(GitAction action, string? branch = null, bool defaultPull = false, bool directNetwork = false)
    {
        var tab = SelectedRepository!;
        ActionTask = QueueAction(tab, action.ToString(), UiText.Get("ActionTitle" + action),
            () => OpenActionAsync(action, branch, defaultPull, directNetwork, tab), isFetch: action == GitAction.Fetch);
    }
    private async Task OpenActionAsync(GitAction action, string? branch, bool defaultPull = false, bool directNetwork = false, RepositoryTab? targetTab = null, bool background = false)
    {
        var tab = targetTab ?? SelectedRepository!;
        var pullTarget = defaultPull ? PreferredPullTarget(tab) : null;
        var savedNetwork = background ? null : NetworkPreference(tab, action);
        var savedTarget = savedNetwork is { ExplicitTarget: true } && (action == GitAction.Fetch || savedNetwork.Branch == (tab.Snapshot?.Branch ?? ""));
        if (defaultPull && pullTarget is null) throw new GitActionException("PullDefaultNeedsUpstream");
        if (tab.Snapshot is not { Operation: RepositoryOperation.None } && action is not (GitAction.Continue or GitAction.Skip or GitAction.Abort)) throw new GitActionException("ActionSequence");
        // A modal confirmation pumps the dispatcher. Freeze its target before it opens;
        // activation/refresh must not silently replace the operation the user reviewed.
        var reviewedSnapshot = tab.Snapshot!;
        var reviewedIdentity = _knownIdentities.GetValueOrDefault(tab.Directory);
        var recoveryAction = action is GitAction.Continue or GitAction.Skip or GitAction.Abort;
        if (recoveryAction && action != GitAction.Continue && reviewedSnapshot.Recovery is { } recovery &&
            !_interaction!.ConfirmRecovery(UiText.Format("RecoveryConfirm" + action, tab.Directory,
                UiText.Get("Operation" + recovery.Kind), recovery.Branch.Length > 0 ? recovery.Branch : reviewedSnapshot.Branch ?? UiText.Get("NoBranch"),
                recovery.Target.Length > 0 ? recovery.Target : reviewedSnapshot.Commit ?? UiText.Get("NoBranch"), recovery.CurrentCommit), UiText.Get("ActionTitle" + action))) return;
        if (SelectedRepository == tab) CancelCountRead();
        var attempted = false;
        if (action == GitAction.Fetch) _fetchOwners.Add(tab);
        SetActionOpen(true, tab); if (!background) Status = UiText.Get("ActionPreparing");
        try
        {
            var identity = recoveryAction ? reviewedIdentity : _knownIdentities.GetValueOrDefault(tab.Directory);
            var history = tab.History.Page?.Session ?? (background || directNetwork || defaultPull ? new HistorySession(tab.Snapshot!, HistoryScope.AllCommits, "", [], false) : null);
            if (history is null || identity is null)
                throw new GitActionException("ActionInspectionNeeded");
            var snapshot = recoveryAction ? reviewedSnapshot : tab.Snapshot!;
            var cached = new ActionContext(_git!, new RepositoryIdentity(snapshot.Worktree, snapshot.GitDirectory,
                snapshot.CommonDirectory, identity.Fingerprint), snapshot, history,
                snapshot.RemoteNames.Select(name => new GitRemote(name, name, name)).ToArray(), "");
            var model = new GitActionViewModel(cached, action, _actions!, () => action == GitAction.Fetch ? ReconcileFetchAsync(tab) : ReconcileActionAsync(tab), branch, background ? null : _interaction, recoveryAction ? null : async token =>
            {
                var context = await _actions!.PrepareAsync(_git!, tab.Directory, token);
                if (!_knownIdentities.TryGetValue(tab.Directory, out var known) || known.Fingerprint != context.Identity.Fingerprint)
                    throw new GitActionException("ActionIdentityChanged");
                if (!background && (context.Repository.Commit != cached.Repository.Commit || context.Repository.Branch != cached.Repository.Branch ||
                    (!directNetwork && !defaultPull && action != GitAction.SwitchBranch && context.History.Fingerprint != cached.History.Fingerprint)))
                    throw new GitActionException("ActionChanged");
                if (!savedTarget && pullTarget is { } target)
                    await _actions.ValidateDefaultPullTargetAsync(context, target.Remote, target.Branch, token);
                return context;
            }, prepareOptions: directNetwork && !savedTarget ? (context, options, token) => _actions!.ResolveDefaultNetworkAsync(context, options, token) : null);
            model.ReviewOptions = options => ReviewCheckout(tab, options);
            model.IsBackground = background;
            if (!background) RestoreNetworkOptions(tab, model);
            if (pullTarget is { } selectedTarget)
            {
                model.Remote = cached.Remotes.FirstOrDefault(remote => remote.Name == selectedTarget.Remote);
                model.RemoteBranch = selectedTarget.Branch;
            }
            var previous = _repositoryActions.GetValueOrDefault(tab.Directory);
            _preparingAction = false;
            SetLastAction(model);
            if (recoveryAction || defaultPull || directNetwork || action == GitAction.SwitchBranch)
            {
                if (!model.RunCommand.CanExecute(null)) throw new GitActionException("ActionNoRemote");
                model.RunCommand.Execute(null);
            }
            else
            {
                var initialOptions = model.CaptureOptions();
                _interaction!.ShowGitAction(model);
                // The arrow is a preference editor as well as a submit route. Closing it
                // preserves choices; execution always uses the snapshot captured on submit.
                RememberNetworkOptions(tab, model, initialOptions);
            }
            model.DismissOptions(); attempted = model.HasAttempt;
            if (!model.HasAttempt && previous is not null) SetLastAction(previous);
            else
            {
                if (!background && SelectedRepository == tab) { DetailsIndex = 2; Status = UiText.Get("ActionRunning"); }
                // The options window is gone; keep owning the operation through fresh inspection.
                await model.RunningTask;
                if (!background && SelectedRepository == tab) Status = model.Summary;
            }
        }
        catch (GitActionException exception) { if (!background) _interaction!.ShowActionError(UiText.Get(exception.Code)); }
        catch (GitReadException exception)
        {
            if (State is { } state) { state.RawOutput = GitActions.Redact(exception.RawOutput); Changed(nameof(RawOutput)); }
            if (!background) _interaction!.ShowActionError(UiText.Get("ActionReadFailed"));
        }
        catch (OperationCanceledException) { if (!background) _interaction!.ShowActionError(UiText.Get("ActionReadFailed")); }
        catch (Exception exception) when (exception is System.IO.IOException or UnauthorizedAccessException or System.ComponentModel.Win32Exception)
        { if (!background) _interaction!.ShowActionError(UiText.Get("ActionReadFailed")); }
        finally
        {
            if (Status == UiText.Get("ActionPreparing")) Status = UiText.Get("NoActiveOperation");
            _fetchOwners.Remove(tab);
            SetActionOpen(false, tab);
            DrainActionQueue();
            if (attempted) { if (!background) BeginCountRead(tab); BeginSidebarRead(tab); }
        }
    }
    private void SetActionOpen(bool value, RepositoryTab? owner = null)
    {
        if (value && (owner is null || SelectedRepository == owner)) _sidebarCancel?.Cancel();
        if (owner is null) _actionOpen = value;
        else if (value) _operationOwners.Add(owner); else _operationOwners.Remove(owner);
        _preparingAction = value; if (!value) DrainActionQueue(); Changed(nameof(ActionOpen)); Changed(nameof(CanNavigate)); Changed(nameof(OperationStatus)); RefreshCommands();
    }
    private void SetLastAction(GitActionViewModel? model)
    {
        var key = model?.Action is GitAction.Init or GitAction.Clone ? _initializationOwner?.Directory : model?.CurrentRepository ?? SelectedRepository?.Directory;
        if (key is not null && _repositoryActions.Remove(key, out var previous)) previous.PropertyChanged -= OnActionChanged;
        if (model is not null)
        {
            if (key is not null) _repositoryActions[key] = model;
            model.PropertyChanged += OnActionChanged;
        }
        if (model?.Action is GitAction.Init or GitAction.Clone) _lastAction = model;
        NotifyActionContext();
    }
    private void OnActionChanged(object? sender, System.ComponentModel.PropertyChangedEventArgs e)
    {
        if (sender is GitActionViewModel operation && e.PropertyName is nameof(GitActionViewModel.Summary) or nameof(GitActionViewModel.IsRunning) or nameof(GitActionViewModel.RawOutput))
        {
            if (operation.Activity is { } activity) Journal.Record(activity, operation.CurrentRepository, operation.IsBackground, operation.RawOutput);
            if (operation.HasAttempt && !operation.IsRunning) ReportOperationProblem(operation);
        }
        if (ReferenceEquals(sender, CurrentAction) && e.PropertyName is nameof(GitActionViewModel.Summary) or nameof(GitActionViewModel.IsRunning))
            PublishActivity(CurrentAction?.Activity);
        if (e.PropertyName == nameof(GitActionViewModel.Summary)) Changed(nameof(OperationStatus));
        if (e.PropertyName is nameof(GitActionViewModel.IsRunning) or nameof(GitActionViewModel.HasAttempt) or nameof(GitActionViewModel.CanCancel))
            NotifyActionContext();
    }
    private void NotifyActionContext()
    {
        if (!HasAction && DetailsIndex == 2) DetailsIndex = 0;
        Changed(nameof(CurrentAction)); Changed(nameof(HasAction)); Changed(nameof(IsActionRunning)); Changed(nameof(OperationStatus));
        ShowActionDetailsCommand?.Refresh(); CancelCommand?.Refresh();
    }
}
