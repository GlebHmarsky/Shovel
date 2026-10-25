using System.Globalization;
using GitGui.Core.Diagnostics;
using System.Text;

namespace GitGui.Core.Git;

public sealed record CommitRecord(string Hash, IReadOnlyList<string> Parents, string Author, string AuthorEmail,
    DateTimeOffset AuthoredAt, string Committer, string CommitterEmail, DateTimeOffset CommittedAt, string Message)
{
    public string Subject => Message.Split('\n', 2)[0].TrimEnd('\r');
}
public enum HistoryScope { AllCommits, CurrentHead }
public sealed record CommitRef(string Name, string Hash);
public sealed record HistoryEntry(CommitRecord Commit, GraphRow Graph, IReadOnlyList<CommitRef>? References = null);
public sealed record HistorySession(RepositorySnapshot Repository, HistoryScope Scope, string Fingerprint,
    IReadOnlyList<CommitRef> References, bool IsShallow);
public sealed record HistoryCursor(long Offset, GraphCursor Graph, string? ExpectedFirst = null)
{
    public static HistoryCursor Start { get; } = new(0, GraphCursor.Start);
}
public sealed record CommitHistory(HistorySession Session, HistoryCursor Cursor, HistoryCursor? Next,
    IReadOnlyList<HistoryEntry> Entries)
{
    public bool HasMore => Next is not null;
    public bool IsShallow => Session.IsShallow;
}

public sealed partial class RepositoryReader
{
    public const int HistoryPageSize = 200;
    public const int MaximumHistoryBytes = 4 * 1024 * 1024;

    public Task<HistorySession> OpenHistoryAsync(GitInstallation git, RepositorySnapshot repository,
        HistoryScope scope, CancellationToken token) => Task.Run(async () =>
    {
        using var timing = trace?.Measure(TimingStage.References);
        var state = await ReadHistoryStateAsync(git, repository, token).ConfigureAwait(false);
        var refs = new List<CommitRef>();
        foreach (var record in state.Refs.Split('\n', StringSplitOptions.RemoveEmptyEntries))
        {
            token.ThrowIfCancellationRequested();
            var fields = record.Split('\0');
            if (fields.Length != 5 || !IsObjectId(fields[1])) throw new GitReadException(GitFailure.InvalidOutput);
            var name = fields[0];
            if (!name.StartsWith("refs/heads/", StringComparison.Ordinal) &&
                !name.StartsWith("refs/remotes/", StringComparison.Ordinal) &&
                !name.StartsWith("refs/tags/", StringComparison.Ordinal)) continue;
            var hash = fields[2] == "commit" ? fields[1] : fields[4] == "commit" ? fields[3] : null;
            // Nested annotated tags require recursive peeling. Non-commit tags have no history row.
            if (fields[4] == "tag")
            {
                var peeled = await RunAsync(git.Executable, repository.Worktree,
                    ReadArguments(["rev-parse", "--verify", "--quiet", fields[1] + "^{commit}"]), token).ConfigureAwait(false);
                RequireSuccess(peeled, allowOne: true);
                if (peeled.ExitCode == 0) hash = Line(peeled.StandardOutput);
            }
            if (hash is null) continue;
            if (!IsObjectId(hash)) throw new GitReadException(GitFailure.InvalidOutput);
            refs.Add(new(name, hash));
        }
        if (repository.Commit is not null) refs.Add(new("HEAD", repository.Commit));
        return new HistorySession(repository, scope, state.Fingerprint, refs.ToArray(), state.IsShallow);
    }, token);

    public async Task<CommitHistory> ReadInitialHistoryAsync(GitInstallation git, RepositorySnapshot repository,
        HistoryScope scope, CancellationToken token)
    {
        var session = await OpenHistoryAsync(git, repository, scope, token).ConfigureAwait(false);
        return await ReadPageAsync(git, session, HistoryCursor.Start, token, HistoryPageSize, freshSession: true).ConfigureAwait(false);
    }

    public Task<CommitHistory> ReadHistoryPageAsync(GitInstallation git, HistorySession session,
        HistoryCursor cursor, CancellationToken token, int pageSize = HistoryPageSize) => ReadPageAsync(git, session, cursor, token, pageSize, freshSession: false);

    private Task<CommitHistory> ReadPageAsync(GitInstallation git, HistorySession session,
        HistoryCursor cursor, CancellationToken token, int pageSize, bool freshSession) => Task.Run(async () =>
    {
        if (pageSize is < 1 or > HistoryPageSize || cursor.Offset < 0) throw new ArgumentOutOfRangeException(nameof(pageSize));
        using var timing = trace?.Measure(TimingStage.History);
        if (!freshSession)
        {
            var before = await ReadHistoryStateAsync(git, session.Repository, token).ConfigureAwait(false);
            if (before.Fingerprint != session.Fingerprint) throw new GitReadException(GitFailure.ChangedDuringRead);
        }
        var empty = session.Scope == HistoryScope.CurrentHead ? session.Repository.Commit is null :
            !session.References.Any(reference => reference.Name != "HEAD");
        if (empty)
        {
            var emptyState = await ReadHistoryStateAsync(git, session.Repository, token).ConfigureAwait(false);
            if (emptyState.Fingerprint != session.Fingerprint) throw new GitReadException(GitFailure.ChangedDuringRead);
            return new CommitHistory(session, cursor, null, []);
        }
        string[] roots = session.Scope == HistoryScope.AllCommits ? ["--branches", "--remotes", "--tags"] : [session.Repository.Commit!];
        // Re-walk an unchanged ref snapshot with --skip. Retain only one page of messages/rows.
        // Deep walks can hit the process deadline; they report a retryable error, never 'end'.
        var result = await RunAsync(git.Executable, session.Repository.Worktree, ReadArguments([
            "log", "--topo-order", "--no-color", "--no-decorate", "--no-show-signature", "--no-notes",
            "--no-use-mailmap", "--no-patch", "--no-ext-diff", "--no-textconv", "--encoding=UTF-8", "-z",
            "--format=tformat:%H%x00%P%x00%an%x00%ae%x00%aI%x00%cn%x00%ce%x00%cI%x00%B",
            "--skip=" + cursor.Offset.ToString(CultureInfo.InvariantCulture),
            "--max-count=" + (pageSize + 1).ToString(CultureInfo.InvariantCulture), .. roots, "--"
        ]), token, MaximumHistoryBytes).ConfigureAwait(false);
        RequireSuccess(result);
        var after = await ReadHistoryStateAsync(git, session.Repository, token).ConfigureAwait(false);
        if (after.Fingerprint != session.Fingerprint) throw new GitReadException(GitFailure.ChangedDuringRead);
        IReadOnlyList<CommitRecord> commits;
        using (trace?.Measure(TimingStage.Parsing)) commits = ParseHistory(result.StandardOutput);
        if (commits.Count > pageSize + 1 || (cursor.ExpectedFirst is not null &&
            (commits.Count == 0 || commits[0].Hash != cursor.ExpectedFirst))) throw new GitReadException(GitFailure.ChangedDuringRead);
        var shown = commits.Take(pageSize).ToArray();
        GraphPage graph;
        using (trace?.Measure(TimingStage.Graph)) graph = CommitGraph.BuildPage(shown, cursor.Graph, token);
        var labels = session.References.ToLookup(reference => reference.Hash, StringComparer.Ordinal);
        return new CommitHistory(session, cursor, commits.Count > pageSize
            ? new HistoryCursor(checked(cursor.Offset + shown.Length), graph.Next, commits[pageSize].Hash) : null,
            shown.Select((commit, index) => new HistoryEntry(commit, graph.Rows[index], labels[commit.Hash].ToArray())).ToArray());
    }, token);

    private async Task<(string Refs, string Fingerprint, bool IsShallow)> ReadHistoryStateAsync(
        GitInstallation git, RepositorySnapshot repository, CancellationToken token)
    {
        var result = await RunAsync(git.Executable, repository.Worktree, ReadArguments([
            "for-each-ref", "--sort=refname", "--format=%(refname)%00%(objectname)%00%(objecttype)%00%(*objectname)%00%(*objecttype)",
            "refs/heads/", "refs/remotes/", "refs/tags/", "refs/replace/"
        ]), token, MaximumHistoryBytes).ConfigureAwait(false);
        RequireSuccess(result);
        // Ref tips, replacement refs and traversal boundary files must remain unchanged while paging.
        var shallow = ReadBoundary("shallow");
        var grafts = ReadBoundary(Path.Combine("info", "grafts"));
        using var fingerprint = System.Security.Cryptography.IncrementalHash.CreateHash(System.Security.Cryptography.HashAlgorithmName.SHA256);
        foreach (var part in new[] { result.StandardOutput, shallow, grafts })
        { fingerprint.AppendData(BitConverter.GetBytes(part.Length)); fingerprint.AppendData(part); }
        return (Text(result.StandardOutput), Convert.ToHexString(fingerprint.GetHashAndReset()), shallow.Length != 0);

        byte[] ReadBoundary(string relative)
        {
            token.ThrowIfCancellationRequested();
            try
            {
                using var stream = File.OpenRead(Path.Combine(repository.CommonDirectory, relative));
                if (stream.Length > MaximumHistoryBytes) throw new GitReadException(GitFailure.OutputLimit);
                using var buffer = new MemoryStream();
                var bytes = new byte[8192]; int read;
                while ((read = stream.Read(bytes)) != 0)
                {
                    token.ThrowIfCancellationRequested();
                    if (buffer.Length + read > MaximumHistoryBytes) throw new GitReadException(GitFailure.OutputLimit);
                    buffer.Write(bytes, 0, read);
                }
                return buffer.ToArray();
            }
            catch (FileNotFoundException) { return []; }
            catch (DirectoryNotFoundException) { return []; }
        }
    }

    public static IReadOnlyList<CommitRecord> ParseHistory(byte[] bytes)
    {
        if (bytes.Length > MaximumHistoryBytes) throw new GitReadException(GitFailure.OutputLimit);
        if (bytes.Length == 0) return [];
        // NUL fields + NUL record terminator preserve multiline bodies and separator-like text.
        var fields = Encoding.UTF8.GetString(bytes).Split('\0');
        if (fields[^1].Length != 0 || (fields.Length - 1) % 9 != 0 || (fields.Length - 1) / 9 > HistoryPageSize + 1)
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
