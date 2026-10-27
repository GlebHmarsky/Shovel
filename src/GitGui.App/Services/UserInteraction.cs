using System.Windows;
using GitGui.App.Localization;
using Microsoft.Win32;
using GitGui.App.ViewModels;
using GitGui.App.Views;
using GitGui.Core.Git;
using GitGui.ProcessHost;

namespace GitGui.App.Services;

public interface IUserInteraction
{
    string? ChooseRepository();
    string? ChooseGit();
    void ShowGitAction(GitActionViewModel model) { }
    void ShowActionError(string message) { }
    IAuthenticationSession? CreateAuthenticationSession(string context,
        Func<AuthenticationPrompt, CancellationToken, Task<AuthenticationAnswer?>> prompt, CancellationToken token) => null;
    Task<AuthenticationAnswer?> ShowAuthenticationAsync(string context, AuthenticationPrompt prompt, CancellationToken token) => Task.FromResult<AuthenticationAnswer?>(null);
}

public sealed class UserInteraction : IUserInteraction
{
    private static Window Owner => Application.Current.MainWindow;
    public void ShowGitAction(GitActionViewModel model)
    {
        var window = new GitActionWindow(model) { Owner = Owner };
        window.ShowDialog();
    }
    public IAuthenticationSession CreateAuthenticationSession(string context,
        Func<AuthenticationPrompt, CancellationToken, Task<AuthenticationAnswer?>> prompt, CancellationToken token) =>
        new AskPassSession(System.IO.Path.Combine(AppContext.BaseDirectory, "Shovel.AskPass.exe"), prompt, token);
    public async Task<AuthenticationAnswer?> ShowAuthenticationAsync(string context, AuthenticationPrompt prompt, CancellationToken token) =>
        await Application.Current.Dispatcher.InvokeAsync(() =>
        {
            if (token.IsCancellationRequested) return null;
            var window = new AuthenticationWindow(context, prompt) { Owner = Owner };
            using var cancelled = token.Register(() => window.Dispatcher.BeginInvoke(() => window.Close()));
            window.ShowDialog();
            if (!token.IsCancellationRequested) return window.TakeAnswer();
            window.TakeAnswer()?.Dispose(); return null;
        });
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
