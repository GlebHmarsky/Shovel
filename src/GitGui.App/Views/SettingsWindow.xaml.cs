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
        Closing += (_, e) => e.Cancel = model.IsSaving || model.Identity.Busy;
        Closed += (_, _) => model.Saved -= OnSaved;
        Loaded += (_, _) => CategoriesInput.Focus();
    }
    private void OnBrowseMergeTool(object sender, RoutedEventArgs e)
    {
        if (DataContext is not SettingsViewModel { CanEdit: true } model) return;
        var dialog = new Microsoft.Win32.OpenFileDialog
        { Title = UiText.Get("SettingsMergeExecutable"), Filter = UiText.Get("SettingsMergeExecutableFilter"), CheckFileExists = true };
        if (dialog.ShowDialog(this) == true) model.MergeToolExecutablePath = dialog.FileName;
    }
    private void OnBrowseWorkspaceTool(object sender, RoutedEventArgs e)
    {
        if (DataContext is not SettingsViewModel { CanEdit: true } model || sender is not System.Windows.Controls.Button { Tag: string property }) return;
        var dialog = new Microsoft.Win32.OpenFileDialog { Filter = UiText.Get("SettingsMergeExecutableFilter"), CheckFileExists = true };
        if (dialog.ShowDialog(this) != true) return;
        if (property == nameof(model.RiderExecutable)) model.RiderExecutable = dialog.FileName;
        else if (property == nameof(model.CodeExecutable)) model.CodeExecutable = dialog.FileName;
        else if (property == nameof(model.BashExecutable)) model.BashExecutable = dialog.FileName;
    }
    private void OnSaved(object? sender, EventArgs e) => DialogResult = true;
}
