using System.Buffers.Binary;
using System.Runtime.InteropServices;
using Microsoft.Win32.SafeHandles;

namespace GitGui.ProcessHost;

internal static class AskPassProtocol
{
    public const int MaxPromptBytes = 16 * 1024;
    public const int MaxAnswerBytes = 4096;
    public const string PipeVariable = "SHOVEL_ASKPASS_PIPE";
    public const string TokenVariable = "SHOVEL_ASKPASS_TOKEN";
    public const string ServerVariable = "SHOVEL_ASKPASS_SERVER";

    public static async Task<byte[]> ReadFrame(Stream stream, int maximum, CancellationToken token)
    {
        var header = new byte[4];
        await stream.ReadExactlyAsync(header, token).ConfigureAwait(false);
        var length = BinaryPrimitives.ReadInt32LittleEndian(header);
        if (length < 0 || length > maximum) throw new IOException();
        var bytes = new byte[length];
        try { await stream.ReadExactlyAsync(bytes, token).ConfigureAwait(false); return bytes; }
        catch { System.Security.Cryptography.CryptographicOperations.ZeroMemory(bytes); throw; }
    }

    public static async Task WriteFrame(Stream stream, ReadOnlyMemory<byte> bytes, CancellationToken token)
    {
        var header = new byte[4];
        BinaryPrimitives.WriteInt32LittleEndian(header, bytes.Length);
        await stream.WriteAsync(header, token).ConfigureAwait(false);
        await stream.WriteAsync(bytes, token).ConfigureAwait(false);
        await stream.FlushAsync(token).ConfigureAwait(false);
    }

    [DllImport("kernel32.dll", SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    public static extern bool GetNamedPipeServerProcessId(SafePipeHandle pipe, out uint processId);
}
