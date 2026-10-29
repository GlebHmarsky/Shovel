using System.IO;
using System.Text;
using System.Windows;
using GitGui.App.ViewModels;
using GitGui.App.Views;
using GitGui.App.Localization;
using GitGui.App.Services;
using GitGui.Core.Git;
using GitGui.Core.Settings;
using GitGui.ProcessHost;

namespace GitGui.FoundationChecks;
internal static partial class Program
{
    private static bool _sidebarChecks;
    private static bool _sidebarRenderOnly;
    private static async Task RunSidebarChecksAsync()
    {
        var runner = new WindowsProcessRunner(); var reader = new RepositoryReader(runner); var identities = new WindowsRepositoryIdentity();
        var git = await reader.DiscoverAsync(null, default); var actions = new GitActions(runner, reader, identities);
        var root = Path.Combine(Path.GetTempPath(), "Shovel-sidebar-" + Guid.NewGuid().ToString("N")); Directory.CreateDirectory(root);
        await File.WriteAllTextAsync(Path.Combine(_root, "fixtures.txt"), root);
        async Task<string> Git(string path, params string[] arguments)
        {
            var result = await runner.RunAsync(new(git.Executable, path, arguments), default);
            Expect(result.ExitCode == 0, Encoding.UTF8.GetString(result.StandardError)); return Encoding.UTF8.GetString(result.StandardOutput).Trim();
        }
        async Task<string> Repo(string name)
        {
            var path = Path.Combine(root, name); Directory.CreateDirectory(path);
            await Git(path, "init", "-b", "main"); await Git(path, "config", "user.name", "Sidebar"); await Git(path, "config", "user.email", "sidebar@example.invalid");
            await File.WriteAllTextAsync(Path.Combine(path, "file.txt"), "base\n"); await Git(path, "add", "."); await Git(path, "commit", "-m", "Base"); return path;
        }
        var repo = await Repo("parent"); var childSource = await Repo("child-source");
        await Git(repo, "-c", "protocol.file.allow=always", "submodule", "add", childSource, "module space"); await Git(repo, "commit", "-am", "Add module");
        await Git(repo, "branch", "feature"); await Git(repo, "tag", "base-tag"); await Git(repo, "remote", "add", "origin", childSource);
        await Git(repo, "update-ref", "refs/remotes/origin/main", "HEAD~1"); await Git(repo, "branch", "--set-upstream-to=origin/main", "main");
        await File.AppendAllTextAsync(Path.Combine(repo, "file.txt"), "saved edit\n"); await Git(repo, "stash", "push", "-m", "Sidebar stash");
        var ui = new SidebarInteraction(); var shell = new ShellViewModel(reader, ui, identities, actions: actions);
        SidebarNode Group(string key) => shell.SidebarNodes.Single(n => n.Label == UiText.Get(key));
        await Check("Sidebar lists real branches, tracking, tags, remotes and stash history without a stash modal", async () =>
        {
            shell.CompleteStartup(new(new() { RepositoryPaths = [repo], SelectedRepositoryPath = repo }, SettingsLoadStatus.Loaded)); await shell.WaitForIdleAsync();
            Expect(Group("Branches").Children.Single(n => n.Label == "main").Detail.Contains("↑1"), "Ahead count");
            Expect(Group("Remotes").Children.Single().Children.Count == 1 && Group("Tags").Children.Count == 1, "Fetched ref and tag");
            var stash = Group("Stashes").Children.Single(); stash.Select!.Execute(null); await shell.WaitForIdleAsync();
            Expect(shell.SelectedCommit?.Badges.Any(b => b.Label == "stash@{0}") == true, "Stash commit selected in graph");
            var window = new MainWindow(shell);
            try
            {
                var content = (FrameworkElement)window.Content;
                Arrange(content, 1200, 800); await System.Windows.Threading.Dispatcher.Yield(System.Windows.Threading.DispatcherPriority.ContextIdle);
                var clicked = VisualDescendants<System.Windows.Controls.TreeViewItem>(content).Single(item => item.DataContext is SidebarNode node && node.Label == "feature");
                var selected = shell.SelectedCommit;
                clicked.RaiseEvent(new System.Windows.Input.MouseButtonEventArgs(System.Windows.Input.Mouse.PrimaryDevice, 0, System.Windows.Input.MouseButton.Right)
                    { RoutedEvent = UIElement.PreviewMouseRightButtonDownEvent });
                Expect(clicked.IsSelected && ReferenceEquals(shell.SelectedCommit, selected), "Right click targets the nested branch without navigating history");
                clicked.ContextMenu.PlacementTarget = clicked;
                clicked.ContextMenu.GetBindingExpression(FrameworkElement.DataContextProperty)?.UpdateTarget();
                clicked.ContextMenu.GetBindingExpression(System.Windows.Controls.ItemsControl.ItemsSourceProperty)?.UpdateTarget();
                Expect(clicked.ContextMenu.Items.Count == 5, "Branch menu binds actual row actions");
                SaveRender(content, "sidebar.png", 1200, 800);
                Arrange(content, 900, 580); await System.Windows.Threading.Dispatcher.Yield(System.Windows.Threading.DispatcherPriority.ContextIdle);
                SaveRender(content, "sidebar-minimum.png", 900, 580);
            }
            finally { window.Close(); }
        });
        if (_sidebarRenderOnly) { shell.BeginShutdown(); await shell.WaitForIdleAsync(); return; }
        await Check("Sidebar branch switch and stash Apply, Pop, Drop run real commands", async () =>
        {
            Group("Branches").Children.Single(n => n.Label == "feature").Activate!.Execute(null); await shell.ActionTask;
            Expect(shell.SelectedRepository?.Snapshot?.Branch == "feature", "Switched through bound sidebar");
            var stash = Group("Stashes").Children.Single(); stash.Menu!.Single(m => m.Label == UiText.Get("ActionTitleStashApply")).Command.Execute(null); await shell.ActionTask;
            Expect((await File.ReadAllTextAsync(Path.Combine(repo, "file.txt"))).Contains("saved edit"), "Applied real content");
            await Git(repo, "restore", "--", "file.txt"); shell.RefreshRepositoryCommand.Execute(null); await shell.WaitForIdleAsync();
            Group("Stashes").Children.Single().Menu!.Single(m => m.Label == UiText.Get("ActionTitleStashPop")).Command.Execute(null); await shell.ActionTask;
            Expect(Group("Stashes").Children.Count == 0, "Pop removed saved entry");
            await Git(repo, "stash", "push", "-m", "drop target"); shell.RefreshRepositoryCommand.Execute(null); await shell.WaitForIdleAsync();
            Group("Stashes").Children.Single().Menu!.Single(m => m.Label == UiText.Get("ActionTitleStashDrop")).Command.Execute(null); await shell.ActionTask;
            Expect(ui.Confirmations == 1 && Group("Stashes").Children.Count == 0, "Drop confirms target and executes");
        });
        await Check("Submodule update/sync and opening a child tab preserve Git protections", async () =>
        {
            var child = Path.Combine(repo, "module space");
            await File.WriteAllTextAsync(Path.Combine(childSource, "file.txt"), "next\n"); await Git(childSource, "commit", "-am", "Next");
            var target = await Git(childSource, "rev-parse", "HEAD"); await Git(repo, "update-index", "--cacheinfo", "160000," + target + ",module space");
            shell.RefreshRepositoryCommand.Execute(null); await shell.WaitForIdleAsync();
            var module = Group("Submodules").Children.Single();
            module.Menu!.Single(m => m.Label == UiText.Get("ActionTitleSubmoduleUpdate")).Command.Execute(null); await shell.ActionTask;
            Expect(await Git(child, "rev-parse", "HEAD") == target, "Checked out recorded gitlink");
            Group("Submodules").Children.Single().Menu!.Single(m => m.Label == UiText.Get("ActionTitleSubmoduleSync")).Command.Execute(null); await shell.ActionTask;
            Expect(shell.CurrentAction?.Succeeded == true, "Sync completed");
            await File.WriteAllTextAsync(Path.Combine(child, "file.txt"), "keep local\n");
            await Git(repo, "update-index", "--cacheinfo", "160000," + await Git(childSource, "rev-parse", "HEAD~1") + ",module space");
            shell.RefreshRepositoryCommand.Execute(null); await shell.WaitForIdleAsync();
            Group("Submodules").Children.Single().Menu!.Single(m => m.Label == UiText.Get("ActionTitleSubmoduleUpdate")).Command.Execute(null); await shell.ActionTask;
            Expect(shell.CurrentAction?.Succeeded == false && await File.ReadAllTextAsync(Path.Combine(child, "file.txt")) == "keep local\n", "Dirty checkout not forced");
            Group("Submodules").Children.Single().Select!.Execute(null); await shell.ActionTask; await shell.WaitForIdleAsync();
            Expect(shell.Repositories.Count == 2 && shell.SelectedRepository?.Directory == child, "Child repository opened as another tab");
        });
        await Check("History navigation locates an older tagged commit beyond the first page", async () =>
        {
            using var area = new HistoryPerformanceArea(); var traceRunner = new HistoryPerformanceRunner(area);
            var large = await HistoryPerformanceFixtures.CreateAsync(area, traceRunner, git, new("sidebar", 403, true, 3), "history", default);
            var snapshot = await reader.InspectAsync(git, large, default); var history = await reader.OpenHistoryAsync(git, snapshot, HistoryScope.AllCommits, default);
            var first = (await Git(large, "rev-list", "--max-parents=0", "HEAD")).Split('\n')[0];
            var page = await reader.LocateHistoryAsync(git, history, first, default);
            Expect(page.Cursor.Offset >= 200 && page.Entries.Any(e => e.Commit.Hash == first), "Bounded older page includes target");
        });
        shell.BeginShutdown(); await shell.WaitForIdleAsync();
    }
    private sealed class SidebarInteraction : IUserInteraction
    {
        public int Confirmations;
        public string? ChooseRepository() => null;
        public string? ChooseGit() => null;
        public bool ConfirmRecovery(string message, string title) { Confirmations++; return true; }
    }
}
