using System.IO;
using System.Text;
using System.Diagnostics;
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
    private static async Task RunRecoveryChecksAsync()
    {
        var runner = new WindowsProcessRunner(); var reader = new RepositoryReader(runner);
        var git = await reader.DiscoverAsync(null, default); var identity = new WindowsRepositoryIdentity();
        var actions = new GitActions(runner, reader, identity);
        async Task<ProcessResult> Raw(string repo, params string[] args) => await runner.RunAsync(new(git.Executable, repo, args)
            { Environment = new Dictionary<string, string?> { ["GIT_EDITOR"] = "true", ["GIT_SEQUENCE_EDITOR"] = "false" } }, default);
        async Task<string> Git(string repo, params string[] args)
        {
            var result = await Raw(repo, args);
            Expect(result.ExitCode == 0, "Fixture Git: " + string.Join(' ', args) + " " + Encoding.UTF8.GetString(result.StandardError));
            return Encoding.UTF8.GetString(result.StandardOutput).Trim();
        }
        async Task Commit(string repo, string content)
        {
            await File.WriteAllTextAsync(Path.Combine(repo, "conflict.txt"), content + "\n");
            await Git(repo, "add", "."); await Git(repo, "commit", "-m", content);
        }
        async Task<string> Conflict(RecoveryKind kind, string suffix)
        {
            var repo = Fixture("recovery " + kind + " " + suffix);
            await Git(repo, "init", "-b", "main"); await Git(repo, "config", "user.name", "Recovery Fixture");
            await Git(repo, "config", "user.email", "recovery@example.invalid");
            await Commit(repo, "base"); await Git(repo, "branch", "base");
            await Commit(repo, "incoming"); await Git(repo, "branch", "incoming");
            if (kind != RecoveryKind.Revert) await Git(repo, "switch", "-C", "main", "base");
            await Commit(repo, "local");
            string[] command = kind switch { RecoveryKind.Merge => ["merge", "incoming"], RecoveryKind.Rebase => ["rebase", "incoming"],
                RecoveryKind.CherryPick => ["cherry-pick", "incoming"], _ => ["revert", "incoming"] };
            Expect((await Raw(repo, command)).ExitCode != 0, "Real conflict created");
            return repo;
        }
        async Task<ActionContext> Context(string repo)
        {
            var snapshot = await reader.InspectAsync(git, repo, default, includeHistory: false);
            return new(git, await identity.ReadAsync(repo, default), snapshot, new(snapshot, HistoryScope.AllCommits, "", [], false), [], "");
        }
        async Task Reject(string code, Func<Task<ActionResult>> run)
        {
            try { await run(); throw new InvalidOperationException("Expected " + code); }
            catch (GitActionException error) { Expect(error.Code == code, "Expected " + code + ", got " + error.Code); }
        }
        foreach (var kind in Enum.GetValues<RecoveryKind>())
        {
            await Check($"Recovery {kind}: reopen, unresolved refusal, stage and Continue preserve unrelated content", async () =>
            {
                var repo = await Conflict(kind, "continue"); var context = await Context(repo);
                Expect(context.Repository.Recovery?.Kind == kind, "Reopened state selects actual Git operation");
                await Reject("CommitUnresolved", () => actions.ExecuteAsync(context, new(GitAction.Continue), default));
                await File.WriteAllTextAsync(Path.Combine(repo, "untracked.txt"), "preserve");
                await File.WriteAllTextAsync(Path.Combine(repo, "conflict.txt"), "resolved\n");
                var working = await actions.ReadWorkingCopyAsync(git, repo, default);
                Expect((await actions.ExecuteWorkingCopyAsync(working, new(WorkingCopyAction.Stage,
                    working.Changes.Entries.Where(entry => entry.Path == "conflict.txt").ToArray()), default)).Result.Success, "Stage resolution through real app action");
                context = await Context(repo);
                var watch = Stopwatch.StartNew();
                var result = await actions.ExecuteAsync(context, new(GitAction.Continue), default);
                Console.WriteLine($"Recovery {kind} validation/command/state: {watch.Elapsed.TotalMilliseconds:F0} ms");
                Expect(result.Success, "Continue completes: " + result.Output);
                Expect((await Context(repo)).Repository.Operation == RepositoryOperation.None && await Git(repo, "ls-files", "-u") == "", "No remaining sequence/conflicts");
                Expect(await File.ReadAllTextAsync(Path.Combine(repo, "conflict.txt")) == "resolved\n" &&
                    await File.ReadAllTextAsync(Path.Combine(repo, "untracked.txt")) == "preserve", "Resolved and unrelated bytes preserved");
                if (kind == RecoveryKind.Merge) Expect((await Git(repo, "rev-list", "--parents", "-1", "HEAD")).Split(' ').Length == 3, "Merge keeps both parents");
            });
            await Check($"Recovery {kind}: Abort returns original tip without automatic lock cleanup", async () =>
            {
                var repo = await Conflict(kind, "abort"); var context = await Context(repo);
                var original = context.Repository.Recovery!.Target is { Length: > 0 } target ? target : context.Repository.Commit;
                var lockPath = Path.Combine(context.Repository.GitDirectory, "index.lock"); await File.WriteAllTextAsync(lockPath, "fixture lock");
                var failed = await actions.ExecuteAsync(context, new(GitAction.Abort), default);
                Expect(!failed.Success && await File.ReadAllTextAsync(lockPath) == "fixture lock", "Lock failure remains visible; no cleanup");
                File.Delete(lockPath); // Only this test's own known lock, after asserting production preserved it.
                var result = await actions.ExecuteAsync(await Context(repo), new(GitAction.Abort), default);
                Expect(result.Success && await Git(repo, "rev-parse", "HEAD") == original, "Abort restores original tip: " + result.Output);
                Expect(await File.ReadAllTextAsync(Path.Combine(repo, "conflict.txt")) == "local\n", "Abort restores local committed content");
            });
            if (kind == RecoveryKind.Merge) continue;
            await Check($"Recovery {kind}: Skip really omits conflicted patch", async () =>
            {
                var repo = await Conflict(kind, "skip");
                var result = await actions.ExecuteAsync(await Context(repo), new(GitAction.Skip), default);
                Expect(result.Success, "Skip succeeds: " + result.Output);
                Expect(await File.ReadAllTextAsync(Path.Combine(repo, "conflict.txt")) == (kind == RecoveryKind.Rebase ? "incoming\n" : "local\n"), "Current patch omitted");
            });
        }
        await Check("Recovery rejects stale HEAD, changed sequence and missing/unsupported operation", async () =>
        {
            var repo = await Conflict(RecoveryKind.Merge, "stale"); var context = await Context(repo);
            await Reject("RecoveryUnavailable", () => actions.ExecuteAsync(context, new(GitAction.Skip), default));
            await Reject("RecoveryChanged", () => actions.ExecuteAsync(context with { Repository = context.Repository with { Commit = new string('0', 40) } }, new(GitAction.Abort), default));
            await File.AppendAllTextAsync(Path.Combine(repo, ".git", "MERGE_MSG"), "\nExternal change");
            await Reject("RecoveryChanged", () => actions.ExecuteAsync(context, new(GitAction.Abort), default));
            await actions.ExecuteAsync(await Context(repo), new(GitAction.Abort), default);
            await Reject("RecoveryChanged", () => actions.ExecuteAsync(context, new(GitAction.Continue), default));
            await Reject("RecoveryUnavailable", async () => await actions.ExecuteAsync(await Context(repo), new(GitAction.Continue), default));
        });
        await Check("Recovery failed commit hook keeps resolved index and sequence for retry", async () =>
        {
            var repo = await Conflict(RecoveryKind.Merge, "hook");
            await File.WriteAllTextAsync(Path.Combine(repo, "conflict.txt"), "resolved\n"); await Git(repo, "add", ".");
            var hook = Path.Combine(repo, ".git", "hooks", "pre-commit"); await File.WriteAllTextAsync(hook, "#!/bin/sh\necho recovery-hook-failed >&2\nexit 1\n", new UTF8Encoding(false));
            var result = await actions.ExecuteAsync(await Context(repo), new(GitAction.Continue), default);
            Expect(!result.Success && result.Output.Contains("recovery-hook-failed") && (await Context(repo)).Repository.Recovery is not null, "Hook honored and recovery retained");
            File.Delete(hook); Expect((await actions.ExecuteAsync(await Context(repo), new(GitAction.Continue), default)).Success, "Explicit retry succeeds");
        });
        await Check("Recovery resumes a multi-pick sequencer after external resolution removed CHERRY_PICK_HEAD", async () =>
        {
            var repo = await Conflict(RecoveryKind.CherryPick, "sequencer"); await Git(repo, "cherry-pick", "--abort");
            await Git(repo, "switch", "-c", "second", "incoming");
            await File.WriteAllTextAsync(Path.Combine(repo, "second.txt"), "second patch"); await Git(repo, "add", "."); await Git(repo, "commit", "-m", "second patch");
            await Git(repo, "switch", "main"); Expect((await Raw(repo, "cherry-pick", "incoming", "second")).ExitCode != 0, "Multi-pick stops");
            await File.WriteAllTextAsync(Path.Combine(repo, "conflict.txt"), "resolved\n"); await Git(repo, "add", "."); await Git(repo, "commit", "--no-edit");
            var context = await Context(repo);
            Expect(context.Repository.Operation == RepositoryOperation.Sequencer && context.Repository.Recovery?.Kind == RecoveryKind.CherryPick, "Sequencer todo identifies remaining operation");
            var result = await actions.ExecuteAsync(context, new(GitAction.Continue), default);
            Expect(result.Success && await File.ReadAllTextAsync(Path.Combine(repo, "second.txt")) == "second patch", "Continue applies remaining patch: " + result.Output);
        });
        await Check("Recovery Continue can stop at the next conflict and survive reopen", async () =>
        {
            var repo = await Conflict(RecoveryKind.Rebase, "next conflict"); await Git(repo, "rebase", "--abort");
            await Commit(repo, "second local"); Expect((await Raw(repo, "rebase", "incoming")).ExitCode != 0, "First rebase conflict");
            await File.WriteAllTextAsync(Path.Combine(repo, "conflict.txt"), "first resolution\n"); await Git(repo, "add", ".");
            var before = await Context(repo); var result = await actions.ExecuteAsync(before, new(GitAction.Continue), default);
            var after = await Context(repo);
            Expect(!result.Success && result.HasConflicts && after.Repository.Recovery?.Kind == RecoveryKind.Rebase &&
                before.Repository.Recovery != after.Repository.Recovery, "Next conflict is pending, not reported complete");
            Expect((await actions.ExecuteAsync(after, new(GitAction.Abort), default)).Success, "Reopened next conflict can abort");
        });
        await Check("Recovery auto-stash conflict after Continue is not false success", async () =>
        {
            var repo = await Conflict(RecoveryKind.Rebase, "autostash"); await Git(repo, "rebase", "--abort");
            await File.WriteAllTextAsync(Path.Combine(repo, "conflict.txt"), "uncommitted stash\n");
            Expect((await Raw(repo, "rebase", "--autostash", "incoming")).ExitCode != 0, "Rebase with stash stops");
            await File.WriteAllTextAsync(Path.Combine(repo, "conflict.txt"), "resolved\n"); await Git(repo, "add", ".");
            var result = await actions.ExecuteAsync(await Context(repo), new(GitAction.Continue), default);
            Expect(!result.Success && result.HasConflicts && result.Operation == RepositoryOperation.None &&
                (await Git(repo, "stash", "list")).Length > 0, "Stash conflicts retained even after successful sequence exit");
        });
        await Check("Recovery cancellation stops a real hook, holds shared mutation gate and preserves sequence", async () =>
        {
            var repo = await Conflict(RecoveryKind.Merge, "cancel hook");
            await File.WriteAllTextAsync(Path.Combine(repo, "conflict.txt"), "resolved\n"); await Git(repo, "add", ".");
            var hook = Path.Combine(repo, ".git", "hooks", "pre-commit");
            await File.WriteAllTextAsync(hook, "#!/bin/sh\necho recovery-waiting >&2\nsleep 20\n", new UTF8Encoding(false));
            var context = await Context(repo); using var cancel = new CancellationTokenSource(TimeSpan.FromSeconds(15));
            var started = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
            var running = actions.ExecuteAsync(context, new(GitAction.Continue), cancel.Token,
                output => { if (Encoding.UTF8.GetString(output.Bytes).Contains("recovery-waiting")) started.TrySetResult(); });
            await started.Task.WaitAsync(TimeSpan.FromSeconds(12));
            await Reject("ActionOtherInstance", () => actions.ExecuteAsync(context, new(GitAction.Abort), default));
            cancel.Cancel(); var result = await running.WaitAsync(TimeSpan.FromSeconds(12));
            Expect(result.Outcome == ProcessOutcome.Cancelled && !result.Success && (await Context(repo)).Repository.Recovery is not null, "Cancellation keeps recoverable state");
            File.Delete(hook);
        });
        await Check("Recovery shell controls are cached, confirmation-free Continue, targeted Skip/Abort and cancellable", async () =>
        {
            var repo = await Conflict(RecoveryKind.Rebase, "shell");
            var gate = new RecoveryGateRunner(runner); var observed = new RepositoryReader(gate); var interaction = new RecoveryInteraction();
            var shell = new ShellViewModel(observed, interaction, identity, actions: new GitActions(gate, observed, identity));
            shell.CompleteStartup(new(new() { RepositoryPaths = [repo], SelectedRepositoryPath = repo }, SettingsLoadStatus.Loaded));
            await shell.WaitForIdleAsync();
            Expect(shell.HasRecovery && shell.CanSkipRecovery && shell.ContinueCommand.CanExecute(null), "Restored tab exposes recovery immediately after history");
            var calls = gate.Calls; var window = new MainWindow(shell);
            try
            {
                var content = (FrameworkElement)window.Content; var watch = Stopwatch.StartNew();
                Arrange(content, 1200, 760); await Dispatcher.Yield(DispatcherPriority.ContextIdle);
                Console.WriteLine($"Recovery layout (headless): {watch.Elapsed.TotalMilliseconds:F0} ms; opening Git calls: {gate.Calls - calls}");
                SaveRender(content, "recovery.png", 1200, 760);
                Arrange(content, 900, 580); await Dispatcher.Yield(DispatcherPriority.ContextIdle); SaveRender(content, "recovery-minimum.png", 900, 580);
                Expect(gate.Calls == calls, "Rendering recovery starts zero Git calls");
                Expect(VisualDescendants<Button>(content).Single(button => button.Command == shell.ContinueCommand).IsEnabled, "Bound recovery control usable");
                shell.AbortCommand.Execute(null); await shell.ActionTask;
                Expect(gate.Calls == calls && interaction.Confirmation.Contains(repo) && interaction.Confirmation.Contains(shell.SelectedRepository!.Snapshot!.Recovery!.Target), "Declining actual-target Abort runs no Git");
                var realSnapshot = shell.SelectedRepository!.Snapshot!;
                shell.SelectedRepository.Apply(realSnapshot with { Recovery = realSnapshot.Recovery! with { Stamp = "old displayed sequence" } });
                interaction.Allow = true; interaction.OnConfirm = () => shell.SelectedRepository.Apply(realSnapshot);
                shell.AbortCommand.Execute(null); await shell.ActionTask;
                Expect(shell.CurrentAction?.Status == UiText.Get("RecoveryChanged") && shell.HasRecovery, "Confirmation freezes reviewed sequence even if snapshot refreshes inside modal dispatcher");
                interaction.OnConfirm = null; interaction.Allow = false;
                shell.ContinueCommand.Execute(null); await shell.ActionTask;
                Expect(shell.CurrentAction?.Status == UiText.Get("CommitUnresolved") && shell.HasRecovery, "Unresolved Continue retains recovery/output with no approval modal");
                interaction.Allow = true; gate.Hold = true;
                shell.SkipCommand.Execute(null); await gate.Started.Task.WaitAsync(TimeSpan.FromSeconds(20));
                Expect(shell.IsActionRunning && shell.CancelCommand.CanExecute(null) && !shell.ContinueCommand.CanExecute(null), "Progress/cancel and duplicate admission gate");
                shell.CancelCommand.Execute(null); await shell.ActionTask.WaitAsync(TimeSpan.FromSeconds(30));
                Expect(shell.HasRecovery && shell.CurrentAction is { Succeeded: false, IsRunning: false }, "Cancelled write retains sequence after reinspection");
                gate.Hold = false; shell.AbortCommand.Execute(null); await shell.ActionTask;
                Expect(shell.CurrentAction?.Succeeded == true && !shell.HasRecovery, "Real bound Abort updates history and hides recovery controls");
                Expect((await Raw(repo, "rebase", "incoming")).ExitCode != 0, "Another external rebase stops");
                shell.RefreshRepositoryCommand.Execute(null); await shell.WaitForIdleAsync();
                await File.WriteAllTextAsync(Path.Combine(repo, "conflict.txt"), "resolved\n"); await Git(repo, "add", ".");
                shell.ContinueCommand.Execute(null); await shell.ActionTask;
                Expect(shell.CurrentAction?.Succeeded == true && !shell.HasRecovery && shell.SelectedRepository!.Snapshot!.Branch == "main",
                    "Bound Continue after external resolution completes and restores attached branch");
            }
            finally { window.Close(); shell.BeginShutdown(); await shell.WaitForIdleAsync(); }
        });
        await Check("Recovery successful command remains successful when later history refresh fails", async () =>
        {
            var repo = await Conflict(RecoveryKind.Merge, "refresh failure");
            var model = new GitActionViewModel(await Context(repo), GitAction.Abort, actions, () => throw new IOException());
            model.RunCommand.Execute(null); await model.RunningTask;
            Expect(model.Succeeded && model.Phase == UiText.Get("ActionRefreshFailedShort") && !model.IsRunning &&
                (await Context(repo)).Repository.Operation == RepositoryOperation.None, "Git success and refresh failure remain distinct");
        });
    }

    private sealed class RecoveryInteraction : IUserInteraction
    {
        public bool Allow; public string Confirmation = ""; public Action? OnConfirm;
        public string? ChooseRepository() => null;
        public string? ChooseGit() => null;
        public bool ConfirmRecovery(string message, string title) { Confirmation = message; OnConfirm?.Invoke(); return Allow; }
        public void ShowGitAction(GitActionViewModel model) => throw new InvalidOperationException("Recovery must not open options");
    }
    private sealed class RecoveryGateRunner(IProcessRunner inner) : IProcessRunner
    {
        public int Calls; public bool Hold;
        public TaskCompletionSource Started { get; } = new(TaskCreationOptions.RunContinuationsAsynchronously);
        public async Task<ProcessResult> RunAsync(ProcessRequest request, CancellationToken token, Action<ProcessOutput>? progress = null)
        {
            Interlocked.Increment(ref Calls);
            if (Hold && request.Arguments.Contains("--skip"))
            { Started.TrySetResult(); await Task.Delay(Timeout.Infinite, token); }
            return await inner.RunAsync(request, token, progress);
        }
    }
}
