using System.Windows;
using System.Windows.Threading;
using GitGui.App.Views;
using GitGui.App.Localization;

namespace GitGui.FoundationChecks;

internal static partial class Program
{
    private static async Task RunDivergenceViewChecksAsync()
    {
        await Check("History publishes before comparison; toolbar maps Pull to behind and Push to ahead", async () =>
        {
            var path = Fixture("counts-after-history");
            var runner = new ModelRunner(path) { HasUpstream = true, HoldDivergence = true }; runner.Release.TrySetResult();
            var model = CreateModel(runner, path); model.OpenRepositoryCommand.Execute(null);
            await runner.DivergenceStarted.Task.WaitAsync(TimeSpan.FromSeconds(5));
            Expect(model.Commits.Count == 1 && model.IsBusy && model.PullCount == UiText.Get("CountUnavailable"), "A slow comparison must not delay rows or invent zero");
            runner.ReleaseDivergence.TrySetResult(); await model.WaitForIdleAsync();
            Expect(model.PullCount == "2" && model.PushCount == "3", "Pull is behind and Push is ahead");
            var window = new MainWindow(model);
            try
            {
                var content = (FrameworkElement)window.Content;
                Arrange(content, 1200, 760); await Dispatcher.Yield(DispatcherPriority.ContextIdle);
                SaveRender(content, "workspace-counts.png", 1200, 760);
                Arrange(content, 900, 580); await Dispatcher.Yield(DispatcherPriority.ContextIdle);
                SaveRender(content, "workspace-counts-minimum.png", 900, 580);
            }
            finally { window.Close(); }
            runner.FailDivergence = true; model.RefreshRepositoryCommand.Execute(null); await model.WaitForIdleAsync();
            Expect(model.Commits.Count == 1 && model.PullCount == UiText.Get("CountUnavailable") && model.DivergenceHelp == UiText.Get("DivergenceFailed") && model.RawOutput.Contains("comparison fixture failure"), "Comparison failure retains rows and exposes diagnostics");
        });
        await Check("Cancelled or closed-tab comparison cannot publish obsolete counts", async () =>
        {
            foreach (var close in new[] { false, true })
            {
                var path = Fixture("obsolete-counts-" + close);
                var runner = new ModelRunner(path) { HasUpstream = true, HoldDivergence = true, IgnoreDivergenceCancellation = true }; runner.Release.TrySetResult();
                var model = CreateModel(runner, path); model.OpenRepositoryCommand.Execute(null);
                await runner.DivergenceStarted.Task.WaitAsync(TimeSpan.FromSeconds(5));
                if (close) model.CloseTabCommand.Execute(model.SelectedRepository); else model.CancelCommand.Execute(null);
                runner.ReleaseDivergence.TrySetResult(); await model.WaitForIdleAsync();
                Expect(model.PullCount == UiText.Get("CountUnavailable") && !model.IsBusy, "Late counts cannot overwrite cancelled/closed state");
                Expect(model.Commits.Count == (close ? 0 : 1), "Cancelling counts preserves history while closing clears it");
                if (!close) Expect(model.DivergenceHelp == UiText.Get("DivergenceFailed"), "Cancelled comparison does not remain indefinitely pending");
            }
        });
    }
}
