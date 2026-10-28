using System.ComponentModel;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Input;
using GitGui.App.ViewModels;

namespace GitGui.App.Views;

public partial class CommitWindow : Window
{
    private readonly CommitViewModel _model;
    private bool _closing, _allowClose;
    public CommitWindow(CommitViewModel model)
    {
        _model = model; DataContext = model; InitializeComponent();
        Loaded += async (_, _) => await _model.StartLoading();
        PreviewKeyDown += OnKeyState; PreviewKeyUp += OnKeyState;
        Deactivated += (_, _) => _model.ShiftHeld = false;
        Activated += (_, _) => _model.ShiftHeld = Keyboard.Modifiers.HasFlag(ModifierKeys.Shift);
        Closing += OnClosing;
    }
    private void OnKeyState(object sender, KeyEventArgs e)
    {
        _model.ShiftHeld = Keyboard.Modifiers.HasFlag(ModifierKeys.Shift);
        if (e.Key == Key.Escape) { e.Handled = true; Close(); }
    }
    private void OnUnstagedSelection(object sender, SelectionChangedEventArgs e) => _model.Select(false, ((DataGrid)sender).SelectedItems.Cast<ChangeRow>().ToArray());
    private void OnStagedSelection(object sender, SelectionChangedEventArgs e) => _model.Select(true, ((DataGrid)sender).SelectedItems.Cast<ChangeRow>().ToArray());
    private void OnDiffSelection(object sender, SelectionChangedEventArgs e)
    {
        if (((DataGrid)sender).SelectedItem is { } row) ((DataGrid)sender).ScrollIntoView(row);
    }
    private async void OnClosing(object? sender, CancelEventArgs e)
    {
        if (_allowClose) return;
        e.Cancel = true; if (_closing) return; _closing = true;
        // Even an already idle model may finish synchronously. Leave WPF's Closing callback
        // before closing again; otherwise the ordinary idle close throws VerifyNotClosing.
        await System.Windows.Threading.Dispatcher.Yield();
        await _model.CloseAsync(); _allowClose = true; Close();
    }
}
