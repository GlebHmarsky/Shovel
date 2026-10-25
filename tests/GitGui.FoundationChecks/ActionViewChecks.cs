using System.IO;
using System.Windows;
using System.Windows.Automation;
using System.Windows.Controls;
using System.Windows.Threading;
using GitGui.App.Localization;
using GitGui.App.Services;
using GitGui.App.ViewModels;
using GitGui.App.Views;
using GitGui.Core.Git;
using GitGui.Core.Settings;
using GitGui.ProcessHost;

namespace GitGui.FoundationChecks;

internal static partial class Program
{
    private static async Task RunActionViewChecksAsync()
    {
        await Check("Ref badges distinguish current/local/remote/tag and combine only matching upstream tips", () =>
        {
            var hash = new string('a', 40);
            var commit = new CommitRecord(hash, [], "Author", "author@example.invalid", DateTimeOffset.Now, "Author", "author@example.invalid", DateTimeOffset.Now, "Badge fixture");
            var refs = new CommitRef[] { new("refs/heads/main", hash, "refs/remotes/origin/main"), new("refs/remotes/origin/main", hash), new("refs/remotes/other/main", hash), new("refs/tags/v1", hash), new("HEAD", hash) };
            var repository = new RepositorySnapshot("", "", "", "main", hash, HeadKind.Branch, RepositoryOperation.None, DateTimeOffset.Now);
            var entry = new HistoryEntry(commit, new(0, 0, 1, false, []), refs);
            var row = new CommitRow(entry, repository);
            Expect(row.Badges.Count == 3 && row.Badges[0] is { IsCurrent: true, HasRemote: true, Label: "main" }, "Configured same-tip upstream collapses into local badge");
            Expect(row.Badges.Any(item => item.IsRemote && item.Label == "other/main") && row.Badges.Any(item => item.IsTag), "Other remote and tag remain visible");
            Expect(row.AccessibilityLabel.Contains("Checked-out") && row.AccessibilityLabel.Contains("origin/main"), "Accessible current/synchronized relationship includes upstream name");
            var crowded = new CommitRow(entry with { References = [new("refs/heads/a-first", hash), .. refs] }, repository);
            Expect(crowded.Badges[0].IsCurrent, "Current branch must lead even when many refs share the tip");
            var localOnly = new CommitRow(entry with { References = [refs[0]] }, repository);
            Expect(!localOnly.Badges[0].HasRemote, "Different-tip remote must not collapse");
            var detached = new CommitRow(entry with { References = [refs[4]] }, repository with { Branch = null, HeadKind = HeadKind.Detached });
            Expect(detached.Badges.Single().IsCurrent, "Detached current tip has explicit badge");
            return Task.CompletedTask;
        });
        if (_actionFixture is null) throw new InvalidOperationException("Real action fixture must be available for dialog checks");
        var runner = new WindowsProcessRunner(); var reader = new RepositoryReader(runner);
        var identities = new WindowsRepositoryIdentity(); var actions = new GitActions(runner, reader, identities);
        var context = await actions.PrepareAsync(_actionFixture.Git, _actionFixture.Repository.Worktree, default);
        await Check("Operation dialogs render offscreen at normal/minimum sizes with localized scope and guarded execution", async () =>
        {
            foreach (var action in Enum.GetValues<GitAction>())
            {
                var model = new GitActionViewModel(context, action, actions, () => Task.CompletedTask);
                Expect(!model.RunCommand.CanExecute(null), "Configured helpers require explicit action approval");
                var window = new GitActionWindow(model);
                try
                {
                    var content = (FrameworkElement)window.Content;
                    Arrange(content, 650, 520); await Dispatcher.Yield(DispatcherPriority.ContextIdle);
                    SaveRender(content, "action-" + action + ".png", 650, 520);
                    Arrange(content, 520, 440); await Dispatcher.Yield(DispatcherPriority.ContextIdle);
                    SaveRender(content, "action-" + action + "-minimum.png", 520, 440);
                    Expect(VisualDescendants<Button>(content).Any(button => button.Command == model.RunCommand && !button.IsEnabled), "Visible action cannot execute before approval");
                    Expect(VisualDescendants<TextBox>(content).Any(box => AutomationProperties.GetName(box) == UiText.Get("ActionOutput")), "Output remains accessible");
                }
                finally { window.Close(); }
            }
        });
        await Check("Modal action blocks tab changes/close and refreshes real history after execution", async () =>
        {
            var interaction = new ActionInteraction();
            var shell = new ShellViewModel(reader, interaction, identities, actions: actions);
            shell.CompleteStartup(new(new() { RepositoryPaths = [context.Repository.Worktree], SelectedRepositoryPath = context.Repository.Worktree }, SettingsLoadStatus.Loaded));
            await shell.WaitForIdleAsync();
            Expect(shell.BranchMenuItems.Count > 0 && shell.BranchMenuItems.All(item => item.Command == shell.SwitchBranchCommand && item.Command.CanExecute(item.Name)), "Branch menu entries carry the real command without popup ancestor bindings");
            var shellWindow = new MainWindow(shell);
            try
            {
                var content = (FrameworkElement)shellWindow.Content;
                Arrange(content, 1200, 760); await Dispatcher.Yield(DispatcherPriority.ContextIdle);
                SaveRender(content, "actions-workspace.png", 1200, 760);
                Arrange(content, 900, 580); await Dispatcher.Yield(DispatcherPriority.ContextIdle);
                SaveRender(content, "actions-workspace-minimum.png", 900, 580);
            }
            finally { shellWindow.Close(); }
            var original = shell.SelectedRepository!;
            var other = new RepositoryTab(Fixture("inactive-action-tab")); shell.Repositories.Add(other);
            interaction.Show = model =>
            {
                Expect(shell.ActionOpen && !shell.CanNavigate, "Admission closed during modal");
                shell.SelectedRepository = other; shell.CloseTab(original);
                Expect(shell.SelectedRepository == original && shell.Repositories.Contains(original), "No close or tab switch during action");
                model.NewName = "ui-created"; model.Checkout = false; model.Approved = true;
                model.RunCommand.Execute(null);
                var frame = new DispatcherFrame();
                var dispatcher = Dispatcher.CurrentDispatcher;
                _ = model.RunningTask.ContinueWith(_ => dispatcher.BeginInvoke(() => frame.Continue = false), TaskScheduler.Default);
                Dispatcher.PushFrame(frame); model.RunningTask.GetAwaiter().GetResult();
                Expect(model.Status == UiText.Get("ActionSucceeded") && !model.RunCommand.CanExecute(null), "One attempt per fresh dialog");
                Expect(shell.LocalBranches.Contains("ui-created"), "Reconciliation updated history refs before modal exit");
                interaction.Done.TrySetResult();
            };
            shell.NewBranchCommand.Execute(null);
            await interaction.Done.Task.WaitAsync(TimeSpan.FromSeconds(20));
            Expect(!shell.ActionOpen && shell.CanNavigate, "Admission reopened only after completion");
        });
        await Check("Failed and cancelled modal actions always reconcile and release their running state", async () =>
        {
            foreach (var cancel in new[] { false, true })
            {
                var fresh = await actions.PrepareAsync(context.Git, context.Repository.Worktree, default);
                var controlled = new ControlledActionRunner(runner, cancel);
                var reconciled = 0;
                var model = new GitActionViewModel(fresh, GitAction.Push, new GitActions(controlled, reader, identities),
                    async () => { await reader.InspectAsync(fresh.Git, fresh.Repository.Worktree, default); reconciled++; });
                model.Approved = true; model.RunCommand.Execute(null);
                await controlled.Started.Task.WaitAsync(TimeSpan.FromSeconds(10));
                if (cancel) model.Cancel();
                await model.RunningTask.WaitAsync(TimeSpan.FromSeconds(10));
                Expect(reconciled == 1 && !model.IsRunning && !model.RunCommand.CanExecute(null), "Reinspect once, then require fresh dialog for retry");
                Expect(model.Status == UiText.Get(cancel ? "ActionResultCancelled" : "ActionResultExited"), "Cancellation/failure stays visible after reconciliation");
            }
        });
    }
    private sealed class ActionInteraction : IUserInteraction
    {
        public Action<GitActionViewModel>? Show { get; set; }
        public TaskCompletionSource Done { get; } = new();
        public string? ChooseRepository() => null;
        public string? ChooseGit() => null;
        public void ShowGitAction(GitActionViewModel model) => Show?.Invoke(model);
        public void ShowActionError(string message) => Done.TrySetException(new InvalidOperationException(message));
    }
    private sealed class ControlledActionRunner(GitGui.Core.Processes.IProcessRunner inner, bool waitForCancel) : GitGui.Core.Processes.IProcessRunner
    {
        public TaskCompletionSource Started { get; } = new();
        public async Task<GitGui.Core.Processes.ProcessResult> RunAsync(GitGui.Core.Processes.ProcessRequest request, CancellationToken token,
            Action<GitGui.Core.Processes.ProcessOutput>? output = null)
        {
            if (!request.Arguments.Contains("push")) return await inner.RunAsync(request, token, output);
            Started.TrySetResult();
            if (waitForCancel) await Task.Delay(Timeout.Infinite, token);
            return new(GitGui.Core.Processes.ProcessOutcome.Exited, 128, [], [], TimeSpan.Zero);
        }
    }
}
