using System.Windows;
using GitGui.App.Services;
using GitGui.App.ViewModels;
namespace GitGui.App.Views;
public partial class RepositoryCloneWindow : Window
{
    public RepositoryCloneWindow(RepositoryCloneViewModel model)
    {
        InitializeComponent(); DataContext = model; TitleBarTheme.Attach(this);
        model.Accepted += OnAccepted; Closed += (_, _) => model.Accepted -= OnAccepted; Loaded += (_, _) => SourceInput.Focus();
    }
    private void OnAccepted(object? sender, EventArgs args) => DialogResult = true;
}
