using System.ComponentModel;
using System.Windows;
using GitGui.Core.Settings;

namespace GitGui.App.Services;

/// <summary>Semantic application colors only; never modifies Windows theme/settings.</summary>
public sealed class AppTheme : IDisposable
{
    private readonly Application _app;
    private AppAppearance _appearance;
    public static event EventHandler? PaletteChanged;
    public void Select(AppAppearance appearance)
    { _appearance = appearance; Apply(_app.Resources, SystemParameters.HighContrast, appearance); }
    public AppTheme(Application app)
    {
        _app = app; Apply(app.Resources, SystemParameters.HighContrast);
        SystemParameters.StaticPropertyChanged += Changed;
    }
    private void Changed(object? sender, PropertyChangedEventArgs e)
    {
        if (e.PropertyName == nameof(SystemParameters.HighContrast) || SystemParameters.HighContrast)
            _app.Dispatcher.InvokeAsync(() => Apply(_app.Resources, SystemParameters.HighContrast, _appearance));
    }
    public static void Apply(ResourceDictionary resources, bool highContrast, AppAppearance appearance = AppAppearance.Dark)
    {
        var palette = new ResourceDictionary { Source = new Uri($"/Shovel;component/Resources/{appearance}Palette.xaml", UriKind.Relative) };
        resources["Theme.IsDark"] = appearance == AppAppearance.Dark;
        foreach (string key in palette.Keys)
        {
            if (!highContrast) { resources[key] = palette[key]; continue; }
            resources[key] = key switch
            {
                "Theme.Text" or "Theme.HistoryText" or "Theme.Muted" or "Theme.DiffAddedText" or "Theme.DiffRemovedText" => SystemColors.WindowTextBrush,
                "Theme.FileModified" or "Theme.FileAdded" or "Theme.FileDeleted" => SystemColors.WindowTextBrush,
                "Theme.Disabled" => SystemColors.GrayTextBrush,
                "Theme.ActiveSelection" or "Theme.Selected" or "Theme.Pressed" or "Theme.Hover" => SystemColors.HighlightBrush,
                "Theme.SelectionText" => SystemColors.HighlightTextBrush,
                "Theme.Border" or "Theme.Focus" or "Theme.Accent" => SystemColors.WindowTextBrush,
                _ => SystemColors.WindowBrush
            };
        }
        PaletteChanged?.Invoke(null, EventArgs.Empty);
    }
    public void Dispose() => SystemParameters.StaticPropertyChanged -= Changed;
}
