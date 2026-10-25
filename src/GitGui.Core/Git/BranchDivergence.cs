using System.Globalization;

namespace GitGui.Core.Git;

public enum DivergenceState { Pending, Available, NoUpstream, MissingUpstream, Detached, Unborn, Shallow, Failed }
public sealed record BranchDivergence(DivergenceState State, long Ahead = 0, long Behind = 0);

public sealed partial class RepositoryReader
{
    /// <summary>Compare immutable tips from the validated history snapshot, never contact a remote.
    /// Run after publishing history; a slow/failed comparison must not hide useful rows.</summary>
    public Task<BranchDivergence> ReadDivergenceAsync(GitInstallation git, HistorySession session, CancellationToken token) => Task.Run(async () =>
    {
        var repository = session.Repository;
        if (repository.HeadKind == HeadKind.Unborn) return new BranchDivergence(DivergenceState.Unborn);
        if (repository.HeadKind == HeadKind.Detached) return new BranchDivergence(DivergenceState.Detached);
        if (repository.UpstreamRef is null) return new BranchDivergence(DivergenceState.NoUpstream);
        var upstream = session.References.FirstOrDefault(reference => reference.Name == repository.UpstreamRef);
        if (upstream is null) return new BranchDivergence(DivergenceState.MissingUpstream);
        if (session.IsShallow) return new BranchDivergence(DivergenceState.Shallow);
        if (!IsObjectId(repository.Commit!) || !IsObjectId(upstream.Hash)) throw new GitReadException(GitFailure.InvalidOutput);
        var result = await RunAsync(git.Executable, repository.Worktree,
            ReadArguments(["rev-list", "--left-right", "--count", repository.Commit + "..." + upstream.Hash, "--"]), token, 4096).ConfigureAwait(false);
        RequireSuccess(result);
        var counts = Line(result.StandardOutput).Split('\t');
        if (counts.Length != 2 || !long.TryParse(counts[0], NumberStyles.None, CultureInfo.InvariantCulture, out var ahead) ||
            !long.TryParse(counts[1], NumberStyles.None, CultureInfo.InvariantCulture, out var behind)) throw new GitReadException(GitFailure.InvalidOutput);
        return new BranchDivergence(DivergenceState.Available, ahead, behind);
    }, token);
}
