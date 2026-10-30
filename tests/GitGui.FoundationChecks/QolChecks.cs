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
    private static bool _qolChecks;
    private static async Task DrainFiles(CommitFilesViewModel model)
    { Task pending; do { pending = model.Pending; await pending; } while (pending != model.Pending); }
    private static async Task RunQolChecksAsync()
    {
        var runner = new WindowsProcessRunner(); var reader = new RepositoryReader(runner); var ids = new WindowsRepositoryIdentity();
        var git = await reader.DiscoverAsync(null, default); var repo = Fixture("qol");
        async Task<string> Git(params string[] args)
        {
            var result = await runner.RunAsync(new(git.Executable, repo, args), default);
            Expect(result.ExitCode == 0, Encoding.UTF8.GetString(result.StandardError)); return Encoding.UTF8.GetString(result.StandardOutput).Trim();
        }
        await Git("init", "-b", "main"); await Git("config", "user.name", "Qol"); await Git("config", "user.email", "qol@example.invalid"); await Git("config", "core.autocrlf", "false");
        foreach (var name in new[] { "a.txt", "b.txt" }) await File.WriteAllTextAsync(Path.Combine(repo, name), "base\n");
        await Git("add", "."); await Git("commit", "-m", "base");
        foreach (var name in new[] { "a.txt", "b.txt" }) await File.WriteAllTextAsync(Path.Combine(repo, name), name + "\n");
        await Git("commit", "-am", "changed"); await Git("commit", "--allow-empty", "-m", "empty");
        var actions = new GitActions(runner, reader, ids);
        var shell = new ShellViewModel(reader, new CommitInteraction(), ids, actions: actions);
        shell.CompleteStartup(new(new() { RepositoryPaths = [repo] }, SettingsLoadStatus.Loaded)); await shell.WaitForIdleAsync();
        var changed = shell.Commits.Single(r => r.Commit.Subject == "changed").Commit;
        var empty = shell.Commits.Single(r => r.Commit.Subject == "empty").Commit;
        await Check("Committed selection retains old comparison until replacement and clears on a real empty commit", async () =>
        {
            var gate = new QolGate(runner); var model = new CommitFilesViewModel(new(gate), git, repo, changed);
            model.Start(); await DrainFiles(model); var lines = model.Lines; var files = model.Files;
            gate.Hold = true; model.SelectCommit(empty); await gate.Started.Task;
            Expect(ReferenceEquals(model.Lines, lines) && ReferenceEquals(model.Files, files) && model.Busy, "Useful old content remains while commit list is held");
            model.SelectCommit(changed); gate.Hold = false; gate.Release.TrySetResult(); await DrainFiles(model);
            Expect(model.CommitHash == changed.Hash && model.Lines.Any(r => r.Text == "a.txt"), "Last selection wins over obsolete empty result");
            model.SelectCommit(empty); await DrainFiles(model);
            Expect(model.Files.Count == 0 && model.Lines.Count == 0 && model.Binary is null && !model.Busy, "Completed empty commit clears stale comparison"); model.Stop();
        });
        await Check("Unchanged history keeps selected row identity and user commit selection opens Commit changes", async () =>
        {
            var rows = shell.Commits; var selected = shell.SelectedCommit;
            shell.RefreshTabCommand.Execute(shell.SelectedRepository); await shell.WaitForIdleAsync();
            Expect(ReferenceEquals(rows, shell.Commits) && ReferenceEquals(selected, shell.SelectedCommit), "Refresh preserves row containers and selection");
            shell.DetailsIndex = 3; shell.SelectedCommit = shell.Commits.Single(r => r.Commit.Hash == changed.Hash);
            Expect(shell.DetailsIndex == 4 && shell.CommitFiles is not null, "Selection opens changes, not operations");
            await DrainFiles(shell.CommitFiles!);
        });
        foreach (var name in new[] { "a.txt", "b.txt" }) await File.WriteAllTextAsync(Path.Combine(repo, name), "edited " + name + "\n");
        await Check("Working file switch retains visible lines, rejects stale hunk actions and publishes only latest selection", async () =>
        {
            var gate = new QolGate(runner); var observed = new RepositoryReader(gate);
            var model = new CommitViewModel(repo, git, observed, new(gate, observed, ids), new CommitInteraction());
            await model.StartLoading(); await model.DiffTask; var lines = model.DiffRows; var rows = model.Unstaged; var selected = model.SelectedUnstaged;
            model.RefreshCommand.Execute(null); await model.LoadingTask; await model.DiffTask;
            Expect(ReferenceEquals(rows, model.Unstaged) && ReferenceEquals(selected, model.SelectedUnstaged), "Unchanged status retains focused row objects");
            lines = model.DiffRows; gate.Hold = true; model.Select(false, [model.Unstaged.Single(r => r.Path == "b.txt")]); await gate.Started.Task;
            Expect(ReferenceEquals(lines, model.DiffRows) && model.DiffTitle == "a.txt" && model.IsDiffLoading, "Previous labeled preview stays visible");
            Expect(!model.ApplyHunksCommand.CanExecute(null) && !model.NextHunkCommand.CanExecute(null), "Stale displayed hunks cannot be staged or navigated as the new selection");
            model.Select(false, [model.Unstaged.Single(r => r.Path == "a.txt")]); gate.Hold = false; gate.Release.TrySetResult(); await model.DiffTask;
            Expect(model.DiffTitle == "a.txt" && model.DiffRows.Any(r => r.Text == "edited a.txt") && !model.IsDiffLoading, "Cancelled b result cannot overwrite a");
            await model.CloseAsync();
        });
        await Check("Pane sizes migrate from schema 5 and round-trip through settings and actual Commit layout", async () =>
        {
            var folder = Fixture("qol-settings"); var store = new SettingsStore(folder);
            await File.WriteAllTextAsync(store.FilePath, "{\"SchemaVersion\":5,\"DetailsHeight\":620}");
            var loaded = await store.LoadAsync(default); Expect(loaded.Status == SettingsLoadStatus.Migrated && loaded.Settings.DetailsHeight == 620, "Migration preserves expanded history details");
            var layout = new CommitLayout { Width = 1250, Height = 850, FilesWidth = 475, UnstagedFraction = .6, MessageHeight = 240, ConflictSourcesFraction = .65 };
            await store.SaveAsync(loaded.Settings with { CommitLayout = layout, HistoryFilesWidth = 420 }, default);
            var restored = (await new SettingsStore(folder).LoadAsync(default)).Settings;
            Expect(restored.CommitLayout == layout && restored.HistoryFilesWidth == 420, "Every pane dimension survives restart");
            var model = new CommitViewModel(repo, git, reader, actions, new CommitInteraction()) { Layout = restored.CommitLayout };
            var window = new CommitWindow(model); Arrange((FrameworkElement)window.Content, 1250, 850); window.CaptureLayout();
            Expect(Math.Abs(model.Layout.FilesWidth - 475) < 1 && Math.Abs(model.Layout.UnstagedFraction - .6) < .02 && Math.Abs(model.Layout.MessageHeight - 240) < 1, "Actual layout restores split positions");
            await model.CloseAsync(); window.Close();
            Expect(new CommitLayout { Width = double.NaN, FilesWidth = -1, UnstagedFraction = 100 }.Normalize() is { Width: 1200, FilesWidth: 250, UnstagedFraction: .9 }, "Invalid layout values are bounded");
        });
        await Check("Activity categories keep selection, redact credentials and bound session history", () =>
        {
            var journal = new ActivityJournal { Repository = repo }; var user = new object(); var background = new object();
            journal.Record(new(user, "Fetch", "done", 1, OperationOutcome.Success), repo, false, "https://name:secret@example.invalid/repo");
            var selected = journal.Selected; journal.Record(new(background, "History", "done", .2, OperationOutcome.Success), repo, true);
            Expect(journal.Visible.Cast<object>().Count() == 2 && journal.Selected == selected && !selected!.Output.Contains("secret"), "All retains selected output and redacts URL credentials");
            journal.Filter = 2; Expect(journal.Visible.Cast<ActivityEntry>().Single().Background, "Background filter");
            journal.Filter = 1; Expect(!journal.Visible.Cast<ActivityEntry>().Single().Background, "User filter");
            journal.Record(new(user, "Fetch", "done", 1, OperationOutcome.Success), repo, false, new string('x', 40000));
            Expect(selected!.Output.Length < 33000, "Output bounded");
            for (var i = 0; i < 150; i++) journal.Record(new(new object(), "Read", "done", .1, OperationOutcome.Success), repo, true);
            Expect(journal.Entries.Count == 128, "Session entries bounded");
            return Task.CompletedTask;
        });
        await Check("Dark and light workspace and activity layouts retain usable compact controls", async () =>
        {
            var window = new MainWindow(shell); var content = (FrameworkElement)window.Content;
            shell.Journal.Repository = repo;
            var log = new ActivityLogView { DataContext = shell.Journal };
            foreach (var theme in new[] { AppAppearance.Dark, AppAppearance.Light })
            {
                AppTheme.Apply(Application.Current.Resources, false, theme);
                Arrange(content, 1200, 800); await Dispatcher.Yield(DispatcherPriority.ContextIdle); SaveRender(content, $"qol-workspace-{theme}.png", 1200, 800);
                Arrange(log, 1000, 460); SaveRender(log, $"qol-activity-{theme}.png", 1000, 460);
            }
            AppTheme.Apply(Application.Current.Resources, false); shell.BeginShutdown(); await shell.WaitForIdleAsync(); window.Close();
        });
    }
    private sealed class QolGate(IProcessRunner inner) : IProcessRunner
    {
        public bool Hold;
        public TaskCompletionSource Started { get; } = new(TaskCreationOptions.RunContinuationsAsynchronously);
        public TaskCompletionSource Release { get; } = new(TaskCreationOptions.RunContinuationsAsynchronously);
        public async Task<ProcessResult> RunAsync(ProcessRequest request, CancellationToken token, Action<ProcessOutput>? output = null)
        {
            if (Hold && (request.Arguments.Contains("--raw") || request.Arguments.Contains("--patch")))
            { Started.TrySetResult(); await Release.Task; token = default; }
            return await inner.RunAsync(request, token, output);
        }
    }
}
