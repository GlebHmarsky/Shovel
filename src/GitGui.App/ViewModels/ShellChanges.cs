using System.Windows.Threading;
using GitGui.App.Localization;
using GitGui.Core.Git;
using GitGui.Core.Drafts;

namespace GitGui.App.ViewModels;

public sealed record ChangeRow(LocalChange Change)
{
    public string Code => Change.Code.ToString();
    public string Marker => Change.Code switch { 'A' or '?' => "+", 'D' => "−", 'R' => "→", _ => Code };
    public string Group => UiText.Get("Changes" + Change.Group);
    public string Kind => UiText.Get("ChangeCode" + Change.Code);
    public string Path => string.Concat(Change.Path.Select(c => char.IsControl(c) ? "\\u" + ((int)c).ToString("X4") : c.ToString()));
    public string Details => UiText.Format("ChangeDetails", Group, Kind, Change.Path, Change.OriginalPath ?? UiText.Get("ChangeNoOriginal"), Change.Submodule);
    public string CommittedDetails => UiText.Format("CommittedFileDetails", Kind, Change.Path, Change.OriginalPath ?? UiText.Get("ChangeNoOriginal"));
}
public sealed partial class ShellViewModel
{
    private bool _commitOpen;
    public ICommitDraftStore? DraftStore { private get; set; }
    private CommitViewModel? _commitModel;
    private CancellationTokenSource? _countCancel;
    private Task _countTask = Task.CompletedTask;
    private long _countGeneration;
    public Task CommitDialogTask { get; private set; } = Task.CompletedTask;
    public string CommitButtonLabel => SelectedRepository?.Changes is { ReadAt: var at } state && at != default
        ? UiText.Format("CommitOpen", state.Rows.Select(row => row.Change.Path).Distinct(StringComparer.Ordinal).Count()) : UiText.Get("CommitOpenUnknown");
    public RelayCommand ShowChangesCommand { get; private set; } = null!;
    public RelayCommand ShowHistoryCommand { get; private set; } = null!;
    private void InitializeChanges()
    {
        ShowChangesCommand = new(() => CommitDialogTask = OpenCommitAsync(), () => CanRead && _actions is not null && _interaction is not null);
        ShowHistoryCommand = new(() => { RevealDetails(DetailsIndex); if (!IsBusy && State?.Page?.Cursor.Offset > 0) BeginInspection(SelectedRepository!, force: true, firstPage: true); }, () => CanRead);
    }
    private void BeginCurrentRead(RepositoryTab tab, bool force = false, bool reopen = false)
    { if (force) tab.Changes.ReadAt = default; BeginInspection(tab, force: force, reopen: reopen); }
    private void CancelCountRead() { _countCancel?.Cancel(); _countCancel = null; _countGeneration++; }
    private void RevealDetails(int index) { DetailsIndex = index; PublishState(); }
    private void PublishChanges() => Changed(nameof(CommitButtonLabel));

    private async Task OpenCommitAsync()
    {
        var tab = SelectedRepository!;
        CancelCountRead(); _sidebarCancel?.Cancel(); _commitOpen = true; RefreshCommands(); Changed(nameof(ActionOpen)); Changed(nameof(CanNavigate));
        var model = new CommitViewModel(tab.Directory, _git, _reader!, _actions!, _interaction!, tab.Changes,
            _knownIdentities.GetValueOrDefault(tab.Directory)?.Fingerprint, DraftStore ?? new CommitDraftStore(null, _identities));
        _commitModel = model;
        void OnChanged(object? sender, System.ComponentModel.PropertyChangedEventArgs args) { if (args.PropertyName == nameof(CommitViewModel.Summary)) Changed(nameof(OperationStatus)); }
        void OnSnapshot(object? sender, EventArgs args) { TrimChangesCache(tab); PublishChanges(); if (tab.Changes.Context is { } context) tab.Apply(context.Repository); }
        model.PropertyChanged += OnChanged; model.SnapshotChanged += OnSnapshot;
        try
        {
            // No awaits/queries before showing the editor. Its Loaded handler schedules the read after layout.
            await _interaction!.ShowCommitAsync(model);
        }
        finally
        {
            await model.CloseAsync();
            if (model.HasMutation)
                foreach (var related in Repositories.Where(item => item == tab || item.Snapshot?.CommonDirectory == tab.Snapshot?.CommonDirectory))
                { related.History.ValidatedAt = default; related.History.NeedsRefresh = true; related.Changes.ReadAt = default; }
            model.PropertyChanged -= OnChanged; model.SnapshotChanged -= OnSnapshot;
            _commitModel = null; _commitOpen = false; Changed(nameof(ActionOpen)); Changed(nameof(CanNavigate)); Changed(nameof(OperationStatus)); PublishChanges(); RefreshCommands();
            if (model.HasMutation && !_stopping) BeginInspection(tab, force: true);
        }
    }
    private void BeginCountRead(RepositoryTab tab)
    {
        // One local count for the selected tab after history, never a preload/poll or history prerequisite.
        if (_reader is null || _actions is null || _git is null || !CanNavigate || SelectedRepository != tab ||
            DateTimeOffset.UtcNow - tab.Changes.ReadAt < RefreshAge) return;
        _countCancel?.Cancel(); var previous = _countTask; var generation = ++_countGeneration;
        var cancellation = new CancellationTokenSource(TimeSpan.FromSeconds(45)); _countCancel = cancellation;
        var git = _git; _countTask = ReadAsync();
        async Task ReadAsync()
        {
            try
            {
                await Dispatcher.Yield(DispatcherPriority.Background); await previous;
                cancellation.Token.ThrowIfCancellationRequested();
                var identity = await _identities.ReadAsync(tab.Directory, cancellation.Token);
                if (!_knownIdentities.TryGetValue(tab.Directory, out var known) || known.Fingerprint != identity.Fingerprint) return;
                var data = await _reader.ReadLocalChangesAsync(git, tab.Directory, false, cancellation.Token);
                var after = await _identities.ReadAsync(tab.Directory, cancellation.Token);
                var rows = await Task.Run(() => data.Entries.Select(entry => new ChangeRow(entry)).ToArray(), cancellation.Token);
                if (_stopping || _commitOpen || cancellation.IsCancellationRequested || generation != _countGeneration || SelectedRepository != tab || after.Fingerprint != identity.Fingerprint) return;
                // This lightweight count does not authorize actions; opening the editor obtains a full context.
                tab.Changes.Rows = rows; tab.Changes.ReadAt = data.ReadAt; tab.Changes.Context = null;
                TrimChangesCache(tab);
            }
            catch (Exception) { if (generation == _countGeneration && SelectedRepository == tab) tab.Changes.ReadAt = default; }
            finally { if (generation == _countGeneration) { _countCancel = null; PublishChanges(); } cancellation.Dispose(); }
        }
    }
    private void TrimChangesCache(RepositoryTab active)
    {
        foreach (var other in Repositories.Where(item => item != active && (item.Changes.Context is not null || item.Changes.Rows.Count != 0))
            .OrderByDescending(item => item.History.LastUse).Skip(CachedTabLimit - 1)) other.Changes.Clear();
    }
}
