using System.Collections.Concurrent;
using System.Globalization;
using System.Security.Cryptography;
using System.Text;

namespace GitGui.Core.Git;

public sealed partial class RepositoryReader
{
    private sealed record StatusPointer(string Oid = "", long Size = 0);
    private readonly ConcurrentDictionary<string, StatusPointer> _statusPointers = new(StringComparer.Ordinal);

    // Git's racy-index content check compares hydrated assets with pointers when filters are
    // suppressed. Verify only ordinary LFS v1 pointers locally; never run/download/install LFS.
    private async Task<IReadOnlyList<LocalChange>> NormalizeLfsStatusAsync(GitInstallation git, string path,
        IReadOnlyList<LocalChange> entries, CancellationToken token)
    {
        var clean = new HashSet<LocalChange>();
        var candidates = entries.Where(e => e.Group == ChangeGroup.Unstaged && e.Code == 'M' && e.OriginalPath is null &&
            e.IndexMode is "100644" or "100755" && e.IndexMode == e.WorktreeMode && IsObjectId(e.IndexObject)).ToArray();
        foreach (var batch in candidates.Chunk(64))
        {
            // Keep argv bounded even for long Unicode paths. Oversized groups stay visible.
            if (batch.Sum(e => e.Path.Length + 3) > 16000) continue;
            var attributes = await RunAsync(git.Executable, path, ReadArguments(["--literal-pathspecs", "check-attr", "-z", "filter", "--", .. batch.Select(e => e.Path)]), token);
            RequireSuccess(attributes);
            var fields = Text(attributes.StandardOutput).Split('\0');
            var lfs = new HashSet<string>(StringComparer.Ordinal);
            for (var i = 0; i + 2 < fields.Length; i += 3) if (fields[i + 1] == "filter" && fields[i + 2] == "lfs") lfs.Add(fields[i]);
            var selected = batch.Where(e => lfs.Contains(e.Path)).ToArray();
            await ReadStatusPointersAsync(git, path, selected.Select(e => e.IndexObject).Distinct().Where(h => !_statusPointers.ContainsKey(h)).ToArray(), token);
            foreach (var entry in selected)
            {
                token.ThrowIfCancellationRequested();
                if (!_statusPointers.TryGetValue(entry.IndexObject, out var pointer) || pointer.Oid.Length == 0) continue;
                var file = WorkingCopyPath(path, entry.Path);
                try
                {
                    if (HasReparseComponent(path, file)) continue;
                    var before = FileStamp(file);
                    if (before.Length != pointer.Size) continue;
                    await using var stream = new FileStream(file, FileMode.Open, FileAccess.Read, FileShare.Read, 65536, FileOptions.Asynchronous | FileOptions.SequentialScan);
                    if (stream.Length != pointer.Size) continue;
                    var oid = Convert.ToHexStringLower(await SHA256.HashDataAsync(stream, token));
                    if (oid == pointer.Oid && before == FileStamp(file)) clean.Add(entry);
                }
                catch (Exception error) when (error is IOException or UnauthorizedAccessException) { /* Keep uncertain rows visible. */ }
            }
        }
        return clean.Count == 0 ? entries : entries.Where(e => !clean.Contains(e)).ToArray();
    }

    private async Task ReadStatusPointersAsync(GitInstallation git, string path, string[] hashes, CancellationToken token)
    {
        if (hashes.Length == 0) return;
        if (_statusPointers.Count >= 4096) _statusPointers.Clear();
        var input = Path.Combine(Path.GetTempPath(), "shovel-objects-" + Guid.NewGuid().ToString("N"));
        try
        {
            await File.WriteAllTextAsync(input, string.Join('\n', hashes) + "\n", new UTF8Encoding(false), token);
            var sizes = await RunAsync(git.Executable, path, ReadArguments(["cat-file", "--batch-check=%(objectname) %(objecttype) %(objectsize)"]), token, inputFile: input);
            RequireSuccess(sizes);
            var small = new Dictionary<string, int>(StringComparer.Ordinal);
            foreach (var line in Text(sizes.StandardOutput).Split('\n', StringSplitOptions.RemoveEmptyEntries))
            {
                var fields = line.Split(' ');
                if (fields.Length != 3 || !hashes.Contains(fields[0]) || fields[1] != "blob" || !int.TryParse(fields[2], NumberStyles.None, CultureInfo.InvariantCulture, out var size)) continue;
                if (size is > 0 and < 1024) small.Add(fields[0], size);
                else _statusPointers[fields[0]] = new();
            }
            if (small.Count == 0) return;
            await File.WriteAllTextAsync(input, string.Join('\n', small.Keys) + "\n", new UTF8Encoding(false), token);
            var contents = await RunAsync(git.Executable, path, ReadArguments(["cat-file", "--batch"]), token, inputFile: input);
            RequireSuccess(contents);
            var data = contents.StandardOutput; var offset = 0;
            foreach (var (hash, size) in small)
            {
                var end = Array.IndexOf(data, (byte)'\n', offset);
                if (end < 0 || Encoding.ASCII.GetString(data, offset, end - offset) != $"{hash} blob {size}" || end + size + 1 >= data.Length || data[end + size + 1] != '\n')
                    throw new GitReadException(GitFailure.InvalidOutput);
                var text = Encoding.UTF8.GetString(data, end + 1, size); var lines = text.Split('\n');
                // Extension pointers/custom transforms remain visible instead of guessing their semantics.
                var lfsSize = LfsSize(data.AsSpan(end + 1, size));
                _statusPointers[hash] = lfsSize is { } length && lines.Length == 4 && lines[1].StartsWith("oid sha256:", StringComparison.Ordinal) && lines[2].StartsWith("size ", StringComparison.Ordinal)
                    ? new(lines[1][11..], length) : new();
                offset = end + size + 2;
            }
            if (offset != data.Length) throw new GitReadException(GitFailure.InvalidOutput);
        }
        finally { File.Delete(input); }
    }
}
