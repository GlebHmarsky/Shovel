using System.IO;
using System.Text;
using GitGui.Core.Processes;
using System.Windows.Threading;
using GitGui.App.Localization;
using GitGui.App.Services;
using GitGui.Core.Git;

namespace GitGui.App.ViewModels;

public sealed record HistoryResetChoice(string Label, string? Revision, string Path, RelayCommand Command);
public sealed class HistoryFileMenu
{
    public RelayCommand OpenCommand { get; }
    public RelayCommand DiffCommand { get; }
    public RelayCommand ExplorerCommand { get; }
    public RelayCommand HistoryCommand { get; }
    public RelayCommand BlameCommand { get; }
    public RelayCommand TreeCommand { get; }
    public RelayCommand SaveCommand { get; }
    public RelayCommand CopyCommand { get; }
    public RelayCommand CopyFullCommand { get; }
    public RelayCommand LfsTrackCommand { get; }
    public RelayCommand LfsUntrackCommand { get; }
    public IReadOnlyList<HistoryResetChoice> Resets { get; }
    public HistoryFileMenu(CommitFilesViewModel owner, LocalChange file, string revision, string? parent)
    {
        var present = file.IndexMode is "100644" or "100755";
        bool Available() => owner.CanUseHistoryMenu;
        OpenCommand = new(() => owner.RunHistoryAction("open", revision, parent, file), () => Available() && present);
        DiffCommand = new(() => owner.RunHistoryAction("diff", revision, parent, file), Available);
        ExplorerCommand = new(() => owner.RunHistoryAction("explorer", revision, parent, file), Available);
        HistoryCommand = new(() => owner.RunHistoryAction("history", revision, parent, file), Available);
        BlameCommand = new(() => owner.RunHistoryAction("blame", revision, parent, file), () => Available() && (present || parent is not null));
        TreeCommand = new(() => owner.BrowseTree(revision, file.Path.Contains('/') ? file.Path[..file.Path.LastIndexOf('/')] : "", file.Path), Available);
        SaveCommand = new(() => owner.RunHistoryAction("save", revision, parent, file), () => Available() && present);
        CopyCommand = new(() => owner.CopyHistoryPath(file.Path, false), Available);
        CopyFullCommand = new(() => owner.CopyHistoryPath(file.Path, true), Available);
        LfsTrackCommand = new(() => owner.RunHistoryAction("lfs-track", revision, parent, file), Available);
        LfsUntrackCommand = new(() => owner.RunHistoryAction("lfs-untrack", revision, parent, file), Available);
        var choices = new List<HistoryResetChoice>();
        void Add(string label, string? hash, string relative) => choices.Add(new(label, hash, relative,
            new(() => owner.RunHistoryAction("restore", hash, parent, file with { Path = relative }), Available)));
        Add(UiText.Get("HistoryFileResetHead"), null, file.Path);
        Add(UiText.Format("HistoryFileResetCommit", revision[..8]), revision, file.Path);
        if (parent is not null) Add(UiText.Format("HistoryFileResetParent", parent[..8]), parent, file.OriginalPath ?? file.Path);
        Resets = choices;
    }
}

public sealed partial class CommitFilesViewModel
{
    public GitActions? Actions { get; init; }
    public string? ExpectedIdentity { get; init; }
    public IUserInteraction? Interaction { get; init; }
    public Func<Task>? AfterFileMutation { get; init; }
    public Action<CommitFilesViewModel, bool>? FileActionStateChanged { get; init; }
    public Func<string, string, bool, Func<Task>, Task>? ScheduleFileAction { get; init; }
    public Action? FileActionFailed { get; init; }
    public bool FileActionBusy => _fileActionBusy;
    public bool FileActionMutating { get; private set; }
    private readonly Dictionary<string, Task> _pendingFileActions = [];
    private bool _historyMenuOpen, _historyMenuDiffPending;
    public void BeginHistoryMenu() => _historyMenuOpen = true;
    public void EndHistoryMenu() { _historyMenuOpen = false; if (_historyMenuDiffPending) { _historyMenuDiffPending = false; if (!_closed) LoadDiff(); } }
    private bool _fileActionBusy, _treeMode;
    private string _treeDirectory = "", _treeRevision = "";
    public Task FileActionTask { get; private set; } = Task.CompletedTask;
    public bool CanUseHistoryMenu => !_closed && Actions is not null && Interaction is not null;
    public bool TreeMode { get => _treeMode; private set => Set(ref _treeMode, value); }
    public string TreeDirectory { get => _treeDirectory; private set => Set(ref _treeDirectory, value); }
    public RelayCommand TreeUpCommand => new(() => BrowseTree(_treeRevision, TreeDirectory.Contains('/') ? TreeDirectory[..TreeDirectory.LastIndexOf('/')] : ""), () => CanUseHistoryMenu && TreeDirectory.Length > 0);
    public RelayCommand TreeExitCommand => new(Refresh, () => !_closed);
    public HistoryFileMenu? CreateHistoryFileMenu(ChangeRow row) => row.Change.IndexMode != "040000" && _comparison is { } comparison && comparison.Files.Contains(row.Change) && CanUseHistoryMenu
        ? new(this, row.Change, comparison.Commit, comparison.Parent) : null;
    public void OpenTreeDirectory(ChangeRow row) { if (TreeMode && row.Change.IndexMode == "040000") BrowseTree(_treeRevision, row.Change.Path); }
    public void BrowseTree(string revision, string directory, string? selectedPath = null)
    {
        if (!CanUseHistoryMenu) return;
        Queue(async token =>
        {
            var entries = await Actions!.ReadHistoricalDirectoryAsync(git, path, revision, directory, token);
            var comparison = await reader.ReadCommitFilesAsync(git, path, revision, _parent?.Hash, token);
            var files = entries.Select(entry => comparison.Files.FirstOrDefault(file => file.Path == entry.Path) ?? new LocalChange(ChangeGroup.Staged, entry.Path, 'M')
                { IndexMode = entry.Mode, IndexObject = entry.Hash, HeadMode = entry.Mode, HeadObject = entry.Hash }).ToArray();
            return () => { TreeMode = true; _treeRevision = revision; TreeDirectory = directory; Changed(nameof(TreeUpCommand));
                _comparison = new(revision, _parent?.Hash, files); Files = files.Select(file => new ChangeRow(file)).ToArray(); _selected = null;
                Status = UiText.Format("HistoryFileTreeScope", revision, directory); };
        }, after: () => { if (selectedPath is not null) Selected = Files.FirstOrDefault(file => file.Change.Path == selectedPath); });
    }
    public void CopyHistoryPath(string relative, bool full)
    {
        try { Interaction!.CopyFilePaths(full ? RepositoryReader.WorkingCopyPath(path, relative) : relative); Status = UiText.Get("FileMenuCopied"); }
        catch { Status = UiText.Get("FileMenuCopyFailed"); }
    }
    public void RunHistoryAction(string action, string? revision, string? parent, LocalChange file)
    {
        if (!CanUseHistoryMenu) return;
        var saveTo = action == "save" ? Interaction!.ChooseHistorySaveFile(System.IO.Path.GetFileName(file.Path)) : null;
        if (action == "save" && saveTo is null) return;
        var key = "HistoryFile|" + action + "|" + revision + "|" + file.Path;
        if (_pendingFileActions.ContainsKey(key)) return;
        var previous = Task.WhenAll(_pendingFileActions.Values);
        var completion = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        _pendingFileActions.Add(key, completion.Task); FileActionTask = Task.WhenAll(_pendingFileActions.Values);
        _ = Schedule();
        async Task Schedule()
        {
            try
            {
                if (ScheduleFileAction is not null) await ScheduleFileAction(key, UiText.Format("HistoryFileAction", UiText.Get("HistoryFileWorking"), file.Path), action is "restore" or "lfs-track" or "lfs-untrack", async () => { await previous; await Run(); });
                else { await previous; await Run(); }
            }
            finally { _pendingFileActions.Remove(key); completion.TrySetResult(); }
        }
        async Task Run()
        {
            using var cancellation = new CancellationTokenSource();
            _fileActionBusy = true; Changed(nameof(FileActionBusy));
            FileActionMutating = action is "restore" or "lfs-track" or "lfs-untrack";
            FileActionStateChanged?.Invoke(this, true);
            using var lifetime = GitGui.Core.Processes.ProcessLifetime.CompleteStartedCommands();
            var label = UiText.Get(action switch { "restore" => "HistoryFileReset", "diff" => "FileExternalDiff", "history" => "HistoryFileHistory", "blame" => "HistoryFileBlame",
                "explorer" => "FileMenuExplorer", "save" => "HistoryFileSave", "lfs-track" => "FileLfsTrack", "lfs-untrack" => "FileLfsUntrack", _ => "FileMenuOpen" });
            var title = UiText.Format("HistoryFileAction", label, file.Path); var attempt = new object(); var outcome = OperationOutcome.Failed;
            var transcript = new CommandTranscript();
            var dispatcher = Dispatcher.CurrentDispatcher;
            var streamed = new List<byte>(); var nativeText = new List<byte>();
            var publishScheduled = false; var finished = false;
            // cat-file stdout is exported repository content, never activity text.
            var captureStdout = action is "restore" or "diff";
            var exporting = action is "open" or "save";
            string TextSnapshot(List<byte> bytes) { lock (streamed) return GitActions.Redact(Encoding.UTF8.GetString(bytes.ToArray())); }
            void Progress(ProcessOutput chunk)
            {
                transcript.Collect(chunk);
                if (!chunk.IsCommand && !chunk.IsError && !captureStdout) return;
                lock (streamed)
                {
                    if (finished) return;
                    var available = ActivityJournal.OutputLimit - streamed.Count;
                    if (available > 0) streamed.AddRange(chunk.Bytes.Take(available));
                    if (!chunk.IsCommand)
                    {
                        available = ActivityJournal.OutputLimit - nativeText.Count;
                        if (available > 0) nativeText.AddRange(chunk.Bytes.Take(available));
                    }
                    if (publishScheduled) return;
                    publishScheduled = true;
                }
                dispatcher.BeginInvoke(DispatcherPriority.Background, new Action(() =>
                {
                    string snapshot;
                    lock (streamed)
                    {
                        publishScheduled = false;
                        if (finished) return;
                        snapshot = TextSnapshot(streamed);
                    }
                    // This attempt owns its journal entry even after another
                    // commit/file changes the shared comparison's Output property.
                    Journal?.Record(new(attempt, title, UiText.Get("HistoryFileWorking"), 0, OperationOutcome.Running), path, false, snapshot);
                }));
            }
            var actionOutput = ""; string? finalMessage = null;
            Status = UiText.Get("HistoryFileWorking"); Output = "";
            Journal?.Record(new(attempt, title, Status, 0, OperationOutcome.Running), path, false);
            try
            {
                await Dispatcher.Yield(DispatcherPriority.Background); var token = cancellation.Token;
                if (ExpectedIdentity is not null) await Actions!.ValidateHistoricalIdentityAsync(path, ExpectedIdentity, token);
                if (action is "restore" or "lfs-track" or "lfs-untrack")
                {
                    var review = await Actions!.ReviewHistoricalFileAsync(git, path, revision, file.Path, token);
                    if (action == "restore" && !Interaction!.ConfirmRecovery(UiText.Format("HistoryFileResetConfirm", file.Path, review.Revision, path), UiText.Get("HistoryFileReset")))
                    { outcome = OperationOutcome.Cancelled; return; }
                    var result = await Actions.ExecuteHistoricalFileAsync(review, action == "restore" ? HistoricalFileMutation.Restore : action == "lfs-track" ? HistoricalFileMutation.LfsTrack : HistoricalFileMutation.LfsUntrack, token, Progress);
                    actionOutput = result.Output; outcome = result.Success ? OperationOutcome.Success : result.Outcome == GitGui.Core.Processes.ProcessOutcome.Cancelled ? OperationOutcome.Cancelled : OperationOutcome.Failed;
                    if (result.MessageCode is { } message) finalMessage = UiText.Get(message);
                    if (AfterFileMutation is not null) await AfterFileMutation();
                }
                else if (action == "diff")
                { var result = await Actions!.DiffHistoricalFileAsync(git, path, revision!, parent, file, token, Progress); actionOutput = result.Output; outcome = result.Success ? OperationOutcome.Success : result.Outcome == GitGui.Core.Processes.ProcessOutcome.Cancelled ? OperationOutcome.Cancelled : OperationOutcome.Failed; }
                else if (action is "history" or "blame")
                {
                    var source = action == "blame" && file.Code == 'D' ? parent ?? revision! : revision!;
                    var relative = file.Code == 'D' ? file.OriginalPath ?? file.Path : file.Path;
                    var context = await Actions!.ReadHistoricalInspectionAsync(git, path, source, relative, token);
                    Interaction!.ShowFileHistory(new(context, context.Changes.Entries[0], Actions, reader, action == "blame")); outcome = OperationOutcome.Success;
                }
                else if (action == "explorer") { await Interaction!.OpenWorkingFileAsync(path, file.Path, true, token); outcome = OperationOutcome.Success; }
                else
                {
                    var bytes = await Actions!.ReadHistoricalContentAsync(git, path, revision!, file.Path, token, Progress);
                    if (action == "save") await FileShellActions.SavePatchAsync(saveTo!, bytes, path, [file.Path], token);
                    else await Interaction!.OpenHistoricalFileAsync(file.Path, bytes, token);
                    outcome = OperationOutcome.Success;
                }
            }
            catch (Exception error)
            {
                outcome = error is OperationCanceledException ? OperationOutcome.Cancelled : OperationOutcome.Failed;
                finalMessage = UiText.Get(error is GitActionException known ? known.Code : error is GitReadException read ? "GitError" + read.Failure : error is OperationCanceledException ? "ActionResultCancelled" : "ActionReadFailed");
                if (error is GitReadException detail) actionOutput = exporting ? TextSnapshot(nativeText) : GitActions.Redact(detail.RawOutput);
            }
            finally
            {
                lock (streamed) finished = true;
                var finalOutput = transcript.Complete(actionOutput.Length > 0 ? actionOutput : TextSnapshot(nativeText));
                Output = finalOutput;
                Status = finalMessage ?? UiText.Get(outcome == OperationOutcome.Success ? "HistoryFileDone" : outcome == OperationOutcome.Cancelled ? "ActionResultCancelled" : "ActionResultExited");
                Journal?.Record(new(attempt, title, Status, 0, outcome), path, false, finalOutput);
                if (outcome is OperationOutcome.Failed or OperationOutcome.Conflict or OperationOutcome.RefreshFailed) FileActionFailed?.Invoke();
                _fileActionBusy = false; Changed(nameof(FileActionBusy));
                FileActionStateChanged?.Invoke(this, false);
                if (OperationAttention.Required(outcome, finalOutput)) Interaction?.ShowOperationProblem(path, title, Status, finalOutput);
            }
        }
    }
}
