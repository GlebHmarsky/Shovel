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
    // Register inside the existing pumped, isolated-config RunViewModelChecks scope.
    // No dependency on another check's fixture and no live remote or visible window.
    private static async Task RunBranchManagementChecksAsync()
    {
        var runner = new BranchManagementRunner(new WindowsProcessRunner());
        var reader = new RepositoryReader(runner); var identities = new WindowsRepositoryIdentity();
        var actions = new GitActions(runner, reader, identities); var git = await reader.DiscoverAsync(null, default);
        async Task<ProcessResult> Raw(string path, params string[] args) => await runner.RunAsync(new(git.Executable, path, args), default);
        async Task<string> Git(string path, params string[] args)
        {
            var result = await Raw(path, args);
            Expect(result.Outcome == ProcessOutcome.Exited && result.ExitCode == 0, "Fixture Git: " + Encoding.UTF8.GetString(result.StandardError));
            return Encoding.UTF8.GetString(result.StandardOutput).Trim();
        }
        async Task<string> Repository(string label)
        {
            var path = Fixture("branch-management " + label);
            await Git(path, "init", "-b", "main");
            await Git(path, "config", "user.name", "Branch Fixture"); await Git(path, "config", "user.email", "branch@example.invalid");
            await Git(path, "config", "commit.gpgSign", "false");
            await Git(path, "config", "core.hooksPath", Fixture("branch hooks " + label));
            await File.WriteAllTextAsync(Path.Combine(path, "tracked.txt"), "base\n");
            await Git(path, "add", "."); await Git(path, "commit", "-m", "Base");
            await Git(path, "branch", "merged/日本語");
            // An unavailable local path ensures branch management never needs a remote
            // connection. Seed the local tracking tip without running fetch or push.
            await Git(path, "remote", "add", "origin", Path.Combine(_root, "not-a-remote.git"));
            await Git(path, "update-ref", "refs/remotes/origin/main", "HEAD");
            return path;
        }
        async Task<ActionContext> Context(string path)
        {
            var identity = await identities.ReadAsync(path, default);
            var snapshot = await reader.InspectAsync(git, path, default);
            var history = await reader.OpenHistoryAsync(git, snapshot, HistoryScope.AllCommits, default);
            return await actions.PrepareBranchManagementAsync(git, identity, snapshot, history, default);
        }
        static BranchManagementOptions Options(ActionContext context, BranchManagementAction action,
            string branch = "merged/日本語", string? upstream = null) => new(action,
                context.History.References.Single(item => item.Name == "refs/heads/" + branch),
                upstream is null ? null : context.History.References.Single(item => item.Name == upstream));
        async Task Reject(string code, Func<Task> run)
        {
            try { await run(); throw new InvalidOperationException("Expected " + code); }
            catch (GitActionException error) { Expect(error.Code == code, "Expected " + code + ", got " + error.Code); }
        }
        async Task<bool> Exists(string path, string name) => (await Raw(path, "show-ref", "--verify", "--quiet", name)).ExitCode == 0;

        await Check("Branch management deletes a merged local branch without touching HEAD, index, files or remote refs", async () =>
        {
            var repo = await Repository("merged"); var context = await Context(repo);
            await File.WriteAllTextAsync(Path.Combine(repo, "tracked.txt"), "dirty retained\n");
            await File.WriteAllTextAsync(Path.Combine(repo, "untracked.txt"), "retained");
            var index = await File.ReadAllBytesAsync(Path.Combine(context.Repository.GitDirectory, "index"));
            var result = await actions.ExecuteBranchManagementAsync(context, Options(context, BranchManagementAction.Delete), default);
            Expect(result.Success && !await Exists(repo, "refs/heads/merged/日本語") && await Exists(repo, "refs/remotes/origin/main"), "Only the chosen local ref is removed");
            Expect(await Git(repo, "rev-parse", "HEAD") == context.Repository.Commit && await Git(repo, "branch", "--show-current") == "main", "HEAD unchanged");
            Expect(index.SequenceEqual(await File.ReadAllBytesAsync(Path.Combine(context.Repository.GitDirectory, "index"))) &&
                await File.ReadAllTextAsync(Path.Combine(repo, "tracked.txt")) == "dirty retained\n" &&
                await File.ReadAllTextAsync(Path.Combine(repo, "untracked.txt")) == "retained", "Index and dirty/untracked files unchanged");
        });
        await Check("Branch deletion preserves Git's unmerged protection and returns original failure output", async () =>
        {
            var repo = await Repository("unmerged"); await Git(repo, "switch", "-c", "unmerged");
            await Git(repo, "commit", "--allow-empty", "-m", "Not integrated"); await Git(repo, "switch", "main");
            var context = await Context(repo);
            var result = await actions.ExecuteBranchManagementAsync(context, Options(context, BranchManagementAction.Delete, "unmerged"), default);
            Expect(!result.Success && result.ExitCode != 0 && result.Output.Length > 0 && await Exists(repo, "refs/heads/unmerged"), "No force fallback on unmerged failure");
        });
        await Check("Branch deletion uses the configured upstream's merged test rather than inventing a HEAD-only policy", async () =>
        {
            var repo = await Repository("upstream merged"); await Git(repo, "switch", "-c", "upstream-only");
            await Git(repo, "commit", "--allow-empty", "-m", "On upstream"); await Git(repo, "update-ref", "refs/remotes/origin/landed", "HEAD");
            await Git(repo, "branch", "--set-upstream-to=origin/landed"); await Git(repo, "switch", "main");
            var context = await Context(repo);
            Expect((await actions.ExecuteBranchManagementAsync(context, Options(context, BranchManagementAction.Delete, "upstream-only"), default)).Success,
                "Normal Git permits a branch merged into its upstream even when not merged into HEAD");
        });
        await Check("Current and linked-worktree branches cannot be deleted, including from a linked worktree", async () =>
        {
            var repo = await Repository("occupied"); var context = await Context(repo);
            await Reject("BranchManageCurrent", () => actions.ExecuteBranchManagementAsync(context, Options(context, BranchManagementAction.Delete, "main"), default));
            var linked = Path.Combine(_root, "branch-linked " + Guid.NewGuid().ToString("N"));
            await Git(repo, "worktree", "add", linked, "merged/日本語"); context = await Context(repo);
            await Reject("BranchManageOccupied", () => actions.ExecuteBranchManagementAsync(context, Options(context, BranchManagementAction.Delete), default));
            var linkedContext = await Context(linked);
            await Reject("BranchManageOccupied", () => actions.ExecuteBranchManagementAsync(linkedContext, Options(linkedContext, BranchManagementAction.Delete, "main"), default));
            Expect(await Exists(repo, "refs/heads/main") && await Exists(repo, "refs/heads/merged/日本語"), "Occupied refs preserved");
        });
        await Check("Set and unset upstream change real tracking configuration for current and noncurrent branches", async () =>
        {
            var repo = await Repository("tracking");
            foreach (var branch in new[] { "main", "merged/日本語" })
            {
                var context = await Context(repo);
                Expect((await actions.ExecuteBranchManagementAsync(context, Options(context, BranchManagementAction.SetUpstream, branch, "refs/remotes/origin/main"), default)).Success, "Set real upstream");
                Expect(await Git(repo, "config", "--get", "branch." + branch + ".remote") == "origin" &&
                    await Git(repo, "config", "--get", "branch." + branch + ".merge") == "refs/heads/main", "Git writes remote/merge mapping");
                context = await Context(repo);
                Expect(context.History.References.Single(item => item.Name == "refs/heads/" + branch).Upstream == "refs/remotes/origin/main", "Ref metadata exposes updated tracking");
                if (branch == "main") Expect((await reader.ReadDivergenceAsync(git, context.History, default)).State == DivergenceState.Available, "Current upstream counts available");
                Expect((await actions.ExecuteBranchManagementAsync(context, Options(context, BranchManagementAction.UnsetUpstream, branch), default)).Success, "Unset real upstream");
                Expect((await Raw(repo, "config", "--get", "branch." + branch + ".remote")).ExitCode == 1 &&
                    (await Raw(repo, "config", "--get", "branch." + branch + ".merge")).ExitCode == 1, "Tracking keys removed");
            }
            Expect(await Git(repo, "branch", "--show-current") == "main" && await Exists(repo, "refs/remotes/origin/main"), "No checkout or remote mutation");
        });
        await Check("Missing upstream can be unset; missing refs and symbolic remote aliases are rejected", async () =>
        {
            var repo = await Repository("missing"); await Git(repo, "branch", "--set-upstream-to=origin/main", "main");
            await Git(repo, "update-ref", "-d", "refs/remotes/origin/main"); var context = await Context(repo);
            Expect((await actions.ExecuteBranchManagementAsync(context, Options(context, BranchManagementAction.UnsetUpstream, "main"), default)).Success, "Unset works when tracked destination has disappeared");
            context = await Context(repo);
            await Reject("BranchManageNoRemoteRef", () => actions.ExecuteBranchManagementAsync(context, Options(context, BranchManagementAction.SetUpstream, "main"), default));
            // Context changed after unset, so prepare a new one for concrete-ref validation.
            await Git(repo, "update-ref", "refs/remotes/origin/main", "HEAD"); await Git(repo, "symbolic-ref", "refs/remotes/origin/alias", "refs/remotes/origin/main");
            context = await Context(repo);
            await Reject("BranchManageSymbolicRef", () => actions.ExecuteBranchManagementAsync(context, Options(context, BranchManagementAction.SetUpstream, "main", "refs/remotes/origin/alias"), default));
        });
        await Check("Changed or deleted targets, remote tips, configuration and identity fail before writing", async () =>
        {
            var repo = await Repository("stale"); var context = await Context(repo);
            await Git(repo, "commit", "--allow-empty", "-m", "External");
            await Git(repo, "update-ref", "refs/heads/merged/日本語", "HEAD");
            await Reject("BranchManageChanged", () => actions.ExecuteBranchManagementAsync(context, Options(context, BranchManagementAction.Delete), default));
            context = await Context(repo); await Git(repo, "branch", "-d", "merged/日本語");
            await Reject("BranchManageChanged", () => actions.ExecuteBranchManagementAsync(context, Options(context, BranchManagementAction.Delete), default));
            context = await Context(repo); var tracking = Options(context, BranchManagementAction.SetUpstream, "main", "refs/remotes/origin/main");
            await Git(repo, "update-ref", "refs/remotes/origin/main", "HEAD");
            await Reject("BranchManageChanged", () => actions.ExecuteBranchManagementAsync(context, tracking, default));
            context = await Context(repo); await Git(repo, "config", "branch.main.remote", "other");
            await Reject("BranchManageChanged", () => actions.ExecuteBranchManagementAsync(context, Options(context, BranchManagementAction.UnsetUpstream, "main"), default));
            context = await Context(repo);
            await Reject("ActionIdentityChanged", () => actions.ExecuteBranchManagementAsync(context with { Identity = context.Identity with { Fingerprint = "replaced" } }, Options(context, BranchManagementAction.UnsetUpstream, "main"), default));
        });
        await Check("Active sequences block management; existing ref and config locks survive failed commands", async () =>
        {
            var repo = await Repository("locks"); var context = await Context(repo);
            var marker = Path.Combine(context.Repository.GitDirectory, "MERGE_HEAD");
            await File.WriteAllTextAsync(marker, context.Repository.Commit);
            try { await Reject("ActionSequence", () => actions.ExecuteBranchManagementAsync(context, Options(context, BranchManagementAction.Delete), default)); }
            finally { File.Delete(marker); }
            context = await Context(repo);
            var refLock = Path.Combine(context.Repository.CommonDirectory, "refs", "heads", "merged", "日本語.lock");
            await File.WriteAllTextAsync(refLock, "fixture lock");
            try
            {
                Expect(!(await actions.ExecuteBranchManagementAsync(context, Options(context, BranchManagementAction.Delete), default)).Success &&
                    await File.ReadAllTextAsync(refLock) == "fixture lock" && await Exists(repo, "refs/heads/merged/日本語"), "Ref lock is not deleted or bypassed");
            }
            finally { File.Delete(refLock); }
            var configLock = Path.Combine(context.Repository.CommonDirectory, "config.lock"); await File.WriteAllTextAsync(configLock, "fixture config lock");
            try
            {
                Expect(!(await actions.ExecuteBranchManagementAsync(context, Options(context, BranchManagementAction.SetUpstream, "main", "refs/remotes/origin/main"), default)).Success &&
                    await File.ReadAllTextAsync(configLock) == "fixture config lock", "Config lock is not deleted or bypassed");
            }
            finally { File.Delete(configLock); }
        });
        await Check("Cancelled branch execution releases the common gate shared with existing Git commands", async () =>
        {
            var repo = await Repository("cancel gate"); var context = await Context(repo);
            runner.HoldWrite = true; runner.WriteStarted = new(TaskCreationOptions.RunContinuationsAsynchronously);
            using var cancel = new CancellationTokenSource(TimeSpan.FromSeconds(20));
            var pending = actions.ExecuteBranchManagementAsync(context, Options(context, BranchManagementAction.Delete), cancel.Token);
            try
            {
                await runner.WriteStarted.Task.WaitAsync(TimeSpan.FromSeconds(12));
                await Reject("ActionOtherInstance", () => actions.ExecuteAsync(context, new(GitAction.NewBranch, "blocked", Checkout: false), default));
                cancel.Cancel();
                try { await pending; throw new InvalidOperationException("Expected cancellation"); } catch (OperationCanceledException) { }
            }
            finally { cancel.Cancel(); runner.HoldWrite = false; try { await pending; } catch (OperationCanceledException) { } }
            Expect(await Exists(repo, "refs/heads/merged/日本語"), "Cancelled before write retains branch");
            Expect((await actions.ExecuteAsync(await Context(repo), new(GitAction.NewBranch, "after-cancel", Checkout: false), default)).Success, "Existing command reacquires the same gate");
        });
        await Check("Failed and cancelled process outcomes preserve their outcome even when Git already changed the ref", async () =>
        {
            var repo = await Repository("process outcomes"); var context = await Context(repo);
            runner.BeforeWriteOutcome = ProcessOutcome.TimedOut;
            try
            {
                var failed = await actions.ExecuteBranchManagementAsync(context, Options(context, BranchManagementAction.Delete), default);
                Expect(!failed.Success && failed.Outcome == ProcessOutcome.TimedOut && await Exists(repo, "refs/heads/merged/日本語"), "Timeout is reported without removal or retry");
            }
            finally { runner.BeforeWriteOutcome = null; }
            var ui = new BranchManagementInteraction { Choose = model => { model.SubmitCommand.Execute(null); return model.SubmittedOptions; } };
            var shell = new ShellViewModel(reader, ui, identities, actions: actions);
            shell.CompleteStartup(new(new() { RepositoryPaths = [repo], SelectedRepositoryPath = repo }, SettingsLoadStatus.Loaded)); await shell.WaitForIdleAsync();
            runner.CancelAfterWrite = true;
            try
            {
                shell.DeleteBranchCommand.Execute(null); await shell.ActionTask; await shell.WaitForIdleAsync();
                Expect(shell.CurrentAction is { Succeeded: false, IsRunning: false } && shell.CurrentAction.Status == UiText.Get("ActionResultCancelled") &&
                    shell.CurrentAction.RawOutput.Length > 0 && !shell.LocalBranches.Contains("merged/日本語") && !await Exists(repo, "refs/heads/merged/日本語"),
                    "A cancelled result can still have changed Git; fresh history reflects that without rollback");
            }
            finally { runner.CancelAfterWrite = false; shell.BeginShutdown(); await shell.WaitForIdleAsync(); }
        });
        await Check("Branch options are cached and deletion confirms the exact branch/tip with Cancel as default", async () =>
        {
            var repo = await Repository("options"); var context = await Context(repo); var calls = runner.Calls;
            var model = new BranchManagementViewModel(BranchManagementAction.Delete, context.Repository, context.History);
            model.Branch = context.History.References.Single(item => item.Name == "refs/heads/merged/日本語");
            var window = new BranchManagementWindow(model);
            try
            {
                var content = (FrameworkElement)window.Content; Arrange(content, 660, 500);
                Expect(runner.Calls == calls && model.Target.Contains(model.Branch!.Hash) && model.Target.Contains(model.Branch.Name) &&
                    !model.SubmitIsDefault && window.ResizeMode == ResizeMode.NoResize, "Opening/editing/layout starts no Git and shows reviewed target");
                Expect(VisualDescendants<Button>(content).Single(button => button.IsCancel).IsDefault, "Delete defaults to Cancel");
                model.Branch = context.History.References.Single(item => item.Name == "refs/heads/main");
                Expect(!model.SubmitCommand.CanExecute(null) && model.Readiness == UiText.Get("BranchManageCurrent"), "Current branch has a localized disabled reason");
            }
            finally { window.Close(); }
            // A never-shown offscreen window cannot accept DialogResult. Exercise submission
            // on a standalone options model after the view's event lifetime has ended.
            var standalone = new BranchManagementViewModel(BranchManagementAction.Delete, context.Repository, context.History);
            standalone.SubmitCommand.Execute(null); var submitted = standalone.SubmittedOptions;
            standalone.Branch = context.History.References.Single(item => item.Name == "refs/heads/main");
            Expect(submitted?.Branch.Name == "refs/heads/merged/日本語", "Submitted target is immutable");
        });
        await Check("Bound branch menu commands execute and refresh labels/counts; dismissal and stale targets are accurate", async () =>
        {
            var repo = await Repository("shell"); var ui = new BranchManagementInteraction();
            var shell = new ShellViewModel(reader, ui, identities, actions: actions);
            shell.CompleteStartup(new(new() { RepositoryPaths = [repo], SelectedRepositoryPath = repo }, SettingsLoadStatus.Loaded)); await shell.WaitForIdleAsync();
            try
            {
                var calls = runner.Calls;
                ui.Choose = model => { Expect(runner.Calls == calls, "No Git before showing cached options"); return null; };
                shell.DeleteBranchCommand.Execute(null); await shell.ActionTask;
                Expect(runner.Calls == calls && !shell.ActionOpen && shell.CurrentAction is null, "Dismissal runs no Git and leaves prior operation intact");
                ui.Choose = model =>
                {
                    model.Branch = model.Branches.Single(item => item.Name == "refs/heads/main");
                    model.Upstream = model.RemoteBranches.Single(item => item.Name == "refs/remotes/origin/main");
                    model.SubmitCommand.Execute(null); return model.SubmittedOptions;
                };
                shell.SetBranchUpstreamCommand.Execute(null); await shell.ActionTask; await shell.WaitForIdleAsync();
                Expect(shell.CurrentAction?.Succeeded == true && shell.SelectedRepository!.Snapshot!.UpstreamRef == "refs/remotes/origin/main" &&
                    shell.PullCount == "0" && shell.PushCount == "0" && shell.CurrentAction.Title == UiText.Get("BranchManageTitleSetUpstream"), "Bound tracking and refreshed counts with accurate operation title");
                shell.UnsetBranchUpstreamCommand.Execute(null); await shell.ActionTask; await shell.WaitForIdleAsync();
                Expect(shell.CurrentAction?.Succeeded == true && shell.SelectedRepository!.Snapshot!.UpstreamRef is null && shell.PullCount != "0", "Unset refreshes comparison to unavailable");
                ui.Choose = model => { model.Branch = model.Branches.Single(item => item.Name == "refs/heads/merged/日本語"); model.SubmitCommand.Execute(null); return model.SubmittedOptions; };
                await Git(repo, "branch", "-d", "merged/日本語"); // cached options are deliberately stale
                shell.DeleteBranchCommand.Execute(null); await shell.ActionTask; await shell.WaitForIdleAsync();
                Expect(shell.CurrentAction?.Status == UiText.Get("BranchManageChanged") && !shell.LocalBranches.Contains("merged/日本語"), "Failed preparation still refreshes stale refs");
                await Git(repo, "branch", "merged/日本語"); shell.RefreshRepositoryCommand.Execute(null); await shell.WaitForIdleAsync();
                shell.DeleteBranchCommand.Execute(null); await shell.ActionTask; await shell.WaitForIdleAsync();
                Expect(shell.CurrentAction?.Succeeded == true && !shell.LocalBranches.Contains("merged/日本語"), "Bound deletion removes real ref and cached menu entry");
            }
            finally { shell.BeginShutdown(); await shell.WaitForIdleAsync(); }
        });
        await Check("Bound cancellation retains output and reconciles; Git success remains separate from refresh failure", async () =>
        {
            var repo = await Repository("shell cancellation"); var ui = new BranchManagementInteraction();
            ui.Choose = model => { model.SubmitCommand.Execute(null); return model.SubmittedOptions; };
            var shell = new ShellViewModel(reader, ui, identities, actions: actions);
            shell.CompleteStartup(new(new() { RepositoryPaths = [repo], SelectedRepositoryPath = repo }, SettingsLoadStatus.Loaded)); await shell.WaitForIdleAsync();
            runner.HoldWrite = true; runner.WriteStarted = new(TaskCreationOptions.RunContinuationsAsynchronously);
            try
            {
                shell.DeleteBranchCommand.Execute(null); await runner.WriteStarted.Task.WaitAsync(TimeSpan.FromSeconds(12));
                Expect(shell.IsActionRunning && shell.CancelCommand.CanExecute(null) && !shell.DeleteBranchCommand.CanExecute(null), "Shared progress/cancel and duplicate admission");
                shell.CancelCommand.Execute(null); await shell.ActionTask.WaitAsync(TimeSpan.FromSeconds(30));
                Expect(shell.CurrentAction is { Succeeded: false, IsRunning: false } && shell.LocalBranches.Contains("merged/日本語") && shell.CanNavigate, "Cancelled operation reinspected before navigation resumes");
            }
            finally
            {
                shell.CancelCommand.Execute(null); runner.HoldWrite = false; await shell.ActionTask;
                shell.BeginShutdown(); await shell.WaitForIdleAsync();
            }
            var context = await Context(repo);
            var model = new GitActionViewModel(context, GitAction.DeleteBranch, actions, () => throw new IOException(),
                execute: (token, progress) => actions.ExecuteBranchManagementAsync(context, Options(context, BranchManagementAction.Delete), token, progress));
            model.RunCommand.Execute(null); await model.RunningTask;
            Expect(model.Succeeded && model.Phase == UiText.Get("ActionRefreshFailedShort") && !await Exists(repo, "refs/heads/merged/日本語"), "Successful write and failed reconciliation remain distinct");
        });
        Expect(!runner.SawNetworkOrCheckout, "Management never fetches/pushes or checks out implicitly");
    }

    private sealed class BranchManagementInteraction : IUserInteraction
    {
        public Func<BranchManagementViewModel, BranchManagementOptions?>? Choose;
        public string? ChooseRepository() => null;
        public string? ChooseGit() => null;
        public BranchManagementOptions? ChooseBranchManagement(BranchManagementViewModel model) => Choose?.Invoke(model);
    }
    private sealed class BranchManagementRunner(IProcessRunner inner) : IProcessRunner
    {
        public int Calls; public bool HoldWrite; public bool SawNetworkOrCheckout; public bool CancelAfterWrite;
        public ProcessOutcome? BeforeWriteOutcome;
        public TaskCompletionSource WriteStarted = new(TaskCreationOptions.RunContinuationsAsynchronously);
        public async Task<ProcessResult> RunAsync(ProcessRequest request, CancellationToken token, Action<ProcessOutput>? progress = null)
        {
            Interlocked.Increment(ref Calls);
            // Fixture commands use short raw argument lists; production mutation commands
            // have --no-lazy-fetch and preserve their structured branch-only scope.
            if (request.Arguments.Contains("--no-lazy-fetch") && request.Arguments.Any(argument => argument is "fetch" or "push" or "switch" or "checkout")) SawNetworkOrCheckout = true;
            var management = request.Arguments.Contains("branch") && request.Arguments.Any(argument => (argument is "--delete" or "--unset-upstream") || argument.StartsWith("--set-upstream-to=", StringComparison.Ordinal));
            if (HoldWrite && management)
            { WriteStarted.TrySetResult(); await Task.Delay(Timeout.Infinite, token); }
            if (management && BeforeWriteOutcome is { } outcome)
                return new(outcome, null, [], Encoding.UTF8.GetBytes("fixture interrupted before write"), TimeSpan.Zero);
            var result = await inner.RunAsync(request, token, progress);
            return management && CancelAfterWrite ? result with { Outcome = ProcessOutcome.Cancelled, ExitCode = null } : result;
        }
    }
}
