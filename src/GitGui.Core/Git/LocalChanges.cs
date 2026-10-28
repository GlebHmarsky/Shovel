using System.Text;
using System.Text.RegularExpressions;

namespace GitGui.Core.Git;

public enum ChangeGroup { Conflicted, Staged, Unstaged, Untracked, Ignored }
public sealed record LocalChange(ChangeGroup Group, string Path, char Code, string? OriginalPath = null, string Submodule = "N...")
{
    public string HeadObject { get; init; } = "";
    public string IndexObject { get; init; } = "";
    public string HeadMode { get; init; } = "";
    public string IndexMode { get; init; } = "";
}
public sealed record LocalChanges(IReadOnlyList<LocalChange> Entries, DateTimeOffset ReadAt, bool IncludesIgnored);

public sealed partial class RepositoryReader
{
    public const int MaximumStatusBytes = 4 * 1024 * 1024;
    public const int MaximumStatusEntries = 20000;

    public Task<LocalChanges> ReadLocalChangesAsync(GitInstallation git, string path, bool ignored, CancellationToken token,
        IReadOnlyList<string>? paths = null, string[]? filterOverrides = null) => Task.Run(async () =>
    {
        var overrides = filterOverrides ?? await FilterOverridesAsync(git, path, token);
        foreach (var item in paths ?? []) WorkingCopyPath(path, item);
        var result = await RunAsync(git.Executable, path, ReadArguments([.. overrides,
            "--literal-pathspecs", "-c", "status.submoduleSummary=false", "status", "--porcelain=v2", "-z", "--untracked-files=all",
            "--ignore-submodules=dirty", "--renames", ignored ? "--ignored=matching" : "--ignored=no", "--", .. paths ?? []]), token, MaximumStatusBytes);
        RequireSuccess(result);
        return new LocalChanges(ParseLocalChanges(result.StandardOutput, token), DateTimeOffset.Now, ignored);
    }, token);

    // Shared by automatic status/diff reads. Explicit staging still honors configured filters.
    internal async Task<string[]> FilterOverridesAsync(GitInstallation git, string path, CancellationToken token)
    {
        var config = await RunAsync(git.Executable, path, ReadArguments(["config", "--null", "--name-only", "--get-regexp", @"^filter\..*\.(clean|process|required)$"]), token);
        RequireSuccess(config, allowOne: true);
        var overrides = new List<string>();
        foreach (var record in Text(config.StandardOutput).Split('\0', StringSplitOptions.RemoveEmptyEntries))
        {
            var key = record;
            if (Regex.IsMatch(key, @"^filter\..+\.(clean|process|required)$", RegexOptions.CultureInvariant | RegexOptions.Singleline))
            { overrides.Add("-c"); overrides.Add(key + (key.EndsWith(".required", StringComparison.Ordinal) ? "=false" : "=")); }
        }
        return overrides.ToArray();
    }

    public static IReadOnlyList<LocalChange> ParseLocalChanges(byte[] bytes, CancellationToken token = default)
    {
        if (bytes.Length > MaximumStatusBytes) throw new GitReadException(GitFailure.OutputLimit);
        string text;
        try { text = new UTF8Encoding(false, true).GetString(bytes); }
        catch (DecoderFallbackException) { throw new GitReadException(GitFailure.InvalidOutput); }
        if (text.Length != 0 && text[^1] != '\0') throw new GitReadException(GitFailure.InvalidOutput);
        var records = text.Split('\0'); var entries = new List<LocalChange>();
        for (var i = 0; i < records.Length - 1; i++)
        {
            token.ThrowIfCancellationRequested();
            var record = records[i];
            if (record.StartsWith("? ", StringComparison.Ordinal) || record.StartsWith("! ", StringComparison.Ordinal))
            { Add(new(record[0] == '?' ? ChangeGroup.Untracked : ChangeGroup.Ignored, record[2..], record[0])); continue; }
            var count = record.StartsWith("1 ", StringComparison.Ordinal) ? 9 : record.StartsWith("2 ", StringComparison.Ordinal) ? 10 : record.StartsWith("u ", StringComparison.Ordinal) ? 11 : 0;
            if (count == 0) throw new GitReadException(GitFailure.InvalidOutput);
            var fields = record.Split(' ', count);
            if (fields.Length != count || fields[1].Length != 2 || fields[2].Length != 4 || fields[1].Any(c => !".MADRCUT".Contains(c)))
                throw new GitReadException(GitFailure.InvalidOutput);
            string? original = null;
            if (record[0] == '2')
            {
                if (++i >= records.Length - 1 || records[i].Length == 0) throw new GitReadException(GitFailure.InvalidOutput);
                original = records[i];
            }
            LocalChange Entry(ChangeGroup group, char code) => new(group, fields[^1], code, original, fields[2])
            { HeadObject = fields[6], IndexObject = fields[7], HeadMode = fields[3], IndexMode = fields[4] };
            if (record[0] == 'u') Add(new(ChangeGroup.Conflicted, fields[^1], 'U', Submodule: fields[2]));
            else
            {
                if (fields[1][0] != '.') Add(Entry(ChangeGroup.Staged, fields[1][0]));
                if (fields[1][1] != '.') Add(Entry(ChangeGroup.Unstaged, fields[1][1]));
            }
        }
        return entries.OrderBy(entry => entry.Group).ThenBy(entry => entry.Path, StringComparer.Ordinal).ToArray();
        void Add(LocalChange entry)
        {
            if (string.IsNullOrEmpty(entry.Path)) throw new GitReadException(GitFailure.InvalidOutput);
            if (entries.Count == MaximumStatusEntries) throw new GitReadException(GitFailure.OutputLimit);
            entries.Add(entry);
        }
    }
}
