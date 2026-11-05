using System.IO;
using System.Text;
using GitGui.App.ViewModels;
using GitGui.Core.Git;
using GitGui.Core.Processes;
using GitGui.Core.Settings;
using GitGui.ProcessHost;

namespace GitGui.FoundationChecks;
internal static partial class Program
{
    private static bool _operationLifetimeChecks;
    private static async Task RunOperationLifetimeChecksAsync()
    {
        var runner = new LifetimeRunner(); var reader = new RepositoryReader(runner); var ids = new WindowsRepositoryIdentity();
        var actions = new GitActions(runner, reader, ids); var ui = new RepositoryWorkflowInteraction();
        var git = await reader.DiscoverAsync(null, default); var native = new WindowsProcessRunner();
        async Task<string> Git(string path, params string[] arguments)
        {
            var result = await native.RunAsync(new(git.Executable, path, arguments), default);
            Expect(result.ExitCode == 0, Encoding.UTF8.GetString(result.StandardError));
            return Encoding.UTF8.GetString(result.StandardOutput).Trim();
        }
        var a = Fixture("lifetime-a"); var b = Fixture("lifetime-b");
        foreach (var path in new[] { a, b })
        {
            await Git(path, "init", "-b", "main"); await Git(path, "config", "user.name", "Fixture"); await Git(path, "config", "user.email", "fixture@example.invalid");
            await File.WriteAllTextAsync(Path.Combine(path, "file.txt"), "base\n"); await Git(path, "add", "."); await Git(path, "commit", "-m", "base");
        }
        var remote = Fixture("lifetime-remote"); await Git(remote, "init", "--bare");
        await Git(a, "remote", "add", "origin", remote); await Git(a, "push", "-u", "origin", "main");
        var shell = new ShellViewModel(reader, ui, ids, actions: actions);
        try
        {
            await Check("Commit opens during initial history loading without cancelling repository initialization", async () =>
            {
                runner.Hold(a, "log");
                shell.CompleteStartup(new(new() { RepositoryPaths = [a, b] }, SettingsLoadStatus.Loaded));
                await runner.Entered.Task.WaitAsync(TimeSpan.FromSeconds(8));
                var opened = new TaskCompletionSource<CommitViewModel>(); var close = new TaskCompletionSource();
                ui.CommitHandler = model => { opened.SetResult(model); return close.Task; };
                shell.ShowChangesCommand.Execute(null); var editor = await opened.Task.WaitAsync(TimeSpan.FromSeconds(2));
                await editor.StartLoading();
                Expect(!runner.Cancelled && !runner.Release.Task.IsCompleted, "Editor leaves initial log running");
                Expect(editor.Ready, "Local changes are available independently of history");
                runner.Release.SetResult(); await shell.WaitForIdleAsync();
                Expect(shell.Commits.Count == 1, "History publishes while modal editor exists");
                close.SetResult(); await shell.CommitDialogTask; ui.CommitHandler = null;
            });
            await Check("Switching and closing a loading tab keep its initialization alive and reconnect on reopen", async () =>
            {
                var first = shell.Repositories[0]; var second = shell.Repositories[1];
                runner.Hold(a, "log"); shell.RefreshRepositoryCommand.Execute(null); var refresh = shell.ActionTask;
                await runner.Entered.Task.WaitAsync(TimeSpan.FromSeconds(8));
                shell.SelectedRepository = second; await shell.WaitForIdleAsync();
                Expect(shell.Commits.Count == 1 && !runner.Cancelled, "Independent repository loads without waiting for the first");
                shell.CloseTab(first); Expect(!shell.Repositories.Contains(first), "Busy tab closes immediately");
                ui.RepositoryChoice = a; shell.OpenRepositoryCommand.Execute(null);
                Expect(ReferenceEquals(shell.SelectedRepository, first) && !runner.Cancelled, "Reopen attaches to original owner");
                runner.Release.SetResult(); await refresh; await shell.WaitForIdleAsync();
                Expect(shell.Commits.Count == 1, "Retained initialization publishes successfully");
                Expect(shell.Journal.Entries.Where(entry => entry.Title == GitGui.App.Localization.UiText.Get("OperationHistoryTitle")).All(entry => !entry.Running && entry.Background), "Each repository's history attempt settles in Background activity");
            });
            await Check("Submitted Commit closes its view, survives tab closure and records actual committed content", async () =>
            {
                await File.WriteAllTextAsync(Path.Combine(a, "background.txt"), "background commit\n"); await Git(a, "add", "background.txt");
                var opened = new TaskCompletionSource<CommitViewModel>(); var close = new TaskCompletionSource();
                ui.CommitHandler = model => { model.CommitSubmitted += (_, _) => close.TrySetResult(); opened.SetResult(model); return close.Task; };
                shell.ShowChangesCommand.Execute(null); var editor = await opened.Task; await editor.StartLoading(); editor.Subject = "background commit";
                runner.Hold(a, "commit"); editor.CommitCommand.Execute(null);
                await runner.Entered.Task.WaitAsync(TimeSpan.FromSeconds(8)); await shell.CommitDialogTask;
                Expect(shell.CanNavigate && shell.HasUserOperation && !editor.ViewAttached, "Window closes but execution still guards app exit");
                var tab = shell.SelectedRepository!; shell.CloseTab(tab);
                Expect(!shell.Repositories.Contains(tab) && !runner.Cancelled, "Closing repository does not cancel commit");
                runner.Release.SetResult(); await editor.OperationTask;
                Expect(await Git(a, "log", "-1", "--format=%s") == "background commit", "Commit finishes on original repository");
                Expect(await Git(a, "show", "HEAD:background.txt") == "background commit" && editor.Subject.Length == 0, "Exact submitted content and successful draft clearing");
                ui.RepositoryChoice = a; shell.OpenRepositoryCommand.Execute(null); await shell.WaitForIdleAsync(); ui.CommitHandler = null;
            });
            await Check("Failed detached commit preserves the message and actual index and offers repository-specific reopening", async () =>
            {
                await File.WriteAllTextAsync(Path.Combine(a, "rejected.txt"), "keep staged\n"); await Git(a, "add", "rejected.txt");
                var index = await Git(a, "write-tree"); var head = await Git(a, "rev-parse", "HEAD");
                var hook = Path.Combine(a, ".git", "hooks", "pre-commit"); await File.WriteAllTextAsync(hook, "#!/bin/sh\necho fixture-rejected >&2\nexit 1\n", new UTF8Encoding(false));
                var opened = new TaskCompletionSource<CommitViewModel>(); var close = new TaskCompletionSource();
                ui.CommitHandler = model => { model.CommitSubmitted += (_, _) => close.TrySetResult(); opened.SetResult(model); return close.Task; };
                try
                {
                    shell.ShowChangesCommand.Execute(null); var editor = await opened.Task; await editor.StartLoading(); editor.Subject = "keep this message";
                    editor.CommitCommand.Execute(null); await shell.CommitDialogTask; await editor.OperationTask;
                    Expect(editor.Subject == "keep this message" && await Git(a, "write-tree") == index && await Git(a, "rev-parse", "HEAD") == head, "Rejected commit retains draft/index/HEAD");
                    Expect(ui.Problems.Any(problem => problem.Repository == a && problem.Output.Contains("fixture-rejected")), "Background failure is visible with actual hook output");
                    Expect(shell.Journal.Entries.Any(entry => entry.Repository == a && entry.ReopenCommit is not null), "Activity offers reopening the failed repository's editor");
                }
                finally { File.Delete(hook); ui.CommitHandler = null; }
            });
            await Check("Individual waiting requests can be removed while manual Fetch remains uncancellable", async () =>
            {
                runner.Hold(a, "fetch"); shell.DefaultFetchCommand.Execute(null); var fetch = shell.ActionTask;
                await runner.Entered.Task.WaitAsync(TimeSpan.FromSeconds(8));
                shell.DefaultPullCommand.Execute(null); var pull = shell.ActionTask;
                shell.DefaultPushCommand.Execute(null); var push = shell.ActionTask;
                var entry = shell.Journal.Entries.First(item => item.CanRemoveWaiting && item.Title == GitGui.App.Localization.UiText.Get("ActionTitlePull"));
                shell.Journal.Selected = entry;
                var activity = new GitGui.App.Views.ActivityLogView { DataContext = shell.Journal };
                Arrange(activity, 950, 400); await System.Windows.Threading.Dispatcher.Yield(System.Windows.Threading.DispatcherPriority.ContextIdle);
                var remove = VisualDescendants<System.Windows.Controls.Button>(activity).Single(button => ReferenceEquals(button.Command, entry.CancelWaiting));
                Expect(remove.Visibility == System.Windows.Visibility.Visible && remove.IsEnabled, "Waiting Activity selection exposes its bound removal action");
                SaveRender(activity, "waiting-request-activity.png", 950, 400);
                remove.Command.Execute(null); await pull;
                Expect(!push.IsCompleted && !fetch.IsCompleted && shell.CurrentAction?.CanCancel == false, "Removing one waiting request preserves the others and active Fetch");
                runner.Release.SetResult(); await Task.WhenAll(fetch, push); await shell.WaitForIdleAsync();
                Expect(!runner.Cancelled, "Manual Fetch was never cancelled");
            });
            await Check("Failure clears that repository's waiting requests with an explanation", async () =>
            {
                runner.Hold(a, "fetch", fail: true); shell.DefaultFetchCommand.Execute(null); var fetch = shell.ActionTask;
                await runner.Entered.Task.WaitAsync(TimeSpan.FromSeconds(8)); shell.DefaultPushCommand.Execute(null); var push = shell.ActionTask;
                var calls = runner.PushCalls; runner.Release.SetResult(); await Task.WhenAll(fetch, push);
                Expect(runner.PushCalls == calls && shell.Journal.Entries.Any(entry => entry.Status == GitGui.App.Localization.UiText.Get("QueueStoppedAfterFailure")), "Queued Push never starts after failed Fetch");
            });
            await Check("Distinct staging requests and Commit queue in order and commit the submitted files", async () =>
            {
                shell.SelectedRepository = shell.Repositories.Single(tab => tab.Directory == b); await shell.WaitForIdleAsync();
                await File.WriteAllTextAsync(Path.Combine(b, "queued-one.txt"), "one\n"); await File.WriteAllTextAsync(Path.Combine(b, "queued-two.txt"), "two\n");
                var opened = new TaskCompletionSource<CommitViewModel>(); var close = new TaskCompletionSource();
                ui.CommitHandler = model => { model.CommitSubmitted += (_, _) => close.TrySetResult(); opened.SetResult(model); return close.Task; };
                shell.ShowChangesCommand.Execute(null); var editor = await opened.Task; await editor.StartLoading();
                editor.Select(false, [editor.Unstaged.Single(row => row.Path == "queued-one.txt")]);
                runner.Hold(b, "add"); editor.StageCommand.Execute(null); await runner.Entered.Task.WaitAsync(TimeSpan.FromSeconds(8));
                editor.Select(false, [editor.Unstaged.Single(row => row.Path == "queued-two.txt")]);
                Expect(editor.StageCommand.CanExecute(null), "A separate staging request remains available"); editor.StageCommand.Execute(null);
                editor.Subject = "queued commit"; Expect(editor.CommitCommand.CanExecute(null), "Commit can await accepted staging"); editor.CommitCommand.Execute(null);
                await shell.CommitDialogTask; Expect(!editor.OperationTask.IsCompleted, "Editor closes while its queue is still running");
                runner.Release.SetResult(); await editor.OperationTask.WaitAsync(TimeSpan.FromSeconds(20));
                Expect(await Git(b, "log", "-1", "--format=%s") == "queued commit" && await Git(b, "show", "HEAD:queued-two.txt") == "two", "Commit contains both queued staging results");
                await shell.WaitForIdleAsync(); ui.CommitHandler = null;
            });
            await Check("Removing a waiting Commit preserves its draft and lets accepted staging finish", async () =>
            {
                var head = await Git(b, "rev-parse", "HEAD"); await File.WriteAllTextAsync(Path.Combine(b, "waiting.txt"), "preserve staged\n");
                var opened = new TaskCompletionSource<CommitViewModel>(); var close = new TaskCompletionSource();
                ui.CommitHandler = model => { model.CommitSubmitted += (_, _) => close.TrySetResult(); opened.SetResult(model); return close.Task; };
                shell.ShowChangesCommand.Execute(null); var editor = await opened.Task; await editor.StartLoading(); editor.Subject = "preserve waiting draft";
                editor.Select(false, [editor.Unstaged.Single(row => row.Path == "waiting.txt")]); runner.Hold(b, "add"); editor.StageCommand.Execute(null);
                await runner.Entered.Task.WaitAsync(TimeSpan.FromSeconds(8)); editor.CommitCommand.Execute(null); await shell.CommitDialogTask;
                shell.Journal.Entries.First(entry => entry.CanRemoveWaiting && entry.Title == GitGui.App.Localization.UiText.Get("Commit")).CancelWaiting!.Execute(null);
                runner.Release.SetResult(); await editor.OperationTask; await shell.WaitForIdleAsync();
                Expect(await Git(b, "rev-parse", "HEAD") == head && await Git(b, "show", ":waiting.txt") == "preserve staged" && editor.Subject == "preserve waiting draft", "Waiting cancellation changes neither HEAD, completed staging nor message");
                ui.CommitHandler = null;
            });
            await Check("Closing Stashes leaves an accepted stash command running and records its result", async () =>
            {
                var opened = new TaskCompletionSource<StashViewModel>(); var close = new TaskCompletionSource();
                ui.StashHandler = model => { opened.SetResult(model); return close.Task; };
                shell.StashesCommand.Execute(null); var model = await opened.Task; await model.StartAsync(); model.Message = "background stash";
                runner.Hold(b, "stash"); model.CreateCommand.Execute(null); await runner.Entered.Task.WaitAsync(TimeSpan.FromSeconds(8));
                close.SetResult(); await System.Windows.Threading.Dispatcher.Yield();
                Expect(shell.HasUserOperation && !runner.Cancelled, "Accepted stash still guards shutdown after closing its view");
                runner.Release.SetResult(); await shell.StashDialogTask; await model.Running;
                Expect((await Git(b, "stash", "list")).Contains("background stash") && !runner.Cancelled, "Stash completes after its window closes"); ui.StashHandler = null;
            });
            await Check("Returning from a blocked shutdown reopens a usable editor with its saved draft", async () =>
            {
                shell.BeginShutdown(); await shell.WaitForIdleAsync(); shell.ResumeAfterSaveFailure();
                var opened = new TaskCompletionSource<CommitViewModel>(); var close = new TaskCompletionSource();
                ui.CommitHandler = model => { opened.SetResult(model); return close.Task; };
                shell.ShowChangesCommand.Execute(null); var editor = await opened.Task; await editor.StartLoading(); await editor.DraftLoadingTask;
                Expect(editor.Ready && editor.CanEditMessage && editor.Subject == "preserve waiting draft", "Resumed shell replaces closed writers and restores the draft");
                close.SetResult(); await shell.CommitDialogTask; ui.CommitHandler = null;
            });
            await Check("Started native user commands finish despite timeout and cancellation while read processes remain bounded", async () =>
            {
                using var cancel = new CancellationTokenSource(); var started = new TaskCompletionSource();
                using var lifetime = ProcessLifetime.CompleteStartedCommands();
                var run = new WindowsProcessRunner().RunAsync(new(ProbeExecutable, a, ["--probe", "finish-started"]) { Timeout = TimeSpan.FromMilliseconds(150) }, cancel.Token,
                    output => { if (Encoding.UTF8.GetString(output.Bytes).Contains("started")) started.TrySetResult(); });
                await started.Task.WaitAsync(TimeSpan.FromSeconds(3)); cancel.Cancel(); var result = await run;
                Expect(result.Outcome == ProcessOutcome.Exited && result.ExitCode == 0 && Encoding.UTF8.GetString(result.StandardOutput).Contains("finished"), "Started process is not killed by deadline or cancellation");
                using var readLifetime = ProcessLifetime.CompleteStartedCommands(false);
                var read = await new WindowsProcessRunner().RunAsync(new(ProbeExecutable, a, ["--probe", "finish-started"]) { Timeout = TimeSpan.FromMilliseconds(150) }, default);
                Expect(read.Outcome == ProcessOutcome.TimedOut, "Obsolete read processes retain bounded cancellation");
            });
        }
        finally { runner.Release.TrySetResult(); shell.BeginShutdown(); await shell.WaitForIdleAsync(); }
    }
    private sealed class LifetimeRunner : IProcessRunner
    {
        private readonly WindowsProcessRunner _inner = new();
        private string _path = "", _command = ""; private bool _held, _fail;
        public bool Cancelled { get; private set; }
        public int PushCalls { get; private set; }
        public TaskCompletionSource Entered { get; private set; } = new(TaskCreationOptions.RunContinuationsAsynchronously);
        public TaskCompletionSource Release { get; private set; } = new(TaskCreationOptions.RunContinuationsAsynchronously);
        public void Hold(string path, string command, bool fail = false)
        { _path = path; _command = command; _held = false; _fail = fail; Cancelled = false; Entered = new(TaskCreationOptions.RunContinuationsAsynchronously); Release = new(TaskCreationOptions.RunContinuationsAsynchronously); }
        public async Task<ProcessResult> RunAsync(ProcessRequest request, CancellationToken token, Action<ProcessOutput>? output = null)
        {
            if (request.Arguments.Contains("push")) PushCalls++;
            if (!_held && request.WorkingDirectory == _path && request.Arguments.Contains(_command))
            {
                _held = true; Entered.TrySetResult();
                try { await Release.Task.WaitAsync(token); } catch (OperationCanceledException) { Cancelled = true; throw; }
                if (_fail) return new(ProcessOutcome.Exited, 1, [], Encoding.UTF8.GetBytes("fatal: fixture unavailable"), TimeSpan.Zero);
            }
            return await _inner.RunAsync(request, token, output);
        }
    }
}
