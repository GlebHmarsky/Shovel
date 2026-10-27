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
        await Check("Missing operation targets explain disabled actions without introducing consent", () =>
        {
            var model = new GitActionViewModel(context, GitAction.NewBranch, actions, () => Task.CompletedTask);
            Expect(!model.RunCommand.CanExecute(null) && model.RunUnavailableReason == UiText.Get("ActionEnterBranchName"), "Blank branch name has a concrete reason");
            model.NewName = "valid-name"; model.StartRef = "missing";
            Expect(!model.RunCommand.CanExecute(null) && model.RunUnavailableReason == UiText.Get("ActionNoStart"), "Missing starting ref is explained");
            model.StartRef = "HEAD";
            Expect(model.RunCommand.CanExecute(null), "Selecting a valid target enables creation");
            model.Cancel();
            Expect(!model.HasAttempt, "Cancel before execution leaves repository untouched");
            model = new(context, GitAction.Push, actions, () => Task.CompletedTask);
            model.Branch = "missing";
            Expect(!model.RunCommand.CanExecute(null) && model.RunUnavailableReason == UiText.Get("ActionChooseBranch"), "Unknown source branch is explained");
            model.Branch = "main"; model.RemoteBranch = " ";
            Expect(!model.RunCommand.CanExecute(null) && model.RunUnavailableReason == UiText.Get("ActionChooseRemoteBranch"), "Blank remote destination is explained");
            model.RemoteBranch = "main"; model.Remote = null;
            Expect(!model.RunCommand.CanExecute(null) && model.RunUnavailableReason == UiText.Get("ActionNoRemote"), "Missing remote is explained");
            return Task.CompletedTask;
        });
        await Check("Normal dialog buttons create, push, fetch, pull and switch using real local repositories", async () =>
        {
            foreach (var action in new[] { GitAction.NewBranch, GitAction.Push, GitAction.Fetch, GitAction.Pull, GitAction.SwitchBranch })
            {
                var fresh = await actions.PrepareAsync(context.Git, context.Repository.Worktree, default);
                var reconciled = 0;
                var model = new GitActionViewModel(fresh, action, actions, async () =>
                {
                    await reader.InspectAsync(fresh.Git, fresh.Repository.Worktree, default); reconciled++;
                }) { NewName = "ui-workflow", SetUpstream = true };
                if (action == GitAction.Push) { model.Branch = "ui-workflow"; model.RemoteBranch = "ui-workflow"; }
                if (action == GitAction.SwitchBranch) model.Branch = "main";
                Expect(model.RunCommand.CanExecute(null) && !model.HasAttempt, "Targets alone enable the ordinary action");
                model.RunCommand.Execute(null);
                await model.RunningTask.WaitAsync(TimeSpan.FromSeconds(30));
                Expect(model.Status == UiText.Get("ActionSucceeded") && reconciled == 1, "Real action succeeded and reinspected once");
                Expect(!model.RunCommand.CanExecute(null) && model.RunUnavailableReason == UiText.Get("ActionAlreadyAttempted"), "Another attempt needs fresh targets, not consent");
                var after = await actions.PrepareAsync(context.Git, context.Repository.Worktree, default);
                Expect(after.Repository.Branch == (action == GitAction.SwitchBranch ? "main" : "ui-workflow"), "Actual checked-out branch matches the operation");
                if (action is GitAction.Push or GitAction.Fetch or GitAction.Pull)
                    Expect(after.Repository.UpstreamRef == "refs/remotes/origin/ui-workflow" &&
                        after.History.References.Any(item => item.Name == "refs/remotes/origin/ui-workflow" && item.Hash == after.Repository.Commit),
                        "Real push/fetch/pull retains the expected upstream tip");
            }
        });
        await Check("Operation dialogs render offscreen at normal/minimum sizes with localized targets and only implemented options", async () =>
        {
            foreach (var action in Enum.GetValues<GitAction>())
            {
                var model = new GitActionViewModel(context, action, actions, () => Task.CompletedTask);
                model.NewName = "render-only";
                Expect(model.RunCommand.CanExecute(null) && !model.HasAttempt, "Valid targets enable the ordinary action without executing on dialog construction");
                var window = new GitActionWindow(model);
                try
                {
                    var content = (FrameworkElement)window.Content;
                    Arrange(content, 650, 520); await Dispatcher.Yield(DispatcherPriority.ContextIdle);
                    SaveRender(content, "action-" + action + ".png", 650, 520);
                    Arrange(content, 520, 440); await Dispatcher.Yield(DispatcherPriority.ContextIdle);
                    SaveRender(content, "action-" + action + "-minimum.png", 520, 440);
                    Expect(VisualDescendants<Button>(content).Any(button => button.Command == model.RunCommand && button.IsEnabled), "Ordinary action button is enabled for valid targets");
                    var options = VisualDescendants<CheckBox>(content).Where(box => box.Visibility == Visibility.Visible && box.IsVisible).ToArray();
                    Expect(options.All(box => box.IsEnabled), "No disabled prototype options remain");
                    Expect(Descendants<CheckBox>(window).Count() == 4, "Only checkout, all remotes, all tags and upstream options exist; no routine consent control");
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
                model.NewName = "ui-created"; model.Checkout = false;
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
                model.RunCommand.Execute(null);
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
