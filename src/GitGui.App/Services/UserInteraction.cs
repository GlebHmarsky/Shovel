using System.Windows;
using GitGui.App.Localization;
using Microsoft.Win32;
using GitGui.App.ViewModels;
using GitGui.App.Views;

namespace GitGui.App.Services;

public interface IUserInteraction
{
    string? ChooseRepository();
    string? ChooseGit();
    void ShowGitAction(GitActionViewModel model) { }
    void ShowActionError(string message) { }
}

public sealed class UserInteraction : IUserInteraction
{
    private static Window Owner => Application.Current.MainWindow;
    public void ShowGitAction(GitActionViewModel model) => new GitActionWindow(model) { Owner = Owner }.ShowDialog();
    public void ShowActionError(string message) => MessageBox.Show(Owner, message, UiText.Get("AppTitle"), MessageBoxButton.OK, MessageBoxImage.Warning);
    public string? ChooseRepository()
    {
        var dialog = new OpenFolderDialog { Title = UiText.Get("ChooseRepositoryTitle"), Multiselect = false };
        return dialog.ShowDialog(Owner) == true ? dialog.FolderName : null;
    }
    public string? ChooseGit()
    {
        var dialog = new OpenFileDialog { Title = UiText.Get("ChooseGitTitle"), Filter = UiText.Get("ExecutableFilter"), CheckFileExists = true };
        if (dialog.ShowDialog(Owner) != true) return null;
        return MessageBox.Show(Owner, UiText.Format("ConfirmGitExecutable", dialog.FileName), UiText.Get("AppTitle"),
            MessageBoxButton.YesNo, MessageBoxImage.Warning, MessageBoxResult.No) == MessageBoxResult.Yes ? dialog.FileName : null;
    }
}
