using GitGui.App.Localization;
using GitGui.Core.Git;
using GitGui.App.Services;
using GitGui.App.Views;
using System.IO;
using System.Windows.Threading;

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
    public RelayCommand DiscardCommand { get; }
    public RelayCommand StashCommand { get; }
    public RelayCommand IgnoreCommand { get; }
    public RelayCommand IgnoreExtensionCommand { get; }
    public RelayCommand LfsTrackCommand { get; }
    public RelayCommand LfsUntrackCommand { get; }
    public RelayCommand ExternalDiffCommand { get; }
    public RelayCommand PatchCommand { get; }
    public RelayCommand HistoryCommand { get; }
    public RelayCommand BlameCommand { get; }
    private bool Supported => _selected.All(row => row.Change.Group is not (ChangeGroup.Conflicted or ChangeGroup.Ignored) && row.Change.HeadMode != "160000" && row.Change.IndexMode != "160000");
    private bool CanWrite => Current && _owner.Ready && _owner.CanUseFileMenu && _selected.Length > 0 && Supported;
    public string ActionHelp => UiText.Get(!Current ? "FileMenuStale" : !_owner.Ready || !_owner.CanUseFileMenu ? "CommitWaitForRead" : !Supported ? "FileActionUnsupported" : "FileActionHelp");
    public string StashHelp => UiText.Get(_context?.Repository.Commit is null ? "StashNeedsCommit" : _context.Repository.Operation != RepositoryOperation.None ? "ActionSequence" : "FileStashHelp");
    public string IgnoreHelp => UiText.Get(_selected.Any(row => row.Change.Group != ChangeGroup.Untracked) ? "FileIgnoreTracked" : "FileIgnoreHelp");
    public string InspectionHelp => UiText.Get(_selected.Length != 1 ? "FileMenuSingle" : "FileHistoryHelp");
    public string StashLabel => UiText.Format("FileStashLabel", _selected.Length);
    internal ChangeFileMenu(CommitViewModel owner, bool staged, ChangeRow[] selected, ChangeRow[] all)
    {
        _owner = owner; _context = owner.FileMenuContext; _staged = staged; _selected = selected; _all = all;
        StageCommand = new(() => owner.RunFileMenuAction(staged, _selected), () => Current && owner.Ready && _selected.Length > 0);
        AllCommand = new(() => owner.RunFileMenuAction(staged, _all), () => Current && owner.Ready && _all.Length > 0);
        OpenCommand = new(() => owner.OpenMenuFile(_selected[0], false), () => owner.CanUseFileMenu && _selected.Length == 1 && _selected[0].Change.Code != 'D');
        ExplorerCommand = new(() => owner.OpenMenuFile(_selected[0], true), () => owner.CanUseFileMenu && _selected.Length == 1);
        CopyCommand = new(() => owner.CopyMenuPaths(_selected, false), () => owner.CanUseFileMenu && _selected.Length > 0);
        CopyFullCommand = new(() => owner.CopyMenuPaths(_selected, true), () => owner.CanUseFileMenu && _selected.Length > 0);
        DiscardCommand = new(() => owner.RunExtendedFileAction(_context!, WorkingCopyAction.Discard, _selected), () => CanWrite);
        StashCommand = new(() => owner.RunExtendedFileAction(_context!, WorkingCopyAction.StashFiles, _selected), () => CanWrite && _context?.Repository is { Commit: not null, Operation: RepositoryOperation.None });
        IgnoreCommand = new(() => owner.RunExtendedFileAction(_context!, WorkingCopyAction.IgnoreFiles, _selected), () => CanWrite && _selected.All(row => row.Change.Group == ChangeGroup.Untracked));
        IgnoreExtensionCommand = new(() => owner.RunExtendedFileAction(_context!, WorkingCopyAction.IgnoreExtension, _selected), () => IgnoreCommand.CanExecute(null) && _selected.All(row => Path.GetExtension(row.Change.Path).Length > 0));
        LfsTrackCommand = new(() => owner.RunExtendedFileAction(_context!, WorkingCopyAction.LfsTrack, _selected), () => CanWrite);
        LfsUntrackCommand = new(() => owner.RunExtendedFileAction(_context!, WorkingCopyAction.LfsUntrack, _selected), () => CanWrite);
        ExternalDiffCommand = new(() => owner.RunExtendedFileAction(_context!, WorkingCopyAction.ExternalDiff, _selected), () => CanWrite && _selected.Length == 1);
        PatchCommand = new(() => owner.ExportMenuPatch(_context!, _selected), () => CanWrite);
        HistoryCommand = new(() => owner.InspectMenuFile(_context!, _selected[0], false), () => Current && owner.CanUseFileMenu && _context is not null && _selected.Length == 1);
        BlameCommand = new(() => owner.InspectMenuFile(_context!, _selected[0], true), () => HistoryCommand.CanExecute(null) && _context?.Repository.Commit is not null);
        owner.PropertyChanged += OnOwnerChanged;
    }
    private void OnOwnerChanged(object? sender, System.ComponentModel.PropertyChangedEventArgs args)
    {
        StageCommand.Refresh(); AllCommand.Refresh(); OpenCommand.Refresh(); ExplorerCommand.Refresh(); CopyCommand.Refresh(); CopyFullCommand.Refresh();
        Changed(nameof(MutationHelp)); Changed(nameof(OpenHelp)); Changed(nameof(ExplorerHelp));
        foreach (var command in new[] { DiscardCommand, StashCommand, IgnoreCommand, IgnoreExtensionCommand, LfsTrackCommand, LfsUntrackCommand, ExternalDiffCommand, PatchCommand, HistoryCommand, BlameCommand }) command.Refresh();
        Changed(nameof(ActionHelp)); Changed(nameof(StashHelp)); Changed(nameof(IgnoreHelp)); Changed(nameof(InspectionHelp));
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
    internal void RunExtendedFileAction(WorkingCopyContext context, WorkingCopyAction action, IReadOnlyList<ChangeRow> rows)
    {
        if (!ReferenceEquals(context, _state.Context) || !Ready) return;
        string message = "";
        if (action is WorkingCopyAction.Discard or WorkingCopyAction.StashFiles)
        {
            var staged = rows[0].Change.Group == ChangeGroup.Staged;
            var targets = rows.Select(row => new { Row = row, Label = row.Change.OriginalPath is { } old
                ? UiText.Format("FileActionPath", row.Path, new ChangeRow(row.Change with { Path = old }).Path) : row.Path })
                .Select(item => action == WorkingCopyAction.StashFiles ? UiText.Format("FileStashTarget", item.Label) :
                UiText.Format(staged ? context.Repository.Commit is null ? "FileDiscardNewTarget" : "FileDiscardStagedTarget" :
                    item.Row.Change.Group == ChangeGroup.Untracked ? "FileDiscardNewTarget" : "FileDiscardWorktreeTarget", item.Label)).ToArray();
            var response = _interaction.ConfirmFileAction(new(UiText.Get(action == WorkingCopyAction.Discard ? "FileDiscardTitle" : "FileStashTitle"),
                UiText.Format(action == WorkingCopyAction.Discard ? "FileDiscardExplanation" : "FileStashExplanation", context.Repository.Commit ?? UiText.Get("NoBranch"), Directory),
                targets, UiText.Get(action == WorkingCopyAction.Discard ? "FileDiscardButton" : "FileStashButton"), action == WorkingCopyAction.StashFiles));
            if (response is null) return;
            // Confirmation pumps WPF; a background refresh must not retarget reviewed files.
            if (!ReferenceEquals(context, _state.Context) || !Ready) { Status = UiText.Get("FileMenuStale"); return; }
            message = response;
        }
        StartAction(action, explicitEntries: rows, actionMessage: message);
    }
    internal void InspectMenuFile(WorkingCopyContext context, ChangeRow row, bool blame)
    {
        var model = new FileHistoryViewModel(context, row.Change, _actions, _reader, blame);
        _interaction.ShowFileHistory(model);
    }
    internal void ExportMenuPatch(WorkingCopyContext context, IReadOnlyList<ChangeRow> rows)
    {
        var destination = _interaction.ChoosePatchFile(); if (destination is null) return;
        if (!ReferenceEquals(context, _state.Context) || !Ready) { Status = UiText.Get("FileMenuStale"); return; }
        _fileBusy = true; _fileCancel = new(TimeSpan.FromMinutes(5)); Changed(nameof(CanUseFileMenu)); UpdateBusy(); Status = UiText.Get("FilePatchWriting");
        FileTask = Export();
        async Task Export()
        {
            try
            {
                var cancellation = _fileCancel.Token;
                await Dispatcher.Yield(DispatcherPriority.Background);
                var bytes = await _actions.ReadSelectedPatchAsync(context, rows.Select(row => row.Change).ToArray(), cancellation);
                await FileShellActions.SavePatchAsync(destination, bytes, Directory, GitActions.FileActionPaths(rows.Select(row => row.Change).ToArray()), cancellation);
                if (!_closed) Status = UiText.Get("FilePatchSaved");
            }
            catch (Exception error) { if (!_closed) { CaptureError(error); Status = Explain(error); } }
            finally { _fileCancel.Dispose(); _fileCancel = null; _fileBusy = false; Changed(nameof(CanUseFileMenu)); UpdateBusy(); }
        }
    }
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
        _fileBusy = true; _fileCancel = new(); Changed(nameof(CanUseFileMenu)); UpdateBusy(); Status = UiText.Get("FileMenuOpening");
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
            finally { _fileCancel.Dispose(); _fileCancel = null; _fileBusy = false; Changed(nameof(CanUseFileMenu)); UpdateBusy(); }
        }
    }
}
