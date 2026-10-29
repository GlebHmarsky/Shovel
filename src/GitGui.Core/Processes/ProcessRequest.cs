namespace GitGui.Core.Processes;

public sealed record ProcessRequest(string Executable, string WorkingDirectory, string[] Arguments)
{
    public Dictionary<string, string?> Environment { get; init; } = new(StringComparer.OrdinalIgnoreCase);
    public TimeSpan Timeout { get; init; } = TimeSpan.FromSeconds(15);
    public int MaximumOutputBytes { get; init; } = 256 * 1024;
    // Optional bounded, caller-owned input file. EOF remains the default for interactive programs.
    public string? StandardInputFile { get; init; }
}

public enum ProcessOutcome { Exited, Cancelled, TimedOut, OutputLimit, StartFailed }
public sealed record ProcessOutput(bool IsError, byte[] Bytes);
public sealed record ProcessResult(ProcessOutcome Outcome, int? ExitCode, byte[] StandardOutput,
    byte[] StandardError, TimeSpan Duration);

public interface IProcessRunner
{
    // Output callbacks run off the UI thread, may arrive concurrently, and must not block.
    Task<ProcessResult> RunAsync(ProcessRequest request, CancellationToken cancellationToken,
        Action<ProcessOutput>? output = null);
}
