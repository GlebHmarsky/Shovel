using System.IO;
using System.Text;
using System.Text.Json;
using System.Windows;
using System.Windows.Automation;
using System.Windows.Controls;
using System.Windows.Shell;
using System.Windows.Threading;
using GitGui.App.Services;
using GitGui.App.ViewModels;
using GitGui.App.Views;
using GitGui.Core.Git;
using GitGui.Core.Settings;
using GitGui.ProcessHost;

namespace GitGui.FoundationChecks;
internal static partial class Program
{
    private static bool _historyLayoutChecks;
    private static async Task RunHistoryLayoutChecksAsync()
    {
        CommitRecord Node(string hash, params string[] parents) => new(hash, parents, "Author", "a@example.invalid", DateTimeOffset.UnixEpoch, "Author", "a@example.invalid", DateTimeOffset.UnixEpoch, hash);
        var pattern = new[] { Node("m3", "m2", "c"), Node("c", "base"), Node("m2", "m1", "b"), Node("b", "base"), Node("m1", "base", "a"), Node("a", "base"), Node("base") };
        await Check("Successive merges keep straight distinct branches through their shared base and across page boundaries", () =>
        {
            var graph = CommitGraph.Build(pattern);
            Expect(graph[1].NodeLane == 1 && graph[3].NodeLane == 2 && graph[5].NodeLane == 3, "Three branches retain distinct columns");
            Expect(graph[2].Lines.Any(l => l.FromLane == 1 && l.ToLane == 1 && l.FromY == 0 && l.ToY == 1), "Older merge does not displace live branch");
            Expect(graph[^1].Lines.Count(l => l.ToLane == 0 && l.ToY == .5) == 4, "All four tracks join on the actual base row");
            var first = CommitGraph.BuildPage(pattern[..3], GraphCursor.Start); var rest = CommitGraph.BuildPage(pattern[3..], first.Next);
            Expect(JsonSerializer.Serialize(graph) == JsonSerializer.Serialize(first.Rows.Concat(rest.Rows)), "Pagination preserves lane and color continuity");
            AssertGraphConnections(pattern.Select((c, i) => new HistoryEntry(c, graph[i])).ToArray());
            return Task.CompletedTask;
        });
        await Check("Octopus, independent roots and lane reuse retain topology without dangling or diagonal live lanes", async () =>
        {
            var commits = new[] { Node("oct", "p1", "p2", "p3"), Node("p3", "root"), Node("other"), Node("p2", "root"), Node("p1", "root"), Node("root") };
            var page = CommitGraph.BuildPage(commits, GraphCursor.Start);
            AssertGraphConnections(commits.Select((c, i) => new HistoryEntry(c, page.Rows[i])).ToArray());
            Expect(page.Next.Lanes.Count == 0 && page.Rows[0].IsMerge, "All lanes terminate, octopus remains a merge");
            var crossed = new[] { Node("tip", "left", "right"), Node("left", "p", "q"), Node("right", "q", "p"), Node("p", "root"), Node("q", "root"), Node("root") };
            var crossGraph = CommitGraph.BuildPage(crossed, GraphCursor.Start);
            AssertGraphConnections(crossed.Select((c, i) => new HistoryEntry(c, crossGraph.Rows[i])).ToArray());
            Expect(crossGraph.Next.Lanes.Count == 0, "Criss-cross ancestry terminates every track");
            await ExpectGitFailure(GitFailure.InvalidOutput, () => Task.FromResult(CommitGraph.Build([Node("cycle", "cycle")])));
            await ExpectGitFailure(GitFailure.HistoryTooWide, () => Task.FromResult(CommitGraph.Build([Node("wide", Enumerable.Range(0, 65).Select(i => "p" + i).ToArray())])));
        });
        var runner = new WindowsProcessRunner(); var reader = new RepositoryReader(runner); var git = await reader.DiscoverAsync(null, default);
        var repo = Fixture("folded-stash");
        async Task<string> Git(params string[] args)
        {
            var result = await runner.RunAsync(new(git.Executable, repo, args), default);
            Expect(result.ExitCode == 0, Encoding.UTF8.GetString(result.StandardError)); return Encoding.UTF8.GetString(result.StandardOutput).Trim();
        }
        await Check("Real tracked/untracked stashes fold into one row, fill pages, retain metadata and keep independently tagged internals", async () =>
        {
            await Git("init", "-b", "main", "--object-format=sha1"); await Git("config", "user.name", "Graph fixture"); await Git("config", "user.email", "graph@example.invalid");
            await File.WriteAllTextAsync(Path.Combine(repo, "tracked.txt"), "base\n"); await Git("add", "."); await Git("commit", "-m", "Base");
            await File.AppendAllTextAsync(Path.Combine(repo, "tracked.txt"), "edit\n"); await File.WriteAllTextAsync(Path.Combine(repo, "untracked.txt"), "saved\n");
            await Git("stash", "push", "-u", "-m", "One untracked stash");
            var stash = await Git("rev-parse", "stash@{0}"); var index = await Git("rev-parse", "stash@{0}^2"); var untracked = await Git("rev-parse", "stash@{0}^3");
            await File.AppendAllTextAsync(Path.Combine(repo, "tracked.txt"), "second\n"); await Git("stash", "push", "-m", "One tracked stash");
            var secondIndex = await Git("rev-parse", "stash@{0}^2");
            var baseHash = await Git("rev-parse", "HEAD"); var tree = await Git("rev-parse", "HEAD^{tree}"); var tip = baseHash;
            for (var i = 0; i < 205; i++) tip = Commit(repo, tree, [tip], "History " + i + "\n");
            await Git("update-ref", "refs/heads/main", tip);
            var watch = System.Diagnostics.Stopwatch.StartNew();
            var snapshot = await reader.InspectAsync(git, repo, default); var session = await reader.OpenHistoryAsync(git, snapshot, HistoryScope.AllCommits, default);
            Console.WriteLine($"Fixture inspection/session including stash projection: {watch.Elapsed.TotalMilliseconds:F0} ms"); watch.Restart();
            Expect(session.HiddenStashCommits.SetEquals(new[] { index, secondIndex, untracked }), "Index and untracked stash objects (identical index objects can be shared)");
            var first = await reader.ReadHistoryPageAsync(git, session, HistoryCursor.Start, default);
            Console.WriteLine($"Fixture first visible page: {watch.Elapsed.TotalMilliseconds:F0} ms");
            var second = await reader.ReadHistoryPageAsync(git, session, first.Next!, default);
            var all = first.Entries.Concat(second.Entries).ToArray();
            Expect(first.Entries.Count == 200 && all.Length == 208 && all.Select(e => e.Commit.Hash).Distinct().Count() == 208 && !second.HasMore, "Full visible pages without duplicate/skipped normal commits");
            var entry = all.Single(e => e.Commit.Hash == stash);
            Expect(!all.Any(e => e.Commit.Hash == index || e.Commit.Hash == untracked) && entry.Commit.Parents.Count == 3 && !entry.Graph.IsMerge, "One graph item with original three-parent details");
            var located = await reader.LocateHistoryAsync(git, session, baseHash, default);
            Expect(located.Entries.Any(e => e.Commit.Hash == baseHash), "Ref navigation uses the same visible/raw offsets");
            await Git("tag", "saved-index", index);
            snapshot = await reader.InspectAsync(git, repo, default); session = await reader.OpenHistoryAsync(git, snapshot, HistoryScope.AllCommits, default);
            Expect(!session.HiddenStashCommits.Contains(index) && session.HiddenStashCommits.Contains(untracked), "Independently reachable history is not silently hidden");
            await Git("tag", "-d", "saved-index");
        });
        await Check("Integrated caption menus, reordered toolbar and rounded multi-merge graph render at normal/minimum sizes", async () =>
        {
            var visualRepo = Fixture("three-merges");
            var initialized = await runner.RunAsync(new(git.Executable, visualRepo, ["init", "-b", "main", "--object-format=sha1"]), default); Expect(initialized.ExitCode == 0, "Visual fixture init");
            var tree = WriteFixtureObject(visualRepo, "tree", []); var root = Commit(visualRepo, tree, [], "Shared base\n");
            var tip = root;
            foreach (var name in new[] { "branch-management", "commit-drafts", "history-benchmarks" })
            {
                var branch = Commit(visualRepo, tree, [root], "Implement " + name + "\n");
                await File.WriteAllTextAsync(Path.Combine(visualRepo, ".git", "refs", "heads", name), branch + "\n");
                tip = Commit(visualRepo, tree, [tip, branch], "Merge " + name + "\n");
            }
            await File.WriteAllTextAsync(Path.Combine(visualRepo, ".git", "refs", "heads", "main"), tip + "\n");
            var model = new ShellViewModel(reader, new WorkspaceInteraction());
            model.CompleteStartup(new(new() { RepositoryPaths = [visualRepo] }, SettingsLoadStatus.Loaded)); await model.WaitForIdleAsync();
            var window = new MainWindow(model); var content = (FrameworkElement)window.Content;
            try
            {
                Expect(WindowChrome.GetWindowChrome(window).CaptionHeight == 32, "One shared caption/menu area with native move/resize hit testing");
                var menu = VisualDescendants<Menu>(content).First(); Expect(WindowChrome.GetIsHitTestVisibleInChrome(menu), "Menu remains interactive in caption");
                Expect(window.CommandBindings.Count == 3, "Minimize, maximize/restore and close wired");
                foreach (var theme in Enum.GetValues<AppAppearance>())
                {
                    AppTheme.Apply(Application.Current.Resources, false, theme);
                    foreach (var size in new[] { new Size(1200, 800), new Size(900, 580) })
                    {
                        Arrange(content, (int)size.Width, (int)size.Height); await Dispatcher.Yield(DispatcherPriority.ContextIdle);
                        var buttons = VisualDescendants<Button>((Grid)window.FindName("RepositoryToolbar")).ToArray();
                        var push = buttons.Single(b => ReferenceEquals(b.Command, model.PushCommand)); var commit = buttons.Single(b => ReferenceEquals(b.Command, model.ShowChangesCommand));
                        Expect(commit.TransformToAncestor(content).Transform(new Point()).X > push.TransformToAncestor(content).Transform(new Point()).X, "Commit follows Push");
                        SaveRender(content, $"history-layout-{theme}-{size.Width}.png", (int)size.Width, (int)size.Height);
                    }
                }
            }
            finally { window.Close(); model.BeginShutdown(); await model.WaitForIdleAsync(); AppTheme.Apply(Application.Current.Resources, false); }
            var stashModel = new ShellViewModel(reader, new WorkspaceInteraction());
            stashModel.CompleteStartup(new(new() { RepositoryPaths = [repo] }, SettingsLoadStatus.Loaded)); await stashModel.WaitForIdleAsync();
            var stashWindow = new MainWindow(stashModel);
            try
            {
                var stashContent = (FrameworkElement)stashWindow.Content; Arrange(stashContent, 1200, 800); await Dispatcher.Yield(DispatcherPriority.ContextIdle);
                SaveRender(stashContent, "folded-stashes-Dark.png", 1200, 800);
            }
            finally { stashWindow.Close(); stashModel.BeginShutdown(); await stashModel.WaitForIdleAsync(); }
        });
    }
}
