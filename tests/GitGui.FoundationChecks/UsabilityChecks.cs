using System.Diagnostics;
using System.IO;
using System.Text;
using System.Windows;
using System.Windows.Automation;
using System.Windows.Controls;
using System.Windows.Media;
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
    private static bool _startupChecks;
    private static async Task RunUsabilityChecksAsync()
    {
        var runner = new PreviewDelayRunner(); var reader = new RepositoryReader(runner); var ids = new WindowsRepositoryIdentity();
        var actions = new GitActions(runner, reader, ids); var git = await reader.DiscoverAsync(null, default);
        async Task<string> Git(string repo, params string[] args)
        {
            var result = await runner.RunAsync(new(git.Executable, repo, args), default);
            Expect(result.ExitCode == 0, Encoding.UTF8.GetString(result.StandardError)); return Encoding.UTF8.GetString(result.StandardOutput).Trim();
        }
        var repo = Fixture("usability"); await Git(repo, "init", "-b", "main"); await Git(repo, "config", "user.name", "Usability");
        await Git(repo, "config", "user.email", "ui@example.invalid"); await Git(repo, "config", "core.autocrlf", "false");
        await File.WriteAllTextAsync(Path.Combine(repo, "file.txt"), "base\n"); await Git(repo, "add", "."); await Git(repo, "commit", "-m", "base");
        var interaction = new UsabilityInteraction(); var shell = new ShellViewModel(reader, interaction, ids, actions: actions);
        var cache = new HistoryCacheStore(Fixture("startup-cache")); shell.StartupCache = cache;
        shell.CompleteStartup(new(new() { RepositoryPaths = [repo] }, SettingsLoadStatus.Loaded)); await shell.WaitForIdleAsync();

        await Check("Startup view survives restart, publishes before delayed Git and cannot authorize mutations", async () =>
        {
            shell.SaveHistoryScroll(3, 7); await shell.SaveStartupCacheAsync(default); var settings = shell.CaptureSession(new());
            var watch = Stopwatch.StartNew(); runner.Hold = true;
            var restored = new ShellViewModel(reader, interaction, ids, actions: actions) { StartupCache = cache };
            restored.CompleteStartup(new(settings, SettingsLoadStatus.Loaded));
            while (!restored.ShowingStartupPreview && watch.Elapsed < TimeSpan.FromSeconds(3)) await Task.Delay(5);
            Console.WriteLine($"Restart cached rows: {watch.Elapsed.TotalMilliseconds:F1} ms while fresh Git is held");
            Expect(restored.ShowingStartupPreview && restored.Commits.Count == 1 && restored.HistoryScroll == (3, 7), "Useful cached page/selection/scroll before Git");
            Expect(!restored.CreateHistoryMenu(restored.Commits[0])[0].Command!.CanExecute(null), "Cached preview cannot authorize checkout");
            runner.Hold = false; runner.Release.TrySetResult(); await restored.WaitForIdleAsync();
            Expect(!restored.ShowingStartupPreview && restored.Commits.Count == 1, "Fresh rows replace stale presentation");
        });
        await Check("Corrupt or mismatched startup cache is disposable", async () =>
        {
            var identity = await ids.ReadAsync(repo, default);
            Expect(await cache.ReadAsync(repo, "different identity", default) is null, "Wrong identity rejected");
            await File.WriteAllTextAsync(cache.FilePath, "{broken"); Expect(await cache.ReadAsync(repo, identity.Fingerprint, default) is null, "Malformed JSON ignored");
            await shell.SaveStartupCacheAsync(default); Expect(await cache.ReadAsync(repo, identity.Fingerprint, default) is not null, "Rebuildable cache can be replaced");
            var valid = (await cache.ReadAsync(repo, identity.Fingerprint, default))!;
            await cache.SaveAsync(valid with { Entries = [valid.Entries[0] with { Graph = valid.Entries[0].Graph with { NodeLane = 999 } }] }, default);
            Expect(await cache.ReadAsync(repo, identity.Fingerprint, default) is null, "Invalid graph cannot reach drawing/bindings");
        });
        if (_startupChecks) return;
        await Check("Tab reorder preserves selection/order and disabled tab strip stays themed", async () =>
        {
            var selected = shell.SelectedRepository; shell.Repositories.Add(new(repo + " second")); shell.Repositories.Add(new(repo + " third"));
            var calls = runner.Calls; shell.ReorderTab(selected!, 2);
            Expect(shell.SelectedRepository == selected && shell.Repositories[2] == selected && runner.Calls == calls && shell.CaptureSession(new()).RepositoryPaths[2] == repo, "Move does not switch/read; order persists");
            var window = new MainWindow(shell); var content = (FrameworkElement)window.Content;
            try
            {
                foreach (var theme in new[] { AppAppearance.Dark, AppAppearance.Light })
                {
                    AppTheme.Apply(Application.Current.Resources, false, theme); Arrange(content, 1200, 800);
                    var tabs = (ListBox)window.FindName("RepositoryTabs"); tabs.IsEnabled = false; content.UpdateLayout();
                    var border = VisualDescendants<Border>(tabs).First();
                    Expect(((SolidColorBrush)border.Background).Color == ((SolidColorBrush)Application.Current.Resources["Theme.Sidebar"]).Color, "Disabled list background uses semantic brush");
                    SaveRender(content, $"disabled-tabs-{theme}.png", 1200, 800); tabs.ClearValue(UIElement.IsEnabledProperty);
                }
                AppTheme.Apply(Application.Current.Resources, true); content.UpdateLayout();
                Expect(((SolidColorBrush)VisualDescendants<Border>((ListBox)window.FindName("RepositoryTabs")).First().Background).Color == SystemColors.WindowColor, "High contrast honors system background");
            }
            finally { window.Close(); AppTheme.Apply(Application.Current.Resources, false); }
        });
        foreach (var action in new[] { StashAction.Apply, StashAction.Pop, StashAction.Drop })
            await Check($"Stash history menu executes {action} with the exact saved target", async () =>
            {
                await Git(repo, "reset", "--hard", "HEAD");
                if (action != StashAction.Pop) { await File.WriteAllTextAsync(Path.Combine(repo, "file.txt"), "stash\n"); await Git(repo, "stash", "push", "-m", "history stash"); }
                shell.RefreshRepositoryCommand.Execute(null); await shell.WaitForIdleAsync();
                var hash = await Git(repo, "rev-parse", "stash@{0}"); var row = shell.Commits.Single(r => r.Commit.Hash == hash);
                var calls = runner.Calls; var menu = shell.CreateHistoryMenu(row);
                Expect(menu.Count == 3 && runner.Calls == calls, "Stash-specific menu, zero preparation");
                menu[(int)action - (int)StashAction.Apply].Command!.Execute(null); await shell.ActionTask;
                Expect(shell.CurrentAction?.Succeeded == true, shell.Status);
                if (action != StashAction.Drop) Expect(await File.ReadAllTextAsync(Path.Combine(repo, "file.txt")) == "stash\n", "Actual stash applied");
                if (action == StashAction.Apply) Expect((await Git(repo, "stash", "list")).Length > 0, "Apply keeps stash");
                else Expect(await Git(repo, "stash", "list") == "", "Pop/drop removes stash");
                if (action == StashAction.Drop) Expect(interaction.Confirmation.Contains(hash), "Drop names saved hash");
            });
        await Check("Merge banner opens Local Changes and Abort restores active merge", async () =>
        {
            await Git(repo, "switch", "-c", "other"); await File.WriteAllTextAsync(Path.Combine(repo, "file.txt"), "other\n"); await Git(repo, "commit", "-am", "other");
            await Git(repo, "switch", "main"); await File.WriteAllTextAsync(Path.Combine(repo, "file.txt"), "main\n"); await Git(repo, "commit", "-am", "main");
            await runner.RunAsync(new(git.Executable, repo, ["merge", "--no-edit", "other"]), default);
            shell.RefreshRepositoryCommand.Execute(null); await shell.WaitForIdleAsync(); Expect(shell.HasOperationBanner, "Merge detected");
            var window = new MainWindow(shell); var content = (FrameworkElement)window.Content;
            try
            {
                Arrange(content, 900, 620);
                var banner = VisualDescendants<Border>(content).Single(b => AutomationProperties.GetAutomationId(b) == "OperationBanner");
                Expect(banner.Visibility == Visibility.Visible && banner.ActualHeight > 30, "Banner occupies space above history");
                SaveRender(content, "merge-banner.png", 900, 620);
                var resolve = VisualDescendants<Button>(banner).Single(b => ReferenceEquals(b.Command, shell.ShowChangesCommand));
                resolve.Command.Execute(null); await shell.CommitDialogTask; Expect(interaction.Commits == 1, "Resolve opens Local Changes");
                shell.AbortCommand.Execute(null); await shell.ActionTask;
                Expect(!shell.HasOperationBanner && shell.CurrentAction?.Succeeded == true && await Git(repo, "ls-files", "-u") == "", "Real abort reconciles banner/index");
            }
            finally { window.Close(); }
        });
    }
    private sealed class UsabilityInteraction : IUserInteraction
    {
        public string Confirmation = ""; public int Commits;
        public string? ChooseRepository() => null;
        public string? ChooseGit() => null;
        public bool ConfirmRecovery(string message, string title) { Confirmation = message; return true; }
        public Task ShowCommitAsync(CommitViewModel model) { Commits++; return Task.CompletedTask; }
    }
    private sealed class PreviewDelayRunner : IProcessRunner
    {
        private readonly WindowsProcessRunner _inner = new();
        public bool Hold; public int Calls;
        public TaskCompletionSource Release = new(TaskCreationOptions.RunContinuationsAsynchronously);
        public async Task<ProcessResult> RunAsync(ProcessRequest request, CancellationToken token, Action<ProcessOutput>? output = null)
        {
            Interlocked.Increment(ref Calls); if (Hold) await Release.Task.WaitAsync(token);
            return await _inner.RunAsync(request, token, output);
        }
    }
}
