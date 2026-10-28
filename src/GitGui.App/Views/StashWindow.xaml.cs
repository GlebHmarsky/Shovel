using System.ComponentModel;
using System.Windows;
using GitGui.App.ViewModels;

namespace GitGui.App.Views;

public partial class StashWindow : Window
{
    private readonly StashViewModel _model;
    private bool _closing, _allowClose;
    public StashWindow(StashViewModel model)
    {
        _model = model; DataContext = model; InitializeComponent(); Services.TitleBarTheme.Attach(this);
        Loaded += async (_, _) => await model.StartAsync(); Closing += OnClosing;
    }
    private async void OnClosing(object? sender, CancelEventArgs args)
    {
        if (_allowClose) return;
        args.Cancel = true; if (_closing) return; _closing = true;
        await System.Windows.Threading.Dispatcher.Yield(); await _model.CloseAsync(); _allowClose = true; Close();
    }
}
