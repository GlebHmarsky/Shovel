using System.Windows;
using System.Windows.Controls;
using System.Windows.Media;
using System.Windows.Threading;
using GitGui.Core.Settings;

namespace GitGui.App.Services;

/// <summary>Profile-owned geometry. Views opt into pane persistence with a stable key;
/// no content, selections, credentials or filesystem work belong in this store.</summary>
public sealed class WindowLayouts
{
    private readonly Dictionary<string, WindowLayout> _layouts = new(StringComparer.Ordinal);
    public Action? Changed { get; set; }
    public WindowLayout[] Snapshot => _layouts.Values.ToArray();
    public void Load(IEnumerable<WindowLayout> layouts)
    { _layouts.Clear(); foreach (var layout in WindowLayout.Normalize(layouts)) _layouts[layout.Key] = layout; }

    public static readonly DependencyProperty StoreProperty = DependencyProperty.RegisterAttached("Store", typeof(WindowLayouts), typeof(WindowLayouts));
    public static void SetStore(DependencyObject target, WindowLayouts value) => target.SetValue(StoreProperty, value);
    public static WindowLayouts? GetStore(DependencyObject target) => (WindowLayouts?)target.GetValue(StoreProperty);
    public static readonly DependencyProperty PaneKeyProperty = DependencyProperty.RegisterAttached("PaneKey", typeof(string), typeof(WindowLayouts),
        new PropertyMetadata(null, OnPaneKey));
    public static void SetPaneKey(DependencyObject target, string value) => target.SetValue(PaneKeyProperty, value);
    public static string? GetPaneKey(DependencyObject target) => (string?)target.GetValue(PaneKeyProperty);

    private static void OnPaneKey(DependencyObject target, DependencyPropertyChangedEventArgs args)
    {
        if (target is not Grid grid || args.OldValue is not null) return;
        Window? owner = null; WindowLayouts? store = null;
        grid.Loaded += (_, _) => grid.Dispatcher.BeginInvoke(() =>
        {
            owner = Window.GetWindow(grid);
            store = owner is null ? null : GetStore(owner);
            // Tab contents may materialize after the owner's Loaded event.
            if (owner is not null && store is not null) store.RestorePane(owner.GetType().Name, grid);
        }, DispatcherPriority.Loaded);
        grid.Unloaded += (_, _) =>
        {
            if (owner is null || store is null || grid.ActualWidth <= 0 || grid.ActualHeight <= 0) return;
            store.Capture(owner);
            if (store._layouts.TryGetValue(owner.GetType().Name, out var layout))
                store._layouts[layout.Key] = layout with { Panes = [.. layout.Panes.Where(pane => pane.Key != GetPaneKey(grid)), CapturePane(grid)] };
        };
    }

    public static void Attach(Window window)
    {
        WindowLayouts? store = null;
        // Owner is assigned after construction. Resolve at load; never keep global profile state.
        window.Loaded += (_, _) =>
        {
            if (window.ResizeMode == ResizeMode.NoResize || window.SizeToContent != SizeToContent.Manual) return;
            store = GetStore(window) ?? (window.Owner is { } owner ? GetStore(owner) : null) ??
                (Application.Current?.MainWindow is { } main ? GetStore(main) : null);
            if (store is null) return;
            SetStore(window, store);
            window.Dispatcher.BeginInvoke(() => { if (window.IsLoaded) store.Restore(window); }, DispatcherPriority.Loaded);
        };
        window.Closing += (_, _) => { if (window.IsLoaded) store?.Capture(window); };
        window.Closed += (_, _) => store?.Changed?.Invoke();
    }

    public void Restore(Window window)
    {
        if (!_layouts.TryGetValue(window.GetType().Name, out var layout)) return;
        window.Width = Math.Max(window.MinWidth, Math.Min(layout.Width, SystemParameters.VirtualScreenWidth));
        window.Height = Math.Max(window.MinHeight, Math.Min(layout.Height, SystemParameters.VirtualScreenHeight));
        // Modal editors can be resized, but do not restore a forbidden maximize state.
        if (window.ResizeMode == ResizeMode.CanResize && !WindowCaption.GetCloseOnly(window) && layout.Maximized) window.WindowState = WindowState.Maximized;
        foreach (var grid in Grids(window)) RestorePane(layout.Key, grid);
    }
    private void RestorePane(string key, Grid grid)
    {
        var pane = _layouts.GetValueOrDefault(key)?.Panes.FirstOrDefault(item => item.Key == GetPaneKey(grid));
        if (pane is null) return;
        if (pane.Columns.Length == grid.ColumnDefinitions.Count)
            for (var i = 0; i < pane.Columns.Length; i++) if (!grid.ColumnDefinitions[i].Width.IsAuto) grid.ColumnDefinitions[i].Width = Length(pane.Columns[i]);
        if (pane.Rows.Length == grid.RowDefinitions.Count)
            for (var i = 0; i < pane.Rows.Length; i++) if (!grid.RowDefinitions[i].Height.IsAuto) grid.RowDefinitions[i].Height = Length(pane.Rows[i]);
    }
    private static PaneLayout CapturePane(Grid grid) => new(GetPaneKey(grid)!,
        grid.ColumnDefinitions.Select(d => Value(d.Width)).ToArray(), grid.RowDefinitions.Select(d => Value(d.Height)).ToArray());
    public void Capture(Window window)
    {
        if (window.ResizeMode == ResizeMode.NoResize || window.SizeToContent != SizeToContent.Manual) return;
        var bounds = window.WindowState == WindowState.Normal ? new Rect(0, 0, window.Width, window.Height) : window.RestoreBounds;
        if (!double.IsFinite(bounds.Width) || !double.IsFinite(bounds.Height) || bounds.Width <= 0 || bounds.Height <= 0) return;
        var prior = _layouts.GetValueOrDefault(window.GetType().Name);
        // Unloaded/invisible tabs have no measured geometry; retain their last-known pane values.
        var panes = (prior?.Panes ?? []).ToDictionary(item => item.Key);
        foreach (var grid in Grids(window).Where(grid => grid.ActualWidth > 0 && grid.ActualHeight > 0))
            panes[GetPaneKey(grid)!] = CapturePane(grid);
        _layouts[window.GetType().Name] = new(window.GetType().Name, bounds.Width, bounds.Height,
            window.WindowState == WindowState.Maximized, panes.Values.ToArray());
    }
    private static PaneLength Value(GridLength length) => new(length.Value, length.IsStar);
    private static GridLength Length(PaneLength length) => new(length.Value, length.Star ? GridUnitType.Star : GridUnitType.Pixel);
    private static IEnumerable<Grid> Grids(DependencyObject root)
    {
        if (root is Window { Content: DependencyObject content })
        { foreach (var child in Grids(content)) yield return child; yield break; }
        if (root is Grid grid && GetPaneKey(grid) is { Length: > 0 }) yield return grid;
        for (var i = 0; i < VisualTreeHelper.GetChildrenCount(root); i++)
            foreach (var child in Grids(VisualTreeHelper.GetChild(root, i))) yield return child;
    }
}
