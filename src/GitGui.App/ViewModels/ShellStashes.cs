using GitGui.Core.Git;

namespace GitGui.App.ViewModels;

public sealed partial class ShellViewModel
{
    public RelayCommand StashesCommand { get; private set; } = null!;
    public Task StashDialogTask { get; private set; } = Task.CompletedTask;
    private void InitializeStashes() => StashesCommand = new(() => StashDialogTask = OpenStashesWhenReadyAsync(SelectedRepository!), () => CanRequestAction);
    private async Task OpenStashesWhenReadyAsync(RepositoryTab tab)
    {
        if (tab.Snapshot is null || !_knownIdentities.ContainsKey(tab.Directory)) await tab.InspectionTask;
        if (tab.Snapshot is null || !_knownIdentities.ContainsKey(tab.Directory))
        { _interaction?.ShowActionError(GitGui.App.Localization.UiText.Get("ActionInspectionNeeded")); return; }
        await OpenStashesAsync(tab);
    }
    private async Task OpenStashesAsync(RepositoryTab tab)
    {
        var snapshot = tab.Snapshot!;
        if (!_knownIdentities.TryGetValue(tab.Directory, out var known)) return;
        CancelCountRead();
        var context = new ActionContext(_git!, new(snapshot.Worktree, snapshot.GitDirectory, snapshot.CommonDirectory, known.Fingerprint), snapshot,
            tab.History.Page?.Session ?? new(snapshot, HistoryScope.AllCommits, "", [], false), [], "");
        var model = new StashViewModel(context, _actions!, _reader!, _interaction!, tab.Stashes, SetLastAction)
        { ScheduleOperation = (key, title, run) => QueueAction(tab, key, title, run) };
        try { await _interaction!.ShowStashesAsync(model); }
        finally
        {
            await model.CloseAsync();
            if (model.HasMutation)
                foreach (var related in Repositories.Where(item => item.Snapshot?.CommonDirectory == snapshot.CommonDirectory))
                { related.History.NeedsRefresh = true; related.History.ValidatedAt = default; related.Changes.ReadAt = default; if (related != tab) related.Stashes.List = null; }
            foreach (var other in Repositories.Where(item => item != tab).OrderByDescending(item => item.History.LastUse).Skip(2)) other.Stashes.List = null;
            if (model.HasMutation && !_stopping) BeginInspection(tab, force: true);
        }
    }
}
