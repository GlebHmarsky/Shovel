using System.Windows;
using GitGui.App.Services;
using GitGui.App.Localization;
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
    private void OnBrowseMergeTool(object sender, RoutedEventArgs e)
    {
        if (DataContext is not SettingsViewModel { CanEdit: true } model) return;
        var dialog = new Microsoft.Win32.OpenFileDialog
        { Title = UiText.Get("SettingsMergeExecutable"), Filter = UiText.Get("SettingsMergeExecutableFilter"), CheckFileExists = true };
        if (dialog.ShowDialog(this) == true) model.MergeToolExecutablePath = dialog.FileName;
    }
    private void OnSaved(object? sender, EventArgs e) => DialogResult = true;
}
