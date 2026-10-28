using System.Security.Cryptography;
using System.Text;
using GitGui.Core.Processes;

namespace GitGui.Core.Git;

public sealed record RepositoryInitOptions(string Directory, string InitialBranch = "");
public sealed record RepositoryInitResult(ActionResult Action, RepositorySnapshot? Repository);

public sealed partial class GitActions
{
    public Task<RepositoryInitResult> InitializeRepositoryAsync(GitInstallation git, RepositoryInitOptions options,
        CancellationToken token, Action<ProcessOutput>? progress = null) => Task.Run(() =>
    {
        token.ThrowIfCancellationRequested();
        string target;
        try
        {
            if (!Path.IsPathFullyQualified(options.Directory)) throw new ArgumentException();
            target = Path.TrimEndingDirectorySeparator(Path.GetFullPath(options.Directory));
        }
        catch (Exception error) when (error is ArgumentException or NotSupportedException or PathTooLongException)
        { throw new GitActionException("InitInvalidDirectory"); }
        // No repository identity exists yet. Serialize this canonical destination across Shovel profiles.
        var key = Convert.ToHexString(SHA256.HashData(Encoding.UTF8.GetBytes(target.ToUpperInvariant())));
        using var gate = new Mutex(false, @"Local\Shovel.Init." + key);
        bool owned; try { owned = gate.WaitOne(0); } catch (AbandonedMutexException) { owned = true; }
        if (!owned) throw new GitActionException("ActionOtherInstance");
        try { return Execute().GetAwaiter().GetResult(); }
        finally { gate.ReleaseMutex(); }

        async Task<RepositoryInitResult> Execute()
        {
            var parent = ValidateDestination(target);
            var probe = await Raw(git, parent, ["rev-parse", "--is-bare-repository"], token);
            if (probe.Outcome != ProcessOutcome.Exited) throw ReadFailure(probe);
            if (probe.ExitCode == 0)
            {
                if (Decode(probe).Trim() != "false") throw new GitActionException("InitBareRepository");
                var existing = await reader.InspectAsync(git, parent, token, includeHistory: false);
                if (!SameDirectory(existing.Worktree, target)) throw new GitActionException("InitNestedRepository");
                return new(new(ProcessOutcome.Exited, 0, "") { MessageCode = "InitAlreadyExists" }, existing);
            }
            // Exit 128 also covers ownership or malformed metadata. Never infer 'not a repo'
            // from localized stderr, or turn a failed ownership check into reinitialization.
            if (probe.ExitCode != 128 || HasMetadataAncestor(parent)) throw ReadFailure(probe);
            if (!string.IsNullOrWhiteSpace(options.InitialBranch))
            {
                if (options.InitialBranch.StartsWith('-') || options.InitialBranch.Any(char.IsControl)) throw new GitActionException("InitInvalidBranch");
                var branch = await Raw(git, parent, ["check-ref-format", "--branch", options.InitialBranch], token);
                if (branch.Outcome != ProcessOutcome.Exited) throw ReadFailure(branch);
                if (branch.ExitCode != 0) throw new GitActionException("InitInvalidBranch");
            }
            token.ThrowIfCancellationRequested();
            // Recheck immediately before Git creates directories. No worktree scan or cleanup.
            if (!SameDirectory(parent, ValidateDestination(target)) || HasMetadataAncestor(parent)) throw new GitActionException("InitDestinationChanged");
            var environment = EnvironmentForGit(); environment["GIT_OPTIONAL_LOCKS"] = null;
            var result = await runner.RunAsync(new(git.Executable, parent,
                ["--no-pager", "--no-lazy-fetch", "-c", "core.fsmonitor=false", "-c", "gc.auto=0", "-c", "maintenance.auto=false",
                    "init", .. string.IsNullOrWhiteSpace(options.InitialBranch) ? Array.Empty<string>() : ["--initial-branch=" + options.InitialBranch], "--", target])
                { Environment = environment, Timeout = TimeSpan.FromMinutes(2), MaximumOutputBytes = 1024 * 1024 }, token, progress);
            var action = new ActionResult(result.Outcome, result.ExitCode, Redact(Decode(result) + Encoding.UTF8.GetString(result.StandardError)))
                { MessageCode = result.Outcome == ProcessOutcome.Exited && result.ExitCode == 0 ? "InitSucceeded" : "InitIncomplete" };
            RepositorySnapshot? observed = null;
            using var inspect = new CancellationTokenSource(TimeSpan.FromSeconds(20));
            try
            {
                ValidateDestination(target);
                var snapshot = await reader.InspectAsync(git, target, inspect.Token, includeHistory: false);
                if (SameDirectory(snapshot.Worktree, target)) observed = snapshot;
                else action = action with { MessageCode = "InitUnexpectedLayout" };
            }
            catch (Exception error) when (error is GitReadException or GitActionException or IOException or UnauthorizedAccessException or OperationCanceledException)
            { if (action.Success) action = action with { MessageCode = "InitCreatedUnverified" }; }
            return new(action, observed);
        }
    }, token);

    private static bool SameDirectory(string first, string second) => string.Equals(Path.TrimEndingDirectorySeparator(Path.GetFullPath(first)),
        Path.TrimEndingDirectorySeparator(Path.GetFullPath(second)), StringComparison.OrdinalIgnoreCase);
    private static string ValidateDestination(string target)
    {
        string? existing = null;
        for (var part = target; part is not null; part = Path.GetDirectoryName(part))
        {
            if (Path.GetFileName(part).Equals(".git", StringComparison.OrdinalIgnoreCase)) throw new GitActionException("InitMetadataDirectory");
            FileAttributes attributes;
            try { attributes = File.GetAttributes(part); }
            catch (FileNotFoundException) { continue; }
            catch (DirectoryNotFoundException) { continue; }
            if (attributes.HasFlag(FileAttributes.ReparsePoint)) throw new GitActionException("InitLinkedDirectory");
            if (!attributes.HasFlag(FileAttributes.Directory)) throw new GitActionException("InitInvalidDirectory");
            existing ??= part;
        }
        return existing ?? throw new GitActionException("InitInvalidDirectory");
    }
    private static bool HasMetadataAncestor(string directory)
    {
        for (var part = directory; part is not null; part = Path.GetDirectoryName(part))
        {
            // Probe only known metadata entries; never recursively enumerate a project.
            if (Exists(Path.Combine(part, ".git")) || Exists(Path.Combine(part, "HEAD")) &&
                (Exists(Path.Combine(part, "objects")) || Exists(Path.Combine(part, "reftable")))) return true;
        }
        return false;
        static bool Exists(string path)
        {
            try { File.GetAttributes(path); return true; }
            catch (FileNotFoundException) { return false; }
            catch (DirectoryNotFoundException) { return false; }
        }
    }
    private static GitReadException ReadFailure(ProcessResult result) => new(result.Outcome switch
    {
        ProcessOutcome.Cancelled => GitFailure.Cancelled, ProcessOutcome.TimedOut => GitFailure.TimedOut,
        ProcessOutcome.OutputLimit => GitFailure.OutputLimit, ProcessOutcome.StartFailed => GitFailure.StartFailed, _ => GitFailure.CommandFailed
    }, Redact(Encoding.UTF8.GetString(result.StandardError)));
}
