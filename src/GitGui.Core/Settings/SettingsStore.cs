using System.Text.Json;

namespace GitGui.Core.Settings;

public enum SettingsLoadStatus { Defaults, Loaded, Migrated, Invalid, NewerSchema, UnsupportedSchema, Unavailable }
public sealed record SettingsLoadResult(AppSettings Settings, SettingsLoadStatus Status)
{
    public bool CanSave => Status is SettingsLoadStatus.Defaults or SettingsLoadStatus.Loaded or SettingsLoadStatus.Migrated;
}

/// <summary>Single profile writer, owned by the application's instance lock. Never overwrites unknown data.</summary>
public sealed class SettingsStore(string directory)
{
    private const long MaximumBytes = 64 * 1024;
    private static readonly JsonSerializerOptions JsonOptions = new() { WriteIndented = true };
    public string FilePath { get; } = Path.Combine(directory, "settings.json");
    private SettingsLoadStatus? _loadedStatus;
    private byte[]? _loadedBytes;

    public async Task<SettingsLoadResult> LoadAsync(CancellationToken cancellationToken)
    {
        cancellationToken.ThrowIfCancellationRequested();
        try
        {
            _loadedBytes = await ReadBoundedAsync(cancellationToken).ConfigureAwait(false);
            if (_loadedBytes is null)
                return Complete(new(), SettingsLoadStatus.Defaults);

            using var document = JsonDocument.Parse(_loadedBytes);
            if (document.RootElement.ValueKind != JsonValueKind.Object ||
                !document.RootElement.TryGetProperty(nameof(AppSettings.SchemaVersion), out var version) ||
                version.ValueKind != JsonValueKind.Number ||
                !version.TryGetInt32(out var schema))
                return Complete(new(), SettingsLoadStatus.Invalid);
            if (schema > AppSettings.CurrentSchemaVersion)
                return Complete(new(), SettingsLoadStatus.NewerSchema);
            if (schema is not (1 or 2 or 3 or AppSettings.CurrentSchemaVersion))
                return Complete(new(), SettingsLoadStatus.UnsupportedSchema);

            var settings = JsonSerializer.Deserialize<AppSettings>(_loadedBytes, JsonOptions);
            // Former approval fingerprints remain useful as replacement-detection stamps, not consent.
            if (schema == 3 && settings is not null && document.RootElement.TryGetProperty("Approvals", out var legacy))
                settings = settings with { RepositoryIdentities = legacy.Deserialize<GitGui.Core.Git.RepositoryIdentityStamp[]>(JsonOptions) ?? [] };
            if (schema < 3 && settings is not null) settings = settings with { RepositoryIdentities = [], RepositoryPreferences = [] };
            return settings is null ? Complete(new(), SettingsLoadStatus.Invalid) :
                Complete(settings.Normalize() with { SchemaVersion = AppSettings.CurrentSchemaVersion },
                    schema == AppSettings.CurrentSchemaVersion ? SettingsLoadStatus.Loaded : SettingsLoadStatus.Migrated);
        }
        catch (JsonException) { return Complete(new(), SettingsLoadStatus.Invalid); }
        catch (InvalidDataException) { return Complete(new(), SettingsLoadStatus.Invalid); }
        catch (IOException) { return Complete(new(), SettingsLoadStatus.Unavailable); }
        catch (UnauthorizedAccessException) { return Complete(new(), SettingsLoadStatus.Unavailable); }
    }

    public async Task SaveAsync(AppSettings settings, CancellationToken cancellationToken)
    {
        cancellationToken.ThrowIfCancellationRequested();
        if (_loadedStatus is not (SettingsLoadStatus.Defaults or SettingsLoadStatus.Loaded or SettingsLoadStatus.Migrated))
            throw new InvalidOperationException("SettingsNotWritable");

        // Detect external edits since startup rather than silently replacing them.
        var current = await ReadBoundedAsync(cancellationToken).ConfigureAwait(false);
        if (!(current is null && _loadedBytes is null) &&
            (current is null || _loadedBytes is null || !current.AsSpan().SequenceEqual(_loadedBytes)))
            throw new IOException("SettingsChangedExternally");

        Directory.CreateDirectory(directory);
        var temporary = Path.Combine(directory, $"settings.{Guid.NewGuid():N}.tmp");
        var bytes = JsonSerializer.SerializeToUtf8Bytes(settings.Normalize() with
        { SchemaVersion = AppSettings.CurrentSchemaVersion }, JsonOptions);
        if (bytes.Length > MaximumBytes) throw new IOException("SettingsTooLarge");
        try
        {
            await using (var stream = new FileStream(temporary, FileMode.CreateNew, FileAccess.Write,
                FileShare.None, 4096, FileOptions.Asynchronous | FileOptions.WriteThrough))
            {
                await stream.WriteAsync(bytes, cancellationToken).ConfigureAwait(false);
                await stream.FlushAsync(cancellationToken).ConfigureAwait(false);
            }
            cancellationToken.ThrowIfCancellationRequested();
            if (current is null) File.Move(temporary, FilePath, overwrite: false);
            else File.Replace(temporary, FilePath, FilePath + ".bak");
            _loadedBytes = bytes;
        }
        finally
        {
            try { File.Delete(temporary); }
            catch (IOException) { }
            catch (UnauthorizedAccessException) { }
        }
    }

    private async Task<byte[]?> ReadBoundedAsync(CancellationToken cancellationToken)
    {
        try
        {
            await using var stream = new FileStream(FilePath, FileMode.Open, FileAccess.Read,
                FileShare.Read, 4096, FileOptions.Asynchronous);
            if (stream.Length > MaximumBytes) throw new InvalidDataException("SettingsTooLarge");
            var bytes = new byte[checked((int)stream.Length)];
            await stream.ReadExactlyAsync(bytes, cancellationToken).ConfigureAwait(false);
            return bytes;
        }
        catch (FileNotFoundException) { return null; }
        catch (DirectoryNotFoundException) { return null; }
    }

    private SettingsLoadResult Complete(AppSettings settings, SettingsLoadStatus status)
    {
        _loadedStatus = status;
        return new(settings, status);
    }
}
