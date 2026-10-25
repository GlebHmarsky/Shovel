using System.ComponentModel;
using System.Diagnostics;
using GitGui.Core.Processes;

namespace GitGui.ProcessHost;

public sealed class WindowsProcessRunner : IProcessRunner
{
    private readonly SemaphoreSlim _slots = new(2, 2);

    public Task<ProcessResult> RunAsync(ProcessRequest request, CancellationToken cancellationToken,
        Action<ProcessOutput>? output = null) => Task.Run(() => RunWorkerAsync(request, cancellationToken, output));

    private async Task<ProcessResult> RunWorkerAsync(ProcessRequest request, CancellationToken cancellationToken,
        Action<ProcessOutput>? output)
    {
        if (request.MaximumOutputBytes is < 1 or > 16 * 1024 * 1024 || request.Timeout <= TimeSpan.Zero ||
            request.Timeout > TimeSpan.FromMinutes(10)) throw new ArgumentOutOfRangeException(nameof(request));
        var watch = Stopwatch.StartNew();
        try { await _slots.WaitAsync(cancellationToken).ConfigureAwait(false); }
        catch (OperationCanceledException) { return new(ProcessOutcome.Cancelled, null, [], [], watch.Elapsed); }
        using var timeout = new CancellationTokenSource(request.Timeout);
        using var limit = new CancellationTokenSource();
        using var stop = CancellationTokenSource.CreateLinkedTokenSource(cancellationToken, timeout.Token, limit.Token);
        using var stdout = new MemoryStream();
        using var stderr = new MemoryStream();
        NativeChildProcess? child = null;
        ProcessJob? job = null;
        Task? drains = null;
        var drainFailed = 0;
        try
        {
            stop.Token.ThrowIfCancellationRequested();
            job = new ProcessJob();
            child = NativeChildProcess.Start(request, job);
            drains = Task.WhenAll(DrainAsync(child.StandardOutput, stdout, false),
                DrainAsync(child.StandardError, stderr, true));
            await child.Completion.WaitAsync(stop.Token).ConfigureAwait(false);
            // Close the job after exit, including any descendant that retained a pipe.
            job.Dispose();
            await drains.WaitAsync(stop.Token).ConfigureAwait(false);
            if (limit.IsCancellationRequested) return Result(drainFailed != 0 ? ProcessOutcome.StartFailed : ProcessOutcome.OutputLimit, null);
            return Result(ProcessOutcome.Exited, child.ExitCode);
        }
        catch (OperationCanceledException)
        {
            job?.Dispose();
            await FinishAsync().ConfigureAwait(false);
            return Result(cancellationToken.IsCancellationRequested ? ProcessOutcome.Cancelled :
                limit.IsCancellationRequested ? (drainFailed != 0 ? ProcessOutcome.StartFailed : ProcessOutcome.OutputLimit) : ProcessOutcome.TimedOut, null);
        }
        catch (Exception exception) when (exception is Win32Exception or IOException or UnauthorizedAccessException)
        {
            job?.Dispose();
            await FinishAsync().ConfigureAwait(false);
            return Result(ProcessOutcome.StartFailed, null);
        }
        finally
        {
            try { job?.Dispose(); await FinishAsync().ConfigureAwait(false); }
            finally { child?.Dispose(); _slots.Release(); }
        }

        ProcessResult Result(ProcessOutcome outcome, int? code) => new(outcome, code, stdout.ToArray(), stderr.ToArray(), watch.Elapsed);

        async Task FinishAsync()
        {
            if (child is not null) await child.Completion.ConfigureAwait(false);
            if (drains is not null) await drains.ConfigureAwait(false);
        }

        async Task DrainAsync(Stream source, MemoryStream destination, bool isError)
        {
            try
            {
                var buffer = new byte[8192];
                int read;
                while ((read = await source.ReadAsync(buffer).ConfigureAwait(false)) != 0)
                {
                    var keep = Math.Min(read, request.MaximumOutputBytes - (int)destination.Length);
                    if (keep > 0)
                    {
                        destination.Write(buffer, 0, keep);
                        output?.Invoke(new(isError, buffer.AsSpan(0, keep).ToArray()));
                    }
                    if (keep < read) limit.Cancel();
                }
            }
            catch (Exception) { Interlocked.Exchange(ref drainFailed, 1); limit.Cancel(); }
        }
    }
}
