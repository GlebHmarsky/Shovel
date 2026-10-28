using System.Security.Cryptography;
using System.Text;
using GitGui.Core.Processes;

namespace GitGui.Core.Git;

public sealed record WorkingCopyContext(GitInstallation Git, RepositoryIdentity Identity, RepositorySnapshot Repository,
    LocalChanges Changes, string IndexStamp, string ConfigStamp, string HeadMessage, string MergeMessage);
public enum WorkingCopyAction { Stage, Unstage, Commit }
public sealed record WorkingCopyOptions(WorkingCopyAction Action, IReadOnlyList<LocalChange> Entries, string Message = "", bool Amend = false);
public sealed record WorkingCopyResult(ActionResult Result, string? CommitHash = null);

public sealed partial class GitActions
{
    public Task<WorkingCopyContext> ReadWorkingCopyAsync(GitInstallation git, string path, CancellationToken token) => Task.Run(async () =>
    {
        var identity = await identities.ReadAsync(path, token);
        var repository = await reader.InspectAsync(git, path, token);
        if (!SamePaths(identity, repository)) throw new GitActionException("ActionIdentityChanged");
        var stamp = await IndexStampAsync(repository, token);
        var changes = await reader.ReadLocalChangesAsync(git, path, false, token);
        var messageResult = repository.Commit is null ? null : await Raw(git, path,
            ["-c", "log.showSignature=false", "log", "-1", "--no-show-signature", "--no-notes", "--format=%B", repository.Commit, "--"], token);
        var message = messageResult is { Outcome: ProcessOutcome.Exited, ExitCode: 0 } ? Decode(messageResult).TrimEnd('\r', '\n') : "";
        if (Encoding.UTF8.GetByteCount(message) > 65536) message = "";
        var mergeMessage = "";
        if (repository.Operation == RepositoryOperation.Merge)
        {
            var mergePath = Path.Combine(repository.GitDirectory, "MERGE_MSG");
            if (File.Exists(mergePath) && new FileInfo(mergePath).Length <= 65536)
                mergeMessage = await File.ReadAllTextAsync(mergePath, token);
        }
        if (stamp != await IndexStampAsync(repository, token) || (await identities.ReadAsync(path, token)).Fingerprint != identity.Fingerprint)
            throw new GitActionException("CommitStale");
        return new WorkingCopyContext(git, identity, repository, changes, stamp, await ConfigStamp(git, path, token), message, mergeMessage);
    }, token);

    // Content hash also catches index-only edits with unchanged file names/status codes. Split-index
    // shared files are immutable by name; the worktree index contains their reference. No index writes.
    public static async Task<string> IndexStampAsync(RepositorySnapshot repository, CancellationToken token)
    {
        var file = Path.Combine(repository.GitDirectory, "index");
        try
        {
            await using var stream = new FileStream(file, FileMode.Open, FileAccess.Read, FileShare.ReadWrite | FileShare.Delete,
                65536, FileOptions.Asynchronous | FileOptions.SequentialScan);
            return Convert.ToHexString(await SHA256.HashDataAsync(stream, token));
        }
        catch (FileNotFoundException) { return "missing"; }
    }

    public Task<WorkingCopyResult> ExecuteWorkingCopyAsync(WorkingCopyContext context, WorkingCopyOptions options,
        CancellationToken token, Action<ProcessOutput>? progress = null) => Task.Run(() =>
    {
        // Same common-directory key as branch/network actions; hold ownership on one worker thread.
        var key = Convert.ToHexString(SHA256.HashData(Encoding.UTF8.GetBytes(context.Identity.CommonDirectory.ToUpperInvariant())));
        using var gate = new Mutex(false, @"Local\Shovel.GitAction." + key);
        bool owned;
        try { owned = gate.WaitOne(0); } catch (AbandonedMutexException) { owned = true; }
        if (!owned) throw new GitActionException("ActionOtherInstance");
        try { return ExecuteLocked().GetAwaiter().GetResult(); }
        finally { gate.ReleaseMutex(); }

        async Task<WorkingCopyResult> ExecuteLocked()
        {
            token.ThrowIfCancellationRequested();
            var git = context.Git; var path = context.Identity.Worktree;
            if ((await identities.ReadAsync(path, token)).Fingerprint != context.Identity.Fingerprint) throw new GitActionException("ActionIdentityChanged");
            var fresh = await reader.InspectAsync(git, path, token);
            if (!SamePaths(context.Identity, fresh) || fresh.Branch != context.Repository.Branch || fresh.Commit != context.Repository.Commit ||
                fresh.Operation != context.Repository.Operation || await IndexStampAsync(fresh, token) != context.IndexStamp ||
                await ConfigStamp(git, path, token) != context.ConfigStamp) throw new GitActionException("CommitStale");
            if (!Enum.IsDefined(options.Action)) throw new GitActionException("ActionReadFailed");
            var temporary = Directory.CreateTempSubdirectory("shovel-git-");
            try
            {
                string[] args;
                if (options.Action == WorkingCopyAction.Commit)
                {
                    if (string.IsNullOrWhiteSpace(options.Message) || Encoding.UTF8.GetByteCount(options.Message) > 65536 || options.Message.Contains('\0'))
                        throw new GitActionException("CommitMessageRequired");
                    if (fresh.Operation is not (RepositoryOperation.None or RepositoryOperation.Merge) ||
                        options.Amend && (fresh.Commit is null || fresh.Operation != RepositoryOperation.None)) throw new GitActionException("ActionSequence");
                    var unmerged = await Read(git, path, ["ls-files", "--unmerged", "-z"], token);
                    if (unmerged.StandardOutput.Length != 0) throw new GitActionException("CommitUnresolved");
                    var messageFile = Path.Combine(temporary.FullName, "message.txt");
                    await File.WriteAllTextAsync(messageFile, options.Message, new UTF8Encoding(false), token);
                    args = ["-c", "i18n.commitEncoding=UTF-8", "commit", "--cleanup=strip", "--file=" + messageFile, .. options.Amend ? new[] { "--amend" } : []];
                }
                else
                {
                    if (options.Entries.Count == 0 || options.Entries.Any(entry => !context.Changes.Entries.Contains(entry) ||
                        (options.Action == WorkingCopyAction.Unstage ? entry.Group != ChangeGroup.Staged : entry.Group is ChangeGroup.Staged or ChangeGroup.Ignored)))
                        throw new GitActionException("CommitStale");
                    var paths = options.Entries.SelectMany(entry => entry.OriginalPath is { } old &&
                        (options.Action == WorkingCopyAction.Unstage || entry.Code == 'R') ? new[] { entry.Path, old } : [entry.Path]).Distinct(StringComparer.Ordinal).ToArray();
                    foreach (var relative in paths) RepositoryReader.WorkingCopyPath(path, relative);
                    var pathFile = Path.Combine(temporary.FullName, "paths.nul");
                    await File.WriteAllTextAsync(pathFile, string.Join('\0', paths) + "\0", new UTF8Encoding(false), token);
                    var pathArgs = new[] { "--pathspec-from-file=" + pathFile, "--pathspec-file-nul" };
                    args = options.Action == WorkingCopyAction.Stage ? ["add", "--all", .. pathArgs] : fresh.Commit is null
                        // Unborn index removal, including partially staged content. --cached never removes worktree files.
                        ? ["rm", "--cached", "--ignore-unmatch", "-r", "-f", .. pathArgs]
                        : ["restore", "--staged", "--source=" + fresh.Commit, .. pathArgs];
                }
                var environment = EnvironmentForGit(); environment["GIT_OPTIONAL_LOCKS"] = null;
                // Supplied message removes the editor dependency; hooks/signing are still Git's configured tools.
                // Terminal stdin is closed by the runner. Visible progress/cancel/timeout cover owned work.
                environment["GIT_EDITOR"] = "false";
                var result = await runner.RunAsync(new(git.Executable, path,
                    ["--no-pager", "--no-lazy-fetch", "--literal-pathspecs", "-c", "core.fsmonitor=false", "-c", "gc.auto=0", "-c", "maintenance.auto=false", .. args])
                    { Environment = environment, Timeout = TimeSpan.FromMinutes(5), MaximumOutputBytes = 1024 * 1024 }, token, progress);
                var action = new ActionResult(result.Outcome, result.ExitCode, Redact(Decode(result) + Encoding.UTF8.GetString(result.StandardError)));
                string? commit = null;
                if (action.Success && options.Action == WorkingCopyAction.Commit)
                {
                    using var inspect = new CancellationTokenSource(TimeSpan.FromSeconds(20));
                    // A successful Git exit remains successful even if subsequent metadata cannot be read.
                    // The caller clears the draft and never retries the commit or pushes an unknown tip.
                    try { commit = (await reader.InspectAsync(git, path, inspect.Token)).Commit; }
                    catch (Exception error) when (error is GitReadException or IOException or UnauthorizedAccessException or OperationCanceledException) { }
                }
                return new WorkingCopyResult(action, commit);
            }
            finally
            {
                // Only this call's two known temporary files; never recursively clean a computed repository path.
                foreach (var name in new[] { "paths.nul", "message.txt" })
                    try { File.Delete(Path.Combine(temporary.FullName, name)); } catch (IOException) { } catch (UnauthorizedAccessException) { }
                try { temporary.Delete(); } catch (IOException) { } catch (UnauthorizedAccessException) { }
            }
        }
    }, token);
}
