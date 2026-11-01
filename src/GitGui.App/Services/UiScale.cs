using System.Windows;
using System.Windows.Input;
using System.Windows.Media;

namespace GitGui.App.Services;

public static class UiScale
{
    public static double Value { get; private set; } = 1;
    public static event EventHandler? Changed;
    public static void Set(double value)
    {
        var next = double.IsFinite(value) ? Math.Clamp(Math.Round(value * 10) / 10, .7, 2) : 1;
        if (Value == next) return;
        Value = next; Changed?.Invoke(null, EventArgs.Empty);
    }
    public static void Attach(Window window)
    {
        void Apply(object? sender, EventArgs e)
        { if (window.Content is FrameworkElement root) root.LayoutTransform = new ScaleTransform(Value, Value); }
        Changed += Apply; window.Loaded += Apply;
        window.PreviewKeyDown += (_, e) =>
        {
            if ((Keyboard.Modifiers & ModifierKeys.Control) == 0 || e.IsRepeat) return;
            if (e.Key is Key.Add or Key.OemPlus) { Set(Value + .1); e.Handled = true; }
            else if (e.Key is Key.Subtract or Key.OemMinus) { Set(Value - .1); e.Handled = true; }
        };
        window.Closed += (_, _) => Changed -= Apply;
    }
}
