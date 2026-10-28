using System.Globalization;
using System.Text;
using GitGui.Core.Processes;

namespace GitGui.Core.Git;

public sealed record FileHistoryCursor(string Hash, string Path, int Offset);
public sealed record FileHistoryPage(IReadOnlyList<CommitRecord> Commits, FileHistoryCursor? Next, int Offset)
{
    public bool HasMore => Next is not null;
}
public sealed record BlameLine(string Hash, string Author, string Email, DateTimeOffset Date, int Number, string Text, string Summary);

public sealed partial class GitActions
{
    public Task<FileHistoryPage> ReadFileHistoryAsync(WorkingCopyContext context, LocalChange entry, int offset, CancellationToken token, FileHistoryCursor? cursor = null) => Task.Run(async () =>
    {
        await CheckFileRead(context, entry, token);
        if (offset < 0 || offset > 0 && cursor?.Offset != offset || cursor is not null && !ObjectId(cursor.Hash)) throw new GitReadException(GitFailure.InvalidOutput);
        if (context.Repository.Commit is null) return new FileHistoryPage([], null, offset);
        var relative = cursor?.Path ?? (entry.Code == 'R' ? entry.OriginalPath ?? entry.Path : entry.Path);
        RepositoryReader.WorkingCopyPath(context.Identity.Worktree, relative);
        // A single ancestry chain makes a hash/path cursor complete across rename boundaries.
        // The dialog explicitly labels this scope; the main All Commits view is unchanged.
        var result = await FileRead(context, ["log", "--first-parent", "--follow", "--no-color", "--no-decorate", "--no-show-signature", "--no-notes",
            "--no-use-mailmap", "--name-status", "--no-ext-diff", "--no-textconv", "--encoding=UTF-8", "-z",
            "--format=tformat:%H%x00%P%x00%an%x00%ae%x00%aI%x00%cn%x00%ce%x00%cI%x00%B",
            "--max-count=201", cursor?.Hash ?? context.Repository.Commit, "--", relative], token);
        var page = ParseFileHistory(result.StandardOutput, relative, offset);
        await CheckFileRead(context, entry, token);
        return page;
    }, token);

    private static FileHistoryPage ParseFileHistory(byte[] data, string relative, int offset)
    {
        var fields = new UTF8Encoding(false, true).GetString(data).Split('\0');
        var rows = new List<CommitRecord>(); FileHistoryCursor? next = null;
        if (fields[^1] != "") throw new GitReadException(GitFailure.InvalidOutput);
        for (var i = 0; i < fields.Length - 1;)
        {
            if (fields.Length - 1 - i < 9 || rows.Count > 200) throw new GitReadException(GitFailure.InvalidOutput);
            var commit = RepositoryReader.ParseHistory(Encoding.UTF8.GetBytes(string.Join('\0', fields.Skip(i).Take(9)) + "\0")).Single(); i += 9;
            if (rows.Count == 200) next = new(commit.Hash, relative, checked(offset + 200));
            rows.Add(commit);
            while (i < fields.Length - 1 && !ObjectId(fields[i]))
            {
                var status = fields[i++].TrimStart('\n'); if (status.Length == 0) continue;
                if (!"MADRTCU".Contains(status[0]) || i >= fields.Length - 1) throw new GitReadException(GitFailure.InvalidOutput);
                var first = fields[i++];
                if (status[0] is 'R' or 'C')
                {
                    if (i >= fields.Length - 1) throw new GitReadException(GitFailure.InvalidOutput);
                    var second = fields[i++]; if (relative == second) relative = first;
                }
            }
        }
        return new(rows.Take(200).ToArray(), next, offset);
    }

    public Task<IReadOnlyList<BlameLine>> ReadFileBlameAsync(WorkingCopyContext context, LocalChange entry, CancellationToken token) => Task.Run(async () =>
    {
        await CheckFileRead(context, entry, token);
        if (context.Repository.Commit is null) throw new GitActionException("FileBlameNoHead");
        var relative = entry.Code == 'R' ? entry.OriginalPath ?? entry.Path : entry.Path;
        var result = await FileRead(context, ["blame", "--line-porcelain", "--no-textconv", "--encoding=UTF-8", "--root",
            context.Repository.Commit, "--", relative], token);
        var rows = ParseFileBlame(result.StandardOutput);
        await CheckFileRead(context, entry, token);
        return rows;
    }, token);

    public static IReadOnlyList<BlameLine> ParseFileBlame(byte[] bytes)
    {
        if (bytes.Length > 4 * 1024 * 1024) throw new GitReadException(GitFailure.OutputLimit);
        string text;
        try { text = new UTF8Encoding(false, true).GetString(bytes); }
        catch (DecoderFallbackException) { throw new GitActionException("FileBlameBinary"); }
        if (text.Contains('\0')) throw new GitActionException("FileBlameBinary");
        var lines = text.Split('\n'); var rows = new List<BlameLine>();
        for (var i = 0; i < lines.Length - 1; i++)
        {
            var header = lines[i].Split(' ');
            if (header.Length is < 3 or > 4 || !ObjectId(header[0]) || !int.TryParse(header[2], CultureInfo.InvariantCulture, out var number))
                throw new GitReadException(GitFailure.InvalidOutput);
            string author = "", email = "", summary = ""; long seconds = 0; bool found = false;
            while (++i < lines.Length - 1)
            {
                var line = lines[i];
                if (line.StartsWith('\t'))
                {
                    if (rows.Count >= 20000 || line.Length > 16384) throw new GitReadException(GitFailure.OutputLimit);
                    rows.Add(new(header[0], author, email, DateTimeOffset.FromUnixTimeSeconds(seconds), number, line[1..], summary)); found = true; break;
                }
                if (line.StartsWith("author ", StringComparison.Ordinal)) author = line[7..];
                else if (line.StartsWith("author-mail ", StringComparison.Ordinal)) email = line[12..].Trim('<', '>');
                else if (line.StartsWith("author-time ", StringComparison.Ordinal) && !long.TryParse(line[12..], CultureInfo.InvariantCulture, out seconds)) throw new GitReadException(GitFailure.InvalidOutput);
                else if (line.StartsWith("summary ", StringComparison.Ordinal)) summary = line[8..];
            }
            if (!found) throw new GitReadException(GitFailure.InvalidOutput);
        }
        return rows;
    }

    public const int MaximumPatchBytes = 16 * 1024 * 1024;
    public Task<byte[]> ReadSelectedPatchAsync(WorkingCopyContext context, IReadOnlyList<LocalChange> entries, CancellationToken token) => Task.Run(async () =>
    {
        if (entries.Count == 0 || entries.Select(item => item.Group == ChangeGroup.Staged).Distinct().Count() != 1) throw new GitActionException("CommitStale");
        if (entries.Any(entry => !context.Changes.Entries.Contains(entry))) throw new GitActionException("CommitStale");
        await CheckFileRead(context, entries[0], token);
        if (await IndexStampAsync(context.Repository, token) != context.IndexStamp) throw new GitActionException("CommitStale");
        var paths = FileActionPaths(entries); foreach (var relative in paths) ValidateRegularTarget(context.Identity.Worktree, relative);
        if (paths.Length > 128 || paths.Sum(item => item.Length + 3) > 16000) throw new GitActionException("FileActionSelectionLimit");
        var before = await PatchStamps();
        var filters = await reader.FilterOverridesAsync(context.Git, context.Identity.Worktree, token);
        string[] flags = ["--binary", "--full-index", "--no-ext-diff", "--no-textconv", "--no-color", "--no-relative", "--src-prefix=a/", "--dst-prefix=b/", "--submodule=short", "--ignore-submodules=dirty"];
        using var stream = new MemoryStream();
        var tracked = FileActionPaths(entries.Where(item => item.Group != ChangeGroup.Untracked).ToArray());
        if (tracked.Length > 0)
        {
            string[] scope = entries[0].Group == ChangeGroup.Staged ? context.Repository.Commit is { } head ? ["--cached", head] : ["--cached"] : [];
            Add(await FileRead(context, [.. filters, "diff", .. scope, .. flags, "--", .. tracked], token, MaximumPatchBytes));
        }
        foreach (var entry in entries.Where(item => item.Group == ChangeGroup.Untracked))
            Add(await FileRead(context, [.. filters, "diff", "--no-index", .. flags, "--", "/dev/null", entry.Path], token, MaximumPatchBytes, allowOne: true));
        if (before != await PatchStamps() || await IndexStampAsync(context.Repository, token) != context.IndexStamp) throw new GitActionException("CommitStale");
        await CheckFileRead(context, entries[0], token);
        return stream.ToArray();
        void Add(ProcessResult result)
        {
            if (stream.Length + result.StandardOutput.Length > MaximumPatchBytes) throw new GitReadException(GitFailure.OutputLimit);
            stream.Write(result.StandardOutput);
        }
        async Task<string> PatchStamps()
        {
            if (entries[0].Group == ChangeGroup.Staged) return "";
            var stamps = new List<string>();
            foreach (var item in paths) stamps.Add(await RepositoryReader.WorktreeContentStampAsync(context.Identity.Worktree, item, token));
            return string.Join(';', stamps);
        }
    }, token);

    private async Task CheckFileRead(WorkingCopyContext context, LocalChange entry, CancellationToken token)
    {
        if (!context.Changes.Entries.Contains(entry)) throw new GitActionException("CommitStale");
        RepositoryReader.WorkingCopyPath(context.Identity.Worktree, entry.Path);
        if ((await identities.ReadAsync(context.Identity.Worktree, token)).Fingerprint != context.Identity.Fingerprint) throw new GitActionException("ActionIdentityChanged");
    }
    private async Task<ProcessResult> FileRead(WorkingCopyContext context, string[] args, CancellationToken token, int limit = 4 * 1024 * 1024, bool allowOne = false)
    {
        var result = await runner.RunAsync(new(context.Git.Executable, context.Identity.Worktree,
            ["--no-pager", "--no-lazy-fetch", "--no-optional-locks", "--literal-pathspecs", "-c", "core.fsmonitor=false", "-c", "gc.auto=0", "-c", "maintenance.auto=false", .. args])
            { Environment = EnvironmentForGit(), MaximumOutputBytes = limit }, token);
        if (result.Outcome != ProcessOutcome.Exited || result.ExitCode != 0 && !(allowOne && result.ExitCode == 1))
            throw new GitReadException(result.Outcome == ProcessOutcome.OutputLimit ? GitFailure.OutputLimit : result.Outcome == ProcessOutcome.Cancelled ? GitFailure.Cancelled : result.Outcome == ProcessOutcome.TimedOut ? GitFailure.TimedOut : GitFailure.CommandFailed,
                Redact(Encoding.UTF8.GetString(result.StandardError)));
        return result;
    }
}
