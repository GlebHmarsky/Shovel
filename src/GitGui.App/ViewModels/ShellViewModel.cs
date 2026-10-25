using System.Collections.ObjectModel;
using System.Diagnostics;
using System.Windows.Threading;
using GitGui.App.Localization;
using GitGui.App.Services;
using GitGui.Core.Diagnostics;
using GitGui.Core.Git;
using GitGui.Core.Settings;
using GitGui.ProcessHost;

namespace GitGui.App.ViewModels;

public sealed record HistoryScopeItem(HistoryScope Scope, string Label);
public sealed record NavigationItem(NavigationSection Section, string Label);

public sealed class ShellViewModel : ObservableObject
{
    private readonly RepositoryReader? _reader;
    private readonly IUserInteraction? _interaction;
    private readonly IRepositoryIdentityProvider _identities;
    private readonly Func<AppSettings, Task>? _persist;
    private readonly PerformanceTrace _trace;
    private readonly Dictionary<string, RepositoryApproval> _approvals = new(StringComparer.OrdinalIgnoreCase);
    private readonly DispatcherTimer _timer = new() { Interval = TimeSpan.FromMilliseconds(250) };
    private readonly Stopwatch _elapsed = new();
    private CancellationTokenSource? _operationCancellation;
    private Task _activeOperation = Task.CompletedTask;
    private GitInstallation? _git;
    private string? _explicitGit;
    private long _generation, _useCounter;
    private bool _stopping, _publishingState;
    public static TimeSpan RefreshAge { get; } = TimeSpan.FromSeconds(30);
    public const int CachedTabLimit = 3;
    private HistoryViewState? State => SelectedRepository?.History;
    public IReadOnlyList<NavigationItem> Sections { get; } = [new(NavigationSection.History, UiText.Get("History"))];
    public ObservableCollection<RepositoryTab> Repositories { get; } = [];
    public string Version => UiText.Format("VersionFormat", typeof(ShellViewModel).Assembly.GetName().Version?.ToString(3) ?? "0.0.0");
    private bool _isReady;
    public bool IsReady { get => _isReady; private set { if (Set(ref _isReady, value)) { RefreshCommands(); Changed(nameof(CanNavigate)); } } }
    private bool _isBusy;
    public bool IsBusy { get => _isBusy; private set { if (Set(ref _isBusy, value)) { RefreshCommands(); Changed(nameof(OperationStatus)); } } }
    public bool CanNavigate => IsReady && !_stopping;
    public string OperationStatus => IsBusy ? UiText.Format("InspectingDuration", _elapsed.Elapsed.TotalSeconds) : UiText.Get("NoActiveOperation");
    private string _status = UiText.Get("LoadingSettings");
    public string Status { get => _status; private set => Set(ref _status, value); }
    public string RawOutput => State?.RawOutput ?? "";
    private string _settingsNotice = "";
    public string SettingsNotice { get => _settingsNotice; private set => Set(ref _settingsNotice, value); }
    public string GitStatus => _git is null ? UiText.Get("GitNotChecked") : UiText.Format("GitVersionFormat", _git.Version);
    public string TimingDetails => string.Join(Environment.NewLine, _trace.Snapshot().Select(sample => UiText.Format("TimingSample", UiText.Get("Timing" + sample.Stage), sample.Milliseconds)));
    public IReadOnlyList<HistoryScopeItem> HistoryScopes { get; } = [new(HistoryScope.AllCommits, UiText.Get("AllCommits")), new(HistoryScope.CurrentHead, UiText.Get("CurrentHeadHistory"))];
    public HistoryScopeItem SelectedHistoryScope
    {
        get => HistoryScopes.Single(item => item.Scope == (State?.Scope ?? HistoryScope.AllCommits));
        set
        {
            if (value is null || State is not { } state || state.Scope == value.Scope) return;
            state.Scope = value.Scope;
            // A new scope must not mislabel rows from the preceding scope.
            state.Clear(); PublishState();
            if (CanNavigate) BeginInspection(SelectedRepository!, force: true);
        }
    }
    public string HistoryScopeDescription => UiText.Get(SelectedHistoryScope.Scope == HistoryScope.AllCommits ? "AllCommitsScope" : "CurrentHeadScope");
    public string HistoryScopeHelp => UiText.Get(SelectedHistoryScope.Scope == HistoryScope.AllCommits ? "AllCommitsHelp" : "CurrentHeadHelp");
    public string HistoryRange => State?.Page is not { } page || Commits.Count == 0 ? "" : UiText.Format("HistoryRange", page.Cursor.Offset + 1, page.Cursor.Offset + Commits.Count);
    public IReadOnlyList<CommitRow> Commits => State?.Rows ?? [];
    public double GraphWidth => Views.CommitGraphCell.WidthFor(Commits.Count == 0 ? 1 : Commits.Max(row => row.Graph.LaneCount));
    public CommitRow? SelectedCommit
    {
        get => State?.Selected;
        set
        {
            if (_publishingState || State is not { } state || state.Selected == value || (value is not null && !state.Rows.Contains(value))) return;
            state.Selected = value; HighlightAuthors(); Changed();
        }
    }
    public string HistoryNotice => State?.Notice ?? UiText.Get("HistoryNotLoaded");
    public bool IsHistory => true;
    public NavigationItem SelectedSection { get => Sections[0]; set { } }
    private RepositoryTab? _selectedRepository;
    public RepositoryTab? SelectedRepository
    {
        get => _selectedRepository;
        set
        {
            if (_selectedRepository == value) return;
            HistoryViewChanging?.Invoke(this, EventArgs.Empty);
            _operationCancellation?.Cancel(); _generation++;
            _selectedRepository = value;
            if (value is not null) value.History.LastUse = ++_useCounter;
            Status = value?.History.Error is { Length: > 0 } error ? error : UiText.Get(value?.History.Page is null ? "HistoryNotLoaded" : "HistoryCheckingCached");
            Changed(); Changed(nameof(RepositoryTitle)); PublishState();
            if (CanNavigate && value is not null) BeginInspection(value);
        }
    }
    // View-only scroll offsets are captured/restored by code-behind; no filesystem/UI coupling.
    public event EventHandler? HistoryViewChanging;
    public event EventHandler? HistoryViewChanged;
    public (double Vertical, double Horizontal) HistoryScroll => (State?.VerticalOffset ?? 0, State?.HorizontalOffset ?? 0);
    public void SaveHistoryScroll(double vertical, double horizontal)
    { if (State is { } state) { state.VerticalOffset = vertical; state.HorizontalOffset = horizontal; } }
    public string RepositoryTitle => SelectedRepository?.Label ?? UiText.Get("NoRepositorySelected");
    public string AreaTitle => SelectedSection.Label;
    public SettingsLoadResult? LoadedSettings { get; private set; }
    private int _detailsIndex;
    public int DetailsIndex { get => _detailsIndex; set => Set(ref _detailsIndex, value); }
    public RelayCommand OpenRepositoryCommand { get; }
    public RelayCommand RefreshRepositoryCommand { get; }
    public RelayCommand CancelCommand { get; }
    public RelayCommand ChooseGitCommand { get; }
    public RelayCommand ApproveRepositoryCommand { get; }
    public RelayCommand RevokeApprovalCommand { get; }
    public RelayCommand DiagnosticsCommand { get; }
    public RelayCommand CloseRepositoryCommand { get; }
    public RelayCommand MoveLeftCommand { get; }
    public RelayCommand MoveRightCommand { get; }
    public RelayCommand LoadMoreHistoryCommand { get; }
    public RelayCommand NewerHistoryCommand { get; }
    public RelayCommand FirstHistoryCommand { get; }

    public ShellViewModel(RepositoryReader? reader = null, IUserInteraction? interaction = null,
        IRepositoryIdentityProvider? identities = null, Func<AppSettings, Task>? persist = null, PerformanceTrace? trace = null)
    {
        _reader = reader; _interaction = interaction; _identities = identities ?? new WindowsRepositoryIdentity();
        _persist = persist; _trace = trace ?? new();
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
            BeginInspection(tab, approve: true);
        }, () => CanNavigate && !IsBusy && _reader is not null && _interaction is not null);
        RefreshRepositoryCommand = new(() => BeginInspection(SelectedRepository!, force: true), () => CanRead && !IsBusy);
        ApproveRepositoryCommand = new(() => BeginInspection(SelectedRepository!, approve: true, force: true), () => CanRead && !IsBusy && _interaction is not null);
        RevokeApprovalCommand = new(RevokeApproval, () => CanNavigate && SelectedRepository is not null && _approvals.ContainsKey(SelectedRepository.Directory));
        LoadMoreHistoryCommand = new(() => BeginInspection(SelectedRepository!, direction: 1), () => CanRead && !IsBusy && State?.Page?.HasMore == true && State.NeedsRefresh == false);
        NewerHistoryCommand = new(() => BeginInspection(SelectedRepository!, direction: -1), () => CanRead && !IsBusy && State?.PageIndex > 0 && State.NeedsRefresh == false);
        FirstHistoryCommand = new(() => BeginInspection(SelectedRepository!, force: true, firstPage: true), () => CanRead && !IsBusy && State?.Page?.Cursor.Offset > 0);
        CancelCommand = new(() => { _operationCancellation?.Cancel(); }, () => IsBusy);
        ChooseGitCommand = new(() =>
        {
            var path = _interaction!.ChooseGit(); if (path is null) return;
            _explicitGit = path; _git = null; Changed(nameof(GitStatus));
            foreach (var tab in Repositories) tab.History.ValidatedAt = default;
            if (SelectedRepository is not null) BeginInspection(SelectedRepository!, force: true);
        }, () => CanNavigate && !IsBusy && _interaction is not null);
        DiagnosticsCommand = new(() => { Changed(nameof(TimingDetails)); DetailsIndex = 2; }, () => CanNavigate);
        CloseRepositoryCommand = new(() =>
        {
            var tab = SelectedRepository!; _operationCancellation?.Cancel();
            Repositories.Remove(tab); _approvals.Remove(tab.Directory);
            SelectedRepository = Repositories.FirstOrDefault();
        }, () => CanNavigate && !IsBusy && SelectedRepository is not null);
        MoveLeftCommand = new(() => MoveTab(-1), () => CanNavigate && SelectedRepository is not null && Repositories.IndexOf(SelectedRepository) > 0);
        MoveRightCommand = new(() => MoveTab(1), () => CanNavigate && SelectedRepository is not null && Repositories.IndexOf(SelectedRepository) < Repositories.Count - 1);
        _timer.Tick += (_, _) => Changed(nameof(OperationStatus));
    }
    private bool CanRead => CanNavigate && SelectedRepository is not null && _reader is not null;

    public void CompleteStartup(SettingsLoadResult result)
    {
        LoadedSettings = result;
        Repositories.Clear(); _approvals.Clear();
        foreach (var path in result.Settings.RepositoryPaths)
        {
            var tab = new RepositoryTab(path);
            tab.History.Scope = result.Settings.RepositoryPreferences.FirstOrDefault(item => item.Path.Equals(path, StringComparison.OrdinalIgnoreCase))?.Scope ?? HistoryScope.AllCommits;
            Repositories.Add(tab);
        }
        foreach (var approval in result.Settings.Approvals)
            if (Repositories.Any(tab => tab.Directory.Equals(approval.Path, StringComparison.OrdinalIgnoreCase))) _approvals[approval.Path] = approval;
        SelectedRepository = Repositories.FirstOrDefault(item => string.Equals(item.Directory, result.Settings.SelectedRepositoryPath, StringComparison.OrdinalIgnoreCase)) ?? Repositories.FirstOrDefault();
        SettingsNotice = result.CanSave ? "" : UiText.Get(result.Status switch
        {
            SettingsLoadStatus.NewerSchema => "SettingsNewer", SettingsLoadStatus.UnsupportedSchema => "SettingsUnsupported",
            SettingsLoadStatus.Invalid => "SettingsInvalid", _ => "SettingsUnavailable"
        });
        IsReady = true; Status = result.CanSave ? UiText.Get("FoundationReady") : SettingsNotice;
        if (SelectedRepository is not null) BeginInspection(SelectedRepository);
    }

    public void RefreshIfStale()
    {
        if (CanRead && !IsBusy && State?.Page is not null && DateTimeOffset.UtcNow - State.ValidatedAt >= RefreshAge)
            BeginInspection(SelectedRepository!);
    }

    private void BeginInspection(RepositoryTab tab, bool approve = false, bool force = false, int direction = 0, bool firstPage = false)
    {
        if (_reader is null || !CanNavigate) return;
        _operationCancellation?.Cancel();
        var prior = _activeOperation;
        var generation = ++_generation;
        var cancellation = new CancellationTokenSource(TimeSpan.FromSeconds(45));
        _operationCancellation = cancellation;
        _elapsed.Restart(); IsBusy = true; _timer.Start();
        _activeOperation = InspectAsync();

        async Task InspectAsync()
        {
            var state = tab.History;
            try
            {
                await prior; // Drain owned cancelled work before admitting the next read.
                if (Obsolete()) return;
                if (!approve && !_approvals.ContainsKey(tab.Directory)) { state.Notice = UiText.Get("ApprovalRequired"); Status = state.Notice; return; }
                state.Notice = UiText.Get(state.Page is null ? "HistoryLoading" : "HistoryCheckingCached"); Changed(nameof(HistoryNotice));
                RepositoryIdentity identity;
                try
                {
                    using (_trace.Measure(TimingStage.Identity)) identity = await _identities.ReadAsync(tab.Directory, cancellation.Token);
                }
                catch (Exception exception) when (exception is System.IO.IOException or UnauthorizedAccessException or ArgumentException or System.ComponentModel.Win32Exception)
                {
                    if (Obsolete()) return;
                    state.Clear(); tab.Invalidate(); _approvals.Remove(tab.Directory);
                    state.Notice = UiText.Get("ApprovalIdentityUnavailable"); Status = state.Notice;
                    await PersistApprovalsAsync(); return;
                }
                if (Obsolete()) return;
                if (!_approvals.TryGetValue(tab.Directory, out var approval) || approval.Fingerprint != identity.Fingerprint)
                {
                    state.Clear(); tab.Invalidate(); _approvals.Remove(tab.Directory);
                    if (!approve)
                    {
                        state.Notice = UiText.Get("ApprovalChanged"); Status = state.Notice;
                        await PersistApprovalsAsync(); return;
                    }
                    if (!_interaction!.ConfirmTrust(tab.Directory)) { state.Notice = UiText.Get("TrustDeclined"); Status = state.Notice; return; }
                    _approvals[tab.Directory] = new(tab.Directory, identity.Fingerprint);
                    if (!await PersistApprovalsAsync()) { _approvals.Remove(tab.Directory); state.Notice = UiText.Get("ApprovalRequired"); Status = SettingsNotice; return; }
                    if (Obsolete()) return;
                    // Recheck after a user prompt or settings write; never grant a different target consent.
                    if ((await _identities.ReadAsync(tab.Directory, cancellation.Token)).Fingerprint != identity.Fingerprint)
                    { await RevokeChangedIdentityAsync(); return; }
                }
                if (!force && direction == 0 && state.Page is not null && !state.NeedsRefresh && DateTimeOffset.UtcNow - state.ValidatedAt < RefreshAge)
                { state.Notice = LoadedNotice(state.Page); Status = UiText.Get("HistoryCached"); return; }
                state.Notice = UiText.Get(state.Page is null ? "HistoryLoading" : "HistoryRefreshing"); Changed(nameof(HistoryNotice));
                state.ValidatedAt = default; // A failed/cancelled refresh cannot later masquerade as fresh cached data.
                Status = UiText.Get("InspectingRepository");
                _git ??= await _reader.DiscoverAsync(_explicitGit, cancellation.Token); Changed(nameof(GitStatus));
                var previous = state.Page;
                var snapshot = direction == 0 ? await _reader.InspectAsync(_git, tab.Directory, cancellation.Token) : tab.Snapshot!;
                if (Obsolete()) return;
                if (!string.Equals(snapshot.Worktree, identity.Worktree, StringComparison.OrdinalIgnoreCase) ||
                    !string.Equals(snapshot.GitDirectory, identity.GitDirectory, StringComparison.OrdinalIgnoreCase) ||
                    !string.Equals(snapshot.CommonDirectory, identity.CommonDirectory, StringComparison.OrdinalIgnoreCase))
                { await RevokeChangedIdentityAsync(); return; }
                CommitHistory history;
                var keepPage = false;
                if (direction != 0)
                    history = await _reader.ReadHistoryPageAsync(_git, previous!.Session, direction > 0 ? previous.Next! : state.Cursors[state.PageIndex - 1], cancellation.Token);
                else if (!firstPage && previous is not null && previous.Session.Scope == state.Scope)
                {
                    var session = await _reader.OpenHistoryAsync(_git, snapshot, state.Scope, cancellation.Token);
                    keepPage = session.Fingerprint == previous.Session.Fingerprint && snapshot.Commit == previous.Session.Repository.Commit;
                    history = await _reader.ReadHistoryPageAsync(_git, session, keepPage ? previous.Cursor : HistoryCursor.Start, cancellation.Token);
                }
                else history = await _reader.ReadInitialHistoryAsync(_git, snapshot, state.Scope, cancellation.Token);
                RepositoryIdentity currentIdentity;
                try { currentIdentity = await _identities.ReadAsync(tab.Directory, cancellation.Token); }
                catch (Exception exception) when (exception is System.IO.IOException or UnauthorizedAccessException or ArgumentException or System.ComponentModel.Win32Exception)
                { await RevokeChangedIdentityAsync(); return; }
                if (currentIdentity.Fingerprint != identity.Fingerprint) { await RevokeChangedIdentityAsync(); return; }
                (CommitRow[] Rows, long Bytes) prepared;
                using (_trace.Measure(TimingStage.Formatting)) prepared = await Task.Run(() =>
                {
                    var formatted = history.Entries.Select(entry => new CommitRow(entry)).ToArray();
                    var bytes = formatted.Sum(row => 2L * (row.Commit.Message.Length + row.RefLabels.Length + row.AuthorDetails.Length + row.CommitterDetails.Length) + 4096) +
                        history.Session.References.Sum(reference => 2L * (reference.Name.Length + reference.Hash.Length) + 128);
                    return (formatted, bytes);
                }, cancellation.Token);
                var rows = prepared.Rows;
                if (Obsolete()) return;
                HistoryViewChanging?.Invoke(this, EventArgs.Empty);
                var selectedHash = state.Selected?.Commit.Hash;
                if (previous is not null)
                {
                    state.PageViews[previous.Cursor.Offset] = new(selectedHash, state.VerticalOffset, state.HorizontalOffset);
                    while (state.PageViews.Count > 128) state.PageViews.Remove(state.PageViews.Keys.Min());
                }
                if (direction == 0 && !keepPage) { state.Cursors.Clear(); state.Cursors.Add(history.Cursor); state.PageIndex = 0; }
                else if (direction != 0)
                {
                    state.PageIndex += direction;
                    if (state.Cursors.Count == state.PageIndex) state.Cursors.Add(history.Cursor);
                    // Bounded backwards window; First page is always available without a total history limit.
                    if (state.Cursors.Count > 128) { state.Cursors.RemoveAt(0); state.PageIndex--; }
                }
                if (!keepPage)
                {
                    state.PageViews.TryGetValue(history.Cursor.Offset, out var savedView);
                    selectedHash = savedView?.SelectedHash ?? selectedHash;
                    state.VerticalOffset = savedView?.Vertical ?? 0; state.HorizontalOffset = savedView?.Horizontal ?? 0;
                }
                state.Page = history; state.Rows = rows; state.EstimatedBytes = prepared.Bytes;
                state.Selected = rows.FirstOrDefault(row => row.Commit.Hash == selectedHash) ?? rows.FirstOrDefault();
                state.ValidatedAt = DateTimeOffset.UtcNow; state.NeedsRefresh = false;
                state.Notice = LoadedNotice(history); state.Error = state.RawOutput = "";
                tab.Apply(snapshot); EvictCaches(tab); Status = UiText.Get("HistoryLoaded");
            }
            catch (GitReadException exception)
            {
                if (!Obsolete())
                {
                    state.NeedsRefresh = exception.Failure == GitFailure.ChangedDuringRead;
                    state.Notice = UiText.Get(state.Rows.Count == 0 ? "HistoryFailed" : "HistoryRefreshFailed");
                    state.Error = UiText.Get("GitError" + exception.Failure); state.RawOutput = exception.RawOutput; Status = state.Error;
                }
            }
            catch (OperationCanceledException)
            { if (!Obsolete(ignoreCancellation: true)) { state.Notice = UiText.Get(state.Rows.Count == 0 ? "HistoryCancelled" : "HistoryRefreshCancelled"); Status = UiText.Get("GitErrorCancelled"); } }
            catch (Exception)
            { if (!Obsolete()) { state.Notice = UiText.Get(state.Rows.Count == 0 ? "HistoryFailed" : "HistoryRefreshFailed"); Status = UiText.Get("GitErrorUnexpected"); } }
            finally
            {
                if (ReferenceEquals(_operationCancellation, cancellation)) _operationCancellation = null;
                cancellation.Dispose();
                if (generation == _generation)
                {
                    if (cancellation.IsCancellationRequested && !_stopping)
                    { state.Notice = UiText.Get(state.Rows.Count == 0 ? "HistoryCancelled" : "HistoryRefreshCancelled"); Status = UiText.Get("GitErrorCancelled"); }
                    _timer.Stop(); _elapsed.Stop(); _operationCancellation = null; IsBusy = false;
                    Changed(nameof(OperationStatus)); PublishState();
                }
            }
            async Task RevokeChangedIdentityAsync()
            {
                if (Obsolete()) return;
                state.Clear(); tab.Invalidate(); _approvals.Remove(tab.Directory);
                state.Notice = UiText.Get("ApprovalChanged"); Status = state.Notice;
                await PersistApprovalsAsync();
            }
            bool Obsolete(bool ignoreCancellation = false) => _stopping || generation != _generation || !Repositories.Contains(tab) || (!ignoreCancellation && cancellation.IsCancellationRequested);
        }
    }
    private static string LoadedNotice(CommitHistory history) => UiText.Get(history.Entries.Count == 0
        ? history.Session.Scope == HistoryScope.AllCommits ? "HistoryEmptyAll" : "HistoryUnborn"
        : history.HasMore ? "HistoryMore" : history.IsShallow ? "HistoryShallowEnd" : "HistoryEnd");
    private void EvictCaches(RepositoryTab active)
    {
        // At most three pages AND a conservative 32 MiB character-data estimate across them.
        var cached = Repositories.Where(tab => tab.History.Page is not null).OrderBy(tab => tab.History.LastUse).ToList();
        while (cached.Count > CachedTabLimit || (cached.Count > 1 && cached.Sum(tab => tab.History.EstimatedBytes) > 32 * 1024 * 1024))
        {
            var oldest = cached.First(tab => tab != active); oldest.History.Clear(); cached.Remove(oldest);
        }
    }
    private async Task<bool> PersistApprovalsAsync()
    {
        if (_persist is null) return true; // Tests can observe CaptureSession instead of touching a real profile.
        try { await _persist(CaptureSession(LoadedSettings?.Settings ?? new())); return true; }
        catch (Exception) { SettingsNotice = UiText.Get("ApprovalSaveFailed"); return false; }
    }
    private void RevokeApproval()
    {
        var tab = SelectedRepository!; _operationCancellation?.Cancel(); _generation++;
        var generation = _generation; IsBusy = true;
        _approvals.Remove(tab.Directory); tab.History.Clear(); tab.Invalidate();
        tab.History.Notice = UiText.Get("ApprovalRevoked"); Status = tab.History.Notice; PublishState();
        var prior = _activeOperation;
        _activeOperation = SaveAsync();
        async Task SaveAsync()
        {
            await prior; await PersistApprovalsAsync();
            if (generation == _generation) { _operationCancellation = null; _timer.Stop(); _elapsed.Stop(); IsBusy = false; RefreshCommands(); }
        }
    }
    private void HighlightAuthors()
    { foreach (var row in Commits) row.IsSameAuthor = row != SelectedCommit && row.HasSameAuthor(SelectedCommit); }
    private void PublishState()
    {
        using (_trace.Measure(TimingStage.Binding))
        {
            _publishingState = true;
            try
            {
                HighlightAuthors();
                foreach (var property in new[] { nameof(Commits), nameof(SelectedCommit), nameof(SelectedHistoryScope), nameof(HistoryScopeDescription), nameof(HistoryScopeHelp), nameof(HistoryRange), nameof(HistoryNotice), nameof(GraphWidth), nameof(RawOutput), nameof(TimingDetails) }) Changed(property);
                RefreshCommands(); HistoryViewChanged?.Invoke(this, EventArgs.Empty);
            } finally { _publishingState = false; }
        }
    }
    private void MoveTab(int offset)
    { var tab = SelectedRepository!; var index = Repositories.IndexOf(tab); Repositories.Move(index, index + offset); RefreshCommands(); }
    private void RefreshCommands()
    {
        OpenRepositoryCommand?.Refresh(); RefreshRepositoryCommand?.Refresh(); CancelCommand?.Refresh(); ChooseGitCommand?.Refresh();
        ApproveRepositoryCommand?.Refresh(); RevokeApprovalCommand?.Refresh(); DiagnosticsCommand?.Refresh();
        CloseRepositoryCommand?.Refresh(); MoveLeftCommand?.Refresh(); MoveRightCommand?.Refresh();
        LoadMoreHistoryCommand?.Refresh(); NewerHistoryCommand?.Refresh(); FirstHistoryCommand?.Refresh();
    }
    public AppSettings CaptureSession(AppSettings layout) => layout with
    {
        RepositoryPaths = Repositories.Select(tab => tab.Directory).ToArray(), SelectedRepositoryPath = SelectedRepository?.Directory,
        Approvals = _approvals.Values.ToArray(), RepositoryPreferences = Repositories.Select(tab => new RepositoryPreference(tab.Directory, tab.History.Scope)).ToArray()
    };
    public void BeginShutdown() { _stopping = true; _operationCancellation?.Cancel(); IsReady = false; Status = UiText.Get("SavingSettings"); }
    public async Task WaitForIdleAsync() { Task task; do { task = _activeOperation; await task; } while (task != _activeOperation); }
    public void ResumeAfterSaveFailure() { _stopping = false; IsReady = true; Status = UiText.Get("SettingsSaveFailedStatus"); }
}
