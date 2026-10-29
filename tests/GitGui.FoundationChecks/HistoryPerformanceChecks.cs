using System.Diagnostics;
using System.Globalization;
using System.IO;
using System.Text;
using GitGui.Core.Git;

namespace GitGui.FoundationChecks;

internal static partial class Program
{
    // Small correctness checks only. Neither this method nor default dispatch runs a large fixture.
    // Registration is intentionally left to integration; see docs/handoffs/task-3.md.
    private static async Task RunHistoryPerformanceChecksAsync()
    {
        await Check("History performance options require explicit expensive/idle opt-in", () =>
        {
            var normal = HistoryPerformanceOptions.Parse(["--measure-history-performance", "merge-heavy"]);
            Expect(!normal.IdleFiveMinutes && normal.Samples == 3 && HistoryPerformanceSpec.For(normal.Profile).Commits == 20_000, "Ordinary opt-in has no million/idle work");
            var million = HistoryPerformanceOptions.Parse(["--measure-history-performance", "million", "--idle-five-minutes", "--samples=1", "--cancel-after-ms=1"]);
            Expect(million.IdleFiveMinutes && million.CancelAfterMs == 1 && HistoryPerformanceSpec.For(million.Profile).Commits == 1_000_000, "Explicit large options retained");
            foreach (string[] invalid in new string[][]
            {
                [], ["--measure-history-performance"], ["--measure-history-performance", "user-repo"],
                ["--measure-history-performance", "large", "--samples=0"],
                ["--measure-history-performance", "large", "--samples=21"],
                ["--measure-history-performance", "large", "--cancel-after-ms=0"],
                ["--measure-history-performance", "large", "--samples=2", "--samples=3"],
                ["--measure-history-performance", "large", "--repository=C:\\not-a-fixture"]
            })
            {
                try { HistoryPerformanceOptions.Parse(invalid); throw new InvalidOperationException("Accepted invalid options"); }
                catch (ArgumentException) { }
            }
            return Task.CompletedTask;
        });
        await Check("History performance import is deterministic and bounded per record", async () =>
        {
            var spec = new HistoryPerformanceSpec("small", 803, true, 15);
            using var first = new StringWriter(CultureInfo.InvariantCulture);
            using var second = new StringWriter(CultureInfo.GetCultureInfo("ar-SA"));
            await HistoryPerformanceFixtures.WriteImportAsync(first, spec, default);
            await HistoryPerformanceFixtures.WriteImportAsync(second, spec, default);
            Expect(first.ToString() == second.ToString(), "Import protocol must be culture invariant");
            Expect(first.ToString().Split('\n').Count(line => line.StartsWith("commit refs/", StringComparison.Ordinal)) == spec.Commits + 1, "Visible commits plus excluded stash sentinel");
            Expect(first.ToString().Split('\n').Count(line => line.StartsWith("merge :", StringComparison.Ordinal)) == spec.Merges, "Actual merge parent records");
            Expect(spec.Parents(1).Length == 0 && spec.Parents(2).SequenceEqual([1]) && spec.Parents(3).SequenceEqual([1]) && spec.Parents(4).SequenceEqual([2, 3]), "Diamond diverges and rejoins");
            Expect(Enumerable.Range(2, spec.Commits).All(id => spec.Parents(id).All(parent => parent < id)), "Only previously emitted marks are referenced");
        });
        await Check("History performance streaming cancellation stops generation without full-history buffering", async () =>
        {
            using var cancel = new CancellationTokenSource();
            using var writer = new HistoryPerformanceCancellingWriter(cancel);
            try
            {
                await HistoryPerformanceFixtures.WriteImportAsync(writer, HistoryPerformanceSpec.For("million"), cancel.Token);
                throw new InvalidOperationException("Ignored generator cancellation");
            }
            catch (OperationCanceledException) when (cancel.IsCancellationRequested) { }
            Expect(writer.Records == 10 && writer.LargestRecord < 1024, "Cancel after ten bounded records, without generating the million fixture");
        });
        await Check("History performance cleanup refuses wrong owner and path escape", () =>
        {
            using var area = new HistoryPerformanceArea();
            var child = area.Create("child"); File.WriteAllText(Path.Combine(child, "keep"), "fixture");
            foreach (var root in new[] { area.Root, Path.GetTempPath(), child })
            {
                try { HistoryPerformanceArea.DeleteOwned(root, "wrong-marker"); throw new InvalidOperationException("Deleted unowned path"); }
                catch (IOException) { }
            }
            Expect(File.Exists(Path.Combine(child, "keep")), "Failed cleanup authorization preserves fixture");
            return Task.CompletedTask;
        });
        await Check("History performance disposable merge fixture matches all paged topology and scope", async () =>
        {
            using var deadline = new CancellationTokenSource(TimeSpan.FromMinutes(2));
            var token = deadline.Token;
            using var area = new HistoryPerformanceArea();
            var runner = new HistoryPerformanceRunner(area);
            var reader = new RepositoryReader(runner);
            var git = await reader.DiscoverAsync(null, token);
            var spec = new HistoryPerformanceSpec("small", 803, true, 15);
            var repo = await HistoryPerformanceFixtures.CreateAsync(area, runner, git, spec, "paged", token);
            await HistoryPerformanceHarness.ValidateFixtureAsync(runner, git, repo, spec, token);
            var page = await reader.ReadInitialHistoryAsync(git, await reader.InspectAsync(git, repo, token), HistoryScope.AllCommits, token);
            var commits = new List<CommitRecord>(); var graph = new List<GraphRow>(); var seen = new HashSet<string>(StringComparer.Ordinal);
            while (true)
            {
                Expect(page.Cursor.Offset == commits.Count && page.Entries.Count is > 0 and <= 200, "Actual cursor range and unmodified page bound");
                foreach (var entry in page.Entries)
                {
                    Expect(seen.Add(entry.Commit.Hash) && entry.Commit.Parents.All(parent => !seen.Contains(parent)), "Unique child-before-parent order across page edges");
                    commits.Add(entry.Commit); graph.Add(entry.Graph);
                }
                if (page.Next is not { } next) break;
                page = await reader.ReadHistoryPageAsync(git, page.Session, next, token);
            }
            Expect(commits.Count == spec.Commits && commits.Count(commit => commit.Parents.Count == 2) == spec.Merges, "Complete history, actual merge count, no stash-only commit");
            var hashes = commits.ToDictionary(commit => int.Parse(commit.Subject[19..], CultureInfo.InvariantCulture), commit => commit.Hash);
            foreach (var commit in commits)
            {
                var id = int.Parse(commit.Subject[19..], CultureInfo.InvariantCulture);
                Expect(commit.Parents.SequenceEqual(spec.Parents(id).Select(parent => hashes[parent])), "Imported parent identities match generated diamonds");
            }
            var wholeGraph = CommitGraph.Build(commits);
            Expect(wholeGraph.Zip(graph).All(pair => pair.First.NodeLane == pair.Second.NodeLane && pair.First.NodeColor == pair.Second.NodeColor &&
                pair.First.LaneCount == pair.Second.LaneCount && pair.First.IsMerge == pair.Second.IsMerge && pair.First.Lines.SequenceEqual(pair.Second.Lines)), "Sequential pages preserve full-graph lane/color/edge continuity");
            using var cancelled = new CancellationTokenSource(); cancelled.Cancel();
            try { await reader.ReadHistoryPageAsync(git, page.Session, HistoryCursor.Start, cancelled.Token); throw new InvalidOperationException("Cancelled read succeeded"); }
            catch (OperationCanceledException) { }
            catch (GitReadException error) when (error.Failure == GitFailure.Cancelled) { }
            var after = await reader.ReadHistoryPageAsync(git, page.Session, HistoryCursor.Start, token);
            Expect(after.Entries.Count == 200 && after.Entries[0].Commit.Hash == commits[0].Hash, "Runner remains usable after cancelled read");
            Expect(runner.IsolatedEnvironment()["GIT_CONFIG_GLOBAL"] == area.Config && runner.IsolatedEnvironment()["GIT_CONFIG_NOSYSTEM"] == "1", "Fixture config is isolated");
        });
        await Check("History performance resource sampler observes only this host's child and normalizes CPU", async () =>
        {
            Expect(HistoryPerformanceSampler.TotalMachinePercent(1000, 1000, 4) == 25, "One full core on a four-core machine is 25 percent total CPU");
            using var child = new Process { StartInfo = new(ProbeExecutable) { UseShellExecute = false, CreateNoWindow = true, ArgumentList = { "--probe", "wait" } } };
            child.Start();
            try
            {
                await using var sampler = new HistoryPerformanceSampler(20);
                await Task.Delay(250);
                var resources = await sampler.FinishAsync();
                Expect(resources.Samples >= 2 && resources.HostPrivatePeakBytes > 0 && resources.ObservedChildren >= 1 && resources.LastObservedLiveChildren >= 1, "Live owned probe child is sampled separately from host");
                Expect(resources.CombinedPrivatePeakBytes >= resources.HostPrivatePeakBytes && resources.HostTotalMachineCpuPercent >= 0, "Resource units and simultaneous peak are consistent");
            }
            finally
            {
                if (!child.HasExited) child.Kill(entireProcessTree: true);
                await child.WaitForExitAsync().WaitAsync(TimeSpan.FromSeconds(10));
            }
        });
    }
    private sealed class HistoryPerformanceCancellingWriter(CancellationTokenSource cancel) : TextWriter
    {
        public override Encoding Encoding => Encoding.UTF8;
        public int Records { get; private set; }
        public int LargestRecord { get; private set; }
        public override Task WriteAsync(ReadOnlyMemory<char> buffer, CancellationToken cancellationToken = default)
        {
            cancellationToken.ThrowIfCancellationRequested();
            LargestRecord = Math.Max(LargestRecord, buffer.Length);
            if (++Records == 10) cancel.Cancel();
            return Task.CompletedTask;
        }
    }
}
