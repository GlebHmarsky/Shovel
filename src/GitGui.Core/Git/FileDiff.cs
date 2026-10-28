using System.Globalization;
using System.Text;
using System.Text.RegularExpressions;

namespace GitGui.Core.Git;

public enum DiffLineKind { Metadata, Hunk, Context, Added, Removed }
public enum FileDiffKind { Text, Binary, Submodule, Directory, UnsupportedPath, UnsupportedEncoding, TooLarge }
public sealed record DiffLine(DiffLineKind Kind, string Text, int? OldLine = null, int? NewLine = null);
public sealed record FileDiff(FileDiffKind Kind, IReadOnlyList<DiffLine> Lines, long? OldBytes, long? NewBytes,
    int Added, int Removed, string RawPatch);
public sealed record DiffOptions(bool IgnoreWhitespace = false, bool WholeFile = false);

public sealed partial class RepositoryReader
{
    public const int MaximumDiffBytes = 2 * 1024 * 1024;
    public const int MaximumDiffLines = 20000;
    public const int MaximumDiffLineLength = 16384;

    public Task<FileDiff> ReadFileDiffAsync(WorkingCopyContext context, LocalChange entry, DiffOptions options, CancellationToken token) => Task.Run(async () =>
    {
        var git = context.Git; var path = context.Identity.Worktree;
        if (!context.Changes.Entries.Contains(entry)) throw new GitActionException("CommitStale");
        var file = WorkingCopyPath(path, entry.Path);
        var isIndex = entry.Group == ChangeGroup.Staged;
        var isNew = entry.Group is ChangeGroup.Untracked or ChangeGroup.Ignored;
        var oldHash = isIndex ? entry.HeadObject : entry.IndexObject;
        var submodule = entry.HeadMode == "160000" || entry.IndexMode == "160000";
        if (!isIndex && HasReparseComponent(path, file)) return new(FileDiffKind.UnsupportedPath, [], null, null, 0, 0, "");
        if (!isIndex && Directory.Exists(file) && !submodule) return new(FileDiffKind.Directory, [], null, null, 0, 0, "");
        var before = FileStamp(file);
        if (await GitActions.IndexStampAsync(context.Repository, token) != context.IndexStamp) throw new GitActionException("CommitStale");
        long? oldBytes = submodule ? null : isNew ? 0 : await BlobSize(oldHash);
        long? newBytes = submodule ? null : isIndex ? await BlobSize(entry.IndexObject) : before.Length;
        string[] overrides = ["-c", "core.quotepath=false", .. await FilterOverridesAsync(git, path, token)];
        var scope = isNew ? new[] { "--no-index" } : isIndex
            ? context.Repository.Commit is { } head ? new[] { "--cached", head } : ["--cached"] : Array.Empty<string>();
        var paths = isNew ? new[] { "/dev/null", entry.Path } : isIndex && entry.OriginalPath is { } oldPath
            ? new[] { entry.Path, oldPath } : [entry.Path];
        var common = new[] { "--no-ext-diff", "--no-textconv", "--no-color", "--find-renames", "--submodule=short", "--ignore-submodules=dirty" };
        var stat = await RunAsync(git.Executable, path, ReadArguments([.. overrides, "--literal-pathspecs", "diff", .. scope,
            .. common, .. options.IgnoreWhitespace ? new[] { "--ignore-all-space" } : [], "--numstat", "-z", "--", .. paths]), token, MaximumDiffBytes);
        RequireSuccess(stat, allowOne: isNew);
        var (binary, added, removed) = ParseDiffStats(Text(stat.StandardOutput));
        string patch = ""; IReadOnlyList<DiffLine> lines = []; var kind = binary ? FileDiffKind.Binary : submodule ? FileDiffKind.Submodule : FileDiffKind.Text;
        if (!binary)
        {
            var result = await RunAsync(git.Executable, path, ReadArguments([.. overrides, "--literal-pathspecs", "diff", .. scope,
                .. common, .. options.IgnoreWhitespace ? new[] { "--ignore-all-space" } : [], "--patch", options.WholeFile ? "--unified=20000" : "--unified=3", "--", .. paths]), token, MaximumDiffBytes);
            if (result.Outcome == Processes.ProcessOutcome.OutputLimit) kind = FileDiffKind.TooLarge;
            else
            {
                RequireSuccess(result, allowOne: isNew);
                try { patch = new UTF8Encoding(false, true).GetString(result.StandardOutput); lines = ParseDiffLines(patch, token); }
                catch (DecoderFallbackException) { kind = FileDiffKind.UnsupportedEncoding; patch = ""; }
                catch (GitReadException exception) when (exception.Failure == GitFailure.OutputLimit) { kind = FileDiffKind.TooLarge; patch = ""; }
            }
        }
        if (await GitActions.IndexStampAsync(context.Repository, token) != context.IndexStamp || !isIndex && FileStamp(file) != before)
            throw new GitActionException("CommitStale");
        return new FileDiff(kind, lines, oldBytes, newBytes, added, removed, patch);

        async Task<long?> BlobSize(string hash)
        {
            if (hash.Length == 0) return null;
            if (hash.All(c => c == '0')) return 0;
            if (!Regex.IsMatch(hash, "^[0-9a-f]{40}([0-9a-f]{24})?$")) throw new GitReadException(GitFailure.InvalidOutput);
            var result = await RunAsync(git.Executable, path, ReadArguments(["cat-file", "-s", hash]), token);
            RequireSuccess(result);
            if (!long.TryParse(Text(result.StandardOutput).Trim(), NumberStyles.None, CultureInfo.InvariantCulture, out var size)) throw new GitReadException(GitFailure.InvalidOutput);
            return size;
        }
    }, token);

    internal static string WorkingCopyPath(string root, string relative)
    {
        if (string.IsNullOrEmpty(relative) || relative.IndexOf('\0') >= 0 || Path.IsPathRooted(relative) ||
            relative.Split('/', '\\').Any(part => part == ".." || part.Equals(".git", StringComparison.OrdinalIgnoreCase)))
            throw new GitActionException("CommitInvalidPath");
        var result = Path.GetFullPath(Path.Combine(root, relative));
        if (!result.StartsWith(Path.TrimEndingDirectorySeparator(root) + Path.DirectorySeparatorChar, StringComparison.OrdinalIgnoreCase))
            throw new GitActionException("CommitInvalidPath");
        return result;
    }
    private static bool HasReparseComponent(string root, string file)
    {
        for (var item = file; item.Length > root.Length; item = Path.GetDirectoryName(item)!)
            if (Path.Exists(item) && File.GetAttributes(item).HasFlag(FileAttributes.ReparsePoint)) return true;
        return false;
    }
    private static (long Length, DateTime LastWrite) FileStamp(string file)
    {
        var info = new FileInfo(file); return info.Exists ? (info.Length, info.LastWriteTimeUtc) : (0, default);
    }
    private static (bool Binary, int Added, int Removed) ParseDiffStats(string output)
    {
        var records = output.Split('\0'); var binary = false; var added = 0; var removed = 0;
        for (var i = 0; i < records.Length - 1; i++)
        {
            var fields = records[i].Split('\t', 3);
            if (fields.Length != 3) throw new GitReadException(GitFailure.InvalidOutput);
            if (fields[0] == "-" && fields[1] == "-") binary = true;
            else if (int.TryParse(fields[0], NumberStyles.None, CultureInfo.InvariantCulture, out var a) && int.TryParse(fields[1], NumberStyles.None, CultureInfo.InvariantCulture, out var r))
            { added = checked(added + a); removed = checked(removed + r); }
            else throw new GitReadException(GitFailure.InvalidOutput);
            if (fields[2].Length == 0) { if (i + 2 >= records.Length - 1) throw new GitReadException(GitFailure.InvalidOutput); i += 2; }
        }
        return (binary, added, removed);
    }
    public static IReadOnlyList<DiffLine> ParseDiffLines(string patch, CancellationToken token = default)
    {
        var result = new List<DiffLine>(); int? old = null, next = null; var combined = false;
        foreach (var text in patch.Split('\n'))
        {
            token.ThrowIfCancellationRequested();
            if (result.Count == MaximumDiffLines || text.Length > MaximumDiffLineLength) throw new GitReadException(GitFailure.OutputLimit);
            if (text.StartsWith("@@", StringComparison.Ordinal))
            {
                var match = Regex.Match(text, @"^@@ -(\d+)(?:,\d+)? \+(\d+)(?:,\d+)? @@", RegexOptions.CultureInvariant);
                combined = !match.Success;
                old = match.Success ? int.Parse(match.Groups[1].Value, CultureInfo.InvariantCulture) : null;
                next = match.Success ? int.Parse(match.Groups[2].Value, CultureInfo.InvariantCulture) : null;
                result.Add(new(DiffLineKind.Hunk, text));
            }
            else if (!combined && old is not null && text.Length > 0 && text[0] == '+') result.Add(new(DiffLineKind.Added, text[1..], null, next++));
            else if (!combined && old is not null && text.Length > 0 && text[0] == '-') result.Add(new(DiffLineKind.Removed, text[1..], old++, null));
            else if (!combined && old is not null && text.Length > 0 && text[0] == ' ') result.Add(new(DiffLineKind.Context, text[1..], old++, next++));
            else
            {
                if (text.StartsWith("diff ", StringComparison.Ordinal)) { old = next = null; combined = false; }
                result.Add(new(DiffLineKind.Metadata, text));
            }
        }
        return result;
    }
}
