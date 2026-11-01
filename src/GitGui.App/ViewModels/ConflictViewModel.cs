using System.Diagnostics;
using System.Windows.Threading;
using GitGui.App.Localization;
using GitGui.Core.Git;
using GitGui.Core.Processes;

namespace GitGui.App.ViewModels;

public sealed class ConflictViewModel : ObservableObject
{
    public sealed record ToolOption(string Name, string Label);
    public IReadOnlyList<ToolOption> Tools { get; private set; } = [new("", UiText.Get("ConflictToolDefault"))];
    private string _externalTool = "";
    public string ExternalTool { get => _externalTool; set => Set(ref _externalTool, value ?? ""); }
    public ActivityJournal? Journal { get; init; }
    private readonly GitActions _actions;
    private WorkingCopyContext _context;
    private LocalChange _change;
    private ConflictPreview? _preview;
    private CancellationTokenSource? _cancel;
    private bool _busy, _current, _incoming, _closed, _canSubmit;
    private string _text = "", _combinedText = "", _status = UiText.Get("ConflictLoading"), _output = "";
    private readonly Stopwatch _watch = new();
    private readonly DispatcherTimer _timer = new() { Interval = TimeSpan.FromMilliseconds(250) };
    public string Path => _change.Path;
    public string Title => UiText.Get("ConflictTitle");
    public string Status { get => _status; private set => Set(ref _status, value); }
    public string Output { get => _output; private set => Set(ref _output, value); }
    public bool Busy { get => _busy; private set { Set(ref _busy, value); Refresh(); } }
    public bool CanChoose => !Busy && _preview is not null;
    public bool CanEdit => CanChoose && _preview?.CanEdit == true;
    public bool Current { get => _current; set { if (Set(ref _current, value)) ChooseSides(); } }
    public bool Incoming { get => _incoming; set { if (Set(ref _incoming, value)) ChooseSides(); } }
    public bool HasEditorChanges { get; private set; }
    public string Text { get => _text; set { if (Set(ref _text, value)) { _combinedText = value; HasEditorChanges = true; _current = _incoming = true; Changed(nameof(Current)); Changed(nameof(Incoming)); Refresh(); } } }
    public IReadOnlyList<DiffRow> CurrentLines { get; private set; } = [];
    public IReadOnlyList<DiffRow> IncomingLines { get; private set; } = [];
    public bool IsText => _preview?.CanEdit == true;
    public bool HasPreview => _preview is not null;
    public string ResultLabel
    {
        get
        {
            var encoding = Current && Incoming ? _preview?.TextEncoding : Current ? _preview?.Current?.TextEncoding : Incoming ? _preview?.Incoming?.TextEncoding : null;
            return encoding is { } value ? UiText.Format("ConflictResultEncoding", UiText.Get("ConflictEncoding" + value)) : UiText.Get("ConflictResult");
        }
    }
    private void ChooseSides()
    {
        _text = Current && Incoming ? _combinedText : Current ? _preview?.Current?.Text ?? "" : Incoming ? _preview?.Incoming?.Text ?? "" : "";
        Changed(nameof(Text)); Refresh();
    }
    public string CurrentLabel => _preview is null ? "" : UiText.Format(_context.Repository.Recovery?.Kind == RecoveryKind.Rebase ? "ConflictRebasedSide" : "ConflictCurrentSide", _preview.CurrentSource);
    public string IncomingLabel => _preview is null ? "" : _context.Repository.Recovery?.Kind == RecoveryKind.Rebase ? UiText.Format("ConflictReplayedSide", _preview.IncomingSource) :
        _preview.IncomingSource.Length > 0 ? UiText.Format("ConflictIncomingSide", _preview.IncomingSource) : UiText.Get("ConflictIncomingUnknown");
    public string CurrentText => SideText(_preview?.Current);
    public string IncomingText => SideText(_preview?.Incoming);
    public string BaseText => SideText(_preview?.Base);
    private static string SideText(ConflictSide? side) => side is null ? UiText.Get("ConflictDeletedSide") : side.Text ?? UiText.Get("ConflictBinarySide");
    public string Help => UiText.Get(_preview is { CanEdit: false } ? "ConflictBinaryHelp" : "ConflictEditorHelp");
    public string ResolveHelp => UiText.Get(Busy ? "ConflictLoading" : !_canSubmit ? "ConflictReloadRequired" :
        !Current && !Incoming ? "ConflictChoose" : Current && Incoming && _preview?.CanEdit != true ? "ConflictBinaryHelp" : "ConflictResolveHelp");
    public string Duration => UiText.Format("ConflictDuration", _watch.Elapsed.TotalSeconds);
    public bool HasMutation { get; private set; }
    public bool Resolved { get; private set; }
    public RelayCommand ResolveCommand { get; }
    public RelayCommand ReloadCommand { get; }
    public RelayCommand CancelCommand { get; }
    public RelayCommand ExternalCommand { get; }
    public RelayCommand UseCurrentTextCommand { get; }
    public RelayCommand UseIncomingTextCommand { get; }
    public Task Work { get; private set; } = Task.CompletedTask;
    public event EventHandler? Completed;
    public ConflictViewModel(WorkingCopyContext context, LocalChange change, GitActions actions)
    {
        _context = context; _change = change; _actions = actions;
        ResolveCommand = new(() => Work = ResolveAsync(Current && Incoming ? ConflictChoice.Edited : Current ? ConflictChoice.Current : ConflictChoice.Incoming),
            () => CanChoose && _canSubmit && (Current || Incoming) && (!(Current && Incoming) || CanEdit));
        ExternalCommand = new(() => Work = ResolveAsync(ConflictChoice.External), () => CanChoose && _canSubmit);
        ReloadCommand = new(() => Work = LoadAsync(true), () => !Busy && !_closed);
        CancelCommand = new(() => _cancel?.Cancel(), () => Busy);
        UseCurrentTextCommand = new(() => Text = _preview?.Current?.Text ?? "", () => CanEdit);
        UseIncomingTextCommand = new(() => Text = _preview?.Incoming?.Text ?? "", () => CanEdit);
        _timer.Tick += (_, _) => Changed(nameof(Duration));
    }
    public Task StartAsync() => Work = LoadAsync(false);
    private async Task LoadAsync(bool fresh)
    {
        Begin();
        try
        {
            await Dispatcher.Yield(DispatcherPriority.Background);
            if (fresh)
            {
                _context = await _actions.ReadWorkingCopyAsync(_context.Git, _context.Identity.Worktree, _cancel!.Token);
                _change = _context.Changes.Entries.FirstOrDefault(e => e.Path == _change.Path && e.Group == ChangeGroup.Conflicted) ?? throw new GitActionException("ConflictNoLongerUnmerged");
            }
            _preview = await _actions.ReadConflictAsync(_context, _change, _cancel!.Token);
            if (_closed) return;
            Tools = [new("", UiText.Get("ConflictToolDefault")), .. _preview.Tools.Select(name => new ToolOption(name, name))];
            if (!Tools.Any(t => t.Name == ExternalTool)) ExternalTool = "";
            Changed(nameof(Tools));
            if (!fresh || !HasEditorChanges) { _combinedText = _text = _preview.WorkingText ?? ""; HasEditorChanges = false; _current = _incoming = true; }
            var sides = await System.Threading.Tasks.Task.Run(() => (Numbered(_preview.Current?.Text), Numbered(_preview.Incoming?.Text)));
            if (_closed) return;
            CurrentLines = sides.Item1; IncomingLines = sides.Item2;
            _canSubmit = true;
            foreach (var name in new[] { nameof(Text), nameof(Current), nameof(Incoming), nameof(CurrentLabel), nameof(IncomingLabel), nameof(CurrentText), nameof(IncomingText), nameof(BaseText), nameof(Help), nameof(CurrentLines), nameof(IncomingLines), nameof(IsText) }) Changed(name);
            Status = UiText.Get("ConflictChoose");
        }
        catch (Exception error) { _canSubmit = false; Status = Explain(error); }
        finally { End(); }
    }
    private static DiffRow[] Numbered(string? text) => text is null ? [] : text.Split('\n').Select((line, index) =>
        new DiffRow(new(DiffLineKind.Context, line.TrimEnd('\r'), index + 1, index + 1))).ToArray();
    private async Task ResolveAsync(ConflictChoice choice)
    {
        var resolution = new ConflictResolution(_preview!, choice, Text, ExternalTool); HasMutation = true; Begin(mutation: true);
        var outcome = OperationOutcome.Failed;
        var attempt = new object(); Journal?.Record(new(attempt, Title, UiText.Get("ConflictResolving"), 0, OperationOutcome.Running), _context.Identity.Worktree, false);
        var retryWithoutReload = false;
        Status = UiText.Get("ConflictResolving");
        try
        {
            await Dispatcher.Yield(DispatcherPriority.Background);
            var result = await _actions.ExecuteWorkingCopyAsync(_context, new(WorkingCopyAction.ResolveConflict, [_change], Resolution: resolution), _cancel!.Token);
            Output = result.Result.Output;
            Resolved = result.Result.Success && !result.Result.HasConflicts;
            outcome = Resolved ? OperationOutcome.Success : result.Result.Outcome == ProcessOutcome.Cancelled ? OperationOutcome.Cancelled : OperationOutcome.Failed;
            Status = UiText.Get(result.Result.MessageCode ?? (Resolved ? "ConflictResolved" : "ActionResult" + result.Result.Outcome));
        }
        catch (Exception error)
        {
            if (error is OperationCanceledException) outcome = OperationOutcome.Cancelled;
            retryWithoutReload = error is GitActionException { Code: "ConflictMarkersRemain" or "ConflictTextLimit" or "ConflictToolMissing" };
            Status = Explain(error);
        }
        finally
        {
            _canSubmit = retryWithoutReload; End();
            Journal?.Record(new(attempt, Title, Status, _watch.Elapsed.TotalSeconds, outcome), _context.Identity.Worktree, false, Output);
        }
        if (!_closed && !retryWithoutReload) Completed?.Invoke(this, EventArgs.Empty);
    }
    private void Begin(bool mutation = false) { _cancel = new(mutation ? TimeSpan.FromMinutes(10) : TimeSpan.FromSeconds(45)); Busy = true; _watch.Restart(); _timer.Start(); }
    private void End() { _watch.Stop(); _timer.Stop(); _cancel?.Dispose(); _cancel = null; Busy = false; Changed(nameof(Duration)); }
    private static string Explain(Exception error) => UiText.Get(error is GitActionException action ? action.Code : error is GitReadException read ? "GitError" + read.Failure :
        error is OperationCanceledException ? "ActionResultCancelled" : "ActionReadFailed");
    private void Refresh()
    {
        foreach (var command in new[] { ResolveCommand, ExternalCommand, ReloadCommand, CancelCommand, UseCurrentTextCommand, UseIncomingTextCommand }) command?.Refresh();
        Changed(nameof(CanChoose)); Changed(nameof(CanEdit)); Changed(nameof(ResolveHelp));
        Changed(nameof(HasPreview));
        Changed(nameof(ResultLabel));
    }
    public async Task CloseAsync() { _closed = true; _cancel?.Cancel(); await Work; _timer.Stop(); }
}
