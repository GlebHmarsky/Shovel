using System.Collections.ObjectModel;
using System.Diagnostics;
using System.Windows.Threading;
using GitGui.App.Localization;
using GitGui.App.Services;
using GitGui.Core.Git;
using GitGui.Core.Settings;

namespace GitGui.App.ViewModels;

public sealed record NavigationItem(NavigationSection Section, string Label);

public sealed class ShellViewModel : ObservableObject
{
    private readonly RepositoryReader? _reader;
    private readonly IUserInteraction? _interaction;
    private readonly HashSet<string> _trustedPaths = new(StringComparer.OrdinalIgnoreCase);
    private readonly DispatcherTimer _timer = new() { Interval = TimeSpan.FromMilliseconds(250) };
    private readonly Stopwatch _elapsed = new();
    private CancellationTokenSource? _operationCancellation;
    private Task _activeOperation = Task.CompletedTask;
    private GitInstallation? _git;
    private string? _explicitGit;
    private long _generation;
    private bool _stopping;
    public IReadOnlyList<NavigationItem> Sections { get; } = Enum.GetValues<NavigationSection>()
        .Select(section => new NavigationItem(section, UiText.Get(section.ToString()))).ToArray();
    public ObservableCollection<RepositoryTab> Repositories { get; } = [];
    public string Version => UiText.Format("VersionFormat", typeof(ShellViewModel).Assembly.GetName().Version?.ToString(3) ?? "0.0.0");
    private bool _isReady;
    public bool IsReady { get => _isReady; private set { if (Set(ref _isReady, value)) { RefreshCommands(); Changed(nameof(CanNavigate)); } } }
    private bool _isBusy;
    public bool IsBusy { get => _isBusy; private set { if (Set(ref _isBusy, value)) { RefreshCommands(); Changed(nameof(CanNavigate)); Changed(nameof(OperationStatus)); } } }
    public bool CanNavigate => IsReady && !IsBusy;
    public string OperationStatus => IsBusy ? UiText.Format("InspectingDuration", _elapsed.Elapsed.TotalSeconds) : UiText.Get("NoActiveOperation");
    private string _status = UiText.Get("LoadingSettings");
    public string Status { get => _status; private set => Set(ref _status, value); }
    private string _rawOutput = "";
    public string RawOutput { get => _rawOutput; private set => Set(ref _rawOutput, value); }
    private string _settingsNotice = "";
    public string SettingsNotice { get => _settingsNotice; private set => Set(ref _settingsNotice, value); }
    public string GitStatus => _git is null ? UiText.Get("GitNotChecked") : UiText.Format("GitVersionFormat", _git.Version);
    private NavigationItem _selectedSection;
    public NavigationItem SelectedSection
    {
        get => _selectedSection;
        set { if (value is not null && Set(ref _selectedSection, value)) Changed(nameof(AreaTitle)); }
    }
    private RepositoryTab? _selectedRepository;
    public RepositoryTab? SelectedRepository
    {
        get => _selectedRepository;
        set { if (Set(ref _selectedRepository, value)) { _generation++; RawOutput = ""; RefreshCommands(); Changed(nameof(RepositoryTitle)); } }
    }
    public string RepositoryTitle => SelectedRepository?.Label ?? UiText.Get("NoRepositorySelected");
    public string AreaTitle => SelectedSection.Label;
    public SettingsLoadResult? LoadedSettings { get; private set; }
    public RelayCommand OpenRepositoryCommand { get; }
    public RelayCommand RefreshRepositoryCommand { get; }
    public RelayCommand CancelCommand { get; }
    public RelayCommand ChooseGitCommand { get; }
    public RelayCommand CloseRepositoryCommand { get; }
    public RelayCommand MoveLeftCommand { get; }
    public RelayCommand MoveRightCommand { get; }

    public ShellViewModel(RepositoryReader? reader = null, IUserInteraction? interaction = null)
    {
        _reader = reader;
        _interaction = interaction;
        _selectedSection = Sections.Single(item => item.Section == NavigationSection.History);
        OpenRepositoryCommand = new(() =>
        {
            var path = _interaction!.ChooseRepository();
            if (path is null) return;
            var tab = Repositories.FirstOrDefault(item => string.Equals(item.Directory, path, StringComparison.OrdinalIgnoreCase));
            if (tab is null)
            {
                if (Repositories.Count >= 20) { Status = UiText.Get("TooManyRepositories"); return; }
                tab = new(path); Repositories.Add(tab);
            }
            SelectedRepository = tab;
            BeginInspection(tab);
        }, () => CanNavigate && _reader is not null);
        RefreshRepositoryCommand = new(() => BeginInspection(SelectedRepository!), () => CanNavigate && SelectedRepository is not null && _reader is not null);
        CancelCommand = new(() => _operationCancellation?.Cancel(), () => IsBusy);
        ChooseGitCommand = new(() =>
        {
            var path = _interaction!.ChooseGit();
            if (path is null) return;
            _explicitGit = path; _git = null; Changed(nameof(GitStatus));
            Status = UiText.Get("GitSelected");
        }, () => CanNavigate && _interaction is not null);
        CloseRepositoryCommand = new(() =>
        {
            var tab = SelectedRepository!;
            Repositories.Remove(tab); _trustedPaths.Remove(tab.Directory);
            SelectedRepository = Repositories.FirstOrDefault();
        }, () => CanNavigate && SelectedRepository is not null);
        MoveLeftCommand = new(() => MoveTab(-1), () => CanNavigate && SelectedRepository is not null && Repositories.IndexOf(SelectedRepository) > 0);
        MoveRightCommand = new(() => MoveTab(1), () => CanNavigate && SelectedRepository is not null && Repositories.IndexOf(SelectedRepository) < Repositories.Count - 1);
        _timer.Tick += (_, _) => Changed(nameof(OperationStatus));
    }

    public void CompleteStartup(SettingsLoadResult result)
    {
        LoadedSettings = result;
        SelectedSection = Sections.Single(item => item.Section == result.Settings.SelectedSection);
        Repositories.Clear();
        foreach (var path in result.Settings.RepositoryPaths) Repositories.Add(new(path));
        SelectedRepository = Repositories.FirstOrDefault(item => string.Equals(item.Directory, result.Settings.SelectedRepositoryPath, StringComparison.OrdinalIgnoreCase)) ?? Repositories.FirstOrDefault();
        SettingsNotice = result.CanSave ? "" : UiText.Get(result.Status switch
        {
            SettingsLoadStatus.NewerSchema => "SettingsNewer",
            SettingsLoadStatus.UnsupportedSchema => "SettingsUnsupported",
            SettingsLoadStatus.Invalid => "SettingsInvalid",
            _ => "SettingsUnavailable"
        });
        IsReady = true;
        Status = result.CanSave ? UiText.Get("FoundationReady") : SettingsNotice;
    }

    private void BeginInspection(RepositoryTab tab)
    {
        if (!_trustedPaths.Contains(tab.Directory))
        {
            if (!_interaction!.ConfirmTrust(tab.Directory)) { Status = UiText.Get("TrustDeclined"); return; }
            _trustedPaths.Add(tab.Directory);
        }
        _activeOperation = InspectAsync(tab);
    }

    private async Task InspectAsync(RepositoryTab tab)
    {
        var generation = ++_generation;
        using var cancellation = new CancellationTokenSource(TimeSpan.FromSeconds(45));
        _operationCancellation = cancellation;
        _elapsed.Restart(); IsBusy = true; _timer.Start(); RawOutput = "";
        tab.Invalidate();
        Status = UiText.Get("InspectingRepository");
        try
        {
            _git ??= await _reader!.DiscoverAsync(_explicitGit, cancellation.Token);
            Changed(nameof(GitStatus));
            var snapshot = await _reader!.InspectAsync(_git, tab.Directory, cancellation.Token);
            if (_stopping || generation != _generation || !Repositories.Contains(tab)) return;
            tab.Apply(snapshot);
            Status = UiText.Get("RepositoryInspected");
        }
        catch (GitReadException exception)
        {
            if (!_stopping && generation == _generation)
            { Status = UiText.Get("GitError" + exception.Failure); RawOutput = exception.RawOutput; }
        }
        catch (OperationCanceledException) { if (!_stopping) Status = UiText.Get("GitErrorCancelled"); }
        catch (Exception) { if (!_stopping) Status = UiText.Get("GitErrorUnexpected"); }
        finally
        {
            _timer.Stop(); _elapsed.Stop(); _operationCancellation = null; IsBusy = false;
            Changed(nameof(OperationStatus));
        }
    }

    private void MoveTab(int offset)
    {
        var tab = SelectedRepository!;
        var index = Repositories.IndexOf(tab);
        Repositories.Move(index, index + offset);
        SelectedRepository = tab;
        RefreshCommands();
    }
    private void RefreshCommands()
    {
        OpenRepositoryCommand?.Refresh(); RefreshRepositoryCommand?.Refresh(); CancelCommand?.Refresh();
        ChooseGitCommand?.Refresh(); CloseRepositoryCommand?.Refresh(); MoveLeftCommand?.Refresh(); MoveRightCommand?.Refresh();
    }
    public AppSettings CaptureSession(AppSettings layout) => layout with
    {
        RepositoryPaths = Repositories.Select(tab => tab.Directory).ToArray(), SelectedRepositoryPath = SelectedRepository?.Directory
    };
    public void BeginShutdown() { _stopping = true; _generation++; IsReady = false; _operationCancellation?.Cancel(); Status = UiText.Get("SavingSettings"); }
    public Task WaitForIdleAsync() => _activeOperation;
    public void ResumeAfterSaveFailure() { _stopping = false; IsReady = true; Status = UiText.Get("SettingsSaveFailedStatus"); }
}
