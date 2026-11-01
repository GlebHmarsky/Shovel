using System.Text.RegularExpressions;
using GitGui.App.Localization;
using GitGui.Core.Git;

namespace GitGui.App.ViewModels;

public sealed record SidebarMenuAction(string Label, RelayCommand Command);
public sealed record SidebarNode(string Label, string Icon, string Detail, IReadOnlyList<SidebarNode> Children,
    RelayCommand? Select = null, RelayCommand? Activate = null, IReadOnlyList<SidebarMenuAction>? Menu = null)
{
    public string Accessible => string.Join(" ", Icon, Label, Detail);
}

public sealed partial class ShellViewModel
{
    private CancellationTokenSource? _sidebarCancel;
    private Task _sidebarTask = Task.CompletedTask;
    public IReadOnlyList<SidebarNode> SidebarNodes { get; private set; } = [];
    private string _sidebarStatus = "";
    public string SidebarStatus { get => _sidebarStatus; private set => Set(ref _sidebarStatus, value); }

    private void RefreshSidebarCommands()
    {
        foreach (var node in SidebarNodes) Refresh(node);
        static void Refresh(SidebarNode node)
        {
            node.Select?.Refresh(); node.Activate?.Refresh();
            foreach (var action in node.Menu ?? []) action.Command.Refresh();
            foreach (var child in node.Children) Refresh(child);
        }
    }

    private void PublishSidebar()
    {
        if (SelectedRepository is not { Snapshot: { } snapshot } tab || tab.History.Page is not { } page)
        { SidebarNodes = []; Changed(nameof(SidebarNodes)); return; }
        bool CanUse() => SelectedRepository == tab && CanAct;
        RelayCommand Command(Action action) => new(action, CanUse);
        SidebarMenuAction Menu(string key, Action action) => new(UiText.Get(key), Command(action));
        RelayCommand Jump(string hash) => new(() => LocateSidebarCommit(tab, hash), () => SelectedRepository == tab && CanRead && !IsBusy);
        SidebarNode Group(string key, IEnumerable<SidebarNode> nodes) => new(UiText.Get(key), "", "", nodes.ToArray());
        void BranchManage(BranchManagementAction action, string name) => ActionTask = ManageBranchAsync(action, name);
        var refs = page.Session.References;
        var branches = refs.Where(r => r.Name.StartsWith("refs/heads/", StringComparison.Ordinal)).Select(r =>
        {
            var name = r.Name[11..];
            var track = tab.Sidebar?.Branches.FirstOrDefault(b => b.Ref == r.Name)?.Tracking ?? "";
            var counts = Regex.Matches(track, @"(ahead|behind) (\d+)");
            var detail = string.Join(" ", counts.Select(m => (m.Groups[1].Value == "ahead" ? "↑" : "↓") + m.Groups[2].Value));
            if (track == "gone") detail = UiText.Get("SidebarGone");
            if (r.Upstream is not null && tab.Sidebar is null) detail = "…";
            return new SidebarNode(name, snapshot.Branch == name ? "✓" : "⑂", detail, [], Jump(r.Hash), new RelayCommand(() => ActionTask = SwitchSidebarAsync(tab, name), () => SelectedRepository == tab && CanNavigate),
                [Menu("ActionTitleSwitchBranch", () => RunSidebarAction(tab, GitAction.SwitchBranch, name)),
                 new(UiText.Get("SidebarLocate"), Jump(r.Hash)),
                 Menu("BranchManageTitleDeleteBranch", () => BranchManage(BranchManagementAction.Delete, r.Name)),
                 Menu("BranchManageTitleSetUpstream", () => BranchManage(BranchManagementAction.SetUpstream, r.Name)),
                 Menu("BranchManageTitleUnsetUpstream", () => BranchManage(BranchManagementAction.UnsetUpstream, r.Name))]);
        });
        var remoteRefs = refs.Where(r => r.Name.StartsWith("refs/remotes/", StringComparison.Ordinal)).ToArray();
        var remotes = snapshot.RemoteNames.Concat(remoteRefs.Select(r => r.Name[13..].Split('/')[0])).Distinct().Select(name =>
            new SidebarNode(name, "↔", "", remoteRefs.Where(r => r.Name.StartsWith("refs/remotes/" + name + "/", StringComparison.Ordinal)).Select(r =>
                new SidebarNode(r.Name[(14 + name.Length)..], "⑂", "", [], Jump(r.Hash), Menu: [new(UiText.Get("SidebarLocate"), Jump(r.Hash))])).ToArray(),
                Menu: snapshot.RemoteNames.Contains(name) ? [Menu("ActionTitleFetch", () => RunSidebarAction(tab, GitAction.Fetch, remote: name)),
                    new(UiText.Get("RemoteManageTitle"), new(() => ActionTask = ManageRemotesAsync(name), () => CanUse() && tab.RemoteConfiguration is not null))] : []));
        var tags = refs.Where(r => r.Name.StartsWith("refs/tags/", StringComparison.Ordinal)).Select(r =>
            new SidebarNode(r.Name[10..], "◇", "", [], Jump(r.Hash), Menu: [new(UiText.Get("SidebarLocate"), Jump(r.Hash))]));
        var stashes = (tab.Stashes.List?.Entries ?? []).Select(entry => new SidebarNode(entry.Message, "▣", entry.Selector, [], Jump(entry.Hash),
            Menu: [new(UiText.Get("SidebarLocate"), Jump(entry.Hash)),
                Menu("ActionTitleStashApply", () => RunSidebarStash(tab, entry, StashAction.Apply)),
                Menu("ActionTitleStashPop", () => RunSidebarStash(tab, entry, StashAction.Pop)),
                Menu("ActionTitleStashDrop", () => RunSidebarStash(tab, entry, StashAction.Drop))]));
        var modules = (tab.Sidebar?.Submodules ?? []).Select(module => new SidebarNode(module.Path, "↗", "", [],
            Command(() => ActionTask = OpenSubmoduleAsync(tab, module)), Command(() => ActionTask = OpenSubmoduleAsync(tab, module)),
            [Menu("SidebarOpenSubmodule", () => ActionTask = OpenSubmoduleAsync(tab, module)),
             Menu("ActionTitleSubmoduleUpdate", () => RunSidebarAction(tab, GitAction.SubmoduleUpdate, module.Path, start: module.Hash)),
             Menu("ActionTitleSubmoduleSync", () => RunSidebarAction(tab, GitAction.SubmoduleSync, module.Path, start: module.Hash))]));
        SidebarNodes = [Group("Branches", branches), Group("Remotes", remotes) with { Menu = [new(UiText.Get("RemoteAddTitle"), AddRemoteCommand), new(UiText.Get("RemoteManageTitle"), ManageRemotesCommand)] }, Group("Tags", tags), Group("Stashes", stashes), Group("Submodules", modules)];
        Changed(nameof(SidebarNodes));
    }

    private void BeginSidebarRead(RepositoryTab tab)
    {
        if (_actions is null || _git is null || tab.Snapshot is null || !_knownIdentities.TryGetValue(tab.Directory, out var known)) return;
        if (tab.SidebarReadAt != default && DateTimeOffset.UtcNow - tab.SidebarReadAt < RefreshAge) return;
        _sidebarCancel?.Cancel(); var cancel = new CancellationTokenSource(TimeSpan.FromSeconds(20)); _sidebarCancel = cancel;
        var prior = _sidebarTask; var snapshot = tab.Snapshot; var git = _git;
        SidebarStatus = UiText.Get("SidebarLoading");
        _sidebarTask = Load();
        async Task Load()
        {
            using var activity = Journal.BeginRead(tab.Directory, UiText.Get("ActivitySidebar"));
            try
            {
                await prior;
                var identity = new RepositoryIdentity(snapshot.Worktree, snapshot.GitDirectory, snapshot.CommonDirectory, known.Fingerprint);
                var data = await _reader!.ReadSidebarAsync(git, tab.Directory, cancel.Token);
                var stashes = await _actions.ReadStashesAsync(git, identity, cancel.Token);
                RemoteConfigurationSnapshot? remotes = null;
                // A remote configuration problem must not suppress branches, stashes or submodules.
                try { remotes = await _actions.ReadRemoteConfigurationAsync(git, tab.Directory, cancel.Token); }
                catch (GitReadException) when (!cancel.IsCancellationRequested) { }
                catch (GitActionException) { }
                if (cancel.IsCancellationRequested || SelectedRepository != tab || tab.Snapshot != snapshot) return;
                tab.Sidebar = data; tab.Stashes.List = stashes; tab.RemoteConfiguration = remotes; tab.SidebarReadAt = DateTimeOffset.UtcNow;
                SidebarStatus = ""; PublishSidebar(); RefreshBranchManagement();
                activity.Outcome = OperationOutcome.Success;
            }
            catch (OperationCanceledException) { }
            catch (GitReadException error)
            {
                activity.Outcome = OperationOutcome.Failed;
                if (SelectedRepository == tab && !cancel.IsCancellationRequested)
                { tab.History.RawOutput = error.RawOutput; Changed(nameof(RawOutput)); SidebarStatus = UiText.Get("SidebarFailed"); }
            }
            catch (Exception) { activity.Outcome = OperationOutcome.Failed; if (SelectedRepository == tab && !cancel.IsCancellationRequested) SidebarStatus = UiText.Get("SidebarFailed"); }
            finally { if (ReferenceEquals(_sidebarCancel, cancel)) _sidebarCancel = null; cancel.Dispose(); }
        }
    }

    private void LocateSidebarCommit(RepositoryTab tab, string hash)
    {
        if (SelectedRepository != tab || IsBusy || tab.History.Page is null) return;
        var existing = Commits.FirstOrDefault(row => row.Commit.Hash == hash);
        if (existing is not null) { SelectedCommit = existing; SidebarCommitLocated?.Invoke(this, EventArgs.Empty); return; }
        _operationCancellation?.Cancel(); CancelCountRead();
        var cancel = new CancellationTokenSource(TimeSpan.FromSeconds(30)); _operationCancellation = cancel;
        var generation = ++_generation; var session = tab.History.Page.Session;
        _elapsed.Restart(); _timer.Start(); IsBusy = true; Status = UiText.Get("SidebarLocating"); StartReadActivity();
        var located = false;
        _activeOperation = Locate();
        async Task Locate()
        {
            try
            {
                var history = await _reader!.LocateHistoryAsync(_git!, session, hash, cancel.Token);
                var rows = await Task.Run(() => history.Entries.Select(e => new CommitRow(e, history.Session.Repository)).ToArray(), cancel.Token);
                if (generation != _generation || cancel.IsCancellationRequested || SelectedRepository != tab) return;
                var state = tab.History;
                state.Page = history; state.Rows = rows; state.Cursors.Clear(); state.Cursors.Add(history.Cursor); state.PageIndex = 0;
                state.PageViews.Clear(); state.EstimatedBytes = rows.Sum(r => (long)r.Commit.Message.Length * 2 + 1024);
                state.VerticalOffset = 0; state.Selected = rows.FirstOrDefault(r => r.Commit.Hash == hash); state.Notice = LoadedNotice(history);
                located = true; PublishState(); SidebarCommitLocated?.Invoke(this, EventArgs.Empty); Status = UiText.Get("HistoryLoaded");
            }
            catch (GitReadException error)
            {
                if (generation == _generation)
                { tab.History.RawOutput = error.RawOutput; Changed(nameof(RawOutput)); Status = UiText.Get("SidebarLocateFailed"); }
            }
            catch (OperationCanceledException) { if (generation == _generation) Status = UiText.Get("GitErrorCancelled"); }
            catch (Exception) { if (generation == _generation) Status = UiText.Get("SidebarLocateFailed"); }
            finally { if (generation == _generation) { _operationCancellation = null; _timer.Stop(); _elapsed.Stop(); IsBusy = false; FinishReadActivity(cancel.IsCancellationRequested ? OperationOutcome.Cancelled : located ? OperationOutcome.Success : OperationOutcome.Failed); } cancel.Dispose(); }
        }
    }
    public event EventHandler? SidebarCommitLocated;

    private async Task SwitchSidebarAsync(RepositoryTab tab, string name)
    {
        _operationCancellation?.Cancel(); await _activeOperation;
        if (SelectedRepository == tab && CanAct) await ExecuteSidebarAsync(tab, GitAction.SwitchBranch, name);
    }

    private ActionContext SidebarContext(RepositoryTab tab)
    {
        var snapshot = tab.Snapshot!;
        return new(_git!, new(snapshot.Worktree, snapshot.GitDirectory, snapshot.CommonDirectory, _knownIdentities[tab.Directory].Fingerprint),
            snapshot, tab.History.Page!.Session, snapshot.RemoteNames.Select(n => new GitRemote(n, "", "")).ToArray(), "");
    }
    private void RunSidebarAction(RepositoryTab tab, GitAction action, string branch = "", string remote = "", string start = "HEAD") =>
        ActionTask = ExecuteSidebarAsync(tab, action, branch, remote, start);
    private void RunSidebarStash(RepositoryTab tab, StashEntry entry, StashAction action)
    {
        var list = tab.Stashes.List!;
        if (action == StashAction.Drop && !_interaction!.ConfirmRecovery(UiText.Format("StashConfirmDrop", tab.Directory, entry.Selector, entry.Hash, entry.Message), UiText.Get("ActionTitleStashDrop"))) return;
        ActionTask = ExecuteSidebarAsync(tab, action == StashAction.Apply ? GitAction.StashApply : action == StashAction.Pop ? GitAction.StashPop : GitAction.StashDrop,
            stash: new(action, list, entry));
    }
    private async Task ExecuteSidebarAsync(RepositoryTab tab, GitAction action, string branch = "", string remote = "", string start = "HEAD", StashOptions? stash = null,
        StashAction? historyStashAction = null, string historyStashHash = "", string historyStashSelector = "")
    {
        if (SelectedRepository != tab || !CanAct) return;
        var cached = SidebarContext(tab); ActionContext? prepared = null;
        CancelCountRead(); _sidebarCancel?.Cancel(); SetActionOpen(true);
        try
        {
            var operation = new GitActionViewModel(cached, action, _actions!, async () =>
            {
                foreach (var item in Repositories) { item.History.ValidatedAt = default; item.SidebarReadAt = default; item.Changes.ReadAt = default; }
                BeginInspection(tab, force: true, actionReconcile: true); await WaitForIdleAsync();
                if (tab.History.ValidatedAt == default || tab.History.Error.Length != 0) throw new System.IO.IOException();
            }, branch, _interaction, async token =>
            {
                var fresh = await _actions!.PrepareAsync(cached.Git, tab.Directory, token);
                if (fresh.Identity.Fingerprint != cached.Identity.Fingerprint || fresh.History.Fingerprint != cached.History.Fingerprint) throw new GitActionException("ActionChanged");
                if (historyStashAction is { } selectedAction)
                {
                    var list = await _actions.ReadStashesAsync(cached.Git, fresh.Identity, token);
                    var entry = list.Entries.FirstOrDefault(e => e.Hash == historyStashHash && e.Selector == historyStashSelector) ?? throw new GitActionException("StashChanged");
                    stash = new(selectedAction, list, entry); tab.Stashes.List = list;
                }
                prepared = fresh; return fresh;
            }, stash is null && historyStashAction is null ? null : (token, output) => _actions!.ExecuteStashAsync(prepared!, stash!, token, output));
            operation.Branch = branch; operation.StartRef = start;
            if (remote.Length > 0) operation.Remote = operation.Remotes.FirstOrDefault(r => r.Name == remote);
            SetLastAction(operation); DetailsIndex = 2; operation.RunCommand.Execute(null); await operation.RunningTask; Status = operation.Summary;
        }
        finally { SetActionOpen(false); BeginSidebarRead(tab); await _sidebarTask; }
    }
    private async Task OpenSubmoduleAsync(RepositoryTab parent, SubmoduleEntry module)
    {
        if (SelectedRepository != parent || !CanAct) return;
        SetActionOpen(true);
        string? directory = null;
        try
        {
            directory = await Task.Run(async () =>
            {
                var path = GitActions.ValidateSubmodulePath(parent.Directory, module.Path);
                var identity = await _identities.ReadAsync(path, default);
                if (!string.Equals(identity.Worktree, path, StringComparison.OrdinalIgnoreCase)) throw new System.IO.IOException();
                return path;
            });
        }
        catch (Exception) { Status = UiText.Get("SidebarSubmoduleUnavailable"); }
        finally { SetActionOpen(false); }
        if (directory is null || _stopping) return;
        var tab = Repositories.FirstOrDefault(t => string.Equals(t.Directory, directory, StringComparison.OrdinalIgnoreCase));
        if (tab is null) { if (Repositories.Count >= 20) { Status = UiText.Get("TooManyRepositories"); return; } tab = new(directory); Repositories.Add(tab); }
        SelectedRepository = tab;
    }
}
