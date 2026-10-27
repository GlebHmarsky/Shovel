using System.IO;
using System.Text;
using GitGui.Core.Git;
using GitGui.Core.Processes;
using GitGui.ProcessHost;

namespace GitGui.FoundationChecks;

internal static partial class Program
{
    private static string AskPassExecutable => Path.Combine(AppContext.BaseDirectory, "Shovel.AskPass.exe");
    private static async Task RunAuthenticationChecksAsync()
    {
        var runner = new WindowsProcessRunner();
        var path = Fixture("askpass — 日本語 spaces");
        // Real deployed helper, including a Unicode/space path; no desktop window or network.
        foreach (var file in new[] { "Shovel.AskPass.exe", "Shovel.AskPass.dll", "Shovel.AskPass.deps.json", "Shovel.AskPass.runtimeconfig.json", "GitGui.Core.dll", "GitGui.ProcessHost.dll" })
            File.Copy(Path.Combine(AppContext.BaseDirectory, file), Path.Combine(path, file));
        var helper = Path.Combine(path, "Shovel.AskPass.exe");
        await Check("Missing askpass helper fails before creating an authentication endpoint", () =>
        {
            try
            {
                _ = new AskPassSession(Path.Combine(path, "missing.exe"), (_, _) => Task.FromResult<AuthenticationAnswer?>(null), default);
                throw new InvalidOperationException("Expected missing helper rejection");
            }
            catch (GitActionException exception) { Expect(exception.Code == "ActionAskPassMissing", "Explicit repair guidance"); }
            return Task.CompletedTask;
        });
        ProcessRequest Request(IAuthenticationSession session, string message = "fixture prompt", string? hint = null) =>
            new(helper, path, [message]) { Environment = new(session.Environment) { ["SSH_ASKPASS_PROMPT"] = hint }, Timeout = TimeSpan.FromSeconds(15) };

        await Check("Askpass helper round-trips Unicode privately and clears broker reply buffers", async () =>
        {
            byte[]? reply = null; var kinds = new List<AuthenticationPromptKind>();
            await using var session = new AskPassSession(helper, (prompt, _) =>
            {
                kinds.Add(prompt.Kind); Expect(prompt.Message == "fixture prompt", "Prompt preserved as plain data");
                reply = Encoding.UTF8.GetBytes("fixture-שלום-日本語"); return Task.FromResult<AuthenticationAnswer?>(new(reply));
            }, default);
            foreach (var hint in new string?[] { null, "confirm" })
            {
                var result = await runner.RunAsync(Request(session, hint: hint), default);
                Expect(result.ExitCode == 0 && Encoding.UTF8.GetString(result.StandardOutput) == "fixture-שלום-日本語\n" && result.StandardError.Length == 0,
                    "Only reply protocol reaches helper stdout; stderr is empty");
                Expect(reply!.All(value => value == 0), "Broker releases response bytes after send");
            }
            Expect(kinds.SequenceEqual(new[] { AuthenticationPromptKind.Secret, AuthenticationPromptKind.Confirmation }), "OpenSSH hint classification");
        });
        await Check("Askpass rejects incorrect session token/server and oversize prompts without requesting input", async () =>
        {
            foreach (var failure in new[] { "token", "server", "size" })
            {
                var prompts = 0;
                await using var session = new AskPassSession(helper, (_, _) => { prompts++; return Task.FromResult<AuthenticationAnswer?>(null); }, default);
                var request = Request(session, failure == "size" ? new string('x', 16385) : "fixture prompt");
                if (failure == "token") request.Environment["SHOVEL_ASKPASS_TOKEN"] = new string('0', 64);
                if (failure == "server") request.Environment["SHOVEL_ASKPASS_SERVER"] = "0";
                var result = await runner.RunAsync(request, default);
                Expect(result.ExitCode == 1 && result.StandardOutput.Length == 0 && result.StandardError.Length == 0 && prompts == 0, "Fail closed, no prompt or diagnostics");
            }
        });
        await Check("Prompt cancellation and invalid multiline replies stop authentication without returning a secret", async () =>
        {
            foreach (var answer in new[] { "cancel", "multiline", "oversize" })
            {
                await using var session = new AskPassSession(helper, (_, _) => Task.FromResult<AuthenticationAnswer?>(answer == "cancel" ? null :
                    new(Encoding.UTF8.GetBytes(answer == "multiline" ? "secret\nextra" : new string('x', 4097)))), default);
                var result = await runner.RunAsync(Request(session), default);
                Expect(result.ExitCode == 1 && result.StandardOutput.Length == 0 && session.CancellationToken.IsCancellationRequested, "No partial secret accepted");
            }
        });
        await Check("Owned helper timeout closes pending input and stale endpoints cannot be reused", async () =>
        {
            var started = new TaskCompletionSource(); var closed = new TaskCompletionSource();
            var session = new AskPassSession(helper, async (_, token) =>
            {
                started.TrySetResult();
                try { await Task.Delay(Timeout.Infinite, token); return null; }
                finally { closed.TrySetResult(); }
            }, default);
            var request = Request(session); using var cancel = new CancellationTokenSource();
            var pending = runner.RunAsync(request, cancel.Token);
            await started.Task.WaitAsync(TimeSpan.FromSeconds(5)); cancel.Cancel();
            Expect((await pending).Outcome == ProcessOutcome.Cancelled, "Runner stops owned helper");
            await closed.Task.WaitAsync(TimeSpan.FromSeconds(3));
            await session.DisposeAsync();
            var stale = await runner.RunAsync(request, default);
            Expect(stale.ExitCode == 1 && stale.StandardOutput.Length == 0, "Expired endpoint cannot provide a reply");
        });
        await Check("Authentication deadline and prompt-count limits cancel with distinct failure reasons", async () =>
        {
            await using (var expired = new AskPassSession(helper, async (_, token) =>
            { await Task.Delay(Timeout.Infinite, token); return null; }, default, TimeSpan.FromMilliseconds(300)))
            {
                var result = await runner.RunAsync(Request(expired), default);
                Expect(result.ExitCode == 1 && expired.Failure == AuthenticationFailure.TimedOut, "Expired prompt is distinct from user cancellation");
            }
            await using var limited = new AskPassSession(helper, (_, _) => Task.FromResult<AuthenticationAnswer?>(new("reply"u8.ToArray())), default);
            for (var i = 0; i < 16; i++) Expect((await runner.RunAsync(Request(limited), default)).ExitCode == 0, "First sixteen requests complete");
            Expect((await runner.RunAsync(Request(limited), default)).ExitCode == 1, "Seventeenth request is rejected");
            Expect(limited.CancellationToken.IsCancellationRequested && limited.Failure == AuthenticationFailure.PromptLimit, "Bound repeated requests even if each prompt completes");
        });
        await Check("OpenSSH informational helper exit dismisses notification without cancelling its operation", async () =>
        {
            var started = new TaskCompletionSource(); var closed = new TaskCompletionSource();
            await using var session = new AskPassSession(helper, async (prompt, token) =>
            {
                if (prompt.Kind != AuthenticationPromptKind.Information) return new("next"u8.ToArray());
                started.TrySetResult();
                try { await Task.Delay(Timeout.Infinite, token); return null; }
                finally { closed.TrySetResult(); }
            }, default);
            using var cancel = new CancellationTokenSource();
            var pending = runner.RunAsync(Request(session, hint: "none"), cancel.Token);
            await started.Task.WaitAsync(TimeSpan.FromSeconds(5)); cancel.Cancel(); await pending;
            await closed.Task.WaitAsync(TimeSpan.FromSeconds(3));
            var next = await runner.RunAsync(Request(session), default);
            Expect(!session.CancellationToken.IsCancellationRequested && next.ExitCode == 0, "Touch notification lifecycle is independent of SSH completion");
        });
    }
}
