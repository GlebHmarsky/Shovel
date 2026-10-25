using GitGui.Core.Git;
using GitGui.Core.Processes;
using System.IO;

namespace GitGui.FoundationChecks;

internal static partial class Program
{
    private static async Task RunDivergenceChecksAsync(RepositoryReader reader, GitInstallation git,
        Func<string, string[], Task<ProcessResult>> runGit)
    {
        var repo = Fixture("upstream — 日本語");
        await runGit(repo, ["init", "--initial-branch=main", "--object-format=sha1"]);
        var tree = WriteFixtureObject(repo, "tree", []);
        var root = Commit(repo, tree, [], "Root\n");
        var left = Commit(repo, tree, [root], "Local one\n");
        var tip = Commit(repo, tree, [left], "Local two\n");
        var right = Commit(repo, tree, [root], "Upstream one\n");
        await runGit(repo, ["config", "remote.team.url", "https://example.invalid/never-contact"]);
        await runGit(repo, ["config", "remote.team.fetch", "+refs/heads/*:refs/remotes/team/*"]);
        await runGit(repo, ["config", "branch.main.remote", "team"]);
        await runGit(repo, ["config", "branch.main.merge", "refs/heads/topic"]);
        await runGit(repo, ["config", "branch.main.pushRemote", "different-destination"]);
        await Check("Ahead/behind counts match local upstream ancestry for sync, ahead, behind and divergence", async () =>
        {
            foreach (var (head, upstream, ahead, behind) in new[] { (root, root, 0L, 0L), (tip, root, 2L, 0L), (root, right, 0L, 1L), (tip, right, 2L, 1L) })
            {
                await runGit(repo, ["update-ref", "refs/heads/main", head]);
                await runGit(repo, ["update-ref", "refs/remotes/team/topic", upstream]);
                var session = await Session();
                Expect(session.Repository.UpstreamRef == "refs/remotes/team/topic", "Configured upstream is used, not assumed origin/main or push destination");
                var counts = await reader.ReadDivergenceAsync(git, session, default);
                Expect(counts == new BranchDivergence(DivergenceState.Available, ahead, behind), "Exact unique ancestry counts");
            }
        });
        await Check("Divergence has explicit missing, unconfigured, detached, unborn and shallow states", async () =>
        {
            var session = await Session();
            // Comparison stays tied to the immutable displayed tips even if an external client moves refs.
            await runGit(repo, ["update-ref", "refs/remotes/team/topic", tip]);
            Expect(await reader.ReadDivergenceAsync(git, session, default) == new BranchDivergence(DivergenceState.Available, 2, 1), "No mixing of newer refs with displayed snapshot");
            await File.WriteAllTextAsync(Path.Combine(repo, ".git", "shallow"), tip + "\n");
            Expect((await reader.ReadDivergenceAsync(git, await Session(), default)).State == DivergenceState.Shallow, "Incomplete ancestry is not presented as full counts");
            File.Delete(Path.Combine(repo, ".git", "shallow"));
            await runGit(repo, ["update-ref", "-d", "refs/remotes/team/topic"]);
            Expect((await reader.ReadDivergenceAsync(git, await Session(), default)).State == DivergenceState.MissingUpstream, "Gone upstream is not zero");
            await runGit(repo, ["config", "--unset", "branch.main.remote"]);
            Expect((await reader.ReadDivergenceAsync(git, await Session(), default)).State == DivergenceState.NoUpstream, "Unconfigured upstream is not zero");
            await runGit(repo, ["checkout", "--detach", tip]);
            Expect((await reader.ReadDivergenceAsync(git, await Session(), default)).State == DivergenceState.Detached, "Detached has no branch upstream");
            var empty = Fixture("upstream-unborn"); await runGit(empty, ["init", "--initial-branch=main"]);
            var unborn = await reader.ReadInitialHistoryAsync(git, await reader.InspectAsync(git, empty, default), HistoryScope.AllCommits, default);
            Expect((await reader.ReadDivergenceAsync(git, unborn.Session, default)).State == DivergenceState.Unborn, "Unborn comparison is unavailable");
        });
        async Task<HistorySession> Session() => (await reader.ReadInitialHistoryAsync(git,
            await reader.InspectAsync(git, repo, default), HistoryScope.AllCommits, default)).Session;
    }
}
