using System.Windows;
using System.ComponentModel;
using System.Windows.Media;
using System.Windows.Threading;
using System.Windows.Controls;
using System.Windows.Data;
using GitGui.App.ViewModels;
using GitGui.Core.Settings;

namespace GitGui.App.Views;

public partial class MainWindow : Window
{
    public static readonly DependencyProperty HighContrastProperty = DependencyProperty.Register(nameof(HighContrast), typeof(bool), typeof(MainWindow), new PropertyMetadata(false));
    public bool HighContrast { get => (bool)GetValue(HighContrastProperty); set => SetValue(HighContrastProperty, value); }
    private bool _restoringScroll;
    private int _viewGeneration;
    private bool _changesLayout;
    private GridLength _previousDetailsHeight;
    public ShellViewModel ViewModel { get; }
    public MainWindow(ShellViewModel viewModel)
    {
        InitializeComponent(); GitGui.App.Services.TitleBarTheme.Attach(this);
        DataContext = ViewModel = viewModel;
        HighContrast = SystemParameters.HighContrast;
        SystemParameters.StaticPropertyChanged += OnSystemPropertyChanged;
        viewModel.HistoryViewChanging += CaptureHistoryScroll;
        viewModel.HistoryViewChanged += RestoreHistoryScroll;
        Activated += (_, _) => viewModel.RefreshIfStale();
        Closed += (_, _) =>
        {
            SystemParameters.StaticPropertyChanged -= OnSystemPropertyChanged;
            viewModel.HistoryViewChanging -= CaptureHistoryScroll;
            viewModel.HistoryViewChanged -= RestoreHistoryScroll;
        };
        // Columns are outside the visual tree. Keep every computed graph lane visible when
        // the viewport narrows; the table can scroll horizontally instead of clipping edges.
        BindingOperations.SetBinding(HistoryTable.Columns[0], DataGridColumn.MinWidthProperty,
            new Binding(nameof(ShellViewModel.GraphWidth)) { Source = viewModel });
    }

    private void OnSystemPropertyChanged(object? sender, PropertyChangedEventArgs e)
    { if (e.PropertyName == nameof(SystemParameters.HighContrast)) Dispatcher.InvokeAsync(() => HighContrast = SystemParameters.HighContrast); }
    private void OnBranchMenu(object sender, RoutedEventArgs e)
    {
        if (sender is Button { ContextMenu: { } menu } button)
        { menu.PlacementTarget = button; menu.Placement = System.Windows.Controls.Primitives.PlacementMode.Bottom; menu.IsOpen = true; }
    }
    private void OnRepositoryTabMouseDown(object sender, System.Windows.Input.MouseButtonEventArgs e)
    {
        if (e.ChangedButton == System.Windows.Input.MouseButton.Middle && sender is ListBoxItem { DataContext: RepositoryTab tab })
        { e.Handled = true; ViewModel.CloseTab(tab); }
    }
    private void CaptureHistoryScroll(object? sender, EventArgs e)
    {
        if (ViewModel.IsHistory && !_restoringScroll && FindScroll(HistoryTable) is { } scroll)
            ViewModel.SaveHistoryScroll(scroll.VerticalOffset, scroll.HorizontalOffset);
    }
    private void RestoreHistoryScroll(object? sender, EventArgs e)
    {
        var version = ++_viewGeneration;
        if (!ViewModel.IsHistory) { _restoringScroll = false; return; }
        var offsets = ViewModel.HistoryScroll;
        _restoringScroll = true;
        Dispatcher.InvokeAsync(() =>
        {
            if (version != _viewGeneration) return;
            HistoryTable.UpdateLayout();
            if (FindScroll(HistoryTable) is { } scroll)
            { scroll.ScrollToVerticalOffset(offsets.Vertical); scroll.ScrollToHorizontalOffset(offsets.Horizontal); }
            _restoringScroll = false;
        }, DispatcherPriority.Loaded);
    }
    private static ScrollViewer? FindScroll(DependencyObject parent)
    {
        for (var i = 0; i < VisualTreeHelper.GetChildrenCount(parent); i++)
        {
            var child = VisualTreeHelper.GetChild(parent, i);
            if (child is ScrollViewer scroll) return scroll;
            if (FindScroll(child) is { } descendant) return descendant;
        }
        return null;
    }

    // Read WPF geometry here; the settings model stores only plain values.
    public void RestoreLayout(AppSettings settings)
    {
        var area = SystemParameters.WorkArea;
        Width = Math.Min(settings.WindowWidth, area.Width);
        Height = Math.Min(settings.WindowHeight, area.Height);
        SidebarColumn.Width = new(settings.SidebarWidth);
        DetailsRow.Height = new(settings.DetailsHeight);
        if (settings.IsMaximized) WindowState = WindowState.Maximized;
    }

    public AppSettings CaptureLayout() => new()
    {
        WindowWidth = RestoreBounds.Width,
        WindowHeight = RestoreBounds.Height,
        IsMaximized = WindowState == WindowState.Maximized,
        SidebarWidth = SidebarColumn.ActualWidth,
        DetailsHeight = DetailsRow.ActualHeight,
        SelectedSection = NavigationSection.History
    };

    private void OnDetailsSelection(object sender, SelectionChangedEventArgs e)
    {
        if (e.OriginalSource != sender || HistoryRow is null || DetailsRow is null) return;
        var changes = ((TabControl)sender).SelectedIndex == 4;
        if (changes && !_changesLayout) { _previousDetailsHeight = DetailsRow.Height; DetailsRow.Height = new(Math.Max(260, DetailsRow.Height.Value)); }
        else if (!changes && _changesLayout) DetailsRow.Height = _previousDetailsHeight;
        _changesLayout = changes;
        HistoryRow.MinHeight = changes ? 160 : 300;
        DetailsRow.MinHeight = changes ? 200 : 80;
        FitDetails();
    }
    private void OnWorkspaceSizeChanged(object sender, SizeChangedEventArgs e)
    {
        FitDetails();
    }
    private void FitDetails()
    {
        if (ShellLayout is null || DetailsRow is null || ShellLayout.ActualHeight <= 0) return;
        // A child's desired height can exceed its viewport. Use the finite shell height,
        // excluding chrome/footer, so the details panel cannot cover the status bar.
        var available = ShellLayout.ActualHeight - ShellLayout.RowDefinitions.Where((_, index) => index != 3).Sum(row => row.ActualHeight);
        DetailsRow.MaxHeight = Math.Max(DetailsRow.MinHeight, Math.Min(360, available - HistoryRow.MinHeight - 5));
    }
}
