using System.Globalization;
using System.IO;
using System.IO.Compression;
using System.Security.Cryptography;
using System.Text;
using GitGui.Core.Git;
using GitGui.Core.Processes;

namespace GitGui.FoundationChecks;

internal static partial class Program
{
    private static string? _historyRepository;
    private static string? _historyExecutable;
    private static string? _longHistoryRepository;

    private static async Task RunHistoryChecksAsync(RepositoryReader reader, GitInstallation git,
        Func<string, string[], Task<ProcessResult>> runGit)
    {
        var repo = Fixture("history — 日本語");
        Expect((await runGit(repo, ["init", "--initial-branch=main", "--object-format=sha1"])).ExitCode == 0, "Initialize isolated history repository");
        if (!Directory.Exists(Path.Combine(repo, ".git", "objects"))) throw new InvalidOperationException("Fixture is not initialized");
        var tree = WriteFixtureObject(repo, "tree", []);
        var root = Commit(repo, tree, [], "Root — שלום\n\nBody with |, tabs\tand separator \u001e text.\n");
        var left = Commit(repo, tree, [root], "Left branch\n");
        var right = Commit(repo, tree, [root], "Right branch\n");
        var merge = Commit(repo, tree, [left, right], "Merge description — 日本語\n\nA multiline message.\nSecond body line.\n", signed: true);
        Expect((await runGit(repo, ["update-ref", "refs/heads/main", merge])).ExitCode == 0, "Set fixture HEAD");
        _historyRepository = repo; _historyExecutable = git.Executable;

        await Check("Real merge history preserves messages/identities/dates and graph parent connections", async () =>
        {
            var history = await reader.ReadHistoryAsync(git, repo, merge, 200, default);
            var cli = await runGit(repo, ["rev-list", "--topo-order", merge, "--"]);
            Expect(history.Entries.Select(entry => entry.Commit.Hash).SequenceEqual(Encoding.UTF8.GetString(cli.StandardOutput).Split('\n', StringSplitOptions.RemoveEmptyEntries).Select(hash => hash.TrimEnd('\r'))), "Order must match Git's topological walk");
            Expect(history.Entries.Count == 4 && !history.HasMore && history.Entries[0].Commit.Parents.SequenceEqual([left, right]), "Merge must retain both parents");
            var first = history.Entries[0];
            Expect(first.Graph.IsMerge && first.Graph.Lines.Count(line => line.FromY == .5) == 2 && first.Graph.LaneCount == 2, "Merge node must branch to two real parents");
            Expect(first.Commit.Message.Contains("Second body line.") && first.Commit.Author == "Author 日本語" && first.Commit.Committer == "Committer שלום" && first.Commit.AuthoredAt.Offset == TimeSpan.FromHours(2), "Keep original identity, body and date offset");
            AssertGraphConnections(history.Entries);
        });
        await Check("History disables configured signature verification and does not depend on unborn HEAD", async () =>
        {
            var marker = Path.Combine(repo, "verifier-ran");
            var verifier = Path.Combine(repo, "verifier.sh");
            await File.WriteAllTextAsync(verifier, "#!/bin/sh\nprintf unsafe > '" + marker.Replace('\\', '/') + "'\n");
            await runGit(repo, ["config", "log.showSignature", "true"]);
            await runGit(repo, ["config", "gpg.program", verifier.Replace('\\', '/')]);
            Expect((await reader.ReadHistoryAsync(git, repo, merge, 200, default)).Entries.Count == 4 && !File.Exists(marker), "Do not invoke gpg for configured signed history");
            Expect((await reader.ReadHistoryAsync(git, repo, null, 200, default)).Entries.Count == 0, "Unborn history must be empty");
            await ExpectGitFailure(GitFailure.CommandFailed, () => reader.ReadHistoryAsync(git, repo, new string('f', 40), 200, default));
            using var stop = new CancellationTokenSource(); stop.Cancel();
            await ExpectException<OperationCanceledException>(() => reader.ReadHistoryAsync(git, repo, merge, 200, stop.Token));
        });
        await Check("Pinned incremental history survives external branch movement and exposes shallow limits", async () =>
        {
            var page = await reader.ReadHistoryAsync(git, repo, merge, 2, default);
            Expect(page.Entries.Count == 2 && page.HasMore, "Use lookahead to offer more history");
            var newer = Commit(repo, tree, [merge], "New external commit\n");
            await runGit(repo, ["update-ref", "refs/heads/main", newer]);
            var expanded = await reader.ReadHistoryAsync(git, repo, page.Anchor, 4, default);
            Expect(expanded.Entries.Count == 4 && expanded.Entries[0].Commit.Hash == merge && expanded.Entries.Take(2).Select(entry => entry.Graph.NodeLane).SequenceEqual(page.Entries.Select(entry => entry.Graph.NodeLane)), "Pinned rows and lanes remain stable");
            Expect((await reader.ReadHistoryAsync(git, repo, newer, 200, default)).Entries.Count == 5, "Explicit refresh can observe new tip");
            // Only the disposable fixture gets a shallow boundary; no transport/network is used.
            await File.WriteAllTextAsync(Path.Combine(repo, ".git", "shallow"), merge + "\n");
            var shallow = await reader.ReadHistoryAsync(git, repo, newer, 200, default);
            Expect(shallow.IsShallow && shallow.Entries.Count == 2 && !shallow.HasMore, "Shallow end is distinct from complete ancestry");
            File.Delete(Path.Combine(repo, ".git", "shallow"));
            await runGit(repo, ["update-ref", "refs/heads/main", merge]);
        });
        await Check("Large history is bounded with lookahead and no duplicate rows", async () =>
        {
            var large = Fixture("long history");
            Expect((await runGit(large, ["init", "--initial-branch=main", "--object-format=sha1"])).ExitCode == 0, "Initialize large fixture");
            var emptyTree = WriteFixtureObject(large, "tree", []);
            string? parent = null;
            for (var i = 0; i < 2005; i++) parent = Commit(large, emptyTree, parent is null ? [] : [parent], "History fixture " + i + "\n");
            await runGit(large, ["update-ref", "refs/heads/main", parent!]);
            _longHistoryRepository = large;
            var first = await reader.ReadHistoryAsync(git, large, parent, 200, default);
            var full = await reader.ReadHistoryAsync(git, large, parent, 2000, default);
            Expect(first.Entries.Count == 200 && full.Entries.Count == 2000 && full.HasMore && full.Entries.Select(entry => entry.Commit.Hash).Distinct().Count() == 2000, "Respect row cap without claiming history is complete");
            Expect(first.Entries.Select(entry => entry.Commit.Hash).SequenceEqual(full.Entries.Take(200).Select(entry => entry.Commit.Hash)), "Expansion preserves the prefix");
        });
        await Check("History parser rejects malformed boundaries and supports variable hash widths", async () =>
        {
            string Row(string hash, string message) => string.Join('\0', hash, "", "A", "a@example.invalid", "2026-10-23T12:30:00+02:00", "C", "c@example.invalid", "2026-10-23T11:00:00+00:00", message) + '\0';
            var bytes = Encoding.UTF8.GetBytes(Row(new string('a', 64), "Subject\n\nBody\t|\u001e\n"));
            Expect(RepositoryReader.ParseHistory(bytes)[0].Hash.Length == 64, "Do not assume SHA-1 hashes");
            await ExpectGitFailure(GitFailure.InvalidOutput, () => Task.FromResult(RepositoryReader.ParseHistory(bytes[..^1])));
            await ExpectGitFailure(GitFailure.InvalidOutput, () => Task.FromResult(RepositoryReader.ParseHistory(Encoding.UTF8.GetBytes(Row("--bad", "x")))));
            await ExpectGitFailure(GitFailure.InvalidOutput, () => Task.FromResult(RepositoryReader.ParseHistory(Encoding.UTF8.GetBytes(Row("abcd", "unexpected\0field")))));
        });
        await Check("Graph handles octopus/convergence and rejects cycles or excessive lanes", async () =>
        {
            CommitRecord Node(string hash, params string[] parents) => new(hash, parents, "", "", DateTimeOffset.UnixEpoch, "", "", DateTimeOffset.UnixEpoch, "");
            var commits = new[] { Node("aa", "bb", "cc", "dd"), Node("bb", "ee"), Node("cc", "ee"), Node("dd", "ee"), Node("ee") };
            var graph = CommitGraph.Build(commits);
            AssertGraphConnections(commits.Select((commit, index) => new HistoryEntry(commit, graph[index])).ToArray());
            Expect(graph[0].Lines.Count == 3 && graph[^1].LaneCount == 1, "Octopus converges to common ancestor");
            await ExpectGitFailure(GitFailure.InvalidOutput, () => Task.FromResult(CommitGraph.Build([Node("aa", "aa")])));
            await ExpectGitFailure(GitFailure.HistoryTooWide, () => Task.FromResult(CommitGraph.Build([Node("tip", Enumerable.Range(0, 65).Select(value => "p" + value).ToArray())])));
        });
    }

    private static void AssertGraphConnections(IReadOnlyList<HistoryEntry> rows)
    {
        for (var i = 0; i < rows.Count - 1; i++)
        {
            var bottom = rows[i].Graph.Lines.Where(line => line.ToY == 1).Select(line => (line.ToLane, line.Color)).Distinct().OrderBy(pair => pair.ToLane).ToArray();
            var top = rows[i + 1].Graph.Lines.Where(line => line.FromY == 0).Select(line => (line.FromLane, line.Color)).Distinct().OrderBy(pair => pair.FromLane).ToArray();
            Expect(bottom.SequenceEqual(top), "Every inter-row line must meet the same lane and color");
        }
    }

    // Raw Git object construction keeps large test setup fast; confined to newly initialized fixtures.
    private static string Commit(string repo, string tree, string[] parents, string message, bool signed = false)
    {
        var header = "tree " + tree + "\n" + string.Concat(parents.Select(parent => "parent " + parent + "\n")) +
            "author Author 日本語 <author@example.invalid> 1792747800 +0200\ncommitter Committer שלום <committer@example.invalid> 1792749600 +0000\n";
        if (signed) header += "gpgsig -----BEGIN PGP SIGNATURE-----\n fixture-signature\n -----END PGP SIGNATURE-----\n";
        return WriteFixtureObject(repo, "commit", Encoding.UTF8.GetBytes(header + "\n" + message));
    }

    private static string WriteFixtureObject(string repo, string type, byte[] content)
    {
        var objects = Path.Combine(repo, ".git", "objects");
        if (!Path.GetFullPath(repo).StartsWith(Path.GetFullPath(_root) + Path.DirectorySeparatorChar, StringComparison.OrdinalIgnoreCase) || !Directory.Exists(objects))
            throw new InvalidOperationException("Only initialized disposable fixtures are allowed");
        var header = Encoding.ASCII.GetBytes(type + " " + content.Length.ToString(CultureInfo.InvariantCulture) + '\0');
        var payload = header.Concat(content).ToArray();
        var hash = Convert.ToHexString(SHA1.HashData(payload)).ToLowerInvariant();
        var folder = Path.Combine(objects, hash[..2]); Directory.CreateDirectory(folder);
        var file = Path.Combine(folder, hash[2..]);
        if (!File.Exists(file))
        {
            using var output = File.Create(file);
            using var zipped = new ZLibStream(output, CompressionLevel.Fastest);
            zipped.Write(payload);
        }
        return hash;
    }
}
