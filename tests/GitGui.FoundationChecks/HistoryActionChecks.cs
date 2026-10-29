using System.Diagnostics;
using System.IO;
using System.Text;
using System.Windows;
using System.Windows.Automation;
using System.Windows.Controls;
using System.Windows.Input;
using System.Windows.Threading;
using GitGui.App.Localization;
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
    private static bool _historyActionChecks, _defaultNetworkChecks;
    private static async Task RunHistoryActionChecksAsync()
    {
        var runner = new HistoryActionRunner(); var reader = new RepositoryReader(runner);
        var identities = new WindowsRepositoryIdentity(); var actions = new GitActions(runner, reader, identities);
        var git = await reader.DiscoverAsync(null, default);
        async Task<string> Git(string path, params string[] args)
        {
            var result = await runner.RunAsync(new(git.Executable, path, args), default);
            Expect(result.ExitCode == 0, Encoding.UTF8.GetString(result.StandardError));
            return Encoding.UTF8.GetString(result.StandardOutput).Trim();
        }
        async Task<(string Path, string A, string B)> Repo(string name)
        {
            var path = Fixture("history action " + name + " 日本語");
            await Git(path, "init", "-b", "main"); await Git(path, "config", "user.name", "History Fixture");
            await Git(path, "config", "user.email", "history@example.invalid"); await Git(path, "config", "core.autocrlf", "false");
            await File.WriteAllTextAsync(Path.Combine(path, "file.txt"), "a\n");
            await Git(path, "add", "."); await Git(path, "commit", "-m", "First"); var a = await Git(path, "rev-parse", "HEAD");
            await File.WriteAllTextAsync(Path.Combine(path, "file.txt"), "b\n");
            await Git(path, "commit", "-am", "Second"); return (path, a, await Git(path, "rev-parse", "HEAD"));
        }
        async Task<ActionContext> Context(string path)
        {
            var snapshot = await reader.InspectAsync(git, path, default);
            var history = await reader.OpenHistoryAsync(git, snapshot, HistoryScope.AllCommits, default);
            return await actions.PrepareBranchManagementAsync(git, await identities.ReadAsync(path, default), snapshot, history, default);
        }
        async Task Reject(string code, Func<Task> action)
        {
            try { await action(); throw new InvalidOperationException("Expected " + code); }
            catch (GitActionException error) { Expect(error.Code == code, error.Code); }
        }

        if (!_defaultNetworkChecks)
        {
            await Check("History menu uses clicked SHA, confirms exact reset scope, retains modal-free selection and renders split toolbar", async () =>
            {
                var repo = await Repo("menu"); var interaction = new HistoryInteraction();
                var shell = new ShellViewModel(reader, interaction, identities, actions: actions);
                shell.CompleteStartup(new(new() { RepositoryPaths = [repo.Path] }, SettingsLoadStatus.Loaded)); await shell.WaitForIdleAsync();
                var window = new MainWindow(shell);
                try
                {
                    var content = (FrameworkElement)window.Content; Arrange(content, 1200, 800);
                    await Dispatcher.Yield(DispatcherPriority.ContextIdle);
                    var table = (DataGrid)window.FindName("HistoryTable"); var target = shell.Commits.Single(row => row.Commit.Hash == repo.A);
                    var row = (DataGridRow)table.ItemContainerGenerator.ContainerFromItem(target);
                    var calls = runner.Calls; var watch = Stopwatch.StartNew();
                    row.RaiseEvent(new MouseButtonEventArgs(Mouse.PrimaryDevice, 0, MouseButton.Right) { RoutedEvent = UIElement.PreviewMouseRightButtonDownEvent });
                    var menu = window.PrepareHistoryMenu(row); Arrange(menu, 295, 170);
                    Expect(shell.SelectedCommit == target && runner.Calls == calls && table.ContextMenu is null, "Clicked row selected; no menu reads/background menu");
                    Console.WriteLine($"History menu construction/layout: {watch.Elapsed.TotalMilliseconds:F0} ms; zero Git calls");
                    SaveRender(menu, "history-context-menu.png", 295, 170);
                    var entries = shell.CreateHistoryMenu(target); entries[2].Command!.Execute(null);
                    Expect(interaction.Clipboard == repo.A && runner.Calls == calls, "Copy full SHA without Git");
                    entries[1].Children![2].Command!.Execute(null); await shell.ActionTask;
                    Expect(interaction.Confirmation.Contains(repo.Path) && interaction.Confirmation.Contains(repo.A) && interaction.Confirmation.Contains(repo.B) &&
                        interaction.Confirmation.Contains("main") && runner.Calls == calls, "Declined hard reset names exact scope without Git preparation");
                    foreach (var theme in new[] { AppAppearance.Dark, AppAppearance.Light })
                    {
                        AppTheme.Apply(Application.Current.Resources, false, theme);
                        foreach (var size in new[] { (1200, 800), (900, 620) })
                        {
                            Arrange(content, size.Item1, size.Item2); await Dispatcher.Yield(DispatcherPriority.ContextIdle);
                            var buttons = VisualDescendants<Button>(content).ToArray();
                            foreach (var id in new[] { "DefaultFetch", "FetchOptions", "DefaultPull", "PullOptions", "DefaultPush", "PushOptions" })
                            {
                                var button = buttons.Single(b => AutomationProperties.GetAutomationId(b) == id);
                                Expect(button.ActualWidth >= 22 && button.TranslatePoint(new(button.ActualWidth, 0), content).X < size.Item1, "Split controls fit");
                            }
                            SaveRender(content, $"history-actions-{theme}-{size.Item1}.png", size.Item1, size.Item2);
                        }
                    }
                    shell.EndHistoryMenu(); shell.RefreshRepositoryCommand.Execute(null); await shell.WaitForIdleAsync();
                    Expect(!entries[0].Command!.CanExecute(null), "Frozen menu invalid after refresh");
                    var checkout = shell.CreateHistoryMenu(shell.Commits.Single(row => row.Commit.Hash == repo.A))[0].Command!;
                    checkout.Execute(null); await shell.ActionTask;
                    Expect(shell.CurrentAction?.Succeeded == true && shell.SelectedRepository?.Snapshot?.HeadKind == HeadKind.Detached &&
                        await Git(repo.Path, "rev-parse", "HEAD") == repo.A, "Actual menu checkout reconciles detached HEAD");
                    Expect(shell.CurrentAction!.RawOutput.Contains(repo.A) && shell.CurrentAction!.RawOutput.Contains(repo.B), "Result retains requested and previous targets");
                }
                finally { window.Close(); AppTheme.Apply(Application.Current.Resources, false); }
            });
            await Check("Detached checkout preserves conflicting dirty files and rejects a stale ref snapshot", async () =>
            {
                var repo = await Repo("checkout"); var context = await Context(repo.Path);
                await File.WriteAllTextAsync(Path.Combine(repo.Path, "file.txt"), "dirty\n");
                var result = await actions.ExecuteAsync(context, new(GitAction.CheckoutCommit, StartRef: repo.A), default);
                Expect(!result.Success && result.Output.Length > 0 && await Git(repo.Path, "rev-parse", "HEAD") == repo.B &&
                    await File.ReadAllTextAsync(Path.Combine(repo.Path, "file.txt")) == "dirty\n", "Git dirty protection retained");
                await Git(repo.Path, "branch", "external-change");
                await Reject("ActionChanged", () => actions.ExecuteAsync(context, new(GitAction.ResetHard, StartRef: repo.A), default));
            });
            foreach (var mode in new[] { GitAction.ResetSoft, GitAction.ResetMixed, GitAction.ResetHard })
                await Check($"{mode} has exact HEAD/index/worktree semantics and preserves unrelated untracked files", async () =>
                {
                    var repo = await Repo(mode.ToString());
                    await File.WriteAllTextAsync(Path.Combine(repo.Path, "file.txt"), "staged\n"); await Git(repo.Path, "add", "file.txt");
                    await File.WriteAllTextAsync(Path.Combine(repo.Path, "file.txt"), "working\n");
                    await File.WriteAllTextAsync(Path.Combine(repo.Path, "keep.txt"), "keep");
                    var result = await actions.ExecuteAsync(await Context(repo.Path), new(mode, StartRef: repo.A), default);
                    Expect(result.Success, result.Output);
                    Expect(await Git(repo.Path, "rev-parse", "HEAD") == repo.A && await Git(repo.Path, "branch", "--show-current") == "main", "Move attached branch");
                    Expect(await Git(repo.Path, "show", ":file.txt") == (mode == GitAction.ResetSoft ? "staged" : "a"), "Exact index result");
                    Expect(await File.ReadAllTextAsync(Path.Combine(repo.Path, "file.txt")) == (mode == GitAction.ResetHard ? "a\n" : "working\n") &&
                        await File.ReadAllTextAsync(Path.Combine(repo.Path, "keep.txt")) == "keep", "Exact working tree result");
                    Expect(await Git(repo.Path, "rev-parse", "ORIG_HEAD") == repo.B, "Previous tip retained by Git");
                });
            await Check("Revert creates a commit and requires an explicit valid merge mainline", async () =>
            {
                var repo = await Repo("revert");
                var result = await actions.ExecuteAsync(await Context(repo.Path), new(GitAction.RevertCommit, StartRef: repo.B), default);
                Expect(result.Success && await Git(repo.Path, "rev-parse", "HEAD^") == repo.B && await Git(repo.Path, "show", "HEAD:file.txt") == "a", "New inverse commit, original history retained");
                await Git(repo.Path, "switch", "-c", "topic"); await File.WriteAllTextAsync(Path.Combine(repo.Path, "topic.txt"), "topic\n");
                await Git(repo.Path, "add", "."); await Git(repo.Path, "commit", "-m", "Topic"); await Git(repo.Path, "switch", "main");
                await Git(repo.Path, "merge", "--no-ff", "--no-edit", "topic"); var merge = await Git(repo.Path, "rev-parse", "HEAD"); var context = await Context(repo.Path);
                await Reject("HistoryRevertMainlineRequired", () => actions.ExecuteAsync(context, new(GitAction.RevertCommit, StartRef: merge), default));
                result = await actions.ExecuteAsync(context, new(GitAction.RevertCommit, StartRef: merge, Mainline: 1), default);
                Expect(result.Success && !File.Exists(Path.Combine(repo.Path, "topic.txt")) && await Git(repo.Path, "rev-parse", "HEAD^") == merge, "Selected mainline used");
            });
            await Check("Revert conflict exposes recovery; cancelled checkout drains without changing HEAD", async () =>
            {
                var repo = await Repo("conflict"); await File.WriteAllTextAsync(Path.Combine(repo.Path, "file.txt"), "c\n"); await Git(repo.Path, "commit", "-am", "Third");
                var tip = await Git(repo.Path, "rev-parse", "HEAD");
                var result = await actions.ExecuteAsync(await Context(repo.Path), new(GitAction.RevertCommit, StartRef: repo.B), default);
                Expect(!result.Success && result.HasConflicts && result.Operation == RepositoryOperation.Revert, "Conflict reported separately from exit code");
                result = await actions.ExecuteAsync(await actions.PrepareAsync(git, repo.Path, default), new(GitAction.Abort), default);
                Expect(result.Success && await Git(repo.Path, "rev-parse", "HEAD") == tip && await File.ReadAllTextAsync(Path.Combine(repo.Path, "file.txt")) == "c\n", "Existing Abort restores sequence start");
                var context = await Context(repo.Path); runner.PauseCheckout = true;
                using var cancel = new CancellationTokenSource(); var pending = actions.ExecuteAsync(context, new(GitAction.CheckoutCommit, StartRef: repo.A), cancel.Token);
                await runner.Started.Task.WaitAsync(TimeSpan.FromSeconds(10)); cancel.Cancel();
                try { await pending; throw new InvalidOperationException("Expected cancellation"); } catch (OperationCanceledException) { }
                runner.PauseCheckout = false;
                Expect(await Git(repo.Path, "rev-parse", "HEAD") == tip, "Cancelled pending command preserves HEAD; no rollback invented");
            });
        }
        await Check("Fetch and Push buttons run local remote workflows; arrows remain preparation-free and Commit remains modal", async () =>
        {
            var repo = await Repo("network"); var origin = Fixture("history origin.git"); var publish = Fixture("history publish.git");
            await Git(origin, "init", "--bare", "-b", "main"); await Git(publish, "init", "--bare", "-b", "main");
            await Git(repo.Path, "remote", "add", "origin", origin); await Git(repo.Path, "remote", "add", "publish", publish);
            await Git(repo.Path, "push", "-u", "origin", "main"); await Git(repo.Path, "config", "branch.main.pushRemote", "publish");
            await Git(repo.Path, "config", "push.default", "current");
            var interaction = new HistoryInteraction(); var shell = new ShellViewModel(reader, interaction, identities, actions: actions);
            shell.CompleteStartup(new(new() { RepositoryPaths = [repo.Path] }, SettingsLoadStatus.Loaded)); await shell.WaitForIdleAsync();
            var calls = runner.Calls;
            shell.FetchCommand.Execute(null); await shell.ActionTask; shell.PushCommand.Execute(null); await shell.ActionTask;
            Expect(interaction.Options == 2 && runner.Calls == calls, "Options use only cached information");
            shell.DefaultPushCommand.Execute(null); await shell.ActionTask;
            Expect(shell.CurrentAction?.Succeeded == true && interaction.Options == 2, (shell.CurrentAction?.Status ?? "Push not attempted") + " " + shell.CurrentAction?.RawOutput);
            Expect(await Git(publish, "rev-parse", "refs/heads/main") == repo.B, "Push honors separate configured push remote");
            await Git(origin, "update-ref", "refs/heads/other", repo.A); var head = await Git(repo.Path, "rev-parse", "HEAD");
            shell.DefaultFetchCommand.Execute(null); await shell.ActionTask;
            Expect(shell.CurrentAction?.Succeeded == true && interaction.Options == 2 && await Git(repo.Path, "rev-parse", "refs/remotes/origin/other") == repo.A &&
                await Git(repo.Path, "rev-parse", "HEAD") == head, "Default Fetch updates refs from branch remote without moving HEAD");
            shell.ShowChangesCommand.Execute(null); await shell.CommitDialogTask;
            Expect(interaction.Commits == 1, "Commit still opens its workspace");
        });
        await Check("Default Push resolves simple/upstream targets and refuses matching scope locally", async () =>
        {
            var repo = await Repo("push defaults"); var remote = Fixture("push defaults.git");
            await Git(remote, "init", "--bare", "-b", "main"); await Git(repo.Path, "remote", "add", "origin", remote);
            await Git(repo.Path, "push", "-u", "origin", "main");
            var options = await actions.ResolveDefaultNetworkAsync(await actions.PrepareAsync(git, repo.Path, default), new(GitAction.Push), default);
            Expect(options.Remote == "origin" && options.RemoteBranch == "main" && !options.SetUpstream && !options.AllTags, "Simple uses matching upstream only");
            await Git(repo.Path, "config", "push.default", "upstream"); await Git(repo.Path, "config", "branch.main.merge", "refs/heads/destination");
            options = await actions.ResolveDefaultNetworkAsync(await actions.PrepareAsync(git, repo.Path, default), new(GitAction.Push), default);
            Expect(options.RemoteBranch == "destination", "Upstream preserves server branch spelling");
            await Git(repo.Path, "config", "push.default", "matching");
            await Reject("DefaultPushNeedsTarget", async () => await actions.ResolveDefaultNetworkAsync(await actions.PrepareAsync(git, repo.Path, default), new(GitAction.Push), default));
        });
    }
    private sealed class HistoryInteraction : IUserInteraction
    {
        public string Clipboard = "", Confirmation = "";
        public int Options, Commits;
        public string? ChooseRepository() => null;
        public string? ChooseGit() => null;
        public void CopyFilePaths(string text) => Clipboard = text;
        public bool ConfirmRecovery(string message, string title) { Confirmation = message; return false; }
        public void ShowGitAction(GitActionViewModel model) => Options++;
        public Task ShowCommitAsync(CommitViewModel model) { Commits++; return Task.CompletedTask; }
    }
    private sealed class HistoryActionRunner : IProcessRunner
    {
        private readonly WindowsProcessRunner _inner = new();
        public int Calls;
        public bool PauseCheckout;
        public TaskCompletionSource Started = new(TaskCreationOptions.RunContinuationsAsynchronously);
        public async Task<ProcessResult> RunAsync(ProcessRequest request, CancellationToken token, Action<ProcessOutput>? output = null)
        {
            Interlocked.Increment(ref Calls);
            if (PauseCheckout && request.Arguments.Contains("switch")) { Started.TrySetResult(); await Task.Delay(Timeout.Infinite, token); }
            return await _inner.RunAsync(request, token, output);
        }
    }
}
