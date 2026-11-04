using System.Windows;
using GitGui.App.Services;

namespace GitGui.App.Views;

public sealed record OperationProblem(string Repository, string Title, string Status, string Output, Action? Reopen = null)
{ public bool CanReopen => Reopen is not null; }
public partial class OperationProblemWindow : Window
{
    public OperationProblemWindow(OperationProblem problem)
    { InitializeComponent(); DataContext = problem; TitleBarTheme.Attach(this); }
    private void OnDismiss(object sender, RoutedEventArgs e) => Close();
    private void OnReopen(object sender, RoutedEventArgs e) { var reopen = ((OperationProblem)DataContext).Reopen; Close(); reopen?.Invoke(); }
}
