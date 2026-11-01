using System.Diagnostics;
using System.Globalization;
using System.IO;
using System.Text;
using GitGui.Core.Git;
using GitGui.Core.Processes;
using GitGui.ProcessHost;

namespace GitGui.FoundationChecks;

internal sealed record HistoryPerformanceSpec(string Name, int Commits, bool MergeHeavy, int ExtraRefs)
{
    public int MainCommits => Commits - 3;
    public int Merges => MergeHeavy ? (MainCommits - 1) / 3 : 0;
    public static HistoryPerformanceSpec For(string name) => name switch
    {
        "merge-heavy" => new(name, 20_000, true, 1_200),
        "large" => new(name, 100_000, false, 1_200),
        "million" => new(name, 1_000_000, true, 1_200),
        _ => throw new ArgumentException("Choose merge-heavy, large or million.")
    };

    // A sequence of diamonds: two children of the previous tip, then their merge.
    // Two concurrent mainline lanes, plus scope sentinels; no pathological octopus graph.
    public int[] Parents(int id)
    {
        if (id == 1) return [];
        if (id > MainCommits) return [1]; // remote-only, tag-only, detached HEAD, excluded stash
        if (!MergeHeavy) return [id - 1];
        var completeEnd = 1 + Merges * 3;
        if (id > completeEnd) return [id - 1];
        return ((id - 2) % 3) switch
        {
            0 => [id - 1],
            1 => [id - 2],
            _ => [id - 2, id - 1]
        };
    }
}

/// <summary>Only this freshly created, marked temp tree can be recursively deleted.</summary>
internal sealed class HistoryPerformanceArea : IDisposable
{
    private readonly string _marker = Guid.NewGuid().ToString("N");
    public string Root { get; } = Path.Combine(Path.GetTempPath(), "Shovel-HistoryPerformance-" + Guid.NewGuid().ToString("N"));
    public string Config => Path.Combine(Root, "empty.config");
    public string Template => Path.Combine(Root, "empty-template");
    public bool RetainForUndrainedWork { get; set; }
    public HistoryPerformanceArea()
    {
        Directory.CreateDirectory(Root);
        File.WriteAllText(Path.Combine(Root, ".history-performance-owned"), _marker);
        File.WriteAllText(Config, "");
        Directory.CreateDirectory(Template);
    }
    public string Create(string name)
    {
        if (name.Length == 0 || name.Any(c => !char.IsAsciiLetterOrDigit(c) && c != '-'))
            throw new ArgumentException("Fixture names must be simple local names.");
        var path = Path.Combine(Root, name);
        Directory.CreateDirectory(path);
        return path;
    }
    public void Dispose()
    {
        if (RetainForUndrainedWork) throw new IOException("Retaining fixtures still owned by undrained work.");
        DeleteOwned(Root, _marker);
    }
    internal static void DeleteOwned(string root, string marker)
    {
        var full = Path.TrimEndingDirectorySeparator(Path.GetFullPath(root));
        if (!string.Equals(Path.GetDirectoryName(full), Path.TrimEndingDirectorySeparator(Path.GetFullPath(Path.GetTempPath())), StringComparison.OrdinalIgnoreCase) ||
            !Path.GetFileName(full).StartsWith("Shovel-HistoryPerformance-", StringComparison.Ordinal) ||
            (File.GetAttributes(full) & FileAttributes.ReparsePoint) != 0 ||
            File.ReadAllText(Path.Combine(full, ".history-performance-owned")) != marker)
            throw new IOException("Refusing cleanup of an unowned fixture tree.");
        // Check every entry before deleting anything; never follow a newly introduced junction.
        // External hostile replacement during deletion is outside this disposable-fixture model.
        var pending = new Stack<string>(); pending.Push(full);
        while (pending.TryPop(out var directory))
            foreach (var entry in Directory.EnumerateFileSystemEntries(directory))
            {
                var attributes = File.GetAttributes(entry);
                if ((attributes & FileAttributes.ReparsePoint) != 0) throw new IOException("Refusing cleanup through a linked fixture entry.");
                if ((attributes & FileAttributes.Directory) != 0) pending.Push(entry);
            }
        // Git can mark object files read-only on Windows. Only touch files in the validated owned tree.
        foreach (var file in Directory.EnumerateFiles(full, "*", SearchOption.AllDirectories))
            File.SetAttributes(file, File.GetAttributes(file) & ~FileAttributes.ReadOnly);
        Directory.Delete(full, recursive: true);
    }
}

internal sealed record HistoryPerformanceCall(string Operation, double Milliseconds, string Outcome, int? ExitCode);

/// <summary>Isolate every child, without changing the host process environment or user's config.</summary>
internal sealed class HistoryPerformanceRunner(HistoryPerformanceArea area) : IProcessRunner
{
    private readonly WindowsProcessRunner _inner = new();
    private readonly List<HistoryPerformanceCall> _calls = [];
    private readonly object _sync = new();
    private TaskCompletionSource? _nextLog;
    public int Calls { get { lock (_sync) return _calls.Count; } }
    public HistoryPerformanceCall[] Since(int index) { lock (_sync) return _calls.Skip(index).ToArray(); }
    public Task ArmLog()
    {
        lock (_sync) { _nextLog = new(TaskCreationOptions.RunContinuationsAsynchronously); return _nextLog.Task; }
    }
    public void DisarmLog() { lock (_sync) _nextLog = null; }
    public Dictionary<string, string?> IsolatedEnvironment()
    {
        var environment = new Dictionary<string, string?>(StringComparer.OrdinalIgnoreCase);
        foreach (System.Collections.DictionaryEntry variable in Environment.GetEnvironmentVariables())
            if (((string)variable.Key).StartsWith("GIT_", StringComparison.OrdinalIgnoreCase)) environment[(string)variable.Key] = null;
        environment["GIT_CONFIG_GLOBAL"] = area.Config;
        environment["GIT_CONFIG_SYSTEM"] = area.Config;
        environment["GIT_CONFIG_NOSYSTEM"] = "1";
        environment["GIT_ATTR_NOSYSTEM"] = "1";
        environment["GIT_CEILING_DIRECTORIES"] = area.Root;
        environment["GIT_TERMINAL_PROMPT"] = "0";
        environment["GIT_NO_LAZY_FETCH"] = "1";
        environment["GCM_INTERACTIVE"] = "never";
        return environment;
    }
    public string[] Arguments(string[] arguments) => ["-c", "gc.auto=0", "-c", "maintenance.auto=false",
        "-c", "core.fsmonitor=false", "-c", "core.hooksPath=" + area.Template,
        "-c", "protocol.allow=never", .. arguments];
    public async Task<ProcessResult> RunAsync(ProcessRequest request, CancellationToken token, Action<ProcessOutput>? output = null)
    {
        // Preserve explicit non-Git overrides, then force isolation over caller Git overrides.
        var environment = new Dictionary<string, string?>(request.Environment, StringComparer.OrdinalIgnoreCase);
        var isolated = IsolatedEnvironment();
        foreach (var item in isolated) environment[item.Key] = item.Value;
        foreach (var key in environment.Keys.Where(key => key.StartsWith("GIT_", StringComparison.OrdinalIgnoreCase) &&
                     !isolated.ContainsKey(key)).ToArray()) environment[key] = null;
        if (request.Arguments.Contains("log"))
            lock (_sync) { _nextLog?.TrySetResult(); _nextLog = null; }
        var result = await _inner.RunAsync(request with { Arguments = Arguments(request.Arguments), Environment = environment }, token, output).ConfigureAwait(false);
        var operation = request.Arguments.FirstOrDefault(arg => new[] { "--version", "init", "rev-parse", "for-each-ref", "symbolic-ref", "log", "remote", "status", "rev-list", "config", "diff-tree", "ls-files", "ls-tree", "cat-file", "reflog" }.Contains(arg)) ?? "fixture";
        lock (_sync) _calls.Add(new(operation, result.Duration.TotalMilliseconds, result.Outcome.ToString(), result.ExitCode));
        return result;
    }
    public async Task<string> GitAsync(GitInstallation git, string repo, CancellationToken token, params string[] args)
    {
        var result = await RunAsync(new(git.Executable, repo, args) { Timeout = TimeSpan.FromMinutes(5), MaximumOutputBytes = 1024 * 1024 }, token).ConfigureAwait(false);
        token.ThrowIfCancellationRequested();
        if (result.Outcome != ProcessOutcome.Exited || result.ExitCode != 0)
            throw new IOException($"Fixture Git {args[0]}: {result.Outcome}, exit {result.ExitCode}; {Encoding.UTF8.GetString(result.StandardError).TruncateHistoryPerformance(2048)}");
        return Encoding.UTF8.GetString(result.StandardOutput).TrimEnd('\r', '\n');
    }
}

internal static class HistoryPerformanceFixtures
{
    public static async Task<string> CreateAsync(HistoryPerformanceArea area, HistoryPerformanceRunner runner,
        GitInstallation git, HistoryPerformanceSpec spec, string name, CancellationToken token)
    {
        var repo = area.Create(name);
        await runner.GitAsync(git, repo, token, "init", "--initial-branch=main", "--object-format=sha1", "--template=" + area.Template).ConfigureAwait(false);
        var info = new ProcessStartInfo(git.Executable)
        {
            WorkingDirectory = repo, UseShellExecute = false, CreateNoWindow = true,
            RedirectStandardInput = true, RedirectStandardOutput = true, RedirectStandardError = true,
            StandardInputEncoding = new UTF8Encoding(false)
        };
        foreach (var argument in runner.Arguments(["fast-import", "--quiet", "--done"])) info.ArgumentList.Add(argument);
        foreach (var variable in runner.IsolatedEnvironment())
            if (variable.Value is null) info.Environment.Remove(variable.Key); else info.Environment[variable.Key] = variable.Value;
        using var process = new Process { StartInfo = info };
        token.ThrowIfCancellationRequested();
        process.Start();
        var stdout = DrainAsync(process.StandardOutput);
        var stderr = DrainAsync(process.StandardError);
        // Includes pipe backpressure and the final pack/index flush. No unbounded import wait.
        using var deadline = CancellationTokenSource.CreateLinkedTokenSource(token);
        deadline.CancelAfter(TimeSpan.FromMinutes(30));
        using var kill = deadline.Token.Register(() =>
        {
            try { if (!process.HasExited) process.Kill(entireProcessTree: true); }
            catch (InvalidOperationException) { }
            catch (System.ComponentModel.Win32Exception) { } // Drain below must still complete, or retain the fixture.
        });
        try
        {
            await WriteImportAsync(process.StandardInput, spec, deadline.Token).ConfigureAwait(false);
            process.StandardInput.Close();
            await process.WaitForExitAsync(deadline.Token).ConfigureAwait(false);
            var error = await stderr.ConfigureAwait(false); await stdout.ConfigureAwait(false);
            deadline.Token.ThrowIfCancellationRequested();
            if (process.ExitCode != 0) throw new IOException($"Fixture fast-import exit {process.ExitCode}: {error}");
        }
        finally
        {
            area.RetainForUndrainedWork = true;
            if (!process.HasExited) process.Kill(entireProcessTree: true);
            await process.WaitForExitAsync().WaitAsync(TimeSpan.FromSeconds(10)).ConfigureAwait(false);
            await Task.WhenAll(stdout, stderr).WaitAsync(TimeSpan.FromSeconds(10)).ConfigureAwait(false);
            area.RetainForUndrainedWork = false;
        }
        var head = await runner.GitAsync(git, repo, token, "rev-parse", "refs/heads/detached-import").ConfigureAwait(false);
        await runner.GitAsync(git, repo, token, "update-ref", "--no-deref", "HEAD", head).ConfigureAwait(false);
        await runner.GitAsync(git, repo, token, "update-ref", "-d", "refs/heads/detached-import").ConfigureAwait(false);
        // Same layout for all profiles: packed refs and a reachable commit graph. Fixture only.
        await runner.GitAsync(git, repo, token, "pack-refs", "--all").ConfigureAwait(false);
        await runner.GitAsync(git, repo, token, "commit-graph", "write", "--reachable").ConfigureAwait(false);
        return repo;
    }

    public static async Task WriteImportAsync(TextWriter writer, HistoryPerformanceSpec spec, CancellationToken token)
    {
        if (spec.Commits < 8 || spec.ExtraRefs < 0) throw new ArgumentOutOfRangeException(nameof(spec));
        for (var id = 1; id <= spec.Commits + 1; id++)
        {
            token.ThrowIfCancellationRequested();
            var reference = id <= spec.MainCommits ? "refs/heads/main" : id == spec.MainCommits + 1 ? "refs/remotes/fixture/only" :
                id == spec.MainCommits + 2 ? "refs/tags/only" : id == spec.MainCommits + 3 ? "refs/heads/detached-import" : "refs/stash";
            var message = $"HistoryPerformance {id.ToString(CultureInfo.InvariantCulture)}\n";
            var parents = spec.Parents(id);
            var record = new StringBuilder($"commit {reference}\nmark :{id.ToString(CultureInfo.InvariantCulture)}\ncommitter Fixture <fixture@example.invalid> {(1_700_000_000L + id).ToString(CultureInfo.InvariantCulture)} +0000\ndata {Encoding.UTF8.GetByteCount(message)}\n{message}");
            if (parents.Length != 0) record.Append(CultureInfo.InvariantCulture, $"from :{parents[0]}\n");
            foreach (var parent in parents.Skip(1)) record.Append(CultureInfo.InvariantCulture, $"merge :{parent}\n");
            record.Append('\n');
            await writer.WriteAsync(record.ToString().AsMemory(), token).ConfigureAwait(false);
        }
        for (var index = 1; index <= spec.ExtraRefs; index++)
        {
            token.ThrowIfCancellationRequested();
            var prefix = index % 3 == 0 ? "refs/tags" : index % 3 == 1 ? "refs/heads" : "refs/remotes/fixture";
            var mark = 1 + (long)index * (spec.MainCommits - 1) / Math.Max(1, spec.ExtraRefs);
            await writer.WriteAsync(FormattableString.Invariant($"reset {prefix}/sample-{index}\nfrom :{mark}\n\n").AsMemory(), token).ConfigureAwait(false);
        }
        await writer.WriteAsync("done\n".AsMemory(), token).ConfigureAwait(false);
    }

    private static async Task<string> DrainAsync(StreamReader reader)
    {
        var prefix = new StringBuilder(); var buffer = new char[4096]; int read;
        while ((read = await reader.ReadAsync(buffer).ConfigureAwait(false)) != 0)
            if (prefix.Length < 4096) prefix.Append(buffer, 0, Math.Min(read, 4096 - prefix.Length));
        return prefix.ToString();
    }
    public static string TruncateHistoryPerformance(this string value, int length) => value.Length <= length ? value : value[..length] + " [truncated]";
}
