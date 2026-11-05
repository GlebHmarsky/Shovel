using System.Runtime.InteropServices;
using System.Windows;
using System.Windows.Interop;

namespace GitGui.App.Services;

/// <summary>Keep native resizing and close, without modal minimize/maximize controls.</summary>
public static class WindowCaption
{
    public static readonly DependencyProperty CloseOnlyProperty = DependencyProperty.RegisterAttached("CloseOnly", typeof(bool), typeof(WindowCaption));
    public static bool GetCloseOnly(DependencyObject target) => (bool)target.GetValue(CloseOnlyProperty);
    public static void CloseOnly(Window window)
    {
        window.SetValue(CloseOnlyProperty, true);
        window.SourceInitialized += (_, _) =>
        {
            var handle = new WindowInteropHelper(window).Handle;
            var style = GetWindowLong(handle, -16);
            SetWindowLong(handle, -16, style & ~0x00030000); // WS_MINIMIZEBOX | WS_MAXIMIZEBOX
            SetWindowPos(handle, IntPtr.Zero, 0, 0, 0, 0, 0x0027); // FRAMECHANGED, no move/size/z-order
            HwndSource.FromHwnd(handle)?.AddHook((IntPtr hwnd, int msg, IntPtr w, IntPtr l, ref bool handled) =>
            {
                if (msg == 0x0112 && ((w.ToInt64() & 0xfff0) is 0xf020 or 0xf030)) handled = true;
                return IntPtr.Zero;
            });
        };
    }
    [DllImport("user32.dll", EntryPoint = "GetWindowLongW")] private static extern int GetWindowLong(IntPtr window, int index);
    [DllImport("user32.dll", EntryPoint = "SetWindowLongW")] private static extern int SetWindowLong(IntPtr window, int index, int value);
    [DllImport("user32.dll")] [return: MarshalAs(UnmanagedType.Bool)] private static extern bool SetWindowPos(IntPtr window, IntPtr after, int x, int y, int width, int height, uint flags);
}
