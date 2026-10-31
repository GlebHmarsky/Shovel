namespace GitGui.Core.Git;

public sealed partial class RepositoryReader
{
    // Index-only readiness check: no working-file scan, untracked enumeration or external filters.
    public Task<bool> HasUnmergedFilesAsync(GitInstallation git, string path, CancellationToken token) => Task.Run(async () =>
    {
        var result = await RunAsync(git.Executable, path, ReadArguments(["ls-files", "--unmerged", "-z"]), token, MaximumStatusBytes);
        RequireSuccess(result);
        return result.StandardOutput.Length != 0;
    }, token);
}
