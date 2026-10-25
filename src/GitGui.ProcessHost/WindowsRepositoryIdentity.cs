using System.ComponentModel;
using System.Runtime.InteropServices;
using System.Security.Cryptography;
using System.Text;
using GitGui.Core.Git;
using Microsoft.Win32.SafeHandles;

namespace GitGui.ProcessHost;

/// <summary>Replacement identity for local, non-reparse worktree roots. Not a hostile-filesystem sandbox.</summary>
public sealed class WindowsRepositoryIdentity : IRepositoryIdentityProvider
{
    public Task<RepositoryIdentity> ReadAsync(string path, CancellationToken token) => Task.Run(() =>
    {
        var root = Canonical(path);
        RejectReparseAncestors(root);
        var marker = Path.Combine(root, ".git");
        RejectReparseAncestors(marker);
        var gitDirectory = Canonical(Directory.Exists(marker) ? marker : ResolvePointer(marker, "gitdir: ", root));
        RejectReparseAncestors(gitDirectory);
        var commonFile = Path.Combine(gitDirectory, "commondir");
        var common = File.Exists(commonFile) ? ResolvePointer(commonFile, "", gitDirectory) : gitDirectory;
        using var hash = IncrementalHash.CreateHash(HashAlgorithmName.SHA256);
        foreach (var item in new[] { root, marker, gitDirectory, common })
        {
            token.ThrowIfCancellationRequested();
            var full = Canonical(item);
            RejectReparseAncestors(full);
            using var handle = CreateFile(full, 0, 7, IntPtr.Zero, 3, 0x02000000, IntPtr.Zero);
            if (handle.IsInvalid || !GetFileInformationByHandle(handle, out var info) ||
                !GetFileInformationByHandleEx(handle, 18, out var id, (uint)Marshal.SizeOf<FileIdentity>()))
                throw new IOException("RepositoryIdentityUnavailable");
            if ((id.High | id.Low) == 0) throw new IOException("RepositoryIdentityUnavailable");
            Append(full.ToUpperInvariant());
            Append($"{id.Volume:X16}:{id.High:X16}{id.Low:X16}:{info.CreatedHigh:X8}{info.CreatedLow:X8}");
        }
        // Detect linked-worktree redirection even if the pointer file is edited in place.
        if (!Directory.Exists(marker)) Append(ReadPointer(marker));
        if (File.Exists(commonFile)) { RejectReparseAncestors(commonFile); Append(ReadPointer(commonFile)); }
        return new RepositoryIdentity(root, Canonical(gitDirectory), Canonical(common), Convert.ToHexString(hash.GetHashAndReset()));

        void Append(string value)
        {
            var bytes = Encoding.UTF8.GetBytes(value);
            hash.AppendData(BitConverter.GetBytes(bytes.Length)); hash.AppendData(bytes);
        }
    }, token);

    private static string Canonical(string path)
    {
        if (!Path.IsPathFullyQualified(path)) throw new IOException("RepositoryIdentityUnavailable");
        var full = Path.TrimEndingDirectorySeparator(Path.GetFullPath(path));
        if (full.StartsWith(@"\\", StringComparison.Ordinal) || new DriveInfo(Path.GetPathRoot(full)!).DriveType != DriveType.Fixed)
            throw new IOException("RepositoryIdentityUnavailable");
        return full;
    }
    private static void RejectReparseAncestors(string path)
    {
        var ancestors = new Stack<string>();
        for (string? current = path; current is not null; current = Path.GetDirectoryName(current)) ancestors.Push(current);
        // Check from the drive down so a junction is rejected before probing anything beneath it.
        foreach (var current in ancestors)
            if (File.GetAttributes(current).HasFlag(FileAttributes.ReparsePoint)) throw new IOException("RepositoryIdentityUnavailable");
    }
    private static string ResolvePointer(string file, string prefix, string directory)
    {
        RejectReparseAncestors(file);
        var text = ReadPointer(file).TrimEnd('\r', '\n');
        if (!text.StartsWith(prefix, StringComparison.Ordinal) || text.Length == prefix.Length || text.Contains('\n') || text.Contains('\0'))
            throw new IOException("RepositoryIdentityUnavailable");
        return Path.GetFullPath(text[prefix.Length..], directory);
    }
    private static string ReadPointer(string file)
    {
        using var stream = File.OpenRead(file);
        if (stream.Length > 8192) throw new IOException("RepositoryIdentityUnavailable");
        var bytes = new byte[(int)stream.Length]; stream.ReadExactly(bytes);
        return new UTF8Encoding(false, true).GetString(bytes);
    }
    [StructLayout(LayoutKind.Sequential)]
    private struct FileInformation
    {
        public uint Attributes, CreatedLow, CreatedHigh, AccessedLow, AccessedHigh, WrittenLow, WrittenHigh,
            Volume, SizeHigh, SizeLow, Links, IndexHigh, IndexLow;
    }
    [StructLayout(LayoutKind.Sequential)]
    private struct FileIdentity { public ulong Volume, Low, High; }
    [DllImport("kernel32.dll", SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool GetFileInformationByHandleEx(SafeFileHandle file, int informationClass, out FileIdentity information, uint size);
    [DllImport("kernel32.dll", EntryPoint = "CreateFileW", CharSet = CharSet.Unicode, SetLastError = true)]
    private static extern SafeFileHandle CreateFile(string name, uint access, uint share, IntPtr security, uint disposition, uint flags, IntPtr template);
    [DllImport("kernel32.dll", SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool GetFileInformationByHandle(SafeFileHandle file, out FileInformation information);
}
