using System.Diagnostics;

namespace GitGui.Core.Diagnostics;

public enum TimingStage { Shell, Settings, Identity, Discovery, Inspection, References, History, Parsing, Graph, Formatting, Binding }
public sealed record TimingSample(TimingStage Stage, double Milliseconds);

/// <summary>Bounded in-memory timings only: no paths, arguments, output or identities.</summary>
public sealed class PerformanceTrace
{
    private readonly Queue<TimingSample> _samples = new();
    public IReadOnlyList<TimingSample> Snapshot() { lock (_samples) return _samples.ToArray(); }
    public IDisposable Measure(TimingStage stage) => new Measurement(this, stage);
    private sealed class Measurement(PerformanceTrace owner, TimingStage stage) : IDisposable
    {
        private readonly long _start = Stopwatch.GetTimestamp();
        public void Dispose()
        {
            lock (owner._samples)
            {
                while (owner._samples.Count >= 64) owner._samples.Dequeue();
                owner._samples.Enqueue(new(stage, Stopwatch.GetElapsedTime(_start).TotalMilliseconds));
            }
        }
    }
}
