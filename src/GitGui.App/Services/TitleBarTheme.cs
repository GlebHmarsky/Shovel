using System.ComponentModel;
using System.Runtime.InteropServices;
using System.Windows;
using System.Windows.Interop;
using System.Windows.Media;

namespace GitGui.App.Services;

/// <summary>Colors native chrome without replacing Windows hit testing or caption controls.</summary>
public static class TitleBarTheme
{
    public static void Attach(Window window)
    {
        window.SourceInitialized += Refresh;
        window.Activated += Refresh;
        window.Deactivated += Refresh;
        AppTheme.PaletteChanged += Refresh;
        SystemParameters.StaticPropertyChanged += SystemChanged;
        window.Closed += Closed;

        void Refresh(object? sender, EventArgs e) => Apply(window, SystemParameters.HighContrast);
        void SystemChanged(object? sender, PropertyChangedEventArgs e)
        {
            if (e.PropertyName == nameof(SystemParameters.HighContrast))
                window.Dispatcher.InvokeAsync(() => Apply(window, SystemParameters.HighContrast));
        }
        void Closed(object? sender, EventArgs e)
        {
            window.SourceInitialized -= Refresh;
            window.Activated -= Refresh;
            window.Deactivated -= Refresh;
            AppTheme.PaletteChanged -= Refresh;
            window.Closed -= Closed;
            SystemParameters.StaticPropertyChanged -= SystemChanged;
        }
    }

    public static bool Apply(Window window, bool highContrast)
    {
        // Handle lookup does not create a window or delay its construction with native work.
        var handle = new WindowInteropHelper(window).Handle;
        if (handle == IntPtr.Zero) return false;
        var accepted = true;
        Set(20, highContrast || window.TryFindResource("Theme.IsDark") is false ? 0 : 1); // Best effort on Windows 10.
        if (!OperatingSystem.IsWindowsVersionAtLeast(10, 0, 22000)) return accepted;
        Set(35, highContrast ? -1 : Color("Theme.Chrome")); // CAPTION_COLOR / COLOR_DEFAULT
        Set(36, highContrast ? -1 : Color(window.IsActive ? "Theme.Text" : "Theme.Muted"));
        Set(34, highContrast ? -1 : Color("Theme.Border"));
        return accepted;

        int Color(string key)
        {
            if (window.TryFindResource(key) is not SolidColorBrush brush) return -1;
            var color = brush.Color;
            return color.R | color.G << 8 | color.B << 16; // COLORREF is 0x00BBGGRR.
        }
        void Set(int attribute, int value)
        {
            // Unsupported compositor attributes are cosmetic failures; retain native defaults.
            accepted &= DwmSetWindowAttribute(handle, attribute, ref value, sizeof(int)) >= 0;
        }
    }

    [DllImport("dwmapi.dll", ExactSpelling = true)]
    private static extern int DwmSetWindowAttribute(IntPtr hwnd, int attribute, ref int value, int size);
}
