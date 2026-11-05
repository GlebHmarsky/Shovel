using GitGui.App.Localization;
using GitGui.Core.Git;

namespace GitGui.App.ViewModels;

public sealed class CheckoutViewModel(string repository, string target, bool detached,
    Func<CancellationToken, Task<WorkingCopyContext>> read) : ObservableObject, IDisposable
{
    private readonly CancellationTokenSource _readStop = new();
    private CheckoutChanges _choice;
    private string _status = UiText.Get("CheckoutReading");
    private WorkingCopyContext? _context;
    public string Repository => repository;
    public string Target => target;
    public string Title => UiText.Get(detached ? "HistoryMenuCheckoutCommit" : "ActionTitleSwitchBranch");
    public string Description => UiText.Get(detached ? "CheckoutDetachedHelp" : "CheckoutBranchHelp");
    public string Status { get => _status; private set => Set(ref _status, value); }
    public bool CanConfirm => _context is not null;
    public bool HasChanges => _context?.Changes.Entries.Count > 0;
    public bool Keep { get => _choice == CheckoutChanges.Keep; set { if (value) Choose(CheckoutChanges.Keep); } }
    public bool Stash { get => _choice == CheckoutChanges.StashAndReapply; set { if (value) Choose(CheckoutChanges.StashAndReapply); } }
    public bool Discard { get => _choice == CheckoutChanges.DiscardTracked; set { if (value) Choose(CheckoutChanges.DiscardTracked); } }
    public Task Loading { get; private set; } = Task.CompletedTask;
    public Task StartAsync() => Loading = LoadAsync();
    private void Choose(CheckoutChanges choice) { _choice = choice; Changed(nameof(Keep)); Changed(nameof(Stash)); Changed(nameof(Discard)); }
    public ActionOptions Apply(ActionOptions options) => options with
    { CheckoutReview = _context ?? throw new InvalidOperationException(), CheckoutChanges = HasChanges ? _choice : CheckoutChanges.Keep };
    private async Task LoadAsync()
    {
        try
        {
            _context = await read(_readStop.Token);
            Status = HasChanges ? UiText.Get("CheckoutDirtyHelp") : UiText.Get("CheckoutClean");
            Changed(nameof(HasChanges)); Changed(nameof(CanConfirm));
        }
        catch (OperationCanceledException) { }
        catch (Exception) { Status = UiText.Get("CheckoutReadFailed"); }
    }
    // Closing a review abandons only its read; no Git mutation has been submitted yet.
    public void Dispose() { _readStop.Cancel(); _readStop.Dispose(); }
}
