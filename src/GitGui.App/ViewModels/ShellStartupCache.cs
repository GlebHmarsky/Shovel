using GitGui.App.Localization;
using GitGui.Core.Git;

namespace GitGui.App.ViewModels;
public sealed partial class ShellViewModel
{
    public HistoryCacheStore? StartupCache { get; set; }
    private Task _startupPreviewTask = Task.CompletedTask;
    private async Task RestoreStartupPreviewAsync(RepositoryTab tab)
    {
        try
        {
            if (StartupCache is null || !_knownIdentities.TryGetValue(tab.Directory, out var known)) return;
            using var timeout = new CancellationTokenSource(TimeSpan.FromSeconds(2));
            var cache = await StartupCache.ReadAsync(tab.Directory, known.Fingerprint, timeout.Token);
            if (cache is null) return;
            var identity = await _identities.ReadAsync(tab.Directory, timeout.Token);
            if (identity.Fingerprint != known.Fingerprint) return;
            var rows = await Task.Run(() => cache.Entries.Select(entry => new CommitRow(entry, cache.Repository)).ToArray(), timeout.Token);
            if (_stopping || SelectedRepository != tab || tab.History.Page is not null) return;
            var state = tab.History;
            state.StartupPreview = true; state.NeedsRefresh = true;
            state.Page = new(new(cache.Repository, HistoryScope.AllCommits, "", [], false), new(cache.Offset, GraphCursor.Start), null, cache.Entries);
            state.Rows = rows; state.Selected = rows.FirstOrDefault(row => row.Commit.Hash == cache.SelectedHash) ?? rows.FirstOrDefault();
            state.VerticalOffset = cache.Vertical; state.HorizontalOffset = cache.Horizontal;
            state.Notice = UiText.Format("HistoryStartupCached", cache.SavedAt.LocalDateTime);
            state.EstimatedBytes = rows.Sum(row => 2L * row.Commit.Message.Length + 4096);
            tab.Apply(cache.Repository); PublishState();
        }
        catch (Exception) { /* Disposable cache failures never replace the actual read/error. */ }
    }
    public Task SaveStartupCacheAsync(CancellationToken token)
    {
        if (StartupCache is null) return Task.CompletedTask;
        CachedHistory? cache = null;
        if (SelectedRepository is { } tab && tab.History is { StartupPreview: false, ValidatedAt: var at, Page: { } page } state && at != default &&
            _knownIdentities.TryGetValue(tab.Directory, out var identity))
            cache = new(1, tab.Directory, identity.Fingerprint, page.Session.Repository, page.Entries.ToArray(), page.Cursor.Offset,
                state.Selected?.Commit.Hash, state.VerticalOffset, state.HorizontalOffset, at);
        return StartupCache.SaveAsync(cache, token);
    }
    public bool ShowingStartupPreview => State?.StartupPreview == true;
}
