using GitGui.App.Localization;

namespace GitGui.App.ViewModels;

public sealed partial class ShellViewModel
{
    private OperationActivity? _activity;
    public OperationActivity? Activity => _activity;
    public string OperationCaption => Activity?.Caption ?? (HasRecovery ? UiText.Get("RecoveryWaiting") :
        IsBusy ? UiText.Get(_readingDivergence ? "OperationReadingCounts" : "OperationReadingHistory") :
        ActionOpen ? UiText.Get("ActionReviewing") : UiText.Get("NoActiveOperation"));
    private void PublishActivity(OperationActivity? activity)
    {
        if (Set(ref _activity, activity, nameof(Activity))) Changed(nameof(OperationCaption));
    }
}
