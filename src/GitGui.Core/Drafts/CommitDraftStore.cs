using System.Diagnostics;
using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
using GitGui.Core.Git;

namespace GitGui.Core.Drafts;

public sealed record CommitDraftText(string Subject, string Description)
{
    public static CommitDraftText Empty { get; } = new("", "");
    public bool IsEmpty => Subject.Length == 0 && Description.Length == 0;
}
public sealed record CommitDraftSnapshot(string? Revision, CommitDraftText Text, bool HasRecoveryCopies = false);
public enum CommitDraftFailure { Unavailable, Invalid, NewerSchema, Changed, IdentityChanged, TooLarge }
public enum CommitDraftOperation { Load, Save, Recovery }
public enum CommitDraftStage { ValidateIdentity, OpenDirectory, AcquireWriter, ReadRecord, WriteTemporary, PublishRecord, EnumerateRecovery, DeleteRecovery }
public sealed record CommitDraftDiagnostic(CommitDraftOperation Operation, CommitDraftStage Stage, int? NativeError, bool Retrying);
public sealed class CommitDraftException(CommitDraftFailure failure, CommitDraftDiagnostic? diagnostic = null) : IOException("CommitDraft" + failure)
{
    public CommitDraftFailure Failure { get; } = failure;
    public CommitDraftDiagnostic? Diagnostic { get; } = diagnostic;
}
public interface ICommitDraftStore
{
    string Location { get; }
    Task<CommitDraftSnapshot> LoadAsync(string worktree, string identity, CancellationToken token);
    Task<CommitDraftSnapshot> SaveAsync(string worktree, string identity, string? expectedRevision, CommitDraftText text, CancellationToken token);
    Task<bool> SaveRecoveryAsync(string worktree, string identity, string copyId, CommitDraftText text, CancellationToken token);
}

/// <summary>Local user text only. Independent of settings, Git and profiles; never logs content or exceptions.</summary>
public sealed class CommitDraftStore(string? directory, IRepositoryIdentityProvider identities, Action<CommitDraftDiagnostic>? diagnostic = null) : ICommitDraftStore
{
    // UI limits include messages not yet valid for Git's smaller UTF-8 limit.
    public const int MaximumSubjectCharacters = 16384, MaximumDescriptionCharacters = 65536;
    public const int MaximumFileBytes = 512 * 1024;
    public static string DefaultDirectory => Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "GitGui", "CommitDrafts");
    private readonly Lazy<string> _location = new(() => directory ?? DefaultDirectory);
    public string Location => _location.Value;
    private sealed record Document(int SchemaVersion, string Key, string Revision, string Subject, string Description);

    public Task<CommitDraftSnapshot> LoadAsync(string worktree, string identity, CancellationToken token) => Guard(CommitDraftOperation.Load, async attempt =>
    {
        await ValidateIdentityAsync(worktree, identity, attempt, token).ConfigureAwait(false);
        var key = Key(identity);
        var snapshot = await ReadAsync(MainPath(key), key, attempt, token).ConfigureAwait(false);
        attempt.Stage = CommitDraftStage.EnumerateRecovery;
        var hasCopies = Directory.Exists(Location) && Directory.EnumerateFiles(Location, key + ".*.recovery.json").Any();
        await ValidateIdentityAsync(worktree, identity, attempt, token).ConfigureAwait(false);
        return snapshot with { HasRecoveryCopies = hasCopies };
    }, token);

    public Task<CommitDraftSnapshot> SaveAsync(string worktree, string identity, string? expectedRevision, CommitDraftText text, CancellationToken token) => Guard(CommitDraftOperation.Save, async attempt =>
    {
        ValidateText(text);
        await ValidateIdentityAsync(worktree, identity, attempt, token).ConfigureAwait(false);
        var key = Key(identity);
        attempt.Stage = CommitDraftStage.OpenDirectory;
        Directory.CreateDirectory(Location);
        await using var gate = await AcquireAsync(key, attempt, token).ConfigureAwait(false);
        var current = await ReadAsync(MainPath(key), key, attempt, token).ConfigureAwait(false);
        if (current.Revision != expectedRevision) throw new CommitDraftException(CommitDraftFailure.Changed);
        var revision = Guid.NewGuid().ToString("N");
        await ReplaceAsync(MainPath(key), key, revision, text, worktree, identity, attempt, token).ConfigureAwait(false);
        // A revisioned empty record removes submitted text without missing-file ABA for older profiles.
        return new CommitDraftSnapshot(revision, text);
    }, token);

    public Task<bool> SaveRecoveryAsync(string worktree, string identity, string copyId, CommitDraftText text, CancellationToken token) => Guard(CommitDraftOperation.Recovery, async attempt =>
    {
        if (!Guid.TryParseExact(copyId, "N", out _)) throw new CommitDraftException(CommitDraftFailure.Invalid);
        ValidateText(text);
        await ValidateIdentityAsync(worktree, identity, attempt, token).ConfigureAwait(false);
        var key = Key(identity);
        attempt.Stage = CommitDraftStage.OpenDirectory;
        Directory.CreateDirectory(Location);
        await using var gate = await AcquireAsync(key, attempt, token).ConfigureAwait(false);
        var path = Path.Combine(Location, key + "." + copyId + ".recovery.json");
        // Do not replace corrupt/newer data, even in this writer's independent recovery slot.
        await ReadAsync(path, key, attempt, token).ConfigureAwait(false);
        await ValidateIdentityAsync(worktree, identity, attempt, token).ConfigureAwait(false);
        token.ThrowIfCancellationRequested();
        if (text.IsEmpty) { attempt.Stage = CommitDraftStage.DeleteRecovery; File.Delete(path); }
        else await ReplaceAsync(path, key, Guid.NewGuid().ToString("N"), text, worktree, identity, attempt, token).ConfigureAwait(false);
        attempt.Stage = CommitDraftStage.EnumerateRecovery;
        return Directory.EnumerateFiles(Location, key + ".*.recovery.json").Any();
    }, token);

    private string MainPath(string key) => Path.Combine(Location, key + ".json");
    private static string Key(string identity)
    {
        if (identity.Length != 64 || !identity.All(char.IsAsciiHexDigit)) throw new CommitDraftException(CommitDraftFailure.IdentityChanged);
        return Convert.ToHexString(SHA256.HashData(Encoding.UTF8.GetBytes(identity.ToUpperInvariant())));
    }
    private async Task ValidateIdentityAsync(string worktree, string expected, DraftAttempt attempt, CancellationToken token)
    {
        attempt.Stage = CommitDraftStage.ValidateIdentity;
        if ((await identities.ReadAsync(worktree, token).ConfigureAwait(false)).Fingerprint != expected)
            throw new CommitDraftException(CommitDraftFailure.IdentityChanged);
    }
    private async Task<FileStream> AcquireAsync(string key, DraftAttempt attempt, CancellationToken token)
    {
        attempt.Stage = CommitDraftStage.AcquireWriter;
        var time = Stopwatch.StartNew();
        while (true)
        {
            token.ThrowIfCancellationRequested();
            try { return new FileStream(Path.Combine(Location, key + ".lock"), FileMode.OpenOrCreate, FileAccess.ReadWrite, FileShare.None); }
            catch (IOException error) when (NativeError(error) is 32 or 33 && time.Elapsed < TimeSpan.FromSeconds(2))
            { await Task.Delay(50, token).ConfigureAwait(false); }
        }
    }
    private static async Task<CommitDraftSnapshot> ReadAsync(string path, string key, DraftAttempt attempt, CancellationToken token)
    {
        attempt.Stage = CommitDraftStage.ReadRecord;
        try
        {
            // Delete sharing lets replacement coexist with an immutable old read snapshot.
            await using var stream = new FileStream(path, FileMode.Open, FileAccess.Read, FileShare.Read | FileShare.Delete, 4096, FileOptions.Asynchronous);
            if (stream.Length > MaximumFileBytes) throw new CommitDraftException(CommitDraftFailure.Invalid);
            var bytes = new byte[checked((int)stream.Length)];
            await stream.ReadExactlyAsync(bytes, token).ConfigureAwait(false);
            using var json = JsonDocument.Parse(bytes, new JsonDocumentOptions { MaxDepth = 8 });
            if (json.RootElement.ValueKind != JsonValueKind.Object || !json.RootElement.TryGetProperty("SchemaVersion", out var schema) || schema.ValueKind != JsonValueKind.Number || !schema.TryGetInt32(out var version))
                throw new CommitDraftException(CommitDraftFailure.Invalid);
            if (version > 1) throw new CommitDraftException(CommitDraftFailure.NewerSchema);
            if (version != 1) throw new CommitDraftException(CommitDraftFailure.Invalid);
            var document = json.RootElement.Deserialize<Document>();
            if (document is null || document.Key != key || !Guid.TryParseExact(document.Revision, "N", out _) ||
                document.Subject is null || document.Description is null) throw new CommitDraftException(CommitDraftFailure.Invalid);
            var text = new CommitDraftText(document.Subject, document.Description);
            ValidateText(text);
            return new(document.Revision, text);
        }
        catch (FileNotFoundException) { return new(null, CommitDraftText.Empty); }
        catch (DirectoryNotFoundException) { return new(null, CommitDraftText.Empty); }
        catch (JsonException) { throw new CommitDraftException(CommitDraftFailure.Invalid); }
    }
    private static void ValidateText(CommitDraftText text)
    {
        if (text.Subject.Length > MaximumSubjectCharacters || text.Description.Length > MaximumDescriptionCharacters)
            throw new CommitDraftException(CommitDraftFailure.TooLarge);
        // An incomplete UTF-16 edit must not be silently replaced on disk.
        try { _ = new UTF8Encoding(false, true).GetByteCount(text.Subject); _ = new UTF8Encoding(false, true).GetByteCount(text.Description); }
        catch (EncoderFallbackException) { throw new CommitDraftException(CommitDraftFailure.Invalid); }
    }
    private async Task ReplaceAsync(string destination, string key, string revision, CommitDraftText text, string worktree, string identity, DraftAttempt attempt, CancellationToken token)
    {
        attempt.Stage = CommitDraftStage.WriteTemporary;
        var bytes = JsonSerializer.SerializeToUtf8Bytes(new Document(1, key, revision, text.Subject, text.Description));
        if (bytes.Length > MaximumFileBytes) throw new CommitDraftException(CommitDraftFailure.TooLarge);
        var temporary = Path.Combine(Location, key + "." + Guid.NewGuid().ToString("N") + ".tmp");
        try
        {
            await using (var stream = new FileStream(temporary, FileMode.CreateNew, FileAccess.Write, FileShare.None, 4096, FileOptions.Asynchronous | FileOptions.WriteThrough))
            {
                await stream.WriteAsync(bytes, token).ConfigureAwait(false);
                await stream.FlushAsync(token).ConfigureAwait(false);
                stream.Flush(flushToDisk: true);
            }
            await ValidateIdentityAsync(worktree, identity, attempt, token).ConfigureAwait(false);
            token.ThrowIfCancellationRequested();
            attempt.Stage = CommitDraftStage.PublishRecord;
            if (File.Exists(destination)) File.Replace(temporary, destination, null);
            else File.Move(temporary, destination, overwrite: false);
        }
        finally
        {
            try { File.Delete(temporary); }
            catch (IOException) { }
            catch (UnauthorizedAccessException) { }
        }
    }
    private sealed class DraftAttempt { public CommitDraftStage Stage { get; set; } = CommitDraftStage.OpenDirectory; }
    private static int? NativeError(Exception error) => error is System.ComponentModel.Win32Exception native ? native.NativeErrorCode :
        ((uint)error.HResult & 0xFFFF0000u) == 0x80070000u ? error.HResult & 0xFFFF : null;

    // Includes synchronous filesystem calls, parsing, serialization and identity validation.
    private Task<T> Guard<T>(CommitDraftOperation operation, Func<DraftAttempt, Task<T>> action, CancellationToken token) => Task.Run(async () =>
    {
        for (var retry = 0; ; retry++)
        {
            token.ThrowIfCancellationRequested();
            var attempt = new DraftAttempt();
            try { _ = Location; return await action(attempt).ConfigureAwait(false); }
            catch (CommitDraftException) { throw; }
            catch (Exception error) when (error is IOException or UnauthorizedAccessException or System.ComponentModel.Win32Exception or ArgumentException or NotSupportedException)
            {
                var code = NativeError(error);
                // A transient external reader can deny atomic publication. Retry the entire
                // operation after releasing handles/temp files, never just File.Replace:
                // another writer may have won the revision while this attempt backed off.
                // The writer gate already has its own bounded wait; do not extend it here.
                var retrying = retry < 3 && code is 32 or 33 && attempt.Stage != CommitDraftStage.AcquireWriter;
                var detail = new CommitDraftDiagnostic(operation, attempt.Stage, code, retrying);
                diagnostic?.Invoke(detail);
                if (!retrying) throw new CommitDraftException(CommitDraftFailure.Unavailable, detail);
                await Task.Delay(TimeSpan.FromMilliseconds(50 * (retry + 1)), token).ConfigureAwait(false);
            }
        }
    }, token);
}
