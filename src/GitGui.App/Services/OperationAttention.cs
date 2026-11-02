using System.Runtime.InteropServices;
using System.Windows;
using System.Windows.Interop;
using GitGui.App.ViewModels;

namespace GitGui.App.Services;

public static class OperationAttention
{
    public static bool Required(OperationOutcome? outcome, string output) =>
        outcome is OperationOutcome.Failed or OperationOutcome.Conflict or OperationOutcome.RefreshFailed ||
        outcome == OperationOutcome.Success && output.Split('\n').Any(line =>
            line.TrimStart().StartsWith("warning:", StringComparison.OrdinalIgnoreCase) ||
            line.TrimStart().StartsWith("remote: warning:", StringComparison.OrdinalIgnoreCase));

    public static void Flash(Window window)
    {
        var handle = new WindowInteropHelper(window).Handle;
        if (handle == IntPtr.Zero || window.IsActive) return;
        var info = new FlashInfo { Size = (uint)Marshal.SizeOf<FlashInfo>(), Window = handle,
            Flags = 2 | 12, Count = uint.MaxValue }; // taskbar, until foreground; never activate
        FlashWindowEx(ref info);
    }
    [StructLayout(LayoutKind.Sequential)]
    private struct FlashInfo { public uint Size; public IntPtr Window; public uint Flags, Count, Timeout; }
    [DllImport("user32.dll")]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool FlashWindowEx(ref FlashInfo info);
}
