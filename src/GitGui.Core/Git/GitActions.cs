using System.Security.Cryptography;
using System.Text;
using System.Text.RegularExpressions;
using GitGui.Core.Processes;

namespace GitGui.Core.Git;

public enum GitAction { NewBranch, SwitchBranch, Fetch, Pull, Push }
public enum GitActionStage { Validating, Fetching, CheckingFetchedTip, Rebasing, Merging, Pushing, UpdatingBranch }
public enum PullStrategy { Rebase, Merge }
public sealed record GitRemote(string Name, string FetchAddress, string PushAddress);
public sealed record ActionContext(GitInstallation Git, RepositoryIdentity Identity, RepositorySnapshot Repository,
    HistorySession History, IReadOnlyList<GitRemote> Remotes, string ConfigStamp);
public sealed record ActionOptions(GitAction Action, string Branch = "", string StartRef = "HEAD",
    string Remote = "", string RemoteBranch = "", bool Checkout = true, bool AllRemotes = false,
    bool AllTags = false, bool SetUpstream = false, PullStrategy PullStrategy = PullStrategy.Rebase, bool AutoStash = false);
public sealed class GitActionException(string code) : Exception(code) { public string Code { get; } = code; }
public sealed record ActionResult(ProcessOutcome Outcome, int? ExitCode, string Output)
{
    public RepositoryOperation Operation { get; init; }
    public bool HasConflicts { get; init; }
    public bool Success => Outcome == ProcessOutcome.Exited && ExitCode == 0 && Operation == RepositoryOperation.None && !HasConflicts;
}

/// <summary>Explicit user actions. Never used by startup/history reads. No persistent raw output.</summary>
public sealed partial class GitActions(IProcessRunner runner, RepositoryReader reader, IRepositoryIdentityProvider identities)
{
    public Task<ActionContext> PrepareAsync(GitInstallation git, string path, CancellationToken token) => Task.Run(async () =>
    {
        var identity = await identities.ReadAsync(path, token).ConfigureAwait(false);
        var initialConfig = await ConfigStamp(git, path, token);
        var repository = await reader.InspectAsync(git, path, token).ConfigureAwait(false);
        if (!SamePaths(identity, repository)) throw new GitActionException("ActionIdentityChanged");
        var history = await reader.OpenInspectedHistoryAsync(git, repository, HistoryScope.AllCommits, token).ConfigureAwait(false);
        var remotes = new List<GitRemote>();
        foreach (var name in repository.RemoteNames)
        {
            if (remotes.Count >= 32 || name.StartsWith('-') || name.Contains('\r') || name.Any(char.IsControl))
                throw new GitActionException("ActionUnsupportedRemote");
            await Read(git, path, ["check-ref-format", "refs/remotes/" + name + "/check"], token);
            var addresses = await Task.WhenAll(Read(git, path, ["remote", "get-url", "--all", name], token),
                Read(git, path, ["remote", "get-url", "--push", "--all", name], token));
            var fetch = Decode(addresses[0]).TrimEnd('\r', '\n');
            var push = Decode(addresses[1]).TrimEnd('\r', '\n');
            // Do not expose URL credentials. Multiple push destinations need a separate scope UI.
            if (fetch.Contains('\n') || push.Contains('\n')) throw new GitActionException("ActionUnsupportedRemote");
            remotes.Add(new(name, Redact(fetch), Redact(push)));
        }
        var stamp = await ConfigStamp(git, path, token);
        if (initialConfig != stamp) throw new GitActionException("ActionChanged");
        if ((await identities.ReadAsync(path, token)).Fingerprint != identity.Fingerprint)
            throw new GitActionException("ActionIdentityChanged");
        return new ActionContext(git, identity, repository, history, remotes, stamp);
    }, token);

    public Task<ActionResult> ExecuteAsync(ActionContext context, ActionOptions options, CancellationToken token,
        Action<ProcessOutput>? progress = null, IAuthenticationSession? authentication = null,
        Action<GitActionStage>? stage = null) => Task.Run(() =>
    {
        using var authenticationStop = CancellationTokenSource.CreateLinkedTokenSource(token,
            authentication?.CancellationToken ?? CancellationToken.None);
        token = authenticationStop.Token;
        // Named mutex covers shared refs across linked worktrees and Shovel profiles. Acquire/release
        // on this one worker thread: an await continuation must never release another thread's mutex.
        var key = Convert.ToHexString(SHA256.HashData(Encoding.UTF8.GetBytes(context.Identity.CommonDirectory.ToUpperInvariant())));
        using var gate = new Mutex(false, @"Local\Shovel.GitAction." + key);
        bool owned;
        try { owned = gate.WaitOne(0); } catch (AbandonedMutexException) { owned = true; }
        if (!owned) throw new GitActionException("ActionOtherInstance");
        try { return ExecuteLocked().GetAwaiter().GetResult(); }
        finally { gate.ReleaseMutex(); }

        async Task<ActionResult> ExecuteLocked()
        {
            token.ThrowIfCancellationRequested();
            stage?.Invoke(GitActionStage.Validating);
            var git = context.Git; var path = context.Identity.Worktree;
            if ((await identities.ReadAsync(path, token)).Fingerprint != context.Identity.Fingerprint)
                throw new GitActionException("ActionIdentityChanged");
            var fresh = await reader.InspectAsync(git, path, token);
            var history = await reader.OpenInspectedHistoryAsync(git, fresh, HistoryScope.AllCommits, token);
            if (!SamePaths(context.Identity, fresh) || history.Fingerprint != context.History.Fingerprint ||
                fresh.Branch != context.Repository.Branch || fresh.Commit != context.Repository.Commit ||
                await ConfigStamp(git, path, token) != context.ConfigStamp)
                throw new GitActionException("ActionChanged");
            if (fresh.Operation != RepositoryOperation.None) throw new GitActionException("ActionSequence");
            if (options.Action == GitAction.Pull && !Enum.IsDefined(options.PullStrategy)) throw new GitActionException("ActionReadFailed");
            if (options.Action == GitAction.SwitchBranch || options.Action == GitAction.NewBranch && options.Checkout)
            {
                // Branch switching has not yet been validated for these worktree modes.
                if (File.Exists(Path.Combine(path, ".gitmodules")) || File.Exists(Path.Combine(fresh.GitDirectory, "info", "sparse-checkout")))
                    throw new GitActionException("ActionWorktreeUnsupported");
                var status = await Read(git, path, ["status", "--porcelain=v1", "-z", "--untracked-files=normal", "--ignore-submodules=none"], token);
                if (status.StandardOutput.Length != 0) throw new GitActionException("ActionDirty");
            }
            var commands = new List<string[]>();
            if (options.Action is GitAction.NewBranch or GitAction.SwitchBranch)
            {
                await ValidateBranch(options.Branch);
                if (options.Action == GitAction.NewBranch)
                {
                    var start = history.References.SingleOrDefault(item => item.Name == options.StartRef);
                    if (start is null) throw new GitActionException("ActionNoStart");
                    commands.Add(options.Checkout ? ["switch", "--no-guess", "--no-overwrite-ignore", "--no-recurse-submodules", "--no-track", "--create", options.Branch, start.Hash]
                        : ["branch", "--no-track", "--", options.Branch, start.Hash]);
                }
                else
                {
                    if (!history.References.Any(item => item.Name == "refs/heads/" + options.Branch)) throw new GitActionException("ActionChanged");
                    commands.Add(["switch", "--no-guess", "--no-overwrite-ignore", "--no-recurse-submodules", "--", options.Branch]);
                }
            }
            else
            {
                var selected = context.Remotes.SingleOrDefault(item => item.Name == options.Remote);
                if (selected is null) throw new GitActionException("ActionNoRemote");
                if (options.Action == GitAction.Fetch)
                {
                    foreach (var remote in options.AllRemotes ? context.Remotes : [selected])
                        commands.Add(["fetch", "--progress", "--no-recurse-submodules", "--no-prune", "--no-prune-tags", "--no-auto-maintenance",
                            "--refmap=", "--", remote.Name, "+refs/heads/*:refs/remotes/" + remote.Name + "/*"]);
                }
                else
                {
                    await ValidateBranch(options.RemoteBranch);
                    if (options.Action == GitAction.Pull)
                    {
                        if (fresh.HeadKind != HeadKind.Branch) throw new GitActionException("ActionAttachedRequired");
                        // Fetch explicit scope first; merge immutable fetched tip only if local HEAD still matches.
                        commands.Add(["fetch", "--progress", "--no-recurse-submodules", "--no-prune", "--no-prune-tags", "--no-auto-maintenance", "--refmap=",
                            "--", selected.Name, "+refs/heads/" + options.RemoteBranch + ":refs/remotes/" + selected.Name + "/" + options.RemoteBranch]);
                    }
                    else
                    {
                        await ValidateBranch(options.Branch);
                        if (!history.References.Any(item => item.Name == "refs/heads/" + options.Branch)) throw new GitActionException("ActionChanged");
                        commands.Add(["-c", "remote." + selected.Name + ".mirror=false", "push", "--porcelain", "--progress", "--no-force", "--no-follow-tags",
                            "--recurse-submodules=no", .. options.AllTags ? new[] { "--tags" } : [], .. options.SetUpstream ? new[] { "--set-upstream" } : [],
                            "--", selected.Name, "refs/heads/" + options.Branch + ":refs/heads/" + options.RemoteBranch]);
                    }
                }
            }
            var output = new StringBuilder();
            ActionResult? last = null;
            foreach (var command in commands)
            {
                stage?.Invoke(options.Action switch
                {
                    GitAction.Fetch or GitAction.Pull => GitActionStage.Fetching,
                    GitAction.Push => GitActionStage.Pushing,
                    _ => GitActionStage.UpdatingBranch
                });
                last = await Write(command);
                if (!last.Success) return last with { Output = output.ToString() };
            }
            if (options.Action == GitAction.Pull)
            {
                token.ThrowIfCancellationRequested();
                stage?.Invoke(GitActionStage.CheckingFetchedTip);
                var afterFetch = await reader.InspectAsync(git, path, token);
                if (afterFetch.Branch != fresh.Branch || afterFetch.Commit != fresh.Commit || afterFetch.Operation != RepositoryOperation.None ||
                    await ConfigStamp(git, path, token) != context.ConfigStamp ||
                    (await identities.ReadAsync(path, token)).Fingerprint != context.Identity.Fingerprint)
                    throw new GitActionException("ActionChangedAfterFetch");
                var tip = Decode(await Read(git, path, ["rev-parse", "--verify", "FETCH_HEAD^{commit}"], token)).Trim();
                if (!Regex.IsMatch(tip, "^[0-9a-f]{40}([0-9a-f]{24})?$")) throw new GitActionException("ActionChangedAfterFetch");
                var trackingTip = Decode(await Read(git, path, ["rev-parse", "--verify", "refs/remotes/" + options.Remote + "/" + options.RemoteBranch + "^{commit}"], token)).Trim();
                if (tip != trackingTip) throw new GitActionException("ActionChangedAfterFetch");
                var stash = options.AutoStash ? "--autostash" : "--no-autostash";
                if (options.PullStrategy == PullStrategy.Rebase)
                {
                    // Rebase's internal checkout may overwrite ignored paths. Check incoming additions
                    // against ignored files first; ordinary untracked/dirty conflicts remain Git's decision.
                    var additions = Decode(await Read(git, path, ["diff", "--no-ext-diff", "--no-textconv", "--no-renames", "--name-only", "--diff-filter=A", "-z", fresh.Commit!, tip, "--"], token))
                        .Split('\0', StringSplitOptions.RemoveEmptyEntries);
                    if (additions.Length != 0)
                    {
                        // Collapse wholly ignored directories instead of enumerating generated build trees.
                        var ignored = Decode(await Read(git, path, ["ls-files", "--others", "--ignored", "--exclude-standard", "--directory", "--no-empty-directory", "-z"], token))
                            .Split('\0', StringSplitOptions.RemoveEmptyEntries).Select(file => file.TrimEnd('/')).ToArray();
                        if (PathsOverlap(ignored, additions))
                            throw new GitActionException("ActionIgnoredCollision");
                    }
                    stage?.Invoke(GitActionStage.Rebasing);
                    last = await Write(["-c", "submodule.recurse=false", "rebase", "--merge", "--no-fork-point", "--no-update-refs", "--no-rebase-merges", stash, tip]);
                }
                else
                {
                    stage?.Invoke(GitActionStage.Merging);
                    // The explicit choice defines strategy; branch.mergeOptions must not turn it into squash/no-commit/ff-only.
                    last = await Write(["-c", "submodule.recurse=false", "-c", "branch." + fresh.Branch + ".mergeOptions=", "merge", "--ff", "--no-squash", "--commit", "--no-overwrite-ignore", stash, "--no-edit", tip]);
                }
                // Autostash application can conflict even when Git exits zero. Never report that as clean success.
                if (last.Outcome == ProcessOutcome.Exited)
                {
                    var observed = await reader.InspectAsync(git, path, token);
                    var unmerged = await Read(git, path, ["ls-files", "--unmerged", "-z"], token);
                    last = last with { Operation = observed.Operation, HasConflicts = unmerged.StandardOutput.Length != 0 };
                }
            }
            return last! with { Output = output.ToString() };

            async Task ValidateBranch(string name)
            {
                if (string.IsNullOrWhiteSpace(name) || name.StartsWith('-') || name == "HEAD" || name.Contains('@') || name.Any(char.IsControl))
                    throw new GitActionException("ActionBranchName");
                try { await Read(git, path, ["check-ref-format", "refs/heads/" + name], token); }
                catch (GitReadException) { throw new GitActionException("ActionBranchName"); }
            }
            async Task<ActionResult> Write(string[] args)
            {
                var environment = EnvironmentForGit();
                environment["GIT_OPTIONAL_LOCKS"] = null;
                environment["GIT_MERGE_AUTOEDIT"] = "no";
                // The broker exists only for this user-started operation, never history reads.
                if (options.Action is GitAction.Fetch or GitAction.Pull or GitAction.Push)
                {
                    if (authentication is not null)
                        foreach (var pair in authentication.Environment) environment[pair.Key] = pair.Value;
                    var sshConfig = await Raw(git, path, ["config", "--get", "core.sshCommand"], token);
                    if (sshConfig.ExitCode == 0) throw new GitActionException("ActionCustomSsh");
                    var root = Directory.GetParent(Path.GetDirectoryName(git.Executable)!)!;
                    if (root.Name is "mingw64" or "mingw32" or "ucrt64") root = root.Parent!;
                    var ssh = Path.Combine(root.FullName, "usr", "bin", "ssh.exe");
                    if (!File.Exists(ssh)) throw new GitActionException("ActionSshMissing");
                    environment["GIT_SSH_COMMAND"] = "'" + ssh.Replace('\\', '/').Replace("'", "'\\''") +
                        (authentication is null ? "' -oBatchMode=yes -oStrictHostKeyChecking=yes" : "' -oBatchMode=no -oStrictHostKeyChecking=ask");
                    environment["GIT_SSH_VARIANT"] = "ssh";
                }
                var result = await runner.RunAsync(new(git.Executable, path,
                    ["--no-pager", "--no-lazy-fetch", "-c", "core.fsmonitor=false", "-c", "gc.auto=0", "-c", "maintenance.auto=false",
                     "-c", "protocol.allow=never", "-c", "protocol.file.allow=always", "-c", "protocol.https.allow=always",
                     "-c", "protocol.http.allow=always", "-c", "protocol.ssh.allow=always", "-c", "protocol.git.allow=always", .. args])
                { Environment = environment, Timeout = TimeSpan.FromMinutes(5), MaximumOutputBytes = 1024 * 1024 }, token, progress);
                var text = Redact(Decode(result) + Encoding.UTF8.GetString(result.StandardError));
                if (output.Length + text.Length <= 1024 * 1024) output.Append(text);
                else throw new GitActionException("ActionOutputLimit");
                return new(result.Outcome, result.ExitCode, text);
            }
        }
    }, token);

    private static bool SamePaths(RepositoryIdentity identity, RepositorySnapshot repository) =>
        string.Equals(identity.Worktree, repository.Worktree, StringComparison.OrdinalIgnoreCase) &&
        string.Equals(identity.GitDirectory, repository.GitDirectory, StringComparison.OrdinalIgnoreCase) &&
        string.Equals(identity.CommonDirectory, repository.CommonDirectory, StringComparison.OrdinalIgnoreCase);
    private static bool PathsOverlap(string[] existing, string[] incoming)
    {
        var files = new HashSet<string>(existing, StringComparer.OrdinalIgnoreCase);
        var directories = new HashSet<string>(StringComparer.OrdinalIgnoreCase);
        foreach (var file in existing)
            for (var i = file.IndexOf('/'); i >= 0; i = file.IndexOf('/', i + 1)) directories.Add(file[..i]);
        foreach (var file in incoming)
        {
            if (files.Contains(file) || directories.Contains(file)) return true;
            for (var i = file.IndexOf('/'); i >= 0; i = file.IndexOf('/', i + 1)) if (files.Contains(file[..i])) return true;
        }
        return false;
    }
    private async Task<string> ConfigStamp(GitInstallation git, string path, CancellationToken token) =>
        Convert.ToHexString(SHA256.HashData((await Read(git, path, ["config", "--null", "--list"], token)).StandardOutput));
    private async Task<ProcessResult> Read(GitInstallation git, string path, string[] args, CancellationToken token)
    {
        var result = await Raw(git, path, args, token);
        if (result.Outcome != ProcessOutcome.Exited || result.ExitCode != 0)
            throw new GitReadException(result.Outcome == ProcessOutcome.Cancelled ? GitFailure.Cancelled : GitFailure.CommandFailed, Redact(Encoding.UTF8.GetString(result.StandardError)));
        return result;
    }
    private Task<ProcessResult> Raw(GitInstallation git, string path, string[] args, CancellationToken token) => runner.RunAsync(
        new(git.Executable, path, ["--no-pager", "--no-optional-locks", "--no-lazy-fetch", "-c", "core.fsmonitor=false", "-c", "gc.auto=0", "-c", "maintenance.auto=false", .. args])
        { Environment = EnvironmentForGit(), MaximumOutputBytes = 1024 * 1024 }, token);
    private static string Decode(ProcessResult result) => Encoding.UTF8.GetString(result.StandardOutput);
    private static Dictionary<string, string?> EnvironmentForGit()
    {
        var environment = new Dictionary<string, string?>(StringComparer.OrdinalIgnoreCase);
        foreach (System.Collections.DictionaryEntry variable in Environment.GetEnvironmentVariables())
        {
            var key = (string)variable.Key;
            if ((key.StartsWith("GIT_", StringComparison.OrdinalIgnoreCase) && key is not ("GIT_CONFIG_GLOBAL" or "GIT_CONFIG_SYSTEM" or "GIT_CONFIG_NOSYSTEM")) ||
                key.StartsWith("SHOVEL_ASKPASS_", StringComparison.OrdinalIgnoreCase) || key.StartsWith("GCM_TRACE", StringComparison.OrdinalIgnoreCase)) environment[key] = null;
        }
        environment["GIT_TERMINAL_PROMPT"] = "0"; environment["GCM_INTERACTIVE"] = "never";
        environment["GIT_ASKPASS"] = ""; environment["SSH_ASKPASS"] = ""; environment["SSH_ASKPASS_REQUIRE"] = "never";
        environment["SSH_ASKPASS_PROMPT"] = null;
        environment["GCM_DEBUG"] = "0"; environment["GCM_TRACE"] = "0"; environment["GCM_TRACE_SECRETS"] = "0";
        environment["GCM_TRACE_MSAUTH"] = "0";
        environment["GIT_OPTIONAL_LOCKS"] = "0"; environment["GIT_NO_LAZY_FETCH"] = "1";
        return environment;
    }
    public static string Redact(string text) => Regex.Replace(Regex.Replace(text,
        @"(https?://)[^\s/@]+@", "$1[redacted]@", RegexOptions.IgnoreCase, TimeSpan.FromSeconds(1)),
        @"(https?://[^\s?#]+)[?#][^\s]*", "$1?[redacted]", RegexOptions.IgnoreCase, TimeSpan.FromSeconds(1));
}
