using System.Windows;
using GitGui.App.Services;

namespace GitGui.App.Views;

public sealed record FileActionPrompt(string Title, string Explanation, IReadOnlyList<string> Targets, string AcceptLabel, bool AskMessage);
public partial class FileActionWindow : Window
{
    public string Message => MessageBox.Text;
    public FileActionWindow(FileActionPrompt prompt)
    {
        DataContext = prompt; InitializeComponent(); TitleBarTheme.Attach(this);
        MessagePanel.Visibility = prompt.AskMessage ? Visibility.Visible : Visibility.Collapsed;
    }
    private void OnAccept(object sender, RoutedEventArgs e) => DialogResult = true;
}
