using System.Text;
using GitGui.Core.Diagnostics;
using System.Text.RegularExpressions;
using GitGui.Core.Processes;

namespace GitGui.Core.Git;

public enum GitFailure { MissingGit, UnsupportedGit, StartFailed, CommandFailed, Cancelled, TimedOut, OutputLimit, NotWorktree, InvalidOutput, ChangedDuringRead, HistoryTooWide }
public sealed class GitReadException(GitFailure failure, string rawOutput = "") : Exception(failure.ToString())
{
    public GitFailure Failure { get; } = failure;
    // Display only. Never include this in persistent diagnostics or exception messages.
    public string RawOutput { get; } = rawOutput;
}
public sealed record GitInstallation(string Executable, string Version);
public enum HeadKind { Branch, Unborn, Detached }
[Flags]
public enum RepositoryOperation { None = 0, Merge = 1, Rebase = 2, ApplyMailbox = 4, CherryPick = 8, Revert = 16, Sequencer = 32 }
public sealed record RepositorySnapshot(string Worktree, string GitDirectory, string CommonDirectory,
    string? Branch, string? Commit, HeadKind HeadKind, RepositoryOperation Operation, DateTimeOffset ReadAt)
{
    internal HistoryState? InitialHistoryState { get; init; }
    public string? UpstreamRef { get; init; }
    public IReadOnlyList<string> RemoteNames { get; init; } = [];
}

/// <summary>Read-only metadata commands. Caller validates repository identity before automatic reads; no helper/script permission is implied.</summary>
public sealed partial class RepositoryReader(IProcessRunner runner, PerformanceTrace? trace = null)
{
    public static Version MinimumVersion { get; } = new(2, 47, 0);

    public Task<GitInstallation> DiscoverAsync(string? explicitExecutable, CancellationToken token) => Task.Run(async () =>
    {
        using var timing = trace?.Measure(TimingStage.Discovery);
        // Git for Windows supports direct absolute-path invocation of its native binary.
        // Newer installs use ucrt64; keep the stable cmd entry point as a discovery fallback.
        // An explicitly selected executable is always honored exactly.
        var roots = new[] { Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.ProgramFiles), "Git"),
            Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "Programs", "Git") };
        var candidates = explicitExecutable is not null ? [explicitExecutable] : roots.SelectMany(root =>
            new[] { "ucrt64/bin/git.exe", "mingw64/bin/git.exe", "mingw32/bin/git.exe", "cmd/git.exe" }.Select(relative => Path.Combine(root, relative))).ToArray();
        var executable = candidates.FirstOrDefault(path => Path.IsPathFullyQualified(path) &&
            path.EndsWith(".exe", StringComparison.OrdinalIgnoreCase) && File.Exists(path));
        if (executable is null) throw new GitReadException(GitFailure.MissingGit);
        var result = await RunAsync(executable, AppContext.BaseDirectory, ["--version"], token).ConfigureAwait(false);
        RequireSuccess(result);
        var match = GitVersion().Match(Text(result.StandardOutput));
        if (!match.Success || !Version.TryParse(match.Groups[1].Value, out var version) || version < MinimumVersion)
            throw new GitReadException(GitFailure.UnsupportedGit, Text(result.StandardOutput));
        return new GitInstallation(executable, match.Groups[1].Value);
    }, token);

    public Task<RepositorySnapshot> InspectAsync(GitInstallation git, string repositoryPath, CancellationToken token) => Task.Run(async () =>
    {
        using var timing = trace?.Measure(TimingStage.Inspection);
        token.ThrowIfCancellationRequested();
        if (!Path.IsPathFullyQualified(repositoryPath) || !Directory.Exists(repositoryPath))
            throw new GitReadException(GitFailure.NotWorktree);
        var locations = await Command("rev-parse", "--path-format=absolute", "--is-inside-work-tree", "--show-toplevel", "--absolute-git-dir", "--git-common-dir");
        var lines = Text(locations.StandardOutput).TrimEnd('\r', '\n').Split('\n').Select(value => value.TrimEnd('\r')).ToArray();
        if (lines.Length > 0 && lines[0] == "false") throw new GitReadException(GitFailure.NotWorktree);
        if (lines.Length != 4 || lines[0] != "true" || lines.Skip(1).Any(value => !Path.IsPathFullyQualified(value) || value.Contains('\0')))
            throw new GitReadException(GitFailure.InvalidOutput);
        var worktree = Path.GetFullPath(lines[1]);
        var gitDirectory = Path.GetFullPath(lines[2]);
        var common = Path.GetFullPath(lines[3]);
        var operation = ReadOperation(gitDirectory);
        var location = new RepositorySnapshot(worktree, gitDirectory, common, null, null, HeadKind.Unborn, operation, DateTimeOffset.Now);
        // Local names only: no URL resolution, helpers or network. Run alongside the ref read.
        var remotesTask = Command("remote");
        var stateTask = ReadHistoryStateAsync(git, location, token);
        await Task.WhenAll(remotesTask, stateTask).ConfigureAwait(false);
        var state = await stateTask;
        var remotes = Text((await remotesTask).StandardOutput).Split(['\r', '\n'], StringSplitOptions.RemoveEmptyEntries);
        if (operation != ReadOperation(gitDirectory)) throw new GitReadException(GitFailure.ChangedDuringRead);
        return location with { Branch = state.Branch, Commit = state.Commit,
            HeadKind = state.Branch is null ? HeadKind.Detached : state.Commit is null ? HeadKind.Unborn : HeadKind.Branch,
            InitialHistoryState = state, UpstreamRef = state.UpstreamRef, RemoteNames = remotes };

        async Task<ProcessResult> Command(params string[] args)
        {
            var result = await RunAsync(git.Executable, repositoryPath, ReadArguments(args), token).ConfigureAwait(false);
            if (args.Contains("--is-inside-work-tree") && Text(result.StandardOutput).StartsWith("false", StringComparison.Ordinal))
                throw new GitReadException(GitFailure.NotWorktree);
            RequireSuccess(result);
            return result;
        }
    }, token);

    private Task<ProcessResult> RunAsync(string executable, string directory, string[] arguments, CancellationToken token, int maximumOutputBytes = 256 * 1024)
    {
        var environment = new Dictionary<string, string?>(StringComparer.OrdinalIgnoreCase);
        // Inherited Git overrides/trace destinations must not redirect inspection or leak output.
        foreach (System.Collections.DictionaryEntry variable in Environment.GetEnvironmentVariables())
        {
            var key = (string)variable.Key;
            if (key.StartsWith("GIT_", StringComparison.OrdinalIgnoreCase) &&
                !new[] { "GIT_CONFIG_GLOBAL", "GIT_CONFIG_SYSTEM", "GIT_CONFIG_NOSYSTEM" }.Contains(key, StringComparer.OrdinalIgnoreCase)) environment[key] = null;
        }
        environment["GIT_TERMINAL_PROMPT"] = "0";
        environment["GCM_INTERACTIVE"] = "never";
        environment["GIT_OPTIONAL_LOCKS"] = "0";
        environment["GIT_NO_LAZY_FETCH"] = "1";
        return runner.RunAsync(new(executable, directory, arguments) { Environment = environment, MaximumOutputBytes = maximumOutputBytes }, token);
    }

    private static string[] ReadArguments(string[] args) => ["--no-pager", "--no-optional-locks", "--no-lazy-fetch",
        "-c", "core.fsmonitor=false", "-c", "gc.auto=0", "-c", "maintenance.auto=false", .. args];

    private static RepositoryOperation ReadOperation(string directory)
    {
        var state = RepositoryOperation.None;
        if (File.Exists(Path.Combine(directory, "MERGE_HEAD"))) state |= RepositoryOperation.Merge;
        if (Directory.Exists(Path.Combine(directory, "rebase-merge"))) state |= RepositoryOperation.Rebase;
        if (Directory.Exists(Path.Combine(directory, "rebase-apply")))
            state |= File.Exists(Path.Combine(directory, "rebase-apply", "applying")) ? RepositoryOperation.ApplyMailbox : RepositoryOperation.Rebase;
        if (File.Exists(Path.Combine(directory, "CHERRY_PICK_HEAD"))) state |= RepositoryOperation.CherryPick;
        if (File.Exists(Path.Combine(directory, "REVERT_HEAD"))) state |= RepositoryOperation.Revert;
        if (Directory.Exists(Path.Combine(directory, "sequencer"))) state |= RepositoryOperation.Sequencer;
        return state;
    }

    private static void RequireSuccess(ProcessResult result, bool allowOne = false)
    {
        var failure = result.Outcome switch
        {
            ProcessOutcome.Cancelled => GitFailure.Cancelled,
            ProcessOutcome.TimedOut => GitFailure.TimedOut,
            ProcessOutcome.OutputLimit => GitFailure.OutputLimit,
            ProcessOutcome.StartFailed => GitFailure.StartFailed,
            _ => GitFailure.CommandFailed
        };
        if (result.Outcome != ProcessOutcome.Exited || (result.ExitCode != 0 && !(allowOne && result.ExitCode == 1)))
            throw new GitReadException(failure, Text(result.StandardError));
    }

    private static string Text(byte[] bytes) => Encoding.UTF8.GetString(bytes);
    private static string Line(byte[] bytes)
    {
        var value = Text(bytes).TrimEnd('\r', '\n');
        if (value.Length == 0 || value.Contains('\n') || value.Contains('\0')) throw new GitReadException(GitFailure.InvalidOutput);
        return value;
    }
    [GeneratedRegex(@"^git version (\d+\.\d+\.\d+)(?:[.\s]|$)", RegexOptions.CultureInvariant)]
    private static partial Regex GitVersion();
}
