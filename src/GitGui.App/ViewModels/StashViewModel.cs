using System.Windows.Threading;
using GitGui.App.Localization;
using GitGui.App.Services;
using GitGui.Core.Git;

namespace GitGui.App.ViewModels;

public sealed record StashRow(StashEntry Entry)
{
    public string Label => UiText.Format("StashRow", Entry.Selector, Entry.Message, Entry.Date.LocalDateTime);
}
public sealed record StashComparison(string Label, CommitRecord Commit);
public sealed class StashCache
{
    public StashList? List; public string? SelectedHash; public string Message = "";
}

public sealed class StashViewModel : ObservableObject
{
    private readonly ActionContext _context;
    private readonly GitActions _actions;
    private readonly RepositoryReader _reader;
    private readonly IUserInteraction _interaction;
    private readonly StashCache _cache;
    private readonly Action<GitActionViewModel> _started;
    private CancellationTokenSource? _readCancel, _inspectCancel;
    private long _readGeneration, _inspectGeneration;
    private bool _closed, _running, _readFailed;
    private Task _read = Task.CompletedTask, _inspect = Task.CompletedTask, _oldDetails = Task.CompletedTask;
    private IReadOnlyList<StashRow> _rows;
    private StashRow? _selected;
    private IReadOnlyList<StashComparison> _comparisons = [];
    private StashComparison? _comparison;
    private CommitFilesViewModel? _files;
    private GitActionViewModel? _action;
    private string _status = UiText.Get("StashNotLoaded"), _output = "", _message, _details = "";
    public string Repository => _context.Identity.Worktree;
    public IReadOnlyList<StashRow> Rows { get => _rows; private set => Set(ref _rows, value); }
    public StashRow? Selected { get => _selected; set { if (Set(ref _selected, value)) { _cache.SelectedHash = value?.Entry.Hash; Inspect(); RefreshCommands(); } } }
    public IReadOnlyList<StashComparison> Comparisons { get => _comparisons; private set => Set(ref _comparisons, value); }
    public StashComparison? Comparison { get => _comparison; set { if (Set(ref _comparison, value)) ShowComparison(); } }
    public CommitFilesViewModel? Files { get => _files; private set => Set(ref _files, value); }
    public string Details { get => _details; private set => Set(ref _details, value); }
    public string Status { get => _status; private set => Set(ref _status, value); }
    public string Output { get => _output; private set => Set(ref _output, value); }
    public string Message { get => _message; set { if (Set(ref _message, value)) _cache.Message = value; } }
    public bool IncludeUntracked { get; set; }
    public bool RestoreIndex { get; set; }
    public bool CanEdit => !_closed && !_running;
    public GitActionViewModel? Action { get => _action; private set => Set(ref _action, value); }
    public bool HasMutation { get; private set; }
    public Task Running { get; private set; } = Task.CompletedTask;
    public RelayCommand CreateCommand { get; }
    public RelayCommand ApplyCommand { get; }
    public RelayCommand PopCommand { get; }
    public RelayCommand DropCommand { get; }
    public RelayCommand RefreshCommand { get; }
    public RelayCommand CancelCommand { get; }
    public StashViewModel(ActionContext context, GitActions actions, RepositoryReader reader, IUserInteraction interaction,
        StashCache cache, Action<GitActionViewModel> started)
    {
        _context = context; _actions = actions; _reader = reader; _interaction = interaction; _cache = cache; _started = started;
        _rows = cache.List?.Entries.Select(entry => new StashRow(entry)).ToArray() ?? []; _message = cache.Message;
        _selected = _rows.FirstOrDefault(row => row.Entry.Hash == cache.SelectedHash) ?? _rows.FirstOrDefault();
        _cache.SelectedHash = _selected?.Entry.Hash;
        CreateCommand = new(() => Run(StashAction.Create), () => CanEdit);
        ApplyCommand = new(() => Run(StashAction.Apply), CanSelectAction);
        PopCommand = new(() => Run(StashAction.Pop), CanSelectAction);
        DropCommand = new(() => Run(StashAction.Drop), CanSelectAction);
        RefreshCommand = new(() => _ = RefreshAsync(), () => CanEdit);
        CancelCommand = new(() => { _readCancel?.Cancel(); _inspectCancel?.Cancel(); Files?.CancelCommand.Execute(null); Action?.Cancel(); });
    }
    private bool CanSelectAction() => CanEdit && !_readFailed && Selected is not null && _cache.List is not null;
    private void RefreshCommands() { Changed(nameof(CanEdit)); CreateCommand.Refresh(); ApplyCommand.Refresh(); PopCommand.Refresh(); DropCommand.Refresh(); RefreshCommand.Refresh(); }
    public Task StartAsync() => RefreshAsync();
    public Task RefreshAsync()
    {
        if (_closed) return Task.CompletedTask;
        _readCancel?.Cancel(); var old = _read; var generation = ++_readGeneration;
        var cancel = new CancellationTokenSource(TimeSpan.FromSeconds(45)); _readCancel = cancel;
        Status = UiText.Get(Rows.Count == 0 ? "StashLoading" : "StashRefreshing");
        return _read = Read();
        async Task Read()
        {
            try
            {
                await Dispatcher.Yield(DispatcherPriority.Background); await old;
                var list = await _actions.ReadStashesAsync(_context.Git, _context.Identity, cancel.Token);
                var rows = await Task.Run(() => list.Entries.Select(entry => new StashRow(entry)).ToArray(), cancel.Token);
                if (_closed || generation != _readGeneration || cancel.IsCancellationRequested) return;
                var selectedHash = _cache.SelectedHash;
                _cache.List = list; _readFailed = false; Rows = rows;
                _selected = rows.FirstOrDefault(row => row.Entry.Hash == selectedHash) ?? rows.FirstOrDefault();
                _cache.SelectedHash = _selected?.Entry.Hash; Changed(nameof(Selected));
                Inspect();
                Status = UiText.Format("StashCount", rows.Length); Output = "";
            }
            catch (Exception error) { if (!_closed && generation == _readGeneration) { _readFailed = true; ShowError(error); } }
            finally { if (generation == _readGeneration) { _readCancel = null; RefreshCommands(); } cancel.Dispose(); }
        }
    }
    private void ClearInspection() { StopFiles(); Comparisons = []; _comparison = null; Changed(nameof(Comparison)); Details = ""; }
    private void StopFiles()
    {
        if (Files is { } old) { old.Stop(); _oldDetails = Task.WhenAll(_oldDetails, old.Pending); Files = null; }
    }
    private void Inspect()
    {
        _inspectCancel?.Cancel(); _inspectCancel = null; var old = _inspect; var generation = ++_inspectGeneration;
        if (_closed || _running || Selected is not { } selected) { ClearInspection(); return; }
        Files?.Pause(); Status = UiText.Get("StashLoading");
        var cancel = new CancellationTokenSource(TimeSpan.FromSeconds(45)); _inspectCancel = cancel;
        _inspect = Read();
        async Task Read()
        {
            try
            {
                await Dispatcher.Yield(DispatcherPriority.Background); await old;
                var data = await _actions.InspectStashAsync(_context.Git, _context.Identity, selected.Entry.Hash, cancel.Token);
                if (_closed || generation != _inspectGeneration || cancel.IsCancellationRequested) return;
                _comparisons = [new(UiText.Get("StashTracked"), data.Worktree with { Parents = [data.Worktree.Parents[0]] }),
                    new(UiText.Get("StashStaged"), data.Index), new(UiText.Get("StashUnstaged"), data.Worktree with { Parents = [data.Worktree.Parents[1]] }),
                    .. data.Untracked is { } untracked ? new[] { new StashComparison(UiText.Get("StashUntracked"), untracked) } : []];
                Details = selected.Entry.Hash + Environment.NewLine + data.Worktree.Message;
                Changed(nameof(Comparisons)); Comparison = Comparisons[0];
                Status = UiText.Format("StashCount", Rows.Count);
            }
            catch (Exception error) { if (!_closed && generation == _inspectGeneration) ShowError(error); }
            finally { if (generation == _inspectGeneration) _inspectCancel = null; cancel.Dispose(); }
        }
    }
    private void ShowComparison()
    {
        if (_closed || _running) { StopFiles(); return; }
        if (Comparison is not { } choice) return;
        if (Files is { } retained) { retained.SelectCommit(choice.Commit); return; }
        var model = new CommitFilesViewModel(_reader, _context.Git, Repository, choice.Commit) { ShowParentSelector = false }; Files = model;
        Start();
        async void Start() { await _oldDetails; if (!_closed && ReferenceEquals(Files, model)) model.Start(); }
    }
    private void Run(StashAction action)
    {
        var selected = Selected?.Entry; var list = _cache.List ?? new([], "");
        // Freeze the reviewed target before a modal confirmation pumps the dispatcher.
        if (action == StashAction.Drop && !_interaction.ConfirmRecovery(UiText.Format("StashConfirmDrop", Repository, selected!.Selector, selected.Hash, selected.Message), UiText.Get("ActionTitleStashDrop"))) return;
        var options = new StashOptions(action, list, selected, Message, IncludeUntracked, RestoreIndex);
        _running = true; HasMutation = true; _readGeneration++; _inspectGeneration++;
        _readCancel?.Cancel(); _readCancel = null; _inspectCancel?.Cancel(); _inspectCancel = null; StopFiles(); RefreshCommands();
        var model = new GitActionViewModel(_context, action switch { StashAction.Create => GitAction.StashCreate, StashAction.Apply => GitAction.StashApply, StashAction.Pop => GitAction.StashPop, _ => GitAction.StashDrop },
            _actions, async () => { await RefreshAsync(); if (_readFailed) throw new System.IO.IOException(); },
            execute: (token, output) => _actions.ExecuteStashAsync(_context, options, token, output));
        Action = model; _started(model); model.RunCommand.Execute(null); Running = Finish();
        async Task Finish()
        {
            try { await model.RunningTask; if (model.Succeeded && action == StashAction.Create) Message = ""; }
            finally { _running = false; RefreshCommands(); if (!_closed) Inspect(); }
        }
    }
    private void ShowError(Exception error)
    {
        Status = UiText.Get(error is GitActionException action ? action.Code : error is GitReadException read ? "GitError" + read.Failure : error is OperationCanceledException ? "ActionResultCancelled" : "ActionReadFailed");
        if (error is GitReadException failure) Output = GitActions.Redact(failure.RawOutput);
    }
    private Task? _close;
    public async Task WaitForIdleAsync()
    {
        await _read; await _inspect;
        if (Files is { } files) { Task pending; do { pending = files.Pending; await pending; } while (pending != files.Pending); }
    }
    public Task CloseAsync() => _close ??= CloseCoreAsync();
    private async Task CloseCoreAsync()
    {
        _closed = true; _readGeneration++; _inspectGeneration++;
        _readCancel?.Cancel(); _readCancel = null; _inspectCancel?.Cancel(); _inspectCancel = null;
        StopFiles(); Action?.Cancel(); RefreshCommands();
        await Running; await Task.WhenAll(_read, _inspect, _oldDetails);
    }
}
