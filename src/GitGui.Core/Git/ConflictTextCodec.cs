using System.Text;

namespace GitGui.Core.Git;

public enum ConflictTextEncoding { Utf8, Utf8Bom, Utf16LittleEndianBom, Utf16BigEndianBom }

internal sealed record ConflictTextContent(string Text, ConflictTextEncoding Encoding);

internal static class ConflictTextCodec
{
    public static ConflictTextContent? Decode(ReadOnlySpan<byte> bytes)
    {
        if (bytes.Length > GitActions.MaximumConflictTextBytes ||
            bytes.StartsWith(new byte[] { 0xff, 0xfe, 0, 0 }) || bytes.StartsWith(new byte[] { 0, 0, 0xfe, 0xff })) return null;
        var kind = bytes.StartsWith(new byte[] { 0xff, 0xfe }) ? ConflictTextEncoding.Utf16LittleEndianBom :
            bytes.StartsWith(new byte[] { 0xfe, 0xff }) ? ConflictTextEncoding.Utf16BigEndianBom :
            bytes.StartsWith(new byte[] { 0xef, 0xbb, 0xbf }) ? ConflictTextEncoding.Utf8Bom : ConflictTextEncoding.Utf8;
        var encoding = Encoder(kind);
        try
        {
            // Strip exactly one transport BOM. A subsequent U+FEFF is file content.
            var text = encoding.GetString(bytes[encoding.Preamble.Length..]);
            return SupportedText(text) ? new(text, kind) : null;
        }
        catch (DecoderFallbackException) { return null; }
    }

    public static byte[] Encode(string text, ConflictTextEncoding kind)
    {
        if (!SupportedText(text)) throw new GitActionException("ConflictTextLimit");
        var encoding = Encoder(kind);
        try
        {
            var size = encoding.GetByteCount(text);
            if (size > GitActions.MaximumConflictTextBytes - encoding.Preamble.Length) throw new GitActionException("ConflictTextLimit");
            var bytes = new byte[size + encoding.Preamble.Length];
            encoding.Preamble.CopyTo(bytes);
            encoding.GetBytes(text, bytes.AsSpan(encoding.Preamble.Length));
            return bytes;
        }
        catch (EncoderFallbackException) { throw new GitActionException("ConflictTextLimit"); }
    }

    private static bool SupportedText(string text) => !text.Contains('\0') &&
        !text.StartsWith("version https://git-lfs.github.com/spec/v1", StringComparison.Ordinal) &&
        text.Count(c => c == '\n') <= 20000 && text.Split('\n').All(line => line.Length <= 16384);

    private static Encoding Encoder(ConflictTextEncoding kind) => kind switch
    {
        ConflictTextEncoding.Utf8 => new UTF8Encoding(false, true),
        ConflictTextEncoding.Utf8Bom => new UTF8Encoding(true, true),
        ConflictTextEncoding.Utf16LittleEndianBom => new UnicodeEncoding(false, true, true),
        ConflictTextEncoding.Utf16BigEndianBom => new UnicodeEncoding(true, true, true),
        _ => throw new GitActionException("ConflictTextLimit")
    };
}
