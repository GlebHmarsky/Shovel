using System.Windows;
using GitGui.App.Services;
using GitGui.App.ViewModels;

namespace GitGui.App.Views;

public partial class SettingsWindow : Window
{
    public SettingsWindow(SettingsViewModel model)
    {
        InitializeComponent(); DataContext = model; TitleBarTheme.Attach(this);
        model.Saved += OnSaved;
        Closing += (_, e) => e.Cancel = model.IsSaving;
        Closed += (_, _) => model.Saved -= OnSaved;
        Loaded += (_, _) => AutoFetchEnabledInput.Focus();
    }
    private void OnSaved(object? sender, EventArgs e) => DialogResult = true;
}
