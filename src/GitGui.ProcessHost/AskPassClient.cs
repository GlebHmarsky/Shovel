using System.Globalization;
using System.IO.Pipes;
using System.Security.Cryptography;
using System.Text;
using GitGui.Core.Git;

namespace GitGui.ProcessHost;

public static class AskPassClient
{
    /// <summary>Invoked only by Git/OpenSSH. Stdout is the reply protocol, never diagnostics.</summary>
    public static async Task<int> RunAsync(string[] args)
    {
        byte[]? nonce = null, answer = null;
        try
        {
            if (args.Length != 1 || Encoding.UTF8.GetByteCount(args[0]) > AskPassProtocol.MaxPromptBytes) return 1;
            var name = Environment.GetEnvironmentVariable(AskPassProtocol.PipeVariable);
            var secret = Environment.GetEnvironmentVariable(AskPassProtocol.TokenVariable);
            if (name is null || !name.StartsWith("Shovel.AskPass.", StringComparison.Ordinal) || name.Length > 100 ||
                secret?.Length != 64 || !uint.TryParse(Environment.GetEnvironmentVariable(AskPassProtocol.ServerVariable),
                NumberStyles.None, CultureInfo.InvariantCulture, out var server)) return 1;
            nonce = Convert.FromHexString(secret);
            using var deadline = new CancellationTokenSource(TimeSpan.FromSeconds(125));
            using var pipe = new NamedPipeClientStream(".", name, PipeDirection.InOut,
                PipeOptions.Asynchronous | PipeOptions.CurrentUserOnly);
            await pipe.ConnectAsync(5000, deadline.Token).ConfigureAwait(false);
            if (!AskPassProtocol.GetNamedPipeServerProcessId(pipe.SafePipeHandle, out var actual) || server != actual) return 1;
            await AskPassProtocol.WriteFrame(pipe, nonce, deadline.Token).ConfigureAwait(false);
            var kind = Environment.GetEnvironmentVariable("SSH_ASKPASS_PROMPT") switch
            {
                "confirm" => AuthenticationPromptKind.Confirmation,
                "none" => AuthenticationPromptKind.Information,
                _ => AuthenticationPromptKind.Secret
            };
            var request = new byte[Encoding.UTF8.GetByteCount(args[0]) + 1];
            request[0] = (byte)kind; Encoding.UTF8.GetBytes(args[0], request.AsSpan(1));
            await AskPassProtocol.WriteFrame(pipe, request, deadline.Token).ConfigureAwait(false);
            var status = new byte[1]; await pipe.ReadExactlyAsync(status, deadline.Token).ConfigureAwait(false);
            if (status[0] != 1) return 1;
            answer = await AskPassProtocol.ReadFrame(pipe, AskPassProtocol.MaxAnswerBytes, deadline.Token).ConfigureAwait(false);
            using var stdout = Console.OpenStandardOutput();
            await stdout.WriteAsync(answer, deadline.Token).ConfigureAwait(false);
            await stdout.WriteAsync(new byte[] { 10 }, deadline.Token).ConfigureAwait(false);
            return 0;
        }
        catch (Exception exception) when (exception is IOException or OperationCanceledException or TimeoutException or
            UnauthorizedAccessException or FormatException or ArgumentException)
        { return 1; }
        finally
        {
            if (nonce is not null) CryptographicOperations.ZeroMemory(nonce);
            if (answer is not null) CryptographicOperations.ZeroMemory(answer);
        }
    }
}
