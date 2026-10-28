using System.Windows.Threading;
using GitGui.App.Localization;
using GitGui.Core.Git;

namespace GitGui.App.ViewModels;

public sealed record FileHistoryRow(CommitRecord Commit)
{
    public string Hash => Commit.Hash[..Math.Min(8, Commit.Hash.Length)];
    public string Subject => Commit.Subject;
    public string Author => Commit.Author;
    public DateTime Date => Commit.CommittedAt.LocalDateTime;
}
public sealed record FileBlameRow(BlameLine Line)
{
    public string Hash => Line.Hash[..Math.Min(8, Line.Hash.Length)];
    public string Author => Line.Author;
    public DateTime Date => Line.Date.LocalDateTime;
    public int Number => Line.Number;
    public string Text => new DiffRow(new(DiffLineKind.Context, Line.Text)).Text;
    public string Help => UiText.Format("FileBlameHelp", Line.Hash, Line.Author, Line.Email, Line.Summary);
}

public sealed class FileHistoryViewModel(WorkingCopyContext context, LocalChange entry, GitActions actions, RepositoryReader reader, bool blame) : ObservableObject
{
    private int _mode = blame ? 1 : 0, _offset;
    private readonly Dictionary<int, FileHistoryCursor?> _cursors = new() { [0] = null };
    private bool _closed, _busy, _more;
    private long _generation;
    private CancellationTokenSource? _cancel;
    private FileHistoryRow? _selected;
    private IReadOnlyList<FileHistoryRow> _history = [];
    private IReadOnlyList<FileBlameRow> _blame = [];
    private CommitFilesViewModel? _files;
    private Task _oldFiles = Task.CompletedTask;
    private string _status = UiText.Get("FileHistoryLoading"), _output = "";
    public string Title => UiText.Format("FileHistoryTitle", entry.Path);
    public string Scope => UiText.Format("FileHistoryScope", context.Repository.Commit ?? UiText.Get("NoBranch"));
    public string Page => UiText.Format("FileHistoryPage", _offset + (History.Count > 0 ? 1 : 0), _offset + History.Count);
    public int Mode { get => _mode; set { if (Set(ref _mode, value)) Load(_offset); } }
    public bool Busy { get => _busy; private set { if (Set(ref _busy, value)) RefreshCommands(); } }
    public IReadOnlyList<FileHistoryRow> History { get => _history; private set => Set(ref _history, value); }
    public IReadOnlyList<FileBlameRow> Blame { get => _blame; private set => Set(ref _blame, value); }
    public string Status { get => _status; private set => Set(ref _status, value); }
    public string Output { get => _output; private set => Set(ref _output, value); }
    public CommitFilesViewModel? Files { get => _files; private set => Set(ref _files, value); }
    public FileHistoryRow? Selected
    {
        get => _selected;
        set
        {
            if (!Set(ref _selected, value)) return;
            Changed(nameof(Details)); Files?.Stop(); _oldFiles = Task.WhenAll(_oldFiles, Files?.Pending ?? Task.CompletedTask);
            Files = value is null ? null : new(reader, context.Git, context.Identity.Worktree, value.Commit) { PreferredPath = entry.OriginalPath ?? entry.Path };
            Files?.Start();
        }
    }
    public string Details => Selected is not { } row ? "" : UiText.Format("FileHistoryDetails", row.Commit.Hash, row.Commit.Author, row.Commit.AuthorEmail,
        row.Commit.AuthoredAt, row.Commit.Committer, row.Commit.CommitterEmail, row.Commit.CommittedAt, string.Join(", ", row.Commit.Parents), row.Commit.Message);
    public RelayCommand OlderCommand => new(() => Load(checked(_offset + 200)), () => !_closed && !Busy && _more);
    public RelayCommand NewerCommand => new(() => Load(Math.Max(0, _offset - 200)), () => !_closed && !Busy && _offset > 0 && _cursors.ContainsKey(_offset - 200));
    public RelayCommand FirstCommand => new(() => Load(0), () => !_closed && !Busy && _offset > 0);
    public RelayCommand RefreshCommand => new(() => Load(_offset), () => !_closed && !Busy);
    public RelayCommand CancelCommand => new(() => { _cancel?.Cancel(); Files?.CancelCommand.Execute(null); }, () => !_closed);
    public Task Pending { get; private set; } = Task.CompletedTask;
    public Task StartAsync() { Load(_offset); return Pending; }
    private void RefreshCommands() { Changed(nameof(OlderCommand)); Changed(nameof(NewerCommand)); Changed(nameof(FirstCommand)); Changed(nameof(RefreshCommand)); }
    private void Load(int offset)
    {
        if (_closed) return;
        _cancel?.Cancel(); var previous = Pending; var generation = ++_generation; var mode = Mode;
        var cancellation = new CancellationTokenSource(TimeSpan.FromSeconds(45)); _cancel = cancellation;
        Status = UiText.Get("FileHistoryLoading"); Output = ""; Busy = true;
        Pending = Read();
        async Task Read()
        {
            try
            {
                await Dispatcher.Yield(DispatcherPriority.Background); await previous;
                cancellation.Token.ThrowIfCancellationRequested();
                FileHistoryPage? page = null; FileHistoryRow[] history = []; FileBlameRow[] lines = [];
                if (mode == 0)
                {
                    page = await actions.ReadFileHistoryAsync(context, entry, offset, cancellation.Token, _cursors.GetValueOrDefault(offset));
                    history = await Task.Run(() => page.Commits.Select(item => new FileHistoryRow(item)).ToArray(), cancellation.Token);
                }
                else
                {
                    var raw = await actions.ReadFileBlameAsync(context, entry, cancellation.Token);
                    lines = await Task.Run(() => raw.Select(item => new FileBlameRow(item)).ToArray(), cancellation.Token);
                }
                if (_closed || generation != _generation || cancellation.IsCancellationRequested) return;
                if (page is not null)
                {
                    var selectedHash = Selected?.Commit.Hash; History = history; _offset = page.Offset; _more = page.HasMore;
                    if (page.Next is { } next) _cursors[next.Offset] = next;
                    while (_cursors.Count > 129) _cursors.Remove(_cursors.Keys.Where(key => key != 0 && key != _offset).Min());
                    Selected = History.FirstOrDefault(item => item.Commit.Hash == selectedHash) ?? History.FirstOrDefault(); Changed(nameof(Page));
                    Status = UiText.Get(History.Count == 0 ? "FileHistoryEmpty" : _more ? "FileHistoryMore" : "FileHistoryEnd");
                }
                else { Blame = lines; Status = UiText.Format("FileBlameCount", lines.Length); }
            }
            catch (Exception error)
            {
                if (!_closed && generation == _generation)
                {
                    Status = UiText.Get(error is GitActionException action ? action.Code : error is GitReadException read ? "GitError" + read.Failure : error is OperationCanceledException ? "ActionResultCancelled" : "ActionReadFailed");
                    if (error is GitReadException detail) Output = GitActions.Redact(detail.RawOutput);
                }
            }
            finally { cancellation.Dispose(); if (generation == _generation) { _cancel = null; Busy = false; } }
        }
    }
    public async Task CloseAsync()
    {
        _closed = true; _generation++; _cancel?.Cancel(); _cancel = null; Files?.Stop();
        await Pending; await _oldFiles; if (Files is not null) await Files.Pending;
    }
}
