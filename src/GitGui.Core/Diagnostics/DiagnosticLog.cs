using System.Text.Json;
using System.Threading.Channels;

namespace GitGui.Core.Diagnostics;

public enum DiagnosticEvent { Started, SettingsLoaded, SettingsReadOnly, SettingsSaved, SettingsSaveFailed, FatalError, Stopped }
public enum FailureKind { None, AccessDenied, Io, InvalidData, Cancelled, Unexpected }

/// <summary>Only allowlisted codes, never messages, paths, arguments or repository data.</summary>
public sealed class DiagnosticLog : IAsyncDisposable
{
    private const long MaximumFileBytes = 256 * 1024;
    private readonly Channel<Entry> _entries = Channel.CreateBounded<Entry>(new BoundedChannelOptions(128)
    { SingleReader = true, FullMode = BoundedChannelFullMode.DropWrite });
    private readonly Task _writer;
    public bool HasFailed => Volatile.Read(ref _failed) != 0;
    private int _failed;

    public DiagnosticLog(string directory) => _writer = Task.Run(() => WriteAsync(directory));

    public void Record(DiagnosticEvent code, Exception? exception = null)
    {
        if (!Enum.IsDefined(code)) throw new ArgumentOutOfRangeException(nameof(code));
        var failure = exception switch
        {
            null => FailureKind.None,
            UnauthorizedAccessException => FailureKind.AccessDenied,
            InvalidDataException or JsonException => FailureKind.InvalidData,
            IOException => FailureKind.Io,
            OperationCanceledException => FailureKind.Cancelled,
            _ => FailureKind.Unexpected
        };
        _entries.Writer.TryWrite(new(DateTimeOffset.UtcNow, code.ToString(), failure.ToString()));
    }

    public async ValueTask DisposeAsync()
    {
        _entries.Writer.TryComplete();
        await _writer.ConfigureAwait(false);
    }

    private async Task WriteAsync(string directory)
    {
        try
        {
            Directory.CreateDirectory(directory);
            var file = Path.Combine(directory, "diagnostics.jsonl");
            await foreach (var entry in _entries.Reader.ReadAllAsync().ConfigureAwait(false))
            {
                if (File.Exists(file) && new FileInfo(file).Length >= MaximumFileBytes)
                    File.Move(file, file + ".1", overwrite: true);
                await File.AppendAllTextAsync(file, JsonSerializer.Serialize(entry) + Environment.NewLine)
                    .ConfigureAwait(false);
            }
        }
        catch (IOException) { Interlocked.Exchange(ref _failed, 1); }
        catch (UnauthorizedAccessException) { Interlocked.Exchange(ref _failed, 1); }
    }

    private sealed record Entry(DateTimeOffset TimeUtc, string Event, string Failure);
}
