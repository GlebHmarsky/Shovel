using System.ComponentModel;
using System.Windows;
using GitGui.App.ViewModels;

namespace GitGui.App.Views;
public partial class ConflictWindow : Window
{
    private readonly ConflictViewModel _model;
    private bool _closing, _allowClose;
    public ConflictWindow(ConflictViewModel model)
    {
        _model = model; DataContext = model; InitializeComponent(); Services.TitleBarTheme.Attach(this);
        Loaded += async (_, _) => await model.StartAsync(); Closing += OnClosing;
        model.Completed += OnCompleted; Closed += (_, _) => model.Completed -= OnCompleted;
    }
    private void OnCompleted(object? sender, EventArgs args) => Close();
    private async void OnClosing(object? sender, CancelEventArgs args)
    {
        if (_allowClose) return;
        args.Cancel = true; if (_closing) return; _closing = true;
        if (_model.HasEditorChanges && !_model.Resolved && MessageBox.Show(this,
            Localization.UiText.Format("ConflictCloseEdits", _model.Path), _model.Title, MessageBoxButton.YesNo, MessageBoxImage.Warning, MessageBoxResult.No) != MessageBoxResult.Yes)
        { _closing = false; return; }
        await System.Windows.Threading.Dispatcher.Yield(); await _model.CloseAsync(); _allowClose = true; Close();
    }
}
