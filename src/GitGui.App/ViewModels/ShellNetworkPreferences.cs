using GitGui.Core.Git;
using GitGui.Core.Settings;

namespace GitGui.App.ViewModels;

public sealed partial class ShellViewModel
{
    private NetworkActionPreference[] _networkActions = [];
    private NetworkActionPreference? NetworkPreference(RepositoryTab tab, GitAction action) => _networkActions.FirstOrDefault(item =>
        string.Equals(item.Path, tab.Directory, StringComparison.OrdinalIgnoreCase) && item.Action == action);

    private void RestoreNetworkOptions(RepositoryTab tab, GitActionViewModel model)
    {
        if (NetworkPreference(tab, model.Action) is not { } saved) return;
        model.Strategy = saved.Strategy; model.AutoStash = saved.AutoStash;
        model.AllRemotes = saved.AllRemotes; model.AllTags = saved.AllTags; model.SetUpstream = saved.SetUpstream;
        if (!saved.ExplicitTarget || model.Action != GitAction.Fetch && saved.Branch != (tab.Snapshot?.Branch ?? "")) return;
        model.Remote = model.Remotes.FirstOrDefault(remote => remote.Name == saved.Remote);
        model.RemoteBranch = saved.RemoteBranch; model.Branch = saved.SourceBranch;
    }
    private void RememberNetworkOptions(RepositoryTab tab, GitActionViewModel model, ActionOptions initial)
    {
        if (model.Action is not (GitAction.Fetch or GitAction.Pull or GitAction.Push) || model.Remote is null) return;
        var prior = NetworkPreference(tab, model.Action);
        var explicitTarget = prior is { ExplicitTarget: true } && (model.Action == GitAction.Fetch || prior.Branch == (tab.Snapshot?.Branch ?? "")) ||
            model.Remote.Name != initial.Remote || model.RemoteBranch != initial.RemoteBranch || model.Branch != initial.Branch;
        var saved = new NetworkActionPreference(tab.Directory, model.Action, tab.Snapshot?.Branch ?? "", model.Branch,
            model.Remote.Name, model.RemoteBranch, model.Strategy, model.AutoStash, model.AllRemotes, model.AllTags, model.SetUpstream, explicitTarget);
        _networkActions = [saved, .. _networkActions.Where(item => item.Action != saved.Action || !string.Equals(item.Path, saved.Path, StringComparison.OrdinalIgnoreCase)).Take(59)];
        PreferenceTask = SavePreferencesAsync();
        RefreshCommands();
    }
    private (string Remote, string Branch)? PreferredPullTarget(RepositoryTab? tab)
    {
        if (tab is not null && NetworkPreference(tab, GitAction.Pull) is { ExplicitTarget: true } saved && saved.Branch == (tab.Snapshot?.Branch ?? ""))
            return (saved.Remote, saved.RemoteBranch);
        return PullTarget(tab);
    }
}
