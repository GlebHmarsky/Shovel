using System.Security.Cryptography;
using System.Text;
using System.Text.RegularExpressions;
using GitGui.Core.Processes;

namespace GitGui.Core.Git;

public sealed record RepositoryCloneOptions(string Source, string Directory, string Branch = "")
{
    public bool IsValid => new RemoteSetupOptions("origin", Source).IsValid &&
        (Path.IsPathFullyQualified(Source) || Uri.TryCreate(Source, UriKind.Absolute, out var uri) && uri.Scheme is "http" or "https" or "ssh" or "git" or "file" ||
         Regex.IsMatch(Source, @"^(?:[^@\s/:]+@)?[^@\s/:]+:.+$", RegexOptions.CultureInvariant) && !Regex.IsMatch(Source, @"^[A-Za-z]:")) && Path.IsPathFullyQualified(Directory) &&
        Directory.IndexOfAny(Path.GetInvalidPathChars()) < 0 && !Branch.Any(char.IsControl) && !Branch.StartsWith('-');
}
public sealed record RepositoryCloneResult(ActionResult Action, RepositorySnapshot? Repository);

public sealed partial class GitActions
{
    public Task<RepositoryCloneResult> CloneRepositoryAsync(GitInstallation git, RepositoryCloneOptions options,
        CancellationToken token, Action<ProcessOutput>? progress = null, IAuthenticationSession? authentication = null) => Task.Run(() =>
    {
        if (!options.IsValid) throw new GitActionException("CloneInvalid");
        using var cancellation = CancellationTokenSource.CreateLinkedTokenSource(token, authentication?.CancellationToken ?? default);
        token = cancellation.Token; token.ThrowIfCancellationRequested();
        var target = Path.TrimEndingDirectorySeparator(Path.GetFullPath(options.Directory));
        var key = Convert.ToHexString(SHA256.HashData(Encoding.UTF8.GetBytes(target.ToUpperInvariant())));
        // Share initialization's destination lock; no repository identity exists yet.
        using var gate = new Mutex(false, @"Local\Shovel.Init." + key);
        bool owned; try { owned = gate.WaitOne(0); } catch (AbandonedMutexException) { owned = true; }
        if (!owned) throw new GitActionException("ActionOtherInstance");
        try { return Clone().GetAwaiter().GetResult(); }
        finally { gate.ReleaseMutex(); }

        async Task<RepositoryCloneResult> Clone()
        {
            var parent = ValidateDestination(target);
            if (System.IO.Directory.Exists(target) && System.IO.Directory.EnumerateFileSystemEntries(target).Any())
                throw new GitActionException("CloneNotEmpty");
            var environment = EnvironmentForGit(); environment["GIT_OPTIONAL_LOCKS"] = null;
            await ConfigureNetworkEnvironmentAsync(git, parent, environment, authentication, token);
            token.ThrowIfCancellationRequested();
            if (!SameDirectory(parent, ValidateDestination(target))) throw new GitActionException("InitDestinationChanged");
            var result = await runner.RunAsync(new(git.Executable, parent,
                ["--no-pager", "--no-lazy-fetch", "-c", "core.fsmonitor=false", "-c", "gc.auto=0", "-c", "maintenance.auto=false",
                 "-c", "protocol.allow=never", "-c", "protocol.file.allow=always", "-c", "protocol.https.allow=always",
                 "-c", "protocol.http.allow=always", "-c", "protocol.ssh.allow=always", "-c", "protocol.git.allow=always",
                 "clone", "--progress", "--origin=origin", .. options.Branch.Length == 0 ? Array.Empty<string>() : ["--branch=" + options.Branch],
                 "--", options.Source, target])
                { Environment = environment, Timeout = TimeSpan.FromMinutes(10), MaximumOutputBytes = 1024 * 1024 }, token, progress);
            var action = new ActionResult(result.Outcome, result.ExitCode, Redact(Decode(result) + Encoding.UTF8.GetString(result.StandardError)))
                { MessageCode = result.Outcome switch { ProcessOutcome.Cancelled => "CloneCancelled", ProcessOutcome.TimedOut => "CloneTimedOut",
                    ProcessOutcome.OutputLimit => "CloneOutputLimit", _ => result.ExitCode == 0 ? "CloneSucceeded" : "CloneIncomplete" } };
            RepositorySnapshot? repository = null;
            // Cancellation is not rollback. Observe whatever native Git left, without deleting it.
            using var inspect = new CancellationTokenSource(TimeSpan.FromSeconds(20));
            try
            {
                if (System.IO.Directory.Exists(target))
                {
                    ValidateDestination(target);
                    var observed = await reader.InspectAsync(git, target, inspect.Token, includeHistory: false);
                    if (SameDirectory(target, observed.Worktree)) repository = observed;
                }
            }
            catch (Exception error) when (error is GitReadException or GitActionException or IOException or UnauthorizedAccessException or OperationCanceledException) { }
            if (action.Success && repository is null) action = action with { MessageCode = "CloneUnverified" };
            return new(action, repository);
        }
    }, token);

    private async Task ConfigureNetworkEnvironmentAsync(GitInstallation git, string path, Dictionary<string, string?> environment,
        IAuthenticationSession? authentication, CancellationToken token)
    {
        if (authentication is not null) foreach (var pair in authentication.Environment) environment[pair.Key] = pair.Value;
        var sshConfig = await Raw(git, path, ["config", "--get", "core.sshCommand"], token);
        if (sshConfig.Outcome != ProcessOutcome.Exited || sshConfig.ExitCode is not (0 or 1)) throw ReadFailure(sshConfig);
        if (sshConfig.ExitCode == 0) throw new GitActionException("ActionCustomSsh");
        var root = System.IO.Directory.GetParent(Path.GetDirectoryName(git.Executable)!)!;
        if (root.Name is "mingw64" or "mingw32" or "ucrt64") root = root.Parent!;
        var ssh = Path.Combine(root.FullName, "usr", "bin", "ssh.exe");
        if (!File.Exists(ssh)) throw new GitActionException("ActionSshMissing");
        environment["GIT_SSH_COMMAND"] = "'" + ssh.Replace('\\', '/').Replace("'", "'\\''") +
            (authentication is null ? "' -oBatchMode=yes -oStrictHostKeyChecking=yes" : "' -oBatchMode=no -oStrictHostKeyChecking=ask");
        environment["GIT_SSH_VARIANT"] = "ssh";
    }
}
