using System.ComponentModel;
using System.Diagnostics;
using System.Runtime.InteropServices;
using Microsoft.Win32.SafeHandles;

namespace GitGui.FoundationChecks;

internal sealed record HistoryPerformanceResources(double ElapsedMs, int IntervalMs, int Samples,
    long HostPrivatePeakBytes, long HostWorkingSetPeakBytes, long ChildrenPrivatePeakBytes,
    long ChildrenWorkingSetPeakBytes, long CombinedPrivatePeakBytes, int ObservedChildren,
    int LastObservedLiveChildren, int UnavailableSamples, double HostCpuMs, double ObservedChildrenCpuMs,
    double HostTotalMachineCpuPercent, double ObservedChildrenTotalMachineCpuPercent);

/// <summary>
/// Samples the headless host and its observed descendants, never unrelated Git processes by name.
/// Polling can miss short-lived processes and peaks. These are sampled lower bounds, not Job peaks.
/// No process is killed here. Production runner Job Objects own read cancellation.
/// </summary>
internal sealed class HistoryPerformanceSampler : IAsyncDisposable
{
    private readonly CancellationTokenSource _stop = new();
    private readonly Stopwatch _watch = Stopwatch.StartNew();
    private readonly Process _host = Process.GetCurrentProcess();
    private readonly Dictionary<(int Pid, long Start), double> _childrenCpu = [];
    private readonly Dictionary<(int Pid, long Start), double> _childrenBaselineCpu = [];
    private readonly long _startedUtcTicks = DateTime.UtcNow.Ticks;
    private readonly int _intervalMs;
    private readonly double _hostStartCpu;
    private readonly Task _loop;
    private int _samples, _unavailable, _live;
    private long _hostPrivate, _hostWorking, _childPrivate, _childWorking, _combined;
    private double _hostCpu;
    public HistoryPerformanceSampler(int intervalMs = 50)
    {
        if (intervalMs < 10) throw new ArgumentOutOfRangeException(nameof(intervalMs));
        _intervalMs = intervalMs;
        _hostStartCpu = _host.TotalProcessorTime.TotalMilliseconds;
        _loop = Task.Run(async () =>
        {
            try
            {
                while (!_stop.IsCancellationRequested)
                {
                    Sample();
                    await Task.Delay(_intervalMs, _stop.Token).ConfigureAwait(false);
                }
            }
            catch (OperationCanceledException) when (_stop.IsCancellationRequested) { }
        });
    }
    public async Task<HistoryPerformanceResources> FinishAsync()
    {
        _stop.Cancel(); await _loop.ConfigureAwait(false);
        Sample(); _watch.Stop();
        var childCpu = _childrenCpu.Values.Sum();
        return new(_watch.Elapsed.TotalMilliseconds, _intervalMs, _samples, _hostPrivate, _hostWorking,
            _childPrivate, _childWorking, _combined, _childrenCpu.Count, _live, _unavailable, _hostCpu, childCpu,
            TotalMachinePercent(_hostCpu, _watch.Elapsed.TotalMilliseconds, Environment.ProcessorCount),
            TotalMachinePercent(childCpu, _watch.Elapsed.TotalMilliseconds, Environment.ProcessorCount));
    }
    public static double TotalMachinePercent(double cpuMs, double elapsedMs, int logicalProcessors) =>
        elapsedMs > 0 && logicalProcessors > 0 ? cpuMs / elapsedMs / logicalProcessors * 100 : 0;
    private void Sample()
    {
        _samples++;
        try
        {
            _host.Refresh();
            var hostPrivate = _host.PrivateMemorySize64;
            _hostCpu = Math.Max(0, _host.TotalProcessorTime.TotalMilliseconds - _hostStartCpu);
            _hostPrivate = Math.Max(_hostPrivate, hostPrivate);
            _hostWorking = Math.Max(_hostWorking, _host.WorkingSet64);
            var active = new Dictionary<int, long> { [_host.Id] = _host.StartTime.ToUniversalTime().Ticks };
            long privateBytes = 0, workingBytes = 0; _live = 0;
            // Retain observed child identity even if its parent has already exited. PID reuse is rejected.
            foreach (var key in _childrenCpu.Keys.ToArray())
                ReadChild(key.Pid, key.Start, knownIdentity: true);
            var entries = Parents();
            var changed = true;
            while (changed)
            {
                changed = false;
                foreach (var (pid, parent) in entries)
                    if (pid != _host.Id && !active.ContainsKey(pid) && active.TryGetValue(parent, out var parentStart))
                        if (ReadChild(pid, parentStart, knownIdentity: false)) changed = true;
            }
            _childPrivate = Math.Max(_childPrivate, privateBytes);
            _childWorking = Math.Max(_childWorking, workingBytes);
            _combined = Math.Max(_combined, hostPrivate + privateBytes);

            bool ReadChild(int pid, long startBound, bool knownIdentity)
            {
                try
                {
                    using var process = Process.GetProcessById(pid);
                    var start = process.StartTime.ToUniversalTime().Ticks;
                    if (process.HasExited || (knownIdentity ? start != startBound : start < startBound)) return false;
                    if (!active.TryAdd(pid, start)) return false;
                    privateBytes += process.PrivateMemorySize64;
                    workingBytes += process.WorkingSet64;
                    var key = (pid, start);
                    var cpu = process.TotalProcessorTime.TotalMilliseconds;
                    if (!_childrenBaselineCpu.ContainsKey(key)) _childrenBaselineCpu[key] = start < _startedUtcTicks ? cpu : 0;
                    _childrenCpu[key] = Math.Max(_childrenCpu.GetValueOrDefault(key), Math.Max(0, cpu - _childrenBaselineCpu[key]));
                    _live++;
                    return true;
                }
                catch (ArgumentException) { return false; } // Exited between snapshot and sample.
                catch (InvalidOperationException) { return false; }
                catch (Win32Exception) { _unavailable++; return false; }
            }
        }
        catch (Win32Exception) { _unavailable++; }
        catch (InvalidOperationException) { _unavailable++; }
    }
    private static List<(int Pid, int Parent)> Parents()
    {
        using var snapshot = CreateToolhelp32Snapshot(2, 0); // TH32CS_SNAPPROCESS
        if (snapshot.IsInvalid) throw new Win32Exception(Marshal.GetLastWin32Error());
        var entry = new ProcessEntry { Size = (uint)Marshal.SizeOf<ProcessEntry>() };
        var result = new List<(int, int)>();
        if (!Process32FirstW(snapshot, ref entry)) throw new Win32Exception(Marshal.GetLastWin32Error());
        do { result.Add(((int)entry.Pid, (int)entry.ParentPid)); } while (Process32NextW(snapshot, ref entry));
        if (Marshal.GetLastWin32Error() != 18) throw new Win32Exception(Marshal.GetLastWin32Error()); // ERROR_NO_MORE_FILES
        return result;
    }
    public async ValueTask DisposeAsync()
    {
        _stop.Cancel(); await _loop.ConfigureAwait(false);
        _host.Dispose(); _stop.Dispose();
    }

    // PROCESSENTRY32W layout includes pointer-sized default heap ID (x64/ARM64 compatible).
    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
    private struct ProcessEntry
    {
        public uint Size, Usage, Pid;
        public UIntPtr DefaultHeap;
        public uint Module, Threads, ParentPid;
        public int BasePriority;
        public uint Flags;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 260)] public string Executable;
    }
    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern SafeFileHandle CreateToolhelp32Snapshot(uint flags, uint pid);
    [DllImport("kernel32.dll", SetLastError = true, CharSet = CharSet.Unicode, ExactSpelling = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool Process32FirstW(SafeFileHandle snapshot, ref ProcessEntry entry);
    [DllImport("kernel32.dll", SetLastError = true, CharSet = CharSet.Unicode, ExactSpelling = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool Process32NextW(SafeFileHandle snapshot, ref ProcessEntry entry);
}
