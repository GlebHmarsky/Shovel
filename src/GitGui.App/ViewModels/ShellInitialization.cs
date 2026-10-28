using GitGui.App.Localization;
using GitGui.Core.Git;

namespace GitGui.App.ViewModels;

public sealed partial class ShellViewModel
{
    private RepositoryTab? _initializationOwner;
    public RelayCommand NewRepositoryCommand { get; private set; } = null!;
    public string NewRepositoryHelp => UiText.Get(!IsReady || _stopping ? "ActionWorkspaceStarting" :
        ActionOpen ? "ActionWorkspaceBusy" : IsBusy ? "ActionWaitForRead" : Repositories.Count >= 20 ? "TooManyRepositories" : "ActionExplainInit");
    private async Task InitializeRepositoryAsync()
    {
        CancelCountRead(); SetActionOpen(true);
        try
        {
            var options = _interaction!.ChooseRepositoryInitialization();
            if (options is null) return;
            _initializationOwner = SelectedRepository;
            RepositoryInitResult? result = null;
            var model = new GitActionViewModel(options.Directory, _actions!, async () =>
            {
                if (result?.Repository is not { } repository) return;
                var tab = Repositories.FirstOrDefault(item => string.Equals(item.Directory, repository.Worktree, StringComparison.OrdinalIgnoreCase));
                if (tab is null) { tab = new(repository.Worktree); Repositories.Add(tab); }
                _initializationOwner = tab;
                SelectRepository(tab);
                BeginInspection(tab, reopen: true, force: true, actionReconcile: true);
                await WaitForIdleAsync();
                if (tab.History.ValidatedAt == default || tab.History.Error.Length != 0) throw new System.IO.IOException();
            }, async (token, output) =>
            {
                _git ??= await _reader!.DiscoverAsync(_explicitGit, token);
                Changed(nameof(GitStatus));
                result = await _actions!.InitializeRepositoryAsync(_git, options, token, output);
                return result.Action;
            });
            _preparingAction = false; SetLastAction(model); DetailsIndex = 3;
            model.RunCommand.Execute(null);
            await model.RunningTask;
            Status = model.Summary;
        }
        finally
        {
            SetActionOpen(false);
            if (SelectedRepository is { } tab) BeginCountRead(tab);
        }
    }
}
