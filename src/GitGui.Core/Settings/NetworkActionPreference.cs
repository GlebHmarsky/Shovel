using GitGui.Core.Git;

namespace GitGui.Core.Settings;

// Behavioral flags belong to a repository/action. Explicit destinations also retain the
// branch reviewed in the menu, so changing branches never reuses another branch's push target.
public sealed record NetworkActionPreference(string Path, GitAction Action, string Branch, string SourceBranch,
    string Remote, string RemoteBranch, PullStrategy Strategy, bool AutoStash, bool AllRemotes, bool AllTags, bool SetUpstream, bool ExplicitTarget = false)
{
    public static NetworkActionPreference[] Normalize(IEnumerable<NetworkActionPreference>? preferences) => (preferences ?? [])
        .Where(item => item is not null && !string.IsNullOrWhiteSpace(item.Path) && !item.Path.Contains('\0') && System.IO.Path.IsPathFullyQualified(item.Path) && item.Path.Length <= 32768 &&
            item.Action is GitAction.Fetch or GitAction.Pull or GitAction.Push && Enum.IsDefined(item.Strategy) &&
            new[] { item.Branch, item.SourceBranch, item.Remote, item.RemoteBranch }.All(value => value is { Length: <= 1024 } && !value.Any(char.IsControl)))
        .DistinctBy(item => (item.Path.ToUpperInvariant(), item.Action)).Take(60).ToArray();
}
