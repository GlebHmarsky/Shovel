using System.Globalization;
using System.IO.Pipes;
using System.Security.Cryptography;
using System.Text;
using GitGui.Core.Git;
using System.Runtime.CompilerServices;

[assembly: InternalsVisibleTo("GitGui.FoundationChecks")]

namespace GitGui.ProcessHost;

/// <summary>One explicit operation, one current-user pipe, one prompt at a time. No idle listener.</summary>
public sealed class AskPassSession : IAuthenticationSession
{
    private readonly CancellationTokenSource _stop;
    private readonly NamedPipeServerStream _pipe;
    private readonly byte[] _nonce = RandomNumberGenerator.GetBytes(32);
    private readonly Task _serve;
    private readonly TimeSpan _promptTimeout;
    public IReadOnlyDictionary<string, string?> Environment { get; }
    public CancellationToken CancellationToken => _stop.Token;
    public AuthenticationFailure? Failure { get; private set; }

    public AskPassSession(string executable, Func<AuthenticationPrompt, CancellationToken, Task<AuthenticationAnswer?>> prompt,
        CancellationToken token) : this(executable, prompt, token, TimeSpan.FromMinutes(2)) { }

    internal AskPassSession(string executable, Func<AuthenticationPrompt, CancellationToken, Task<AuthenticationAnswer?>> prompt,
        CancellationToken token, TimeSpan promptTimeout)
    {
        _promptTimeout = promptTimeout;
        if (!Path.IsPathFullyQualified(executable) || !File.Exists(executable))
            throw new GitActionException("ActionAskPassMissing");
        var name = "Shovel.AskPass." + Guid.NewGuid().ToString("N");
        _pipe = new(name, PipeDirection.InOut, 1, PipeTransmissionMode.Byte,
            PipeOptions.Asynchronous | PipeOptions.CurrentUserOnly | PipeOptions.FirstPipeInstance);
        _stop = CancellationTokenSource.CreateLinkedTokenSource(token);
        Environment = new Dictionary<string, string?>
        {
            ["GIT_ASKPASS"] = executable, ["SSH_ASKPASS"] = executable,
            ["SSH_ASKPASS_REQUIRE"] = "force", ["SSH_ASKPASS_PROMPT"] = null,
            [AskPassProtocol.PipeVariable] = name, [AskPassProtocol.TokenVariable] = Convert.ToHexString(_nonce),
            [AskPassProtocol.ServerVariable] = System.Environment.ProcessId.ToString(CultureInfo.InvariantCulture)
        };
        _serve = Task.Run(() => ServeAsync(prompt));
    }

    private async Task ServeAsync(Func<AuthenticationPrompt, CancellationToken, Task<AuthenticationAnswer?>> prompt)
    {
        try
        {
            for (var count = 0; count <= 16; count++)
            {
                await _pipe.WaitForConnectionAsync(_stop.Token).ConfigureAwait(false);
                if (count == 16) { Failure = AuthenticationFailure.PromptLimit; break; }
                using var connection = CancellationTokenSource.CreateLinkedTokenSource(_stop.Token);
                connection.CancelAfter(_promptTimeout);
                var token = connection.Token;
                var nonce = await AskPassProtocol.ReadFrame(_pipe, 32, token).ConfigureAwait(false);
                try { if (!CryptographicOperations.FixedTimeEquals(nonce, _nonce)) throw new IOException(); }
                finally { CryptographicOperations.ZeroMemory(nonce); }
                var request = await AskPassProtocol.ReadFrame(_pipe, AskPassProtocol.MaxPromptBytes + 1, token).ConfigureAwait(false);
                if (request.Length == 0 || request[0] > (int)AuthenticationPromptKind.Information) throw new IOException();
                var message = new UTF8Encoding(false, true).GetString(request, 1, request.Length - 1);
                // Observe helper exit (Git timeout/cancellation, including SSH touch notifications).
                var disconnected = _pipe.ReadAsync(new byte[1], token).AsTask();
                var answerTask = prompt(new((AuthenticationPromptKind)request[0], message), token);
                var peerClosed = await Task.WhenAny(answerTask, disconnected).ConfigureAwait(false) == disconnected;
                if (peerClosed) connection.Cancel();
                AuthenticationAnswer? answer = null;
                try
                {
                    try { answer = await answerTask.ConfigureAwait(false); }
                    catch (OperationCanceledException) when (peerClosed && request[0] == (int)AuthenticationPromptKind.Information) { }
                    // OpenSSH kills its informational askpass notifier after a security-key touch.
                    // That closes this prompt, but must not cancel the still-running SSH operation.
                    if (!(peerClosed && request[0] == (int)AuthenticationPromptKind.Information && !_stop.IsCancellationRequested))
                    {
                        token.ThrowIfCancellationRequested();
                        if (answer is not null && (answer.Bytes.Length > AskPassProtocol.MaxAnswerBytes ||
                            answer.Bytes.Any(value => value is 0 or 10 or 13))) throw new IOException();
                        await _pipe.WriteAsync(new byte[] { answer is null ? (byte)0 : (byte)1 }, token).ConfigureAwait(false);
                        if (answer is not null) await AskPassProtocol.WriteFrame(_pipe, answer.Bytes, token).ConfigureAwait(false);
                        if (answer is null) _stop.Cancel();
                    }
                }
                finally
                {
                    answer?.Dispose(); connection.Cancel();
                    try { await disconnected.ConfigureAwait(false); } catch (OperationCanceledException) { }
                }
                _pipe.Disconnect();
            }
        }
        catch (OperationCanceledException) { if (!_stop.IsCancellationRequested) Failure = AuthenticationFailure.TimedOut; }
        catch (Exception exception) when (exception is IOException or UnauthorizedAccessException or DecoderFallbackException or InvalidOperationException)
        { if (!_stop.IsCancellationRequested) Failure = AuthenticationFailure.Unavailable; /* No payload enters diagnostics. */ }
        finally { _stop.Cancel(); _pipe.Dispose(); }
    }

    public async ValueTask DisposeAsync()
    {
        _stop.Cancel();
        try { await _serve.ConfigureAwait(false); }
        finally { _pipe.Dispose(); _stop.Dispose(); CryptographicOperations.ZeroMemory(_nonce); }
    }
}
