namespace GitGui.Core.Git;

public sealed partial class RepositoryReader
{
    private async Task<IReadOnlySet<string>> ReadStashInternalsAsync(GitInstallation git, HistorySession session, CancellationToken token)
    {
        var tips = session.References.Where(r => r.Name.StartsWith("stash@{", StringComparison.Ordinal)).Select(r => r.Hash).Distinct().ToArray();
        var hidden = new HashSet<string>(StringComparer.Ordinal);
        if (session.Scope != HistoryScope.AllCommits || tips.Length == 0) return hidden;
        var result = await RunAsync(git.Executable, session.Repository.Worktree,
            ReadArguments(["rev-list", "--no-walk=unsorted", "--parents", .. tips, "--"]), token, MaximumHistoryBytes);
        RequireSuccess(result);
        var candidates = new HashSet<string>(StringComparer.Ordinal); var bases = new HashSet<string>(StringComparer.Ordinal);
        foreach (var line in Text(result.StandardOutput).Split('\n', StringSplitOptions.RemoveEmptyEntries))
        {
            var ids = line.Split(' ');
            if (ids.Any(id => !IsObjectId(id))) throw new GitReadException(GitFailure.InvalidOutput);
            // Only the normal stash shape is folded; unusual stored commits remain ordinary history.
            if (ids.Length is not (3 or 4)) continue;
            bases.Add(ids[1]); foreach (var id in ids.Skip(2)) candidates.Add(id);
        }
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
        return hidden;
    }
}
