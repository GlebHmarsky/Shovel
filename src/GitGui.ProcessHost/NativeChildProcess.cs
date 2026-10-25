using System.ComponentModel;
using System.Runtime.InteropServices;
using System.Text;
using GitGui.Core.Processes;
using Microsoft.Win32.SafeHandles;

namespace GitGui.ProcessHost;

/// <summary>Windows 10+ atomic job assignment, with only three inherited standard handles.
/// No shell, intermediate executable, persistent worker or launch-before-ownership window.</summary>
internal sealed class NativeChildProcess : IDisposable
{
    private readonly SafeFileHandle _process;
    public FileStream StandardOutput { get; }
    public FileStream StandardError { get; }
    public Task Completion { get; }
    public int ExitCode
    {
        get
        {
            if (!GetExitCodeProcess(_process, out var code)) throw Error();
            return unchecked((int)code);
        }
    }

    private NativeChildProcess(SafeFileHandle process, SafeFileHandle output, SafeFileHandle error)
    {
        _process = process;
        try
        {
            StandardOutput = new FileStream(output, FileAccess.Read, 8192, isAsync: false);
            StandardError = new FileStream(error, FileAccess.Read, 8192, isAsync: false);
            Completion = WaitAsync();
        }
        catch { output.Dispose(); error.Dispose(); throw; }
    }

    public static NativeChildProcess Start(ProcessRequest request, ProcessJob job)
    {
        if (!Path.IsPathFullyQualified(request.Executable) || !request.Executable.EndsWith(".exe", StringComparison.OrdinalIgnoreCase) ||
            !Path.IsPathFullyQualified(request.WorkingDirectory)) throw new ArgumentException("InvalidProcessRequest");
        var commandLine = new StringBuilder(Quote(request.Executable));
        foreach (var argument in request.Arguments) commandLine.Append(' ').Append(Quote(argument));
        if (commandLine.Length >= 32767) throw new ArgumentException("RequestTooLarge");
        var variables = new SortedDictionary<string, string>(StringComparer.OrdinalIgnoreCase);
        foreach (System.Collections.DictionaryEntry entry in Environment.GetEnvironmentVariables()) variables[(string)entry.Key] = (string)entry.Value!;
        foreach (var (key, value) in request.Environment)
        {
            if (string.IsNullOrEmpty(key) || key.Contains('=') || key.Contains('\0') || value?.Contains('\0') == true)
                throw new ArgumentException("InvalidEnvironment");
            if (value is null) variables.Remove(key); else variables[key] = value;
        }
        var environment = string.Join('\0', variables.Select(entry => entry.Key + "=" + entry.Value)) + "\0\0";
        using var input = new Pipe();
        using var output = new Pipe();
        using var error = new Pipe();
        // Anonymous pipe parent ends are never inherited, even by concurrent launches.
        input.Write.Dispose(); // Immediate EOF on stdin, no invisible prompts.
        output.MakeReadPrivate(); error.MakeReadPrivate();
        using var attributes = new Attributes(job.Handle, [input.Read.DangerousGetHandle(), output.Write.DangerousGetHandle(), error.Write.DangerousGetHandle()]);
        var startup = new StartupInfoEx
        {
            Info = new StartupInfo
            {
                Size = (uint)Marshal.SizeOf<StartupInfoEx>(), Flags = 0x100, // STARTF_USESTDHANDLES
                Input = input.Read.DangerousGetHandle(), Output = output.Write.DangerousGetHandle(), Error = error.Write.DangerousGetHandle()
            },
            Attributes = attributes.List
        };
        // JOB_LIST makes ownership atomic at creation. Fail closed if unsupported/denied.
        if (!CreateProcess(request.Executable, commandLine, IntPtr.Zero, IntPtr.Zero, true,
            0x08000000 | 0x00000400 | 0x00080000, environment, request.WorkingDirectory, ref startup, out var info)) throw Error();
        using var thread = new SafeFileHandle(info.Thread, true);
        var process = new SafeFileHandle(info.Process, true);
        try { return new NativeChildProcess(process, output.TakeRead(), error.TakeRead()); }
        catch { job.Dispose(); process.Dispose(); throw; }
    }

    private async Task WaitAsync()
    {
        using var wait = new EventWaitHandle(false, EventResetMode.ManualReset);
        wait.SafeWaitHandle = new SafeWaitHandle(_process.DangerousGetHandle(), ownsHandle: false);
        var finished = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        var registration = ThreadPool.RegisterWaitForSingleObject(wait, (_, _) => finished.TrySetResult(), null, Timeout.Infinite, executeOnlyOnce: true);
        try { await finished.Task.ConfigureAwait(false); }
        finally { registration.Unregister(null); }
    }

    // Windows CRT argv quoting (the target is an explicit .exe, never cmd/PowerShell).
    private static string Quote(string value)
    {
        if (value.Contains('\0')) throw new ArgumentException("InvalidArgument");
        var result = new StringBuilder("\"");
        var slashes = 0;
        foreach (var character in value)
        {
            if (character == '\\') { slashes++; continue; }
            result.Append('\\', character == '"' ? slashes * 2 + 1 : slashes);
            result.Append(character); slashes = 0;
        }
        return result.Append('\\', slashes * 2).Append('"').ToString();
    }

    public void Dispose() { StandardOutput.Dispose(); StandardError.Dispose(); _process.Dispose(); }
    private static Win32Exception Error() => new(Marshal.GetLastWin32Error());

    private sealed class Pipe : IDisposable
    {
        public SafeFileHandle Read { get; private set; }
        public SafeFileHandle Write { get; }
        public Pipe()
        {
            var security = new SecurityAttributes { Size = Marshal.SizeOf<SecurityAttributes>(), Inherit = 1 };
            if (!CreatePipe(out var read, out var write, ref security, 0)) throw Error();
            Read = read; Write = write;
        }
        public void MakeReadPrivate() { if (!SetHandleInformation(Read, 1, 0)) throw Error(); }
        public SafeFileHandle TakeRead() { var handle = Read; Read = new SafeFileHandle(IntPtr.Zero, true); return handle; }
        public void Dispose() { Read.Dispose(); Write.Dispose(); }
    }

    private sealed class Attributes : IDisposable
    {
        public IntPtr List { get; private set; }
        private readonly IntPtr _job, _handles;
        private bool _initialized;
        public Attributes(IntPtr job, IntPtr[] handles)
        {
            try
            {
                nuint size = 0;
                InitializeProcThreadAttributeList(IntPtr.Zero, 2, 0, ref size);
                List = Marshal.AllocHGlobal(checked((int)size));
                if (!InitializeProcThreadAttributeList(List, 2, 0, ref size)) throw Error();
                _initialized = true;
                _job = Marshal.AllocHGlobal(IntPtr.Size); Marshal.WriteIntPtr(_job, job);
                _handles = Marshal.AllocHGlobal(IntPtr.Size * handles.Length); Marshal.Copy(handles, 0, _handles, handles.Length);
                if (!UpdateProcThreadAttribute(List, 0, 0x0002000D, _job, (nuint)IntPtr.Size, IntPtr.Zero, IntPtr.Zero) ||
                    !UpdateProcThreadAttribute(List, 0, 0x00020002, _handles, (nuint)(IntPtr.Size * handles.Length), IntPtr.Zero, IntPtr.Zero)) throw Error();
            }
            catch { Dispose(); throw; }
        }
        public void Dispose()
        {
            if (_initialized) DeleteProcThreadAttributeList(List);
            Marshal.FreeHGlobal(List); Marshal.FreeHGlobal(_job); Marshal.FreeHGlobal(_handles);
        }
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct SecurityAttributes { public int Size; public IntPtr Descriptor; public int Inherit; }
    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
    private struct StartupInfo
    {
        public uint Size; public string? Reserved, Desktop, Title;
        public uint X, Y, Width, Height, Columns, Rows, Fill, Flags;
        public ushort Show, ReservedSize; public IntPtr ReservedBytes, Input, Output, Error;
    }
    [StructLayout(LayoutKind.Sequential)]
    private struct StartupInfoEx { public StartupInfo Info; public IntPtr Attributes; }
    [StructLayout(LayoutKind.Sequential)]
    private struct ProcessInformation { public IntPtr Process, Thread; public uint ProcessId, ThreadId; }
    [DllImport("kernel32.dll", EntryPoint = "CreateProcessW", CharSet = CharSet.Unicode, SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool CreateProcess(string application, StringBuilder commandLine, IntPtr processAttributes, IntPtr threadAttributes,
        [MarshalAs(UnmanagedType.Bool)] bool inherit, uint flags, string environment, string directory, ref StartupInfoEx startup, out ProcessInformation process);
    [DllImport("kernel32.dll", SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool CreatePipe(out SafeFileHandle read, out SafeFileHandle write, ref SecurityAttributes attributes, uint size);
    [DllImport("kernel32.dll", SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool SetHandleInformation(SafeFileHandle handle, uint mask, uint flags);
    [DllImport("kernel32.dll", SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool GetExitCodeProcess(SafeFileHandle process, out uint code);
    [DllImport("kernel32.dll", SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool InitializeProcThreadAttributeList(IntPtr list, int count, uint flags, ref nuint size);
    [DllImport("kernel32.dll", SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool UpdateProcThreadAttribute(IntPtr list, uint flags, nuint attribute, IntPtr value, nuint size, IntPtr previous, IntPtr returned);
    [DllImport("kernel32.dll")]
    private static extern void DeleteProcThreadAttributeList(IntPtr list);
}
