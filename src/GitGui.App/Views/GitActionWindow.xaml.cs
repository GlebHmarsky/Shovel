using System.Windows;
using GitGui.App.ViewModels;

namespace GitGui.App.Views;

public partial class GitActionWindow : Window
{
    private readonly GitActionViewModel _model;
    public GitActionWindow(GitActionViewModel model)
    {
        _model = model; DataContext = model; InitializeComponent();
        _model.ExecutionStarting += OnExecutionStarting;
        Closed += (_, _) => { _model.ExecutionStarting -= OnExecutionStarting; _model.DismissOptions(); };
    }
    private void OnExecutionStarting(object? sender, EventArgs e) => Close();
    private void OnCancel(object sender, RoutedEventArgs e) => Close();
}
