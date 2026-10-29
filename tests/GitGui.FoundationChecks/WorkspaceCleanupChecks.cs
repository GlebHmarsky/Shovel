using System.IO;
using System.Text;
using System.Windows;
using System.Windows.Automation;
using System.Windows.Controls;
using System.Windows.Media.Animation;
using System.Windows.Threading;
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
    private static bool _workspaceCleanupChecks;
    private static async Task RunWorkspaceCleanupChecksAsync()
    {
        var runner = new CleanupRunner(); var reader = new RepositoryReader(runner);
        var identities = new WindowsRepositoryIdentity(); var actions = new GitActions(runner, reader, identities);
        var git = await reader.DiscoverAsync(null, default); var repo = Fixture("workspace cleanup");
        async Task<string> Git(params string[] args)
        {
            var result = await runner.RunAsync(new(git.Executable, repo, args), default);
            Expect(result.ExitCode == 0, Encoding.UTF8.GetString(result.StandardError)); return Encoding.UTF8.GetString(result.StandardOutput).Trim();
        }
        var interaction = new CleanupInteraction(); var shell = new ShellViewModel(reader, interaction, identities, actions: actions);
        try
        {
            await Check("Add remote opens without queries, saves literal local configuration and supports fetched upstream tracking", async () =>
            {
                await Git("init", "-b", "main", "--object-format=sha1");
                var tree = WriteFixtureObject(repo, "tree", []); var tip = Commit(repo, tree, [], "Root\n");
                for (var i = 0; i < 205; i++) tip = Commit(repo, tree, [tip], "History " + i + "\n");
                await Git("update-ref", "refs/heads/main", tip);
                shell.CompleteStartup(new(new() { RepositoryPaths = [repo] }, SettingsLoadStatus.Loaded)); await shell.WaitForIdleAsync();
                var calls = runner.Calls;
                interaction.Remote = model => { Expect(runner.Calls == calls && model.Name == "origin", "Options have no preparation"); return null; };
                shell.AddRemoteCommand.Execute(null); await shell.ActionTask; Expect(runner.Calls == calls, "Cancel does no Git work");
                var location = Path.Combine(_root, "unavailable upstream 日本語.git");
                interaction.Remote = model => { model.Name = "upstream"; model.Location = location; Expect(model.SubmitCommand.CanExecute(null), "Valid local target"); return model.Options; };
                shell.AddRemoteCommand.Execute(null); await shell.ActionTask; await shell.WaitForIdleAsync();
                Expect(shell.CurrentAction?.Succeeded == true && shell.SelectedRepository!.Snapshot!.RemoteNames.Contains("upstream"), "Action completes and refreshes remote list");
                Expect(await Git("config", "--get", "remote.upstream.url") == location, "Literal URL/path retained");
                Expect(!runner.NetworkStarted, "Adding a remote never fetches or probes it");
                await Git("update-ref", "refs/remotes/upstream/main", tip);
                var snapshot = await reader.InspectAsync(git, repo, default); var history = await reader.OpenHistoryAsync(git, snapshot, HistoryScope.AllCommits, default);
                var context = await actions.PrepareBranchManagementAsync(git, await identities.ReadAsync(repo, default), snapshot, history, default);
                var tracked = await actions.ExecuteBranchManagementAsync(context, new(BranchManagementAction.SetUpstream,
                    history.References.Single(r => r.Name == "refs/heads/main"), history.References.Single(r => r.Name == "refs/remotes/upstream/main")), default);
                Expect(tracked.Success && await Git("rev-parse", "--symbolic-full-name", "@{upstream}") == "refs/remotes/upstream/main", "Real upstream configured");
                context = await actions.PrepareRemoteSetupAsync(context, default);
                var duplicate = await actions.AddRemoteAsync(context, new("upstream", location), default);
                Expect(!duplicate.Success && await Git("config", "--get", "remote.upstream.url") == location, "Duplicate cannot overwrite existing config");
                await Git("config", "test.changed", "true");
                try { await actions.AddRemoteAsync(context, new("other", location), default); throw new InvalidOperationException("Stale config accepted"); }
                catch (GitActionException e) { Expect(e.Code == "BranchManageChanged", "Changed config is reported"); }
                using var cancel = new CancellationTokenSource(); cancel.Cancel();
                try { await actions.AddRemoteAsync(context, new("other", location), cancel.Token); throw new InvalidOperationException("Cancellation ignored"); }
                catch (OperationCanceledException) { }
            });
            await Check("History and refresh timers preserve outcomes, page navigation and useful rows on failure or cancellation", async () =>
            {
                shell.RefreshRepositoryCommand.Execute(null); Expect(shell.Activity is { IsComplete: false }, "Read starts feedback synchronously");
                await shell.WaitForIdleAsync(); Expect(shell.Activity is { Outcome: OperationOutcome.Success, Seconds: > 0 }, "Refresh completes with timing");
                var first = shell.Commits[0].Commit.Hash; var attempt = shell.Activity!.Attempt;
                var wheelWindow = new MainWindow(shell);
                try
                {
                    var content = (FrameworkElement)wheelWindow.Content; Arrange(content, 1200, 800);
                    var table = (DataGrid)wheelWindow.FindName("HistoryTable");
                    var scroll = VisualDescendants<ScrollViewer>(table).First(); scroll.ScrollToEnd(); table.UpdateLayout();
                    table.RaiseEvent(new System.Windows.Input.MouseWheelEventArgs(System.Windows.Input.Mouse.PrimaryDevice, 0, -120) { RoutedEvent = UIElement.PreviewMouseWheelEvent });
                    await shell.WaitForIdleAsync();
                }
                finally { wheelWindow.Close(); }
                Expect(shell.Commits.Count == 6 && !ReferenceEquals(attempt, shell.Activity!.Attempt), "Older page remains reachable with its own feedback");
                shell.NewerHistoryCommand.Execute(null); await shell.WaitForIdleAsync(); Expect(shell.Commits[0].Commit.Hash == first, "Newer returns to original page");
                var rows = shell.Commits; runner.FailLogs = true;
                shell.RefreshRepositoryCommand.Execute(null); await shell.WaitForIdleAsync();
                Expect(ReferenceEquals(rows, shell.Commits) && shell.Activity?.Outcome == OperationOutcome.Failed && shell.HistoryProblem.Length > 0, "Read failure retains rows and visible explanation");
                runner.FailLogs = false; runner.BlockLogs = true;
                shell.RefreshRepositoryCommand.Execute(null); await runner.Blocked.Task.WaitAsync(TimeSpan.FromSeconds(5));
                shell.CancelCommand.Execute(null); await shell.WaitForIdleAsync();
                Expect(ReferenceEquals(rows, shell.Commits) && shell.Activity?.Outcome == OperationOutcome.Cancelled, "Cancellation retains rows and distinct outcome");
                runner.BlockLogs = false; shell.RefreshRepositoryCommand.Execute(null); await shell.WaitForIdleAsync();
            });
            await Check("A stash has one base connection while real abandoned ancestry and independently tagged index commits stay visible", async () =>
            {
                var tree = WriteFixtureObject(repo, "tree", []); var root = (await Git("rev-list", "--max-parents=0", "HEAD")).Trim();
                var abandoned = Commit(repo, tree, [root], "Real original stash base\n");
                var index = Commit(repo, tree, [abandoned], "Saved index\n");
                var stash = Commit(repo, tree, [abandoned, index], "Saved work\n");
                await Git("-c", "user.name=Fixture", "-c", "user.email=fixture@example.invalid", "stash", "store", "-m", "Saved work", stash);
                await Git("tag", "independent-index", index);
                var snapshot = await reader.InspectAsync(git, repo, default); var history = await reader.ReadInitialHistoryAsync(git, snapshot, HistoryScope.AllCommits, default);
                var all = history.Entries.ToList(); while (history.Next is { } cursor) { history = await reader.ReadHistoryPageAsync(git, history.Session, cursor, default); all.AddRange(history.Entries); }
                var saved = all.Single(e => e.Commit.Hash == stash);
                Expect(saved.Graph.IsStash && !saved.Graph.IsMerge && saved.Commit.Parents.Count == 2 &&
                    saved.Graph.Lines.Count(l => l.FromY == .5) == 1, "Distinct stash marker and one true-base connector, retaining metadata");
                Expect(all.Any(e => e.Commit.Hash == abandoned) && all.Any(e => e.Commit.Hash == index), "Real base and independent index remain visible");
                AssertGraphConnections(all);
                shell.RefreshRepositoryCommand.Execute(null); await shell.WaitForIdleAsync();
                // Bring the actual stash topology into the offscreen render without changing history scope.
                shell.LoadMoreHistoryCommand.Execute(null); await shell.WaitForIdleAsync();
            });
            await Check("Uncluttered workspace, wide sidebar, caption buttons and read fade clocks render at normal and minimum sizes", async () =>
            {
                shell.DetailsIndex = 0; shell.SelectedCommit = shell.Commits.FirstOrDefault(c => c.Graph.IsStash);
                var window = new MainWindow(shell); var content = (FrameworkElement)window.Content;
                try
                {
                    Expect(new AppSettings { SidebarWidth = 700 }.Normalize().SidebarWidth == 700, "Wide sidebar persists");
                    foreach (var theme in Enum.GetValues<AppAppearance>())
                    {
                        AppTheme.Apply(Application.Current.Resources, false, theme);
                        foreach (var size in new[] { new Size(1200, 800), new Size(900, 620) })
                        {
                            ((ColumnDefinition)window.FindName("SidebarColumn")).Width = new(360);
                            Arrange(content, (int)size.Width, (int)size.Height); await Dispatcher.Yield(DispatcherPriority.ContextIdle);
                            var tree = VisualDescendants<TreeView>(content).Single();
                            Expect(ScrollViewer.GetHorizontalScrollBarVisibility(tree) == ScrollBarVisibility.Disabled && tree.ActualWidth > 320, "Sidebar widens without a horizontal scrollbar");
                            Expect(!VisualDescendants<Button>(content).Any(b => AutomationProperties.GetAutomationId(b) == "LocalChangesNavigation" || ReferenceEquals(b.Command, shell.LoadMoreHistoryCommand)), "No duplicate Local Changes or page-control band");
                            Expect(VisualDescendants<Button>(content).Where(b => b.Command == SystemCommands.CloseWindowCommand).Single().FontSize >= 22, "Larger caption glyphs");
                            SaveRender(content, $"cleanup-{theme}-{size.Width}.png", (int)size.Width, (int)size.Height);
                        }
                    }
                    var remote = new RemoteSetupWindow(new RemoteSetupViewModel(["origin"]));
                    try { Arrange((FrameworkElement)remote.Content, 540, 340); SaveRender((FrameworkElement)remote.Content, "add-remote.png", 540, 340); }
                    finally { remote.Close(); }
                    var view = new OperationIndicator(); var attempt = new object(); view.Activity = new(attempt, "History", "Reading", 0, OperationOutcome.Running);
                    AnimationClock? Clock(string name) => (AnimationClock?)typeof(OperationIndicator).GetField(name, System.Reflection.BindingFlags.NonPublic | System.Reflection.BindingFlags.Instance)!.GetValue(view);
                    var entrance = Clock("_entranceClock"); view.Activity = view.Activity with { Seconds = .2 };
                    Expect(ReferenceEquals(entrance, Clock("_entranceClock")), "Ticks do not restart fade-in");
                    view.Activity = view.Activity with { Outcome = OperationOutcome.Success };
                    Expect(Clock("_entranceClock") is null && Clock("_completionClock") is not null, "Completion replaces entrance with hold/fade-out");
                    Clock("_completionClock")!.Controller!.SeekAlignedToLastTick(TimeSpan.FromSeconds(2.1), TimeSeekOrigin.BeginTime);
                    Expect(((Border)view.FindName("DurationBadge")).Opacity < .01, "Timer fades away");
                }
                finally { window.Close(); AppTheme.Apply(Application.Current.Resources, false); }
            });
        }
        finally { shell.BeginShutdown(); await shell.WaitForIdleAsync(); }
    }
    private sealed class CleanupInteraction : IUserInteraction
    {
        public Func<RemoteSetupViewModel, RemoteSetupOptions?>? Remote;
        public RemoteSetupOptions? ChooseRemoteSetup(RemoteSetupViewModel model) => Remote?.Invoke(model);
        public string? ChooseRepository() => null;
        public string? ChooseGit() => null;
    }
    private sealed class CleanupRunner : IProcessRunner
    {
        private readonly WindowsProcessRunner _inner = new();
        public int Calls; public bool NetworkStarted, FailLogs, BlockLogs;
        public TaskCompletionSource Blocked { get; } = new(TaskCreationOptions.RunContinuationsAsynchronously);
        public async Task<ProcessResult> RunAsync(ProcessRequest request, CancellationToken token, Action<ProcessOutput>? output = null)
        {
            Interlocked.Increment(ref Calls);
            if (request.Arguments.Contains("fetch") || request.Arguments.Contains("ls-remote")) NetworkStarted = true;
            if (request.Arguments.Contains("log"))
            {
                if (FailLogs) return new(ProcessOutcome.Exited, 1, [], Encoding.UTF8.GetBytes("Fixture read failed"), TimeSpan.Zero);
                if (BlockLogs) { Blocked.TrySetResult(); await Task.Delay(Timeout.Infinite, token); }
            }
            return await _inner.RunAsync(request, token, output);
        }
    }
}
