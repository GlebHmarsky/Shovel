using GitGui.App.Localization;

namespace GitGui.App.ViewModels;

public enum OperationOutcome { Running, Success, Conflict, Failed, Cancelled, RefreshFailed }

/// <summary>Immutable progress/result snapshot; scheduling belongs to the repository owner.</summary>
public sealed record OperationActivity(object Attempt, string Title, string Phase, double Seconds, OperationOutcome Outcome)
{
    public bool IsComplete => Outcome != OperationOutcome.Running;
    public string Caption => UiText.Format("OperationCaption", Title, Phase);
    public string Help => Caption;
    public static string CompletedPhase(OperationOutcome outcome) => UiText.Get(outcome switch
    {
        OperationOutcome.Success => "ActionCompletedShort",
        OperationOutcome.Cancelled => "ActionStoppedShort",
        OperationOutcome.Conflict => "ActionConflictShort",
        OperationOutcome.RefreshFailed => "ActionRefreshFailedShort",
        _ => "ActionFailedShort"
    });
}
