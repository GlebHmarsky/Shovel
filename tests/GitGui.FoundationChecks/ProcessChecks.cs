using System.Diagnostics;
using System.IO;
using System.Text;
using System.Text.Json;
using GitGui.Core.Git;
using GitGui.Core.Processes;
using GitGui.Core.Settings;
using GitGui.ProcessHost;

namespace GitGui.FoundationChecks;

internal static partial class Program
{
    private static string ProbeExecutable => Path.Combine(AppContext.BaseDirectory, "GitGui.FoundationChecks.exe");

    private static async Task<int> RunProbeAsync(string[] args)
    {
        switch (args[0])
        {
            case "arguments":
                Console.Write(JsonSerializer.Serialize(args.Skip(1).ToArray())); return 0;
            case "flood":
                await Task.WhenAll(Console.Out.WriteAsync(new string('o', 128 * 1024)), Console.Error.WriteAsync(new string('e', 128 * 1024)));
                return 0;
            case "stdin":
                return Console.Read() == -1 ? 0 : 2;
            case "wait":
                await Task.Delay(TimeSpan.FromSeconds(60)); return 0;
            case "child":
            case "child-exit":
                using (var child = Process.Start(new ProcessStartInfo(ProbeExecutable)
                {
                    UseShellExecute = false, CreateNoWindow = true, RedirectStandardOutput = true, RedirectStandardError = true,
                    ArgumentList = { "--probe", "wait" }
                })!)
                {
                    Console.WriteLine(child.Id); await Console.Out.FlushAsync();
                    if (args[0] == "child") await Task.Delay(TimeSpan.FromSeconds(60));
                    return 0;
                }
            case "failure": return 17;
            default: return 99;
        }
    }

    private static async Task RunProcessChecksAsync()
    {
        var runner = new WindowsProcessRunner();
        var directory = Fixture("processes — spaces 日本語");
        ProcessRequest Probe(params string[] args) => new(ProbeExecutable, directory, ["--probe", .. args]);

        await Check("Structured arguments preserve spaces, Unicode, quotes and shell metacharacters", async () =>
        {
            string[] arguments = ["a b", "שלום 日本語", "\"quoted\"", "trailing\\", "& $(echo NEVER) | > file", ""];
            var result = await runner.RunAsync(Probe("arguments", arguments[0], arguments[1], arguments[2], arguments[3], arguments[4], arguments[5]), default);
            Expect(result.Outcome == ProcessOutcome.Exited && result.ExitCode == 0, "Probe must run");
            Expect(JsonSerializer.Deserialize<string[]>(result.StandardOutput)!.SequenceEqual(arguments), "Argument boundaries must survive without a shell");
        });
        await Check("Both output streams drain concurrently and stream bytes", async () =>
        {
            long streamed = 0;
            var result = await runner.RunAsync(Probe("flood"), default, chunk => Interlocked.Add(ref streamed, chunk.Bytes.Length));
            Expect(result.Outcome == ProcessOutcome.Exited && result.ExitCode == 0 && result.StandardOutput.Length == 128 * 1024 &&
                result.StandardError.Length == 128 * 1024 && streamed == 256 * 1024, "No pipe deadlock or lost bytes");
        });
        await Check("Output overflow stops the job with bounded retention", async () =>
        {
            var result = await runner.RunAsync(Probe("flood") with { MaximumOutputBytes = 4096 }, default);
            Expect(result.Outcome == ProcessOutcome.OutputLimit && result.StandardOutput.Length <= 4096 && result.StandardError.Length <= 4096, "Output cap must be enforced");
        });
        await Check("Input is closed, child exit codes preserved and start failures reported", async () =>
        {
            Expect((await runner.RunAsync(Probe("stdin"), default)).ExitCode == 0, "No invisible stdin wait");
            Expect((await runner.RunAsync(Probe("failure"), default)).ExitCode == 17, "Preserve exit status");
            Expect((await runner.RunAsync(new(Path.Combine(directory, "missing.exe"), directory, []), default)).Outcome == ProcessOutcome.StartFailed, "Missing program must fail safely");
        });
        await Check("Timeout and cancellation terminate owned descendants", async () =>
        {
            var timed = await runner.RunAsync(Probe("child") with { Timeout = TimeSpan.FromSeconds(2) }, default);
            Expect(timed.Outcome == ProcessOutcome.TimedOut, "Timeout must be distinct");
            ExpectChildStopped(timed.StandardOutput);
            using var cancel = new CancellationTokenSource(TimeSpan.FromSeconds(2));
            var cancelled = await runner.RunAsync(Probe("child"), cancel.Token);
            Expect(cancelled.Outcome == ProcessOutcome.Cancelled, "Cancellation must be distinct");
            ExpectChildStopped(cancelled.StandardOutput);
        });
        await Check("Parent exit with inherited pipe handles still has bounded descendant cleanup", async () =>
        {
            var result = await runner.RunAsync(Probe("child-exit") with { Timeout = TimeSpan.FromSeconds(2) }, default);
            Expect(result.Outcome is ProcessOutcome.Exited or ProcessOutcome.TimedOut, "Inherited pipe handles must not cause an indefinite wait");
            ExpectChildStopped(result.StandardOutput);
        });
        await Check("Queued cancellation and failed output consumer release concurrency slots", async () =>
        {
            using var cancel = new CancellationTokenSource(TimeSpan.FromSeconds(2));
            var first = runner.RunAsync(Probe("wait"), cancel.Token);
            var second = runner.RunAsync(Probe("wait"), cancel.Token);
            var third = runner.RunAsync(Probe("wait"), cancel.Token);
            var results = await Task.WhenAll(first, second, third);
            Expect(results.All(result => result.Outcome == ProcessOutcome.Cancelled), "Queued work must cancel too");
            var failed = await runner.RunAsync(Probe("flood"), default, _ => throw new InvalidOperationException("fixture"));
            Expect(failed.Outcome == ProcessOutcome.StartFailed, "Consumer failure must not leak a job/slot");
            Expect((await runner.RunAsync(Probe("stdin"), default)).ExitCode == 0, "Runner remains usable");
        });
        await Check("Schema 1 migrates with backup; schema 2 preserves ordered tabs", async () =>
        {
            var store = new SettingsStore(Fixture("migration"));
            const string old = "{\"SchemaVersion\":1,\"WindowWidth\":1250}";
            await File.WriteAllTextAsync(store.FilePath, old);
            var result = await store.LoadAsync(default);
            Expect(result.Status == SettingsLoadStatus.Migrated && result.Settings.WindowWidth == 1250, "Migrate the known old schema only");
            await store.SaveAsync(result.Settings with { RepositoryPaths = [directory, _root], SelectedRepositoryPath = _root }, default);
            Expect(await File.ReadAllTextAsync(store.FilePath + ".bak") == old, "Retain old-schema recovery copy");
            var current = await new SettingsStore(Path.GetDirectoryName(store.FilePath)!).LoadAsync(default);
            Expect(current.Settings.RepositoryPaths.SequenceEqual(new[] { directory, _root }) && current.Settings.SelectedRepositoryPath == _root, "Tab order/selection must survive");
        });

        // All Git mutations below are confined to newly created fixtures with isolated configuration.
        var emptyConfig = Path.Combine(Fixture("git-config"), "empty.config");
        await File.WriteAllTextAsync(emptyConfig, "");
        var priorGlobal = Environment.GetEnvironmentVariable("GIT_CONFIG_GLOBAL");
        var priorSystem = Environment.GetEnvironmentVariable("GIT_CONFIG_NOSYSTEM");
        Environment.SetEnvironmentVariable("GIT_CONFIG_GLOBAL", emptyConfig);
        Environment.SetEnvironmentVariable("GIT_CONFIG_NOSYSTEM", "1");
        try
        {
            var reader = new RepositoryReader(runner);
            GitInstallation? git = null;
            await Check("Discover installed Git without PATH/current-directory execution", async () =>
            {
                git = await reader.DiscoverAsync(null, default);
                Expect(Path.IsPathFullyQualified(git.Executable) && Version.Parse(git.Version) >= RepositoryReader.MinimumVersion, "Trusted installation and version floor required");
                await ExpectGitFailure(GitFailure.MissingGit, () => reader.DiscoverAsync(Path.Combine(directory, "git.exe"), default));
                await ExpectGitFailure(GitFailure.UnsupportedGit, () => reader.DiscoverAsync(ProbeExecutable, default));
            });
            if (git is null) return;
            async Task<ProcessResult> Git(string folder, params string[] args)
            {
                var environment = new Dictionary<string, string?>();
                foreach (System.Collections.DictionaryEntry variable in Environment.GetEnvironmentVariables())
                    if (((string)variable.Key).StartsWith("GIT_", StringComparison.OrdinalIgnoreCase)) environment[(string)variable.Key] = null;
                environment["GIT_CONFIG_GLOBAL"] = emptyConfig;
                environment["GIT_CONFIG_NOSYSTEM"] = "1";
                environment["GIT_TERMINAL_PROMPT"] = "0";
                // If fixture initialization fails, never discover/mutate the enclosing working repository.
                environment["GIT_CEILING_DIRECTORIES"] = _root;
                var result = await runner.RunAsync(new(git.Executable, folder, args)
                {
                    Environment = environment
                }, default);
                Expect(result.Outcome == ProcessOutcome.Exited, "Fixture command must complete");
                return result;
            }
            var repo = Fixture("repository — spaces 日本語");
            await Check("Open unborn repository and reject non-repositories/bare repositories", async () =>
            {
                Expect((await Git(repo, "init", "--initial-branch=main")).ExitCode == 0, "Initialize disposable fixture");
                var snapshot = await reader.InspectAsync(git, repo, default);
                Expect(snapshot.HeadKind == HeadKind.Unborn && snapshot.Branch == "main" && snapshot.Commit is null, "Unborn is not an error");
                // A folder under this project would discover the project's own parent repository.
                var nonRepository = Path.Combine(Path.GetTempPath(), "GitGui-empty-" + Guid.NewGuid().ToString("N"));
                Directory.CreateDirectory(nonRepository);
                try { await ExpectGitFailure(GitFailure.CommandFailed, () => reader.InspectAsync(git, nonRepository, default)); }
                finally { Directory.Delete(nonRepository); }
                var bare = Fixture("bare");
                await Git(bare, "init", "--bare");
                await ExpectGitFailure(GitFailure.NotWorktree, () => reader.InspectAsync(git, bare, default));
            });
            if (!Directory.Exists(Path.Combine(repo, ".git"))) return;
            await Git(repo, "config", "user.name", "Fixture");
            await Git(repo, "config", "user.email", "fixture@example.invalid");
            await File.WriteAllTextAsync(Path.Combine(repo, "file.txt"), "base\n");
            await Git(repo, "add", "--", "file.txt");
            await Git(repo, "commit", "-m", "Base fixture");
            await Check("Attached/detached HEAD and linked-worktree shared identity", async () =>
            {
                var attached = await reader.InspectAsync(git, repo, default);
                Expect(attached.HeadKind == HeadKind.Branch && attached.Commit is not null, "Committed branch detected");
                await Git(repo, "checkout", "--detach");
                Expect((await reader.InspectAsync(git, repo, default)).HeadKind == HeadKind.Detached, "Detached HEAD detected");
                await Git(repo, "checkout", "main");
                var linked = Path.Combine(_root, "linked worktree");
                Expect((await Git(repo, "worktree", "add", "-b", "linked", linked)).ExitCode == 0, "Create disposable linked worktree");
                var worktree = await reader.InspectAsync(git, linked, default);
                Expect(string.Equals(attached.CommonDirectory, worktree.CommonDirectory, StringComparison.OrdinalIgnoreCase) &&
                    !string.Equals(attached.GitDirectory, worktree.GitDirectory, StringComparison.OrdinalIgnoreCase), "Distinguish common and per-worktree state");
            });
            await Check("Conflicted merge is detected across repeated inspections without mutation", async () =>
            {
                await Git(repo, "checkout", "-b", "side");
                await File.WriteAllTextAsync(Path.Combine(repo, "file.txt"), "side\n");
                await Git(repo, "commit", "-am", "Side fixture");
                await Git(repo, "checkout", "main");
                await File.WriteAllTextAsync(Path.Combine(repo, "file.txt"), "main\n");
                await Git(repo, "commit", "-am", "Main fixture");
                Expect((await Git(repo, "merge", "--no-edit", "side")).ExitCode != 0, "Fixture must conflict");
                var before = await File.ReadAllBytesAsync(Path.Combine(repo, "file.txt"));
                var first = await reader.InspectAsync(git, repo, default);
                var reopened = await new RepositoryReader(runner).InspectAsync(git, repo, default);
                var after = await File.ReadAllBytesAsync(Path.Combine(repo, "file.txt"));
                Expect(first.Operation.HasFlag(RepositoryOperation.Merge) && reopened.Operation == first.Operation && before.SequenceEqual(after), "Inspection preserves the conflict");
            });
            await Check("Read inspection suppresses configured fsmonitor and ignores inherited repository override", async () =>
            {
                var marker = Path.Combine(_root, "helper-must-not-run");
                var hook = Path.Combine(repo, "monitor.sh");
                await File.WriteAllTextAsync(hook, "#!/bin/sh\nprintf unsafe > '" + marker.Replace('\\', '/') + "'\n");
                await Git(repo, "config", "core.fsmonitor", hook.Replace('\\', '/'));
                var prior = Environment.GetEnvironmentVariable("GIT_DIR");
                Environment.SetEnvironmentVariable("GIT_DIR", Path.Combine(directory, "not-a-repository"));
                try { Expect((await reader.InspectAsync(git, repo, default)).Branch == "main", "Do not inherit a redirected repository"); }
                finally { Environment.SetEnvironmentVariable("GIT_DIR", prior); }
                Expect(!File.Exists(marker), "Inspection must not execute the configured helper");
            });
            await Check("Concurrent external branch changes reject incoherent snapshots or return a fresh state", async () =>
            {
                var inspection = reader.InspectAsync(git, repo, default);
                await Git(repo, "branch", "-m", "renamed");
                try { var result = await inspection; Expect(result.Branch is "main" or "renamed", "Only an observed valid branch is allowed"); }
                catch (GitReadException exception) { Expect(exception.Failure == GitFailure.ChangedDuringRead, "Explicit race state expected"); }
                Expect((await reader.InspectAsync(git, repo, default)).Branch == "renamed", "Manual refresh sees external changes");
            });
        }
        finally
        {
            Environment.SetEnvironmentVariable("GIT_CONFIG_GLOBAL", priorGlobal);
            Environment.SetEnvironmentVariable("GIT_CONFIG_NOSYSTEM", priorSystem);
        }
    }

    private static void ExpectChildStopped(byte[] output)
    {
        Expect(int.TryParse(Encoding.UTF8.GetString(output).Trim(), out var id), "Probe must report its child before cancellation");
        try { using var child = Process.GetProcessById(id); Expect(child.HasExited || child.WaitForExit(2000), "Owned child must be gone"); }
        catch (ArgumentException) { }
    }

    private static async Task ExpectGitFailure<T>(GitFailure failure, Func<Task<T>> action)
    {
        try { await action(); }
        catch (GitReadException exception) { Expect(exception.Failure == failure, "Expected Git failure category"); return; }
        throw new InvalidOperationException("Expected Git read failure");
    }
}
