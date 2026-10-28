using System.Globalization;
using System.Security.Cryptography;
using System.Text;
using System.Text.RegularExpressions;

namespace GitGui.Core.Git;

public enum HunkAvailability { Available, ChangeType, PreviewOptions, ContentType, Filtered, NoChanges }
public sealed record DiffHunk(int Id, int StartRow, int RowCount, string Header);
public sealed record HunkSelection(HunkPreview Preview, IReadOnlyList<int> HunkIds);

/// <summary>An immutable exact preview, produced only by the reader; never accepts edited patch text from the UI.</summary>
public sealed class HunkPreview
{
    public LocalChange Change { get; }
    public IReadOnlyList<DiffHunk> Hunks { get; }
    internal WorkingCopyContext Context { get; }
    internal string RawPatch { get; }
    internal string WorktreeStamp { get; }
    internal string AttributeStamp { get; }
    private readonly string[] _lines;

    internal HunkPreview(WorkingCopyContext context, LocalChange change, string patch, string worktreeStamp,
        string attributeStamp, IReadOnlyList<DiffHunk> hunks)
    {
        Context = context; Change = change; RawPatch = patch; WorktreeStamp = worktreeStamp; AttributeStamp = attributeStamp;
        Hunks = Array.AsReadOnly(hunks.ToArray()); _lines = patch.Split('\n');
    }
    internal string Select(IReadOnlyList<int> ids)
    {
        var selected = ids.Distinct().Order().ToArray();
        if (selected.Length == 0 || selected.Any(id => id < 0 || id >= Hunks.Count)) throw new GitActionException("HunkSelectChange");
        var text = new StringBuilder();
        foreach (var line in _lines.Take(Hunks[0].StartRow)) text.Append(line).Append('\n');
        foreach (var id in selected)
            foreach (var line in _lines.Skip(Hunks[id].StartRow).Take(Hunks[id].RowCount)) text.Append(line).Append('\n');
        return text.ToString();
    }
}

public sealed partial class RepositoryReader
{
    internal static bool CanPreviewHunks(LocalChange entry) => entry.Code == 'M' && entry.OriginalPath is null &&
        (entry.Group is ChangeGroup.Staged or ChangeGroup.Unstaged) && (entry.IndexMode is "100644" or "100755") && entry.HeadMode == entry.IndexMode;

    // Hash bytes rather than just size/mtime: editors may preserve both. A read is still not a
    // transaction with external writers; verify again at execution and let Git own its index lock.
    internal static async Task<string> WorktreeContentStampAsync(string root, string relative, CancellationToken token)
    {
        var file = WorkingCopyPath(root, relative);
        if (HasReparseComponent(root, file) || Directory.Exists(file)) throw new GitActionException("HunkUnsupportedPath");
        try
        {
            await using var stream = new FileStream(file, FileMode.Open, FileAccess.Read, FileShare.ReadWrite | FileShare.Delete,
                65536, FileOptions.Asynchronous | FileOptions.SequentialScan);
            return Convert.ToHexString(await SHA256.HashDataAsync(stream, token));
        }
        catch (FileNotFoundException) { return "missing"; }
        catch (DirectoryNotFoundException) { return "missing"; }
    }

    internal async Task<(bool Supported, string Stamp)> ReadHunkAttributesAsync(GitInstallation git, string root, string path, CancellationToken token)
    {
        // Do not apply the unfiltered preview as LFS/clean-filter content or bypass encoding/ident transforms.
        var result = await RunAsync(git.Executable, root, ReadArguments(["check-attr", "-z", "filter", "working-tree-encoding", "ident", "text", "eol", "--", path]), token);
        RequireSuccess(result); var fields = Text(result.StandardOutput).Split('\0');
        if (fields.Length != 16 || fields[^1] != "") throw new GitReadException(GitFailure.InvalidOutput);
        var supported = true;
        for (var i = 0; i < fields.Length - 1; i += 3)
        {
            if (fields[i] != path) throw new GitReadException(GitFailure.InvalidOutput);
            if ((fields[i + 1] is "filter" or "working-tree-encoding" or "ident") && fields[i + 2] is not ("unspecified" or "unset")) supported = false;
        }
        return (supported, Convert.ToHexString(SHA256.HashData(result.StandardOutput)));
    }

    private static IReadOnlyList<DiffHunk>? ParsePatchHunks(string patch, CancellationToken token)
    {
        var lines = patch.Split('\n');
        // Only an ordinary single-file content modification; no mode/path/add/delete side effects.
        if (lines.Length < 6 || lines[^1] != "" || !lines[0].StartsWith("diff --git ", StringComparison.Ordinal) ||
            !Regex.IsMatch(lines[1], @"^index [0-9a-f]+\.\.[0-9a-f]+ (100644|100755)$") ||
            (!lines[2].StartsWith("--- a/", StringComparison.Ordinal) && !lines[2].StartsWith("--- \"a/", StringComparison.Ordinal)) ||
            (!lines[3].StartsWith("+++ b/", StringComparison.Ordinal) && !lines[3].StartsWith("+++ \"b/", StringComparison.Ordinal))) return null;
        var hunks = new List<DiffHunk>(); var index = 4;
        while (index < lines.Length - 1)
        {
            token.ThrowIfCancellationRequested(); var start = index;
            var match = Regex.Match(lines[index++], @"^@@ -(\d+)(?:,(\d+))? \+(\d+)(?:,(\d+))? @@(?: .*)?$", RegexOptions.CultureInvariant);
            if (!match.Success || !int.TryParse(match.Groups[2].Success ? match.Groups[2].Value : "1", NumberStyles.None, CultureInfo.InvariantCulture, out var oldCount) ||
                !int.TryParse(match.Groups[4].Success ? match.Groups[4].Value : "1", NumberStyles.None, CultureInfo.InvariantCulture, out var newCount)) return null;
            var oldLines = 0; var newLines = 0; var changed = false;
            while (index < lines.Length - 1 && !lines[index].StartsWith("@@", StringComparison.Ordinal))
            {
                token.ThrowIfCancellationRequested(); var line = lines[index++];
                if (line.Length == 0) return null;
                switch (line[0])
                {
                    case ' ': oldLines++; newLines++; break;
                    case '+': newLines++; changed = true; break;
                    case '-': oldLines++; changed = true; break;
                    case '\\' when line == "\\ No newline at end of file": break;
                    default: return null;
                }
            }
            if (!changed || oldLines != oldCount || newLines != newCount) return null;
            hunks.Add(new(hunks.Count, start, index - start, lines[start]));
        }
        return hunks;
    }
}
