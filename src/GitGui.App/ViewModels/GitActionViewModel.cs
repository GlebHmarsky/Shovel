using System.Diagnostics;
using System.Text;
using System.Windows.Threading;
using GitGui.App.Localization;
using GitGui.Core.Git;
using GitGui.App.Services;

namespace GitGui.App.ViewModels;

public sealed class GitActionViewModel : ObservableObject
{
    private readonly GitActions _actions;
    private readonly Func<Task> _reconcile;
    private readonly IUserInteraction? _interaction;
    private readonly Dispatcher _dispatcher = Dispatcher.CurrentDispatcher;
    private readonly CancellationTokenSource _cancel = new();
    private readonly Stopwatch _elapsed = new();
    private readonly DispatcherTimer _timer = new() { Interval = TimeSpan.FromMilliseconds(250) };
    private readonly List<byte> _output = [];
    private int _publishedBytes;
    private Task _outputTask = Task.CompletedTask;
    private ActionContext? _context;
    private readonly Func<CancellationToken, Task<ActionContext>>? _prepareExecution;
    private readonly Func<ActionContext, ActionOptions, CancellationToken, Task<ActionOptions>>? _prepareOptions;
    private readonly Func<CancellationToken, Action<GitGui.Core.Processes.ProcessOutput>, Task<ActionResult>>? _execute;
    private readonly Func<CancellationToken, Action<GitGui.Core.Processes.ProcessOutput>, IAuthenticationSession?, Task<ActionResult>>? _authenticatedExecute;
    public ActionContext Context => _context ?? throw new InvalidOperationException();
    private readonly string _repository;
    private bool _dismissed;
    public GitAction Action { get; }
    public bool IsNew => Action == GitAction.NewBranch;
    public bool IsSwitch => Action == GitAction.SwitchBranch;
    public bool IsFetch => Action == GitAction.Fetch;
    public bool IsPull => Action == GitAction.Pull;
    public bool IsPush => Action == GitAction.Push;
    public bool IsNetwork => IsFetch || IsPull || IsPush || Action is GitAction.SubmoduleUpdate or GitAction.Clone;
    public bool ChooseBranch => IsPush || IsSwitch;
    public bool ChooseRemoteBranch => IsPush || IsPull;
    public string Title => UiText.Get((Action is GitAction.DeleteBranch or GitAction.SetUpstream or GitAction.UnsetUpstream ? "BranchManageTitle" : "ActionTitle") + Action);
    public string Explanation => UiText.Get((Action is GitAction.DeleteBranch or GitAction.SetUpstream or GitAction.UnsetUpstream ? "BranchManageExplain" : "ActionExplain") + Action);
    public string Into => _context?.Repository.Branch ?? UiText.Get("NoBranch");
    public string CurrentRepository => _repository;
    public IReadOnlyList<GitRemote> Remotes => _context?.Remotes ?? [];
    public IReadOnlyList<string> Branches { get; private set; } = [];
    public IReadOnlyList<string> StartRefs { get; private set; } = [];
    public string[] RemoteBranches => _context?.History.References.Where(item => item.Name.StartsWith("refs/remotes/" + Remote?.Name + "/", StringComparison.Ordinal))
        .Select(item => item.Name[(14 + Remote!.Name.Length)..]).ToArray() ?? [];
    private GitRemote? _remote;
    public GitRemote? Remote { get => _remote; set { if (Set(ref _remote, value)) { Changed(nameof(Target)); Changed(nameof(RemoteBranches)); Refresh(); } } }
    public string Target => Remote is null ? UiText.Get("ActionNoRemote") : _prepareExecution is not null
        ? UiText.Get("ActionCachedTargets") : IsPush ? Remote.PushAddress : Remote.FetchAddress;
    private string _branch = "", _remoteBranch = "", _newName = "", _startRef = "HEAD";
    public string Branch { get => _branch; set { if (Set(ref _branch, value)) Refresh(); } }
    public string RemoteBranch { get => _remoteBranch; set { if (Set(ref _remoteBranch, value)) Refresh(); } }
    public string NewName { get => _newName; set { if (Set(ref _newName, value)) Refresh(); } }
    public string StartRef { get => _startRef; set { if (Set(ref _startRef, value)) Refresh(); } }
    private bool _checkout = true, _allRemotes, _allTags, _setUpstream, _running, _attempted;
    public bool Checkout { get => _checkout; set => Set(ref _checkout, value); }
    public bool AllRemotes { get => _allRemotes; set => Set(ref _allRemotes, value); }
    public bool AllTags { get => _allTags; set => Set(ref _allTags, value); }
    public bool SetUpstream { get => _setUpstream; set => Set(ref _setUpstream, value); }
    private bool _autoStash;
    private PullStrategy _strategy = PullStrategy.Rebase;
    public bool AutoStash { get => _autoStash; set => Set(ref _autoStash, value); }
    public PullStrategy Strategy { get => _strategy; set { if (Set(ref _strategy, value)) { Changed(nameof(Rebase)); Changed(nameof(Merge)); } } }
    // Radio groups uncheck the previous item before checking the next. Only a positive
    // selection changes strategy, so binding initialization cannot select Merge accidentally.
    public bool Rebase { get => Strategy == PullStrategy.Rebase; set { if (value) Strategy = PullStrategy.Rebase; } }
    public bool Merge { get => Strategy == PullStrategy.Merge; set { if (value) Strategy = PullStrategy.Merge; } }
    public bool IsRunning { get => _running; private set { if (Set(ref _running, value)) { Changed(nameof(CanEdit)); Changed(nameof(CanCancel)); Refresh(); } } }
    public bool CanEdit => (_context is not null || _execute is not null || _authenticatedExecute is not null) && !_dismissed && !IsRunning && !_attempted;
    public bool HasAttempt => _attempted;
    public string RunUnavailableReason => UiText.Get(_context is null && _execute is null && _authenticatedExecute is null ? "ActionReadFailed" : IsRunning ? "ActionRunning" : HasAttempt ? "ActionAlreadyAttempted" :
        (IsFetch || IsPull || IsPush) && Remote is null ? "ActionNoRemote" :
        IsNew && string.IsNullOrWhiteSpace(NewName) ? "ActionEnterBranchName" :
        IsNew && !StartRefs.Contains(StartRef) ? "ActionNoStart" :
        ChooseBranch && !Branches.Contains(Branch) ? "ActionChooseBranch" :
        ChooseRemoteBranch && string.IsNullOrWhiteSpace(RemoteBranch) ? "ActionChooseRemoteBranch" : "ActionReady");
    public event EventHandler? ExecutionStarting;
    private bool _reconciling, _gitFinished, _cancelled, _needsResolution, _refreshFailed;
    public OperationActivity? Activity => !HasAttempt ? null : new(this, Title, Phase, _elapsed.Elapsed.TotalSeconds,
        IsRunning ? OperationOutcome.Running : _refreshFailed ? OperationOutcome.RefreshFailed : _needsResolution ? OperationOutcome.Conflict :
        Succeeded ? OperationOutcome.Success : _cancelled ? OperationOutcome.Cancelled : OperationOutcome.Failed);
    public bool IsReconciling { get => _reconciling; private set { if (Set(ref _reconciling, value)) Changed(nameof(CanCancel)); } }
    public bool CanCancel => IsRunning && !IsReconciling && !_cancel.IsCancellationRequested;
    public bool Succeeded { get; private set; }
    private string _phase = UiText.Get("ActionPreparing");
    private GitActionStage _gitStage = GitActionStage.Validating;
    public string Phase { get => _phase; private set { if (Set(ref _phase, value)) Changed(nameof(Summary)); } }
    public string Summary => UiText.Format("ActionSummary", Title, Phase, _elapsed.Elapsed.TotalSeconds);
    private string _status = UiText.Get("ActionReady"), _rawOutput = "";
    public string Status { get => _status; private set => Set(ref _status, value); }
    public string RawOutput { get => _rawOutput; private set => Set(ref _rawOutput, value); }
    public string Duration => UiText.Format("ActionDuration", _elapsed.Elapsed.TotalSeconds);
    public RelayCommand RunCommand { get; }
    public Task RunningTask { get; private set; } = Task.CompletedTask;

    public GitActionViewModel(ActionContext context, GitAction action, GitActions actions, Func<Task> reconcile, string? initialBranch = null,
        IUserInteraction? interaction = null, Func<CancellationToken, Task<ActionContext>>? prepareExecution = null,
        Func<CancellationToken, Action<GitGui.Core.Processes.ProcessOutput>, Task<ActionResult>>? execute = null,
        Func<ActionContext, ActionOptions, CancellationToken, Task<ActionOptions>>? prepareOptions = null)
    {
        _repository = context.Repository.Worktree; Action = action; _actions = actions; _reconcile = reconcile; _interaction = interaction;
        _prepareExecution = prepareExecution;
        _execute = execute;
        _prepareOptions = prepareOptions;
        RunCommand = new(() => RunningTask = RunAsync(), () => CanEdit && (!(IsFetch || IsPull || IsPush) || Remote is not null) &&
            (!IsNew || !string.IsNullOrWhiteSpace(NewName) && StartRefs.Contains(StartRef)) &&
            (!ChooseBranch || Branches.Contains(Branch)) && (!ChooseRemoteBranch || !string.IsNullOrWhiteSpace(RemoteBranch)));
        ApplyContext(context, initialBranch);
        InitializeTimer();
    }
    // Initialization has no repository snapshot yet, but shares command lifetime and output handling.
    public GitActionViewModel(string repository, GitActions actions, Func<Task> reconcile,
        Func<CancellationToken, Action<GitGui.Core.Processes.ProcessOutput>, Task<ActionResult>> execute)
    {
        _repository = repository; Action = GitAction.Init; _actions = actions; _reconcile = reconcile; _execute = execute;
        RunCommand = new(() => RunningTask = RunAsync(), () => CanEdit);
        InitializeTimer();
    }
    private void InitializeTimer() => _timer.Tick += (_, _) =>
    { Changed(nameof(Duration)); Changed(nameof(Summary)); if (!_gitFinished && _outputTask.IsCompleted) _outputTask = PublishOutputAsync(); };
    public GitActionViewModel(string repository, GitActions actions, Func<Task> reconcile, IUserInteraction interaction,
        Func<CancellationToken, Action<GitGui.Core.Processes.ProcessOutput>, IAuthenticationSession?, Task<ActionResult>> execute)
    {
        _repository = repository; Action = GitAction.Clone; _actions = actions; _reconcile = reconcile; _interaction = interaction; _authenticatedExecute = execute;
        RunCommand = new(() => RunningTask = RunAsync(), () => CanEdit); InitializeTimer();
    }
    public void DismissOptions()
    {
        if (HasAttempt) return;
        _dismissed = true; Refresh();
    }
    private void ApplyContext(ActionContext context, string? initialBranch)
    {
        _context = context;
        Branches = context.History.References.Where(item => item.Name.StartsWith("refs/heads/", StringComparison.Ordinal)).Select(item => item.Name[11..]).ToArray();
        StartRefs = context.History.References.Select(item => item.Name).OrderBy(item => item == "HEAD" ? "" : item, StringComparer.Ordinal).ToArray();
        _branch = initialBranch ?? context.Repository.Branch ?? Branches.FirstOrDefault() ?? "";
        _startRef = StartRefs.FirstOrDefault() ?? "";
        _remote = Remotes.FirstOrDefault(item => context.Repository.UpstreamRef?.StartsWith("refs/remotes/" + item.Name + "/", StringComparison.Ordinal) == true) ?? Remotes.FirstOrDefault();
        _remoteBranch = context.Repository.UpstreamRef is { } upstream && _remote is not null && upstream.StartsWith("refs/remotes/" + _remote.Name + "/", StringComparison.Ordinal)
            ? upstream[(14 + _remote.Name.Length)..] : _branch;
        Status = UiText.Get("ActionReady");
        foreach (var property in new[] { nameof(Into), nameof(Remotes), nameof(Branches), nameof(StartRefs), nameof(Remote), nameof(RemoteBranches),
            nameof(Branch), nameof(RemoteBranch), nameof(StartRef), nameof(Target) }) Changed(property);
    }
    private void Refresh() { RunCommand?.Refresh(); Changed(nameof(RunUnavailableReason)); }
    public void Cancel() { if (CanCancel) { Status = UiText.Get("ActionCancelling"); Phase = UiText.Get("ActionStopping"); _cancel.Cancel(); Changed(nameof(CanCancel)); } }
    private async Task PublishOutputAsync()
    {
        var text = await Task.Run(() =>
        {
            byte[] bytes;
            lock (_output)
            {
                if (_publishedBytes == _output.Count) return null;
                _publishedBytes = _output.Count; bytes = _output.ToArray();
            }
            return GitActions.Redact(Encoding.UTF8.GetString(bytes));
        });
        if (text is not null) RawOutput = text;
    }
    private async Task RunAsync()
    {
        // Freeze the reviewed options before the view closes or any asynchronous work starts.
        var options = new ActionOptions(Action, IsNew ? NewName : Branch, StartRef,
            Remote?.Name ?? "", RemoteBranch, Checkout, AllRemotes, AllTags, SetUpstream, Strategy, AutoStash);
        _attempted = true; Changed(nameof(HasAttempt)); IsRunning = true; _elapsed.Restart(); _timer.Start(); Status = UiText.Get("ActionRunning");
        try
        {
            ExecutionStarting?.Invoke(this, EventArgs.Empty);
            // Let ShowDialog unwind before creating prompts or starting Git. The shell owns this task.
            await Dispatcher.Yield(DispatcherPriority.Background);
            if (_prepareExecution is not null || _prepareOptions is not null)
            {
                Phase = UiText.Get("ActionStageValidating");
                using var validation = CancellationTokenSource.CreateLinkedTokenSource(_cancel.Token);
                validation.CancelAfter(TimeSpan.FromSeconds(45));
                if (_prepareExecution is not null) _context = await _prepareExecution(validation.Token);
                if (_prepareOptions is not null) options = await _prepareOptions(Context, options, validation.Token);
            }
            var promptContext = UiText.Format("AuthContext", Title, CurrentRepository);
            await using var authentication = IsNetwork ? await Task.Run(() => _interaction?.CreateAuthenticationSession(promptContext, async (prompt, token) =>
            {
                await _dispatcher.InvokeAsync(() =>
                {
                    if (!_cancel.IsCancellationRequested) { Status = UiText.Get("ActionWaitingAuthentication"); Phase = UiText.Get("ActionWaitingShort"); }
                });
                try { return await _interaction.ShowAuthenticationAsync(promptContext, prompt, token); }
                finally
                {
                    await _dispatcher.InvokeAsync(() =>
                    {
                        if (!_cancel.IsCancellationRequested && !_gitFinished)
                        { Status = UiText.Get("ActionRunning"); Phase = UiText.Get("ActionStage" + _gitStage); }
                    });
                }
            }, _cancel.Token)) : null;
            void Output(GitGui.Core.Processes.ProcessOutput output)
                { lock (_output) { if (_output.Count + output.Bytes.Length <= 1024 * 1024) _output.AddRange(output.Bytes); } }
            if (_execute is not null || _authenticatedExecute is not null) Phase = UiText.Get("ActionRunning");
            var result = _authenticatedExecute is not null ? await _authenticatedExecute(_cancel.Token, Output, authentication) :
                _execute is not null ? await _execute(_cancel.Token, Output) : await _actions.ExecuteAsync(Context, options, _cancel.Token, Output, authentication,
                stage => _dispatcher.BeginInvoke(() =>
                {
                    if (_gitFinished || _cancel.IsCancellationRequested) return;
                    _gitStage = stage;
                    if (Status != UiText.Get("ActionWaitingAuthentication")) Phase = UiText.Get("ActionStage" + stage);
                }));
            _gitFinished = true; await _outputTask; RawOutput = result.Output;
            Succeeded = result.Success && authentication?.Failure is null;
            _needsResolution = result.HasConflicts || result.Operation != RepositoryOperation.None;
            _cancelled = result.Outcome == GitGui.Core.Processes.ProcessOutcome.Cancelled;
            Status = UiText.Get(authentication?.Failure is { } failure ? "ActionAuthentication" + failure :
                result.MessageCode is { } code ? code :
                result.Operation.HasFlag(RepositoryOperation.Rebase) ? "ActionRebaseConflict" : result.Operation.HasFlag(RepositoryOperation.Merge) ? "ActionMergeConflict" :
                result.Operation != RepositoryOperation.None ? "RecoveryPending" :
                result.HasConflicts ? "ActionStashConflict" :
                result.Success ? "ActionSucceeded" : "ActionResult" + result.Outcome);
        }
        catch (GitActionException exception) { _gitFinished = true; await _outputTask; await PublishOutputAsync(); Status = UiText.Get(exception.Code); }
        catch (GitReadException exception)
        {
            _gitFinished = true; await _outputTask; _cancelled = exception.Failure == GitFailure.Cancelled;
            Status = UiText.Get(_cancelled ? "ActionResultCancelled" : "ActionReadFailed"); RawOutput = exception.RawOutput;
        }
        catch (OperationCanceledException) { _gitFinished = true; _cancelled = true; await _outputTask; await PublishOutputAsync(); Status = UiText.Get("ActionResultCancelled"); }
        catch (Exception exception) when (exception is System.IO.IOException or UnauthorizedAccessException or System.ComponentModel.Win32Exception)
        { Status = UiText.Get("ActionReadFailed"); }
        catch (Exception) { Status = UiText.Get("ActionUnexpectedFailure"); }
        finally
        {
            _gitFinished = true;
            await _outputTask;
            IsReconciling = true; Phase = UiText.Get("ActionReinspecting");
            var refreshed = false;
            // Cancellation is not rollback. Reinspection uses a fresh cancellation scope.
            try { await _reconcile(); refreshed = true; }
            catch (Exception) { Status = UiText.Format("ActionReinspectionFailed", Status); }
            finally
            {
                _timer.Stop(); _elapsed.Stop(); _refreshFailed = !refreshed;
                Phase = UiText.Get(!refreshed ? "ActionRefreshFailedShort" : _needsResolution ? "ActionConflictShort" : Succeeded ? "ActionCompletedShort" : _cancelled ? "ActionStoppedShort" : "ActionFailedShort");
                IsReconciling = false; IsRunning = false; Changed(nameof(Duration)); Changed(nameof(Summary)); _cancel.Dispose();
            }
        }
    }
}
