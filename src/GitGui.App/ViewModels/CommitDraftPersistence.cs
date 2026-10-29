using GitGui.App.Localization;
using GitGui.Core.Drafts;
using GitGui.Core.Git;

namespace GitGui.App.ViewModels;

public sealed partial class CommitViewModel
{
    private CommitDraftCoordinator? _draft;
    private bool _draftCloseReported;
    private Task _mergeDraftTask = Task.CompletedTask;
    public Task DraftLoadingTask => _draft?.LoadingTask ?? Task.CompletedTask;
    public Task DraftWritingTask => _draft?.WritingTask ?? Task.CompletedTask;
    public string DraftProblem => _draft?.Problem is { } problem
        ? UiText.Format(problem.Failure is null ? "CommitDraftCopies" : problem.RecoverySaved ? "CommitDraftRecovery" : "CommitDraft" + problem.Failure, problem.Location)
        : "";
    private string DraftHelp => UiText.Get(_draft is null ? "CommitDraftHelp" : "CommitDraftPersistenceHelp");
    private string WithDraftProblem(string status) => DraftProblem.Length == 0 ? status : UiText.Format("CommitDraftStatus", status, DraftProblem);

    private void InitializeDrafts(ICommitDraftStore? store)
    {
        if (store is null || _identity is null) return;
        _draft = new(_state, store, Directory, _identity);
        _draft.TextRestored += OnDraftRestored;
        _draft.ProblemChanged += OnDraftProblem;
    }
    private void OnDraftRestored(object? sender, EventArgs args)
    { Changed(nameof(Subject)); Changed(nameof(Description)); Changed(nameof(CommitTarget)); RefreshCommands(); }
    private void OnDraftProblem(object? sender, EventArgs args)
    { Changed(nameof(DraftProblem)); Changed(nameof(Status)); Changed(nameof(Summary)); }
    private void DraftEdited()
    {
        if (_draft is not null) _draft.TextChanged();
        else { _state.DraftGeneration++; _state.DraftDirty = true; }
    }
    private CommitDraftSubmission CaptureDraft() => _draft?.CaptureSubmission() ?? new(_state.DraftGeneration, new(Subject, Description));
    private async Task CompleteSubmittedDraftAsync(CommitDraftSubmission submitted)
    {
        if (_draft is not null)
        {
            _draft.CommitSucceeded(submitted);
            _phase = UiText.Get("CommitDraftSaving"); Changed(nameof(Summary));
            await _draft.FlushAsync();
        }
        else if (submitted.Generation == _state.DraftGeneration && submitted.Text == new CommitDraftText(Subject, Description))
            SetMessage("");
    }
    private void ScheduleMergeMessage(WorkingCopyContext context)
    {
        if (Subject.Length != 0 || Description.Length != 0 || context.MergeMessage.Length == 0) return;
        // A saved user draft wins over Git's suggested merge message, independently of scan order.
        if (DraftLoadingTask.IsCompleted) { SetMessage(context.MergeMessage); return; }
        _mergeDraftTask = AfterDraftAsync();
        async Task AfterDraftAsync()
        {
            var generation = _state.DraftGeneration;
            await DraftLoadingTask;
            if (!_closed && _state.Context == context && generation == _state.DraftGeneration && Subject.Length == 0 && Description.Length == 0)
                SetMessage(context.MergeMessage);
        }
    }
    private async Task CloseDraftAsync()
    {
        if (_draft is null) return;
        await _draft.CloseAsync(); await _mergeDraftTask;
        _draft.TextRestored -= OnDraftRestored; _draft.ProblemChanged -= OnDraftProblem;
        if (!_draftCloseReported && _draft.Problem is { Failure: not null, RecoverySaved: false })
        {
            _draftCloseReported = true;
            _interaction.ShowActionError(UiText.Format("CommitDraftCloseFailed", DraftProblem));
        }
    }
}
