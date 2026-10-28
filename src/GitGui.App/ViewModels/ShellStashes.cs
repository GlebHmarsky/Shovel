using GitGui.Core.Git;

namespace GitGui.App.ViewModels;

public sealed partial class ShellViewModel
{
    public RelayCommand StashesCommand { get; private set; } = null!;
    public Task StashDialogTask { get; private set; } = Task.CompletedTask;
    private void InitializeStashes() => StashesCommand = new(() => StashDialogTask = OpenStashesAsync(),
        () => CanRead && _git is not null && _actions is not null && _interaction is not null && SelectedRepository?.Snapshot is not null);
    private async Task OpenStashesAsync()
    {
        var tab = SelectedRepository!; var snapshot = tab.Snapshot!;
        if (!_knownIdentities.TryGetValue(tab.Directory, out var known)) return;
        CancelCountRead(); SetActionOpen(true); _preparingAction = false;
        var context = new ActionContext(_git!, new(snapshot.Worktree, snapshot.GitDirectory, snapshot.CommonDirectory, known.Fingerprint), snapshot,
            tab.History.Page?.Session ?? new(snapshot, HistoryScope.AllCommits, "", [], false), [], "");
        var model = new StashViewModel(context, _actions!, _reader!, _interaction!, tab.Stashes, SetLastAction);
        try { await _interaction!.ShowStashesAsync(model); }
        finally
        {
            await model.CloseAsync();
            if (model.HasMutation)
                foreach (var related in Repositories.Where(item => item.Snapshot?.CommonDirectory == snapshot.CommonDirectory))
                { related.History.NeedsRefresh = true; related.History.ValidatedAt = default; related.Changes.ReadAt = default; if (related != tab) related.Stashes.List = null; }
            foreach (var other in Repositories.Where(item => item != tab).OrderByDescending(item => item.History.LastUse).Skip(2)) other.Stashes.List = null;
            SetActionOpen(false);
            if (model.HasMutation && !_stopping) BeginInspection(tab, force: true);
        }
    }
}
