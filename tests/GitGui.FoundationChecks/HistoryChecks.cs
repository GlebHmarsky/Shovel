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
    private static string? _allHistoryRepository;

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
            var history = await ReadHead( merge, 200, default);
            var cli = await runGit(repo, ["rev-list", "--topo-order", merge, "--"]);
            Expect(history.Entries.Select(entry => entry.Commit.Hash).SequenceEqual(Encoding.UTF8.GetString(cli.StandardOutput).Split('\n', StringSplitOptions.RemoveEmptyEntries).Select(hash => hash.TrimEnd('\r'))), "Order must match Git's topological walk");
            Expect(history.Entries.Count == 4 && !history.HasMore && history.Entries[0].Commit.Parents.SequenceEqual([left, right]), "Merge must retain both parents");
            var first = history.Entries[0];
            Expect(first.Graph.IsMerge && first.Graph.Lines.Count(line => line.FromY == .5) == 2 && first.Graph.LaneCount == 2, "Merge node must branch to two real parents");
            Expect(first.Commit.Message.Contains("Second body line.") && first.Commit.Author == "Author 日本語" && first.Commit.Committer == "Committer שלום" && first.Commit.AuthoredAt.Offset == TimeSpan.FromHours(2), "Keep original identity, body and date offset");
            AssertGraphConnections(history.Entries);
        });
        await Check("History disables signature helpers and rejects stale HEAD metadata", async () =>
        {
            var marker = Path.Combine(repo, "verifier-ran");
            var verifier = Path.Combine(repo, "verifier.sh");
            await File.WriteAllTextAsync(verifier, "#!/bin/sh\nprintf unsafe > '" + marker.Replace('\\', '/') + "'\n");
            await runGit(repo, ["config", "log.showSignature", "true"]);
            await runGit(repo, ["config", "gpg.program", verifier.Replace('\\', '/')]);
            Expect((await ReadHead( merge, 200, default)).Entries.Count == 4 && !File.Exists(marker), "Do not invoke gpg for configured signed history");
            await ExpectGitFailure(GitFailure.ChangedDuringRead, () => ReadHead(null, 200, default));
            await ExpectGitFailure(GitFailure.ChangedDuringRead, () => ReadHead( new string('f', 40), 200, default));
            using var stop = new CancellationTokenSource(); stop.Cancel();
            await ExpectException<OperationCanceledException>(() => ReadHead( merge, 200, stop.Token));
        });
        await Check("Ref movement requires refresh and shallow history exposes local boundaries", async () =>
        {
            var page = await ReadHead( merge, 2, default);
            Expect(page.Entries.Count == 2 && page.HasMore, "Use lookahead to offer more history");
            var newer = Commit(repo, tree, [merge], "New external commit\n");
            await runGit(repo, ["update-ref", "refs/heads/main", newer]);
            await ExpectGitFailure(GitFailure.ChangedDuringRead, () => reader.ReadHistoryPageAsync(git, page.Session, page.Next!, default));
            Expect((await ReadHead( newer, 200, default)).Entries.Count == 5, "Explicit refresh can observe new tip");
            // Only the disposable fixture gets a shallow boundary; no transport/network is used.
            await File.WriteAllTextAsync(Path.Combine(repo, ".git", "shallow"), merge + "\n");
            var shallow = await ReadHead( newer, 200, default);
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
            var session = await reader.OpenHistoryAsync(git, await reader.InspectAsync(git, large, default), HistoryScope.AllCommits, default);
            var rows = new List<HistoryEntry>();
            var cursor = HistoryCursor.Start;
            do
            {
                var page = await reader.ReadHistoryPageAsync(git, session, cursor, default);
                Expect(page.Entries.Count <= 200, "Bound each page without a total commit-count limit");
                rows.AddRange(page.Entries);
                if (page.Next is null) break;
                cursor = page.Next;
            } while (true);
            Expect(rows.Count == 2005 && rows.Select(entry => entry.Commit.Hash).Distinct().Count() == 2005, "Every commit beyond the old 2,000 ceiling is reachable through pagination");
            AssertGraphConnections(rows);
            var cli = await runGit(large, ["rev-list", "--topo-order", "--branches", "--remotes", "--tags", "--"]);
            Expect(rows.Select(row => row.Commit.Hash).SequenceEqual(TextLines(cli)), "No missing, duplicate or reordered commits across pages");
        });
        await RunAllRefChecksAsync(reader, git, runGit);
        await Check("Empty and unborn HEAD repositories retain truthful all-ref history", async () =>
        {
            var empty = Fixture("empty history"); await runGit(empty, ["init", "--initial-branch=main"]);
            var snapshot = await reader.InspectAsync(git, empty, default);
            foreach (var scope in Enum.GetValues<HistoryScope>())
            {
                var session = await reader.OpenHistoryAsync(git, snapshot, scope, default);
                var page = await reader.ReadHistoryPageAsync(git, session, HistoryCursor.Start, default);
                Expect(page.Entries.Count == 0 && !page.HasMore, "No fallback to HEAD in an empty All Commits view");
            }
            await runGit(repo, ["symbolic-ref", "HEAD", "refs/heads/unborn"]);
            var unborn = await reader.InspectAsync(git, repo, default);
            var all = await reader.OpenHistoryAsync(git, unborn, HistoryScope.AllCommits, default);
            Expect((await reader.ReadHistoryPageAsync(git, all, HistoryCursor.Start, default)).Entries.Count == 4, "Unborn current branch must not hide other branch history");
            await runGit(repo, ["symbolic-ref", "HEAD", "refs/heads/main"]);
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
        async Task<CommitHistory> ReadHead(string? hash, int count, CancellationToken token)
        {
            var snapshot = await reader.InspectAsync(git, repo, token);
            var session = await reader.OpenHistoryAsync(git, snapshot with { Commit = hash }, HistoryScope.CurrentHead, token);
            return await reader.ReadHistoryPageAsync(git, session, HistoryCursor.Start, token, count);
        }
    }

    private static string[] TextLines(ProcessResult result) => Encoding.UTF8.GetString(result.StandardOutput).Split('\n', StringSplitOptions.RemoveEmptyEntries).Select(line => line.TrimEnd('\r')).ToArray();

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
