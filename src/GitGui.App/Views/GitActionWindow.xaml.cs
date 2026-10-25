using System.ComponentModel;
using System.Windows;
using GitGui.App.Localization;
using GitGui.App.ViewModels;

namespace GitGui.App.Views;

public partial class GitActionWindow : Window
{
    private readonly GitActionViewModel _model;
    public GitActionWindow(GitActionViewModel model) { _model = model; DataContext = model; InitializeComponent(); }
    private void OnClosing(object? sender, CancelEventArgs e) { if (_model.IsRunning) e.Cancel = true; }
    private void OnCancel(object sender, RoutedEventArgs e)
    {
        if (!_model.IsRunning) { Close(); return; }
        if (MessageBox.Show(this, UiText.Get("ActionCancelQuestion"), _model.Title, MessageBoxButton.YesNo,
            MessageBoxImage.Warning, MessageBoxResult.No) == MessageBoxResult.Yes) _model.Cancel();
    }
}
