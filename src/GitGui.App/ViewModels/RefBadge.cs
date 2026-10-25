using GitGui.App.Localization;
using GitGui.Core.Git;

namespace GitGui.App.ViewModels;

public sealed record RefBadge(string Label, string Help, bool IsCurrent, bool HasRemote, bool IsTag, bool IsRemote)
{
    // Original line drawings; no proprietary provider logos or network-loaded assets.
    public string Icon => IsTag ? "M1,1 L9,1 19,11 11,19 1,9 Z M5,5 L5.1,5.1" :
        IsRemote ? RemoteIcon : "M5,3 L5,17 M5,11 C5,7 15,12 15,5 M3,1 L7,1 7,5 3,5 Z M13,1 L17,1 17,5 13,5 Z M3,15 L7,15 7,19 3,19 Z";
    public string RemoteIcon => "M2,2 L18,2 18,13 2,13 Z M10,13 L10,18 M5,18 L15,18 M5,6 L15,6 M5,9 L12,9";
    public static IReadOnlyList<RefBadge> Build(HistoryEntry entry, RepositorySnapshot? repository)
    {
        var refs = entry.References ?? [];
        var paired = new HashSet<string>(StringComparer.Ordinal);
        var badges = new List<RefBadge>();
        foreach (var reference in refs.Where(item => item.Name.StartsWith("refs/heads/", StringComparison.Ordinal))
            .OrderBy(item => item.Name[11..] == repository?.Branch ? 0 : 1))
        {
            var current = repository?.Branch == reference.Name[11..] && repository.Commit == entry.Commit.Hash;
            var upstream = refs.FirstOrDefault(item => item.Name == reference.Upstream && item.Name.StartsWith("refs/remotes/", StringComparison.Ordinal));
            if (upstream is not null) paired.Add(upstream.Name);
            var help = UiText.Format(upstream is null ? "BranchBadgeHelp" : "SyncedBadgeHelp", reference.Name, upstream?.Name ?? "");
            if (current) help = UiText.Format("CurrentBadgeHelp", help);
            badges.Add(new(reference.Name[11..], help, current, upstream is not null, false, false));
        }
        foreach (var reference in refs.Where(item => item.Name.StartsWith("refs/remotes/", StringComparison.Ordinal) && !paired.Contains(item.Name)))
            badges.Add(new(reference.Name[13..], UiText.Format("RemoteBadgeHelp", reference.Name), false, false, false, true));
        foreach (var reference in refs.Where(item => item.Name.StartsWith("refs/tags/", StringComparison.Ordinal)))
            badges.Add(new(reference.Name[10..], UiText.Format("TagBadgeHelp", reference.Name), false, false, true, false));
        if (repository?.HeadKind == HeadKind.Detached && repository.Commit == entry.Commit.Hash)
            badges.Add(new(UiText.Get("HeadLabel"), UiText.Get("DetachedBadgeHelp"), true, false, false, false));
        return badges;
    }
}
