using System.Windows;
using GitGui.App.ViewModels;
using GitGui.Core.Settings;

namespace GitGui.App.Views;

public partial class MainWindow : Window
{
    public ShellViewModel ViewModel { get; }
    public MainWindow(ShellViewModel viewModel)
    {
        InitializeComponent();
        DataContext = ViewModel = viewModel;
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
        SelectedSection = ViewModel.SelectedSection.Section
    };

    private void OnWorkspaceSizeChanged(object sender, SizeChangedEventArgs e)
    {
        // A restored fixed-height details panel must not push content beneath the status bar.
        DetailsRow.MaxHeight = Math.Max(DetailsRow.MinHeight,
            Math.Min(360, WorkspacePanel.ActualHeight - HistoryRow.MinHeight - 5));
    }
}
