using System.IO;
using System.Text;
using System.Windows;
using System.Windows.Controls;
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
    private static bool _workspaceStateChecks;
    private static async Task RunWorkspaceStateChecksAsync()
    {
        var runner = new WorkspaceStateRunner(); var reader = new RepositoryReader(runner);
        var ids = new WindowsRepositoryIdentity(); var actions = new GitActions(runner, reader, ids);
        var git = await reader.DiscoverAsync(null, default);
        async Task<string> Git(string path, params string[] arguments)
        {
            var result = await runner.RunAsync(new(git.Executable, path, arguments), default);
            Expect(result.ExitCode == 0, Encoding.UTF8.GetString(result.StandardError));
            return Encoding.UTF8.GetString(result.StandardOutput).Trim();
        }
        async Task<string> Repo(string name)
        {
            var path = Fixture("state-" + name);
            await Git(path, "init", "-b", "main"); await Git(path, "config", "user.name", "Fixture");
            await Git(path, "config", "user.email", "fixture@example.invalid"); await Git(path, "config", "core.autocrlf", "false");
            await File.WriteAllTextAsync(Path.Combine(path, "file.txt"), "base\n");
            await Git(path, "add", "."); await Git(path, "commit", "-m", "base");
            await Git(path, "branch", "other"); return path;
        }
        async Task<ActionOptions> Checkout(string path, CheckoutChanges choice, string branch = "other") => new ActionOptions(GitAction.SwitchBranch, Branch: branch)
        { CheckoutChanges = choice, CheckoutReview = await actions.ReadWorkingCopyAsync(git, path, default) };
        async Task Reject(string code, Func<Task> run)
        {
            try { await run(); throw new InvalidOperationException("Expected " + code); }
            catch (GitActionException error) { Expect(error.Code == code, error.Code); }
        }

        await Check("Index stat refresh accepts commit; genuine staged content and intent flags remain protected", async () =>
        {
            var path = await Repo("index");
            await File.WriteAllTextAsync(Path.Combine(path, "staged.txt"), "staged\n"); await Git(path, "add", ".");
            var reviewed = await actions.ReadWorkingCopyAsync(git, path, default);
            var raw = await RepositoryReader.IndexFileStampAsync(reviewed.Repository, default);
            File.SetLastWriteTimeUtc(Path.Combine(path, "file.txt"), DateTime.UtcNow.AddSeconds(-30));
            await Git(path, "update-index", "--refresh");
            Expect(raw != await RepositoryReader.IndexFileStampAsync(reviewed.Repository, default), "Fixture rewrites cache bytes");
            Expect(reviewed.IndexStamp == await actions.IndexStampAsync(git, reviewed.Repository, default), "Same entries survive stat refresh");
            var committed = await actions.ExecuteWorkingCopyAsync(reviewed, new(WorkingCopyAction.Commit, [], "cache refresh is harmless"), default);
            Expect(committed.Result.Success, committed.Result.Output);
            reviewed = await actions.ReadWorkingCopyAsync(git, path, default);
            await File.WriteAllTextAsync(Path.Combine(path, "staged.txt"), "external staging\n"); await Git(path, "add", ".");
            await Reject("CommitStale", () => actions.ExecuteWorkingCopyAsync(reviewed, new(WorkingCopyAction.Commit, [], "must reject"), default));
            await File.WriteAllTextAsync(Path.Combine(path, "empty.txt"), ""); await Git(path, "add", "-N", "empty.txt");
            var intent = await actions.IndexStampAsync(git, reviewed.Repository, default);
            await Git(path, "add", "empty.txt");
            Expect(intent != await actions.IndexStampAsync(git, reviewed.Repository, default), "Intent-to-add differs from a staged empty blob");
        });
        await Check("Native index fingerprints support v4, split index and persistent worktree flags", async () =>
        {
            var path = await Repo("index-formats"); var snapshot = await reader.InspectAsync(git, path, default);
            var original = await actions.IndexStampAsync(git, snapshot, default);
            await Git(path, "update-index", "--index-version=4"); await Git(path, "update-index", "--split-index");
            Expect(original == await actions.IndexStampAsync(git, snapshot, default), "Storage encoding is not staged content");
            await Git(path, "update-index", "--assume-unchanged", "file.txt");
            Expect(original != await actions.IndexStampAsync(git, snapshot, default), "Assume-unchanged remains observable");
            await Git(path, "update-index", "--no-assume-unchanged", "file.txt"); await Git(path, "update-index", "--skip-worktree", "file.txt");
            Expect(original != await actions.IndexStampAsync(git, snapshot, default), "Skip-worktree remains observable");
        });
        await Check("Checkout keep carries local edits and runs the configured checkout hook", async () =>
        {
            var path = await Repo("keep"); await File.WriteAllTextAsync(Path.Combine(path, "file.txt"), "local\n");
            await File.WriteAllTextAsync(Path.Combine(path, ".git", "hooks", "post-checkout"), "#!/bin/sh\nprintf ran > checkout-hook.txt\n");
            var options = await Checkout(path, CheckoutChanges.Keep);
            var context = await actions.PrepareAsync(git, path, default);
            await Git(path, "branch", "unrelated-fetched-ref");
            var result = await actions.ExecuteAsync(context, options, default);
            Expect(result.Success && await Git(path, "branch", "--show-current") == "other", result.Output);
            Expect(await File.ReadAllTextAsync(Path.Combine(path, "file.txt")) == "local\n" && File.Exists(Path.Combine(path, "checkout-hook.txt")), "Local contents and configured hook survive unrelated ref refresh");
        });
        await Check("Checkout stash reapplies staged, unstaged and untracked contents without leaving a redundant stash", async () =>
        {
            var path = await Repo("stash"); await File.WriteAllTextAsync(Path.Combine(path, "file.txt"), "staged\n"); await Git(path, "add", ".");
            await File.WriteAllTextAsync(Path.Combine(path, "file.txt"), "unstaged\n"); await File.WriteAllTextAsync(Path.Combine(path, "new.txt"), "untracked\n");
            var options = await Checkout(path, CheckoutChanges.StashAndReapply);
            var result = await actions.ExecuteAsync(await actions.PrepareAsync(git, path, default), options, default);
            Expect(result.Success, result.Output);
            Expect(await Git(path, "show", ":file.txt") == "staged" && await File.ReadAllTextAsync(Path.Combine(path, "file.txt")) == "unstaged\n", "Index and worktree restored separately");
            Expect(File.Exists(Path.Combine(path, "new.txt")) && await Git(path, "stash", "list") == "", "Untracked file restored, created stash dropped");
        });
        await Check("Checkout stash conflicts retain recovery data; discard keeps unrelated untracked files", async () =>
        {
            var path = await Repo("conflict"); await Git(path, "switch", "other");
            await File.WriteAllTextAsync(Path.Combine(path, "file.txt"), "incoming\n"); await Git(path, "commit", "-am", "incoming"); await Git(path, "switch", "main");
            await File.WriteAllTextAsync(Path.Combine(path, "file.txt"), "local\n");
            var result = await actions.ExecuteAsync(await actions.PrepareAsync(git, path, default), await Checkout(path, CheckoutChanges.StashAndReapply), default);
            Expect(!result.Success && result.MessageCode == "CheckoutStashRetained" && await Git(path, "stash", "list") != "", "Conflicting application retains saved work");
            var discard = await Repo("discard"); await File.WriteAllTextAsync(Path.Combine(discard, "file.txt"), "discard me\n");
            await File.WriteAllTextAsync(Path.Combine(discard, "keep.txt"), "untracked\n");
            result = await actions.ExecuteAsync(await actions.PrepareAsync(git, discard, default), await Checkout(discard, CheckoutChanges.DiscardTracked), default);
            Expect(result.Success && await File.ReadAllTextAsync(Path.Combine(discard, "file.txt")) == "base\n" && File.Exists(Path.Combine(discard, "keep.txt")), result.Output);
        });
        await Check("Checkout rejects newly staged work after confirmation and hides dirty choices for clean trees", async () =>
        {
            var path = await Repo("review");
            using var clean = new CheckoutViewModel(path, "other", false, token => actions.ReadWorkingCopyAsync(git, path, token));
            var window = new CheckoutWindow(clean); await clean.StartAsync();
            Expect(clean.CanConfirm && !clean.HasChanges, "Clean confirmation is available without dirty choices");
            var content = (FrameworkElement)window.Content; Arrange(content, 720, 300);
            Expect(VisualDescendants<RadioButton>(content).All(button => ((FrameworkElement)button.Parent).Visibility == Visibility.Collapsed), "Dirty radios hidden");
            SaveRender(content, "checkout-clean.png", 720, 300); window.Close();
            var options = await Checkout(path, CheckoutChanges.DiscardTracked);
            await File.WriteAllTextAsync(Path.Combine(path, "file.txt"), "new staging\n"); await Git(path, "add", ".");
            await Reject("CheckoutChanged", async () => await actions.ExecuteAsync(await actions.PrepareAsync(git, path, default), options, default));
            Expect(await Git(path, "branch", "--show-current") == "main" && await Git(path, "show", ":file.txt") == "new staging", "Rejected review leaves data and branch intact");
            using var dirty = new CheckoutViewModel(path, "other", false, token => actions.ReadWorkingCopyAsync(git, path, token));
            await dirty.StartAsync(); Expect(dirty.HasChanges && dirty.Keep, "Safe default with dirty choices");
            var dirtyWindow = new CheckoutWindow(dirty); Arrange((FrameworkElement)dirtyWindow.Content, 720, 420);
            SaveRender((FrameworkElement)dirtyWindow.Content, "checkout-dirty.png", 720, 420); dirtyWindow.Close();
        });
        await Check("Window geometry and star panes survive disk reload; empty-log attention stays compact", async () =>
        {
            var store = new WindowLayouts(); var model = new CommitViewModel("", null, reader, actions, new CommitInteraction());
            var first = new CommitWindow(model) { Width = 1500, Height = 900 };
            var files = (ColumnDefinition)first.FindName("FilesColumn"); var diff = (ColumnDefinition)first.FindName("DiffColumn");
            files.Width = new(.45, GridUnitType.Star); diff.Width = new(.55, GridUnitType.Star);
            Arrange((FrameworkElement)first.Content, 1500, 900); store.Capture(first);
            var settings = new SettingsStore(Fixture("saved-layout")); await settings.LoadAsync(default);
            await settings.SaveAsync(new() { WindowLayouts = store.Snapshot }, default);
            var restored = await new SettingsStore(Path.GetDirectoryName(settings.FilePath)!).LoadAsync(default);
            var reopenedStore = new WindowLayouts(); reopenedStore.Load(restored.Settings.WindowLayouts);
            var secondModel = new CommitViewModel("", null, reader, actions, new CommitInteraction());
            var second = new CommitWindow(secondModel); reopenedStore.Restore(second);
            Expect(second.Width == 1500 && second.Height == 900 && ((ColumnDefinition)second.FindName("FilesColumn")).Width == files.Width, "Actual persisted bounds and ratio restored");
            Expect(WindowCaption.GetCloseOnly(second), "Commit uses close-only native caption policy");
            var notice = new OperationProblemWindow(new("fixture", "Commit", "State changed", ""));
            Expect(notice.SizeToContent == SizeToContent.Height && notice.ResizeMode == ResizeMode.NoResize, "No empty resizable log box");
            Arrange((FrameworkElement)notice.Content, 800, 200); SaveRender((FrameworkElement)notice.Content, "compact-attention.png", 800, 200);
            notice.Close(); await model.CloseAsync(); await secondModel.CloseAsync(); first.Close(); second.Close();
        });
        var network = await Repo("network"); var remote = Fixture("state-remote"); await Git(remote, "init", "--bare");
        await Git(network, "remote", "add", "origin", remote); await Git(network, "push", "-u", "origin", "main");
        var interaction = new ActionInteraction(); var shell = new ShellViewModel(reader, interaction, ids, actions: actions);
        shell.CompleteStartup(new(new() { RepositoryPaths = [network] }, SettingsLoadStatus.Loaded)); await shell.WaitForIdleAsync();
        try
        {
            await Check("Pull arrow choices persist and the main button uses merge with autostash", async () =>
            {
                interaction.Show = model => { model.AutoStash = true; model.Strategy = PullStrategy.Merge; };
                shell.PullCommand.Execute(null); await shell.ActionTask;
                var preference = shell.CaptureSession(new()).NetworkActions.Single();
                Expect(preference.AutoStash && preference.Strategy == PullStrategy.Merge, "Closing options saves choices");
                await File.WriteAllTextAsync(Path.Combine(network, "file.txt"), "local\n"); runner.Commands.Clear();
                shell.DefaultPullCommand.Execute(null); await shell.ActionTask;
                Expect(shell.CurrentAction?.Succeeded == true, shell.CurrentAction?.Status ?? "No pull");
                Expect(runner.Commands.Any(args => args.Contains("merge") && args.Contains("--autostash")), "Main Pull uses persisted flags");
                Expect(await File.ReadAllTextAsync(Path.Combine(network, "file.txt")) == "local\n", "Local edits preserved");
            });
            await Check("Fetch and Push arrow flags reach direct commands and persist across settings reload", async () =>
            {
                await Git(network, "remote", "add", "backup", remote); shell.RefreshRepositoryCommand.Execute(null); await shell.ActionTask; await shell.WaitForIdleAsync();
                interaction.Show = model => model.AllRemotes = true; shell.FetchCommand.Execute(null); await shell.ActionTask;
                runner.Commands.Clear(); shell.DefaultFetchCommand.Execute(null); await shell.ActionTask;
                Expect(shell.CurrentAction?.Succeeded == true && runner.Commands.Count(args => args.Contains("fetch")) == 2, "Direct Fetch uses all remotes");
                await Git(network, "tag", "saved-option-tag");
                interaction.Show = model => { model.AllTags = true; model.SetUpstream = true; };
                shell.PushCommand.Execute(null); await shell.ActionTask;
                var settings = new SettingsStore(Fixture("network-preferences")); await settings.LoadAsync(default); await settings.SaveAsync(shell.CaptureSession(new()), default);
                var loaded = await new SettingsStore(Path.GetDirectoryName(settings.FilePath)!).LoadAsync(default);
                Expect(loaded.Settings.NetworkActions.Single(item => item.Action == GitAction.Push).AllTags, "Push flags survive disk");
                runner.Commands.Clear(); shell.DefaultPushCommand.Execute(null); await shell.ActionTask;
                Expect(shell.CurrentAction?.Succeeded == true && runner.Commands.Any(args => args.Contains("push") && args.Contains("--tags") && args.Contains("--set-upstream")), shell.CurrentAction?.Status ?? "No push");
                Expect(await Git(remote, "rev-parse", "refs/tags/saved-option-tag") == await Git(network, "rev-parse", "HEAD"), "Tag was published to disposable remote");
            });
            await Check("Explicit push destinations stay branch-bound while flags follow branch changes", async () =>
            {
                interaction.Show = model => { model.Remote = model.Remotes.Single(remote => remote.Name == "backup"); model.RemoteBranch = "chosen"; };
                shell.PushCommand.Execute(null); await shell.ActionTask;
                Expect(shell.CaptureSession(new()).NetworkActions.Single(item => item.Action == GitAction.Push).ExplicitTarget, "Changed destination is explicit");
                shell.DefaultPushCommand.Execute(null); await shell.ActionTask;
                Expect(shell.CurrentAction?.Succeeded == true && await Git(remote, "rev-parse", "refs/heads/chosen") == await Git(network, "rev-parse", "HEAD"), "Direct push uses chosen destination");
                await Git(network, "switch", "other"); await Git(network, "config", "push.default", "current");
                shell.RefreshRepositoryCommand.Execute(null); await shell.ActionTask; await shell.WaitForIdleAsync();
                runner.Commands.Clear(); shell.DefaultPushCommand.Execute(null); await shell.ActionTask;
                Expect(shell.CurrentAction?.Succeeded == true && runner.Commands.Any(args => args.Contains("push") && args.Contains("refs/heads/other:refs/heads/other") && args.Contains("--tags")),
                    "New branch follows Git's current destination and retains chosen flags");
            });
            await Check("Collapsed sidebar groups survive refresh, fetch and settings restoration", async () =>
            {
                var group = shell.SidebarNodes.Single(item => item.Label == UiText.Get("Stashes")); group.Expansion.IsExpanded = false;
                shell.RefreshRepositoryCommand.Execute(null); await shell.ActionTask; await shell.WaitForIdleAsync();
                Expect(!shell.SidebarNodes.Single(item => item.Label == UiText.Get("Stashes")).Expansion.IsExpanded, "Refresh does not reopen Stashes");
                shell.DefaultFetchCommand.Execute(null); await shell.ActionTask; await shell.WaitForIdleAsync();
                Expect(!shell.SidebarNodes.Single(item => item.Label == UiText.Get("Stashes")).Expansion.IsExpanded, "Fetch does not reopen Stashes");
                var reopened = new ShellViewModel(reader, new ActionInteraction(), ids, actions: actions);
                reopened.CompleteStartup(new(shell.CaptureSession(new()), SettingsLoadStatus.Loaded)); await reopened.WaitForIdleAsync();
                Expect(!reopened.SidebarNodes.Single(item => item.Label == UiText.Get("Stashes")).Expansion.IsExpanded, "Persisted sidebar state restored");
                reopened.BeginShutdown(); await reopened.WaitForIdleAsync();
            });
        }
        finally { shell.BeginShutdown(); await shell.WaitForIdleAsync(); }
    }
    private static bool ConfirmCheckoutForCheck(CheckoutViewModel model)
    {
        var task = model.StartAsync();
        if (!task.IsCompleted)
        {
            var frame = new System.Windows.Threading.DispatcherFrame();
            async Task Drain() { try { await task; } finally { frame.Continue = false; } }
            var drain = Drain(); System.Windows.Threading.Dispatcher.PushFrame(frame); drain.GetAwaiter().GetResult();
        }
        task.GetAwaiter().GetResult(); Expect(model.CanConfirm, model.Status); return true;
    }
    private sealed class WorkspaceStateRunner : IProcessRunner
    {
        private readonly WindowsProcessRunner _inner = new();
        public System.Collections.Concurrent.ConcurrentQueue<string[]> Commands { get; } = new();
        public Task<ProcessResult> RunAsync(ProcessRequest request, CancellationToken token, Action<ProcessOutput>? output = null)
        { Commands.Enqueue(request.Arguments.ToArray()); return _inner.RunAsync(request, token, output); }
    }
}
