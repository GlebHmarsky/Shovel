using System.Collections.Concurrent;
using System.Globalization;
using System.Text;
using System.Text.RegularExpressions;

namespace GitGui.Core.Git;

public sealed partial class RepositoryReader
{
    private sealed record BlobSummary(long? Bytes, bool Lfs = false, bool Binary = false);
    private readonly ConcurrentDictionary<string, BlobSummary> _blobSummaries = new(StringComparer.Ordinal);

    private async Task<BlobSummary> ReadBlobSummaryAsync(GitInstallation git, string path, string hash, CancellationToken token)
    {
        if (hash.Length == 0) return new(null);
        if (hash.All(c => c == '0')) return new(0);
        if (!IsObjectId(hash)) throw new GitReadException(GitFailure.InvalidOutput);
        if (_blobSummaries.TryGetValue(hash, out var cached)) return cached;
        var size = await RunAsync(git.Executable, path, ReadArguments(["cat-file", "-s", hash]), token);
        RequireSuccess(size);
        if (!long.TryParse(Text(size.StandardOutput).Trim(), NumberStyles.None, CultureInfo.InvariantCulture, out var bytes))
            throw new GitReadException(GitFailure.InvalidOutput);
        var value = new BlobSummary(bytes);
        // LFS pointers are below 1 KiB. Never read/download their large objects to obtain size.
        if (bytes is > 0 and < 1024)
        {
            var contents = await RunAsync(git.Executable, path, ReadArguments(["cat-file", "blob", hash]), token, 1024);
            RequireSuccess(contents);
            var lfsSize = LfsSize(contents.StandardOutput);
            value = new(lfsSize ?? bytes, lfsSize is not null, lfsSize is not null || contents.StandardOutput.Contains((byte)0));
        }
        if (_blobSummaries.Count >= 256) _blobSummaries.Clear();
        _blobSummaries[hash] = value;
        return value;
    }

    private static async Task<BlobSummary> ReadWorktreeSummaryAsync(string file, CancellationToken token)
    {
        try
        {
            await using var stream = new FileStream(file, FileMode.Open, FileAccess.Read, FileShare.ReadWrite | FileShare.Delete,
                8192, FileOptions.Asynchronous | FileOptions.SequentialScan);
            var bytes = new byte[Math.Min(8192, stream.Length)];
            var count = await stream.ReadAtLeastAsync(bytes, bytes.Length, throwOnEndOfStream: false, token);
            var prefix = bytes.AsSpan(0, count); var lfsSize = stream.Length < 1024 ? LfsSize(prefix) : null;
            return new(lfsSize ?? stream.Length, lfsSize is not null, lfsSize is not null || prefix.Contains((byte)0));
        }
        catch (FileNotFoundException) { return new(0); }
        catch (DirectoryNotFoundException) { return new(0); }
    }

    // Recognize v1 metadata fields only; malformed pointers remain ordinary file content.
    private static long? LfsSize(ReadOnlySpan<byte> bytes)
    {
        if (bytes.Length is 0 or >= 1024) return null;
        string text;
        try { text = new UTF8Encoding(false, true).GetString(bytes); } catch (DecoderFallbackException) { return null; }
        var lines = text.Split('\n');
        if (lines.Length < 4 || lines[0] != "version https://git-lfs.github.com/spec/v1" || lines[^1] != "") return null;
        var keys = new HashSet<string>(StringComparer.Ordinal); long? size = null; var oid = false;
        foreach (var line in lines.Skip(1).SkipLast(1))
        {
            var split = line.IndexOf(' '); if (split < 1 || line.Contains('\r') || !keys.Add(line[..split])) return null;
            var key = line[..split]; var value = line[(split + 1)..];
            if (key == "oid") oid = Regex.IsMatch(value, "^sha256:[0-9a-f]{64}$", RegexOptions.CultureInvariant);
            if (key == "size") { if (!long.TryParse(value, NumberStyles.None, CultureInfo.InvariantCulture, out var number)) return null; size = number; }
        }
        return oid ? size : null;
    }
}
