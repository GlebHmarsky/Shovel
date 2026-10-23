using System.Globalization;
using System.Text;

namespace GitGui.Core.Git;

public sealed record CommitRecord(string Hash, IReadOnlyList<string> Parents, string Author, string AuthorEmail,
    DateTimeOffset AuthoredAt, string Committer, string CommitterEmail, DateTimeOffset CommittedAt, string Message)
{
    public string Subject => Message.Split('\n', 2)[0].TrimEnd('\r');
}
public sealed record HistoryEntry(CommitRecord Commit, GraphRow Graph);
public sealed record CommitHistory(string? Anchor, IReadOnlyList<HistoryEntry> Entries, bool HasMore, bool IsShallow);

public sealed partial class RepositoryReader
{
    public const int HistoryPageSize = 200;
    public const int MaximumHistoryCommits = 2000;
    public const int MaximumHistoryBytes = 4 * 1024 * 1024;

    // Re-read a bounded prefix from the same immutable tip rather than skip through a moving ref.
    // Caller supplies the observed HEAD hash and maintains its session trust/generation gate.
    public Task<CommitHistory> ReadHistoryAsync(GitInstallation git, string directory, string? anchor,
        int count, CancellationToken token) => Task.Run(async () =>
    {
        if (count is < 1 or > MaximumHistoryCommits) throw new ArgumentOutOfRangeException(nameof(count));
        token.ThrowIfCancellationRequested();
        if (anchor is null) return new CommitHistory(null, [], false, false); // Unborn HEAD.
        if (!IsObjectId(anchor)) throw new GitReadException(GitFailure.InvalidOutput);
        var shallow = await RunAsync(git.Executable, directory,
            ReadArguments(["rev-parse", "--is-shallow-repository"]), token).ConfigureAwait(false);
        RequireSuccess(shallow);
        var shallowText = Line(shallow.StandardOutput);
        if (shallowText is not ("true" or "false")) throw new GitReadException(GitFailure.InvalidOutput);
        var result = await RunAsync(git.Executable, directory, ReadArguments([
            "log", "--topo-order", "--no-color", "--no-decorate", "--no-show-signature", "--no-notes",
            "--no-use-mailmap", "--no-patch", "--no-ext-diff", "--no-textconv", "--encoding=UTF-8", "-z",
            "--format=tformat:%H%x00%P%x00%an%x00%ae%x00%aI%x00%cn%x00%ce%x00%cI%x00%B",
            "--max-count=" + (count + 1).ToString(CultureInfo.InvariantCulture), anchor, "--"
        ]), token, MaximumHistoryBytes).ConfigureAwait(false);
        RequireSuccess(result);
        var commits = ParseHistory(result.StandardOutput);
        if (commits.Count > count + 1 || commits.Count == 0 || commits[0].Hash != anchor)
            throw new GitReadException(GitFailure.InvalidOutput);
        token.ThrowIfCancellationRequested();
        var shown = commits.Take(count).ToArray();
        var graph = CommitGraph.Build(shown, token);
        return new CommitHistory(anchor, shown.Select((commit, index) => new HistoryEntry(commit, graph[index])).ToArray(),
            commits.Count > count, shallowText == "true");
    }, token);

    public static IReadOnlyList<CommitRecord> ParseHistory(byte[] bytes)
    {
        if (bytes.Length > MaximumHistoryBytes) throw new GitReadException(GitFailure.OutputLimit);
        if (bytes.Length == 0) return [];
        // NUL fields + NUL record terminator preserve multiline bodies and separator-like text.
        var fields = Encoding.UTF8.GetString(bytes).Split('\0');
        if (fields[^1].Length != 0 || (fields.Length - 1) % 9 != 0 || (fields.Length - 1) / 9 > MaximumHistoryCommits + 1)
            throw new GitReadException(GitFailure.InvalidOutput);
        var commits = new List<CommitRecord>();
        for (var i = 0; i < fields.Length - 1; i += 9)
        {
            var parents = fields[i + 1].Split(' ', StringSplitOptions.RemoveEmptyEntries);
            if (!IsObjectId(fields[i]) || parents.Any(parent => !IsObjectId(parent)) ||
                !ParseDate(fields[i + 4], out var authored) || !ParseDate(fields[i + 7], out var committed))
                throw new GitReadException(GitFailure.InvalidOutput);
            commits.Add(new(fields[i], parents, fields[i + 2], fields[i + 3], authored,
                fields[i + 5], fields[i + 6], committed, fields[i + 8]));
        }
        return commits;
    }

    private static bool IsObjectId(string value) => value.Length is >= 4 and <= 128 && value.All(char.IsAsciiHexDigit);
    private static bool ParseDate(string value, out DateTimeOffset date) => DateTimeOffset.TryParseExact(
        value.EndsWith('Z') ? value[..^1] + "+00:00" : value,
        "yyyy-MM-dd'T'HH:mm:sszzz", CultureInfo.InvariantCulture, DateTimeStyles.None, out date);
}
