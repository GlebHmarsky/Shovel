using System.Windows;
using GitGui.Core.Settings;

namespace GitGui.App.Services;

public static class DiffPreferences
{
    public static bool SideBySide { get; private set; }
    public static double FontSize { get; private set; } = 12;
    public static void Apply(AppSettings settings)
    {
        SideBySide = settings.DiffSideBySide; FontSize = settings.DiffFontSize;
        if (Application.Current is { } app)
        { app.Resources["Diff.FontSize"] = FontSize; app.Resources["Diff.RowHeight"] = FontSize + 10; }
    }
}
