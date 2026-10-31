using System.Diagnostics;
using System.IO;
using System.Text;
using System.Text.Json;
using System.Security.Cryptography;
using GitGui.Core.Git;
using GitGui.Core.Processes;
using GitGui.ProcessHost;

namespace GitGui.FoundationChecks;
internal static partial class Program
{
    private static async Task MeasureWorkingCopyAsync(bool assets = false, bool discard = false, bool lfs = false, bool profile = false)
    {
        var runner = new WorkingTimingRunner(profile); var reader = new RepositoryReader(runner);
        var actions = new GitActions(runner, reader, new WindowsRepositoryIdentity());
        var git = await reader.DiscoverAsync(null, default); var repo = Fixture("working-copy timing");
        var config = Path.Combine(_root, "isolated.config"); await File.WriteAllTextAsync(config, "");
        Environment.SetEnvironmentVariable("GIT_CONFIG_GLOBAL", config); Environment.SetEnvironmentVariable("GIT_CONFIG_NOSYSTEM", "1");
        async Task<string> Git(params string[] args)
        {
            var result = await runner.RunAsync(new(git.Executable, repo, args), default);
            Expect(result.ExitCode == 0, "Benchmark fixture command: " + Encoding.UTF8.GetString(result.StandardError)); return Encoding.UTF8.GetString(result.StandardOutput).Trim();
        }
        await Git("init", "-b", "main"); await Git("config", "user.name", "Timing"); await Git("config", "user.email", "timing@example.invalid");
        await Git("config", "core.autocrlf", "false");
        if (lfs)
        {
            await Git("config", "filter.lfs.process", "git-lfs filter-process");
            await Git("config", "filter.lfs.clean", "git-lfs clean -- %f");
            await Git("config", "filter.lfs.smudge", "git-lfs smudge -- %f");
            await Git("config", "filter.lfs.required", "true");
            await File.WriteAllTextAsync(Path.Combine(repo, ".gitattributes"), "*.uasset filter=lfs diff=lfs merge=lfs -text\n");
        }
        for (var i = 0; i < 2000; i++) await File.WriteAllTextAsync(Path.Combine(repo, $"file-{i}.txt"), "baseline\n");
        byte[]? assetHash = null;
        if (assets)
        {
            var bytes = new byte[4 * 1024 * 1024]; new Random(17).NextBytes(bytes); bytes[0] = 0;
            assetHash = SHA256.HashData(bytes);
            for (var i = 0; i < 64; i++) await File.WriteAllBytesAsync(Path.Combine(repo, $"asset-{i}.uasset"), bytes);
        }
        await Git("add", "."); await Git("commit", "-m", "Fixture"); var hash = await Git("rev-parse", "HEAD");
        for (var i = 0; i < 1000; i++) await File.WriteAllTextAsync(Path.Combine(repo, ".git", "refs", "heads", $"fixture-{i}"), hash + "\n");
        for (var sample = 0; sample < (discard ? 6 : 3); sample++)
        {
            if (assets) for (var i = 0; i < 64; i++) File.SetLastWriteTimeUtc(Path.Combine(repo, $"asset-{i}.uasset"), DateTime.UtcNow.AddSeconds(sample + 2));
            var count = discard && sample > 2 ? 64 : 1;
            for (var i = 0; i < count; i++)
                if (lfs) { using var changed = File.OpenWrite(Path.Combine(repo, $"asset-{i}.uasset")); changed.Position = 1024; changed.WriteByte((byte)(sample + 100)); }
                else await File.WriteAllTextAsync(Path.Combine(repo, $"file-{i}.txt"), $"changed {sample}\n");
            var context = await actions.ReadWorkingCopyAsync(git, repo, default);
            foreach (var action in discard ? new[] { WorkingCopyAction.Discard } : new[] { WorkingCopyAction.Stage, WorkingCopyAction.Unstage })
            {
                runner.Samples.Clear(); runner.Children.Clear(); var watch = Stopwatch.StartNew();
                var targets = Enumerable.Range(0, count).Select(i => lfs ? $"asset-{i}.uasset" : $"file-{i}.txt").ToHashSet();
                var result = await actions.ExecuteWorkingCopyAsync(context, new(action, context.Changes.Entries.Where(e => targets.Contains(e.Path)).ToArray()), default);
                Expect(result.Result.Success, "Measured operation succeeded"); var execution = watch.Elapsed.TotalMilliseconds;
                context = result.UpdatedContext ?? await actions.ReadWorkingCopyAsync(git, repo, default); var rows = watch.Elapsed.TotalMilliseconds;
                Console.WriteLine($"Remaining selected unstaged rows: {context.Changes.Entries.Count(e => targets.Contains(e.Path) && e.Group == ChangeGroup.Unstaged)}");
                if (discard) Expect(!context.Changes.Entries.Any(e => targets.Contains(e.Path) && e.Group == ChangeGroup.Unstaged), "Discarded rows must leave Local Changes");
                var samples = runner.Samples.ToArray();
                if (!discard) await reader.ReadFileDiffAsync(context, context.Changes.Entries.Single(e => e.Path == "file-0.txt"), new(), default);
                else if (!lfs) foreach (var target in targets) Expect(await File.ReadAllTextAsync(Path.Combine(repo, target)) == "baseline\n", "Discard restored exact bytes");
                else foreach (var target in targets)
                {
                    await using var restored = File.OpenRead(Path.Combine(repo, target));
                    Expect(restored.Length == 4 * 1024 * 1024 && (await SHA256.HashDataAsync(restored)).SequenceEqual(assetHash!),
                        "LFS discard restored exact hydrated bytes, not a pointer stub");
                }
                Console.WriteLine($"Sample {sample + 1} {action} ({count} files): execute {execution:F1} ms; fresh rows {rows:F1} ms; validation {result.ValidationMilliseconds:F1} ms; command {result.CommandMilliseconds:F1} ms; reconciliation {result.RefreshMilliseconds:F1} ms; processes {samples.Length}");
                Console.WriteLine(string.Join("; ", samples.Select(s => $"{s.Name} {s.Milliseconds:F1} ms")));
                foreach (var child in runner.Children) Console.WriteLine($"Discard child {child.Name}: {child.Milliseconds:F1} ms");
            }
        }
        Console.WriteLine("Disposable 2,000-text-file / 1,001-branch fixture; " + (assets ? "plus 64 x 4 MiB binary assets with invalidated stat cache; " : "") +
            (lfs ? "mutated LFS assets, normal smudge/clean and installed fixture hooks; " : "mutated text files; ") +
            "headless, three samples per selection size; byte verification excluded from timings/process counts; " + (profile ? "Trace2 profiling enabled (child lifetimes overlap, profiling adds overhead); " : "") + "not desktop p95. " + _root);
    }
    private sealed class WorkingTimingRunner(bool profile = false) : IProcessRunner
    {
        private readonly WindowsProcessRunner _inner = new();
        public List<(string Name, double Milliseconds)> Samples { get; } = [];
        public List<(string Name, double Milliseconds)> Children { get; } = [];
        public bool EnumeratedHistoryRefs { get; private set; }
        public async Task<ProcessResult> RunAsync(ProcessRequest request, CancellationToken token, Action<ProcessOutput>? output = null)
        {
            if (request.Arguments.Contains("for-each-ref") && !request.Arguments.TakeLast(4).SequenceEqual(new[]
                { "for-each-ref", "--include-root-refs", "--format=%(objectname)%00%(symref)%00%(objecttype)", "HEAD" })) EnumeratedHistoryRefs = true;
            var trace = profile && (request.Arguments.Contains("restore") || request.Arguments.Contains("checkout-index"));
            if (trace) request = request with { Environment = new(request.Environment, StringComparer.OrdinalIgnoreCase) { ["GIT_TRACE2_EVENT"] = "1" } };
            var watch = Stopwatch.StartNew(); var result = await _inner.RunAsync(request, token, output);
            if (trace)
            {
                // Read trace only in memory; persist only allowlisted child category + numeric duration.
                var children = new Dictionary<string, string>(); var ordinary = new StringBuilder();
                foreach (var line in Encoding.UTF8.GetString(result.StandardError).Split('\n'))
                {
                    try
                    {
                        using var json = JsonDocument.Parse(line); var data = json.RootElement;
                        if (!data.TryGetProperty("event", out var ev)) continue;
                        var kind = ev.GetString();
                        if (kind is not ("child_start" or "child_exit")) continue;
                        var key = data.GetProperty("sid").GetString() + "/" + data.GetProperty("child_id").GetInt32();
                        if (kind == "child_start")
                        {
                            var category = data.TryGetProperty("hook_name", out var hook) && hook.GetString() == "post-checkout" ? "post-checkout hook" :
                                data.TryGetProperty("argv", out var argv) && argv.EnumerateArray().Any(arg => arg.GetString() == "git-lfs filter-process") ? "LFS filter-process" : "other configured child";
                            children[key] = category;
                        }
                        else if (children.TryGetValue(key, out var category) && data.TryGetProperty("t_rel", out var seconds))
                            Children.Add((category, seconds.GetDouble() * 1000));
                    }
                    catch (JsonException) { ordinary.AppendLine(line); }
                }
                result = result with { StandardError = Encoding.UTF8.GetBytes(ordinary.ToString()) };
            }
            var commands = new[] { "rev-parse", "symbolic-ref", "for-each-ref", "remote", "config", "log", "status", "diff", "add", "restore", "checkout-index", "reset", "cat-file", "check-attr", "ls-files" };
            lock (Samples) Samples.Add((request.Arguments.FirstOrDefault(commands.Contains) ?? "other", watch.Elapsed.TotalMilliseconds));
            return result;
        }
    }
}
