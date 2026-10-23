using System.Text;
using System.Text.RegularExpressions;
using GitGui.Core.Processes;

namespace GitGui.Core.Git;

public enum GitFailure { MissingGit, UnsupportedGit, StartFailed, CommandFailed, Cancelled, TimedOut, OutputLimit, NotWorktree, InvalidOutput, ChangedDuringRead }
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
    string? Branch, string? Commit, HeadKind HeadKind, RepositoryOperation Operation, DateTimeOffset ReadAt);

/// <summary>Read-only metadata commands. Caller must obtain session trust before InspectAsync.</summary>
public sealed partial class RepositoryReader(IProcessRunner runner)
{
    public static Version MinimumVersion { get; } = new(2, 47, 0);

    public Task<GitInstallation> DiscoverAsync(string? explicitExecutable, CancellationToken token) => Task.Run(async () =>
    {
        var candidates = explicitExecutable is not null ? [explicitExecutable] : new[]
        {
            Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.ProgramFiles), "Git", "cmd", "git.exe"),
            Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "Programs", "Git", "cmd", "git.exe")
        };
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

    public Task<RepositorySnapshot> InspectAsync(GitInstallation git, string approvedPath, CancellationToken token) => Task.Run(async () =>
    {
        token.ThrowIfCancellationRequested();
        if (!Path.IsPathFullyQualified(approvedPath) || !Directory.Exists(approvedPath))
            throw new GitReadException(GitFailure.NotWorktree);
        var inside = await Command("rev-parse", "--is-inside-work-tree");
        if (Line(inside.StandardOutput) != "true") throw new GitReadException(GitFailure.NotWorktree);
        var worktree = Path.GetFullPath(Line((await Command("rev-parse", "--show-toplevel")).StandardOutput));
        var gitDirectory = Path.GetFullPath(Line((await Command("rev-parse", "--absolute-git-dir")).StandardOutput));
        var common = Path.GetFullPath(Line((await Command("rev-parse", "--path-format=absolute", "--git-common-dir")).StandardOutput));
        var before = await HeadAsync();
        var operation = ReadOperation(gitDirectory);
        var after = await HeadAsync();
        if (before != after || operation != ReadOperation(gitDirectory)) throw new GitReadException(GitFailure.ChangedDuringRead);
        if (after.Branch is null && after.Commit is null) throw new GitReadException(GitFailure.InvalidOutput);
        return new RepositorySnapshot(worktree, gitDirectory, common, after.Branch, after.Commit,
            after.Branch is null ? HeadKind.Detached : after.Commit is null ? HeadKind.Unborn : HeadKind.Branch,
            operation, DateTimeOffset.Now);

        async Task<(string? Branch, string? Commit)> HeadAsync()
        {
            var branch = await CommandAllowOne("symbolic-ref", "--quiet", "--short", "HEAD");
            var commit = await CommandAllowOne("rev-parse", "--verify", "--quiet", "HEAD");
            var hash = commit.ExitCode == 0 ? Line(commit.StandardOutput) : null;
            if (hash is not null && (hash.Length == 0 || !hash.All(char.IsAsciiHexDigit))) throw new GitReadException(GitFailure.InvalidOutput);
            return (branch.ExitCode == 0 ? Line(branch.StandardOutput) : null, hash);
        }

        async Task<ProcessResult> Command(params string[] args)
        {
            var result = await RunAsync(git.Executable, approvedPath, ReadArguments(args), token).ConfigureAwait(false);
            RequireSuccess(result);
            return result;
        }
        async Task<ProcessResult> CommandAllowOne(params string[] args)
        {
            var result = await RunAsync(git.Executable, approvedPath, ReadArguments(args), token).ConfigureAwait(false);
            RequireSuccess(result, allowOne: true);
            return result;
        }
    }, token);

    private Task<ProcessResult> RunAsync(string executable, string directory, string[] arguments, CancellationToken token)
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
        return runner.RunAsync(new(executable, directory, arguments) { Environment = environment }, token);
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
