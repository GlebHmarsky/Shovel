namespace GitGui.App.ViewModels;

public sealed partial class ShellViewModel
{
    // File actions outlive the comparison that launched them. Switching tabs must
    // not cancel a restore/tool or hold the next repository's comparison hostage.
    private readonly Dictionary<CommitFilesViewModel, RepositoryTab> _historyFileActions = [];
    private bool HasHistoryFileAction => _historyFileActions.Count != 0;
    private bool HistoryFileBusy(RepositoryTab tab) => _historyFileActions.Values.Any(owner => SameRepository(owner, tab));
    private void CancelSelectedHistoryFileActions()
    {
        foreach (var pair in _historyFileActions.Where(pair => pair.Value == SelectedRepository).ToArray()) pair.Key.CancelFileAction();
    }

    private void TrackHistoryFileAction(CommitFilesViewModel model, RepositoryTab tab, bool running)
    {
        if (running) _historyFileActions[model] = tab;
        else _historyFileActions.Remove(model);
        Changed(nameof(ActionOpen)); RefreshCommands(); CancelCommand.Refresh();
        if (!running) DrainActionQueue();
    }

    private void CancelHistoryFileActions()
    {
        foreach (var model in _historyFileActions.Keys.ToArray()) model.CancelFileAction();
    }

    private async Task DrainHistoryFileActionsAsync()
    {
        while (_historyFileActions.Count != 0)
            await Task.WhenAll(_historyFileActions.Keys.Select(model => model.FileActionTask).ToArray());
    }
}
