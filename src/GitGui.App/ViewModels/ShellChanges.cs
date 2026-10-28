using System.Windows.Threading;
using GitGui.App.Localization;
using GitGui.Core.Git;

namespace GitGui.App.ViewModels;

public sealed record ChangeRow(LocalChange Change)
{
    public string Group => UiText.Get("Changes" + Change.Group);
    public string Kind => UiText.Get("ChangeCode" + Change.Code);
    public string Path => string.Concat(Change.Path.Select(c => char.IsControl(c) ? "\\u" + ((int)c).ToString("X4") : c.ToString()));
    public string Details => UiText.Format("ChangeDetails", Group, Kind, Change.Path, Change.OriginalPath ?? UiText.Get("ChangeNoOriginal"), Change.Submodule);
}
internal sealed class ChangesViewState
{
    public IReadOnlyList<ChangeRow> Rows { get; set; } = [];
    public ChangeRow? Selected { get; set; }
    public DateTimeOffset ReadAt { get; set; }
    public bool ShowIgnored { get; set; }
    public string Notice { get; set; } = UiText.Get("ChangesNotLoaded");
    public string RawOutput { get; set; } = "";
    public void Clear() { Rows = []; Selected = null; ReadAt = default; RawOutput = ""; Notice = UiText.Get("ChangesNotLoaded"); }
}
public sealed partial class ShellViewModel
{
    private bool _readingChanges;
    private ChangesViewState? ChangesState => SelectedRepository?.Changes;
    public bool IsLocalChanges => SelectedRepository?.LocalChangesView == true;
    public IReadOnlyList<ChangeRow> Changes => ChangesState?.Rows ?? [];
    public string ChangesNotice => ChangesState?.Notice ?? UiText.Get("ChangesNoRepository");
    public string ChangesOutput => ChangesState?.RawOutput ?? "";
    public string ChangesCounts => UiText.Format("ChangesCounts", Changes.Count(row => row.Change.Group == ChangeGroup.Staged),
        Changes.Count(row => row.Change.Group == ChangeGroup.Unstaged), Changes.Count(row => row.Change.Group == ChangeGroup.Untracked),
        Changes.Count(row => row.Change.Group == ChangeGroup.Conflicted), Changes.Count(row => row.Change.Group == ChangeGroup.Ignored));
    public ChangeRow? SelectedChange
    {
        get => ChangesState?.Selected;
        set { if (!_publishingState && ChangesState is { } state && (value is null || state.Rows.Contains(value))) { state.Selected = value; Changed(); } }
    }
    public bool ShowIgnored
    {
        get => ChangesState?.ShowIgnored == true;
        set
        {
            if (!CanNavigate || ChangesState is not { } state || state.ShowIgnored == value) return;
            state.ShowIgnored = value; state.ReadAt = default; Changed();
            BeginChanges(SelectedRepository!, true);
        }
    }
    public RelayCommand ShowChangesCommand { get; private set; } = null!;
    public RelayCommand ShowHistoryCommand { get; private set; } = null!;
    private void InitializeChanges()
    {
        ShowChangesCommand = new(() => SelectWorkspace(true), () => CanRead);
        ShowHistoryCommand = new(() => SelectWorkspace(false), () => CanRead);
    }
    private void SelectWorkspace(bool changes)
    {
        if (SelectedRepository is not { } tab || tab.LocalChangesView == changes) return;
        HistoryViewChanging?.Invoke(this, EventArgs.Empty);
        tab.LocalChangesView = changes;
        PublishChanges(); PublishState();
        BeginCurrentRead(tab);
    }
    private void BeginCurrentRead(RepositoryTab tab, bool force = false, bool reopen = false)
    {
        if (tab.LocalChangesView) BeginChanges(tab, force, reopen);
        else BeginInspection(tab, force: force, reopen: reopen);
    }
    private void RevealDetails(int index)
    {
        if (SelectedRepository is { } tab) tab.LocalChangesView = false;
        DetailsIndex = index; PublishState();
    }
    private void PublishChanges()
    {
        if (IsLocalChanges) Status = ChangesNotice;
        var publishing = _publishingState; _publishingState = true;
        try
        {
            foreach (var property in new[] { nameof(IsHistory), nameof(IsLocalChanges), nameof(AreaTitle), nameof(Changes), nameof(SelectedChange),
                nameof(ChangesNotice), nameof(ChangesOutput), nameof(ChangesCounts), nameof(ShowIgnored) }) Changed(property);
        }
        finally { _publishingState = publishing; }
    }
    private void BeginChanges(RepositoryTab tab, bool force = false, bool reopen = false, bool actionReconcile = false)
    {
        if (_reader is null || (!CanNavigate && !actionReconcile)) return;
        _operationCancellation?.Cancel(); var prior = _activeOperation; var generation = ++_generation;
        var cancellation = new CancellationTokenSource(TimeSpan.FromSeconds(45)); _operationCancellation = cancellation;
        _readingChanges = true; _readingDivergence = false; _elapsed.Restart(); IsBusy = true; _timer.Start();
        _activeOperation = ReadAsync();
        async Task ReadAsync()
        {
            var state = tab.Changes;
            try
            {
                // Switch the view and paint available rows before any filesystem/Git work.
                await Dispatcher.Yield(DispatcherPriority.Background);
                await prior;
                if (Obsolete()) return;
                state.Notice = UiText.Get(state.ReadAt == default ? "ChangesLoading" : "ChangesChecking"); PublishChanges();
                var identity = await _identities.ReadAsync(tab.Directory, cancellation.Token);
                if (Obsolete()) return;
                if (_knownIdentities.TryGetValue(tab.Directory, out var known) && identity.Fingerprint != known.Fingerprint)
                {
                    tab.History.Clear(); tab.Invalidate();
                    if (!reopen) { state.Notice = UiText.Get("IdentityChanged"); return; }
                }
                if (!force && state.ReadAt != default && DateTimeOffset.UtcNow - state.ReadAt < RefreshAge)
                { state.Notice = UiText.Format("ChangesLoaded", state.ReadAt.LocalDateTime); return; }
                state.ReadAt = default; // Keep rows but mark failed/cancelled refreshes stale.
                _git ??= await _reader.DiscoverAsync(_explicitGit, cancellation.Token); Changed(nameof(GitStatus));
                var snapshot = await _reader.InspectAsync(_git, tab.Directory, cancellation.Token);
                var data = await _reader.ReadLocalChangesAsync(_git, tab.Directory, state.ShowIgnored, cancellation.Token);
                var after = await _identities.ReadAsync(tab.Directory, cancellation.Token);
                if (Obsolete()) return;
                if (identity.Fingerprint != after.Fingerprint || !string.Equals(snapshot.Worktree, identity.Worktree, StringComparison.OrdinalIgnoreCase) ||
                    !string.Equals(snapshot.GitDirectory, identity.GitDirectory, StringComparison.OrdinalIgnoreCase) ||
                    !string.Equals(snapshot.CommonDirectory, identity.CommonDirectory, StringComparison.OrdinalIgnoreCase))
                { tab.History.Clear(); tab.Invalidate(); state.Notice = UiText.Get("IdentityChanged"); return; }
                var rows = await Task.Run(() => data.Entries.Select(entry => new ChangeRow(entry)).ToArray(), cancellation.Token);
                if (Obsolete()) return;
                var selected = state.Selected?.Change;
                state.Rows = rows; state.Selected = rows.FirstOrDefault(row => row.Change.Group == selected?.Group && row.Change.Path == selected?.Path) ?? rows.FirstOrDefault();
                state.ReadAt = data.ReadAt; state.RawOutput = "";
                state.Notice = UiText.Format(rows.Length == 0 ? "ChangesClean" : "ChangesLoaded", data.ReadAt.LocalDateTime);
                tab.Apply(snapshot); _knownIdentities[tab.Directory] = new(tab.Directory, identity.Fingerprint);
                foreach (var other in Repositories.Where(item => item != tab && item.Changes.Rows.Count != 0)
                    .OrderByDescending(item => item.History.LastUse).Skip(CachedTabLimit - 1)) other.Changes.Clear();
            }
            catch (GitReadException exception)
            {
                if (!Obsolete()) { state.ReadAt = default; state.Notice = UiText.Format("ChangesFailed", UiText.Get("GitError" + exception.Failure)); state.RawOutput = GitActions.Redact(exception.RawOutput); }
            }
            catch (OperationCanceledException) { if (!Obsolete(true)) { state.ReadAt = default; state.Notice = UiText.Get("ChangesCancelled"); } }
            catch (Exception exception) when (exception is System.IO.IOException or UnauthorizedAccessException or ArgumentException or System.ComponentModel.Win32Exception)
            { if (!Obsolete()) { tab.History.Clear(); tab.Invalidate(); state.Notice = UiText.Get("IdentityUnavailable"); } }
            catch (Exception) { if (!Obsolete()) { state.ReadAt = default; state.Notice = UiText.Format("ChangesFailed", UiText.Get("GitErrorUnexpected")); } }
            finally
            {
                if (generation == _generation)
                {
                    if (cancellation.IsCancellationRequested) { state.ReadAt = default; state.Notice = UiText.Get("ChangesCancelled"); }
                    _operationCancellation = null; _readingChanges = false; _timer.Stop(); _elapsed.Stop(); IsBusy = false;
                    PublishChanges(); PublishState(); Changed(nameof(OperationStatus));
                }
                cancellation.Dispose();
            }
            bool Obsolete(bool ignoreCancellation = false) => _stopping || generation != _generation || !Repositories.Contains(tab) || (!ignoreCancellation && cancellation.IsCancellationRequested);
        }
    }
}
