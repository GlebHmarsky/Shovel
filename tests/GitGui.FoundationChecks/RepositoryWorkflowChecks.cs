using System.IO;
using System.Text;
using System.Text.Json;
using System.Windows;
using System.Windows.Controls;
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
    private static bool _repositoryWorkflowChecks;
    private static async Task RunRepositoryWorkflowChecksAsync()
    {
        await Check("Stash-only incoming tracks cannot own ordinary commits, including page boundaries", () =>
        {
            CommitRecord Node(string hash, params string[] parents) => new(hash, parents, "A", "a@example.invalid", DateTimeOffset.UnixEpoch, "A", "a@example.invalid", DateTimeOffset.UnixEpoch, hash);
            var nodes = new[] { Node("tip", "later"), Node("stash", "base"), Node("later", "base"), Node("base", "root"), Node("root") };
            var tips = new HashSet<string> { "stash" };
            var whole = CommitGraph.BuildPage(nodes, GraphCursor.Start, stashTips: tips);
            Expect(whole.Rows[3].NodeLane == whole.Rows[0].NodeLane && whole.Rows[3].NodeColor == whole.Rows[0].NodeColor, "Mainline owns shared base");
            var isolated = new[] { Node("stash", "base"), Node("base", "root"), Node("root") };
            var first = CommitGraph.BuildPage(isolated[..1], GraphCursor.Start, stashTips: tips);
            var rest = CommitGraph.BuildPage(isolated[1..], first.Next, stashTips: tips);
            Expect(rest.Rows[0].NodeColor != first.Rows[0].NodeColor && rest.Rows[0].NodeLane != first.Rows[0].NodeLane, "Stash connection terminates at distinct ordinary lane");
            Expect(rest.Rows[0].Lines.Any(l => l.FromLane == first.Rows[0].NodeLane && l.ToLane == rest.Rows[0].NodeLane && l.ToY == .5), "Actual base connection retained");
            Expect(JsonSerializer.Serialize(first.Rows.Concat(rest.Rows)) == JsonSerializer.Serialize(CommitGraph.BuildPage(isolated, GraphCursor.Start, stashTips: tips).Rows), "Cursor carries stash semantics across pages");
            return Task.CompletedTask;
        });
        await Check("Proportional splitters preserve ratios on both axes and respect minimums", () =>
        {
            var grid = new Grid();
            grid.ColumnDefinitions.Add(new() { Width = new(400), MinWidth = 160 }); grid.ColumnDefinitions.Add(new() { Width = new(1, GridUnitType.Star), MinWidth = 160 });
            grid.RowDefinitions.Add(new() { Height = new(1, GridUnitType.Star), MinHeight = 100 }); grid.RowDefinitions.Add(new() { Height = new(300), MinHeight = 100 });
            Arrange(grid, 800, 600); ProportionalPanes.Columns(grid.ColumnDefinitions[0], grid.ColumnDefinitions[1]); ProportionalPanes.Rows(grid.RowDefinitions[0], grid.RowDefinitions[1]);
            Arrange(grid, 1200, 900);
            Expect(Math.Abs(grid.ColumnDefinitions[0].ActualWidth - 600) < 1 && Math.Abs(grid.RowDefinitions[1].ActualHeight - 450) < 1, "50/50 survives growth");
            Arrange(grid, 400, 300);
            Expect(Math.Abs(grid.ColumnDefinitions[0].ActualWidth - 200) < 1 && Math.Abs(grid.RowDefinitions[1].ActualHeight - 150) < 1, "50/50 survives shrinking");
            return Task.CompletedTask;
        });
        var runner = new RepositoryWorkflowRunner(); var reader = new RepositoryReader(runner); var ids = new WindowsRepositoryIdentity();
        var git = await reader.DiscoverAsync(null, default); var actions = new GitActions(runner, reader, ids); var ui = new RepositoryWorkflowInteraction();
        async Task<string> Git(string path, params string[] args)
        {
            var result = await runner.Inner.RunAsync(new(git.Executable, path, args), default);
            Expect(result.ExitCode == 0, Encoding.UTF8.GetString(result.StandardError)); return Encoding.UTF8.GetString(result.StandardOutput).Trim();
        }
        var source = Fixture("workflow-source"); var a = Fixture("workflow-a"); var b = Fixture("workflow-b");
        await Git(source, "init", "-b", "main"); await File.WriteAllTextAsync(Path.Combine(source, "file.txt"), "base\n"); await Git(source, "add", ".");
        await Git(source, "-c", "user.name=Fixture", "-c", "user.email=fixture@example.invalid", "commit", "-m", "base");
        await Git(a, "clone", "--no-hardlinks", source, "."); await Git(b, "clone", "--no-hardlinks", source, ".");
        foreach (var path in new[] { a, b }) { await Git(path, "config", "user.name", "Fixture"); await Git(path, "config", "user.email", "fixture@example.invalid"); }
        var linked = Path.Combine(Path.GetDirectoryName(a)!, "linked-workflow");
        await Git(a, "worktree", "add", "-b", "linked", linked);
        var shell = new ShellViewModel(reader, ui, ids, actions: actions);
        shell.CompleteStartup(new(new() { RepositoryPaths = [a, b, linked] }, SettingsLoadStatus.Loaded)); await shell.WaitForIdleAsync();
        var firstTab = shell.Repositories[0]; var secondTab = shell.Repositories[1];
        try
        {
            await Check("Two repositories fetch concurrently while switching tabs, retaining independent results", async () =>
            {
                runner.HoldFetch = true;
                shell.DefaultFetchCommand.Execute(null); var firstTask = shell.ActionTask; var firstAction = shell.CurrentAction;
                await runner.FirstStarted.Task.WaitAsync(TimeSpan.FromSeconds(8));
                Expect(shell.CanNavigate && shell.ActionOpen && !shell.CanPrepareUpdate, "Navigation allowed, update/shutdown remains guarded");
                var linkedTab = shell.Repositories[2]; shell.SelectedRepository = linkedTab; await shell.WaitForIdleAsync();
                Expect(!shell.DefaultFetchCommand.CanExecute(null) && !shell.ShowChangesCommand.CanExecute(null), "Linked worktree shares mutation admission");
                shell.SelectedRepository = secondTab; await shell.WaitForIdleAsync();
                Expect(shell.SelectedRepository == secondTab && shell.DefaultFetchCommand.CanExecute(null), "Other repo admits work");
                shell.DefaultFetchCommand.Execute(null); var secondTask = shell.ActionTask; var secondAction = shell.CurrentAction;
                await runner.SecondStarted.Task.WaitAsync(TimeSpan.FromSeconds(8));
                shell.SelectedRepository = firstTab;
                Expect(shell.CurrentAction == firstAction && !shell.DefaultPushCommand.CanExecute(null), "In-flight result follows tab and blocks only conflicting mutations");
                shell.CloseTab(firstTab); Expect(shell.Repositories.Contains(firstTab), "Running tab remains owned until completion");
                runner.Release.TrySetResult(); await Task.WhenAll(firstTask, secondTask); await shell.WaitForIdleAsync(); runner.HoldFetch = false;
                Expect(firstAction?.Succeeded == true && secondAction?.Succeeded == true && !shell.ActionOpen && shell.SelectedRepository == firstTab, "Both real fetches finish without switching selection");
                Expect(firstAction?.Activity?.Outcome == OperationOutcome.Success && secondAction?.Activity?.Outcome == OperationOutcome.Success, "Inactive refresh is not a spurious failure");
                shell.SelectedRepository = secondTab; await shell.WaitForIdleAsync(); Expect(shell.CurrentAction == secondAction, "Other result survives completion");
                shell.CloseTab(linkedTab);
            });
            await Check("Rejected push raises one attention result with latest logs, without closing other work", async () =>
            {
                await File.AppendAllTextAsync(Path.Combine(source, "file.txt"), "remote\n");
                await Git(source, "-c", "user.name=Fixture", "-c", "user.email=fixture@example.invalid", "commit", "-am", "remote advanced");
                shell.SelectedRepository = firstTab; await shell.WaitForIdleAsync();
                await File.WriteAllTextAsync(Path.Combine(a, "local.txt"), "local\n"); await Git(a, "add", "."); await Git(a, "commit", "-m", "local diverged");
                shell.RefreshRepositoryCommand.Execute(null); await shell.WaitForIdleAsync();
                var before = ui.Problems.Count; shell.DefaultPushCommand.Execute(null); await shell.ActionTask;
                Expect(shell.CurrentAction?.Succeeded == false && ui.Problems.Count == before + 1, "Failure demands attention exactly once");
                Expect(ui.Problems[^1].Repository == a && ui.Problems[^1].Output.Contains("rejected", StringComparison.OrdinalIgnoreCase), "Rejected push logs identify owner and native reason");
            });
            await Check("F5 command refresh includes new local changes and does not alter selected bottom pane", async () =>
            {
                shell.DetailsIndex = 1;
                await File.WriteAllTextAsync(Path.Combine(a, "new.txt"), "untracked\n");
                shell.RefreshRepositoryCommand.Execute(null); await shell.WaitForIdleAsync();
                Expect(shell.CommitButtonLabel.Contains('1') && shell.DetailsIndex == 1, "Explicit refresh updates count and retains pane");
                var window = new MainWindow(shell);
                Arrange(window, 1200, 800); await System.Windows.Threading.Dispatcher.Yield(System.Windows.Threading.DispatcherPriority.ContextIdle);
                Expect(window.InputBindings.OfType<System.Windows.Input.KeyBinding>().Any(k => k.Key == System.Windows.Input.Key.F5 && k.Command == shell.RefreshRepositoryCommand), "Window F5 invokes full refresh");
                window.Close();
            });
            await Check("Auto-fetch honors 10-second cadence, uses no prompts, and deduplicates repeated background failures", async () =>
            {
                await shell.WaitForIdleAsync(); var statusCalls = runner.StatusCalls;
                runner.FailFetch = true; var now = DateTimeOffset.UtcNow; var before = runner.FetchCalls; var problems = ui.Problems.Count;
                await shell.AutoFetchDueAsync(now); var calls = runner.FetchCalls;
                Expect(calls == before + 2 && ui.Problems.Count == problems + 2, "Both validated repositories attempted and failures reported");
                await shell.AutoFetchDueAsync(now + TimeSpan.FromSeconds(9)); Expect(runner.FetchCalls == calls, "No early retry");
                await shell.AutoFetchDueAsync(now + ShellViewModel.AutoFetchInterval); Expect(runner.FetchCalls == calls + 2 && ui.Problems.Count == problems + 2, "Retry retains log but suppresses identical alert");
                Expect(shell.Journal.Entries.Any(e => e.Background && e.Title.Contains("Fetch")), "Background category includes auto-fetch");
                Expect(runner.LastFetch!.Environment["GCM_INTERACTIVE"] == "never" && ui.AuthPrompts == 0, "Automatic fetch does not prompt for credentials");
                runner.FailFetch = false; await shell.AutoFetchDueAsync(now + TimeSpan.FromSeconds(20));
                Expect(shell.CurrentAction?.Succeeded == true, "Recovery succeeds and rearms notification");
                Expect(runner.StatusCalls == statusCalls, "Background fetch does not poll local status");
            });
            await Check("Successful commit signals close after reconciliation; rejected commit retains message and logs", async () =>
            {
                await File.AppendAllTextAsync(Path.Combine(b, "file.txt"), "edit\n"); await Git(b, "add", ".");
                var model = new CommitViewModel(b, git, reader, actions, ui); await model.StartLoading(); var closed = 0;
                model.CommitCompleted += (_, _) => closed++;
                model.Subject = "keyboard commit"; Expect(model.CommitCommand.CanExecute(null), "Valid staged commit");
                model.CommitCommand.Execute(null); await model.OperationTask;
                Expect(closed == 1 && model.Subject == "" && !model.IsBusy && await Git(b, "log", "-1", "--format=%s") == "keyboard commit", "Only completed commit signals close");
                await model.CloseAsync();
                await File.AppendAllTextAsync(Path.Combine(b, "file.txt"), "blocked\n"); await Git(b, "add", ".");
                await File.WriteAllTextAsync(Path.Combine(b, ".git", "hooks", "pre-commit"), "#!/bin/sh\necho blocked-by-fixture >&2\nexit 1\n", new UTF8Encoding(false));
                model = new(b, git, reader, actions, ui); await model.StartLoading(); closed = 0; model.CommitCompleted += (_, _) => closed++;
                model.Subject = "keep this message"; var problems = ui.Problems.Count; model.CommitCommand.Execute(null); await model.OperationTask;
                Expect(closed == 0 && model.Subject == "keep this message" && ui.Problems.Count == problems + 1 && ui.Problems[^1].Output.Contains("blocked-by-fixture"), "Hook rejection stays open with draft and attention output");
                await model.CloseAsync();
            });
            await Check("Attention dialog renders themed, selectable latest output at normal and minimum sizes", async () =>
            {
                foreach (var theme in new[] { AppAppearance.Dark, AppAppearance.Light })
                {
                    AppTheme.Apply(Application.Current.Resources, false, theme);
                    var window = new OperationProblemWindow(new(a, "Push", "Push rejected. Pull the remote changes before pushing again.", "git push origin main\n! [rejected] main -> main (fetch first)"));
                    var content = (FrameworkElement)window.Content;
                    foreach (var size in new[] { (840, 520), (560, 320) })
                    {
                        Arrange(content, size.Item1, size.Item2); await System.Windows.Threading.Dispatcher.Yield(System.Windows.Threading.DispatcherPriority.ContextIdle);
                        Expect(((Grid)content).Children.OfType<TextBox>().Single().Text.Contains("rejected"), "Visible bound logs are present");
                        SaveRender(content, $"operation-attention-{theme}-{size.Item1}.png", size.Item1, size.Item2);
                    }
                    window.Close();
                }
                AppTheme.Apply(Application.Current.Resources, false);
            });
            await Check("Only failures, conflicts, refresh failures and real warning output demand attention", () =>
            {
                Expect(OperationAttention.Required(OperationOutcome.Failed, "") && OperationAttention.Required(OperationOutcome.Conflict, "") && OperationAttention.Required(OperationOutcome.RefreshFailed, ""), "Problem outcomes included");
                Expect(OperationAttention.Required(OperationOutcome.Success, "remote: warning: check policy") && !OperationAttention.Required(OperationOutcome.Success, "success") && !OperationAttention.Required(OperationOutcome.Cancelled, "warning: cancelled"), "Successful warnings included; cancellation stays quiet");
                return Task.CompletedTask;
            });
            await Check("Shutdown cancels and drains background fetch without an error alert", async () =>
            {
                runner.BlockBackground = true; var before = ui.Problems.Count;
                var pending = shell.AutoFetchDueAsync(DateTimeOffset.UtcNow + TimeSpan.FromMinutes(1));
                await runner.BackgroundStarted.Task.WaitAsync(TimeSpan.FromSeconds(8));
                Expect(shell.ActionOpen && !shell.HasUserOperation, "Only background work owns repositories");
                shell.BeginShutdown(); await shell.StopAutoFetchAsync(); await pending;
                Expect(!shell.ActionOpen && ui.Problems.Count == before, "Cancelled background work drains and stays quiet");
            });
        }
        finally { runner.Release.TrySetResult(); shell.BeginShutdown(); await shell.WaitForIdleAsync(); }
    }
    private sealed class RepositoryWorkflowInteraction : IUserInteraction
    {
        public List<OperationProblem> Problems { get; } = [];
        public int AuthPrompts { get; private set; }
        public string? ChooseRepository() => null;
        public string? ChooseGit() => null;
        public void ShowOperationProblem(string repository, string title, string status, string output) => Problems.Add(new(repository, title, status, output));
        public Task<AuthenticationAnswer?> ShowAuthenticationAsync(string context, AuthenticationPrompt prompt, CancellationToken token)
        { AuthPrompts++; return Task.FromResult<AuthenticationAnswer?>(null); }
    }
    private sealed class RepositoryWorkflowRunner : IProcessRunner
    {
        public WindowsProcessRunner Inner { get; } = new();
        public bool HoldFetch, FailFetch, BlockBackground;
        public TaskCompletionSource BackgroundStarted { get; } = new(TaskCreationOptions.RunContinuationsAsynchronously);
        public int FetchCalls, StatusCalls;
        public ProcessRequest? LastFetch;
        public TaskCompletionSource FirstStarted { get; } = new(TaskCreationOptions.RunContinuationsAsynchronously);
        public TaskCompletionSource SecondStarted { get; } = new(TaskCreationOptions.RunContinuationsAsynchronously);
        public TaskCompletionSource Release { get; } = new(TaskCreationOptions.RunContinuationsAsynchronously);
        public async Task<ProcessResult> RunAsync(ProcessRequest request, CancellationToken token, Action<ProcessOutput>? output = null)
        {
            if (request.Arguments.Contains("status")) Interlocked.Increment(ref StatusCalls);
            if (request.Arguments.Contains("fetch"))
            {
                LastFetch = request;
                if (BlockBackground) { BackgroundStarted.TrySetResult(); await Task.Delay(Timeout.Infinite, token); }
                var count = Interlocked.Increment(ref FetchCalls);
                if (HoldFetch) { (count == 1 ? FirstStarted : SecondStarted).TrySetResult(); await Release.Task.WaitAsync(token); }
                if (FailFetch) return new(ProcessOutcome.Exited, 1, [], Encoding.UTF8.GetBytes("fatal: fixture remote unavailable"), TimeSpan.Zero);
            }
            return await Inner.RunAsync(request, token, output);
        }
    }
}
