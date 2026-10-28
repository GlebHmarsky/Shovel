using System.Diagnostics;
using System.Text;
using System.Windows.Threading;
using GitGui.App.Localization;
using GitGui.App.Services;
using GitGui.Core.Git;
using GitGui.Core.Processes;

namespace GitGui.App.ViewModels;

/// <summary>Session-only bounded snapshot/draft. No repository content enters diagnostic logs.</summary>
public sealed class CommitWorkspaceState
{
    public WorkingCopyContext? Context { get; set; }
    public IReadOnlyList<ChangeRow> Rows { get; set; } = [];
    public ChangeRow? Selected { get; set; }
    public DateTimeOffset ReadAt { get; set; }
    public string Subject { get; set; } = "";
    public string Description { get; set; } = "";
    public string LastOutput { get; set; } = "";
    public string LastResult { get; set; } = "";
    public void Clear() { Context = null; Rows = []; Selected = null; ReadAt = default; }
}
public sealed record DiffRow(DiffLine Line, int HunkId = -1)
{
    public DiffLineKind Kind => Line.Kind;
    public string OldLine => Line.OldLine?.ToString(System.Globalization.CultureInfo.CurrentCulture) ?? "";
    public string NewLine => Line.NewLine?.ToString(System.Globalization.CultureInfo.CurrentCulture) ?? "";
    public string Marker => Kind == DiffLineKind.Added ? "+" : Kind == DiffLineKind.Removed ? "−" : "";
    public string Text { get; } = string.Concat(Line.Text.Select(c => char.IsControl(c) && c != '\t' ? "\\u" + ((int)c).ToString("X4") : c.ToString()));
    public string Accessibility => HunkId < 0 ? UiText.Format("DiffAccessible", UiText.Get("DiffLine" + Kind), OldLine, NewLine, Text)
        : UiText.Format("DiffHunkAccessible", HunkId + 1, UiText.Get("DiffLine" + Kind), OldLine, NewLine, Text);
}

public sealed class CommitViewModel : ObservableObject
{
    private readonly GitActions _actions;
    private readonly RepositoryReader _reader;
    private readonly IUserInteraction _interaction;
    private readonly CommitWorkspaceState _state;
    private readonly string? _identity;
    private readonly Dispatcher _dispatcher = Dispatcher.CurrentDispatcher;
    private readonly Stopwatch _elapsed = new();
    private readonly DispatcherTimer _timer = new() { Interval = TimeSpan.FromMilliseconds(250) };
    private readonly List<byte> _output = [];
    private CancellationTokenSource? _readCancel, _diffCancel, _actionCancel;
    private long _readGeneration, _diffGeneration;
    private bool _loading, _mutating, _diffLoading, _closed, _shift, _amend, _ignoreWhitespace, _wholeFile;
    private GitInstallation? _git;
    private string _status = UiText.Get("ChangesLoading"), _phase = UiText.Get("ChangesLoading"), _diffNotice = UiText.Get("DiffChooseFile");
    private string _rawOutput = "", _diffTitle = "";
    private ChangeRow? _selectedUnstaged, _selectedStaged;
    private IReadOnlyList<ChangeRow> _unstagedSelection = [], _stagedSelection = [];
    private IReadOnlyList<ChangeRow> _unstaged = [], _staged = [];
    private IReadOnlyList<DiffRow> _diffRows = [];
    private DiffRow? _selectedDiff;
    private FileDiff? _fileDiff;
    private ChangeRow? _diffChange;
    private int[] _selectedHunks = [];
    private string? _automaticAmendMessage;
    private bool _publishing;
    private Task _outputTask = Task.CompletedTask;
    private int _publishedBytes;
    public string Directory { get; }
    public string Title => UiText.Format("CommitWindowTitle", _state.Context?.Repository.Branch ?? UiText.Get("NoBranch"));
    public string RepositoryLabel => Directory;
    public string Status { get => _status; private set => Set(ref _status, value); }
    public string RawOutput { get => _rawOutput; private set => Set(ref _rawOutput, value); }
    public string Summary => IsBusy ? UiText.Format("ActionSummary", UiText.Get("CommitWorkspace"), _phase, _elapsed.Elapsed.TotalSeconds) : Status;
    public bool IsBusy => _loading || _mutating || _diffLoading;
    public bool IsMutating => _mutating;
    public bool CanEditMessage => !_closed && !_mutating;
    public bool Ready => !_closed && !_mutating && !_loading && _state.ReadAt != default && _state.Context is not null;
    public bool CanAmend => Ready && _state.Context?.Repository is { Commit: not null, Operation: RepositoryOperation.None };
    public bool HasMutation { get; private set; }
    public bool ShiftHeld { get => _shift; set { if (Set(ref _shift, value)) RefreshCommands(); } }
    public bool IgnoreWhitespace { get => _ignoreWhitespace; set { if (Set(ref _ignoreWhitespace, value)) QueueDiff(_state.Selected); } }
    public bool WholeFile { get => _wholeFile; set { if (Set(ref _wholeFile, value)) QueueDiff(_state.Selected); } }
    public bool Amend
    {
        get => _amend;
        set
        {
            if (value && !CanAmend || !Set(ref _amend, value)) return;
            if (value && string.IsNullOrWhiteSpace(Message) && _state.Context is { HeadMessage.Length: > 0 } context)
            { SetMessage(context.HeadMessage); _automaticAmendMessage = Message; }
            else if (!value && _automaticAmendMessage == Message) SetMessage("");
            Changed(nameof(CommitTarget)); RefreshCommands();
        }
    }
    public string Subject { get => _state.Subject; set { _state.Subject = value; Changed(); RefreshCommands(); } }
    public string Description { get => _state.Description; set { _state.Description = value; Changed(); RefreshCommands(); } }
    public string Message => Description.Length == 0 ? Subject : Subject + "\n\n" + Description;
    public string CommitTarget => Amend && _state.Context?.Repository.Commit is { } hash ? UiText.Format("CommitAmendTarget", hash) : UiText.Get("CommitDraftHelp");
    public IReadOnlyList<ChangeRow> Unstaged => _unstaged;
    public IReadOnlyList<ChangeRow> Staged => _staged;
    public string UnstagedTitle => UiText.Format("CommitUnstagedCount", Unstaged.Count);
    public string StagedTitle => UiText.Format("CommitStagedCount", Staged.Count);
    public string StageLabel => UiText.Get(ShiftHeld ? "CommitStageAll" : "CommitStage");
    public string UnstageLabel => UiText.Get(ShiftHeld ? "CommitUnstageAll" : "CommitUnstage");
    public string CommitLabel => UiText.Get(ShiftHeld ? "CommitAndPush" : "Commit");
    public string RefreshLabel => UiText.Get(IsBusy ? "ActionCancel" : "CommitRefresh");
    public string PushTarget
    {
        get
        {
            var target = ResolvePushTarget(_state.Context);
            return target is null ? UiText.Get("CommitNoUpstream") : UiText.Format("CommitPushTarget", target.Value.Remote, target.Value.Branch);
        }
    }
    public string CommitUnavailable => UiText.Get(!Ready ? "CommitWaitForRead" : Subject.Trim().Length == 0 ? "CommitMessageRequired" :
        Encoding.UTF8.GetByteCount(Message) > 65536 ? "CommitMessageLimit" : Unstaged.Any(row => row.Change.Group == ChangeGroup.Conflicted) ? "CommitUnresolved" :
        _state.Context!.Repository.Operation is not (RepositoryOperation.None or RepositoryOperation.Merge) ? "ActionSequence" :
        Staged.Count == 0 && !Amend && _state.Context.Repository.Operation != RepositoryOperation.Merge ? "CommitNothingStaged" :
        ShiftHeld && ResolvePushTarget(_state.Context) is null ? "CommitNoUpstream" : "CommitReady");
    public string StageHelp => UiText.Get(!Ready ? "CommitWaitForRead" : "CommitStageHelp");
    public string UnstageHelp => UiText.Get(!Ready ? "CommitWaitForRead" : "CommitUnstageHelp");
    public ChangeRow? SelectedUnstaged { get => _selectedUnstaged; set { if (Set(ref _selectedUnstaged, value) && !_publishing) Select(false, value is null ? [] : [value]); } }
    public ChangeRow? SelectedStaged { get => _selectedStaged; set { if (Set(ref _selectedStaged, value) && !_publishing) Select(true, value is null ? [] : [value]); } }
    public IReadOnlyList<DiffRow> DiffRows { get => _diffRows; private set => Set(ref _diffRows, value); }
    public DiffRow? SelectedDiff
    {
        get => _selectedDiff;
        set { if (Set(ref _selectedDiff, value)) SelectDiffRows(value is null ? [] : [value]); }
    }
    public bool CanApplyHunks => Ready && !_diffLoading && _fileDiff?.HunkPreview is not null &&
        _diffChange == _state.Selected && _selectedHunks.Length > 0;
    public string HunkActionLabel => _selectedHunks.Length > 1 ? UiText.Format(_diffChange?.Change.Group == ChangeGroup.Staged ? "HunkUnstageMany" : "HunkStageMany", _selectedHunks.Length)
        : UiText.Get(_diffChange?.Change.Group == ChangeGroup.Staged ? "HunkUnstage" : "HunkStage");
    public string HunkHelp => !Ready || _diffLoading ? UiText.Get("HunkWaitForDiff") : _fileDiff is null ? UiText.Get("HunkSelectFile") :
        _fileDiff.HunkAvailability != HunkAvailability.Available ? UiText.Get("HunkUnavailable" + _fileDiff.HunkAvailability) :
        _selectedHunks.Length == 0 ? UiText.Get("HunkSelectChange") : UiText.Format("HunkSelectedHelp", _selectedHunks.Length);
    public string DiffNotice { get => _diffNotice; private set => Set(ref _diffNotice, value); }
    public string DiffTitle { get => _diffTitle; private set => Set(ref _diffTitle, value); }
    public RelayCommand StageCommand { get; }
    public RelayCommand UnstageCommand { get; }
    public RelayCommand CommitCommand { get; }
    public RelayCommand RefreshCommand { get; }
    public RelayCommand NextHunkCommand { get; }
    public RelayCommand PreviousHunkCommand { get; }
    public RelayCommand ApplyHunksCommand { get; }
    public Task LoadingTask { get; private set; } = Task.CompletedTask;
    public Task DiffTask { get; private set; } = Task.CompletedTask;
    public Task OperationTask { get; private set; } = Task.CompletedTask;
    public event EventHandler? SnapshotChanged;

    public CommitViewModel(string directory, GitInstallation? git, RepositoryReader reader, GitActions actions, IUserInteraction interaction,
        CommitWorkspaceState? state = null, string? identity = null)
    {
        Directory = directory; _git = git; _reader = reader; _actions = actions; _interaction = interaction; _identity = identity; _state = state ?? new();
        StageCommand = new(() => StartAction(WorkingCopyAction.Stage), () => Ready && (ShiftHeld ? Unstaged.Count : _unstagedSelection.Count) > 0);
        UnstageCommand = new(() => StartAction(WorkingCopyAction.Unstage), () => Ready && (ShiftHeld ? Staged.Count : _stagedSelection.Count) > 0);
        CommitCommand = new(() => StartAction(WorkingCopyAction.Commit), () => CommitUnavailable == UiText.Get("CommitReady"));
        RefreshCommand = new(() => { if (IsBusy) Cancel(); else StartLoading(); }, () => !_closed);
        NextHunkCommand = new(() => MoveHunk(1), () => DiffRows.Any(row => row.Kind == DiffLineKind.Hunk));
        PreviousHunkCommand = new(() => MoveHunk(-1), () => DiffRows.Any(row => row.Kind == DiffLineKind.Hunk));
        ApplyHunksCommand = new(() =>
        {
            if (CanApplyHunks) StartAction(_diffChange!.Change.Group == ChangeGroup.Staged ? WorkingCopyAction.UnstageHunks : WorkingCopyAction.StageHunks,
                new(_fileDiff!.HunkPreview!, _selectedHunks.ToArray()));
        }, () => CanApplyHunks);
        _timer.Tick += (_, _) => { Changed(nameof(Summary)); if (_mutating && _outputTask.IsCompleted) _outputTask = PublishOutputAsync(); };
        PublishRows(); RawOutput = _state.LastOutput;
        if (_state.LastResult.Length != 0) Status = _state.LastResult;
    }
    public Task StartLoading() { if (!_closed && !_mutating) LoadingTask = LoadAsync(); return LoadingTask; }
    private async Task LoadAsync(bool reconcile = false)
    {
        _readCancel?.Cancel(); var previous = LoadingTask; var generation = ++_readGeneration;
        _diffCancel?.Cancel(); _diffCancel = null; _diffGeneration++; _diffLoading = false; _fileDiff = null; _selectedHunks = [];
        using var cancellation = new CancellationTokenSource(TimeSpan.FromSeconds(45)); _readCancel = cancellation;
        _loading = true; _state.ReadAt = default; _phase = UiText.Get(reconcile ? "ActionReinspecting" : "ChangesLoading"); UpdateBusy();
        try
        {
            await Dispatcher.Yield(DispatcherPriority.Background);
            if (!reconcile) await previous;
            cancellation.Token.ThrowIfCancellationRequested();
            _git ??= await _reader.DiscoverAsync(null, cancellation.Token);
            var context = await _actions.ReadWorkingCopyAsync(_git, Directory, cancellation.Token);
            if (_closed || cancellation.IsCancellationRequested || generation != _readGeneration) return;
            if (_identity is not null && context.Identity.Fingerprint != _identity) { _state.Clear(); PublishRows(); throw new GitActionException("ActionIdentityChanged"); }
            var rows = await Task.Run(() => context.Changes.Entries.Select(entry => new ChangeRow(entry)).ToArray(), cancellation.Token);
            if (_closed || cancellation.IsCancellationRequested || generation != _readGeneration) return;
            _state.Context = context; _state.ReadAt = context.Changes.ReadAt; _state.Rows = rows;
            PublishRows();
            if (!reconcile) Status = UiText.Format(_state.Rows.Count == 0 ? "ChangesClean" : "ChangesLoaded", context.Changes.ReadAt.LocalDateTime);
            if (string.IsNullOrWhiteSpace(Message) && context.MergeMessage.Length != 0) SetMessage(context.MergeMessage);
            if (context.Repository.Commit is null || context.Repository.Operation != RepositoryOperation.None) { _amend = false; Changed(nameof(Amend)); }
            SnapshotChanged?.Invoke(this, EventArgs.Empty);
        }
        catch (Exception error) { if (!_closed && generation == _readGeneration) { _state.ReadAt = default; Status = Explain(error); CaptureError(error); } }
        finally
        {
            if (generation == _readGeneration)
            {
                _readCancel = null; _loading = false; UpdateBusy();
                if (_state.ReadAt != default && !reconcile) QueueDiff(_state.Selected);
            }
        }
    }
    private void PublishRows()
    {
        _unstaged = _state.Rows.Where(row => row.Change.Group is ChangeGroup.Unstaged or ChangeGroup.Untracked or ChangeGroup.Conflicted).ToArray();
        _staged = _state.Rows.Where(row => row.Change.Group == ChangeGroup.Staged).ToArray();
        var prior = _state.Selected?.Change;
        _state.Selected = _state.Rows.FirstOrDefault(row => row.Change.Path == prior?.Path && row.Change.Group == prior?.Group) ?? Unstaged.FirstOrDefault() ?? Staged.FirstOrDefault();
        _publishing = true;
        try
        {
            Changed(nameof(Unstaged)); Changed(nameof(Staged)); Changed(nameof(UnstagedTitle)); Changed(nameof(StagedTitle));
            SelectedUnstaged = Unstaged.FirstOrDefault(row => row == _state.Selected); SelectedStaged = Staged.FirstOrDefault(row => row == _state.Selected);
            _unstagedSelection = SelectedUnstaged is null ? [] : [SelectedUnstaged]; _stagedSelection = SelectedStaged is null ? [] : [SelectedStaged];
        }
        finally { _publishing = false; }
        Changed(nameof(Title)); Changed(nameof(PushTarget)); Changed(nameof(CommitTarget)); RefreshCommands();
    }
    public void Select(bool staged, IReadOnlyList<ChangeRow> rows)
    {
        if (_publishing) return;
        var valid = rows.Where(row => (staged ? Staged : Unstaged).Contains(row)).ToArray();
        if (staged) _stagedSelection = valid; else _unstagedSelection = valid;
        // DataGrid can report the bound selection again during layout. Do not restart the same
        // file read after it has completed (or while it is already in flight).
        if (valid.FirstOrDefault() is { } row)
        {
            _publishing = true;
            try
            {
                if (staged) { SelectedStaged = row; SelectedUnstaged = null; _unstagedSelection = []; }
                else { SelectedUnstaged = row; SelectedStaged = null; _stagedSelection = []; }
            }
            finally { _publishing = false; }
            if (_state.Selected != row || DiffTitle != row.Path) { _state.Selected = row; QueueDiff(row); }
        }
        RefreshCommands();
    }
    private void QueueDiff(ChangeRow? row)
    {
        _diffCancel?.Cancel(); var previous = DiffTask; var generation = ++_diffGeneration;
        _fileDiff = null; _diffChange = row; _selectedHunks = [];
        DiffRows = []; SelectedDiff = null; DiffTitle = row?.Path ?? ""; DiffNotice = UiText.Get(row is null ? "DiffChooseFile" : "DiffLoading");
        if (_closed || _mutating || row is null || _state.Context is not { } context) { _diffLoading = false; UpdateBusy(); return; }
        var options = new DiffOptions(IgnoreWhitespace, WholeFile);
        var cancellation = new CancellationTokenSource(TimeSpan.FromSeconds(45)); _diffCancel = cancellation; _diffLoading = true; UpdateBusy();
        DiffTask = RunAsync();
        async Task RunAsync()
        {
            try
            {
                await Dispatcher.Yield(DispatcherPriority.Background); await previous;
                cancellation.Token.ThrowIfCancellationRequested();
                var diff = await _reader.ReadFileDiffAsync(context, row.Change, options, cancellation.Token);
                var lines = await Task.Run(() =>
                {
                    var hunkIds = Enumerable.Repeat(-1, diff.Lines.Count).ToArray();
                    foreach (var hunk in diff.HunkPreview?.Hunks ?? [])
                        Array.Fill(hunkIds, hunk.Id, hunk.StartRow, hunk.RowCount);
                    return diff.Lines.Select((line, index) => new DiffRow(line, hunkIds[index])).ToArray();
                }, cancellation.Token);
                if (_closed || generation != _diffGeneration || cancellation.IsCancellationRequested) return;
                _fileDiff = diff; DiffRows = lines;
                var oldBytes = diff.OldBytes?.ToString("N0", System.Globalization.CultureInfo.CurrentCulture) ?? UiText.Get("CountUnavailable");
                var newBytes = diff.NewBytes?.ToString("N0", System.Globalization.CultureInfo.CurrentCulture) ?? UiText.Get("CountUnavailable");
                DiffNotice = UiText.Format("DiffSummary" + diff.Kind, oldBytes, newBytes, diff.Added, diff.Removed);
                if (row.Change.Group == ChangeGroup.Conflicted) DiffNotice = UiText.Get("DiffConflict");
                if (lines.Length == 0 && diff.Kind == FileDiffKind.Text) DiffNotice = UiText.Get(IgnoreWhitespace ? "DiffNoWhitespaceChanges" : "DiffNoTextChanges");
            }
            catch (Exception error) { if (!_closed && generation == _diffGeneration) { DiffNotice = Explain(error); CaptureError(error); } }
            finally
            {
                if (generation == _diffGeneration) { _diffCancel = null; _diffLoading = false; UpdateBusy(); }
                cancellation.Dispose();
            }
        }
    }
    private void MoveHunk(int direction)
    {
        var hunks = DiffRows.Select((row, index) => (row, index)).Where(item => item.row.Kind == DiffLineKind.Hunk).ToArray();
        if (hunks.Length == 0) return;
        var current = SelectedDiff is null ? -1 : Array.IndexOf(DiffRows.ToArray(), SelectedDiff);
        SelectedDiff = (direction > 0 ? hunks.FirstOrDefault(item => item.index > current).row ?? hunks[0].row
            : hunks.LastOrDefault(item => item.index < current).row ?? hunks[^1].row);
    }
    public void SelectDiffRows(IReadOnlyList<DiffRow> rows)
    {
        var current = new HashSet<DiffRow>(DiffRows, ReferenceEqualityComparer.Instance);
        _selectedHunks = rows.Where(row => row.HunkId >= 0 && current.Contains(row)).Select(row => row.HunkId).Distinct().Order().ToArray();
        RefreshCommands();
    }
    private void StartAction(WorkingCopyAction action, HunkSelection? hunks = null)
    {
        if (!Ready) return;
        var context = _state.Context!; var push = action == WorkingCopyAction.Commit && ShiftHeld;
        if (action == WorkingCopyAction.Commit && Amend && !_interaction.ConfirmAmend(context.Repository.Branch ?? UiText.Get("NoBranch"), context.Repository.Commit!)) return;
        var entries = hunks is not null ? new[] { _diffChange! } : action == WorkingCopyAction.Stage ? ShiftHeld ? Unstaged : _unstagedSelection : action == WorkingCopyAction.Unstage ? ShiftHeld ? Staged : _stagedSelection : [];
        var options = new WorkingCopyOptions(action, entries.Select(row => row.Change).ToArray(), Message, Amend, hunks);
        _mutating = true; HasMutation = true; _diffCancel?.Cancel(); _readCancel?.Cancel(); _state.ReadAt = default;
        _elapsed.Restart(); _phase = UiText.Get("CommitPhase" + action); Status = _phase; RawOutput = "";
        lock (_output) { _output.Clear(); _publishedBytes = 0; }
        _actionCancel = new CancellationTokenSource(); UpdateBusy(); OperationTask = RunAsync();
        async Task RunAsync()
        {
            string? committed = null;
            try
            {
                await Dispatcher.Yield(DispatcherPriority.Background); await LoadingTask; await DiffTask;
                var result = await _actions.ExecuteWorkingCopyAsync(context, options, _actionCancel.Token, CollectOutput);
                await _outputTask; RawOutput = result.Result.Output; committed = result.CommitHash;
                Status = UiText.Get(result.Result.Success ? "CommitSucceeded" + action : "ActionResult" + result.Result.Outcome);
                if (result.Result.Success && action == WorkingCopyAction.Commit)
                {
                    SetMessage(""); _amend = false; _automaticAmendMessage = null; Changed(nameof(Amend));
                    if (push && committed is not null) await PushAsync(context, committed, _actionCancel.Token);
                    else if (committed is not null) Status = UiText.Format("CommitCreated", committed);
                    else Status = UiText.Get("CommitSucceededUnknownTip");
                }
            }
            catch (Exception error)
            {
                await _outputTask; await PublishOutputAsync(); CaptureError(error);
                Status = committed is null ? Explain(error) : UiText.Format("CommitCreatedPushFailed", committed, Explain(error));
            }
            finally
            {
                var outcome = Status; var output = RawOutput;
                await LoadAsync(reconcile: true);
                Status = _state.ReadAt == default ? UiText.Format("ActionReinspectionFailed", outcome) : outcome;
                RawOutput = output; _state.LastResult = Status; _state.LastOutput = output;
                _actionCancel.Dispose(); _actionCancel = null; _mutating = false; UpdateBusy();
                if (!_closed) QueueDiff(_state.Selected);
            }
        }
    }
    private async Task PushAsync(WorkingCopyContext previous, string commit, CancellationToken token)
    {
        var target = ResolvePushTarget(previous) ?? throw new GitActionException("CommitNoUpstream");
        _phase = UiText.Get("ActionStagePushing"); Changed(nameof(Summary));
        var context = await _actions.PrepareAsync(previous.Git, Directory, token);
        if (context.Repository.Commit != commit || context.Repository.Branch != previous.Repository.Branch || context.ConfigStamp != previous.ConfigStamp || context.Identity.Fingerprint != previous.Identity.Fingerprint)
            throw new GitActionException("CommitStale");
        var promptContext = UiText.Format("AuthContext", UiText.Get("CommitAndPush"), Directory);
        await using var authentication = await Task.Run(() => _interaction.CreateAuthenticationSession(promptContext, async (prompt, cancellation) =>
        {
            await _dispatcher.InvokeAsync(() => { _phase = UiText.Get("ActionWaitingAuthentication"); Changed(nameof(Summary)); });
            try { return await _interaction.ShowAuthenticationAsync(promptContext, prompt, cancellation); }
            finally { await _dispatcher.InvokeAsync(() => { _phase = UiText.Get("ActionStagePushing"); Changed(nameof(Summary)); }); }
        }, token));
        var result = await _actions.ExecuteAsync(context, new(GitAction.Push, context.Repository.Branch!, Remote: target.Remote, RemoteBranch: target.Branch), token, CollectOutput, authentication);
        await _outputTask; RawOutput += result.Output;
        Status = result.Success && authentication?.Failure is null ? UiText.Format("CommitCreatedPushed", commit)
            : UiText.Format("CommitCreatedPushFailed", commit, UiText.Get(authentication?.Failure is { } failure ? "ActionAuthentication" + failure : "ActionResult" + result.Outcome));
    }
    private static (string Remote, string Branch)? ResolvePushTarget(WorkingCopyContext? context)
    {
        if (context?.Repository is not { Branch: not null, UpstreamRef: { } upstream }) return null;
        var remote = context.Repository.RemoteNames.OrderByDescending(name => name.Length).FirstOrDefault(name => upstream.StartsWith("refs/remotes/" + name + "/", StringComparison.Ordinal));
        return remote is null ? null : (remote, upstream[(14 + remote.Length)..]);
    }
    private void SetMessage(string message)
    {
        var parts = message.Replace("\r\n", "\n").Split('\n', 2); Subject = parts[0]; Description = parts.Length == 2 ? parts[1].TrimStart('\n') : "";
    }
    private void CollectOutput(ProcessOutput chunk) { lock (_output) { if (_output.Count + chunk.Bytes.Length <= 1024 * 1024) _output.AddRange(chunk.Bytes); } }
    private async Task PublishOutputAsync()
    {
        var text = await Task.Run(() => { byte[] bytes; lock (_output) { if (_publishedBytes == _output.Count) return null; _publishedBytes = _output.Count; bytes = _output.ToArray(); } return GitActions.Redact(Encoding.UTF8.GetString(bytes)); });
        if (text is not null) RawOutput = text;
    }
    private void CaptureError(Exception error) { if (error is GitReadException read && read.RawOutput.Length != 0) RawOutput = GitActions.Redact(read.RawOutput); }
    private static string Explain(Exception error) => error switch
    {
        GitActionException action => UiText.Get(action.Code),
        GitReadException read => UiText.Get("GitError" + read.Failure),
        OperationCanceledException => UiText.Get("ActionResultCancelled"),
        _ => UiText.Get("ActionReadFailed")
    };
    private void UpdateBusy()
    {
        if (IsBusy) { if (!_elapsed.IsRunning) _elapsed.Restart(); _timer.Start(); }
        else { _timer.Stop(); _elapsed.Stop(); }
        foreach (var name in new[] { nameof(IsBusy), nameof(IsMutating), nameof(CanEditMessage), nameof(Ready), nameof(CanAmend), nameof(Summary), nameof(RefreshLabel) }) Changed(name);
        RefreshCommands();
    }
    private void RefreshCommands()
    {
        StageCommand?.Refresh(); UnstageCommand?.Refresh(); CommitCommand?.Refresh(); RefreshCommand?.Refresh(); NextHunkCommand?.Refresh(); PreviousHunkCommand?.Refresh();
        ApplyHunksCommand?.Refresh();
        Changed(nameof(CanApplyHunks)); Changed(nameof(HunkActionLabel)); Changed(nameof(HunkHelp));
        foreach (var name in new[] { nameof(StageLabel), nameof(UnstageLabel), nameof(CommitLabel), nameof(CommitUnavailable), nameof(StageHelp), nameof(UnstageHelp) }) Changed(name);
    }
    public void Cancel() { _readCancel?.Cancel(); _diffCancel?.Cancel(); _actionCancel?.Cancel(); }
    public async Task CloseAsync()
    {
        _closed = true; ShiftHeld = false; Cancel();
        await LoadingTask; await DiffTask; await OperationTask; _timer.Stop();
    }
}
