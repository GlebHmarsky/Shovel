using System.Text;
using GitGui.Core.Processes;

namespace GitGui.Core.Git;

public enum BranchManagementAction { Delete, SetUpstream, UnsetUpstream }
public sealed record BranchManagementOptions(BranchManagementAction Action, CommitRef Branch, CommitRef? Upstream = null);

public sealed partial class GitActions
{
    // No remote URL resolution: these commands operate on existing local refs/configuration only.
    public Task<ActionContext> PrepareBranchManagementAsync(GitInstallation git, RepositoryIdentity expected,
        RepositorySnapshot snapshot, HistorySession history, CancellationToken token) => Task.Run(async () =>
    {
        await CheckIdentity();
        var config = await ConfigStamp(git, expected.Worktree, token);
        var fresh = await reader.InspectAsync(git, expected.Worktree, token);
        if (fresh.Operation != RepositoryOperation.None) throw new GitActionException("ActionSequence");
        var refs = await reader.OpenInspectedHistoryAsync(git, fresh, HistoryScope.AllCommits, token);
        if (!SamePaths(expected, fresh) || fresh.Commit != snapshot.Commit || fresh.Branch != snapshot.Branch ||
            refs.Fingerprint != history.Fingerprint || config != await ConfigStamp(git, expected.Worktree, token))
            throw new GitActionException("BranchManageChanged");
        await CheckIdentity();
        return new ActionContext(git, expected, fresh, refs, [], config);

        async Task CheckIdentity()
        {
            if ((await identities.ReadAsync(expected.Worktree, token)).Fingerprint != expected.Fingerprint)
                throw new GitActionException("ActionIdentityChanged");
        }
    }, token);

    public Task<ActionResult> ExecuteBranchManagementAsync(ActionContext context, BranchManagementOptions options,
        CancellationToken token, Action<ProcessOutput>? progress = null) => Task.Run(() =>
    {
        // Mutex ownership must remain on this worker while async Git work runs on continuations.
        using var gate = EnterMutationGate(context.Identity);
        try { return ExecuteLocked().GetAwaiter().GetResult(); }
        finally { gate.ReleaseMutex(); }

        async Task<ActionResult> ExecuteLocked()
        {
            if (!Enum.IsDefined(options.Action)) throw new GitActionException("BranchManageUnavailable");
            var git = context.Git; var path = context.Identity.Worktree;
            var fresh = await Revalidate();
            var branch = options.Branch;
            if (!branch.Name.StartsWith("refs/heads/", StringComparison.Ordinal) || !context.History.References.Contains(branch))
                throw new GitActionException("BranchManageChanged");
            var name = branch.Name[11..];
            if (name.Length == 0 || name.StartsWith('-') || name.Any(char.IsControl)) throw new GitActionException("BranchManageChanged");
            await Read(git, path, ["check-ref-format", branch.Name], token);
            await CheckDirectRef(branch);
            string[] arguments;
            switch (options.Action)
            {
                case BranchManagementAction.Delete:
                    if (fresh.Branch == name) throw new GitActionException("BranchManageCurrent");
                    await CheckWorktrees();
                    arguments = ["branch", "--delete", "--", name];
                    break;
                case BranchManagementAction.SetUpstream:
                    if (options.Upstream is not { } upstream || !upstream.Name.StartsWith("refs/remotes/", StringComparison.Ordinal) ||
                        !context.History.References.Contains(upstream)) throw new GitActionException("BranchManageNoRemoteRef");
                    await CheckDirectRef(upstream);
                    arguments = ["branch", "--set-upstream-to=" + upstream.Name, "--", name];
                    break;
                default:
                    // Git itself handles tracking whose destination no longer exists. Do not infer
                    // 'unset' from a missing destination or silently treat a failed command as success.
                    arguments = ["branch", "--unset-upstream", "--", name];
                    break;
            }
            // Recheck after occupancy/target reads. Git's own locks and checked-out/merged
            // protections remain authoritative against writers outside the Shovel gate.
            await Revalidate();
            await CheckDirectRef(branch);
            if (options.Action == BranchManagementAction.SetUpstream) await CheckDirectRef(options.Upstream!);
            if (options.Action == BranchManagementAction.Delete) await CheckWorktrees();
            token.ThrowIfCancellationRequested();
            var environment = EnvironmentForGit(); environment["GIT_OPTIONAL_LOCKS"] = null;
            var result = await runner.RunAsync(new(git.Executable, path,
                ["--no-pager", "--no-lazy-fetch", "-c", "core.fsmonitor=false", "-c", "gc.auto=0", "-c", "maintenance.auto=false", .. arguments])
                { Environment = environment, Timeout = TimeSpan.FromMinutes(2), MaximumOutputBytes = 1024 * 1024 }, token, progress);
            return new(result.Outcome, result.ExitCode, Redact(Decode(result) + Encoding.UTF8.GetString(result.StandardError)));

            async Task<RepositorySnapshot> Revalidate()
            {
                token.ThrowIfCancellationRequested();
                if ((await identities.ReadAsync(path, token)).Fingerprint != context.Identity.Fingerprint)
                    throw new GitActionException("ActionIdentityChanged");
                var observed = await reader.InspectAsync(git, path, token);
                if (observed.Operation != RepositoryOperation.None) throw new GitActionException("ActionSequence");
                var refs = await reader.OpenInspectedHistoryAsync(git, observed, HistoryScope.AllCommits, token);
                if (!SamePaths(context.Identity, observed) || observed.Branch != context.Repository.Branch ||
                    observed.Commit != context.Repository.Commit || refs.Fingerprint != context.History.Fingerprint ||
                    await ConfigStamp(git, path, token) != context.ConfigStamp)
                    throw new GitActionException("BranchManageChanged");
                if ((await identities.ReadAsync(path, token)).Fingerprint != context.Identity.Fingerprint)
                    throw new GitActionException("ActionIdentityChanged");
                return observed;
            }
            async Task CheckDirectRef(CommitRef target)
            {
                // for-each-ref's pattern also matches descendants; select the exact full name.
                // Symbolic remote HEAD aliases and symbolic local refs are not concrete targets.
                var data = Decode(await Read(git, path,
                    ["for-each-ref", "--format=%(refname)%00%(objectname)%00%(symref)%00", target.Name], token));
                var rows = data.Split('\n', StringSplitOptions.RemoveEmptyEntries).Select(line => line.TrimEnd('\r').Split('\0')).ToArray();
                var row = rows.SingleOrDefault(fields => fields[0] == target.Name);
                if (row is null || row.Length != 4 || row[1] != target.Hash || row[3].Length != 0)
                    throw new GitActionException("BranchManageChanged");
                if (row[2].Length != 0) throw new GitActionException("BranchManageSymbolicRef");
            }
            async Task CheckWorktrees()
            {
                var data = Decode(await Read(git, path, ["worktree", "list", "--porcelain", "-z"], token));
                if (!data.EndsWith('\0')) throw new GitReadException(GitFailure.InvalidOutput);
                if (data.Split('\0').Any(field => field == "branch " + branch.Name))
                    throw new GitActionException("BranchManageOccupied");
            }
        }
    }, token);

    // Same common-directory name and acquisition semantics as all existing mutations.
    private static Mutex EnterMutationGate(RepositoryIdentity identity)
    {
        var key = Convert.ToHexString(System.Security.Cryptography.SHA256.HashData(Encoding.UTF8.GetBytes(identity.CommonDirectory.ToUpperInvariant())));
        var gate = new Mutex(false, @"Local\Shovel.GitAction." + key);
        bool owned;
        try { owned = gate.WaitOne(0); } catch (AbandonedMutexException) { owned = true; }
        if (owned) return gate;
        gate.Dispose(); throw new GitActionException("ActionOtherInstance");
    }
}
