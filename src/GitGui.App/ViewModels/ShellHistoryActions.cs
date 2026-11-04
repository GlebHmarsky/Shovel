using GitGui.App.Localization;
using GitGui.Core.Git;

namespace GitGui.App.ViewModels;

public sealed record HistoryMenuEntry(string Label, RelayCommand? Command, string Help, IReadOnlyList<HistoryMenuEntry>? Children = null);

public sealed partial class ShellViewModel
{
    private bool _historyMenuOpen;
    private readonly List<RelayCommand> _historyMenuCommands = [];
    public void BeginHistoryMenu() => _historyMenuOpen = true;
    public void EndHistoryMenu() { _historyMenuOpen = false; _historyMenuCommands.Clear(); UpdateCommitFiles(); }
    private void RefreshHistoryActions() { foreach (var command in _historyMenuCommands.ToArray()) command.Refresh(); }

    public IReadOnlyList<HistoryMenuEntry> CreateHistoryMenu(CommitRow row)
    {
        _historyMenuCommands.Clear();
        if (SelectedRepository is not { } tab || tab.History.Page is not { } page || !Commits.Contains(row)) return [];
        var snapshot = tab.Snapshot;
        bool Current() => SelectedRepository == tab && ReferenceEquals(tab.History.Page, page) && Commits.Contains(row);
        if (page.Session.References.Any(r => r.Hash == row.Commit.Hash && r.Name.StartsWith("stash@{", StringComparison.Ordinal)))
        {
            var selector = page.Session.References.First(r => r.Hash == row.Commit.Hash && r.Name.StartsWith("stash@{", StringComparison.Ordinal)).Name;
            return new[] { StashAction.Apply, StashAction.Pop, StashAction.Drop }.Select(action =>
            {
                var command = new RelayCommand(() => ActionTask = QueueAction(tab, "Stash" + action + row.Commit.Hash, UiText.Get("ActionTitleStash" + action), () => RunHistoryStashAsync(tab, row.Commit.Hash, selector, row.Commit.Subject, action)), () => Current() && CanAct);
                _historyMenuCommands.Add(command);
                var title = UiText.Get("ActionTitleStash" + action);
                return new HistoryMenuEntry(title, command, title);
            }).ToArray();
        }
        HistoryMenuEntry Action(GitAction action, int mainline = 0)
        {
            var command = new RelayCommand(() => ActionTask = QueueAction(tab, action + row.Commit.Hash + mainline, UiText.Get("HistoryMenu" + action), () => RunHistoryActionAsync(tab, row, action, mainline)),
                () => Current() && CanAct && (!GitActions.IsReset(action) || snapshot?.HeadKind == HeadKind.Branch));
            _historyMenuCommands.Add(command);
            var label = mainline == 0 ? UiText.Get("HistoryMenu" + action) : UiText.Format("HistoryRevertParent", mainline, row.Commit.Parents[mainline - 1]);
            return new(label, command, UiText.Get(GitActions.IsReset(action) && snapshot?.HeadKind != HeadKind.Branch ? "HistoryResetNeedsBranch" :
                ActionUnavailableKey ?? "HistoryMenuHelp"));
        }
        var copy = new RelayCommand(() =>
        {
            try { _interaction?.CopyFilePaths(row.Commit.Hash); }
            catch (Exception) { _interaction?.ShowActionError(UiText.Get("HistoryCopyFailed")); }
        }, () => Current() && _interaction is not null);
        _historyMenuCommands.Add(copy);
        var revert = row.Commit.Parents.Count > 1
            ? new HistoryMenuEntry(UiText.Get("HistoryMenuRevertCommit"), null, UiText.Get("HistoryRevertParentHelp"),
                Enumerable.Range(1, row.Commit.Parents.Count).Select(parent => Action(GitAction.RevertCommit, parent)).ToArray())
            : Action(GitAction.RevertCommit);
        return [Action(GitAction.CheckoutCommit),
            new(UiText.Get("HistoryResetMenu"), null, UiText.Get("HistoryResetHelp"), [Action(GitAction.ResetSoft), Action(GitAction.ResetMixed), Action(GitAction.ResetHard)]),
            new(UiText.Get("HistoryCopySha"), copy, UiText.Get("HistoryCopySha")), revert];
    }

    private async Task RunHistoryStashAsync(RepositoryTab tab, string hash, string selector, string subject, StashAction action)
    {
        // Menu opening uses only refs. Read the current ordinal list after submission, pin the hash,
        // and let ExecuteStashAsync reject subsequent list changes before any removal.
        if (tab.Snapshot is null) throw new GitActionException("ActionInspectionNeeded");
        if (action == StashAction.Drop && !_interaction!.ConfirmRecovery(UiText.Format("StashConfirmDrop", tab.Directory, selector, hash, subject), UiText.Get("ActionTitleStashDrop"))) return;
        await ExecuteSidebarAsync(tab, action == StashAction.Apply ? GitAction.StashApply : action == StashAction.Pop ? GitAction.StashPop : GitAction.StashDrop,
            historyStashAction: action, historyStashHash: hash, historyStashSelector: selector);
    }

    private async Task RunHistoryActionAsync(RepositoryTab tab, CommitRow row, GitAction action, int mainline)
    {
        var snapshot = tab.Snapshot!; var page = tab.History.Page!;
        if (!_knownIdentities.TryGetValue(tab.Directory, out var identity)) return;
        var cached = new ActionContext(_git!, new(snapshot.Worktree, snapshot.GitDirectory, snapshot.CommonDirectory, identity.Fingerprint), snapshot, page.Session, [], "");
        CancelCountRead(); SetActionOpen(true, tab); _preparingAction = false; Changed(nameof(OperationStatus));
        var attempted = false;
        try
        {
            if (GitActions.IsReset(action) && !_interaction!.ConfirmRecovery(UiText.Format("HistoryResetConfirm", tab.Directory, snapshot.Branch!,
                snapshot.Commit!, row.Commit.Hash, UiText.Get("HistoryEffect" + action)), UiText.Get("HistoryMenu" + action))) return;
            ActionContext? prepared = null;
            var operation = new GitActionViewModel(cached, action, _actions!, () => ReconcileActionAsync(tab), prepareExecution: async token =>
            {
                prepared = await _actions!.PrepareBranchManagementAsync(cached.Git, cached.Identity, cached.Repository, cached.History, token);
                return prepared;
            }, execute: async (token, output) =>
            {
                var result = await _actions!.ExecuteAsync(prepared!, new(action, StartRef: row.Commit.Hash, Mainline: mainline), token, output);
                return result with { Output = UiText.Format("HistoryActionTarget", row.Commit.Hash, snapshot.Commit ?? UiText.Get("NoBranch")) + Environment.NewLine + result.Output };
            });
            SetLastAction(operation); DetailsIndex = 2; operation.RunCommand.Execute(null); attempted = operation.HasAttempt;
            await operation.RunningTask; if (SelectedRepository == tab) Status = operation.Summary;
        }
        finally { SetActionOpen(false, tab); if (attempted) { BeginCountRead(tab); BeginSidebarRead(tab); } }
    }
}
