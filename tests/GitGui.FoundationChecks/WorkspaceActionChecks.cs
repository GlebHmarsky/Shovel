using System.IO;
using System.Windows;
using System.Windows.Automation;
using System.Windows.Controls;
using System.Windows.Threading;
using GitGui.App.Localization;
using GitGui.App.ViewModels;
using GitGui.App.Views;
using GitGui.Core.Git;
using GitGui.Core.Processes;
using GitGui.Core.Settings;
using GitGui.ProcessHost;

namespace GitGui.FoundationChecks;

internal static partial class Program
{
    private static async Task RunWorkspaceActionChecksAsync()
    {
        var git = _actionFixture!.Git;
        var inner = new WindowsProcessRunner(); var reader = new RepositoryReader(inner);
        var identities = new WindowsRepositoryIdentity();
        var repo = Fixture("workspace pull 日本語"); var remote = Fixture("workspace remote.git"); var peer = Fixture("workspace peer");
        async Task Git(string path, params string[] args)
        {
            var result = await inner.RunAsync(new(git.Executable, path, args), default);
            Expect(result.Outcome == ProcessOutcome.Exited && result.ExitCode == 0, "Disposable workspace fixture Git command succeeded");
        }
        await Git(remote, "init", "--bare", "-b", "main"); await Git(repo, "init", "-b", "main");
        await Git(repo, "config", "user.name", "Workspace Fixture"); await Git(repo, "config", "user.email", "workspace@example.invalid");
        await File.WriteAllTextAsync(Path.Combine(repo, "base.txt"), "base");
        await Git(repo, "add", "."); await Git(repo, "commit", "-m", "Workspace base");
        await Git(repo, "remote", "add", "origin", remote); await Git(repo, "push", "--set-upstream", "origin", "main");
        await Git(peer, "clone", remote, ".");
        await Git(peer, "config", "user.name", "Workspace Peer"); await Git(peer, "config", "user.email", "peer@example.invalid");
        await File.WriteAllTextAsync(Path.Combine(peer, "incoming.txt"), "real incoming content");
        await Git(peer, "add", "."); await Git(peer, "commit", "-m", "Incoming workspace commit"); await Git(peer, "push", "origin", "main");
        var expected = await reader.InspectAsync(git, peer, default);
        var gate = new PausedFetchRunner(inner);
        var interaction = new ActionInteraction();
        interaction.Opening = model => Expect(model.CanEdit && model.Remote?.Name == "origin",
            "Shell opens fully editable options from cached local targets without preparation");
        var observedReader = new RepositoryReader(gate);
        var shell = new ShellViewModel(observedReader, interaction, identities, actions: new GitActions(gate, observedReader, identities));
        shell.CompleteStartup(new(new() { RepositoryPaths = [repo, peer], SelectedRepositoryPath = repo }, SettingsLoadStatus.Loaded));
        await shell.WaitForIdleAsync();
        await Check("All operation dialogs open and dismiss with zero Git calls from the cached tab", async () =>
        {
            interaction.Show = model => new GitActionWindow(model).Close();
            var calls = gate.Calls;
            foreach (var command in new[] { shell.NewBranchCommand, shell.FetchCommand, shell.PullCommand, shell.PushCommand })
            {
                command.Execute(null); await shell.ActionTask;
            }
            shell.SwitchBranchCommand.Execute("main"); await shell.ActionTask;
            await Dispatcher.Yield(DispatcherPriority.ContextIdle);
            Expect(gate.Calls == calls && !shell.HasAction, "Opening, closing and pumping dispatch queue perform no Git commands");
        });
        var optionsClosed = false;
        interaction.Show = model =>
        {
            var dialog = new GitActionWindow(model);
            dialog.Closed += (_, _) => optionsClosed = true;
            var content = (FrameworkElement)dialog.Content;
            Arrange(content, 650, 520);
            var run = VisualDescendants<Button>(content).Single(button => button.Command == model.RunCommand);
            run.Command!.Execute(null);
            Expect(optionsClosed && model.IsRunning, "Actual options window closes while execution is still pending");
            model.RunCommand.Execute(null); // A second click must not start another operation.
        };

        await Check("Pull closes options, shows main-window progress and fast-forwards real files/history/counts", async () =>
        {
            var oldRows = shell.Commits;
            shell.PullCommand.Execute(null);
            await gate.Started.Task.WaitAsync(TimeSpan.FromSeconds(20));
            await Dispatcher.Yield(DispatcherPriority.ContextIdle);
            Expect(optionsClosed && shell.ActionOpen && shell.IsActionRunning && shell.HasAction, "Main shell owns submitted operation");
            Expect(shell.OperationStatus.Contains(UiText.Get("ActionStageFetching")) && shell.CancelCommand.CanExecute(null), "Visible fetch phase and cancellation");
            Expect(ReferenceEquals(oldRows, shell.Commits) && !shell.PullCommand.CanExecute(null), "Rows retained and duplicate action blocked");
            var original = shell.SelectedRepository;
            shell.SelectedRepository = shell.Repositories[1]; shell.CloseTab(original!);
            Expect(shell.SelectedRepository == original && shell.Repositories.Contains(original!), "Repository admission stays closed until reconciliation");
            var window = new MainWindow(shell);
            try
            {
                var content = (FrameworkElement)window.Content;
                Arrange(content, 1200, 760); await Dispatcher.Yield(DispatcherPriority.ContextIdle);
                SaveRender(content, "pull-running.png", 1200, 760);
                Arrange(content, 900, 580); await Dispatcher.Yield(DispatcherPriority.ContextIdle);
                SaveRender(content, "pull-running-minimum.png", 900, 580);
                Expect(VisualDescendants<ProgressBar>(content).Single(item => AutomationProperties.GetAutomationId(item) == "ActionProgress").IsIndeterminate,
                    "Real running state drives progress, without a fabricated percentage");
                gate.Release.TrySetResult();
                await shell.ActionTask.WaitAsync(TimeSpan.FromSeconds(30));
                Expect(shell.CurrentAction is { Succeeded: true, IsRunning: false } && !shell.ActionOpen && shell.CanNavigate, "Success retained after reinspection");
                Expect(shell.SelectedRepository!.Snapshot!.Commit == expected.Commit && shell.Commits.Any(row => row.Commit.Hash == expected.Commit), "Local HEAD and visible history really advanced");
                Expect(await File.ReadAllTextAsync(Path.Combine(repo, "incoming.txt")) == "real incoming content" && shell.PullCount == "0", "Worktree updated and behind count refreshed");
                Expect(shell.CurrentAction!.RawOutput.Length > 0 && shell.OperationStatus.Contains(UiText.Get("ActionCompletedShort")) && shell.DetailsIndex == 2,
                    "Completion and original Git output remain available");
                Arrange(content, 1200, 760); await Dispatcher.Yield(DispatcherPriority.ContextIdle);
                SaveRender(content, "pull-completed.png", 1200, 760);
                Arrange(content, 900, 580); await Dispatcher.Yield(DispatcherPriority.ContextIdle);
                SaveRender(content, "pull-completed-minimum.png", 900, 580);
            }
            finally { gate.Release.TrySetResult(); window.Close(); }
            Expect(gate.Fetches == 1, "Exactly one requested fetch ran");
        });
        await Check("Pull failures retain actionable details and the correct repository's result", async () =>
        {
            await File.WriteAllTextAsync(Path.Combine(repo, "base.txt"), "local changes to preserve");
            shell.PullCommand.Execute(null); await shell.ActionTask.WaitAsync(TimeSpan.FromSeconds(25));
            Expect(shell.CurrentAction?.Status == UiText.Get("ActionResultExited") && !shell.CurrentAction.Succeeded && !shell.ActionOpen && shell.HasAction, "Git's dirty-worktree rejection is visible after options close");
            Expect(await File.ReadAllTextAsync(Path.Combine(repo, "base.txt")) == "local changes to preserve", "Local changes preserved");
            await File.WriteAllTextAsync(Path.Combine(repo, "base.txt"), "base");
            await Git(repo, "remote", "set-url", "origin", Path.Combine(_root, "unavailable-workspace-remote.git"));
            shell.PullCommand.Execute(null); await shell.ActionTask.WaitAsync(TimeSpan.FromSeconds(25));
            Expect(shell.CurrentAction is { Succeeded: false, IsRunning: false } && shell.CurrentAction.RawOutput.Length > 0 &&
                shell.CurrentAction.Status == UiText.Get("ActionResultExited"), "Real unavailable-remote error/output retained");
            Expect(shell.SelectedRepository!.Snapshot!.Commit == expected.Commit && shell.Commits.Count > 0, "Failed fetch retains inspectable history");
            var result = shell.CurrentAction; var original = shell.SelectedRepository;
            shell.SelectedRepository = shell.Repositories[1]; await shell.WaitForIdleAsync();
            Expect(!shell.HasAction && shell.CurrentAction is null && shell.DetailsIndex != 2, "Another repository never displays this result");
            shell.SelectedRepository = original; await shell.WaitForIdleAsync();
            Expect(ReferenceEquals(shell.CurrentAction, result), "Bounded last result remains available on its original tab");
            interaction.Show = model => new GitActionWindow(model).Close();
            var attempts = gate.Fetches;
            shell.PullCommand.Execute(null); await shell.ActionTask.WaitAsync(TimeSpan.FromSeconds(20));
            Expect(gate.Fetches == attempts && ReferenceEquals(shell.CurrentAction, result), "Dismissing options does not execute or discard the previous result");
            await Git(repo, "remote", "set-url", "origin", remote);
        });
        await Check("Cached targets include unfetched remotes and reject changed refs on submission", async () =>
        {
            await Git(repo, "remote", "add", "unfetched", remote);
            shell.RefreshRepositoryCommand.Execute(null); await shell.WaitForIdleAsync();
            interaction.Show = model =>
            {
                Expect(model.Remotes.Any(item => item.Name == "unfetched") && !model.Context.History.References.Any(item => item.Name.StartsWith("refs/remotes/unfetched/")),
                    "Configured local remotes do not require a prior fetch");
                new GitActionWindow(model).Close();
            };
            shell.FetchCommand.Execute(null); await shell.ActionTask;
            await Git(repo, "branch", "external-change");
            interaction.Show = model => model.RunCommand.Execute(null);
            var fetches = gate.Fetches;
            shell.PullCommand.Execute(null); await shell.ActionTask;
            Expect(shell.CurrentAction?.Status == UiText.Get("ActionChanged") && gate.Fetches == fetches,
                "Refs changed since cached view fail before network work and reconcile");
        });
        await Check("Main-window cancellation drains an operation after its options dialog closes", async () =>
        {
            var paused = new PausedFetchRunner(inner);
            var cancelInteraction = new ActionInteraction { Show = model => model.RunCommand.Execute(null) };
            var cancelling = new ShellViewModel(reader, cancelInteraction, identities, actions: new GitActions(paused, reader, identities));
            cancelling.CompleteStartup(new(new() { RepositoryPaths = [repo] }, SettingsLoadStatus.Loaded));
            await cancelling.WaitForIdleAsync();
            cancelling.PullCommand.Execute(null); await paused.Started.Task.WaitAsync(TimeSpan.FromSeconds(20));
            cancelling.CancelCommand.Execute(null);
            Expect(!cancelling.CancelCommand.CanExecute(null), "Cancellation request cannot be repeatedly submitted");
            await cancelling.ActionTask.WaitAsync(TimeSpan.FromSeconds(15));
            Expect(!cancelling.ActionOpen && !cancelling.IsActionRunning && cancelling.CurrentAction?.Status == UiText.Get("ActionResultCancelled"), "Cancellation outcome survives reinspection");
            Expect(cancelling.SelectedRepository!.Snapshot!.Commit == expected.Commit && cancelling.Commits.Count > 0, "Fresh history remains usable");
        });
    }

    private sealed class PausedFetchRunner(IProcessRunner inner) : IProcessRunner
    {
        public TaskCompletionSource Started { get; } = new(TaskCreationOptions.RunContinuationsAsynchronously);
        public TaskCompletionSource Release { get; } = new(TaskCreationOptions.RunContinuationsAsynchronously);
        public int Fetches { get; private set; }
        public int Calls;
        public async Task<ProcessResult> RunAsync(ProcessRequest request, CancellationToken token, Action<ProcessOutput>? output = null)
        {
            Interlocked.Increment(ref Calls);
            if (request.Arguments.Contains("fetch"))
            {
                Fetches++; Started.TrySetResult();
                await Release.Task.WaitAsync(token);
            }
            return await inner.RunAsync(request, token, output);
        }
    }
}
