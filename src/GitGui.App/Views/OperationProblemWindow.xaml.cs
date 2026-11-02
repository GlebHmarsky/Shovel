using System.Windows;
using GitGui.App.Services;

namespace GitGui.App.Views;

public sealed record OperationProblem(string Repository, string Title, string Status, string Output);
public partial class OperationProblemWindow : Window
{
    public OperationProblemWindow(OperationProblem problem)
    { InitializeComponent(); DataContext = problem; TitleBarTheme.Attach(this); }
    private void OnDismiss(object sender, RoutedEventArgs e) => Close();
}
