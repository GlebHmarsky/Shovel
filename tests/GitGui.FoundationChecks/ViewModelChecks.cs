using System.IO;
using System.Text;
using System.Windows.Threading;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Media;
using System.Windows.Media.Imaging;
using GitGui.App.Views;
using GitGui.App.Localization;
using GitGui.App.Services;
using GitGui.App.ViewModels;
using GitGui.Core.Git;
using GitGui.Core.Processes;
using GitGui.Core.Settings;

namespace GitGui.FoundationChecks;

internal static partial class Program
{
    private static void RunViewModelChecks(bool localOnly = false, bool commitOnly = false, bool hunksOnly = false, bool commitFilesOnly = false, bool fastChangesOnly = false, bool recoveryOnly = false, bool stashesOnly = false, bool fileMenusOnly = false, bool fileActionsOnly = false, bool initOnly = false, bool branchManagementOnly = false, bool draftsOnly = false, bool routineOnly = false)
    {
        // Pump a real dispatcher for async view-model continuations, without opening a window.
        var dispatcher = Dispatcher.CurrentDispatcher;
        var priorContext = SynchronizationContext.Current;
        var priorGlobal = Environment.GetEnvironmentVariable("GIT_CONFIG_GLOBAL");
        var priorSystem = Environment.GetEnvironmentVariable("GIT_CONFIG_NOSYSTEM");
        var emptyConfig = Path.Combine(Fixture("view-model-config"), "empty.config");
        File.WriteAllText(emptyConfig, "");
        Environment.SetEnvironmentVariable("GIT_CONFIG_GLOBAL", emptyConfig);
        Environment.SetEnvironmentVariable("GIT_CONFIG_NOSYSTEM", "1");
        SynchronizationContext.SetSynchronizationContext(new DispatcherSynchronizationContext(dispatcher));
        try
        {
            var task = RunAsync();
            var frame = new DispatcherFrame();
            // Complete on the dispatcher that owns this frame, including faulted test tasks.
            // A bounded watchdog makes a harness/lifetime regression fail instead of hanging packaging.
            // The full aggregate includes recovery, stashes and extended file fixtures; keep individual
            // operation timeouts unchanged while allowing their sequential total to finish.
            var focused = localOnly || commitOnly || hunksOnly || commitFilesOnly || fastChangesOnly || recoveryOnly || stashesOnly || fileMenusOnly || fileActionsOnly || initOnly || branchManagementOnly || draftsOnly;
            var watchdog = new DispatcherTimer { Interval = TimeSpan.FromMinutes(focused ? 8 : 20) };
            watchdog.Tick += (_, _) => { watchdog.Stop(); frame.Continue = false; };
            watchdog.Start();
            async Task CompleteFrameAsync() { try { await task; } finally { frame.Continue = false; } }
            var completion = CompleteFrameAsync();
            Dispatcher.PushFrame(frame);
            watchdog.Stop();
            Expect(task.IsCompleted, "View-model checks exceeded the dispatcher deadline");
            completion.GetAwaiter().GetResult();
            task.GetAwaiter().GetResult();
        }
        finally
        {
            SynchronizationContext.SetSynchronizationContext(priorContext);
            Environment.SetEnvironmentVariable("GIT_CONFIG_GLOBAL", priorGlobal);
            Environment.SetEnvironmentVariable("GIT_CONFIG_NOSYSTEM", priorSystem);
        }

        async Task RunAsync()
        {
            if (_networkWorkflowChecks) { await RunNetworkWorkflowChecksAsync(); return; }
            if (_qolChecks) { await RunQolChecksAsync(); return; }
            if (_lfsChecks) { await RunLfsChecksAsync(); return; }
            if (_conflictChecks) { await RunConflictChecksAsync(); return; }
            if (_usabilityChecks) { await RunUsabilityChecksAsync(); return; }
            if (_historyActionChecks) { await RunHistoryActionChecksAsync(); return; }
            if (_defaultPullChecks) { await RunDefaultPullChecksAsync(); return; }
            if (_draftIoChecks) { await RunDraftIoChecksAsync(); return; }
            if (_workspaceCleanupChecks) { await RunWorkspaceCleanupChecksAsync(); return; }
            if (_historyLayoutChecks) { await RunHistoryLayoutChecksAsync(); return; }
            if (_operationStyleChecks) { await RunOperationStyleChecksAsync(); return; }
            if (_workspaceChecks) { await RunWorkspaceChecksAsync(); return; }
            if (_sidebarChecks) { await RunSidebarChecksAsync(); return; }
            if (routineOnly) { await RunRoutineScenariosAsync(); return; }
            if (branchManagementOnly) { await RunBranchManagementChecksAsync(); return; }
            if (draftsOnly) { await RunCommitDraftChecksAsync(); return; }
            if (initOnly) { await RunInitializationChecksAsync(); return; }
            if (fileActionsOnly) { await RunFileActionChecksAsync(); return; }
            if (fileMenusOnly) { await RunFileMenuChecksAsync(); return; }
            if (stashesOnly) { await RunStashChecksAsync(); return; }
            if (recoveryOnly) { await RunRecoveryChecksAsync(); return; }
            if (localOnly) { await RunLocalChangesChecksAsync(); return; }
            if (commitOnly) { await RunCommitWorkspaceChecksAsync(); return; }
            if (hunksOnly) { await RunHunkChecksAsync(); return; }
            if (commitFilesOnly) { await RunCommitFilesChecksAsync(); return; }
            if (fastChangesOnly) { await RunFastChangesChecksAsync(); return; }
            var first = Fixture("model-first");
            var second = Fixture("model-second");
            await Check("Restored tabs automatically load without approval and preserve order/selection", async () =>
            {
                var runner = new ModelRunner(first) { UseRequestDirectory = true }; runner.Release.TrySetResult();
                var interaction = new ModelInteraction(first);
                var model = new ShellViewModel(new(runner), interaction, new ModelIdentities());
                model.CompleteStartup(new(new() { RepositoryPaths = [first, second], SelectedRepositoryPath = second }, SettingsLoadStatus.Loaded));
                await model.WaitForIdleAsync();
                Expect(model.Commits.Count == 1 && model.SelectedRepository?.Directory == second && model.Repositories[0].Snapshot is null, "Only selected restored tab reads, without a question");
                model.MoveLeftCommand.Execute(null);
                var saved = model.CaptureSession(new());
                Expect(saved.RepositoryPaths.SequenceEqual([second, first]) && saved.SelectedRepositoryPath == second, "Tab movement persists order and selection");
                model.CloseRepositoryCommand.Execute(null); await model.WaitForIdleAsync();
                Expect(model.SelectedRepository?.Directory == first && model.Commits.Count == 1, "Closing selects and loads a neighbor");
            });
            await Check("Explicit Open displays local history without an approval interaction", async () =>
            {
                var runner = new ModelRunner(first); runner.Release.TrySetResult();
                var interaction = new ModelInteraction(first);
                var model = new ShellViewModel(new(runner), interaction, new ModelIdentities());
                model.CompleteStartup(new(new(), SettingsLoadStatus.Defaults));
                model.OpenRepositoryCommand.Execute(null); await model.WaitForIdleAsync();
                Expect(runner.Calls > 0 && model.Commits.Count == 1 && !model.IsBusy, "Opening means reading local history");
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
                var runner = new ModelRunner(first) { UseRequestDirectory = true };
                var model = CreateModel(runner, first);
                model.OpenRepositoryCommand.Execute(null);
                await runner.Started.Task.WaitAsync(TimeSpan.FromSeconds(5));
                var oldTab = model.SelectedRepository!;
                var otherTab = new RepositoryTab(second);
                model.Repositories.Add(otherTab);
                model.SelectedRepository = otherTab;
                runner.Release.TrySetResult();
                await model.WaitForIdleAsync().WaitAsync(TimeSpan.FromSeconds(5));
                Expect(oldTab.Snapshot is null && otherTab.Snapshot?.Worktree == second && !model.IsBusy, "Discard obsolete read completion");
            });
            await Check("Late history completion cannot leak rows into another repository tab", async () =>
            {
                var runner = new ModelRunner(first) { HoldHistory = true, IgnoreHistoryCancellation = true };
                runner.Release.TrySetResult();
                var model = CreateModel(runner, first);
                model.OpenRepositoryCommand.Execute(null);
                await runner.HistoryStarted.Task.WaitAsync(TimeSpan.FromSeconds(5));
                model.SelectedRepository = new RepositoryTab(second);
                runner.ReleaseHistory.TrySetResult();
                await model.WaitForIdleAsync().WaitAsync(TimeSpan.FromSeconds(5));
                Expect(model.Commits.Count == 0 && model.SelectedCommit is null && !model.LoadMoreHistoryCommand.CanExecute(null), "History belongs to its captured selection generation");
            });
            if (_historyRepository is not null && _historyExecutable is not null)
            {
                await Check("Long row previews are bounded while selected details retain the full message", () =>
                {
                    var commit = new CommitRecord("abcdef", [], new string('a', 1000), "a@example.invalid", DateTimeOffset.UnixEpoch,
                        "C", "c@example.invalid", DateTimeOffset.UnixEpoch, new string('x', 100000));
                    var row = new CommitRow(new(commit, CommitGraph.Build([commit])[0]));
                    Expect(row.Description.Length <= 513 && row.Author.Length <= 257 && row.MessagePreview.Length == 100000 &&
                        row.MessageNotice.Length == 0 && row.Commit.Message.Length == 100000, "Bound visible text and retain original message in memory");
                    return Task.CompletedTask;
                });
                await Check("Real history binds graph/description columns and commit details without showing a window", async () =>
                {
                    var model = await LoadRealModel(_historyRepository);
                    Expect(model.Commits.Count == 4 && model.SelectedCommit?.Commit.Parents.Count == 2, "Real merge history must populate the view model");
                    model.SelectedCommit = model.Commits.Last();
                    Expect(model.SelectedCommit.Commit.Message.Contains("Body with") && model.SelectedCommit.Parents == UiText.Get("RootCommit"), "Row selection exposes the corresponding full message and parents");
                    var window = new MainWindow(model);
                    try
                    {
                        var content = (FrameworkElement)window.Content;
                        Arrange(content, 1200, 760);
                        await Dispatcher.Yield(DispatcherPriority.ContextIdle);
                        content.UpdateLayout();
                        var table = (DataGrid)window.FindName("HistoryTable");
                        Expect(table.Columns.Count == 5 && table.Columns[0].Header as string == UiText.Get("HistoryGraph") && table.Columns[1].Header as string == UiText.Get("HistoryDescription"), "Graph precedes description in the table");
                        Expect(!table.CanUserSortColumns && table.IsReadOnly && table.EnableRowVirtualization, "Topology cannot be reordered by sorting/editing");
                        var cells = VisualDescendants<CommitGraphCell>(content).ToArray();
                        Expect(cells.Length > 0 && cells.Any(cell => cell.Row?.IsMerge == true), "Real graph cells must be generated");
                        SaveRender(content, "history-table.png", 1200, 760);
                        Arrange(content, 900, 580);
                        await Dispatcher.Yield(DispatcherPriority.ContextIdle);
                        content.UpdateLayout();
                        Expect(table.ActualHeight >= 120 && table.ActualWidth > 300, "History remains allocated at minimum shell size");
                        Expect(table.Columns[0].ActualWidth >= model.GraphWidth && table.Columns[3].ActualWidth >= 150, "Narrow windows scroll instead of clipping graph lanes or date columns");
                        SaveRender(content, "history-minimum.png", 900, 580);
                    }
                    finally { window.Close(); }
                });
                await Check("All Commits fills the workspace without scope controls and retains real labels/details", async () =>
                {
                    var model = await LoadRealModel(_allHistoryRepository!);
                    Expect(model.Commits.Count == 7, "Default must include every branch and tag tip");
                    Expect(model.Commits.Any(row => row.RefLabels.Contains("feature-日本語")) && model.Commits.Any(row => row.RefLabels.Contains("origin/remote-only")), "Local and remote labels are visible text");
                    model.SelectedCommit = model.Commits.Single(row => row.Commit.Subject == "Tag-only work");
                    Expect(model.SelectedCommit.MessagePreview.Length > 100000 && model.SelectedCommit.RefLabels.Contains("release"), "Selected tag commit exposes full message and labels");
                    var window = new MainWindow(model);
                    try
                    {
                        var content = (FrameworkElement)window.Content;
                        Arrange(content, 1200, 760);
                        await Dispatcher.Yield(DispatcherPriority.ContextIdle);
                                                Expect(!VisualDescendants<ComboBox>(content).Any(), "No prototype filter or scope selector consumes history space");
                        var table = VisualDescendants<DataGrid>(content).Single(grid => System.Windows.Automation.AutomationProperties.GetAutomationId(grid) == "CommitHistory");
                        var tabs = VisualDescendants<ListBox>(content).Single(list => list.ItemsSource == model.Repositories);
                        var tableTop = table.TranslatePoint(new Point(0, 0), content).Y;
                        var tabsBottom = tabs.TranslatePoint(new Point(0, tabs.ActualHeight), content).Y;
                        Expect(Math.Abs(tableTop - tabsBottom) <= 1, "History begins immediately below repository tabs");
                        SaveRender(content, "all-commits.png", 1200, 760);
                        Arrange(content, 900, 580);
                        await Dispatcher.Yield(DispatcherPriority.ContextIdle);
                        SaveRender(content, "all-commits-minimum.png", 900, 580);
                    }
                    finally { window.Close(); }
                });
                await Check("Failed, cancelled and changed next-page reads retain existing rows", async () =>
                {
                    var runner = new PageFailureRunner();
                    var model = new ShellViewModel(new(runner), new ModelInteraction(_longHistoryRepository!) { Executable = _historyExecutable });
                    model.CompleteStartup(new(new(), SettingsLoadStatus.Defaults)); model.ChooseGitCommand.Execute(null); model.OpenRepositoryCommand.Execute(null);
                    await model.WaitForIdleAsync().WaitAsync(TimeSpan.FromSeconds(30));
                    var firstHash = model.Commits.First().Commit.Hash;
                    foreach (var outcome in new[] { ProcessOutcome.OutputLimit, ProcessOutcome.Cancelled, ProcessOutcome.TimedOut })
                    {
                        runner.Failure = outcome;
                        model.LoadMoreHistoryCommand.Execute(null);
                        await model.WaitForIdleAsync().WaitAsync(TimeSpan.FromSeconds(30));
                        Expect(model.Commits.Count == 200 && model.Commits.First().Commit.Hash == firstHash && model.LoadMoreHistoryCommand.CanExecute(null), "Failure must not erase rows or claim completion");
                    }
                    runner.Failure = null; runner.ChangeRefs = true;
                    model.LoadMoreHistoryCommand.Execute(null);
                    await model.WaitForIdleAsync().WaitAsync(TimeSpan.FromSeconds(30));
                    Expect(model.Commits.Count == 200 && !model.LoadMoreHistoryCommand.CanExecute(null) && model.Status == UiText.Get("GitErrorChangedDuringRead"), "Changed ref snapshot blocks navigation until refresh");
                });
                await Check("History expands incrementally and WPF realizes only visible rows", async () =>
                {
                    var model = await LoadRealModel(_longHistoryRepository!);
                    Expect(model.Commits.Count == 200 && model.LoadMoreHistoryCommand.CanExecute(null), "Initial page and load-more command");
                    var selection = model.Commits[4].Commit.Hash;
                    model.SelectedCommit = model.Commits[4];
                    model.LoadMoreHistoryCommand.Execute(null);
                    await model.WaitForIdleAsync().WaitAsync(TimeSpan.FromSeconds(30));
                    Expect(model.Commits.Count == 200 && model.Commits.All(row => row.Commit.Hash != selection) && model.HistoryRange.Contains("400"), "Older navigation replaces the page and reports its range");
                    model.NewerHistoryCommand.Execute(null);
                    await model.WaitForIdleAsync().WaitAsync(TimeSpan.FromSeconds(30));
                    Expect(model.SelectedCommit?.Commit.Hash == selection && !model.NewerHistoryCommand.CanExecute(null), "Newer navigation returns to the first page");
                    var window = new MainWindow(model);
                    try
                    {
                        var content = (FrameworkElement)window.Content;
                        Arrange(content, 1200, 760);
                        var realized = VisualDescendants<DataGridRow>(content).Count();
                        Expect(realized > 0 && realized < 60, "Do not allocate one WPF row for every loaded commit");
                    }
                    finally { window.Close(); }
                });
            }

            await RunAutomaticHistoryChecksAsync();
            await RunTabInteractionChecksAsync();
            await RunDivergenceViewChecksAsync();
            await RunActionViewChecksAsync();
            await RunLocalChangesChecksAsync();
            await RunCommitWorkspaceChecksAsync();
            await RunHunkChecksAsync();
            await RunCommitFilesChecksAsync();
            await RunFastChangesChecksAsync();
            await RunWorkspaceActionChecksAsync();
            await RunPullChecksAsync();
            await RunRecoveryChecksAsync();
            await RunStashChecksAsync();
            await RunFileMenuChecksAsync();
            await RunFileActionChecksAsync();
            await RunInitializationChecksAsync();
            await RunBranchManagementChecksAsync();
            await RunCommitDraftChecksAsync();
            await RunAuthenticationViewChecksAsync();

            async Task<ShellViewModel> LoadRealModel(string path)
            {
                var model = new ShellViewModel(new(new GitGui.ProcessHost.WindowsProcessRunner()),
                    new ModelInteraction(path) { Executable = _historyExecutable });
                model.CompleteStartup(new(new(), SettingsLoadStatus.Defaults));
                model.ChooseGitCommand.Execute(null);
                model.OpenRepositoryCommand.Execute(null);
                await model.WaitForIdleAsync().WaitAsync(TimeSpan.FromSeconds(30));
                return model;
            }
        }
    }

    private static void Arrange(FrameworkElement content, int width, int height)
    {
        content.Measure(new Size(width, height)); content.Arrange(new Rect(0, 0, width, height)); content.UpdateLayout();
    }
    private static void SaveRender(FrameworkElement content, string name, int width, int height)
    {
        var image = new RenderTargetBitmap(width, height, 96, 96, PixelFormats.Pbgra32);
        image.Render(content);
        var encoder = new PngBitmapEncoder(); encoder.Frames.Add(BitmapFrame.Create(image));
        using var file = File.Create(Path.Combine(_root, name)); encoder.Save(file);
    }
    private static IEnumerable<T> VisualDescendants<T>(DependencyObject parent) where T : DependencyObject
    {
        for (var i = 0; i < VisualTreeHelper.GetChildrenCount(parent); i++)
        {
            var child = VisualTreeHelper.GetChild(parent, i);
            if (child is T match) yield return match;
            foreach (var descendant in VisualDescendants<T>(child)) yield return descendant;
        }
    }

    private static ShellViewModel CreateModel(ModelRunner runner, string directory)
    {
        var model = new ShellViewModel(new(runner), new ModelInteraction(directory), new ModelIdentities());
        model.CompleteStartup(new(new(), SettingsLoadStatus.Defaults));
        model.ChooseGitCommand.Execute(null);
        return model;
    }

    private sealed class ModelIdentities : IRepositoryIdentityProvider
    {
        public Task<RepositoryIdentity> ReadAsync(string path, CancellationToken token) => Task.FromResult(new RepositoryIdentity(path, path, path, new string('A', 64)));
    }
    private sealed class ModelInteraction(string directory) : IUserInteraction
    {
        public string? Executable { get; init; }
        public string? ChooseRepository() => directory;
        public string? ChooseGit() => Executable ?? ProbeExecutable;
    }

    private sealed class PageFailureRunner : IProcessRunner
    {
        private readonly IProcessRunner _inner = new GitGui.ProcessHost.WindowsProcessRunner();
        public ProcessOutcome? Failure { get; set; }
        public bool ChangeRefs { get; set; }
        public async Task<ProcessResult> RunAsync(ProcessRequest request, CancellationToken token, Action<ProcessOutput>? output = null)
        {
            if (request.Arguments.Contains("--skip=200") && Failure is { } failure)
                return new(failure, null, [], Encoding.UTF8.GetBytes("fixture failure"), TimeSpan.Zero);
            var result = await _inner.RunAsync(request, token, output);
            if (ChangeRefs && request.Arguments.Contains("for-each-ref"))
                return result with { StandardOutput = [.. result.StandardOutput, .. Encoding.UTF8.GetBytes("refs/heads/external\0abcdef\0commit\0\0\0\0\n")] };
            return result;
        }
    }

    // Test-only transport: real Git semantics are covered separately in disposable-repository checks.
    private sealed class ModelRunner(string directory) : IProcessRunner
    {
        public TaskCompletionSource Started { get; } = new(TaskCreationOptions.RunContinuationsAsynchronously);
        public TaskCompletionSource Release { get; } = new(TaskCreationOptions.RunContinuationsAsynchronously);
        public TaskCompletionSource HistoryStarted { get; } = new(TaskCreationOptions.RunContinuationsAsynchronously);
        public TaskCompletionSource ReleaseHistory { get; } = new(TaskCreationOptions.RunContinuationsAsynchronously);
        public bool HoldHistory { get; init; }
        public bool UseRequestDirectory { get; set; }
        public bool IgnoreHistoryCancellation { get; init; }
        public bool HasUpstream { get; init; }
        public bool HoldDivergence { get; init; }
        public bool FailDivergence { get; set; }
        public bool IgnoreDivergenceCancellation { get; init; }
        public TaskCompletionSource DivergenceStarted { get; } = new(TaskCreationOptions.RunContinuationsAsynchronously);
        public TaskCompletionSource ReleaseDivergence { get; } = new(TaskCreationOptions.RunContinuationsAsynchronously);
        public int Calls;
        public bool Cancelled;
        public async Task<ProcessResult> RunAsync(ProcessRequest request, CancellationToken cancellationToken, Action<ProcessOutput>? output = null)
        {
            Interlocked.Increment(ref Calls);
            Started.TrySetResult();
            try { await Release.Task.WaitAsync(cancellationToken); }
            catch (OperationCanceledException) { Cancelled = true; return new(ProcessOutcome.Cancelled, null, [], [], TimeSpan.Zero); }
            var args = request.Arguments;
            if (args.Contains("remote")) return new(ProcessOutcome.Exited, 0, Encoding.UTF8.GetBytes("origin\n"), [], TimeSpan.Zero);
            if (args.Contains("rev-list"))
            {
                DivergenceStarted.TrySetResult();
                if (HoldDivergence) await ReleaseDivergence.Task.WaitAsync(IgnoreDivergenceCancellation ? CancellationToken.None : cancellationToken);
                return FailDivergence ? new(ProcessOutcome.Exited, 128, [], Encoding.UTF8.GetBytes("comparison fixture failure"), TimeSpan.Zero)
                    : new(ProcessOutcome.Exited, 0, Encoding.UTF8.GetBytes("3\t2\n"), [], TimeSpan.Zero);
            }
            if (args.Contains("log"))
            {
                HistoryStarted.TrySetResult();
                if (HoldHistory) await ReleaseHistory.Task.WaitAsync(IgnoreHistoryCancellation ? CancellationToken.None : cancellationToken);
                var row = string.Join('\0', "abcdef", "", "A", "a@example.invalid", "2026-10-23T10:00:00+00:00", "C", "c@example.invalid", "2026-10-23T10:00:00+00:00", "Subject\n") + '\0';
                return new(ProcessOutcome.Exited, 0, Encoding.UTF8.GetBytes(row), [], TimeSpan.Zero);
            }
            if (args.Contains("for-each-ref")) return new(ProcessOutcome.Exited, 0,
                Encoding.UTF8.GetBytes("HEAD\0abcdef\0commit\0\0\0refs/heads/main\0\nrefs/heads/main\0abcdef\0commit\0\0\0\0" +
                    (HasUpstream ? "refs/remotes/origin/main\nrefs/remotes/origin/main\0bcdefa\0commit\0\0\0\0\n" : "\n")), [], TimeSpan.Zero);
            var location = UseRequestDirectory ? request.WorkingDirectory : directory;
            var value = args.Contains("--version") ? "git version 2.51.1" :
                args.Contains("--is-shallow-repository") ? "false" :
                args.Contains("--is-inside-work-tree") ? "true\n" + location + "\n" + location + "\n" + location :
                args.Contains("symbolic-ref") ? "main" : args.Contains("--verify") ? "abcdef" : directory;
            return new(ProcessOutcome.Exited, 0, Encoding.UTF8.GetBytes(value + "\n"), [], TimeSpan.Zero);
        }
    }
}
