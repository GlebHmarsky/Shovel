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
    string? ChooseWorkspaceExecutable(RepositoryTool tool) => null;
    BranchManagementOptions? ChooseBranchManagement(BranchManagementViewModel model) => null;
    RepositoryInitOptions? ChooseRepositoryInitialization() => null;
    string? ChooseInitializationFolder() => null;
    string? ChooseRepository();
    string? ChooseGit();
    void ShowGitAction(GitActionViewModel model) { }
    Task ShowCommitAsync(CommitViewModel model) => Task.CompletedTask;
    Task ShowStashesAsync(StashViewModel model) => Task.CompletedTask;
    void CopyFilePaths(string text) => throw new NotSupportedException();
    Task<bool> OpenWorkingFileAsync(string root, string relative, bool explorer, CancellationToken token) => throw new NotSupportedException();
    string? ConfirmFileAction(FileActionPrompt prompt) => null;
    string? ChoosePatchFile() => null;
    void ShowFileHistory(FileHistoryViewModel model) { }
    bool ConfirmAmend(string branch, string hash) => false;
    bool ConfirmRecovery(string message, string title) => false;
    void ShowActionError(string message) { }
    IAuthenticationSession? CreateAuthenticationSession(string context,
        Func<AuthenticationPrompt, CancellationToken, Task<AuthenticationAnswer?>> prompt, CancellationToken token) => null;
    Task<AuthenticationAnswer?> ShowAuthenticationAsync(string context, AuthenticationPrompt prompt, CancellationToken token) => Task.FromResult<AuthenticationAnswer?>(null);
}

public sealed class UserInteraction : IUserInteraction
{
    public string? ChooseWorkspaceExecutable(RepositoryTool tool)
    {
        var dialog = new OpenFileDialog { Title = UiText.Format("WorkspaceChooseExecutable", UiText.Get("Workspace" + tool)), Filter = UiText.Get("WorkspaceExecutableFilter"), CheckFileExists = true };
        return dialog.ShowDialog(Owner) == true ? dialog.FileName : null;
    }
    public BranchManagementOptions? ChooseBranchManagement(BranchManagementViewModel model)
    {
        var window = new BranchManagementWindow(model) { Owner = Owner };
        return window.ShowDialog() == true ? model.SubmittedOptions : null;
    }
    public RepositoryInitOptions? ChooseRepositoryInitialization()
    {
        var model = new RepositoryInitViewModel(this);
        var window = new RepositoryInitWindow(model) { Owner = Owner };
        return window.ShowDialog() == true ? model.Options : null;
    }
    public string? ChooseInitializationFolder()
    {
        var picker = new OpenFolderDialog { Title = UiText.Get("InitChooseFolder"), Multiselect = false };
        return picker.ShowDialog(Owner) == true ? picker.FolderName : null;
    }
    public string? ConfirmFileAction(FileActionPrompt prompt)
    {
        var window = new FileActionWindow(prompt) { Owner = Owner };
        return window.ShowDialog() == true ? window.Message : null;
    }
    public string? ChoosePatchFile()
    {
        var picker = new SaveFileDialog { Title = UiText.Get("FilePatchTitle"), Filter = UiText.Get("FilePatchFilter"), DefaultExt = ".patch", AddExtension = true, OverwritePrompt = true };
        return picker.ShowDialog(Owner) == true ? picker.FileName : null;
    }
    public void ShowFileHistory(FileHistoryViewModel model) => new FileHistoryWindow(model) { Owner = Owner }.ShowDialog();
    public void CopyFilePaths(string text) => Clipboard.SetText(text);
    public Task<bool> OpenWorkingFileAsync(string root, string relative, bool explorer, CancellationToken token) => FileShellActions.OpenAsync(root, relative, explorer, token);
    public Task ShowStashesAsync(StashViewModel model) { new StashWindow(model) { Owner = Owner }.ShowDialog(); return Task.CompletedTask; }
    public bool ConfirmRecovery(string message, string title) => MessageBox.Show(Owner, message, title,
        MessageBoxButton.YesNo, MessageBoxImage.Warning, MessageBoxResult.No) == MessageBoxResult.Yes;
    private static Window Owner => Application.Current.Windows.OfType<Window>().FirstOrDefault(window => window.IsActive) ?? Application.Current.MainWindow;
    public Task ShowCommitAsync(CommitViewModel model)
    {
        new CommitWindow(model) { Owner = Owner }.ShowDialog();
        return Task.CompletedTask;
    }
    public bool ConfirmAmend(string branch, string hash) => MessageBox.Show(Owner,
        UiText.Format("CommitConfirmAmend", branch, hash), UiText.Get("CommitAmend"), MessageBoxButton.YesNo, MessageBoxImage.Warning, MessageBoxResult.No) == MessageBoxResult.Yes;
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
