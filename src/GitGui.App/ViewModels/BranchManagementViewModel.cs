using GitGui.App.Localization;
using GitGui.Core.Git;

namespace GitGui.App.ViewModels;

/// <summary>Options from an immutable cached snapshot. Construction/editing never starts I/O.</summary>
public sealed class BranchManagementViewModel : ObservableObject
{
    public BranchManagementAction Action { get; }
    public string Repository { get; }
    public IReadOnlyList<CommitRef> Branches { get; }
    public IReadOnlyList<CommitRef> RemoteBranches { get; }
    private readonly string? _current;
    private CommitRef? _branch, _upstream;
    public CommitRef? Branch { get => _branch; set { if (Set(ref _branch, value)) Refresh(); } }
    public CommitRef? Upstream { get => _upstream; set { if (Set(ref _upstream, value)) Refresh(); } }
    public bool IsDelete => Action == BranchManagementAction.Delete;
    public bool IsSetUpstream => Action == BranchManagementAction.SetUpstream;
    public bool SubmitIsDefault => !IsDelete;
    private string Suffix => IsDelete ? "DeleteBranch" : IsSetUpstream ? "SetUpstream" : "UnsetUpstream";
    public string Title => UiText.Get("BranchManageTitle" + Suffix);
    public string Explanation => UiText.Get("BranchManageExplain" + Suffix);
    public string SubmitText => UiText.Get("BranchManageSubmit" + Suffix);
    public string Target => Branch is null ? UiText.Get("BranchManageChooseBranch") :
        UiText.Format("BranchManageTarget", Branch.Name, Branch.Hash, Branch.Upstream ?? UiText.Get("BranchManageNoUpstream"));
    private string? Unavailable => Branch is null || !Branches.Contains(Branch) ? "BranchManageChooseBranch" :
        IsDelete && Branch.Name == "refs/heads/" + _current ? "BranchManageCurrent" :
        IsSetUpstream && (Upstream is null || !RemoteBranches.Contains(Upstream)) ? "BranchManageNoRemoteRef" : null;
    public string Readiness => UiText.Get(Unavailable ?? "BranchManageReady");
    public RelayCommand SubmitCommand { get; }
    public BranchManagementOptions? SubmittedOptions { get; private set; }
    public event EventHandler? Accepted;

    public BranchManagementViewModel(BranchManagementAction action, RepositorySnapshot snapshot, HistorySession history)
    {
        Action = action; Repository = snapshot.Worktree; _current = snapshot.Branch;
        Branches = history.References.Where(item => item.Name.StartsWith("refs/heads/", StringComparison.Ordinal)).ToArray();
        // Standard symbolic remote HEAD is not a selectable branch; Core rejects any other
        // symbolic alias after submission because cached CommitRef does not encode symrefs.
        RemoteBranches = history.References.Where(item => item.Name.StartsWith("refs/remotes/", StringComparison.Ordinal) &&
            !item.Name.EndsWith("/HEAD", StringComparison.Ordinal)).ToArray();
        _branch = (IsDelete ? Branches.FirstOrDefault(item => item.Name != "refs/heads/" + _current) :
            Branches.FirstOrDefault(item => item.Name == "refs/heads/" + _current)) ?? Branches.FirstOrDefault();
        _upstream = RemoteBranches.FirstOrDefault(item => item.Name == Branch?.Upstream) ?? RemoteBranches.FirstOrDefault();
        SubmitCommand = new(() =>
        {
            SubmittedOptions = new(Action, Branch!, IsSetUpstream ? Upstream : null);
            Accepted?.Invoke(this, EventArgs.Empty);
        }, () => Unavailable is null && SubmittedOptions is null);
    }
    private void Refresh() { SubmitCommand?.Refresh(); Changed(nameof(Target)); Changed(nameof(Readiness)); }
}
