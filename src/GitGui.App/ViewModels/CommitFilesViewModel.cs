using System.Windows.Threading;
using GitGui.App.Localization;
using GitGui.Core.Git;

namespace GitGui.App.ViewModels;

public sealed record ParentChoice(string? Hash, string Label);
public sealed class CommitFilesViewModel(RepositoryReader reader, GitInstallation git, string path, CommitRecord commit) : ObservableObject
{
    private CancellationTokenSource? _cancel;
    private long _generation;
    private CommitFiles? _comparison;
    private bool _closed;
    private ParentChoice? _parent;
    private ChangeRow? _selected;
    private IReadOnlyList<ChangeRow> _files = [];
    private IReadOnlyList<DiffRow> _lines = [];
    private string _status = UiText.Get("CommitFilesLoading"), _output = "", _notice = UiText.Get("DiffChooseFile");
    private bool _busy;
    private BinaryComparison? _binary;
    public BinaryComparison? Binary { get => _binary; private set { if (Set(ref _binary, value)) Changed(nameof(HasBinary)); } }
    public bool HasBinary => Binary is not null;
    public IReadOnlyList<ParentChoice> Parents { get; } = commit.Parents.Count == 0
        ? [new(null, UiText.Get("CommitFilesRoot"))]
        : commit.Parents.Select((hash, i) => new ParentChoice(hash, UiText.Format("CommitFilesParent", i + 1, hash))).ToArray();
    public string CommitHash => commit.Hash;
    public ParentChoice? Parent { get => _parent; set { if (Set(ref _parent, value)) Refresh(); } }
    public ChangeRow? Selected { get => _selected; set { if (Set(ref _selected, value)) LoadDiff(); } }
    public IReadOnlyList<ChangeRow> Files { get => _files; private set => Set(ref _files, value); }
    public IReadOnlyList<DiffRow> Lines { get => _lines; private set => Set(ref _lines, value); }
    public string Status { get => _status; private set => Set(ref _status, value); }
    public string Output { get => _output; private set => Set(ref _output, value); }
    public string Notice { get => _notice; private set => Set(ref _notice, value); }
    public bool Busy { get => _busy; private set => Set(ref _busy, value); }
    public RelayCommand RefreshCommand => new(Refresh);
    public RelayCommand CancelCommand => new(() => _cancel?.Cancel());
    public Task Pending { get; private set; } = Task.CompletedTask;
    public void Start() { if (_parent is null) Parent = Parents[0]; }
    public void Stop() { _closed = true; _generation++; _cancel?.Cancel(); }
    private void Refresh()
    {
        if (_closed || Parent is null) return;
        var priorParent = _comparison?.Parent; var parent = Parent.Hash;
        if (_comparison is not null && priorParent != parent) { Files = []; Lines = []; Binary = null; _selected = null; Changed(nameof(Selected)); }
        _comparison = null;
        Queue(async token =>
        {
            var comparison = await reader.ReadCommitFilesAsync(git, path, commit.Hash, parent, token);
            var rows = await Task.Run(() => comparison.Files.Select(file => new ChangeRow(file)).ToArray(), token);
            return () => { _comparison = comparison; Files = rows; Status = UiText.Format("CommitFilesCount", rows.Length); if (rows.Length == 0) { Lines = []; Notice = UiText.Get("DiffNoTextChanges"); } };
        }, after: () => { var old = Selected?.Change.Path; _selected = null; Selected = Files.FirstOrDefault(f => f.Change.Path == old) ?? Files.FirstOrDefault(); });
    }
    private void LoadDiff()
    {
        if (_closed || Selected is not { } row || _comparison is not { } comparison) return;
        Lines = []; Binary = null; Notice = UiText.Get("DiffLoading");
        Queue(async token =>
        {
            var diff = await reader.ReadCommitFileDiffAsync(git, path, comparison, row.Change, token);
            var lines = await Task.Run(() => diff.Lines.Where(line => line.Kind != DiffLineKind.Metadata || line.Text.StartsWith("\\ No newline", StringComparison.Ordinal)).Select(line => new DiffRow(line)).ToArray(), token);
            return () => { Lines = lines; Binary = BinaryComparison.Create(diff, row.Change); Output = diff.RawPatch; Notice = UiText.Format("DiffSummary" + diff.Kind,
                diff.OldBytes?.ToString("N0") ?? UiText.Get("CountUnavailable"), diff.NewBytes?.ToString("N0") ?? UiText.Get("CountUnavailable"), diff.Added, diff.Removed); };
        });
    }
    private void Queue(Func<CancellationToken, Task<Action>> read, Action? after = null)
    {
        _cancel?.Cancel(); var previous = Pending; var generation = ++_generation;
        var cancellation = new CancellationTokenSource(TimeSpan.FromSeconds(45)); _cancel = cancellation;
        Busy = true; Output = ""; Status = UiText.Get("CommitFilesLoading");
        Pending = Run();
        async Task Run()
        {
            var success = false;
            try
            {
                await Dispatcher.Yield(DispatcherPriority.Background); await previous;
                cancellation.Token.ThrowIfCancellationRequested();
                var publish = await read(cancellation.Token);
                if (_closed || generation != _generation || cancellation.IsCancellationRequested) return;
                publish(); success = true;
                if (after is null) Status = UiText.Format("CommitFilesCount", Files.Count);
            }
            catch (Exception error)
            {
                if (!_closed && generation == _generation)
                {
                    Status = UiText.Get(error is GitReadException failure ? "GitError" + failure.Failure : error is OperationCanceledException ? "ActionResultCancelled" : "ActionReadFailed");
                    if (error is GitReadException readError) Output = GitActions.Redact(readError.RawOutput);
                }
            }
            finally
            {
                if (generation == _generation) { _cancel = null; Busy = false; }
                cancellation.Dispose();
            }
            if (success && after is not null && !_closed && generation == _generation) after();
        }
    }
}
