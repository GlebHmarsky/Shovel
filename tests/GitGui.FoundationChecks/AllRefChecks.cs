using System.Text;
using GitGui.Core.Git;
using GitGui.Core.Processes;

namespace GitGui.FoundationChecks;

internal static partial class Program
{
    private static async Task RunAllRefChecksAsync(RepositoryReader reader, GitInstallation git,
        Func<string, string[], Task<ProcessResult>> runGit)
    {
        await Check("All Commits includes branch, remote and tag ancestry and current HEAD but excludes reflog-only commits", async () =>
        {
            var all = Fixture("all refs — 日本語");
            await runGit(all, ["init", "--initial-branch=main", "--object-format=sha1"]);
            var tree = WriteFixtureObject(all, "tree", []);
            var root = Commit(all, tree, [], "All root\n");
            var a = Commit(all, tree, [root], "Main work\n");
            var b = Commit(all, tree, [root], "Merged feature\n");
            var tip = Commit(all, tree, [a, b], "Main merge\n");
            var feature = Commit(all, tree, [root], "Unmerged branch\n");
            var remote = Commit(all, tree, [root], "Remote-only work\n");
            var tagged = Commit(all, tree, [root], "Tag-only work\n\n" + new string('x', 100000));
            var ghost = Commit(all, tree, [root], "Detached / reflog-only work\n");
            await runGit(all, ["update-ref", "--create-reflog", "refs/heads/main", ghost]);
            await runGit(all, ["update-ref", "refs/heads/main", tip]);
            await runGit(all, ["update-ref", "refs/heads/feature-日本語", feature]);
            await runGit(all, ["update-ref", "refs/remotes/origin/remote-only", remote]);
            await runGit(all, ["update-ref", "refs/tags/lightweight", root]);
            var tag = WriteFixtureObject(all, "tag", Encoding.UTF8.GetBytes("object " + tagged + "\ntype commit\ntag release\ntagger Test <test@example.invalid> 1792749600 +0000\n\nAnnotated tag\n"));
            await runGit(all, ["update-ref", "refs/tags/release", tag]);
            var nested = WriteFixtureObject(all, "tag", Encoding.UTF8.GetBytes("object " + tag + "\ntype tag\ntag nested\ntagger Test <test@example.invalid> 1792749600 +0000\n\nNested tag\n"));
            await runGit(all, ["update-ref", "refs/tags/nested", nested]);
            await runGit(all, ["update-ref", "refs/tags/tree", tree]);
            await runGit(all, ["update-ref", "refs/stash", ghost]);
            await runGit(all, ["pack-refs", "--all"]);
            _allHistoryRepository = all;
            var snapshot = await reader.InspectAsync(git, all, default);
            var session = await reader.OpenHistoryAsync(git, snapshot, HistoryScope.AllCommits, default);
            var first = await reader.ReadHistoryPageAsync(git, session, HistoryCursor.Start, default, 2);
            var collected = new List<HistoryEntry>(first.Entries);
            var next = first.Next;
            while (next is not null)
            {
                var page = await reader.ReadHistoryPageAsync(git, session, next, default, 2);
                collected.AddRange(page.Entries); next = page.Next;
            }
            var cli = await runGit(all, ["rev-list", "--topo-order", "--branches", "--remotes", "--tags", "--"]);
            Expect(collected.Count == 7 && collected.Select(row => row.Commit.Hash).SequenceEqual(TextLines(cli)) && !collected.Any(row => row.Commit.Hash == ghost), "All named branch/tag ancestors only, including packed refs");
            AssertGraphConnections(collected);
            Expect(collected.Single(row => row.Commit.Hash == tagged).References!.Select(reference => reference.Name).Order().SequenceEqual(new[] { "refs/tags/nested", "refs/tags/release" }), "Peel annotated/nested tags to the labeled commit");
            var repeat = await reader.ReadHistoryPageAsync(git, session, HistoryCursor.Start, default, 2);
            Expect(repeat.Entries.Select(row => row.Commit.Hash).SequenceEqual(first.Entries.Select(row => row.Commit.Hash)), "Newer navigation reproduces the page");
            await runGit(all, ["checkout", "--detach", ghost]);
            var detached = await reader.InspectAsync(git, all, default);
            var current = await reader.OpenHistoryAsync(git, detached, HistoryScope.CurrentHead, default);
            Expect((await reader.ReadHistoryPageAsync(git, current, HistoryCursor.Start, default)).Entries.First().Commit.Hash == ghost, "Detached ancestry belongs to Current branch / HEAD");
            var allDetached = await reader.OpenHistoryAsync(git, detached, HistoryScope.AllCommits, default);
            Expect((await reader.ReadHistoryPageAsync(git, allDetached, HistoryCursor.Start, default)).Entries.Any(row => row.Commit.Hash == ghost), "All Commits includes the current detached HEAD without requiring a filter");
            await runGit(all, ["checkout", "main"]);
            await runGit(all, ["update-ref", "-d", "refs/heads/feature-日本語"]);
            await ExpectGitFailure(GitFailure.ChangedDuringRead, () => reader.ReadHistoryPageAsync(git, session, first.Next!, default));
            await runGit(all, ["update-ref", "refs/heads/feature-日本語", feature]);
        });
    }
}
