namespace GitGui.Core.Processes;

/// <summary>A submitted user operation owns its processes until they exit, independently of views.</summary>
public static class ProcessLifetime
{
    private static readonly AsyncLocal<bool> Finish = new();
    public static bool FinishStarted => Finish.Value;
    public static IDisposable CompleteStartedCommands(bool enabled = true)
    {
        var previous = Finish.Value; Finish.Value = enabled; return new Scope(previous);
    }
    private sealed class Scope(bool previous) : IDisposable { public void Dispose() => Finish.Value = previous; }
}
