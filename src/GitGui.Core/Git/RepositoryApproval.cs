namespace GitGui.Core.Git;

public sealed record RepositoryIdentity(string Worktree, string GitDirectory, string CommonDirectory, string Fingerprint);
public sealed record RepositoryApproval(string Path, string Fingerprint);
public interface IRepositoryIdentityProvider
{
    // Filesystem-only validation, before Git discovery/commands. Never execute repository content.
    Task<RepositoryIdentity> ReadAsync(string path, CancellationToken token);
}
