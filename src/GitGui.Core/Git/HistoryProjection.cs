namespace GitGui.Core.Git;

public sealed partial class RepositoryReader
{
    // Stash timestamps must not influence the ordinary branch walk. Walk their bases,
    // then insert the saved snapshots immediately before each base. Navigation uses
    // exactly this projection too; a cursor can resume halfway through a stash group.
    private static string[] HistoryRoots(HistorySession session) => session.Scope == HistoryScope.CurrentHead
        ? [session.Repository.Commit!]
        : ["--branches", "--remotes", "--tags",
            .. session.References.Where(r => r.Name.StartsWith("stash@{", StringComparison.Ordinal) &&
                !session.FoldedStashes.Any(stash => stash.Hash == r.Hash)).Select(r => r.Hash),
            .. session.FoldedStashes.Select(stash => stash.Parents[0]).Distinct(),
            .. session.Repository.Commit is { } head ? new[] { head } : []];

    private static IEnumerable<(string Hash, int Raw, int Inserted)> ProjectHistory(HistorySession session,
        IEnumerable<string> hashes, int insertedOffset = 0)
    {
        var stashes = session.FoldedStashes.ToLookup(stash => stash.Parents[0]);
        var raw = 0;
        foreach (var hash in hashes)
        {
            if (!session.HiddenStashCommits.Contains(hash))
            {
                var block = stashes[hash].Select(stash => stash.Hash).Append(hash).ToArray();
                for (var i = raw == 0 ? insertedOffset : 0; i < block.Length; i++) yield return (block[i], raw, i);
            }
            raw++;
        }
    }
}
