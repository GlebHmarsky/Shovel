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
    private static void RunViewModelChecks()
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
            _ = task.ContinueWith(_ => dispatcher.BeginInvoke(() => frame.Continue = false), TaskScheduler.Default);
            Dispatcher.PushFrame(frame);
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
            await Check("Late history completion cannot leak rows into another repository tab", async () =>
            {
                var runner = new ModelRunner(first) { HoldHistory = true };
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
                await Check("Long commit display text is bounded without discarding the parsed message", () =>
                {
                    var commit = new CommitRecord("abcdef", [], new string('a', 1000), "a@example.invalid", DateTimeOffset.UnixEpoch,
                        "C", "c@example.invalid", DateTimeOffset.UnixEpoch, new string('x', 100000));
                    var row = new CommitRow(new(commit, CommitGraph.Build([commit])[0]));
                    Expect(row.Description.Length <= 513 && row.Author.Length <= 257 && row.MessagePreview.Length == 65536 &&
                        row.MessageNotice.Length > 0 && row.Commit.Message.Length == 100000, "Bound visible text and retain original message in memory");
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
                        Expect(table.ActualHeight > 50 && table.ActualWidth > 300, "History remains allocated at minimum shell size");
                        Expect(table.Columns[0].ActualWidth >= model.GraphWidth && table.Columns[3].ActualWidth >= 150, "Narrow windows scroll instead of clipping graph lanes or date columns");
                        SaveRender(content, "history-minimum.png", 900, 580);
                    }
                    finally { window.Close(); }
                });
                await Check("History expands incrementally and WPF realizes only visible rows", async () =>
                {
                    var model = await LoadRealModel(_longHistoryRepository!);
                    Expect(model.Commits.Count == 200 && model.LoadMoreHistoryCommand.CanExecute(null), "Initial page and load-more command");
                    var selection = model.Commits[4].Commit.Hash;
                    model.SelectedCommit = model.Commits[4];
                    model.LoadMoreHistoryCommand.Execute(null);
                    await model.WaitForIdleAsync().WaitAsync(TimeSpan.FromSeconds(30));
                    Expect(model.Commits.Count == 400 && model.SelectedCommit?.Commit.Hash == selection, "Expansion retains selection by hash");
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

            async Task<ShellViewModel> LoadRealModel(string path)
            {
                var model = new ShellViewModel(new(new GitGui.ProcessHost.WindowsProcessRunner()),
                    new ModelInteraction(path) { Trust = true, Executable = _historyExecutable });
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
        var model = new ShellViewModel(new(runner), new ModelInteraction(directory) { Trust = true });
        model.CompleteStartup(new(new(), SettingsLoadStatus.Defaults));
        model.ChooseGitCommand.Execute(null);
        return model;
    }

    private sealed class ModelInteraction(string directory) : IUserInteraction
    {
        public bool Trust { get; init; }
        public string? Executable { get; init; }
        public int TrustRequests { get; private set; }
        public string? ChooseRepository() => directory;
        public string? ChooseGit() => Executable ?? ProbeExecutable;
        public bool ConfirmTrust(string _) { TrustRequests++; return Trust; }
    }

    // Test-only transport: real Git semantics are covered separately in disposable-repository checks.
    private sealed class ModelRunner(string directory) : IProcessRunner
    {
        public TaskCompletionSource Started { get; } = new(TaskCreationOptions.RunContinuationsAsynchronously);
        public TaskCompletionSource Release { get; } = new(TaskCreationOptions.RunContinuationsAsynchronously);
        public TaskCompletionSource HistoryStarted { get; } = new(TaskCreationOptions.RunContinuationsAsynchronously);
        public TaskCompletionSource ReleaseHistory { get; } = new(TaskCreationOptions.RunContinuationsAsynchronously);
        public bool HoldHistory { get; init; }
        public int Calls;
        public bool Cancelled;
        public async Task<ProcessResult> RunAsync(ProcessRequest request, CancellationToken cancellationToken, Action<ProcessOutput>? output = null)
        {
            Interlocked.Increment(ref Calls);
            Started.TrySetResult();
            try { await Release.Task.WaitAsync(cancellationToken); }
            catch (OperationCanceledException) { Cancelled = true; return new(ProcessOutcome.Cancelled, null, [], [], TimeSpan.Zero); }
            var args = request.Arguments;
            if (args.Contains("log"))
            {
                HistoryStarted.TrySetResult();
                if (HoldHistory) await ReleaseHistory.Task.WaitAsync(cancellationToken);
                var row = string.Join('\0', "abcdef", "", "A", "a@example.invalid", "2026-10-23T10:00:00+00:00", "C", "c@example.invalid", "2026-10-23T10:00:00+00:00", "Subject\n") + '\0';
                return new(ProcessOutcome.Exited, 0, Encoding.UTF8.GetBytes(row), [], TimeSpan.Zero);
            }
            var value = args.Contains("--version") ? "git version 2.51.1" :
                args.Contains("--is-shallow-repository") ? "false" :
                args.Contains("--is-inside-work-tree") ? "true" :
                args.Contains("symbolic-ref") ? "main" : args.Contains("--verify") ? "abcdef" : directory;
            return new(ProcessOutcome.Exited, 0, Encoding.UTF8.GetBytes(value + "\n"), [], TimeSpan.Zero);
        }
    }
}
