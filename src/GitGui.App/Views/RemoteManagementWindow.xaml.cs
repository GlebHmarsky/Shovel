using System.Windows;
using GitGui.App.Services;
using GitGui.App.ViewModels;
namespace GitGui.App.Views;
public partial class RemoteManagementWindow : Window
{
    public RemoteManagementWindow(RemoteManagementViewModel model)
    {
        InitializeComponent(); DataContext = model; TitleBarTheme.Attach(this);
        model.Accepted += OnAccepted; Closed += (_, _) => model.Accepted -= OnAccepted;
    }
    private void OnAccepted(object? sender, EventArgs args) => DialogResult = true;
}
