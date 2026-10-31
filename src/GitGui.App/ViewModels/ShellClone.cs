using GitGui.App.Localization;
using GitGui.Core.Git;

namespace GitGui.App.ViewModels;
public sealed partial class ShellViewModel
{
    public RelayCommand CloneRepositoryCommand { get; private set; } = null!;
    public string CloneRepositoryHelp => NewRepositoryCommand.CanExecute(null) ? UiText.Get("CloneReady") : NewRepositoryHelp;
    private async Task CloneRepositoryAsync()
    {
        CancelCountRead(); SetActionOpen(true);
        try
        {
            var options = _interaction!.ChooseRepositoryClone();
            if (options is null) return;
            _initializationOwner = SelectedRepository;
            RepositoryCloneResult? result = null;
            var model = new GitActionViewModel(options.Directory, _actions!, async () =>
            {
                if (result?.Repository is not { } repository)
                { if (result?.Action.Success == true) throw new System.IO.IOException(); return; }
                var tab = Repositories.FirstOrDefault(item => string.Equals(item.Directory, repository.Worktree, StringComparison.OrdinalIgnoreCase));
                if (tab is null) { tab = new(repository.Worktree); Repositories.Add(tab); }
                _initializationOwner = tab;
                SelectRepository(tab);
                BeginInspection(tab, reopen: true, force: true, actionReconcile: true);
                await WaitForIdleAsync();
                if (tab.History.ValidatedAt == default || tab.History.Error.Length != 0) throw new System.IO.IOException();
            }, _interaction, async (token, output, authentication) =>
            {
                _git ??= await _reader!.DiscoverAsync(_explicitGit, token);
                Changed(nameof(GitStatus));
                result = await _actions!.CloneRepositoryAsync(_git, options, token, output, authentication);
                return result.Action;
            });
            _preparingAction = false; SetLastAction(model); DetailsIndex = 3;
            model.RunCommand.Execute(null); await model.RunningTask; Status = model.Summary;
        }
        finally
        {
            SetActionOpen(false);
            if (SelectedRepository is { } tab) { BeginCountRead(tab); BeginSidebarRead(tab); }
        }
    }
}
