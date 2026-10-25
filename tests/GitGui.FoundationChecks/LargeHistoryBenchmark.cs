using System.Diagnostics;
using System.IO;
using System.Text;
using System.Text.Json;
using GitGui.Core.Git;
using GitGui.ProcessHost;

namespace GitGui.FoundationChecks;

internal static partial class Program
{
    private static async Task RunLargeHistoryBenchmarkAsync()
    {
        foreach (System.Collections.DictionaryEntry variable in Environment.GetEnvironmentVariables())
            if (((string)variable.Key).StartsWith("GIT_", StringComparison.OrdinalIgnoreCase)) Environment.SetEnvironmentVariable((string)variable.Key, null);
        var config = Path.Combine(_root, "empty.config"); await File.WriteAllTextAsync(config, "");
        Environment.SetEnvironmentVariable("GIT_CONFIG_GLOBAL", config); Environment.SetEnvironmentVariable("GIT_CONFIG_NOSYSTEM", "1");
        Environment.SetEnvironmentVariable("GIT_CEILING_DIRECTORIES", _root);
        var runner = new WindowsProcessRunner(); var reader = new RepositoryReader(runner);
        var git = await reader.DiscoverAsync(null, default); var repo = Fixture("packed-100k");
        Expect((await runner.RunAsync(new(git.Executable, repo, ["init", "--initial-branch=main", "--object-format=sha1"]), default)).ExitCode == 0, "Disposable init");
        // Fixture-only mutation: fast-import creates packed objects with bounded streaming input.
        using (var importer = new Process { StartInfo = new(git.Executable)
        {
            WorkingDirectory = repo, UseShellExecute = false, CreateNoWindow = true,
            RedirectStandardInput = true, RedirectStandardOutput = true, RedirectStandardError = true,
            StandardInputEncoding = new UTF8Encoding(false), ArgumentList = { "fast-import", "--quiet" }
        } })
        {
            importer.Start(); importer.StandardInput.AutoFlush = false;
            var output = importer.StandardOutput.ReadToEndAsync(); var error = importer.StandardError.ReadToEndAsync();
            for (var i = 1; i <= 100000; i++)
            {
                var message = $"Packed commit {i}\n";
                await importer.StandardInput.WriteAsync($"commit refs/heads/main\nmark :{i}\ncommitter Fixture <fixture@example.invalid> {1700000000 + i} +0000\ndata {Encoding.UTF8.GetByteCount(message)}\n{message}");
                if (i > 1) await importer.StandardInput.WriteAsync($"from :{i - 1}\n");
                await importer.StandardInput.WriteAsync("\n");
            }
            for (var i = 1; i <= 1000; i++)
                await importer.StandardInput.WriteAsync($"reset refs/heads/branch-{i}\nfrom :{i * 100}\n\n");
            for (var i = 1; i <= 100; i++)
                await importer.StandardInput.WriteAsync($"reset refs/tags/tag-{i}\nfrom :{i * 1000}\n\nreset refs/remotes/origin/branch-{i}\nfrom :{i * 1000}\n\n");
            importer.StandardInput.Close(); await importer.WaitForExitAsync(); await Task.WhenAll(output, error);
            Expect(importer.ExitCode == 0, "Packed fixture import");
        }
        var samples = new List<object>();
        foreach (var (withGraph, packedRefs) in new[] { (false, false), (true, false), (true, true) })
        {
            if (packedRefs) Expect((await runner.RunAsync(new(git.Executable, repo, ["pack-refs", "--all"]), default)).ExitCode == 0, "Fixture-only packed refs");
            if (withGraph && !packedRefs) Expect((await runner.RunAsync(new(git.Executable, repo, ["commit-graph", "write", "--reachable"]), default)).ExitCode == 0, "Fixture-only commit graph");
            for (var i = 0; i < 3; i++)
            {
                var watch = Stopwatch.StartNew();
                var installation = await reader.DiscoverAsync(null, default);
                var repository = await reader.InspectAsync(installation, repo, default);
                var page = await reader.ReadInitialHistoryAsync(installation, repository, HistoryScope.AllCommits, default);
                var firstMs = watch.Elapsed.TotalMilliseconds;
                Expect(page.Entries.Count == 200 && page.HasMore && page.Entries[0].Commit.Subject == "Packed commit 100000", "First page includes actual latest history");
                watch.Restart(); var deep = await reader.ReadHistoryPageAsync(git, page.Session, new HistoryCursor(99800, GraphCursor.Start), default);
                Expect(deep.Entries.Count == 200 && !deep.HasMore && deep.Entries[^1].Commit.Subject == "Packed commit 1", "Oldest commits remain reachable");
                samples.Add(new { iteration = i, withCommitGraph = withGraph, packedRefs, firstMs, deepMs = watch.Elapsed.TotalMilliseconds });
            }
        }
        await File.WriteAllTextAsync(Path.Combine(_root, "large-history-timings.json"), JsonSerializer.Serialize(samples, new JsonSerializerOptions { WriteIndented = true }));
        Console.WriteLine($"Large-history timings: {_root}");
    }
}
