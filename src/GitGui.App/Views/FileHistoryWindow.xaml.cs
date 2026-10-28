using System.ComponentModel;
using System.Windows;
using System.Windows.Input;
using System.Windows.Threading;
using GitGui.App.Services;
using GitGui.App.ViewModels;

namespace GitGui.App.Views;

public partial class FileHistoryWindow : Window
{
    private readonly FileHistoryViewModel _model;
    private bool _closing, _allowClose;
    public FileHistoryWindow(FileHistoryViewModel model)
    {
        _model = model; DataContext = model; InitializeComponent(); TitleBarTheme.Attach(this);
        Loaded += async (_, _) => await model.StartAsync();
        PreviewKeyDown += (_, e) => { if (e.Key == Key.Escape) { e.Handled = true; Close(); } };
        Closing += OnClosing;
    }
    private async void OnClosing(object? sender, CancelEventArgs e)
    {
        if (_allowClose) return;
        e.Cancel = true; if (_closing) return; _closing = true;
        await Dispatcher.Yield(); await _model.CloseAsync(); _allowClose = true; Close();
    }
}
