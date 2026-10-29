using GitGui.App.Localization;

namespace GitGui.App.ViewModels;

public sealed partial class ShellViewModel
{
    private OperationActivity? _activity;
    private bool _showWorkspaceStatus;
    public OperationActivity? Activity => _activity;
    public string OperationCaption => SettingsNotice.Length != 0 ? SettingsNotice : _showWorkspaceStatus ? Status : Activity?.Caption ?? (HasRecovery ? UiText.Get("RecoveryWaiting") :
        IsBusy ? UiText.Get(_readingDivergence ? "OperationReadingCounts" : "OperationReadingHistory") :
        ActionOpen ? UiText.Get("ActionReviewing") : UiText.Get("NoActiveOperation"));
    private object? _readAttempt;
    private void StartReadActivity(bool suppressed = false)
    {
        _readAttempt = suppressed ? null : new object();
        UpdateReadActivity();
    }
    private void UpdateReadActivity()
    {
        if (_readAttempt is null || IsActionRunning || _commitModel is not null) return;
        PublishActivity(new(_readAttempt, UiText.Get("OperationHistoryTitle"),
            UiText.Get(_readingDivergence ? "OperationReadingCounts" : "OperationReadingHistory"),
            _elapsed.Elapsed.TotalSeconds, OperationOutcome.Running));
    }
    private void FinishReadActivity(OperationOutcome outcome)
    {
        if (_readAttempt is not null && ReferenceEquals(Activity?.Attempt, _readAttempt))
            PublishActivity(new(_readAttempt, UiText.Get("OperationHistoryTitle"),
                OperationActivity.CompletedPhase(outcome), _elapsed.Elapsed.TotalSeconds, outcome));
        _readAttempt = null;
    }
    private void PublishActivity(OperationActivity? activity)
    {
        if (activity is null || !ReferenceEquals(activity.Attempt, _activity?.Attempt)) _showWorkspaceStatus = false;
        if (Set(ref _activity, activity, nameof(Activity))) Changed(nameof(OperationCaption));
    }
}
