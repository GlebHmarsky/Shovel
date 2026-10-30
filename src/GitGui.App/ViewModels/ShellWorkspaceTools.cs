using GitGui.App.Localization;
using GitGui.App.Services;
using GitGui.Core.Settings;

namespace GitGui.App.ViewModels;

public sealed partial class ShellViewModel
{
    private IRepositoryTools _repositoryTools = null!;
    private string? _riderExecutable, _codeExecutable, _bashExecutable;
    private bool _launchingTool;
    public Task WorkspaceToolTask { get; private set; } = Task.CompletedTask;
    public Task PreferenceTask { get; private set; } = Task.CompletedTask;
    public RelayCommand<string> OpenToolCommand { get; private set; } = null!;
    public RelayCommand<string> AppearanceCommand { get; private set; } = null!;
    public AppAppearance Appearance { get; private set; }
    public bool IsDarkAppearance => Appearance == AppAppearance.Dark;
    public bool IsLightAppearance => Appearance == AppAppearance.Light;
    public event EventHandler? AppearanceChanged;
    public bool CanOpenTools => CanNavigate && SelectedRepository is not null && _interaction is not null && !_launchingTool;
    public string WorkspaceToolsHelp => UiText.Get(_launchingTool ? "WorkspaceToolStarting" : SelectedRepository is null ? "ActionOpenRepository" :
        ActionOpen ? "ActionWorkspaceBusy" : !IsReady || _stopping ? "ActionWorkspaceStarting" : "WorkspaceToolsHelp");

    private void InitializeWorkspaceTools(IRepositoryTools? tools)
    {
        _repositoryTools = tools ?? new RepositoryTools();
        OpenToolCommand = new(name => WorkspaceToolTask = OpenToolAsync(Enum.Parse<RepositoryTool>(name)), name => CanOpenTools && Enum.TryParse<RepositoryTool>(name, out _));
        AppearanceCommand = new(name =>
        {
            SetAppearance(Enum.Parse<AppAppearance>(name));
            PreferenceTask = SavePreferencesAsync();
        }, name => IsReady && !_stopping && Enum.TryParse<AppAppearance>(name, out _));
    }
    private void RestoreWorkspacePreferences(AppSettings settings)
    {
        _commitLayout = settings.CommitLayout;
        _riderExecutable = settings.RiderExecutable; _codeExecutable = settings.CodeExecutable; _bashExecutable = settings.BashExecutable;
        SetAppearance(settings.Appearance);
    }
    private void SetAppearance(AppAppearance appearance)
    {
        Appearance = appearance;
        Changed(nameof(IsDarkAppearance)); Changed(nameof(IsLightAppearance)); AppearanceChanged?.Invoke(this, EventArgs.Empty);
    }
    private async Task SavePreferencesAsync()
    {
        if (_persist is null || LoadedSettings?.CanSave != true) return;
        try { await _persist(CaptureSession(LoadedSettings.Settings)); }
        catch (Exception) { SettingsNotice = UiText.Get("WorkspacePreferenceSaveFailed"); }
    }
    private async Task OpenToolAsync(RepositoryTool tool)
    {
        var tab = SelectedRepository!;
        _launchingTool = true; _showWorkspaceStatus = true; RefreshCommands();
        Status = UiText.Get("WorkspaceToolStarting");
        using var cancel = new CancellationTokenSource(TimeSpan.FromSeconds(15));
        try
        {
            var configured = tool switch { RepositoryTool.Rider => _riderExecutable, RepositoryTool.Code => _codeExecutable, RepositoryTool.Bash => _bashExecutable, _ => null };
            var request = await _repositoryTools.PrepareAsync(tool, tab.Directory, _git?.Executable, configured, cancel.Token);
            if (_stopping) return;
            if (request is null && tool != RepositoryTool.Explorer)
            {
                cancel.CancelAfter(Timeout.InfiniteTimeSpan); // User file selection has no countdown.
                var chosen = _interaction!.ChooseWorkspaceExecutable(tool);
                if (chosen is null) { if (SelectedRepository == tab) Status = UiText.Get("WorkspaceToolNotOpened"); return; }
                cancel.CancelAfter(TimeSpan.FromSeconds(15));
                request = await _repositoryTools.PrepareAsync(tool, tab.Directory, _git?.Executable, chosen, cancel.Token);
                if (request is null) throw new System.IO.FileNotFoundException();
                if (tool == RepositoryTool.Rider) _riderExecutable = chosen;
                else if (tool == RepositoryTool.Code) _codeExecutable = chosen;
                else _bashExecutable = chosen;
                PreferenceTask = SavePreferencesAsync();
            }
            if (_stopping) return;
            await _repositoryTools.LaunchAsync(request ?? throw new System.IO.FileNotFoundException(), cancel.Token);
            if (SelectedRepository == tab) Status = UiText.Get("WorkspaceToolOpened");
        }
        catch (Exception) { if (SelectedRepository == tab && !_stopping) Status = UiText.Get("WorkspaceToolFailed"); }
        finally { _launchingTool = false; RefreshCommands(); }
    }
    private void RefreshWorkspaceTools()
    { OpenToolCommand?.Refresh(); AppearanceCommand?.Refresh(); Changed(nameof(CanOpenTools)); Changed(nameof(WorkspaceToolsHelp)); }
}
