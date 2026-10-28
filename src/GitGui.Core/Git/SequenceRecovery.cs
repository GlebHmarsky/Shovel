using System.Security.Cryptography;
using System.Text;
using GitGui.Core.Processes;

namespace GitGui.Core.Git;

public enum RecoveryKind { Merge, Rebase, CherryPick, Revert }
public sealed record SequenceRecovery(RecoveryKind Kind, string Stamp, string Target, string CurrentCommit, string Branch)
{
    public bool CanSkip => Kind != RecoveryKind.Merge && CurrentCommit.Length > 0;
    public string Command => Kind switch { RecoveryKind.Merge => "merge", RecoveryKind.Rebase => "rebase", RecoveryKind.CherryPick => "cherry-pick", _ => "revert" };

    // Read only a fixed set of bounded Git state files, never a worktree scan. The stamp
    // deliberately excludes the index: resolving/staging conflicts must not invalidate it.
    internal static SequenceRecovery? Read(string directory, RepositoryOperation operation)
    {
        if (operation == RepositoryOperation.None || operation.HasFlag(RepositoryOperation.ApplyMailbox)) return null;
        string[] names = ["MERGE_HEAD", "MERGE_MSG", "MERGE_AUTOSTASH", "CHERRY_PICK_HEAD", "REVERT_HEAD", "ORIG_HEAD",
            "rebase-merge/head-name", "rebase-merge/orig-head", "rebase-merge/onto", "rebase-merge/stopped-sha",
            "rebase-merge/git-rebase-todo", "rebase-merge/done", "rebase-merge/message", "rebase-merge/autostash",
            "rebase-apply/head-name", "rebase-apply/orig-head", "rebase-apply/onto", "rebase-apply/next", "rebase-apply/last",
            "rebase-apply/original-commit", "rebase-apply/autostash", "sequencer/head", "sequencer/todo", "sequencer/opts", "sequencer/abort-safety"];
        var values = new Dictionary<string, string>();
        using var hash = IncrementalHash.CreateHash(HashAlgorithmName.SHA256);
        foreach (var name in names)
        {
            var path = Path.Combine(directory, name);
            if (!File.Exists(path)) continue;
            using var stream = new FileStream(path, FileMode.Open, FileAccess.Read, FileShare.ReadWrite | FileShare.Delete);
            if (stream.Length > 256 * 1024) return null;
            var bytes = new byte[(int)stream.Length]; stream.ReadExactly(bytes);
            hash.AppendData(Encoding.UTF8.GetBytes(name + "\0")); hash.AppendData(bytes); hash.AppendData([0]);
            values[name] = Encoding.UTF8.GetString(bytes).Trim();
        }
        RecoveryKind kind;
        if (operation.HasFlag(RepositoryOperation.Rebase)) kind = RecoveryKind.Rebase;
        else if (operation == RepositoryOperation.Merge) kind = RecoveryKind.Merge;
        else if (operation.HasFlag(RepositoryOperation.CherryPick) && !operation.HasFlag(RepositoryOperation.Revert)) kind = RecoveryKind.CherryPick;
        else if (operation.HasFlag(RepositoryOperation.Revert) && !operation.HasFlag(RepositoryOperation.CherryPick)) kind = RecoveryKind.Revert;
        else if (operation == RepositoryOperation.Sequencer)
        {
            var todo = values.GetValueOrDefault("sequencer/todo", "").Split('\n')
                .Select(line => line.Trim()).FirstOrDefault(line => line.Length > 0 && !line.StartsWith('#'));
            if (todo?.StartsWith("pick ", StringComparison.Ordinal) == true) kind = RecoveryKind.CherryPick;
            else if (todo?.StartsWith("revert ", StringComparison.Ordinal) == true) kind = RecoveryKind.Revert;
            else return null;
        }
        else return null;
        var target = kind == RecoveryKind.Rebase ? values.GetValueOrDefault("rebase-merge/orig-head") ?? values.GetValueOrDefault("rebase-apply/orig-head")
            : kind == RecoveryKind.Merge ? values.GetValueOrDefault("ORIG_HEAD") : values.GetValueOrDefault("sequencer/head");
        // Only hashes enter target labels; never display arbitrary state-file contents as a target.
        static string HashOrEmpty(string? value) => value is { Length: 40 or 64 } && value.All(Uri.IsHexDigit) ? value : "";
        var current = kind switch
        {
            RecoveryKind.Rebase => values.GetValueOrDefault("rebase-merge/stopped-sha") ?? values.GetValueOrDefault("rebase-apply/original-commit"),
            RecoveryKind.CherryPick => values.GetValueOrDefault("CHERRY_PICK_HEAD"),
            RecoveryKind.Revert => values.GetValueOrDefault("REVERT_HEAD"),
            _ => null
        };
        var branch = values.GetValueOrDefault("rebase-merge/head-name") ?? values.GetValueOrDefault("rebase-apply/head-name") ?? "";
        if (!branch.StartsWith("refs/heads/", StringComparison.Ordinal) || branch.Any(char.IsControl)) branch = "";
        else branch = branch[11..];
        return new(kind, Convert.ToHexString(hash.GetHashAndReset()), HashOrEmpty(target), HashOrEmpty(current), branch);
    }
}

public sealed partial class GitActions
{
    private Task<ActionResult> ExecuteRecoveryAsync(ActionContext context, GitAction action, CancellationToken token,
        Action<ProcessOutput>? progress, Action<GitActionStage>? stage) => Task.Run(() =>
    {
        var key = Convert.ToHexString(SHA256.HashData(Encoding.UTF8.GetBytes(context.Identity.CommonDirectory.ToUpperInvariant())));
        using var gate = new Mutex(false, @"Local\Shovel.GitAction." + key);
        bool owned;
        try { owned = gate.WaitOne(0); } catch (AbandonedMutexException) { owned = true; }
        if (!owned) throw new GitActionException("ActionOtherInstance");
        try { return ExecuteLocked().GetAwaiter().GetResult(); }
        finally { gate.ReleaseMutex(); }

        async Task<ActionResult> ExecuteLocked()
        {
            token.ThrowIfCancellationRequested(); stage?.Invoke(GitActionStage.Validating);
            var expected = context.Repository.Recovery ?? throw new GitActionException("RecoveryUnavailable");
            var git = context.Git; var path = context.Identity.Worktree;
            if ((await identities.ReadAsync(path, token)).Fingerprint != context.Identity.Fingerprint)
                throw new GitActionException("ActionIdentityChanged");
            // No ref enumeration, remote lookup or whole-worktree scan before recovery.
            var fresh = await reader.ReadWorkingCopyHeadAsync(git, context.Repository, token);
            if (fresh.Commit != context.Repository.Commit || fresh.Branch != context.Repository.Branch ||
                fresh.Operation != context.Repository.Operation || fresh.Recovery != expected)
                throw new GitActionException("RecoveryChanged");
            if (action == GitAction.Skip && !expected.CanSkip) throw new GitActionException("RecoveryUnavailable");
            var index = await IndexStampAsync(fresh, token);
            if (action == GitAction.Continue)
            {
                var unmerged = await Read(git, path, ["ls-files", "--unmerged", "-z"], token);
                if (unmerged.StandardOutput.Length != 0) throw new GitActionException("CommitUnresolved");
            }
            if (SequenceRecovery.Read(fresh.GitDirectory, fresh.Operation) != expected || await IndexStampAsync(fresh, token) != index)
                throw new GitActionException("RecoveryChanged");
            var environment = EnvironmentForGit(); environment["GIT_OPTIONAL_LOCKS"] = null;
            // Continue retains Git's existing message. Never launch an invisible message/todo editor.
            // Configured hooks/signing and an existing rebase's exec commands remain Git-owned.
            environment["GIT_EDITOR"] = "true"; environment["GIT_SEQUENCE_EDITOR"] = "false";
            environment["GIT_MERGE_AUTOEDIT"] = "no";
            stage?.Invoke(GitActionStage.Recovering);
            var result = await runner.RunAsync(new(git.Executable, path,
                ["--no-pager", "--no-lazy-fetch", "-c", "core.fsmonitor=false", "-c", "gc.auto=0", "-c", "maintenance.auto=false",
                    "-c", "submodule.recurse=false", expected.Command, "--" + action.ToString().ToLowerInvariant()])
                { Environment = environment, Timeout = TimeSpan.FromMinutes(5), MaximumOutputBytes = 1024 * 1024 }, token, progress);
            var output = Redact(Decode(result) + Encoding.UTF8.GetString(result.StandardError));
            // Inspect even after cancellation/nonzero exit; stopping children never rolls back Git.
            using var inspect = new CancellationTokenSource(TimeSpan.FromSeconds(20));
            try
            {
                var after = await reader.ReadWorkingCopyHeadAsync(git, fresh, inspect.Token);
                var conflicts = await Read(git, path, ["ls-files", "--unmerged", "-z"], inspect.Token);
                return new(result.Outcome, result.ExitCode, output) { Operation = after.Operation, HasConflicts = conflicts.StandardOutput.Length > 0 };
            }
            catch (Exception error) when (error is GitReadException or IOException or UnauthorizedAccessException or OperationCanceledException)
            {
                // Unknown post-command state must not be reported as clean completion.
                return new(result.Outcome, result.ExitCode, output) { Operation = fresh.Operation };
            }
        }
    }, token);
}
