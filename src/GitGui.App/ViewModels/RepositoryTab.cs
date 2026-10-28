using System.IO;
using GitGui.App.Localization;
using GitGui.Core.Git;

namespace GitGui.App.ViewModels;

public sealed class RepositoryTab(string directory) : ObservableObject
{
    internal CommitWorkspaceState Changes { get; } = new();
    internal StashCache Stashes { get; } = new();
    internal HistoryViewState History { get; } = new();
    public string Directory { get; } = directory;
    public string Label => Path.GetFileName(Path.TrimEndingDirectorySeparator(Directory)) is { Length: > 0 } label ? label : Directory;
    public RepositorySnapshot? Snapshot { get; private set; }
    public bool HasOperation => Snapshot is not null && Snapshot.Operation != RepositoryOperation.None;
    public string Head => Snapshot is null ? UiText.Get("NotInspected") : Snapshot.HeadKind switch
    {
        HeadKind.Unborn => UiText.Format("UnbornHead", Snapshot.Branch!),
        HeadKind.Detached => UiText.Format("DetachedHead", Snapshot.Commit!),
        _ => UiText.Format("BranchHead", Snapshot.Branch!)
    };
    public string State => Snapshot is null ? UiText.Get("NotInspected") : Snapshot.Operation == RepositoryOperation.None
        ? UiText.Get("NoSequence") : UiText.Format("SequenceDetected", string.Join(", ", Enum.GetValues<RepositoryOperation>()
            .Where(value => value != RepositoryOperation.None && Snapshot.Operation.HasFlag(value)).Select(value => UiText.Get("Operation" + value))));
    public string ReadAt => Snapshot is null ? UiText.Get("TrustRequired") : UiText.Format("SnapshotTime", Snapshot.ReadAt.LocalDateTime);
    public void Apply(RepositorySnapshot snapshot)
    {
        Snapshot = snapshot;
        Changed(nameof(Snapshot)); Changed(nameof(Head)); Changed(nameof(State)); Changed(nameof(ReadAt)); Changed(nameof(HasOperation));
    }
    public void Invalidate()
    {
        Snapshot = null; Changes.Clear(); Stashes.List = null;
        Changed(nameof(Snapshot)); Changed(nameof(Head)); Changed(nameof(State)); Changed(nameof(ReadAt)); Changed(nameof(HasOperation));
    }
}

/// <summary>One bounded page per cached tab. UI-only state, never persisted repository content.</summary>
internal sealed record PageView(string? SelectedHash, double Vertical, double Horizontal);
internal sealed class HistoryViewState
{
    public BranchDivergence Divergence { get; set; } = new(DivergenceState.Pending);
    public Dictionary<long, PageView> PageViews { get; } = [];
    public HistoryScope Scope { get; set; }
    public CommitHistory? Page { get; set; }
    public IReadOnlyList<CommitRow> Rows { get; set; } = [];
    public CommitRow? Selected { get; set; }
    public List<HistoryCursor> Cursors { get; } = [];
    public int PageIndex { get; set; }
    public double VerticalOffset { get; set; }
    public double HorizontalOffset { get; set; }
    public DateTimeOffset ValidatedAt { get; set; }
    public long LastUse { get; set; }
    public long EstimatedBytes { get; set; }
    public bool NeedsRefresh { get; set; }
    public string Notice { get; set; } = UiText.Get("HistoryNotLoaded");
    public string Error { get; set; } = "";
    public string RawOutput { get; set; } = "";
    public void Clear()
    {
        Page = null; Rows = []; Selected = null; PageViews.Clear(); EstimatedBytes = 0; Cursors.Clear(); PageIndex = 0;
        VerticalOffset = HorizontalOffset = 0; NeedsRefresh = false; ValidatedAt = default;
        Notice = UiText.Get("HistoryNotLoaded"); Error = RawOutput = "";
        Divergence = new(DivergenceState.Pending);
    }
}
