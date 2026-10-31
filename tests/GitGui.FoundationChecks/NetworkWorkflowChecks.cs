using System.IO;
using System.Text;
using System.Windows;
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
    private static bool _networkWorkflowChecks;
    private static async Task RunNetworkWorkflowChecksAsync()
    {
        var runner = new NetworkWorkflowRunner(); var reader = new RepositoryReader(runner);
        var identities = new WindowsRepositoryIdentity(); var actions = new GitActions(runner, reader, identities);
        var git = await reader.DiscoverAsync(null, default); var root = Fixture("network workflows");
        string Folder(string name) => Path.Combine(root, name);
        async Task<string> Git(string path, params string[] arguments)
        {
            var result = await runner.Inner.RunAsync(new(git.Executable, path, arguments), default);
            Expect(result.Outcome == ProcessOutcome.Exited && result.ExitCode == 0, Encoding.UTF8.GetString(result.StandardError));
            return Encoding.UTF8.GetString(result.StandardOutput).Trim();
        }
        var source = Directory.CreateDirectory(Folder("source 日本語")).FullName;
        await Git(source, "init", "-b", "main");
        await File.WriteAllTextAsync(Path.Combine(source, "a.txt"), "base\n");
        await File.WriteAllTextAsync(Path.Combine(source, ".gitattributes"), "*.txt filter=fixture\n");
        await Git(source, "add", ".");
        await Git(source, "-c", "user.name=Fixture", "-c", "user.email=fixture@example.invalid", "commit", "-m", "first");
        await Git(source, "branch", "topic"); await Git(source, "tag", "v1");
        var head = await Git(source, "rev-parse", "HEAD");
        var clone = Folder("clone 日本語 - files");

        await Check("Native clone checks out files, runs filters and hooks, and configures origin/upstream", async () =>
        {
            var config = Environment.GetEnvironmentVariable("GIT_CONFIG_GLOBAL")!; var saved = await File.ReadAllTextAsync(config);
            try
            {
                var template = Directory.CreateDirectory(Folder("templates/hooks")).FullName;
                await File.WriteAllTextAsync(Path.Combine(template, "post-checkout"), "#!/bin/sh\nprintf ran > hook-marker\n", new UTF8Encoding(false));
                await Git(root, "config", "--file", config, "init.templateDir", Path.GetDirectoryName(template)!);
                await Git(root, "config", "--file", config, "filter.fixture.smudge", "sed s/base/hydrated/g");
                await Git(root, "config", "--file", config, "filter.fixture.clean", "sed s/hydrated/base/g");
                var result = await actions.CloneRepositoryAsync(git, new(source, clone), default);
                Expect(result.Action.Success && result.Repository?.Branch == "main", result.Action.ToString());
                Expect((await File.ReadAllTextAsync(Path.Combine(clone, "a.txt"))).Trim() == "hydrated", "Native smudge filter hydrated checkout");
                Expect(File.Exists(Path.Combine(clone, "hook-marker")), "Configured post-checkout hook ran");
                Expect(await Git(clone, "status", "--porcelain", "--", "a.txt") == "", "Native clean filter retains clean status");
                Expect(await Git(clone, "config", "--get", "remote.origin.url") == source && await Git(clone, "rev-parse", "--symbolic-full-name", "@{upstream}") == "refs/remotes/origin/main", "Clone has real origin and upstream");
                Expect(runner.CloneRequest!.Arguments.Contains("--progress") && !runner.CloneRequest.Environment.ContainsKey("GIT_LFS_SKIP_SMUDGE"), "Progress enabled without disabling LFS");
            }
            finally { await File.WriteAllTextAsync(config, saved); }
        });
        await Check("Clone supports an explicit branch, detached tag and empty remote", async () =>
        {
            var branch = await actions.CloneRepositoryAsync(git, new(source, Folder("branch"), "topic"), default);
            var tag = await actions.CloneRepositoryAsync(git, new(source, Folder("tag"), "v1"), default);
            var empty = Directory.CreateDirectory(Folder("empty.git")).FullName; await Git(empty, "init", "--bare", "-b", "main");
            var unborn = await actions.CloneRepositoryAsync(git, new(empty, Folder("unborn")), default);
            Expect(branch.Action.Success && branch.Repository?.Branch == "topic", "Requested branch checked out");
            Expect(tag.Action.Success && tag.Repository?.HeadKind == HeadKind.Detached, "Tag produces detached HEAD");
            Expect(unborn.Action.Success && unborn.Repository?.HeadKind == HeadKind.Unborn, "Empty remote opens as unborn");
        });
        await Check("Clone rejects occupied destinations and invalid inputs without altering files", async () =>
        {
            var occupied = Directory.CreateDirectory(Folder("occupied")).FullName;
            await File.WriteAllTextAsync(Path.Combine(occupied, "keep"), "untouched"); var calls = runner.CloneCalls;
            try { await actions.CloneRepositoryAsync(git, new(source, occupied), default); throw new InvalidOperationException("Occupied clone accepted"); }
            catch (GitActionException error) { Expect(error.Code == "CloneNotEmpty", error.Code); }
            Expect(runner.CloneCalls == calls && await File.ReadAllTextAsync(Path.Combine(occupied, "keep")) == "untouched", "No destructive command or deletion");
            Expect(!new RepositoryCloneOptions("https://user:secret@example.invalid/repo", Folder("secret")).IsValid &&
                !new RepositoryCloneOptions("relative", Folder("relative")).IsValid && !new RepositoryCloneOptions(source, Folder("bad"), "--bad").IsValid, "Credentials, ambiguous paths and option-shaped branch rejected");
            Expect(new RepositoryCloneOptions("git@example.invalid:team/project.git", Folder("ssh")).IsValid, "SCP-style SSH accepted");
            var failed = await actions.CloneRepositoryAsync(git, new(Folder("missing.git"), Folder("failed")), default);
            Expect(!failed.Action.Success && failed.Action.Output.Length > 0 && failed.Repository is null, "Native failure and output retained");
        });
        await Check("Authentication cancellation stops clone and preserves an existing partial repository", async () =>
        {
            using var cancelled = new CancellationTokenSource(); var auth = new NetworkWorkflowAuthentication(cancelled.Token);
            var entered = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
            runner.CloneOverride = async (request, token, output) =>
            {
                var result = await runner.Inner.RunAsync(request, token, output); Expect(result.ExitCode == 0, "Fixture clone succeeded before cancellation");
                entered.SetResult(); try { await Task.Delay(Timeout.Infinite, token); } catch (OperationCanceledException) { }
                return result with { Outcome = ProcessOutcome.Cancelled, ExitCode = null };
            };
            try
            {
                var target = Folder("cancelled partial"); var task = actions.CloneRepositoryAsync(git, new(source, target), default, authentication: auth);
                await entered.Task.WaitAsync(TimeSpan.FromSeconds(8)); cancelled.Cancel(); var result = await task;
                Expect(result.Action.Outcome == ProcessOutcome.Cancelled && result.Repository?.Worktree == target && File.Exists(Path.Combine(target, "a.txt")), "Partial result observed without deleting data");
                Expect(runner.CloneRequest!.Environment["SHOVEL_FIXTURE_AUTH"] == "active" && runner.CloneRequest.Environment["GIT_SSH_COMMAND"]!.Contains("BatchMode=no"), "Auth session reaches native process");
            }
            finally { runner.CloneOverride = null; }
        });

        var ui = new NetworkWorkflowInteraction(); var shell = new ShellViewModel(reader, ui, identities, actions: actions);
        shell.CompleteStartup(new(new(), SettingsLoadStatus.Defaults));
        try
        {
            await Check("Clone options perform no preparation; submission opens history and cancellation keeps the prior tab", async () =>
            {
                var before = runner.Calls; ui.Clone = () => { Expect(runner.Calls == before && shell.ActionOpen, "No work before Clone submission"); return null; };
                shell.CloneRepositoryCommand.Execute(null); await shell.ActionTask; Expect(runner.Calls == before, "Cancel options makes no Git calls");
                var target = Folder("shell clone"); ui.Clone = () => new(source, target);
                shell.CloneRepositoryCommand.Execute(null); await shell.ActionTask; await shell.WaitForIdleAsync();
                Expect(shell.CurrentAction?.Succeeded == true && shell.SelectedRepository?.Directory == target && shell.Commits.Count == 1, "Real clone opens history: " + shell.CurrentAction?.Status);
                Expect(ui.Auth?.Disposed == true, "Authentication session disposed after operation");
                var owner = shell.SelectedRepository; var entered = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
                runner.CloneOverride = async (_, token, _) => { entered.SetResult(); try { await Task.Delay(Timeout.Infinite, token); } catch (OperationCanceledException) { } return new(ProcessOutcome.Cancelled, null, [], [], TimeSpan.Zero); };
                try
                {
                    ui.Clone = () => new(source, Folder("shell cancel")); shell.CloneRepositoryCommand.Execute(null); await entered.Task.WaitAsync(TimeSpan.FromSeconds(8));
                    Expect(shell.CancelCommand.CanExecute(null) && !shell.CloseRepositoryCommand.CanExecute(null), "Action owns cancel and navigation remains bounded");
                    await Dispatcher.Yield(); shell.CancelCommand.Execute(null); await shell.ActionTask; await shell.WaitForIdleAsync();
                    Expect(shell.SelectedRepository == owner && shell.CurrentAction?.Activity?.Outcome == OperationOutcome.Cancelled, "Cancelled operation retains selected tab and distinct result");
                }
                finally { runner.CloneOverride = null; }
            });
            var target = Folder("shell clone");
            await Check("Remote editing preserves branches and dirty files while updating fetch and optional push URLs", async () =>
            {
                await File.WriteAllTextAsync(Path.Combine(target, "a.txt"), "keep working edits\n");
                var context = await actions.PrepareAsync(git, target, default);
                var reviewed = await actions.ReadRemoteConfigurationAsync(git, target, default);
                var destination = Folder("offline server with spaces.git");
                var result = await actions.ChangeRemoteAsync(context, reviewed, new("origin", destination, "ssh://git@example.invalid/push.git"), default);
                Expect(result.Success, result.Output);
                Expect(await Git(target, "config", "remote.origin.url") == destination && await Git(target, "config", "remote.origin.pushurl") == "ssh://git@example.invalid/push.git", "Literal fetch and push values written");
                reviewed = await actions.ReadRemoteConfigurationAsync(git, target, default);
                result = await actions.ChangeRemoteAsync(context, reviewed, new("origin", destination, ""), default);
                Expect(result.Success && await Git(target, "remote", "get-url", "--push", "origin") == destination, "Clearing push override falls back to fetch URL");
                Expect(await Git(target, "rev-parse", "HEAD") == head && await Git(target, "rev-parse", "--symbolic-full-name", "@{upstream}") == "refs/remotes/origin/main" &&
                    await File.ReadAllTextAsync(Path.Combine(target, "a.txt")) == "keep working edits\n", "Branch, tracking and files preserved");
            });
            await Check("A failed second URL write retains the first update and reports failure", async () =>
            {
                var context = await actions.PrepareAsync(git, target, default); var reviewed = await actions.ReadRemoteConfigurationAsync(git, target, default);
                var prior = reviewed.Remotes.Single().Location; var writes = 0;
                runner.RemoteOverride = (request, token, output) => ++writes == 1 ? runner.Inner.RunAsync(request, token, output) :
                    Task.FromResult(new ProcessResult(ProcessOutcome.Exited, 1, [], Encoding.UTF8.GetBytes("fixture second-write failure"), TimeSpan.Zero));
                try
                {
                    var result = await actions.ChangeRemoteAsync(context, reviewed, new("origin", source, "https://example.invalid/failed"), default);
                    Expect(!result.Success && result.Output.Contains("second-write failure") && await Git(target, "config", "remote.origin.url") == source,
                        "First update retained and second failure returned without false rollback");
                    Expect((await actions.ReadRemoteConfigurationAsync(git, target, default)).Remotes.Single().PushLocation == "", "Failed push write leaves prior value");
                }
                finally { runner.RemoteOverride = null; await Git(target, "config", "remote.origin.url", prior); }
            });
            await Check("Inherited URL configuration disables editing without modifying global settings", async () =>
            {
                var config = Environment.GetEnvironmentVariable("GIT_CONFIG_GLOBAL")!; var saved = await File.ReadAllTextAsync(config);
                try
                {
                    await Git(root, "config", "--file", config, "remote.origin.url", "https://example.invalid/inherited");
                    var reviewed = await actions.ReadRemoteConfigurationAsync(git, target, default);
                    Expect(!reviewed.Remotes.Single().CanEdit, "Additional inherited URL cannot be flattened");
                }
                finally { await File.WriteAllTextAsync(config, saved); }
            });
            await Check("Remote writes reject stale configuration and locks; multiple URLs are never flattened", async () =>
            {
                var context = await actions.PrepareAsync(git, target, default); var reviewed = await actions.ReadRemoteConfigurationAsync(git, target, default);
                await Git(target, "config", "fixture.changed", "true");
                try { await actions.ChangeRemoteAsync(context, reviewed, new("origin", source, ""), default); throw new InvalidOperationException("Stale edit accepted"); }
                catch (GitActionException error) { Expect(error.Code == "RemoteManageChanged", error.Code); }
                reviewed = await actions.ReadRemoteConfigurationAsync(git, target, default);
                var lockPath = Path.Combine(target, ".git", "config.lock"); await File.WriteAllTextAsync(lockPath, "fixture");
                try { var result = await actions.ChangeRemoteAsync(context, reviewed, new("origin", source, ""), default); Expect(!result.Success && result.Output.Length > 0, "Config lock is a visible Git failure"); }
                finally { File.Delete(lockPath); }
                await Git(target, "config", "--add", "remote.origin.url", source);
                reviewed = await actions.ReadRemoteConfigurationAsync(git, target, default); Expect(!reviewed.Remotes.Single().CanEdit, "Multiple fetch URLs disable editing");
                try { await actions.ChangeRemoteAsync(context, reviewed, new("origin", source, ""), default); throw new InvalidOperationException("Multi URL edit accepted"); }
                catch (GitActionException error) { Expect(error.Code == "RemoteManageUnsupported", error.Code); }
                Expect((await Git(target, "config", "--get-all", "remote.origin.url")).Split('\n').Length == 2, "Both URLs retained");
                await Git(target, "config", "--replace-all", "remote.origin.url", source);
            });
            await Check("Remote menu uses cached values; Save and confirmed Remove execute and reconcile without network access", async () =>
            {
                shell.RefreshRepositoryCommand.Execute(null); await shell.WaitForIdleAsync();
                var calls = runner.Calls;
                ui.Remote = model => { Expect(runner.Calls == calls && model.Selected?.Name == "origin", "Cached form, no Git preparation"); return null; };
                shell.ManageRemotesCommand.Execute(null); await shell.ActionTask; Expect(runner.Calls == calls, "Cancel remote form performs no work");
                ui.Remote = _ => new("origin", source, "https://example.invalid/push");
                shell.ManageRemotesCommand.Execute(null); await shell.ActionTask; await shell.WaitForIdleAsync();
                Expect(shell.CurrentAction?.Succeeded == true && await Git(target, "config", "remote.origin.pushurl") == "https://example.invalid/push", "Save wired to actual Git");
                calls = runner.Calls; ui.Remote = _ => new("origin", source, "", true); ui.Confirm = false;
                shell.ManageRemotesCommand.Execute(null); await shell.ActionTask;
                Expect(runner.Calls == calls && ui.Confirmation.Contains("origin") && ui.Confirmation.Contains(target), "Declined actual-target removal does no work");
                ui.Confirm = true; shell.ManageRemotesCommand.Execute(null); await shell.ActionTask; await shell.WaitForIdleAsync();
                Expect(shell.CurrentAction?.Succeeded == true && shell.SelectedRepository!.Snapshot!.RemoteNames.Count == 0 && await Git(target, "for-each-ref", "refs/remotes/origin") == "", "Remove deletes remote and tracking refs, then refreshes");
                Expect(await Git(target, "rev-parse", "HEAD") == head && await File.ReadAllTextAsync(Path.Combine(target, "a.txt")) == "keep working edits\n" && !runner.NetworkCommand, "Local branch and edits preserved, no fetch/ls-remote/push");
            });
            await Check("Clone and remote forms bind native inputs and shared Dark/Light resources without visible windows", async () =>
            {
                foreach (var theme in new[] { AppAppearance.Dark, AppAppearance.Light })
                {
                    AppTheme.Apply(Application.Current.Resources, false, theme);
                    var cloneModel = new RepositoryCloneViewModel(ui) { Source = source, Directory = Folder("typed") };
                    var remoteModel = new RemoteManagementViewModel([new("origin", source, "", true)]);
                    Window[] windows = [new RepositoryCloneWindow(cloneModel), new RemoteManagementWindow(remoteModel)];
                    foreach (var window in windows)
                    {
                        try
                        {
                            var content = (Grid)window.Content; content.Background = window.Background;
                            Arrange(content, 640, 420); await Dispatcher.Yield(DispatcherPriority.ContextIdle);
                            Expect(VisualDescendants<TextBox>(content).Count() >= 2 && VisualDescendants<Button>(content).Any(b => b.Command?.CanExecute(null) == true), "Bound text and submission controls");
                            SaveRender(content, $"network-{window.GetType().Name}-{theme}.png", 640, 420);
                        }
                        finally { window.Close(); }
                    }
                    Expect(cloneModel.SubmitCommand.CanExecute(null) && remoteModel.SaveCommand.CanExecute(null), "Valid options ready without IO");
                }
                AppTheme.Apply(Application.Current.Resources, false);
            });
        }
        finally { shell.BeginShutdown(); await shell.WaitForIdleAsync(); }
    }

    private sealed class NetworkWorkflowRunner : IProcessRunner
    {
        public WindowsProcessRunner Inner { get; } = new();
        public int Calls, CloneCalls; public bool NetworkCommand;
        public ProcessRequest? CloneRequest;
        public Func<ProcessRequest, CancellationToken, Action<ProcessOutput>?, Task<ProcessResult>>? CloneOverride;
        public Func<ProcessRequest, CancellationToken, Action<ProcessOutput>?, Task<ProcessResult>>? RemoteOverride;
        public Task<ProcessResult> RunAsync(ProcessRequest request, CancellationToken token, Action<ProcessOutput>? output = null)
        {
            Interlocked.Increment(ref Calls);
            if (request.Arguments.Contains("fetch") || request.Arguments.Contains("push") || request.Arguments.Contains("ls-remote")) NetworkCommand = true;
            if (request.Arguments.Contains("remote") && request.Arguments.Contains("set-url") && RemoteOverride is not null) return RemoteOverride(request, token, output);
            if (request.Arguments.Contains("clone"))
            {
                Interlocked.Increment(ref CloneCalls); CloneRequest = request;
                if (CloneOverride is not null) return CloneOverride(request, token, output);
            }
            return Inner.RunAsync(request, token, output);
        }
    }
    private sealed class NetworkWorkflowAuthentication(CancellationToken token = default) : IAuthenticationSession
    {
        public IReadOnlyDictionary<string, string?> Environment { get; } = new Dictionary<string, string?> { ["SHOVEL_FIXTURE_AUTH"] = "active" };
        public CancellationToken CancellationToken => token;
        public AuthenticationFailure? Failure => null;
        public bool Disposed;
        public ValueTask DisposeAsync() { Disposed = true; return ValueTask.CompletedTask; }
    }
    private sealed class NetworkWorkflowInteraction : IUserInteraction
    {
        public Func<RepositoryCloneOptions?>? Clone;
        public Func<RemoteManagementViewModel, RemoteManagementOptions?>? Remote;
        public bool Confirm; public string Confirmation = ""; public NetworkWorkflowAuthentication? Auth;
        public RepositoryCloneOptions? ChooseRepositoryClone() => Clone?.Invoke();
        public RemoteManagementOptions? ChooseRemoteManagement(RemoteManagementViewModel model) => Remote?.Invoke(model);
        public bool ConfirmRecovery(string message, string title) { Confirmation = message; return Confirm; }
        public string? ChooseRepository() => null;
        public string? ChooseGit() => null;
        public IAuthenticationSession? CreateAuthenticationSession(string context, Func<AuthenticationPrompt, CancellationToken, Task<AuthenticationAnswer?>> prompt, CancellationToken token) => Auth = new(token);
    }
}
