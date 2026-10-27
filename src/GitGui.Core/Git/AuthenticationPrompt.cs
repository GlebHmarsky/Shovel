using System.Security.Cryptography;

namespace GitGui.Core.Git;

public enum AuthenticationPromptKind { Secret, Confirmation, Information }
public enum AuthenticationFailure { Unavailable, TimedOut, PromptLimit }
public sealed record AuthenticationPrompt(AuthenticationPromptKind Kind, string Message);

/// <summary>Single-use UTF-8 response. Ownership transfers to the broker; never log or persist it.</summary>
public sealed class AuthenticationAnswer(byte[] bytes) : IDisposable
{
    public byte[] Bytes { get; } = bytes;
    public void Dispose() => CryptographicOperations.ZeroMemory(Bytes);
}

public interface IAuthenticationSession : IAsyncDisposable
{
    IReadOnlyDictionary<string, string?> Environment { get; }
    CancellationToken CancellationToken { get; }
    AuthenticationFailure? Failure { get; }
}
