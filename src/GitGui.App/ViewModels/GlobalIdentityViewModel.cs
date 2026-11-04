using GitGui.App.Localization;
using GitGui.Core.Git;

namespace GitGui.App.ViewModels;

public sealed class GlobalIdentityViewModel : ObservableObject
{
    private readonly Func<CancellationToken, Task<GlobalGitIdentity>>? _read;
    private readonly Func<GlobalGitIdentity, string, string, CancellationToken, Task>? _write;
    private GlobalGitIdentity? _reviewed;
    private string _name = "", _email = "", _status = "";
    private bool _busy;
    private CancellationTokenSource? _cancel;
    public string Name { get => _name; set { if (Set(ref _name, value)) { ApplyCommand.Refresh(); Changed(nameof(Status)); } } }
    public string Email { get => _email; set { if (Set(ref _email, value)) { ApplyCommand.Refresh(); Changed(nameof(Status)); } } }
    public string Status { get => CanEdit && (!GitActions.ValidIdentityValue(Name) || !GitActions.ValidIdentityValue(Email)) ? UiText.Get("SettingsIdentityInvalid") : _status; private set => Set(ref _status, value); }
    public bool Busy { get => _busy; private set { Set(ref _busy, value); Changed(nameof(CanEdit)); Changed(nameof(Status)); LoadCommand.Refresh(); ApplyCommand.Refresh(); } }
    public bool CanEdit => !Busy && _reviewed is not null;
    public RelayCommand LoadCommand { get; }
    public RelayCommand ApplyCommand { get; }
    public Task Pending { get; private set; } = Task.CompletedTask;
    public GlobalIdentityViewModel(Func<CancellationToken, Task<GlobalGitIdentity>>? read = null, Func<GlobalGitIdentity, string, string, CancellationToken, Task>? write = null)
    {
        _read = read; _write = write; _status = UiText.Get(read is null ? "SettingsIdentityNoGit" : "SettingsIdentityLoadHelp");
        LoadCommand = new(() => Pending = Run(false), () => !Busy && _read is not null);
        ApplyCommand = new(() => Pending = Run(true), () => CanEdit && _write is not null && GitActions.ValidIdentityValue(Name) && GitActions.ValidIdentityValue(Email));
    }
    private async Task Run(bool save)
    {
        var name = Name; var email = Email; Busy = true; Status = UiText.Get("SettingsIdentityWorking");
        using var cancel = new CancellationTokenSource(TimeSpan.FromSeconds(30)); _cancel = cancel;
        try
        {
            if (save) { using var lifetime = GitGui.Core.Processes.ProcessLifetime.CompleteStartedCommands(); await _write!(_reviewed!, name, email, cancel.Token); }
            _reviewed = await _read!(cancel.Token); Name = _reviewed.Name; Email = _reviewed.Email;
            Status = UiText.Get(save ? "SettingsIdentitySaved" : "SettingsIdentityLoaded");
        }
        catch (GitActionException error) when (error.Code == "SettingsIdentityChanged")
        {
            _reviewed = null;
            Status = UiText.Get("SettingsIdentityChanged");
        }
        catch (Exception)
        {
            _reviewed = null;
            Status = UiText.Get(save ? "SettingsIdentitySaveFailed" : "SettingsIdentityLoadFailed");
        }
        finally { _cancel = null; Busy = false; }
    }
    public void Cancel() => _cancel?.Cancel();
}
