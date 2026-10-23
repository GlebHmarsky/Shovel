using System.IO;
using System.Text;
using System.Windows.Threading;
using GitGui.App.Services;
using GitGui.App.ViewModels;
using GitGui.Core.Git;
using GitGui.Core.Processes;
using GitGui.Core.Settings;

namespace GitGui.FoundationChecks;

internal static partial class Program
{
    private static void RunViewModelChecks()
    {
        // Pump a real dispatcher for async view-model continuations, without opening a window.
        var dispatcher = Dispatcher.CurrentDispatcher;
        var priorContext = SynchronizationContext.Current;
        SynchronizationContext.SetSynchronizationContext(new DispatcherSynchronizationContext(dispatcher));
        try
        {
            var task = RunAsync();
            var frame = new DispatcherFrame();
            _ = task.ContinueWith(_ => dispatcher.BeginInvoke(() => frame.Continue = false), TaskScheduler.Default);
            Dispatcher.PushFrame(frame);
            task.GetAwaiter().GetResult();
        }
        finally { SynchronizationContext.SetSynchronizationContext(priorContext); }

        async Task RunAsync()
        {
            var first = Fixture("model-first");
            var second = Fixture("model-second");
            await Check("Restored tab order/selection never grants trust or runs Git", () =>
            {
                var runner = new ModelRunner(first);
                var interaction = new ModelInteraction(first);
                var model = new ShellViewModel(new(runner), interaction);
                model.CompleteStartup(new(new() { RepositoryPaths = [first, second], SelectedRepositoryPath = second }, SettingsLoadStatus.Loaded));
                Expect(runner.Calls == 0 && interaction.TrustRequests == 0 && model.SelectedRepository?.Directory == second, "Restoring a profile must not execute Git");
                model.MoveLeftCommand.Execute(null);
                var saved = model.CaptureSession(new());
                Expect(saved.RepositoryPaths.SequenceEqual([second, first]) && saved.SelectedRepositoryPath == second, "Tab movement persists order and selection");
                model.CloseRepositoryCommand.Execute(null);
                Expect(model.SelectedRepository?.Directory == first, "Closing a tab selects an existing tab");
                return Task.CompletedTask;
            });
            await Check("Declining repository trust performs no executable discovery or repository read", () =>
            {
                var runner = new ModelRunner(first);
                var interaction = new ModelInteraction(first);
                var model = new ShellViewModel(new(runner), interaction);
                model.CompleteStartup(new(new(), SettingsLoadStatus.Defaults));
                model.OpenRepositoryCommand.Execute(null);
                model.RefreshRepositoryCommand.Execute(null);
                Expect(interaction.TrustRequests == 2 && runner.Calls == 0 && !model.IsBusy && model.SelectedRepository?.Snapshot is null, "Denied trust must not be remembered as consent");
                return Task.CompletedTask;
            });
            await Check("Shutdown cancels active reads and rejects new work before saving", async () =>
            {
                var runner = new ModelRunner(first);
                var model = CreateModel(runner, first);
                model.OpenRepositoryCommand.Execute(null);
                await runner.Started.Task.WaitAsync(TimeSpan.FromSeconds(5));
                Expect(model.IsBusy && !model.OpenRepositoryCommand.CanExecute(null), "Only one UI inspection may be admitted");
                model.BeginShutdown();
                await model.WaitForIdleAsync().WaitAsync(TimeSpan.FromSeconds(5));
                Expect(runner.Cancelled && !model.IsReady && !model.IsBusy && model.SelectedRepository?.Snapshot is null && !model.RefreshRepositoryCommand.CanExecute(null), "Shutdown must drain the cancelled read without applying state");
            });
            await Check("A late read cannot populate a tab after selection generation changes", async () =>
            {
                var runner = new ModelRunner(first);
                var model = CreateModel(runner, first);
                model.OpenRepositoryCommand.Execute(null);
                await runner.Started.Task.WaitAsync(TimeSpan.FromSeconds(5));
                var oldTab = model.SelectedRepository!;
                var otherTab = new RepositoryTab(second);
                model.Repositories.Add(otherTab);
                model.SelectedRepository = otherTab;
                runner.Release.TrySetResult();
                await model.WaitForIdleAsync().WaitAsync(TimeSpan.FromSeconds(5));
                Expect(oldTab.Snapshot is null && otherTab.Snapshot is null && !model.IsBusy, "Discard obsolete read completion");
            });
        }
    }

    private static ShellViewModel CreateModel(ModelRunner runner, string directory)
    {
        var model = new ShellViewModel(new(runner), new ModelInteraction(directory) { Trust = true });
        model.CompleteStartup(new(new(), SettingsLoadStatus.Defaults));
        model.ChooseGitCommand.Execute(null);
        return model;
    }

    private sealed class ModelInteraction(string directory) : IUserInteraction
    {
        public bool Trust { get; init; }
        public int TrustRequests { get; private set; }
        public string? ChooseRepository() => directory;
        public string? ChooseGit() => ProbeExecutable;
        public bool ConfirmTrust(string _) { TrustRequests++; return Trust; }
    }

    // Test-only transport: real Git semantics are covered separately in disposable-repository checks.
    private sealed class ModelRunner(string directory) : IProcessRunner
    {
        public TaskCompletionSource Started { get; } = new(TaskCreationOptions.RunContinuationsAsynchronously);
        public TaskCompletionSource Release { get; } = new(TaskCreationOptions.RunContinuationsAsynchronously);
        public int Calls;
        public bool Cancelled;
        public async Task<ProcessResult> RunAsync(ProcessRequest request, CancellationToken cancellationToken, Action<ProcessOutput>? output = null)
        {
            Interlocked.Increment(ref Calls);
            Started.TrySetResult();
            try { await Release.Task.WaitAsync(cancellationToken); }
            catch (OperationCanceledException) { Cancelled = true; return new(ProcessOutcome.Cancelled, null, [], [], TimeSpan.Zero); }
            var args = request.Arguments;
            var value = args.Contains("--version") ? "git version 2.51.1" :
                args.Contains("--is-inside-work-tree") ? "true" :
                args.Contains("symbolic-ref") ? "main" : args.Contains("--verify") ? "abcdef" : directory;
            return new(ProcessOutcome.Exited, 0, Encoding.UTF8.GetBytes(value + "\n"), [], TimeSpan.Zero);
        }
    }
}
