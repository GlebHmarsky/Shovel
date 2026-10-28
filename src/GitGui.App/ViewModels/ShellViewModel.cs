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

public sealed partial class ShellViewModel : ObservableObject
{
    private readonly RepositoryReader? _reader;
    private readonly IUserInteraction? _interaction;
    private readonly IRepositoryIdentityProvider _identities;
    private readonly Func<AppSettings, Task>? _persist;
    private readonly PerformanceTrace _trace;
    private readonly Dictionary<string, RepositoryIdentityStamp> _knownIdentities = new(StringComparer.OrdinalIgnoreCase);
    private readonly DispatcherTimer _timer = new() { Interval = TimeSpan.FromMilliseconds(250) };
    private readonly Stopwatch _elapsed = new();
    private CancellationTokenSource? _operationCancellation;
    private Task _activeOperation = Task.CompletedTask;
    private GitInstallation? _git;
    private string? _explicitGit;
    private long _generation, _useCounter;
    private bool _stopping, _publishingState, _readingDivergence;
    public static TimeSpan RefreshAge { get; } = TimeSpan.FromSeconds(30);
    public const int CachedTabLimit = 3;
    private HistoryViewState? State => SelectedRepository?.History;
    public ObservableCollection<RepositoryTab> Repositories { get; } = [];
    public string Version => UiText.Format("VersionFormat", typeof(ShellViewModel).Assembly.GetName().Version?.ToString(3) ?? "0.0.0");
    private bool _isReady;
    public bool IsReady { get => _isReady; private set { if (Set(ref _isReady, value)) { RefreshCommands(); Changed(nameof(CanNavigate)); } } }
    private bool _isBusy;
    public bool IsBusy { get => _isBusy; private set { if (Set(ref _isBusy, value)) { RefreshCommands(); Changed(nameof(OperationStatus)); } } }
    public bool CanNavigate => IsReady && !_stopping && !ActionOpen;
    public string OperationStatus => _commitModel is not null ? _commitModel.Summary : IsActionRunning ? CurrentAction!.Summary :
        ActionOpen ? UiText.Get(_preparingAction ? "ActionPreparing" : "ActionReviewing") :
        IsBusy ? UiText.Format(_readingDivergence ? "ComparingDuration" : "InspectingDuration", _elapsed.Elapsed.TotalSeconds) :
        HasAction ? CurrentAction!.Summary : UiText.Get("NoActiveOperation");
    private string _status = UiText.Get("LoadingSettings");
    public string Status { get => _status; private set => Set(ref _status, value); }
    public string RawOutput => State?.RawOutput ?? "";
    private string _settingsNotice = "";
    public string SettingsNotice { get => _settingsNotice; private set => Set(ref _settingsNotice, value); }
    public string GitStatus => _git is null ? UiText.Get("GitNotChecked") : UiText.Format("GitVersionFormat", _git.Version);
    public string TimingDetails => string.Join(Environment.NewLine, _trace.Snapshot().Select(sample => UiText.Format("TimingSample", UiText.Get("Timing" + sample.Stage), sample.Milliseconds)));
    public string PullCount => Count(ahead: false);
    public string PushCount => Count(ahead: true);
    private string Count(bool ahead) => State?.Divergence is { State: DivergenceState.Available } value
        ? (ahead ? value.Ahead : value.Behind).ToString(System.Globalization.CultureInfo.CurrentCulture) : UiText.Get("CountUnavailable");
    public string DivergenceHelp => State?.Divergence is { State: DivergenceState.Available } value && SelectedRepository?.Snapshot is { } snapshot
        ? UiText.Format(State.ValidatedAt == default ? "DivergenceStale" : "DivergenceAvailable", value.Ahead, value.Behind,
            snapshot.UpstreamRef ?? "", snapshot.ReadAt.LocalDateTime)
        : UiText.Get("Divergence" + (State?.Divergence.State ?? DivergenceState.Pending));
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
    private RepositoryTab? _selectedRepository;
    public RepositoryTab? SelectedRepository
    {
        get => _selectedRepository;
        set
        {
            if (_selectedRepository == value || ActionOpen) return;
            HistoryViewChanging?.Invoke(this, EventArgs.Empty);
            _operationCancellation?.Cancel(); _generation++; CancelCountRead();
            _selectedRepository = value;
            if (value is not null) value.History.LastUse = ++_useCounter;
            Status = value?.History.Error is { Length: > 0 } error ? error : UiText.Get(value?.History.Page is null ? "HistoryNotLoaded" : "HistoryCheckingCached");
            Changed(); Changed(nameof(RepositoryTitle)); PublishState();
            if (CanNavigate && value is not null) BeginCurrentRead(value);
            else if (value is null) { _timer.Stop(); _elapsed.Stop(); IsBusy = false; }
        }
    }
    // View-only scroll offsets are captured/restored by code-behind; no filesystem/UI coupling.
    public event EventHandler? HistoryViewChanging;
    public event EventHandler? HistoryViewChanged;
    public (double Vertical, double Horizontal) HistoryScroll => (State?.VerticalOffset ?? 0, State?.HorizontalOffset ?? 0);
    public void SaveHistoryScroll(double vertical, double horizontal)
    { if (State is { } state) { state.VerticalOffset = vertical; state.HorizontalOffset = horizontal; } }
    public string RepositoryTitle => SelectedRepository?.Label ?? UiText.Get("NoRepositorySelected");
    public string AreaTitle => UiText.Get("AllCommits");
    public SettingsLoadResult? LoadedSettings { get; private set; }
    private int _detailsIndex;
    public int DetailsIndex { get => _detailsIndex; set => Set(ref _detailsIndex, value); }
    public RelayCommand<RepositoryTab> CloseTabCommand { get; }
    public RelayCommand<RepositoryTab> CloseOtherTabsCommand { get; }
    public RelayCommand<RepositoryTab> MoveTabLeftCommand { get; }
    public RelayCommand<RepositoryTab> MoveTabRightCommand { get; }
    public RelayCommand<RepositoryTab> RefreshTabCommand { get; }
    public RelayCommand OpenRepositoryCommand { get; }
    public RelayCommand RefreshRepositoryCommand { get; }
    public RelayCommand CancelCommand { get; }
    public RelayCommand ChooseGitCommand { get; }
    public RelayCommand DiagnosticsCommand { get; }
    public RelayCommand CloseRepositoryCommand { get; }
    public RelayCommand MoveLeftCommand { get; }
    public RelayCommand MoveRightCommand { get; }
    public RelayCommand LoadMoreHistoryCommand { get; }
    public RelayCommand NewerHistoryCommand { get; }
    public RelayCommand FirstHistoryCommand { get; }

    public ShellViewModel(RepositoryReader? reader = null, IUserInteraction? interaction = null,
        IRepositoryIdentityProvider? identities = null, Func<AppSettings, Task>? persist = null, PerformanceTrace? trace = null, GitActions? actions = null)
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
            BeginCurrentRead(tab, reopen: true);
        }, () => CanNavigate && !IsBusy && _reader is not null && _interaction is not null);
        RefreshRepositoryCommand = new(() => BeginCurrentRead(SelectedRepository!, force: true), () => CanRead && !IsBusy);
        LoadMoreHistoryCommand = new(() => BeginInspection(SelectedRepository!, direction: 1), () => CanRead && IsHistory && !IsBusy && State?.Page?.HasMore == true && State.NeedsRefresh == false);
        NewerHistoryCommand = new(() => BeginInspection(SelectedRepository!, direction: -1), () => CanRead && IsHistory && !IsBusy && State?.PageIndex > 0 && State.NeedsRefresh == false);
        FirstHistoryCommand = new(() => BeginInspection(SelectedRepository!, force: true, firstPage: true), () => CanRead && IsHistory && !IsBusy && State?.Page?.Cursor.Offset > 0);
        CancelCommand = new(() =>
        {
            if (_commitModel is not null) _commitModel.Cancel();
            else if (ActionOpen) CurrentAction?.Cancel();
            else _operationCancellation?.Cancel();
        }, () => ActionOpen ? CurrentAction?.CanCancel == true : IsBusy);
        ChooseGitCommand = new(() =>
        {
            var path = _interaction!.ChooseGit(); if (path is null) return;
            _explicitGit = path; _git = null; Changed(nameof(GitStatus));
            foreach (var tab in Repositories) { tab.History.ValidatedAt = default; tab.Changes.ReadAt = default; }
            if (SelectedRepository is not null) BeginCurrentRead(SelectedRepository!, force: true);
        }, () => CanNavigate && !IsBusy && _interaction is not null);
        DiagnosticsCommand = new(() => { Changed(nameof(TimingDetails)); RevealDetails(2); }, () => CanNavigate);
        CloseRepositoryCommand = new(() => CloseTab(SelectedRepository!), () => CanNavigate && SelectedRepository is not null);
        MoveLeftCommand = new(() => MoveTab(SelectedRepository!, -1), () => CanMove(SelectedRepository, -1));
        MoveRightCommand = new(() => MoveTab(SelectedRepository!, 1), () => CanMove(SelectedRepository, 1));
        CloseTabCommand = new(CloseTab, tab => CanNavigate && Repositories.Contains(tab));
        CloseOtherTabsCommand = new(tab =>
        {
            SelectedRepository = tab;
            foreach (var other in Repositories.Where(other => other != tab).ToArray()) CloseTab(other);
        }, tab => CanNavigate && Repositories.Contains(tab) && Repositories.Count > 1);
        MoveTabLeftCommand = new(tab => MoveTab(tab, -1), tab => CanMove(tab, -1));
        MoveTabRightCommand = new(tab => MoveTab(tab, 1), tab => CanMove(tab, 1));
        RefreshTabCommand = new(tab => { SelectedRepository = tab; BeginCurrentRead(tab, force: true); },
            tab => CanNavigate && _reader is not null && Repositories.Contains(tab));
        _timer.Tick += (_, _) => Changed(nameof(OperationStatus));
        InitializeActions(actions); InitializeChanges();
    }
    private bool CanRead => CanNavigate && SelectedRepository is not null && _reader is not null;

    public void CompleteStartup(SettingsLoadResult result)
    {
        LoadedSettings = result;
        Repositories.Clear(); _knownIdentities.Clear();
        foreach (var path in result.Settings.RepositoryPaths)
        {
            var tab = new RepositoryTab(path);
            // The user requested one unfiltered view; ignore legacy current-branch preferences.
            tab.History.Scope = HistoryScope.AllCommits;
            Repositories.Add(tab);
        }
        foreach (var identity in result.Settings.RepositoryIdentities)
            if (Repositories.Any(tab => tab.Directory.Equals(identity.Path, StringComparison.OrdinalIgnoreCase))) _knownIdentities[identity.Path] = identity;
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

    private void BeginInspection(RepositoryTab tab, bool reopen = false, bool force = false, int direction = 0, bool firstPage = false, bool actionReconcile = false)
    {
        if (_reader is null || (!CanNavigate && !actionReconcile)) return;
        _operationCancellation?.Cancel();
        var prior = _activeOperation;
        var generation = ++_generation;
        var cancellation = new CancellationTokenSource(TimeSpan.FromSeconds(45));
        _operationCancellation = cancellation;
        _readingDivergence = false; _elapsed.Restart(); IsBusy = true; _timer.Start();
        _activeOperation = InspectAsync();

        async Task InspectAsync()
        {
            var state = tab.History;
            var historyPublished = false;
            try
            {
                await prior; // Drain owned cancelled work before admitting the next read.
                if (Obsolete()) return;
                state.Notice = UiText.Get(state.Page is null ? "HistoryLoading" : "HistoryCheckingCached"); Changed(nameof(HistoryNotice));
                RepositoryIdentity identity;
                try
                {
                    using (_trace.Measure(TimingStage.Identity)) identity = await _identities.ReadAsync(tab.Directory, cancellation.Token);
                }
                catch (Exception exception) when (exception is System.IO.IOException or UnauthorizedAccessException or ArgumentException or System.ComponentModel.Win32Exception)
                {
                    if (Obsolete()) return;
                    state.Clear(); tab.Invalidate();
                    state.Notice = UiText.Get("IdentityUnavailable"); Status = state.Notice;
                    return;
                }
                if (Obsolete()) return;
                var identityChanged = !_knownIdentities.TryGetValue(tab.Directory, out var known) || known.Fingerprint != identity.Fingerprint;
                if (known is not null && identityChanged)
                {
                    state.Clear(); tab.Invalidate();
                    if (!reopen) { state.Notice = UiText.Get("IdentityChanged"); Status = state.Notice; return; }
                }
                // Local history is the default product behavior, not a trust permission.
                // Remember identity only after a successful read; future execution features need separate gates.
                if (!force && direction == 0 && state.Page is not null && !state.NeedsRefresh && DateTimeOffset.UtcNow - state.ValidatedAt < RefreshAge)
                { state.Notice = LoadedNotice(state.Page); Status = UiText.Get("HistoryCached"); return; }
                state.Notice = UiText.Get(state.Page is null ? "HistoryLoading" : "HistoryRefreshing"); Changed(nameof(HistoryNotice));
                state.ValidatedAt = default; // A failed/cancelled refresh cannot later masquerade as fresh cached data.
                Changed(nameof(DivergenceHelp));
                Status = UiText.Get("InspectingRepository");
                _git ??= await _reader.DiscoverAsync(_explicitGit, cancellation.Token); Changed(nameof(GitStatus));
                var previous = state.Page;
                var snapshot = direction == 0 ? await _reader.InspectAsync(_git, tab.Directory, cancellation.Token) : tab.Snapshot!;
                if (Obsolete()) return;
                if (!string.Equals(snapshot.Worktree, identity.Worktree, StringComparison.OrdinalIgnoreCase) ||
                    !string.Equals(snapshot.GitDirectory, identity.GitDirectory, StringComparison.OrdinalIgnoreCase) ||
                    !string.Equals(snapshot.CommonDirectory, identity.CommonDirectory, StringComparison.OrdinalIgnoreCase))
                { RejectChangedIdentity(); return; }
                CommitHistory history;
                var keepPage = false;
                if (direction != 0)
                    history = await _reader.ReadHistoryPageAsync(_git, previous!.Session, direction > 0 ? previous.Next! : state.Cursors[state.PageIndex - 1], cancellation.Token);
                else if (!firstPage && previous is not null && previous.Session.Scope == state.Scope)
                {
                    history = await _reader.ReadRefreshedHistoryAsync(_git, snapshot, previous, cancellation.Token);
                    keepPage = history.Session.Fingerprint == previous.Session.Fingerprint && snapshot.Commit == previous.Session.Repository.Commit;
                }
                else history = await _reader.ReadInitialHistoryAsync(_git, snapshot, state.Scope, cancellation.Token);
                RepositoryIdentity currentIdentity;
                try { currentIdentity = await _identities.ReadAsync(tab.Directory, cancellation.Token); }
                catch (Exception exception) when (exception is System.IO.IOException or UnauthorizedAccessException or ArgumentException or System.ComponentModel.Win32Exception)
                { RejectChangedIdentity(); return; }
                if (currentIdentity.Fingerprint != identity.Fingerprint) { RejectChangedIdentity(); return; }
                (CommitRow[] Rows, long Bytes) prepared;
                using (_trace.Measure(TimingStage.Formatting)) prepared = await Task.Run(() =>
                {
                    var formatted = history.Entries.Select(entry => new CommitRow(entry, history.Session.Repository)).ToArray();
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
                tab.Apply(history.Session.Repository); EvictCaches(tab); Status = UiText.Get("HistoryLoaded");
                historyPublished = true;
                if (direction == 0) state.Divergence = new(DivergenceState.Pending);
                PublishState(); // Rows are usable before optional ahead/behind traversal or a profile write.
                if (identityChanged)
                {
                    _knownIdentities[tab.Directory] = new(tab.Directory, identity.Fingerprint);
                    await PersistRepositoryIdentitiesAsync();
                }
                if (direction != 0) return; // The page uses the same immutable tips and cached comparison.
                _readingDivergence = true; Changed(nameof(OperationStatus));
                try
                {
                    var divergence = await _reader.ReadDivergenceAsync(_git, history.Session, cancellation.Token);
                    if (!Obsolete()) state.Divergence = divergence;
                }
                catch (GitReadException exception)
                {
                    if (!Obsolete()) { state.Divergence = new(DivergenceState.Failed); state.RawOutput = exception.RawOutput; }
                }
                catch (OperationCanceledException) { if (!Obsolete(ignoreCancellation: true)) state.Divergence = new(DivergenceState.Failed); }
                catch (Exception) { if (!Obsolete()) state.Divergence = new(DivergenceState.Failed); }
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
                if (historyPublished && cancellation.IsCancellationRequested) state.Divergence = new(DivergenceState.Failed);
                if (ReferenceEquals(_operationCancellation, cancellation)) _operationCancellation = null;
                cancellation.Dispose();
                if (generation == _generation)
                {
                    if (cancellation.IsCancellationRequested && !_stopping && !historyPublished)
                    { state.Notice = UiText.Get(state.Rows.Count == 0 ? "HistoryCancelled" : "HistoryRefreshCancelled"); Status = UiText.Get("GitErrorCancelled"); }
                    _readingDivergence = false; _timer.Stop(); _elapsed.Stop(); _operationCancellation = null; IsBusy = false;
                    Changed(nameof(OperationStatus)); PublishState();
                    if (historyPublished && !_stopping && !_commitOpen) BeginCountRead(tab);
                }
            }
            void RejectChangedIdentity()
            {
                if (Obsolete()) return;
                state.Clear(); tab.Invalidate();
                state.Notice = UiText.Get("IdentityChanged"); Status = state.Notice;
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
    private async Task<bool> PersistRepositoryIdentitiesAsync()
    {
        if (_persist is null) return true; // Tests can observe CaptureSession instead of touching a real profile.
        try { await _persist(CaptureSession(LoadedSettings?.Settings ?? new())); return true; }
        catch (Exception) { SettingsNotice = UiText.Get("IdentitySaveFailed"); return false; }
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
                PublishChanges(); HighlightAuthors();
                foreach (var property in new[] { nameof(Commits), nameof(SelectedCommit), nameof(PullCount), nameof(PushCount), nameof(DivergenceHelp), nameof(HistoryRange), nameof(HistoryNotice), nameof(GraphWidth), nameof(RawOutput), nameof(TimingDetails) }) Changed(property);
                RefreshCommands(); HistoryViewChanged?.Invoke(this, EventArgs.Empty);
            } finally { _publishingState = false; }
        }
    }
    public void CloseTab(RepositoryTab tab)
    {
        if (!CanNavigate || !Repositories.Contains(tab)) return;
        var index = Repositories.IndexOf(tab);
        var selected = SelectedRepository == tab;
        Repositories.Remove(tab); _knownIdentities.Remove(tab.Directory);
        if (selected) SelectedRepository = Repositories.Count == 0 ? null : Repositories[Math.Min(index, Repositories.Count - 1)];
        RefreshCommands();
    }
    private bool CanMove(RepositoryTab? tab, int offset) => CanNavigate && tab is not null && Repositories.Contains(tab) &&
        Repositories.IndexOf(tab) + offset >= 0 && Repositories.IndexOf(tab) + offset < Repositories.Count;
    private void MoveTab(RepositoryTab tab, int offset)
    { if (CanMove(tab, offset)) { var index = Repositories.IndexOf(tab); Repositories.Move(index, index + offset); RefreshCommands(); } }
    private void RefreshCommands()
    {
        ShowChangesCommand?.Refresh(); ShowHistoryCommand?.Refresh();
        NotifyActionContext();
        foreach (var property in new[] { nameof(CanOpenBranchMenu), nameof(CanSwitchBranch), nameof(BranchActionHelp), nameof(FetchActionHelp), nameof(PullActionHelp), nameof(PushActionHelp), nameof(SwitchActionHelp) }) Changed(property);
        NewBranchCommand?.Refresh(); FetchCommand?.Refresh(); PullCommand?.Refresh(); PushCommand?.Refresh(); SwitchBranchCommand?.Refresh(); Changed(nameof(LocalBranches)); Changed(nameof(BranchMenuItems));
        CloseTabCommand?.Refresh(); CloseOtherTabsCommand?.Refresh(); MoveTabLeftCommand?.Refresh(); MoveTabRightCommand?.Refresh(); RefreshTabCommand?.Refresh();
        OpenRepositoryCommand?.Refresh(); RefreshRepositoryCommand?.Refresh(); CancelCommand?.Refresh(); ChooseGitCommand?.Refresh();
        DiagnosticsCommand?.Refresh();
        CloseRepositoryCommand?.Refresh(); MoveLeftCommand?.Refresh(); MoveRightCommand?.Refresh();
        LoadMoreHistoryCommand?.Refresh(); NewerHistoryCommand?.Refresh(); FirstHistoryCommand?.Refresh();
    }
    public AppSettings CaptureSession(AppSettings layout) => layout with
    {
        RepositoryPaths = Repositories.Select(tab => tab.Directory).ToArray(), SelectedRepositoryPath = SelectedRepository?.Directory,
        RepositoryIdentities = _knownIdentities.Values.ToArray(), RepositoryPreferences = Repositories.Select(tab => new RepositoryPreference(tab.Directory, tab.History.Scope)).ToArray()
    };
    public void BeginShutdown() { _countCancel?.Cancel(); _stopping = true; _operationCancellation?.Cancel(); IsReady = false; Status = UiText.Get("SavingSettings"); }
    public async Task WaitForIdleAsync() { Task task; do { task = _activeOperation; await task; } while (task != _activeOperation); await _countTask; }
    public void ResumeAfterSaveFailure() { _stopping = false; IsReady = true; Status = UiText.Get("SettingsSaveFailedStatus"); }
}
