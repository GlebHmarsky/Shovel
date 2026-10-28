using System.Globalization;
using System.Security.Cryptography;
using System.Text;
using GitGui.Core.Processes;

namespace GitGui.Core.Git;

public sealed record StashEntry(string Hash, string Selector, string Message, DateTimeOffset Date);
public sealed record StashList(IReadOnlyList<StashEntry> Entries, string Stamp);
public enum StashAction { Create, Apply, Pop, Drop }
public sealed record StashOptions(StashAction Action, StashList List, StashEntry? Entry = null,
    string Message = "", bool IncludeUntracked = false, bool RestoreIndex = false);
public sealed record StashInspection(CommitRecord Worktree, CommitRecord Index, CommitRecord? Untracked);

public sealed partial class GitActions
{
    public Task<StashList> ReadStashesAsync(GitInstallation git, RepositoryIdentity identity, CancellationToken token) => Task.Run(async () =>
    {
        if ((await identities.ReadAsync(identity.Worktree, token)).Fingerprint != identity.Fingerprint) throw new GitActionException("ActionIdentityChanged");
        var result = await Read(git, identity.Worktree, ["stash", "list", "-z", "--no-show-signature", "--no-notes", "--max-count=1001",
            "--format=%H%x00%gd%x00%gs%x00%ct"], token);
        var fields = Decode(result).Split('\0'); var entries = new List<StashEntry>();
        if (fields[^1] != "" || (fields.Length - 1) % 4 != 0) throw new GitReadException(GitFailure.InvalidOutput);
        for (var i = 0; i < fields.Length - 1; i += 4)
        {
            if (entries.Count == 1000) throw new GitActionException("StashListLimit");
            if (!ObjectId(fields[i]) || fields[i + 1] != "stash@{" + entries.Count.ToString(CultureInfo.InvariantCulture) + "}" ||
                !long.TryParse(fields[i + 3], CultureInfo.InvariantCulture, out var seconds)) throw new GitReadException(GitFailure.InvalidOutput);
            entries.Add(new(fields[i], fields[i + 1], fields[i + 2], DateTimeOffset.FromUnixTimeSeconds(seconds)));
        }
        if ((await identities.ReadAsync(identity.Worktree, token)).Fingerprint != identity.Fingerprint) throw new GitActionException("ActionIdentityChanged");
        return new StashList(entries, Convert.ToHexString(SHA256.HashData(result.StandardOutput)));
    }, token);

    public Task<StashInspection> InspectStashAsync(GitInstallation git, RepositoryIdentity identity, string hash, CancellationToken token) => Task.Run(async () =>
    {
        if (!ObjectId(hash)) throw new GitReadException(GitFailure.InvalidOutput);
        if ((await identities.ReadAsync(identity.Worktree, token)).Fingerprint != identity.Fingerprint) throw new GitActionException("ActionIdentityChanged");
        var data = Decode(await Read(git, identity.Worktree, ["cat-file", "-p", hash], token)).Split("\n\n", 2);
        var parents = data[0].Split('\n').Where(line => line.StartsWith("parent ", StringComparison.Ordinal)).Select(line => line[7..]).ToArray();
        if (parents.Length is < 2 or > 3 || parents.Any(parent => !ObjectId(parent))) throw new GitActionException("StashInvalid");
        CommitRecord Record(string id, string[] ancestry) => new(id, ancestry, "", "", DateTimeOffset.UnixEpoch, "", "", DateTimeOffset.UnixEpoch, data.Length == 2 ? data[1] : "");
        return new StashInspection(Record(hash, [parents[0], parents[1]]), Record(parents[1], [parents[0]]), parents.Length == 3 ? Record(parents[2], []) : null);
    }, token);

    public Task<ActionResult> ExecuteStashAsync(ActionContext context, StashOptions options, CancellationToken token,
        Action<ProcessOutput>? progress = null) => Task.Run(() =>
    {
        var key = Convert.ToHexString(SHA256.HashData(Encoding.UTF8.GetBytes(context.Identity.CommonDirectory.ToUpperInvariant())));
        using var gate = new Mutex(false, @"Local\Shovel.GitAction." + key);
        bool owned; try { owned = gate.WaitOne(0); } catch (AbandonedMutexException) { owned = true; }
        if (!owned) throw new GitActionException("ActionOtherInstance");
        try { return ExecuteLocked().GetAwaiter().GetResult(); } finally { gate.ReleaseMutex(); }
        async Task<ActionResult> ExecuteLocked()
        {
            if (!Enum.IsDefined(options.Action)) throw new GitActionException("ActionReadFailed");
            var git = context.Git; var path = context.Identity.Worktree;
            var current = await ReadStashesAsync(git, context.Identity, token);
            var fresh = await reader.ReadWorkingCopyHeadAsync(git, context.Repository, token);
            if (fresh.Commit != context.Repository.Commit || fresh.Branch != context.Repository.Branch) throw new GitActionException("ActionChanged");
            if (options.Action != StashAction.Drop && fresh.Operation != RepositoryOperation.None) throw new GitActionException("ActionSequence");
            if (options.Action == StashAction.Create && fresh.Commit is null) throw new GitActionException("StashNeedsCommit");
            if (options.Action != StashAction.Create && (current.Stamp != options.List.Stamp || options.Entry is null || !current.Entries.Contains(options.Entry)))
                throw new GitActionException("StashChanged");
            if (options.Action != StashAction.Drop)
            {
                var index = await IndexStampAsync(fresh, token);
                if ((await Read(git, path, ["ls-files", "--unmerged", "-z"], token)).StandardOutput.Length != 0)
                    throw new GitActionException("CommitUnresolved");
                if (index != await IndexStampAsync(fresh, token)) throw new GitActionException("CommitStale");
            }
            if (options.Message.Contains('\0') || Encoding.UTF8.GetByteCount(options.Message) > 4096) throw new GitActionException("StashMessageLimit");
            string[] args = options.Action switch
            {
                StashAction.Create => ["stash", "push", "--no-keep-index", .. options.IncludeUntracked ? new[] { "--include-untracked" } : [], "--message=" + options.Message],
                StashAction.Drop => ["stash", "drop", options.Entry!.Selector],
                _ => ["stash", "apply", options.RestoreIndex ? "--index" : "--no-index", options.Entry!.Hash]
            };
            if (options.Action == StashAction.Drop && (await ReadStashesAsync(git, context.Identity, token)).Stamp != current.Stamp)
                throw new GitActionException("StashChanged");
            var result = await Write(args, token);
            if (options.Action is StashAction.Apply or StashAction.Pop)
            {
                using var observe = new CancellationTokenSource(TimeSpan.FromSeconds(20));
                try
                {
                    var conflicts = await Read(git, path, ["ls-files", "--unmerged", "-z"], observe.Token);
                    result = result with { HasConflicts = conflicts.StandardOutput.Length > 0 };
                }
                catch (Exception error) when (error is GitReadException or OperationCanceledException or IOException)
                { return result with { HasConflicts = true, MessageCode = "StashInspectFailed" }; }
                if (!result.Success) return result with { MessageCode = "StashApplyFailed" };
                if (options.Action == StashAction.Pop)
                {
                    // Never retry apply. Once content is applied, a failed/cancelled drop is a separate outcome.
                    try
                    {
                        token.ThrowIfCancellationRequested();
                        var after = await ReadStashesAsync(git, context.Identity, token);
                        if (after.Stamp != current.Stamp) return result with { MessageCode = "StashAppliedRetained" };
                        var drop = await Write(["stash", "drop", options.Entry!.Selector], token);
                        return drop with { Output = result.Output + drop.Output, MessageCode = drop.Success ? "StashPopped" : "StashAppliedRetained" };
                    }
                    catch (Exception error) when (error is GitReadException or GitActionException or OperationCanceledException or IOException or UnauthorizedAccessException)
                    { return result with { MessageCode = "StashAppliedRetained" }; }
                }
            }
            return result;

            async Task<ActionResult> Write(string[] arguments, CancellationToken cancellation)
            {
                var environment = EnvironmentForGit(); environment["GIT_OPTIONAL_LOCKS"] = null; environment["GIT_EDITOR"] = "false";
                var run = await runner.RunAsync(new(git.Executable, path, ["--no-pager", "--no-lazy-fetch", "-c", "core.fsmonitor=false",
                    "-c", "gc.auto=0", "-c", "maintenance.auto=false", "-c", "submodule.recurse=false", .. arguments])
                    { Environment = environment, Timeout = TimeSpan.FromMinutes(5), MaximumOutputBytes = 1024 * 1024 }, cancellation, progress);
                return new(run.Outcome, run.ExitCode, Redact(Decode(run) + Encoding.UTF8.GetString(run.StandardError)));
            }
        }
    }, token);
    private static bool ObjectId(string hash) => hash.Length is 40 or 64 && hash.All(Uri.IsHexDigit);
}
