using System.Text;

namespace GitGui.Core.Git;

public sealed record SidebarBranch(string Ref, string Tracking);
public sealed record SubmoduleEntry(string Path, string Hash);
public sealed record RepositorySidebar(IReadOnlyList<SidebarBranch> Branches, IReadOnlyList<SubmoduleEntry> Submodules);

public sealed partial class RepositoryReader
{
    public Task<RepositorySidebar> ReadSidebarAsync(GitInstallation git, string path, CancellationToken token) => Task.Run(async () =>
    {
        var branches = await RunAsync(git.Executable, path, ReadArguments(["for-each-ref", "--format=%(refname)%00%(upstream:track,nobracket)", "refs/heads/"]), token, MaximumHistoryBytes);
        RequireSuccess(branches);
        var index = await RunAsync(git.Executable, path, ReadArguments(["ls-files", "--stage", "-z"]), token, MaximumHistoryBytes);
        RequireSuccess(index);
        var modules = new List<SubmoduleEntry>();
        foreach (var record in Text(index.StandardOutput).Split('\0', StringSplitOptions.RemoveEmptyEntries))
        {
            var tab = record.IndexOf('\t'); if (tab < 0) throw new GitReadException(GitFailure.InvalidOutput);
            var fields = record[..tab].Split(' ');
            if (fields[0] == "160000" && fields.Length == 3 && fields[2] == "0") modules.Add(new(record[(tab + 1)..], fields[1]));
        }
        return new RepositorySidebar(Text(branches.StandardOutput).Split('\n', StringSplitOptions.RemoveEmptyEntries).Select(line =>
        {
            var parts = line.Split('\0'); if (parts.Length != 2) throw new GitReadException(GitFailure.InvalidOutput);
            return new SidebarBranch(parts[0], parts[1]);
        }).ToArray(), modules);
    }, token);

    public Task<CommitHistory> LocateHistoryAsync(GitInstallation git, HistorySession session, string hash, CancellationToken token) => Task.Run(async () =>
    {
        if (!IsObjectId(hash)) throw new GitReadException(GitFailure.InvalidOutput);
        var result = await RunAsync(git.Executable, session.Repository.Worktree, ReadArguments(["rev-list", "--topo-order", "--branches", "--remotes", "--tags",
            .. session.References.Where(r => r.Name == "HEAD" || r.Name.StartsWith("stash@{", StringComparison.Ordinal)).Select(r => r.Hash), "--"]), token, 16 * 1024 * 1024);
        RequireSuccess(result);
        var raw = Text(result.StandardOutput).Split('\n', StringSplitOptions.RemoveEmptyEntries);
        var hashes = raw.Where(id => !session.HiddenStashCommits.Contains(id)).ToArray();
        var index = Array.IndexOf(hashes, hash); if (index < 0) throw new GitReadException(GitFailure.ChangedDuringRead);
        var offset = index / HistoryPageSize * HistoryPageSize;
        return await ReadHistoryPageAsync(git, session, new(offset, GraphCursor.Start, hashes[offset], Array.IndexOf(raw, hashes[offset])), token);
    }, token);
}

public sealed partial class GitActions
{
    private async Task ValidateSubmoduleAsync(ActionContext context, ActionOptions options, CancellationToken token)
    {
        var modules = await reader.ReadSidebarAsync(context.Git, context.Identity.Worktree, token);
        if (!modules.Submodules.Any(m => m.Path == options.Branch && m.Hash == options.StartRef)) throw new GitActionException("ActionChanged");
        ValidateSubmodulePath(context.Identity.Worktree, options.Branch);
    }
    public static string ValidateSubmodulePath(string root, string relative)
    {
        if (Path.IsPathRooted(relative) || relative.Split('/', '\\').Any(p => p is "" or "." or ".." || p.Equals(".git", StringComparison.OrdinalIgnoreCase)))
            throw new GitActionException("SidebarSubmodulePath");
        var full = Path.GetFullPath(Path.Combine(root, relative));
        if (!full.StartsWith(Path.TrimEndingDirectorySeparator(root) + Path.DirectorySeparatorChar, StringComparison.OrdinalIgnoreCase)) throw new GitActionException("SidebarSubmodulePath");
        var current = root;
        foreach (var part in relative.Split('/', '\\'))
        {
            current = Path.Combine(current, part);
            if (Path.Exists(current) && (File.GetAttributes(current) & FileAttributes.ReparsePoint) != 0) throw new GitActionException("SidebarSubmodulePath");
        }
        return full;
    }
}
