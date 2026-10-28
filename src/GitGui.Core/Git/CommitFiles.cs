using System.Globalization;
using System.Text;

namespace GitGui.Core.Git;

public sealed record CommitFiles(string Commit, string? Parent, IReadOnlyList<LocalChange> Files);

public sealed partial class RepositoryReader
{
    // Immutable object IDs only; never use HEAD or read the index/worktree for this view.
    public Task<CommitFiles> ReadCommitFilesAsync(GitInstallation git, string path, string hash, string? parent,
        CancellationToken token) => Task.Run(async () =>
    {
        ValidateComparison(hash, parent);
        if (parent is null)
        {
            var objectData = await RunAsync(git.Executable, path, ReadArguments(["cat-file", "-p", hash]), token, MaximumDiffBytes);
            RequireSuccess(objectData);
            var header = Text(objectData.StandardOutput).Split("\n\n", 2)[0];
            if (header.Split('\n').Any(line => line.StartsWith("parent ", StringComparison.Ordinal)))
                throw new GitReadException(GitFailure.IncompleteHistory);
        }
        var result = await RunAsync(git.Executable, path, ReadArguments(["diff-tree", "--root", "--no-commit-id", "-r", "--raw", "-z", "--no-abbrev",
            "--find-renames", "--no-ext-diff", "--no-textconv", "--ignore-submodules=dirty", .. parent is null ? new[] { hash } : [parent, hash], "--"]), token, MaximumStatusBytes);
        RequireSuccess(result);
        string text;
        try { text = new UTF8Encoding(false, true).GetString(result.StandardOutput); }
        catch (DecoderFallbackException) { throw new GitReadException(GitFailure.InvalidOutput); }
        var records = text.Split('\0'); var files = new List<LocalChange>();
        if (records[^1] != "") throw new GitReadException(GitFailure.InvalidOutput);
        for (var i = 0; i < records.Length - 1; i++)
        {
            token.ThrowIfCancellationRequested();
            var fields = records[i].Split(' ');
            if (fields.Length != 5 || !fields[0].StartsWith(':') || fields[0].Length != 7 || fields[1].Length != 6 ||
                !IsObjectId(fields[2]) || !IsObjectId(fields[3]) || fields[4].Length == 0 || !"MADRCT".Contains(fields[4][0]) || ++i >= records.Length - 1)
                throw new GitReadException(GitFailure.InvalidOutput);
            var name = records[i]; string? old = null;
            if (fields[4][0] is 'R' or 'C') { old = name; if (++i >= records.Length - 1) throw new GitReadException(GitFailure.InvalidOutput); name = records[i]; }
            if (name.Length == 0 || files.Count == MaximumStatusEntries) throw new GitReadException(GitFailure.OutputLimit);
            files.Add(new(ChangeGroup.Staged, name, fields[4][0], old)
            { HeadMode = fields[0][1..], IndexMode = fields[1], HeadObject = fields[2], IndexObject = fields[3] });
        }
        return new CommitFiles(hash, parent, files);
    }, token);

    public Task<FileDiff> ReadCommitFileDiffAsync(GitInstallation git, string path, CommitFiles comparison, LocalChange file,
        CancellationToken token) => Task.Run(async () =>
    {
        ValidateComparison(comparison.Commit, comparison.Parent);
        if (!comparison.Files.Contains(file)) throw new GitReadException(GitFailure.InvalidOutput);
        var paths = file.OriginalPath is { } old ? new[] { old, file.Path } : [file.Path];
        string[] args = ["--literal-pathspecs", "diff-tree", "--root", "--no-commit-id", "-r", "--find-renames", "--no-ext-diff", "--no-textconv", "--no-color",
            "--submodule=short", "--ignore-submodules=dirty", "--src-prefix=a/", "--dst-prefix=b/", "--output-indicator-new=+", "--output-indicator-old=-", "--output-indicator-context= "];
        var revisions = comparison.Parent is { } parent ? new[] { parent, comparison.Commit } : [comparison.Commit];
        var stat = await RunAsync(git.Executable, path, ReadArguments([.. args, "--numstat", "-z", .. revisions, "--", .. paths]), token, MaximumDiffBytes);
        RequireSuccess(stat); var (binary, added, removed) = ParseDiffStats(Text(stat.StandardOutput));
        var submodule = file.HeadMode == "160000" || file.IndexMode == "160000";
        async Task<long?> Size(string hash, string mode)
        {
            if (hash.All(c => c == '0')) return 0;
            if (mode == "160000") return null;
            var value = await RunAsync(git.Executable, path, ReadArguments(["cat-file", "-s", hash]), token);
            RequireSuccess(value);
            if (!long.TryParse(Text(value.StandardOutput).Trim(), NumberStyles.None, CultureInfo.InvariantCulture, out var length)) throw new GitReadException(GitFailure.InvalidOutput);
            return length;
        }
        var before = await Size(file.HeadObject, file.HeadMode); var after = await Size(file.IndexObject, file.IndexMode);
        if (binary) return new FileDiff(FileDiffKind.Binary, [], before, after, added, removed, "");
        var patch = await RunAsync(git.Executable, path, ReadArguments([.. args, "--patch", "--unified=3", .. revisions, "--", .. paths]), token, MaximumDiffBytes);
        if (patch.Outcome == Processes.ProcessOutcome.OutputLimit) return new FileDiff(FileDiffKind.TooLarge, [], before, after, added, removed, "");
        RequireSuccess(patch);
        try
        {
            var raw = new UTF8Encoding(false, true).GetString(patch.StandardOutput);
            return new FileDiff(submodule ? FileDiffKind.Submodule : FileDiffKind.Text, ParseDiffLines(raw, token), before, after, added, removed, raw);
        }
        catch (DecoderFallbackException) { return new FileDiff(FileDiffKind.UnsupportedEncoding, [], before, after, added, removed, ""); }
        catch (GitReadException error) when (error.Failure == GitFailure.OutputLimit) { return new FileDiff(FileDiffKind.TooLarge, [], before, after, added, removed, ""); }
    }, token);
    private static void ValidateComparison(string hash, string? parent)
    { if (!IsObjectId(hash) || parent is not null && !IsObjectId(parent)) throw new GitReadException(GitFailure.InvalidOutput); }
}
