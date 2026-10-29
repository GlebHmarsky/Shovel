using System.Text.Json;

namespace GitGui.Core.Git;

// Rebuildable presentation only. Never restores mutation readiness, command intent, credentials or diagnostics.
public sealed record CachedHistory(int Schema, string Directory, string Identity, RepositorySnapshot Repository,
    HistoryEntry[] Entries, long Offset, string? SelectedHash, double Vertical, double Horizontal, DateTimeOffset SavedAt);

public sealed class HistoryCacheStore(string directory)
{
    private const int MaximumBytes = 4 * 1024 * 1024;
    public string FilePath { get; } = Path.Combine(directory, "history-view.json");
    public Task<CachedHistory?> ReadAsync(string path, string identity, CancellationToken token) => Task.Run(async () =>
    {
        try
        {
            await using var stream = new FileStream(FilePath, FileMode.Open, FileAccess.Read, FileShare.Read | FileShare.Delete, 65536, FileOptions.Asynchronous);
            if (stream.Length > MaximumBytes) return null;
            var cache = await JsonSerializer.DeserializeAsync<CachedHistory>(stream, cancellationToken: token);
            if (cache is not { Schema: 1, Repository: { RemoteNames: not null }, Entries.Length: <= RepositoryReader.HistoryPageSize } || cache.Identity != identity ||
                !string.Equals(cache.Directory, path, StringComparison.OrdinalIgnoreCase) || cache.Repository.Worktree != cache.Directory ||
                cache.Offset < 0 || !double.IsFinite(cache.Vertical) || !double.IsFinite(cache.Horizontal) || cache.Vertical < 0 || cache.Horizontal < 0 ||
                cache.Entries.Any(entry => !ValidEntry(entry))) return null;
            return cache;
        }
        catch (Exception error) when (error is IOException or UnauthorizedAccessException or JsonException or NotSupportedException) { return null; }
    }, token);

    private static bool ValidEntry(HistoryEntry? entry) => entry is { Commit: { Hash: not null, Parents: not null, Message: not null,
        Author: not null, AuthorEmail: not null, Committer: not null, CommitterEmail: not null }, Graph: { Lines: not null } graph } &&
        graph.NodeLane is >= 0 and < CommitGraph.MaximumLanes && graph.LaneCount is >= 1 and <= CommitGraph.MaximumLanes && graph.NodeColor >= 0 &&
        graph.Lines.Count <= 256 && graph.Lines.All(line => line is not null && line.FromLane is >= 0 and < CommitGraph.MaximumLanes &&
            line.ToLane is >= 0 and < CommitGraph.MaximumLanes && line.Color >= 0 && double.IsFinite(line.FromY) && double.IsFinite(line.ToY) &&
            line.FromY is >= 0 and <= 1 && line.ToY is >= 0 and <= 1) &&
        (entry.References is null || entry.References.All(reference => reference is { Name: not null, Hash: not null }));

    public Task SaveAsync(CachedHistory? cache, CancellationToken token) => Task.Run(async () =>
    {
        var temporary = FilePath + "." + Guid.NewGuid().ToString("N") + ".tmp";
        try
        {
            if (cache is null) { File.Delete(FilePath); return; }
            var bytes = JsonSerializer.SerializeToUtf8Bytes(cache);
            if (bytes.Length > MaximumBytes) { File.Delete(FilePath); return; }
            Directory.CreateDirectory(Path.GetDirectoryName(FilePath)!);
            await File.WriteAllBytesAsync(temporary, bytes, token);
            token.ThrowIfCancellationRequested(); File.Move(temporary, FilePath, overwrite: true);
        }
        catch (Exception error) when (error is IOException or UnauthorizedAccessException or JsonException or OperationCanceledException) { }
        finally { try { File.Delete(temporary); } catch (IOException) { } catch (UnauthorizedAccessException) { } }
    }, token);
}
