using System.IO;
using GitGui.App.Localization;
using GitGui.Core.Git;

namespace GitGui.App.ViewModels;

public sealed class RepositoryTab(string directory) : ObservableObject
{
    public string Directory { get; } = directory;
    public string Label => Path.GetFileName(Path.TrimEndingDirectorySeparator(Directory)) is { Length: > 0 } label ? label : Directory;
    public RepositorySnapshot? Snapshot { get; private set; }
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
        Changed(nameof(Snapshot)); Changed(nameof(Head)); Changed(nameof(State)); Changed(nameof(ReadAt));
    }
    public void Invalidate()
    {
        Snapshot = null;
        Changed(nameof(Snapshot)); Changed(nameof(Head)); Changed(nameof(State)); Changed(nameof(ReadAt));
    }
}
