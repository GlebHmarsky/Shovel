using System.Windows;
using GitGui.App.Services;
using GitGui.App.ViewModels;

namespace GitGui.App.Views;

public partial class BranchManagementWindow : Window
{
    public BranchManagementWindow(BranchManagementViewModel model)
    {
        InitializeComponent(); DataContext = model; TitleBarTheme.Attach(this);
        model.Accepted += OnAccepted;
        Closed += (_, _) => model.Accepted -= OnAccepted;
        Loaded += (_, _) => BranchInput.Focus();
    }
    private void OnAccepted(object? sender, EventArgs e) => DialogResult = true;
}
