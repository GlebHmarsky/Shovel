using System.Globalization;
using System.Security.Cryptography;
using System.Text;

namespace GitGui.Core.Git;

public sealed partial class RepositoryReader
{
    private readonly object _indexCacheLock = new();
    private readonly Dictionary<string, (string FileStamp, string EntriesStamp)> _indexStamps = new(StringComparer.OrdinalIgnoreCase);

    public async Task<string> ReadIndexStampAsync(GitInstallation git, RepositorySnapshot repository, CancellationToken token)
    {
        var fileStamp = await IndexFileStampAsync(repository, token);
        lock (_indexCacheLock)
            if (_indexStamps.TryGetValue(repository.GitDirectory, out var cached) && cached.FileStamp == fileStamp) return cached.EntriesStamp;
        // Git decodes v2/v3/v4, split and sparse indexes. Hash entries and persistent flags,
        // not stat-cache timestamps, fsmonitor bits or extensions rewritten by status.
        // The debug trailer is deliberately validated; unknown output fails closed.
        var result = await RunAsync(git.Executable, repository.Worktree,
            ReadArguments(["ls-files", "--stage", "--debug", "-z"]), token, 64 * 1024 * 1024);
        RequireSuccess(result);
        var stamp = IndexEntriesStamp(result.StandardOutput);
        if (fileStamp == await IndexFileStampAsync(repository, token))
            lock (_indexCacheLock)
            {
                if (_indexStamps.Count >= 32 && !_indexStamps.ContainsKey(repository.GitDirectory)) _indexStamps.Clear();
                _indexStamps[repository.GitDirectory] = (fileStamp, stamp);
            }
        return stamp;
    }

    private static string IndexEntriesStamp(byte[] output)
    {
        using var hash = IncrementalHash.CreateHash(HashAlgorithmName.SHA256);
        var offset = 0;
        while (offset < output.Length)
        {
            var end = Array.IndexOf(output, (byte)0, offset);
            if (end < 0) throw new GitReadException(GitFailure.InvalidOutput);
            hash.AppendData(output, offset, end - offset + 1); offset = end + 1;
            string line = "";
            foreach (var prefix in new[] { "  ctime: ", "  mtime: ", "  dev: ", "  uid: ", "  size: " })
            {
                end = Array.IndexOf(output, (byte)'\n', offset);
                if (end < 0) throw new GitReadException(GitFailure.InvalidOutput);
                line = Encoding.ASCII.GetString(output, offset, end - offset).TrimEnd('\r'); offset = end + 1;
                if (!line.StartsWith(prefix, StringComparison.Ordinal)) throw new GitReadException(GitFailure.InvalidOutput);
            }
            var marker = line.IndexOf("flags: ", StringComparison.Ordinal);
            if (marker < 0 || !uint.TryParse(line.AsSpan(marker + 7), NumberStyles.HexNumber, CultureInfo.InvariantCulture, out var flags))
                throw new GitReadException(GitFailure.InvalidOutput);
            // CE_VALID, CE_INTENT_TO_ADD and CE_SKIP_WORKTREE affect future Git actions.
            hash.AppendData(BitConverter.GetBytes(flags & 0x60008000));
        }
        return Convert.ToHexString(hash.GetHashAndReset());
    }

    public static async Task<string> IndexFileStampAsync(RepositorySnapshot repository, CancellationToken token)
    {
        try
        {
            await using var stream = new FileStream(Path.Combine(repository.GitDirectory, "index"), FileMode.Open, FileAccess.Read,
                FileShare.ReadWrite | FileShare.Delete, 65536, FileOptions.Asynchronous | FileOptions.SequentialScan);
            return Convert.ToHexString(await SHA256.HashDataAsync(stream, token));
        }
        catch (FileNotFoundException) { return "missing"; }
    }
}
