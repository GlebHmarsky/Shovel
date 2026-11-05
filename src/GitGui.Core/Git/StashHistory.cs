namespace GitGui.Core.Git;

public sealed partial class RepositoryReader
{
    private async Task<IReadOnlyList<CommitRecord>> ReadFoldedStashesAsync(GitInstallation git, HistorySession session, CancellationToken token)
    {
        var tips = session.References.Where(r => r.Name.StartsWith("stash@{", StringComparison.Ordinal)).Select(r => r.Hash).Distinct().ToArray();
        if (session.Scope != HistoryScope.AllCommits || tips.Length == 0) return [];
        var records = new List<CommitRecord>();
        foreach (var chunk in tips.Chunk(64))
        {
            var result = await RunAsync(git.Executable, session.Repository.Worktree, ReadArguments([
                "log", "--no-walk=unsorted", "--no-show-signature", "--no-notes", "--no-use-mailmap", "--encoding=UTF-8", "-z",
                "--format=tformat:%H%x00%P%x00%an%x00%ae%x00%aI%x00%cn%x00%ce%x00%cI%x00%B", .. chunk, "--"]), token, MaximumHistoryBytes);
            RequireSuccess(result); records.AddRange(ParseHistory(result.StandardOutput, chunk.Length));
        }
        return records.Where(record => record.Parents.Count is 2 or 3).ToArray();
    }
    private async Task<IReadOnlySet<string>> ReadStashInternalsAsync(GitInstallation git, HistorySession session, CancellationToken token)
    {
        var tips = session.References.Where(r => r.Name.StartsWith("stash@{", StringComparison.Ordinal)).Select(r => r.Hash).Distinct().ToArray();
        var hidden = new HashSet<string>(StringComparer.Ordinal);
        if (session.Scope != HistoryScope.AllCommits || tips.Length == 0) return hidden;
        var candidates = session.FoldedStashes.SelectMany(stash => stash.Parents.Skip(1)).ToHashSet(StringComparer.Ordinal);
        var bases = session.FoldedStashes.Select(stash => stash.Parents[0]).Distinct().ToArray();
        // Keep internals that are independently reachable through branches/tags/HEAD or a
        // stash base. Excluding base ancestry makes ordinary stash walks tiny. Chunk SHA-256
        // arguments below Windows' process command-line limit. Reachability can still cost
        // time on old/divergent history; existing process deadlines/cancellation apply.
        foreach (var chunk in candidates.Chunk(64))
        {
            var privateObjects = await RunAsync(git.Executable, session.Repository.Worktree, ReadArguments([
                "rev-list", "--max-count=513", .. chunk, "--not", "--branches", "--remotes", "--tags",
                .. session.Repository.Commit is { } head ? new[] { head } : [], .. bases, "--"]), token, MaximumHistoryBytes);
            RequireSuccess(privateObjects);
            var hashes = Text(privateObjects.StandardOutput).Split('\n', StringSplitOptions.RemoveEmptyEntries);
            if (hashes.Length > 512) throw new GitReadException(GitFailure.OutputLimit);
            if (hashes.Any(hash => !IsObjectId(hash))) throw new GitReadException(GitFailure.InvalidOutput);
            hidden.UnionWith(hashes.Where(candidates.Contains));
        }
        hidden.ExceptWith(tips);
        hidden.UnionWith(session.FoldedStashes.Select(stash => stash.Hash));
        return hidden;
    }
}
