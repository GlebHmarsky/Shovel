namespace GitGui.App.ViewModels;

public sealed partial class ShellViewModel
{
    private CommitFilesViewModel? _commitFiles;
    private string? _commitFilesPath;
    private Task _oldCommitFiles = Task.CompletedTask;
    public CommitFilesViewModel? CommitFiles { get => _commitFiles; private set => Set(ref _commitFiles, value); }
    private void UpdateCommitFiles()
    {
        if (_historyMenuOpen) return;
        var path = SelectedRepository?.Directory; var commit = SelectedCommit?.Commit;
        if (_stopping || ShowingStartupPreview || DetailsIndex != 4 || path is null || commit is null || _git is null || _reader is null)
        { StopCommitFiles(); return; }
        if (_commitFilesPath == path && CommitFiles?.CommitHash == commit.Hash) return;
        if (_commitFilesPath == path && CommitFiles is { } retained) { retained.SelectCommit(commit); return; }
        StopCommitFiles(); _commitFilesPath = path;
        var model = new CommitFilesViewModel(_reader, _git, path, commit) { Journal = Journal }; CommitFiles = model;
        StartAfterDrain();
        async void StartAfterDrain() { await _oldCommitFiles; if (ReferenceEquals(CommitFiles, model) && !_stopping) model.Start(); }
    }
    private void StopCommitFiles()
    {
        if (CommitFiles is not { } old) return;
        old.Stop(); _oldCommitFiles = Task.WhenAll(_oldCommitFiles, old.Pending); CommitFiles = null; _commitFilesPath = null;
    }
}
