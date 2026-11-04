using System.Security.Cryptography;
using System.Text;
using GitGui.Core.Processes;

namespace GitGui.Core.Git;

public sealed record HistoricalTreeEntry(string Path, string Mode, string Hash, bool IsDirectory);
public sealed record HistoricalFileReview(WorkingCopyContext Context, string Revision, string Path, string WorktreeStamp, string AttributeStamp);
public enum HistoricalFileMutation { Restore, LfsTrack, LfsUntrack }

public sealed partial class GitActions
{
    public const int MaximumHistoricalFileBytes = ProcessRequest.MaximumCaptureBytes;
    public async Task ValidateHistoricalIdentityAsync(string root, string expected, CancellationToken token)
    { if ((await identities.ReadAsync(root, token)).Fingerprint != expected) throw new GitActionException("ActionIdentityChanged"); }
    public Task<IReadOnlyList<HistoricalTreeEntry>> ReadHistoricalDirectoryAsync(GitInstallation git, string root, string revision, string directory, CancellationToken token) => Task.Run(async () =>
    {
        if (!ObjectId(revision)) throw new GitActionException("CommitStale");
        if (directory.Length > 0) RepositoryReader.WorkingCopyPath(root, directory);
        var result = await Read(git, root, ["ls-tree", "-z", directory.Length == 0 ? revision : revision + ":" + directory], token);
        return (IReadOnlyList<HistoricalTreeEntry>)Decode(result).Split('\0', StringSplitOptions.RemoveEmptyEntries).Select(record =>
        {
            var split = record.IndexOf('\t'); if (split < 0) throw new GitReadException(GitFailure.InvalidOutput);
            var fields = record[..split].Split(' ');
            if (fields.Length != 3 || !ObjectId(fields[2])) throw new GitReadException(GitFailure.InvalidOutput);
            var name = directory.Length == 0 ? record[(split + 1)..] : directory + "/" + record[(split + 1)..];
            RepositoryReader.WorkingCopyPath(root, name);
            return new HistoricalTreeEntry(name, fields[0], fields[2], fields[1] == "tree");
        }).OrderByDescending(entry => entry.IsDirectory).ThenBy(entry => entry.Path, StringComparer.Ordinal).ToArray();
    }, token);

    public Task<byte[]> ReadHistoricalContentAsync(GitInstallation git, string root, string revision, string relative, CancellationToken token, Action<ProcessOutput>? progress = null) => Task.Run(async () =>
    {
        var entry = await HistoricalEntry(git, root, revision, relative, token);
        if (entry is null) throw new GitActionException("HistoryFileAbsent");
        if (entry.Mode is not ("100644" or "100755")) throw new GitActionException("FileActionUnsupported");
        // Native Git conversions include configured LFS/smudge and working-tree encoding.
        var run = await RunLoggedAsync(new(git.Executable, root, ["--no-pager", "--no-lazy-fetch", "cat-file", "--filters", "--path=" + relative, entry.Hash])
            { Environment = EnvironmentForGit(), MaximumOutputBytes = MaximumHistoricalFileBytes, Timeout = TimeSpan.FromMinutes(5) }, token, progress);
        if (run.Outcome != ProcessOutcome.Exited || run.ExitCode != 0) throw ReadFailure(run);
        return run.StandardOutput;
    }, token);

    private async Task<HistoricalTreeEntry?> HistoricalEntry(GitInstallation git, string root, string revision, string relative, CancellationToken token)
    {
        if (!ObjectId(revision)) throw new GitActionException("CommitStale");
        RepositoryReader.WorkingCopyPath(root, relative);
        var raw = Decode(await Read(git, root, ["--literal-pathspecs", "ls-tree", "-z", revision, "--", relative], token));
        if (raw.Length == 0) return null;
        var record = raw.TrimEnd('\0'); var split = record.IndexOf('\t');
        if (split < 0) throw new GitReadException(GitFailure.InvalidOutput);
        var fields = record[..split].Split(' ');
        if (fields.Length != 3 || record[(split + 1)..] != relative || !ObjectId(fields[2])) throw new GitReadException(GitFailure.InvalidOutput);
        return new(relative, fields[0], fields[2], fields[1] == "tree");
    }

    public async Task<WorkingCopyContext> ReadHistoricalInspectionAsync(GitInstallation git, string root, string revision, string relative, CancellationToken token)
    {
        if (!ObjectId(revision)) throw new GitActionException("CommitStale");
        var context = await ReadWorkingCopyAsync(git, root, token);
        var file = new LocalChange(ChangeGroup.Staged, relative, 'M');
        return context with { Repository = context.Repository with { Commit = revision }, Changes = new([file], DateTimeOffset.UtcNow, false) };
    }

    public async Task<HistoricalFileReview> ReviewHistoricalFileAsync(GitInstallation git, string root, string? revision, string relative, CancellationToken token)
    {
        var context = await ReadWorkingCopyAsync(git, root, token);
        revision ??= context.Repository.Commit ?? throw new GitActionException("FileBlameNoHead");
        if (!ObjectId(revision)) throw new GitActionException("CommitStale");
        ValidateRegularTarget(root, relative);
        var stamp = await RepositoryReader.WorktreeContentStampAsync(root, relative, token);
        var attributes = Decode(await Read(git, root, ["--literal-pathspecs", "check-attr", "-z", "--all", "--", relative], token));
        return new(context, revision, relative, stamp, attributes);
    }

    public Task<ActionResult> ExecuteHistoricalFileAsync(HistoricalFileReview review, HistoricalFileMutation mutation, CancellationToken token, Action<ProcessOutput>? progress = null) => Task.Run(() =>
    {
        var context = review.Context; var root = context.Identity.Worktree; var git = context.Git;
        var key = Convert.ToHexString(SHA256.HashData(Encoding.UTF8.GetBytes(context.Identity.CommonDirectory.ToUpperInvariant())));
        using var gate = new Mutex(false, @"Local\Shovel.GitAction." + key);
        bool owned; try { owned = gate.WaitOne(0); } catch (AbandonedMutexException) { owned = true; }
        if (!owned) throw new GitActionException("ActionOtherInstance");
        try { return Execute().GetAwaiter().GetResult(); } finally { gate.ReleaseMutex(); }
        async Task<ActionResult> Execute()
        {
            token.ThrowIfCancellationRequested(); ValidateRegularTarget(root, review.Path);
            var fresh = await reader.ReadWorkingCopyHeadAsync(git, context.Repository, token);
            if ((await identities.ReadAsync(root, token)).Fingerprint != context.Identity.Fingerprint || fresh.Commit != context.Repository.Commit ||
                fresh.Branch != context.Repository.Branch || fresh.Operation != RepositoryOperation.None ||
                await IndexStampAsync(fresh, token) != context.IndexStamp || await ConfigStamp(git, root, token) != context.ConfigStamp ||
                await RepositoryReader.WorktreeContentStampAsync(root, review.Path, token) != review.WorktreeStamp ||
                Decode(await Read(git, root, ["--literal-pathspecs", "check-attr", "-z", "--all", "--", review.Path], token)) != review.AttributeStamp)
                throw new GitActionException("CommitStale");
            if (mutation == HistoricalFileMutation.Restore)
            {
                var source = await HistoricalEntry(git, root, review.Revision, review.Path, token);
                if (await RepositoryReader.WorktreeContentStampAsync(root, review.Path, token) != review.WorktreeStamp ||
                    await IndexStampAsync(fresh, token) != context.IndexStamp) throw new GitActionException("CommitStale");
                if (source is null)
                {
                    File.Delete(RepositoryReader.WorkingCopyPath(root, review.Path));
                    return new ActionResult(ProcessOutcome.Exited, 0, "");
                }
                if (source.Mode is not ("100644" or "100755")) throw new GitActionException("FileActionUnsupported");
                var environment = EnvironmentForGit(); environment["GIT_OPTIONAL_LOCKS"] = null;
                var result = await RunLoggedAsync(new(git.Executable, root, ["--no-pager", "--literal-pathspecs", "-c", "submodule.recurse=false", "restore", "--worktree", "--no-recurse-submodules", "--source=" + review.Revision, "--", review.Path])
                    { Environment = environment, Timeout = TimeSpan.FromMinutes(5), MaximumOutputBytes = 1024 * 1024 }, token, progress);
                return new ActionResult(result.Outcome, result.ExitCode, Redact(Decode(result) + Encoding.UTF8.GetString(result.StandardError)));
            }
            if (mutation is not (HistoricalFileMutation.LfsTrack or HistoricalFileMutation.LfsUntrack)) throw new GitActionException("ActionReadFailed");
            ValidateRegularTarget(root, ".gitattributes");
            if (mutation == HistoricalFileMutation.LfsTrack)
            {
                var version = await Raw(git, root, ["lfs", "version"], token);
                if (version.ExitCode != 0) throw new GitActionException("FileLfsMissing");
                var filter = await Raw(git, root, ["config", "--get-regexp", @"^filter\.lfs\.(clean|process)$"], token);
                if (filter.ExitCode != 0 || filter.StandardOutput.Length == 0) throw new GitActionException("FileLfsSetup");
            }
            var suffix = mutation == HistoricalFileMutation.LfsTrack ? " filter=lfs diff=lfs merge=lfs -text" : " !filter !diff !merge !text";
            await AppendRulesAsync(root, ".gitattributes", [AttributePattern(review.Path) + suffix], token, alwaysAppend: true);
            var effective = Decode(await Read(git, root, ["--literal-pathspecs", "check-attr", "-z", "filter", "--", review.Path], token)).Split('\0');
            return new ActionResult(ProcessOutcome.Exited, 0, "") { MessageCode = effective.Length >= 3 && (effective[2] == "lfs") != (mutation == HistoricalFileMutation.LfsTrack) ? "FileLfsInherited" : null };
        }
    }, token);

    public Task<ActionResult> DiffHistoricalFileAsync(GitInstallation git, string root, string revision, string? parent, LocalChange file, CancellationToken token, Action<ProcessOutput>? progress = null) => Task.Run(async () =>
    {
        if (!ObjectId(revision) || parent is not null && !ObjectId(parent)) throw new GitActionException("CommitStale");
        string? tool = null;
        foreach (var key in new[] { "diff.guitool", "merge.guitool", "diff.tool", "merge.tool" })
        { var configured = await Raw(git, root, ["config", "--get", key], token); if (configured.ExitCode == 0) { tool = Decode(configured).Trim(); break; } }
        if (string.IsNullOrWhiteSpace(tool) || tool.Any(char.IsControl)) throw new GitActionException("FileDiffToolMissing");
        if (parent is null)
        {
            // Git computes its repository's empty-tree ID (works for SHA-1/SHA-256).
            parent = Decode(await Read(git, root, ["hash-object", "-w", "-t", "tree", "--stdin"], token)).Trim();
        }
        var paths = FileActionPaths([file]); foreach (var relative in paths) RepositoryReader.WorkingCopyPath(root, relative);
        var result = await RunLoggedAsync(new(git.Executable, root, ["--no-pager", "--literal-pathspecs", "difftool", "--no-prompt", "--trust-exit-code", "--tool=" + tool, "--no-textconv", parent, revision, "--", .. paths])
            { Environment = EnvironmentForGit(), Timeout = TimeSpan.FromMinutes(10), MaximumOutputBytes = 1024 * 1024 }, token, progress);
        return new ActionResult(result.Outcome, result.ExitCode, Redact(Decode(result) + Encoding.UTF8.GetString(result.StandardError)));
    }, token);
}
