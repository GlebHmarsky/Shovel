using System.IO;
using System.Text;
using System.Windows;
using System.Windows.Automation;
using System.Windows.Controls;
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
    private static bool _defaultPullChecks;
    private static async Task RunDefaultPullChecksAsync()
    {
        var runner = new DefaultPullRunner(); var reader = new RepositoryReader(runner);
        var identities = new WindowsRepositoryIdentity(); var git = await reader.DiscoverAsync(null, default);
        var repo = Fixture("default pull 日本語"); var remote = Fixture("default remote.git"); var peer = Fixture("default peer");
        async Task<string> Git(string folder, params string[] args)
        {
            var result = await runner.RunAsync(new(git.Executable, folder, args), default);
            Expect(result.ExitCode == 0, Encoding.UTF8.GetString(result.StandardError)); return Encoding.UTF8.GetString(result.StandardOutput).Trim();
        }
        await Git(remote, "init", "--bare", "-b", "main"); await Git(repo, "init", "-b", "main");
        foreach (var folder in new[] { repo, peer })
        {
            if (folder == peer) await Git(peer, "clone", remote, ".");
            await Git(folder, "config", "user.name", "Pull Fixture"); await Git(folder, "config", "user.email", "pull@example.invalid");
        }
        await File.WriteAllTextAsync(Path.Combine(repo, "base.txt"), "base\n");
        await Git(repo, "add", "."); await Git(repo, "commit", "-m", "Base"); await Git(repo, "remote", "add", "origin", remote);
        await Git(repo, "push", "-u", "origin", "main"); await Git(peer, "pull", "origin", "main");
        var interaction = new ActionInteraction(); var opened = 0;
        interaction.Opening = _ => opened++;
        var shell = new ShellViewModel(reader, interaction, identities, actions: new GitActions(runner, reader, identities));
        shell.CompleteStartup(new(new() { RepositoryPaths = [repo] }, SettingsLoadStatus.Loaded)); await shell.WaitForIdleAsync();
        await Check("Pull split controls bind distinct commands and options open without Git preparation", async () =>
        {
            var calls = runner.Calls;
            interaction.Show = model => { Expect(model.Rebase && !model.AutoStash, "Existing dialog defaults preserved"); new GitActionWindow(model).Close(); };
            shell.PullCommand.Execute(null); await shell.ActionTask;
            Expect(opened == 1 && runner.Calls == calls && !shell.HasAction, "Arrow only opens options; no action/read");
            var window = new MainWindow(shell);
            try
            {
                var content = (FrameworkElement)window.Content;
                foreach (var theme in new[] { AppAppearance.Dark, AppAppearance.Light })
                {
                    AppTheme.Apply(Application.Current.Resources, false, theme);
                    foreach (var size in new[] { (1200, 800), (900, 620) })
                    {
                        Arrange(content, size.Item1, size.Item2); await Dispatcher.Yield(DispatcherPriority.ContextIdle);
                        var buttons = VisualDescendants<Button>(content).ToArray();
                        var direct = buttons.Single(b => AutomationProperties.GetAutomationId(b) == "DefaultPull");
                        var options = buttons.Single(b => AutomationProperties.GetAutomationId(b) == "PullOptions");
                        Expect(direct.Command == shell.DefaultPullCommand && options.Command == shell.PullCommand && direct.IsEnabled && options.IsEnabled,
                            "Both accessible controls bind working commands");
                        Expect(options.TranslatePoint(new Point(options.ActualWidth, 0), content).X < size.Item1 && options.ActualWidth >= 22, "Arrow fits the toolbar");
                        SaveRender(content, $"default-pull-{theme}-{size.Item1}.png", size.Item1, size.Item2);
                    }
                }
            }
            finally { window.Close(); AppTheme.Apply(Application.Current.Resources, false); }
        });
        await Check("One click rebases real divergent history with no options dialog and shared progress", async () =>
        {
            await File.WriteAllTextAsync(Path.Combine(repo, "local.txt"), "local\n"); await Git(repo, "add", "."); await Git(repo, "commit", "-m", "Local");
            await File.WriteAllTextAsync(Path.Combine(peer, "remote.txt"), "remote\n"); await Git(peer, "add", "."); await Git(peer, "commit", "-m", "Remote"); await Git(peer, "push", "origin", "main");
            var remoteTip = await Git(peer, "rev-parse", "HEAD");
            shell.RefreshRepositoryCommand.Execute(null); await shell.WaitForIdleAsync();
            runner.Pause(); var previousOpened = opened; var fetches = runner.Fetches;
            shell.DefaultPullCommand.Execute(null); await runner.Started.Task.WaitAsync(TimeSpan.FromSeconds(15));
            Expect(opened == previousOpened && shell.IsActionRunning && shell.ActionOpen && shell.CancelCommand.CanExecute(null), "No modal; shared action and cancellation active");
            Expect(shell.CurrentAction is { Rebase: true, AutoStash: false }, "Main button uses fixed existing defaults");
            shell.DefaultPullCommand.Execute(null); shell.PullCommand.Execute(null);
            Expect(opened == previousOpened && runner.Fetches == fetches + 1, "Duplicate commands blocked");
            runner.Release.TrySetResult(); await shell.ActionTask;
            Expect(shell.CurrentAction?.Succeeded == true && !shell.ActionOpen && shell.CurrentAction.RawOutput.Length > 0, "Completed result/output retained");
            Expect(await Git(repo, "rev-parse", "HEAD^") == remoteTip && File.Exists(Path.Combine(repo, "local.txt")) && File.Exists(Path.Combine(repo, "remote.txt")), "Remote changes imported and local commit replayed");
            Expect(shell.Commits.First().Commit.Hash == await Git(repo, "rev-parse", "HEAD"), "History reconciles actual result");
        });
        await Check("Default Pull rejects changed upstream, explains missing tracking and drains cancellation", async () =>
        {
            // The local tracking ref stays origin/main even though the real server-side
            // source changes. A matching cached ref suffix alone must not authorize Pull.
            await Git(repo, "config", "branch.main.merge", "refs/heads/other");
            await Git(repo, "config", "remote.origin.fetch", "+refs/heads/other:refs/remotes/origin/main"); var fetches = runner.Fetches;
            shell.DefaultPullCommand.Execute(null); await shell.ActionTask;
            Expect(shell.CurrentAction?.Status == UiText.Get("PullDefaultTargetChanged") && runner.Fetches == fetches, "Changed server mapping rejected before fetching");
            await Git(repo, "config", "remote.origin.fetch", "+refs/heads/*:refs/remotes/origin/*");
            await Git(repo, "branch", "--unset-upstream"); shell.RefreshRepositoryCommand.Execute(null); await shell.WaitForIdleAsync();
            Expect(!shell.DefaultPullCommand.CanExecute(null) && shell.PullCommand.CanExecute(null) && shell.DefaultPullHelp == UiText.Get("PullDefaultNeedsUpstream"), "No guessing; options remain available");
            await Git(repo, "branch", "--set-upstream-to=origin/main"); shell.RefreshRepositoryCommand.Execute(null); await shell.WaitForIdleAsync();
            var tip = await Git(repo, "rev-parse", "HEAD"); runner.Pause();
            shell.DefaultPullCommand.Execute(null); await runner.Started.Task.WaitAsync(TimeSpan.FromSeconds(15)); shell.CancelCommand.Execute(null);
            await shell.ActionTask;
            Expect(shell.CurrentAction?.Status == UiText.Get("ActionResultCancelled") && !shell.ActionOpen && await Git(repo, "rev-parse", "HEAD") == tip, "Cancellation retains Git state and closes operation lifetime");
            runner.Release.TrySetResult();
        });
        await Check("Pull options still execute Merge with auto-stash while the default remains Rebase", async () =>
        {
            await File.WriteAllTextAsync(Path.Combine(peer, "second.txt"), "second\n"); await Git(peer, "add", "."); await Git(peer, "commit", "-m", "Second"); await Git(peer, "push", "origin", "main");
            await File.WriteAllTextAsync(Path.Combine(repo, "base.txt"), "my dirty edit\n");
            var closed = false;
            interaction.Show = model =>
            {
                model.Merge = true; model.AutoStash = true;
                var window = new GitActionWindow(model); window.Closed += (_, _) => closed = true;
                model.RunCommand.Execute(null);
            };
            shell.PullCommand.Execute(null); await shell.ActionTask;
            Expect(closed && shell.CurrentAction?.Succeeded == true && shell.CurrentAction.Merge && shell.CurrentAction.AutoStash, "Existing modal submits chosen settings and closes");
            Expect((await Git(repo, "rev-list", "--parents", "-n", "1", "HEAD")).Split(' ').Length == 3 &&
                await File.ReadAllTextAsync(Path.Combine(repo, "base.txt")) == "my dirty edit\n", "Real merge and dirty edit reapplied");
            var previousOpened = opened; shell.DefaultPullCommand.Execute(null); await shell.ActionTask;
            Expect(opened == previousOpened && shell.CurrentAction is { Rebase: true, AutoStash: false, Succeeded: false }, "Modal choices are not persisted as new defaults; dirty rebase rejection retained");
            Expect(shell.Commits.Count > 0 && await File.ReadAllTextAsync(Path.Combine(repo, "base.txt")) == "my dirty edit\n", "Failed default Pull retains history and edits");
        });
    }
    private sealed class DefaultPullRunner : IProcessRunner
    {
        private readonly WindowsProcessRunner _inner = new();
        public int Calls, Fetches;
        public TaskCompletionSource Started { get; private set; } = new(TaskCreationOptions.RunContinuationsAsynchronously);
        public TaskCompletionSource Release { get; private set; } = Completed();
        private static TaskCompletionSource Completed() { var result = new TaskCompletionSource(); result.SetResult(); return result; }
        public void Pause() { Started = new(TaskCreationOptions.RunContinuationsAsynchronously); Release = new(TaskCreationOptions.RunContinuationsAsynchronously); }
        public async Task<ProcessResult> RunAsync(ProcessRequest request, CancellationToken token, Action<ProcessOutput>? output = null)
        {
            Interlocked.Increment(ref Calls);
            if (request.Arguments.Contains("fetch")) { Interlocked.Increment(ref Fetches); Started.TrySetResult(); await Release.Task.WaitAsync(token); }
            return await _inner.RunAsync(request, token, output);
        }
    }
}
