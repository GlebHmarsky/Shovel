using System.IO;
using System.Text;
using System.Windows;
using System.Windows.Controls;
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
    private static bool _historyActivityChecks;
    private static async Task RunHistoryActivityChecksAsync()
    {
        await Check("History reference placement persists and failed saves retain the active choice", async () =>
        {
            var store = new SettingsStore(Fixture("history-ref-settings"));
            var shell = new ShellViewModel(persist: settings => store.SaveAsync(settings, default));
            shell.CompleteStartup(await store.LoadAsync(default));
            Expect(!shell.HistoryRefsOnRight, "Old profiles default left");
            Expect(await shell.SaveSettingsAsync(true, 60, new(), true), "Right placement saves");
            var restored = new ShellViewModel(); restored.CompleteStartup(await store.LoadAsync(default));
            Expect(restored.HistoryRefsOnRight, "Right preference survives restart");
            await shell.SaveAutoFetchSettingsAsync(false, 90);
            Expect((await store.LoadAsync(default)).Settings.HistoryRefsOnRight, "Other settings preserve placement");
            var failed = new ShellViewModel(persist: _ => throw new IOException()); failed.CompleteStartup(new(new(), SettingsLoadStatus.Loaded));
            Expect(!await failed.SaveSettingsAsync(true, 60, new(), true) && !failed.HistoryRefsOnRight, "Failure preserves active setting");
            shell.BeginShutdown(); restored.BeginShutdown(); failed.BeginShutdown();
        });

        var runner = new WindowsProcessRunner(); var reader = new RepositoryReader(runner); var ids = new WindowsRepositoryIdentity();
        var git = await reader.DiscoverAsync(null, default); var actions = new GitActions(runner, reader, ids);
        var repo = Fixture("history-activity"); var remote = Fixture("history-activity-remote");
        async Task<string> Git(string path, params string[] args)
        {
            var result = await runner.RunAsync(new(git.Executable, path, args), default);
            Expect(result.ExitCode == 0, Encoding.UTF8.GetString(result.StandardError));
            return Encoding.UTF8.GetString(result.StandardOutput).Trim();
        }
        await Git(repo, "init", "-b", "main"); await Git(repo, "config", "user.name", "Fixture"); await Git(repo, "config", "user.email", "fixture@example.invalid");
        await File.WriteAllTextAsync(Path.Combine(repo, "file.txt"), "base\n"); await Git(repo, "add", "."); await Git(repo, "commit", "-m", "visible message");
        await Git(remote, "init", "--bare"); await Git(repo, "remote", "add", "origin", remote);
        await Check("Bound history badges change side without replacing retained history rows", async () =>
        {
            var shell = new ShellViewModel(reader, new CommitInteraction(), ids, actions: actions);
            shell.CompleteStartup(new(new() { RepositoryPaths = [repo], AutoFetchEnabled = false }, SettingsLoadStatus.Loaded)); await shell.WaitForIdleAsync();
            var row = shell.Commits[0]; var window = new MainWindow(shell); var content = (FrameworkElement)window.Content;
            try
            {
                foreach (var right in new[] { false, true })
                {
                    Expect(await shell.SaveSettingsAsync(false, 60, new(), right), "Placement applies");
                    Arrange(content, 1200, 800); await Dispatcher.Yield(DispatcherPriority.ContextIdle);
                    var badges = VisualDescendants<ItemsControl>(content).First(control => ReferenceEquals(control.ItemsSource, row.Badges));
                    Expect(DockPanel.GetDock(badges) == (right ? Dock.Right : Dock.Left), "Actual history cell applies configured badge side");
                    Expect(ReferenceEquals(row, shell.Commits[0]), "Rows and selection remain retained");
                    SaveRender(content, "history-refs-" + (right ? "right" : "left") + ".png", 1200, 800);
                }
            }
            finally { shell.BeginShutdown(); await shell.WaitForIdleAsync(); window.Close(); }
        });
        await Check("Quiet native fetch retains an actual command transcript in the activity output pane", async () =>
        {
            var context = await actions.PrepareAsync(git, repo, default);
            var model = new GitActionViewModel(context, GitAction.Fetch, actions, () => Task.CompletedTask);
            var journal = new ActivityJournal { Repository = repo };
            model.PropertyChanged += (_, _) => { if (model.Activity is { } activity) journal.Record(activity, repo, model.IsBackground, model.RawOutput); };
            model.RunCommand.Execute(null); await model.RunningTask;
            Expect(model.Succeeded && model.RawOutput.Contains("$ git fetch "), "Quiet success has command header");
            var log = new ActivityLogView { DataContext = journal }; Arrange(log, 1000, 480); await Dispatcher.Yield(DispatcherPriority.ContextIdle);
            Expect(VisualDescendants<TextBox>(log).Single().Text.Contains("$ git fetch "), "Actual selected-output binding shows native command");
            SaveRender(log, "activity-fetch-output.png", 1000, 480);
            Expect(await Git(repo, "write-tree") is { Length: 40 }, "Native stdout remains parseable");
        });
        await Check("Activity selection updates live, retains background output and excludes sensitive command values", async () =>
        {
            var journal = new ActivityJournal { Repository = repo }; var first = new object(); var second = new object();
            journal.Record(new(first, "Fetch", "Running", 0, OperationOutcome.Running), repo, false, "$ git fetch origin\nfirst output");
            var log = new ActivityLogView { DataContext = journal }; Arrange(log, 1000, 480); await Dispatcher.Yield(DispatcherPriority.ContextIdle);
            journal.Record(new(first, "Fetch", "Running", 0, OperationOutcome.Running), repo, false, "$ git fetch origin\nsecond output");
            await Dispatcher.Yield(DispatcherPriority.ContextIdle);
            Expect(VisualDescendants<TextBox>(log).Single().Text.Contains("second output"), "Selected running entry updates right pane");
            journal.Record(new(second, "Background fetch", "Completed", 0, OperationOutcome.Success), repo, true, "$ git fetch origin\nbackground output");
            journal.Filter = 2; await Dispatcher.Yield(DispatcherPriority.ContextIdle);
            Expect(VisualDescendants<TextBox>(log).Single().Text.Contains("background output"), "Background filter selects real retained output");
            var formatted = GitActions.FormatCommand(["--no-pager", "-c", "secret.key=private", "commit", "-m", "private message"]);
            Expect(formatted.Contains("$ git commit") && !formatted.Contains("private") && !formatted.Contains("secret.key"), "Command header excludes configuration and message values");
            formatted = GitActions.FormatCommand(["fetch", "https://name:password@example.invalid/repo?token=private"]);
            Expect(!formatted.Contains("password") && !formatted.Contains("private"), "URL credentials/query redacted");
            journal.Record(new(second, "Background fetch", "Completed", 0, OperationOutcome.Success), repo, true, new string('x', ActivityJournal.OutputLimit + 100));
            Expect(journal.Selected!.Output.Length < ActivityJournal.OutputLimit + 200, "Retained output remains bounded");
        });
    }
}
