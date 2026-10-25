using System.Diagnostics;
using System.IO;
using System.Text;
using System.Text.Json;
using System.Windows;
using GitGui.App.ViewModels;
using GitGui.App.Views;
using GitGui.Core.Git;
using GitGui.Core.Processes;
using GitGui.Core.Settings;
using GitGui.ProcessHost;

namespace GitGui.FoundationChecks;

internal static partial class Program
{
    private static void MeasureShellConstruction()
    {
        var samples = new List<object>();
        var watch = Stopwatch.StartNew();
        var app = new GitGui.App.App(); app.InitializeComponent();
        var applicationMs = watch.Elapsed.TotalMilliseconds;
        for (var i = 0; i < 3; i++)
        {
            watch.Restart();
            var model = new ShellViewModel(); var window = new MainWindow(model);
            var constructionMs = watch.Elapsed.TotalMilliseconds;
            watch.Restart(); Arrange((FrameworkElement)window.Content, 1200, 760);
            samples.Add(new { iteration = i, applicationMs = i == 0 ? applicationMs : 0, constructionMs, layoutMs = watch.Elapsed.TotalMilliseconds });
            window.Close();
        }
        File.WriteAllText(Path.Combine(_root, "shell-timings.json"), JsonSerializer.Serialize(samples, new JsonSerializerOptions { WriteIndented = true }));
    }

    // Opt-in, headless benchmark. Never inspects a user repository. Same fixture for each run.
    private static async Task RunHistoryBenchmarkAsync()
    {
        foreach (System.Collections.DictionaryEntry variable in Environment.GetEnvironmentVariables())
            if (((string)variable.Key).StartsWith("GIT_", StringComparison.OrdinalIgnoreCase)) Environment.SetEnvironmentVariable((string)variable.Key, null);
        var config = Path.Combine(_root, "empty.config");
        await File.WriteAllTextAsync(config, "");
        Environment.SetEnvironmentVariable("GIT_CONFIG_GLOBAL", config);
        Environment.SetEnvironmentVariable("GIT_CONFIG_NOSYSTEM", "1");
        var runner = new TimingRunner();
        var trace = new GitGui.Core.Diagnostics.PerformanceTrace();
        var reader = new RepositoryReader(runner, trace);
        var git = await reader.DiscoverAsync(null, default);
        var repo = Fixture("timing-repository");
        var result = await runner.RunAsync(new(git.Executable, repo, ["init", "--initial-branch=main", "--object-format=sha1"]), default);
        Expect(result.ExitCode == 0, "Initialize timing fixture");
        var tree = WriteFixtureObject(repo, "tree", []);
        string? parent = null;
        for (var i = 0; i < 2500; i++) parent = Commit(repo, tree, parent is null ? [] : [parent], $"Timing commit {i}\n");
        await File.WriteAllTextAsync(Path.Combine(repo, ".git", "refs", "heads", "main"), parent + "\n");
        var store = new SettingsStore(Fixture("timing-profile"));
        await store.LoadAsync(default);
        await store.SaveAsync(new() { RepositoryPaths = [repo], SelectedRepositoryPath = repo }, default);
        var samples = new List<object>();
        for (var i = 0; i < 3; i++)
        {
            runner.Calls.Clear();
            var watch = Stopwatch.StartNew();
            var settings = await new SettingsStore(Path.GetDirectoryName(store.FilePath)!).LoadAsync(default);
            var settingsMs = watch.Elapsed.TotalMilliseconds;
            watch.Restart(); git = await reader.DiscoverAsync(null, default); var discoveryMs = watch.Elapsed.TotalMilliseconds;
            watch.Restart(); var snapshot = await reader.InspectAsync(git, repo, default); var inspectMs = watch.Elapsed.TotalMilliseconds;
            watch.Restart(); var page = await reader.ReadInitialHistoryAsync(git, snapshot, HistoryScope.AllCommits, default);
            var refsMs = trace.Snapshot().Last(sample => sample.Stage == GitGui.Core.Diagnostics.TimingStage.References).Milliseconds; // Post-log validation; inspection now includes initial refs.
            var pageMs = watch.Elapsed.TotalMilliseconds - refsMs;
            watch.Restart(); CommitGraph.Build(page.Entries.Select(entry => entry.Commit).ToArray()); var graphMs = watch.Elapsed.TotalMilliseconds;
            watch.Restart(); var rows = page.Entries.Select(entry => new GitGui.App.ViewModels.CommitRow(entry)).ToArray(); var formattingMs = watch.Elapsed.TotalMilliseconds;
            var firstPageCalls = runner.Calls.ToArray();
            watch.Restart(); await reader.ReadHistoryPageAsync(git, page.Session, new HistoryCursor(2400, GraphCursor.Start), default);
            var deepPageMs = watch.Elapsed.TotalMilliseconds;
            samples.Add(new { iteration = i, settingsMs, discoveryMs, inspectMs, refsMs, pageMs, graphMs, formattingMs, deepPageMs, calls = firstPageCalls });
        }
        var noOps = new List<object>();
        for (var i = 0; i < 3; i++)
        {
            var watch = Stopwatch.StartNew();
            using var process = new Process { StartInfo = new(git.Executable) { UseShellExecute = false, CreateNoWindow = true, RedirectStandardOutput = true, RedirectStandardError = true } };
            process.StartInfo.ArgumentList.Add("--version"); process.Start();
            await Task.WhenAll(process.StandardOutput.ReadToEndAsync(), process.StandardError.ReadToEndAsync());
            await process.WaitForExitAsync(); var directMs = watch.Elapsed.TotalMilliseconds;
            var hosted = await runner.RunAsync(new(git.Executable, repo, ["--version"]), default);
            noOps.Add(new { directMs, hostedMs = hosted.Duration.TotalMilliseconds });
        }
        await File.WriteAllTextAsync(Path.Combine(_root, "launch-timings.json"), JsonSerializer.Serialize(noOps, new JsonSerializerOptions { WriteIndented = true }));
        await File.WriteAllTextAsync(Path.Combine(_root, "history-timings.json"), JsonSerializer.Serialize(samples, new JsonSerializerOptions { WriteIndented = true }));
        Console.WriteLine($"Timings: {_root}");
    }

    private sealed class TimingRunner : IProcessRunner
    {
        private readonly WindowsProcessRunner _inner = new();
        public List<object> Calls { get; } = [];
        public async Task<ProcessResult> RunAsync(ProcessRequest request, CancellationToken token, Action<ProcessOutput>? output = null)
        {
            var result = await _inner.RunAsync(request, token, output);
            var operation = request.Arguments.FirstOrDefault(arg => new[] { "--version", "rev-parse", "symbolic-ref", "for-each-ref", "log" }.Contains(arg)) ?? "fixture";
            lock (Calls) Calls.Add(new { operation, milliseconds = result.Duration.TotalMilliseconds });
            return result;
        }
    }
}
