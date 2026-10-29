using System.IO;
using System.Text;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Media;
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
    private static bool _workspaceChecks;
    private static async Task RunWorkspaceChecksAsync()
    {
        await Check("Appearance migrates schema 4, persists and preserves light/dark/high-contrast readability", async () =>
        {
            var directory = Fixture("appearance");
            await File.WriteAllTextAsync(Path.Combine(directory, "settings.json"), "{\"SchemaVersion\":4}");
            var store = new SettingsStore(directory); var loaded = await store.LoadAsync(default);
            Expect(loaded.Status == SettingsLoadStatus.Migrated && loaded.Settings.Appearance == AppAppearance.Dark, "Existing profiles retain dark default");
            await store.SaveAsync(loaded.Settings with { Appearance = AppAppearance.Light }, default);
            Expect((await new SettingsStore(directory).LoadAsync(default)).Settings.Appearance == AppAppearance.Light, "Appearance survives restart");
            var resources = Application.Current.Resources;
            foreach (var appearance in Enum.GetValues<AppAppearance>())
            {
                AppTheme.Apply(resources, false, appearance);
                Color ColorOf(string name) => ((SolidColorBrush)resources["Theme." + name]).Color;
                foreach (var surface in new[] { "Chrome", "Canvas", "Panel", "Raised", "Input", "AuthorPeer" })
                    Expect(Contrast(ColorOf("Text"), ColorOf(surface)) >= 4.5, "Text contrast " + appearance + surface);
                foreach (var surface in new[] { "Hover", "Pressed", "Selected", "ActiveSelection" })
                    Expect(Contrast(ColorOf("SelectionText"), ColorOf(surface)) >= 4.5, "Selection contrast " + appearance + surface);
            }
            AppTheme.Apply(resources, true, AppAppearance.Light);
            Expect(resources["Theme.Text"] == SystemColors.WindowTextBrush && resources["Theme.AuthorPeer"] == SystemColors.WindowBrush, "High contrast overrides both palettes");
            AppTheme.Apply(resources, false);
        });
        await Check("External launch planning preserves literal paths and prefers Unreal projects without launching tools", async () =>
        {
            var root = Fixture("project 日本語 & [one]");
            var executable = Path.Combine(root, "tool fixture.exe"); await File.WriteAllTextAsync(executable, "not executable");
            var project = Path.Combine(root, "Game project.uproject"); await File.WriteAllTextAsync(project, "{}");
            await File.WriteAllTextAsync(Path.Combine(root, "generated.sln"), "");
            var tools = new RepositoryTools();
            var rider = await tools.PrepareAsync(RepositoryTool.Rider, root, null, executable, default);
            Expect(rider!.Arguments.SequenceEqual([project]) && rider.Directory == root, "Rider uses .uproject before solution with one literal argument");
            var bash = await tools.PrepareAsync(RepositoryTool.Bash, root, null, executable, default);
            Expect(bash!.Arguments.SequenceEqual(["--cd=" + root]), "Bash never interpolates a shell command");
            var code = await tools.PrepareAsync(RepositoryTool.Code, root, null, executable, default);
            Expect(code!.Arguments.SequenceEqual(["--new-window", root]), "VS Code folder arguments");
            await File.WriteAllTextAsync(Path.Combine(root, "Second.uproject"), "{}");
            Expect((await tools.PrepareAsync(RepositoryTool.Rider, root, null, executable, default))!.Arguments.Single() == root, "Ambiguous projects open folder");
            Expect(await tools.PrepareAsync(RepositoryTool.Rider, root, null, Path.Combine(root, "missing.exe"), default) is null, "Missing executable is surfaced for selection");
        });
        await Check("Toolbar commands dispatch explicit requests and retain useful errors without opening-time preparation", async () =>
        {
            var tools = new WorkspaceToolFake(); var model = new ShellViewModel(interaction: new WorkspaceInteraction(), tools: tools);
            model.CompleteStartup(new(new() { RepositoryPaths = [Fixture("tools")] }, SettingsLoadStatus.Loaded));
            Expect(tools.Calls == 0 && model.OpenToolCommand.CanExecute("Rider"), "Startup and command availability do no discovery");
            model.OpenToolCommand.Execute("Rider"); await model.WorkspaceToolTask;
            Expect(tools.Calls == 1 && tools.Launched == 1, "Bound toolbar action submits and launches");
            tools.Fail = true; model.OpenToolCommand.Execute("Bash"); await model.WorkspaceToolTask;
            Expect(model.CanOpenTools && model.Status == GitGui.App.Localization.UiText.Get("WorkspaceToolFailed"), "Launch failure leaves actions usable");
            model.AppearanceCommand.Execute("Light"); await model.PreferenceTask;
            Expect(model.CaptureSession(new()).Appearance == AppAppearance.Light, "Selected appearance is captured in settings");
            model.BeginShutdown(); await model.WaitForIdleAsync();
        });
        await Check("History peer backgrounds, equal-width tabs, graph and toolbars render in both themes at normal/minimum sizes", async () =>
        {
            var repo = Fixture("workspace-history"); var runner = new WindowsProcessRunner(); var reader = new RepositoryReader(runner);
            var git = await reader.DiscoverAsync(null, default);
            var initialized = await runner.RunAsync(new(git.Executable, repo, ["init", "--initial-branch=main", "--object-format=sha1"]), default);
            Expect(initialized.ExitCode == 0, "Disposable history init");
            var tree = WriteFixtureObject(repo, "tree", []);
            var root = Commit(repo, tree, [], "Root\n"); var feature = Commit(repo, tree, [root], "Feature work\n");
            var other = WriteFixtureObject(repo, "commit", Encoding.UTF8.GetBytes($"tree {tree}\nparent {root}\nauthor Other <other@example.invalid> 1792747800 +0200\ncommitter Other <other@example.invalid> 1792747800 +0200\n\nUnrelated author\n"));
            var merge = Commit(repo, tree, [other, feature], "Merge feature\n");
            await File.WriteAllTextAsync(Path.Combine(repo, ".git", "refs", "heads", "main"), merge + "\n");
            await File.WriteAllTextAsync(Path.Combine(repo, ".git", "refs", "heads", "feature"), feature + "\n");
            var tools = new WorkspaceToolFake(); var model = new ShellViewModel(reader, new WorkspaceInteraction(), tools: tools);
            model.CompleteStartup(new(new() { RepositoryPaths = [repo] }, SettingsLoadStatus.Loaded)); await model.WaitForIdleAsync();
            Expect(model.Commits.Count == 4, "Real merge history loaded"); model.SelectedCommit = model.Commits[0];
            var window = new MainWindow(model); var content = (FrameworkElement)window.Content;
            try
            {
                for (var count = 1; count <= 3; count++)
                {
                    if (count > 1) model.Repositories.Add(new RepositoryTab(Path.Combine(repo, "Another repository " + count)));
                    Arrange(content, 1200, 800); await Dispatcher.Yield(DispatcherPriority.ContextIdle);
                    var tabs = VisualDescendants<ListBox>(content).Single(l => ReferenceEquals(l.ItemsSource, model.Repositories));
                    var widths = VisualDescendants<ListBoxItem>(tabs).Select(t => t.ActualWidth).ToArray();
                    Expect(widths.Length == count && widths.Max() - widths.Min() <= 1 && Math.Abs(widths.Sum() - tabs.ActualWidth) <= 4, "Equal full-width tabs: " + count);
                }
                foreach (var appearance in Enum.GetValues<AppAppearance>())
                {
                    AppTheme.Apply(Application.Current.Resources, false, appearance);
                    foreach (var size in new[] { new Size(1200, 800), new Size(900, 580) })
                    {
                        Arrange(content, (int)size.Width, (int)size.Height); await Dispatcher.Yield(DispatcherPriority.ContextIdle);
                        var table = VisualDescendants<DataGrid>(content).Single(g => ReferenceEquals(g.ItemsSource, model.Commits));
                        var peer = VisualDescendants<DataGridRow>(table).First(r => r.DataContext is CommitRow { IsSameAuthor: true });
                        Expect(peer.Background == Application.Current.Resources["Theme.AuthorPeer"] && peer.Foreground == Application.Current.Resources["Theme.HistoryText"], "Highlight changes background only");
                        var selected = VisualDescendants<DataGridRow>(table).Single(r => r.IsSelected);
                        Expect(selected.Background != peer.Background, "Normal selected-row style wins");
                        SaveRender(content, $"workspace-{appearance}-{size.Width}.png", (int)size.Width, (int)size.Height);
                    }
                }
                Expect(tools.Calls == 0, "Rendering menus/toolbars performs no executable discovery");
                window.HighContrast = true; Arrange(content, 900, 580); await Dispatcher.Yield(DispatcherPriority.ContextIdle);
                var hcPeer = VisualDescendants<DataGridRow>(content).First(r => r.DataContext is CommitRow { IsSameAuthor: true });
                Expect(hcPeer.Background != Application.Current.Resources["Theme.AuthorPeer"], "High contrast suppresses peer fill");
            }
            finally { window.Close(); AppTheme.Apply(Application.Current.Resources, false); model.BeginShutdown(); await model.WaitForIdleAsync(); }
        });
    }
    private sealed class WorkspaceInteraction : IUserInteraction
    { public string? ChooseRepository() => null; public string? ChooseGit() => null; }
    private sealed class WorkspaceToolFake : IRepositoryTools
    {
        public int Calls, Launched; public bool Fail;
        public Task<RepositoryLaunch?> PrepareAsync(RepositoryTool tool, string directory, string? git, string? configured, CancellationToken token)
        { Calls++; return Task.FromResult<RepositoryLaunch?>(new("fixture.exe", directory, [directory])); }
        public Task LaunchAsync(RepositoryLaunch request, CancellationToken token)
        { if (Fail) throw new IOException(); Launched++; return Task.CompletedTask; }
    }
}
