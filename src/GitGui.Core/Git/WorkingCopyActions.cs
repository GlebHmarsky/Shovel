using System.Security.Cryptography;
using System.Diagnostics;
using System.Text;
using GitGui.Core.Processes;

namespace GitGui.Core.Git;

public sealed record WorkingCopyContext(GitInstallation Git, RepositoryIdentity Identity, RepositorySnapshot Repository,
    LocalChanges Changes, string IndexStamp, string ConfigStamp, string HeadMessage, string MergeMessage)
{
    internal StatusFilterOptions? FilterOverrides { get; init; }
}
public enum WorkingCopyAction { Stage, Unstage, Commit, StageHunks, UnstageHunks, Discard, StashFiles, IgnoreFiles, IgnoreExtension, LfsTrack, LfsUntrack, ExternalDiff, ResolveConflict }
public sealed record WorkingCopyOptions(WorkingCopyAction Action, IReadOnlyList<LocalChange> Entries, string Message = "", bool Amend = false,
    HunkSelection? Hunks = null, ConflictResolution? Resolution = null);
public sealed record WorkingCopyResult(ActionResult Result, string? CommitHash = null)
{
    public double ValidationMilliseconds { get; init; }
    public double CommandMilliseconds { get; init; }
    public double RefreshMilliseconds { get; init; }
    public WorkingCopyContext? UpdatedContext { get; init; }
}

public sealed partial class GitActions
{
    public Task<WorkingCopyContext> ReadWorkingCopyAsync(GitInstallation git, string path, CancellationToken token) => Task.Run(async () =>
    {
        var identity = await identities.ReadAsync(path, token);
        var repository = await reader.InspectAsync(git, path, token, includeHistory: false);
        if (!SamePaths(identity, repository)) throw new GitActionException("ActionIdentityChanged");
        var stamp = await IndexStampAsync(repository, token);
        var filterOverrides = await reader.StatusFiltersAsync(git, path, token);
        var changesTask = reader.ReadLocalChangesAsync(git, path, false, token, filterOverrides: filterOverrides);
        var configTask = ConfigStamp(git, path, token);
        var messageTask = repository.Commit is null ? Task.FromResult<ProcessResult?>(null) : ReadMessage();
        await Task.WhenAll(changesTask, configTask, messageTask);
        var changes = await changesTask; var messageResult = await messageTask;
        async Task<ProcessResult?> ReadMessage() => await Raw(git, path,
            ["-c", "log.showSignature=false", "log", "-1", "--no-show-signature", "--no-notes", "--format=%B", repository.Commit!, "--"], token);
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
        return new WorkingCopyContext(git, identity, repository, changes, stamp, await configTask, message, mergeMessage) { FilterOverrides = filterOverrides };
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
            var timing = Stopwatch.StartNew();
            token.ThrowIfCancellationRequested();
            var git = context.Git; var path = context.Identity.Worktree;
            if ((await identities.ReadAsync(path, token)).Fingerprint != context.Identity.Fingerprint) throw new GitActionException("ActionIdentityChanged");
            var inspection = options.Action == WorkingCopyAction.Commit ? reader.InspectAsync(git, path, token, includeHistory: false)
                : reader.ReadWorkingCopyHeadAsync(git, context.Repository, token);
            var configuration = ConfigStamp(git, path, token);
            Task<LocalChanges>? reviewedPaths = null;
            if (options.Action == WorkingCopyAction.Discard && options.Entries.Count > 0 && options.Entries.All(context.Changes.Entries.Contains))
            {
                var targets = FileActionPaths(options.Entries);
                foreach (var target in targets) ValidateRegularTarget(path, target);
                reviewedPaths = ReadDiscardChanges();
                async Task<LocalChanges> ReadDiscardChanges() =>
                    await reader.TryReadLfsDiscardChangesAsync(context, options.Entries, token) ??
                    await reader.ReadLocalChangesAsync(git, path, false, token,
                        targets.Length <= 128 && targets.Sum(item => item.Length + 3) < 16000 ? targets : null, context.FilterOverrides, normalizeLfs: false);
            }
            await Task.WhenAll(inspection, configuration, reviewedPaths ?? Task.CompletedTask);
            var fresh = await inspection;
            if (!SamePaths(context.Identity, fresh) || fresh.Branch != context.Repository.Branch || fresh.Commit != context.Repository.Commit ||
                fresh.Operation != context.Repository.Operation || await IndexStampAsync(fresh, token) != context.IndexStamp ||
                await configuration != context.ConfigStamp) throw new GitActionException("CommitStale");
            if (!Enum.IsDefined(options.Action)) throw new GitActionException("ActionReadFailed");
            if (options.Action == WorkingCopyAction.ResolveConflict)
                return await ResolveConflictLockedAsync(context, fresh, options.Resolution ?? throw new GitActionException("ConflictChoose"), timing, token, progress);
            if (options.Action >= WorkingCopyAction.Discard)
                return await ExecuteFileMenuLockedAsync(context, fresh, options, timing.Elapsed.TotalMilliseconds, token, progress, reviewedPaths is null ? null : await reviewedPaths);
            var temporary = Directory.CreateTempSubdirectory("shovel-git-");
            try
            {
                string[] args;
                string[]? affectedPaths = null;
                if (options.Action is WorkingCopyAction.StageHunks or WorkingCopyAction.UnstageHunks)
                {
                    var selection = options.Hunks ?? throw new GitActionException("HunkSelectChange");
                    var preview = selection.Preview;
                    if (preview.Context != context || options.Entries.Count != 1 || options.Entries[0] != preview.Change ||
                        preview.Change.Group != (options.Action == WorkingCopyAction.StageHunks ? ChangeGroup.Unstaged : ChangeGroup.Staged))
                        throw new GitActionException("HunkStale");
                    var current = await reader.ReadFileDiffAsync(context, preview.Change, new(), token);
                    if (current.HunkPreview is not { } verified || verified.RawPatch != preview.RawPatch ||
                        verified.WorktreeStamp != preview.WorktreeStamp || verified.AttributeStamp != preview.AttributeStamp)
                        throw new GitActionException("HunkStale");
                    var patchFile = Path.Combine(temporary.FullName, "hunks.patch");
                    await File.WriteAllTextAsync(patchFile, verified.Select(selection.HunkIds), new UTF8Encoding(false), token);
                    string[] apply = ["-c", "apply.ignoreWhitespace=false", "apply", "--cached", "--whitespace=nowarn",
                        .. options.Action == WorkingCopyAction.UnstageHunks ? new[] { "--reverse" } : [], "--", patchFile];
                    // Check first, then revalidate immediately before the real cached-only write.
                    // No --reject/--3way, worktree checkout, whitespace fixing or automatic fallback.
                    var check = await Raw(git, path, ["-c", "apply.ignoreWhitespace=false", "apply", "--check", "--cached", "--whitespace=nowarn",
                        .. options.Action == WorkingCopyAction.UnstageHunks ? new[] { "--reverse" } : [], "--", patchFile], token);
                    if (check.Outcome != ProcessOutcome.Exited || check.ExitCode != 0)
                        return new(new(check.Outcome, check.ExitCode, Redact(Decode(check) + Encoding.UTF8.GetString(check.StandardError))));
                    if (await IndexStampAsync(fresh, token) != context.IndexStamp ||
                        await RepositoryReader.WorktreeContentStampAsync(path, preview.Change.Path, token) != preview.WorktreeStamp ||
                        await ConfigStamp(git, path, token) != context.ConfigStamp ||
                        (await reader.ReadHunkAttributesAsync(git, path, preview.Change.Path, token)).Stamp != preview.AttributeStamp)
                        throw new GitActionException("HunkStale");
                    var currentRepository = await reader.InspectAsync(git, path, token, includeHistory: false);
                    if (currentRepository.Commit != fresh.Commit || currentRepository.Branch != fresh.Branch || currentRepository.Operation != fresh.Operation ||
                        (await identities.ReadAsync(path, token)).Fingerprint != context.Identity.Fingerprint) throw new GitActionException("HunkStale");
                    args = apply;
                }
                else if (options.Action == WorkingCopyAction.Commit)
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
                    if (paths.Length <= 128 && paths.Sum(item => item.Length + 3) < 16000) affectedPaths = paths;
                    foreach (var relative in paths) RepositoryReader.WorkingCopyPath(path, relative);
                    var pathFile = Path.Combine(temporary.FullName, "paths.nul");
                    await File.WriteAllTextAsync(pathFile, string.Join('\0', paths) + "\0", new UTF8Encoding(false), token);
                    var pathArgs = new[] { "--pathspec-from-file=" + pathFile, "--pathspec-file-nul" };
                    args = options.Action == WorkingCopyAction.Stage ? ["add", "--all", .. pathArgs] : fresh.Commit is null
                        // Unborn index removal, including partially staged content. --cached never removes worktree files.
                        ? ["rm", "--cached", "--ignore-unmatch", "-r", "-f", .. pathArgs]
                        : ["reset", "--quiet", "--no-refresh", fresh.Commit, .. pathArgs];
                }
                var environment = EnvironmentForGit(); environment["GIT_OPTIONAL_LOCKS"] = null;
                // Supplied message removes the editor dependency; hooks/signing are still Git's configured tools.
                // Terminal stdin is closed by the runner. Visible progress/cancel/timeout cover owned work.
                environment["GIT_EDITOR"] = "false";
                var validationMilliseconds = timing.Elapsed.TotalMilliseconds;
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
                WorkingCopyContext? updated = null; var refresh = Stopwatch.StartNew();
                if (action.Success && affectedPaths is not null)
                {
                    // Publish actual selected-path status first. A full background scan can be
                    // expensive, especially for asset repositories with invalidated stat caches.
                    // Failure here must not turn a successful index write into a failed command.
                    using var reconcile = new CancellationTokenSource(TimeSpan.FromSeconds(20));
                    try
                    {
                        var stamp = await IndexStampAsync(fresh, reconcile.Token);
                        var changed = await reader.ReadLocalChangesAsync(git, path, false, reconcile.Token, affectedPaths, context.FilterOverrides);
                        if (stamp == await IndexStampAsync(fresh, reconcile.Token))
                        {
                            var selected = affectedPaths.ToHashSet(StringComparer.Ordinal);
                            var entries = context.Changes.Entries.Where(entry => !selected.Contains(entry.Path) &&
                                (entry.OriginalPath is null || !selected.Contains(entry.OriginalPath))).Concat(changed.Entries)
                                .OrderBy(entry => entry.Group).ThenBy(entry => entry.Path, StringComparer.Ordinal).ToArray();
                            updated = context with { Repository = fresh, IndexStamp = stamp, Changes = new(entries, changed.ReadAt, false) };
                        }
                    }
                    catch (Exception error) when (error is GitReadException or GitActionException or IOException or UnauthorizedAccessException or OperationCanceledException) { }
                }
                return new WorkingCopyResult(action, commit) { ValidationMilliseconds = validationMilliseconds, CommandMilliseconds = result.Duration.TotalMilliseconds,
                    UpdatedContext = updated, RefreshMilliseconds = refresh.Elapsed.TotalMilliseconds };
            }
            finally
            {
                // Only this call's known temporary files; never recursively clean a computed repository path.
                foreach (var name in new[] { "paths.nul", "message.txt", "hunks.patch" })
                    try { File.Delete(Path.Combine(temporary.FullName, name)); } catch (IOException) { } catch (UnauthorizedAccessException) { }
                try { temporary.Delete(); } catch (IOException) { } catch (UnauthorizedAccessException) { }
            }
        }
    }, token);
}
