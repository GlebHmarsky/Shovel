using System.Diagnostics;
using System.IO;
using System.Text;
using GitGui.Core.Git;
using GitGui.Core.Processes;
using GitGui.ProcessHost;

namespace GitGui.FoundationChecks;
internal static partial class Program
{
    private static async Task MeasureWorkingCopyAsync()
    {
        var runner = new WorkingTimingRunner(); var reader = new RepositoryReader(runner);
        var actions = new GitActions(runner, reader, new WindowsRepositoryIdentity());
        var git = await reader.DiscoverAsync(null, default); var repo = Fixture("working-copy timing");
        var config = Path.Combine(_root, "isolated.config"); await File.WriteAllTextAsync(config, "");
        Environment.SetEnvironmentVariable("GIT_CONFIG_GLOBAL", config); Environment.SetEnvironmentVariable("GIT_CONFIG_NOSYSTEM", "1");
        async Task<string> Git(params string[] args)
        {
            var result = await runner.RunAsync(new(git.Executable, repo, args), default);
            Expect(result.ExitCode == 0, "Benchmark fixture command"); return Encoding.UTF8.GetString(result.StandardOutput).Trim();
        }
        await Git("init", "-b", "main"); await Git("config", "user.name", "Timing"); await Git("config", "user.email", "timing@example.invalid");
        await Git("config", "core.autocrlf", "false");
        for (var i = 0; i < 2000; i++) await File.WriteAllTextAsync(Path.Combine(repo, $"file-{i}.txt"), "baseline\n");
        await Git("add", "."); await Git("commit", "-m", "Fixture"); var hash = await Git("rev-parse", "HEAD");
        for (var i = 0; i < 1000; i++) await File.WriteAllTextAsync(Path.Combine(repo, ".git", "refs", "heads", $"fixture-{i}"), hash + "\n");
        for (var sample = 0; sample < 3; sample++)
        {
            await File.WriteAllTextAsync(Path.Combine(repo, "file-0.txt"), $"changed {sample}\n");
            var context = await actions.ReadWorkingCopyAsync(git, repo, default);
            foreach (var action in new[] { WorkingCopyAction.Stage, WorkingCopyAction.Unstage })
            {
                runner.Samples.Clear(); var watch = Stopwatch.StartNew();
                var result = await actions.ExecuteWorkingCopyAsync(context, new(action, context.Changes.Entries.Where(e => e.Path == "file-0.txt").ToArray()), default);
                Expect(result.Result.Success, "Measured operation succeeded"); var execution = watch.Elapsed.TotalMilliseconds;
                context = await actions.ReadWorkingCopyAsync(git, repo, default); var rows = watch.Elapsed.TotalMilliseconds;
                await reader.ReadFileDiffAsync(context, context.Changes.Entries.Single(e => e.Path == "file-0.txt"), new(), default);
                Console.WriteLine($"Sample {sample + 1} {action}: execute {execution:F1} ms; fresh rows {rows:F1} ms; diff {watch.Elapsed.TotalMilliseconds:F1} ms; processes {runner.Samples.Count}");
                Console.WriteLine(string.Join("; ", runner.Samples.Select(s => $"{s.Name} {s.Milliseconds:F1} ms")));
            }
        }
        Console.WriteLine("Disposable 2,000-file / 1,001-branch fixture; headless, first plus two warm samples, not desktop p95. " + _root);
    }
    private sealed class WorkingTimingRunner : IProcessRunner
    {
        private readonly WindowsProcessRunner _inner = new();
        public List<(string Name, double Milliseconds)> Samples { get; } = [];
        public async Task<ProcessResult> RunAsync(ProcessRequest request, CancellationToken token, Action<ProcessOutput>? output = null)
        {
            var watch = Stopwatch.StartNew(); var result = await _inner.RunAsync(request, token, output);
            var commands = new[] { "rev-parse", "symbolic-ref", "for-each-ref", "remote", "config", "log", "status", "diff", "add", "restore", "cat-file", "check-attr" };
            lock (Samples) Samples.Add((request.Arguments.FirstOrDefault(commands.Contains) ?? "other", watch.Elapsed.TotalMilliseconds));
            return result;
        }
    }
}
