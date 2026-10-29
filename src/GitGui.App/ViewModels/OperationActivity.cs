using GitGui.App.Localization;

namespace GitGui.App.ViewModels;

public enum OperationOutcome { Running, Success, Conflict, Failed, Cancelled, RefreshFailed }

/// <summary>UI snapshot of an existing command timer, never a new work scheduler.</summary>
public sealed record OperationActivity(object Attempt, string Title, string Phase, double Seconds, OperationOutcome Outcome)
{
    public bool IsComplete => Outcome != OperationOutcome.Running;
    public string Caption => UiText.Format("OperationCaption", Title, Phase);
    public string Duration => UiText.Format("OperationSeconds", Seconds);
    public string Help => UiText.Format("OperationTimingHelp", Title, Phase, Seconds);
    public string Mark => Outcome switch
    {
        OperationOutcome.Success => "✓",
        OperationOutcome.Failed => "!",
        OperationOutcome.Conflict or OperationOutcome.RefreshFailed => "!",
        OperationOutcome.Cancelled => "■",
        _ => "◷"
    };
    public static string CompletedPhase(OperationOutcome outcome) => UiText.Get(outcome switch
    {
        OperationOutcome.Success => "ActionCompletedShort",
        OperationOutcome.Cancelled => "ActionStoppedShort",
        OperationOutcome.Conflict => "ActionConflictShort",
        OperationOutcome.RefreshFailed => "ActionRefreshFailedShort",
        _ => "ActionFailedShort"
    });
}
