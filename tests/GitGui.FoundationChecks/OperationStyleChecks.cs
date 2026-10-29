using System.IO;
using System.Text;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Media;
using System.Windows.Media.Animation;
using System.Windows.Threading;
using GitGui.App.Services;
using GitGui.App.ViewModels;
using GitGui.App.Views;
using GitGui.Core.Git;
using GitGui.Core.Processes;
using GitGui.Core.Settings;
using GitGui.ProcessHost;

namespace GitGui.FoundationChecks;
internal static partial class Program
{
    private static bool _operationStyleChecks;
    private static async Task RunOperationStyleChecksAsync()
    {
        var runner = new WindowsProcessRunner(); var reader = new RepositoryReader(runner);
        var identities = new WindowsRepositoryIdentity(); var actions = new GitActions(runner, reader, identities);
        await Check("Operation outcomes stay running through reconciliation and distinguish failure, conflict and cancellation", async () =>
        {
            foreach (var outcome in new[] { OperationOutcome.Success, OperationOutcome.Failed, OperationOutcome.Conflict, OperationOutcome.Cancelled, OperationOutcome.RefreshFailed })
            {
                var entered = new TaskCompletionSource(); var release = new TaskCompletionSource();
                var model = new GitActionViewModel("fixture", actions, async () =>
                { entered.SetResult(); await release.Task; if (outcome == OperationOutcome.RefreshFailed) throw new IOException(); },
                    (_, _) => Task.FromResult(new ActionResult(outcome == OperationOutcome.Cancelled ? ProcessOutcome.Cancelled : ProcessOutcome.Exited,
                        outcome == OperationOutcome.Failed ? 1 : 0, "") { HasConflicts = outcome == OperationOutcome.Conflict }));
                model.RunCommand.Execute(null); await entered.Task;
                Expect(model.Activity is { IsComplete: false } && model.IsReconciling, "No premature completion during refresh");
                release.SetResult(); await model.RunningTask;
                Expect(model.Activity is { IsComplete: true } activity && activity.Outcome == outcome && activity.Seconds > 0, "Typed final outcome with frozen total duration");
            }
        });
        var repo = Fixture("operation-style-repo");
        var interaction = new OperationStyleInteraction();
        var shell = new ShellViewModel(reader, interaction, identities, actions: actions);
        try
        {
            await Check("Real stage/unstage and branch actions publish completion in the shell and clear it on tab change", async () =>
            {
                var git = await reader.DiscoverAsync(null, default);
                Expect((await runner.RunAsync(new(git.Executable, repo, ["init", "--initial-branch=main", "--object-format=sha1"]), default)).ExitCode == 0, "Disposable init");
                var tree = WriteFixtureObject(repo, "tree", []); var first = Commit(repo, tree, [], "Initial workspace\n");
                var second = Commit(repo, tree, [first], "Improve operation feedback\n");
                await File.WriteAllTextAsync(Path.Combine(repo, ".git", "refs", "heads", "main"), second + "\n");
                await File.WriteAllTextAsync(Path.Combine(repo, "local.txt"), "local edit\n");
                shell.CompleteStartup(new(new() { RepositoryPaths = [repo] }, SettingsLoadStatus.Loaded)); await shell.WaitForIdleAsync();
                Expect(shell.Activity is null, "Startup/background reads create no completion toast");
                interaction.Commit = async editor =>
                {
                    await editor.StartLoading(); await editor.DiffTask; editor.ShiftHeld = true;
                    editor.StageCommand.Execute(null); await editor.OperationTask; await editor.LoadingTask;
                    Expect(editor.Staged.Count == 1 && shell.Activity is { Outcome: OperationOutcome.Success }, "Real stage forwards result");
                    var attempt = shell.Activity!.Attempt;
                    editor.UnstageCommand.Execute(null); await editor.OperationTask; await editor.LoadingTask;
                    Expect(editor.Staged.Count == 0 && shell.Activity is { Outcome: OperationOutcome.Success } && !ReferenceEquals(attempt, shell.Activity.Attempt), "Unstage starts a distinct attempt");
                };
                shell.ShowChangesCommand.Execute(null); await shell.CommitDialogTask; await shell.WaitForIdleAsync();
                Expect(shell.Activity is { Outcome: OperationOutcome.Success }, "Closing commit retains its final feedback");
                // The application's existing branch-switch protection requires a clean worktree.
                File.Delete(Path.Combine(repo, "local.txt"));
                Expect(shell.NewBranchCommand.CanExecute(null), "Branch command ready after editor reconciliation");
                shell.NewBranchCommand.Execute(null); await shell.ActionTask;
                Expect(shell.SelectedRepository?.Snapshot?.Branch == "feedback" && shell.Activity is { Outcome: OperationOutcome.Success },
                    $"Actual branch action reaches shell feedback: {shell.SelectedRepository?.Snapshot?.Branch}; {shell.Activity?.Outcome}; {shell.CurrentAction?.Status}; {shell.CurrentAction?.RawOutput}; {interaction.Error}");
                var original = shell.SelectedRepository; var other = new RepositoryTab(Fixture("empty-tab")); shell.Repositories.Add(other);
                shell.SelectedRepository = other; Expect(shell.Activity is null, "Other tab cannot inherit completion"); await shell.WaitForIdleAsync();
                shell.SelectedRepository = original; await shell.WaitForIdleAsync(); Expect(shell.Activity is null, "Old completion does not replay on return");
            });
            await Check("Capsule holds then fades without moving text or allowing old completion to hide a new attempt", async () =>
            {
                var view = new OperationIndicator { Caption = "Pull · Completed" };
                var attempt = new object(); var activity = new OperationActivity(attempt, "Pull", "Completed", 5.3, OperationOutcome.Success);
                view.Activity = activity; Arrange(view, 240, 28); await Dispatcher.Yield(DispatcherPriority.ContextIdle);
                var badge = (Border)view.FindName("DurationBadge"); var caption = (TextBlock)view.FindName("CaptionText");
                var width = caption.ActualWidth; var left = caption.TransformToAncestor(view).Transform(new Point());
                Expect(badge.Opacity > .99, "Completion holds at full opacity");
                // No HWND/render loop exists here. Seek the WPF clock deterministically;
                // desktop scheduling/paint cadence requires separately authorized runtime QA.
                AnimationClock? Clock() => (AnimationClock?)typeof(OperationIndicator).GetField("_completionClock", System.Reflection.BindingFlags.NonPublic | System.Reflection.BindingFlags.Instance)!.GetValue(view);
                var clock = Clock()!;
                Expect(clock.Timeline.BeginTime == TimeSpan.FromSeconds(3) && clock.Timeline.Duration.TimeSpan == (SystemParameters.ClientAreaAnimation && !SystemParameters.HighContrast ? TimeSpan.FromSeconds(2) : TimeSpan.Zero), "Specified hold and motion preference");
                clock.Controller!.SeekAlignedToLastTick(TimeSpan.FromSeconds(.6), TimeSeekOrigin.BeginTime); var fading = badge.Opacity;
                Expect(fading < .99, "Completion fades after three seconds (or hides with disabled animation)");
                view.Activity = activity with { Seconds = 5.3 }; Expect(ReferenceEquals(clock, Clock()), "Same-attempt snapshot preserves the current clock");
                clock.Controller.SeekAlignedToLastTick(TimeSpan.FromSeconds(2.1), TimeSeekOrigin.BeginTime);
                Expect(badge.Opacity < .01, "Repeated final snapshot cannot restart fade");
                view.Activity = new(new object(), "Fetch", "Running", 9.9, OperationOutcome.Running);
                Expect(Clock() is null, "New attempt detaches old animation");
                view.Activity = view.Activity with { Seconds = 1000.0 }; Arrange(view, 240, 28); await Dispatcher.Yield(DispatcherPriority.ContextIdle);
                Expect(badge.Opacity > .99 && caption.ActualWidth == width && caption.TransformToAncestor(view).Transform(new Point()) == left, "New attempt replaces old clock; timing never shifts caption");
                view.Activity = null; Expect(badge.Opacity == 0 && caption.ActualWidth == width, "Clearing feedback preserves layout");
            });
            await Check("Rounded operation panel renders at normal/minimum sizes with readable dark/light/high-contrast states", async () =>
            {
                var window = new MainWindow(shell); var content = (FrameworkElement)window.Content;
                try
                {
                    foreach (var theme in Enum.GetValues<AppAppearance>())
                    {
                        AppTheme.Apply(Application.Current.Resources, false, theme);
                        foreach (var state in new[] { "Running", "Success", "Warning", "Failed" })
                        {
                            Color C(string role) => ((SolidColorBrush)Application.Current.Resources["Theme.Operation" + state + role]).Color;
                            Expect(Contrast(C("Text"), C("Background")) >= 4.5, "Readable capsule " + theme + state);
                        }
                        foreach (var size in new[] { new Size(1200, 800), new Size(900, 580) })
                        {
                            Arrange(content, (int)size.Width, (int)size.Height);
                            var view = VisualDescendants<OperationIndicator>(content).Single();
                            view.Caption = "Pull · Completed"; view.Activity = new(new object(), "Pull", "Completed", 5.3, OperationOutcome.Success);
                            await Dispatcher.Yield(DispatcherPriority.ContextIdle);
                            SaveRender(content, $"operation-{theme}-{size.Width}.png", (int)size.Width, (int)size.Height);
                            Expect(view.ActualWidth >= 180 && ((TextBlock)view.FindName("CaptionText")).ActualWidth >= 90, "Minimum width retains readable operation caption");
                        }
                    }
                    AppTheme.Apply(Application.Current.Resources, true);
                    Expect(Application.Current.Resources["Theme.OperationSuccessText"] == SystemColors.WindowTextBrush && Application.Current.Resources["Theme.OperationSuccessBackground"] == SystemColors.WindowBrush, "High contrast uses system colors");
                }
                finally { window.Close(); AppTheme.Apply(Application.Current.Resources, false); }
            });
        }
        finally { shell.BeginShutdown(); await shell.WaitForIdleAsync(); }
    }
    private sealed class OperationStyleInteraction : IUserInteraction
    {
        public string Error = "";
        public void ShowActionError(string message) => Error = message;
        public Func<CommitViewModel, Task>? Commit;
        public string? ChooseRepository() => null;
        public string? ChooseGit() => null;
        public Task ShowCommitAsync(CommitViewModel model) => Commit?.Invoke(model) ?? Task.CompletedTask;
        public void ShowGitAction(GitActionViewModel model) { model.NewName = "feedback"; model.RunCommand.Execute(null); }
    }
}
