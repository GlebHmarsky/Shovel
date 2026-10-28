using GitGui.App.Localization;
using GitGui.Core.Git;

namespace GitGui.App.ViewModels;

/// <summary>One menu invocation: targets never follow a later selection or refreshed list.</summary>
public sealed class ChangeFileMenu : ObservableObject, IDisposable
{
    private readonly CommitViewModel _owner;
    private readonly WorkingCopyContext? _context;
    private readonly ChangeRow[] _selected, _all;
    private readonly bool _staged;
    public string StageLabel => UiText.Get(_staged ? "CommitUnstage" : "CommitStage");
    public string AllLabel => UiText.Get(_staged ? "CommitUnstageAll" : "CommitStageAll");
    public string SelectionHelp => UiText.Format("FileMenuSelection", _selected.Length);
    public string MutationHelp => UiText.Get(!Current ? "FileMenuStale" : !_owner.Ready ? "CommitWaitForRead" : "FileMenuStageHelp");
    public string OpenHelp => UiText.Get(!_owner.CanUseFileMenu ? "FileMenuBusy" : _selected.Length != 1 ? "FileMenuSingle" : _selected[0].Change.Code == 'D' ? "FileMenuDeleted" : "FileMenuOpenHelp");
    public string ExplorerHelp => UiText.Get(!_owner.CanUseFileMenu ? "FileMenuBusy" : _selected.Length != 1 ? "FileMenuSingle" : "FileMenuExplorerHelp");
    private bool Current => ReferenceEquals(_context, _owner.FileMenuContext);
    public RelayCommand StageCommand { get; }
    public RelayCommand AllCommand { get; }
    public RelayCommand OpenCommand { get; }
    public RelayCommand ExplorerCommand { get; }
    public RelayCommand CopyCommand { get; }
    public RelayCommand CopyFullCommand { get; }
    internal ChangeFileMenu(CommitViewModel owner, bool staged, ChangeRow[] selected, ChangeRow[] all)
    {
        _owner = owner; _context = owner.FileMenuContext; _staged = staged; _selected = selected; _all = all;
        StageCommand = new(() => owner.RunFileMenuAction(staged, _selected), () => Current && owner.Ready && _selected.Length > 0);
        AllCommand = new(() => owner.RunFileMenuAction(staged, _all), () => Current && owner.Ready && _all.Length > 0);
        OpenCommand = new(() => owner.OpenMenuFile(_selected[0], false), () => owner.CanUseFileMenu && _selected.Length == 1 && _selected[0].Change.Code != 'D');
        ExplorerCommand = new(() => owner.OpenMenuFile(_selected[0], true), () => owner.CanUseFileMenu && _selected.Length == 1);
        CopyCommand = new(() => owner.CopyMenuPaths(_selected, false), () => owner.CanUseFileMenu && _selected.Length > 0);
        CopyFullCommand = new(() => owner.CopyMenuPaths(_selected, true), () => owner.CanUseFileMenu && _selected.Length > 0);
        owner.PropertyChanged += OnOwnerChanged;
    }
    private void OnOwnerChanged(object? sender, System.ComponentModel.PropertyChangedEventArgs args)
    {
        StageCommand.Refresh(); AllCommand.Refresh(); OpenCommand.Refresh(); ExplorerCommand.Refresh(); CopyCommand.Refresh(); CopyFullCommand.Refresh();
        Changed(nameof(MutationHelp)); Changed(nameof(OpenHelp)); Changed(nameof(ExplorerHelp));
    }
    public void Dispose() => _owner.PropertyChanged -= OnOwnerChanged;
}

public sealed partial class CommitViewModel
{
    private bool _fileMenuOpen, _fileMenuDiffPending, _fileBusy;
    private CancellationTokenSource? _fileCancel;
    internal WorkingCopyContext? FileMenuContext => _state.Context;
    internal bool CanUseFileMenu => !_closed && !_mutating && !_fileBusy;
    public Task FileTask { get; private set; } = Task.CompletedTask;
    public void BeginFileMenu() => _fileMenuOpen = true;
    public void EndFileMenu()
    {
        _fileMenuOpen = false;
        if (_fileMenuDiffPending) { _fileMenuDiffPending = false; if (!_closed && !_mutating) QueueDiff(_state.Selected); }
    }
    public ChangeFileMenu CreateFileMenu(bool staged) => new(this, staged,
        (staged ? _stagedSelection : _unstagedSelection).ToArray(), (staged ? Staged : Unstaged).ToArray());
    internal void RunFileMenuAction(bool staged, IReadOnlyList<ChangeRow> rows) =>
        StartAction(staged ? WorkingCopyAction.Unstage : WorkingCopyAction.Stage, explicitEntries: rows);
    internal void CopyMenuPaths(IReadOnlyList<ChangeRow> rows, bool full)
    {
        try
        {
            var paths = rows.Select(row => full ? RepositoryReader.WorkingCopyPath(Directory, row.Change.Path) : row.Change.Path);
            _interaction.CopyFilePaths(string.Join(Environment.NewLine, paths)); Status = UiText.Get("FileMenuCopied");
        }
        catch (Exception) { Status = UiText.Get("FileMenuCopyFailed"); }
    }
    internal void OpenMenuFile(ChangeRow row, bool explorer)
    {
        _fileBusy = true; _fileCancel = new(); Changed(nameof(CanUseFileMenu)); Status = UiText.Get("FileMenuOpening");
        FileTask = Open();
        async Task Open()
        {
            try
            {
                var folder = await _interaction.OpenWorkingFileAsync(Directory, row.Change.Path, explorer, _fileCancel.Token);
                if (!_closed) Status = UiText.Get(folder ? "FileMenuOpenedFolder" : "FileMenuOpened");
            }
            catch (Exception error)
            {
                if (!_closed) Status = UiText.Get(error is GitActionException action ? action.Code : error is OperationCanceledException ? "ActionResultCancelled" : "FileMenuOpenFailed");
            }
            finally { _fileCancel.Dispose(); _fileCancel = null; _fileBusy = false; Changed(nameof(CanUseFileMenu)); }
        }
    }
}
