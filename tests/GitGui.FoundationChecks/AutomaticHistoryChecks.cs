using System.Diagnostics;
using System.IO;
using System.Text;
using System.Text.Json;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Media;
using System.Windows.Automation;
using System.Windows.Threading;
using GitGui.App.Localization;
using GitGui.App.ViewModels;
using GitGui.App.Views;
using GitGui.Core.Diagnostics;
using GitGui.Core.Git;
using GitGui.Core.Processes;
using GitGui.Core.Settings;
using GitGui.ProcessHost;

namespace GitGui.FoundationChecks;

internal static partial class Program
{
    private static async Task RunAutomaticHistoryChecksAsync()
    {
        if (_historyRepository is null || _longHistoryRepository is null) return;
        var identities = new WindowsRepositoryIdentity();
        var first = _historyRepository;
        var second = _longHistoryRepository;
        var firstIdentity = await identities.ReadAsync(first, default);
        var secondIdentity = await identities.ReadAsync(second, default);
        var settings = new AppSettings
        {
            RepositoryPaths = [first, second], SelectedRepositoryPath = second,
            RepositoryIdentities = [new(first, firstIdentity.Fingerprint), new(second, secondIdentity.Fingerprint)],
            RepositoryPreferences = [new(second, HistoryScope.CurrentHead)]
        };
        var trace = new PerformanceTrace();
        var runner = new AutomaticRunner();
        var interaction = new ModelInteraction(first) { Executable = _historyExecutable };
        var timings = new List<object>();
        var profile = new SettingsStore(Fixture("automatic-profile"));
        await profile.LoadAsync(default); await profile.SaveAsync(settings, default);
        var model = new ShellViewModel(new(runner, trace), interaction, identities, value => profile.SaveAsync(value, default), trace);
        await Check("Saved tabs restore selected scope and automatically load without permission", async () =>
        {
            var watch = Stopwatch.StartNew();
            var restored = await profile.LoadAsync(default);
            var settingsMs = watch.Elapsed.TotalMilliseconds;
            watch.Restart(); model.CompleteStartup(restored);
            await model.WaitForIdleAsync();
            timings.Add(new { scenario = "restored first repository", settingsMs, milliseconds = watch.Elapsed.TotalMilliseconds, stages = trace.Snapshot() });
            Expect(model.SelectedRepository?.Directory == second && model.Commits.Count == 200 && model.CaptureSession(new()).RepositoryPreferences.All(item => item.Scope == HistoryScope.AllCommits), "Startup needs no click and ignores legacy current-branch filtering");
            Expect(model.Repositories[0].Snapshot is null, "Only selected repository loads; no preloading");
        });
        await Check("Warm tab switching retains page, scope, selection and scroll without repeated Git", async () =>
        {
            model.LoadMoreHistoryCommand.Execute(null); await model.WaitForIdleAsync();
            model.SelectedCommit = model.Commits[8];
            var hash = model.SelectedCommit.Commit.Hash;
            model.SaveHistoryScroll(7, 50);
            model.SelectedRepository = model.Repositories[0]; await model.WaitForIdleAsync();
            var calls = runner.Calls;
            for (var i = 0; i < 5; i++)
            {
                var watch = Stopwatch.StartNew();
                model.SelectedRepository = model.Repositories[1];
                Expect(model.Commits.Count == 200 && model.SelectedCommit?.Commit.Hash == hash && model.HistoryRange.Contains("400"), "Cached page and selection are available synchronously");
                var immediateMs = watch.Elapsed.TotalMilliseconds;
                await model.WaitForIdleAsync();
                timings.Add(new { scenario = "warm switch", immediateMs, validatedMs = watch.Elapsed.TotalMilliseconds });
                Expect(model.HistoryScroll == (7d, 50d) && model.CaptureSession(new()).RepositoryPreferences.All(item => item.Scope == HistoryScope.AllCommits), "Scroll is per tab and all tabs show unfiltered history");
                model.SelectedRepository = model.Repositories[0]; await model.WaitForIdleAsync();
            }
            Expect(runner.Calls == calls, "Fresh cached tabs validate identity without Git calls");
        });
        await Check("Refresh retains useful rows and selection, including failure and cancellation", async () =>
        {
            model.SelectedRepository = model.Repositories[1]; await model.WaitForIdleAsync();
            var savedHash = model.SelectedCommit?.Commit.Hash;
            model.RefreshRepositoryCommand.Execute(null); await model.WaitForIdleAsync();
            Expect(model.HistoryRange.Contains("400") && model.SelectedCommit?.Commit.Hash == savedHash, "Refresh keeps the current older page when refs are unchanged");
            model.SelectedRepository = model.Repositories[0]; await model.WaitForIdleAsync();
            var original = model.Commits;
            model.SelectedCommit = original[2]; var hash = model.SelectedCommit.Commit.Hash;
            for (var i = 0; i < 3; i++)
            {
                var watch = Stopwatch.StartNew(); model.RefreshRepositoryCommand.Execute(null);
                Expect(ReferenceEquals(model.Commits, original), "Keep rows while refresh starts");
                await model.WaitForIdleAsync();
                timings.Add(new { scenario = "background refresh", milliseconds = watch.Elapsed.TotalMilliseconds });
                Expect(model.SelectedCommit?.Commit.Hash == hash, "Refresh retains selection"); original = model.Commits;
            }
            runner.FailLog = true;
            model.RefreshRepositoryCommand.Execute(null); await model.WaitForIdleAsync();
            Expect(ReferenceEquals(model.Commits, original) && model.HistoryNotice == UiText.Get("HistoryRefreshFailed"), "Failure retains rows with stale notice");
            runner.FailLog = false; runner.HoldLog = true;
            model.RefreshRepositoryCommand.Execute(null); await runner.LogStarted.Task.WaitAsync(TimeSpan.FromSeconds(10));
            model.CancelCommand.Execute(null); await model.WaitForIdleAsync();
            Expect(ReferenceEquals(model.Commits, original) && model.HistoryNotice == UiText.Get("HistoryRefreshCancelled"), "Cancellation retains prior rows");
            runner.HoldLog = false;
        });
        await Check("Settings save failure does not block local history", async () =>
        {
            var transport = new ModelRunner(first); transport.Release.TrySetResult();
            var failedSave = new ShellViewModel(new(transport), new ModelInteraction(first), new ModelIdentities(),
                _ => Task.FromException(new IOException("fixture failure")));
            failedSave.CompleteStartup(new(new(), SettingsLoadStatus.Defaults));
            failedSave.OpenRepositoryCommand.Execute(null); await failedSave.WaitForIdleAsync();
            Expect(transport.Calls > 0 && failedSave.Commits.Count == 1 && failedSave.SettingsNotice == UiText.Get("IdentitySaveFailed"), "Read-only history remains available if identity persistence fails");
        });
        await Check("Cached tab pages are bounded and evicted tabs reload automatically", async () =>
        {
            var paths = Enumerable.Range(0, 4).Select(i => Fixture("cache-tab-" + i)).ToArray();
            var transport = new ModelRunner(paths[0]); transport.Release.TrySetResult();
            // ModelRunner returns its working directory for locations so each fixture has distinct state.
            transport.UseRequestDirectory = true;
            var cached = new ShellViewModel(new(transport), new ModelInteraction(paths[0]), new ModelIdentities());
            cached.CompleteStartup(new(new() { RepositoryPaths = paths, SelectedRepositoryPath = paths[0],
                RepositoryIdentities = paths.Select(path => new RepositoryIdentityStamp(path, new string('A', 64))).ToArray() }, SettingsLoadStatus.Loaded));
            await cached.WaitForIdleAsync();
            foreach (var tab in cached.Repositories.Skip(1)) { cached.SelectedRepository = tab; await cached.WaitForIdleAsync(); }
            var calls = transport.Calls;
            cached.SelectedRepository = cached.Repositories[0];
            Expect(cached.Commits.Count == 0, "Oldest of four pages was evicted");
            await cached.WaitForIdleAsync();
            Expect(cached.Commits.Count == 1 && transport.Calls > calls, "Evicted approved tab reloads without a click");
        });
        await Check("Missing and replaced worktree identities stop reads without a consent prompt", async () =>
        {
            var path = Fixture("replacement");
            Directory.CreateDirectory(Path.Combine(path, ".git"));
            var identity = await identities.ReadAsync(path, default);
            var old = Path.Combine(path, "old-git");
            // Literal fixture paths verified below before directory move.
            Expect(Path.GetFullPath(old).StartsWith(Path.GetFullPath(_root) + Path.DirectorySeparatorChar, StringComparison.OrdinalIgnoreCase), "Disposable move only");
            Directory.Move(Path.Combine(path, ".git"), old); Directory.CreateDirectory(Path.Combine(path, ".git"));
            Expect((await identities.ReadAsync(path, default)).Fingerprint != identity.Fingerprint, "Directory file identity detects replacement at the same path");
            var calls = runner.Calls;
            foreach (var target in new[] { path, Path.Combine(_root, "missing") })
            {
                var changed = new ShellViewModel(new(runner), interaction, identities);
                changed.CompleteStartup(new(new() { RepositoryPaths = [target], SelectedRepositoryPath = target, RepositoryIdentities = [new(target, identity.Fingerprint)] }, SettingsLoadStatus.Loaded));
                await changed.WaitForIdleAsync();
                Expect(changed.Commits.Count == 0 && changed.HistoryNotice == UiText.Get(target == path ? "IdentityChanged" : "IdentityUnavailable"), "Changed/missing targets show a concrete error");
            }
            Expect(runner.Calls == calls, "Filesystem validation rejects changes before Git starts");
        });
        await Check("Schema 2/3 migration preserves tabs and replacement stamps, without read permissions", async () =>
        {
            var store = new SettingsStore(Fixture("approval-migration"));
            await File.WriteAllTextAsync(store.FilePath, JsonSerializer.Serialize(settings with { SchemaVersion = 2 }));
            var result = await store.LoadAsync(default);
            Expect(result.Status == SettingsLoadStatus.Migrated && result.Settings.RepositoryIdentities.Length == 0 && result.Settings.RepositoryPaths.Length == 2, "Migration preserves tabs but grants no approval");
            await store.SaveAsync(result.Settings, default);
            Expect((await new SettingsStore(Path.GetDirectoryName(store.FilePath)!).LoadAsync(default)).Settings.SchemaVersion == AppSettings.CurrentSchemaVersion && File.Exists(store.FilePath + ".bak"), "Current schema save retains pre-migration backup");
            var legacy = JsonSerializer.Serialize(settings with { SchemaVersion = 3 }).Replace("RepositoryIdentities", "Approvals");
            await File.WriteAllTextAsync(store.FilePath, legacy);
            var migrated = await new SettingsStore(Path.GetDirectoryName(store.FilePath)!).LoadAsync(default);
            Expect(migrated.Settings.RepositoryIdentities.SequenceEqual(settings.RepositoryIdentities), "Old fingerprints survive as replacement stamps only");

        });
        await Check("Same-author highlighting uses author identity and accessible non-color semantics", async () =>
        {
            var commit = new CommitRecord("abcdef", [], "A", "a@example.invalid", DateTimeOffset.UnixEpoch, "C", "c@example.invalid", DateTimeOffset.UnixEpoch, "Subject");
            CommitRow Row(CommitRecord value) => new(new(value, CommitGraph.Build([value])[0]));
            var row = Row(commit);
            Expect(row.HasSameAuthor(Row(commit with { Committer = "Different" })) && !row.HasSameAuthor(Row(commit with { AuthorEmail = "other@example.invalid" })) &&
                !row.HasSameAuthor(Row(commit with { Author = "Different" })) && !Row(commit with { AuthorEmail = "" }).HasSameAuthor(Row(commit with { AuthorEmail = "" })), "Exact author name/email; no committer or blank-email matching");
            var authorRepo = Fixture("author-highlighting");
            var setup = new WindowsProcessRunner();
            var initialized = await setup.RunAsync(new(_historyExecutable!, authorRepo, ["init", "--initial-branch=main", "--object-format=sha1"]), default);
            Expect(initialized.ExitCode == 0, "Initialize disposable author fixture");
            var tree = WriteFixtureObject(authorRepo, "tree", []);
            var root = Commit(authorRepo, tree, [], "Original author root\n");
            var same = Commit(authorRepo, tree, [root], "Selected author\n");
            var different = WriteFixtureObject(authorRepo, "commit", Encoding.UTF8.GetBytes($"tree {tree}\nparent {same}\nauthor Other author <other@example.invalid> 1792747800 +0200\ncommitter Committer <author@example.invalid> 1792749600 +0000\n\nUnrelated author\n"));
            var tip = Commit(authorRepo, tree, [different], "Same author peer\n");
            await File.WriteAllTextAsync(Path.Combine(authorRepo, ".git", "refs", "heads", "main"), tip + "\n");
            var authorIdentity = await identities.ReadAsync(authorRepo, default);
            var rendered = new ShellViewModel(new(new WindowsProcessRunner()), interaction, identities);
            rendered.CompleteStartup(new(new() { RepositoryPaths = [authorRepo], SelectedRepositoryPath = authorRepo,
                RepositoryIdentities = [new(authorRepo, authorIdentity.Fingerprint)] }, SettingsLoadStatus.Loaded)); await rendered.WaitForIdleAsync();
            rendered.SelectedCommit = rendered.Commits[2];
            Expect(!rendered.SelectedCommit.IsSameAuthor && rendered.Commits[0].IsSameAuthor && !rendered.Commits[1].IsSameAuthor && rendered.Commits[3].IsSameAuthor, "Selected and unrelated rows remain normal; author peers match");
            Expect(rendered.Commits[0].AccessibilityLabel.Contains("Same author"), "Accessible relation is explicit");
            var watch = Stopwatch.StartNew(); var window = new MainWindow(rendered); var creationMs = watch.Elapsed.TotalMilliseconds;
            try
            {
                var content = (FrameworkElement)window.Content;
                watch.Restart(); Arrange(content, 1200, 760); await Dispatcher.Yield(DispatcherPriority.ContextIdle);
                timings.Add(new { scenario = "offscreen window creation and first layout", creationMs, layoutMs = watch.Elapsed.TotalMilliseconds });
                var table = VisualDescendants<DataGrid>(content).Single(grid => System.Windows.Automation.AutomationProperties.GetAutomationId(grid) == "CommitHistory");
                Expect(VisualDescendants<Menu>(content).Any() && !VisualDescendants<Button>(content).Any(button => button.Command == rendered.RefreshRepositoryCommand), "No inspection toolbar; a real menu exists");
                var normalPeer = VisualDescendants<TextBlock>(table).First(text => text.Text == rendered.Commits[0].Description);
                var unrelatedText = VisualDescendants<TextBlock>(table).First(text => text.Text == rendered.Commits[1].Description);
                Expect(normalPeer.Foreground is SolidColorBrush ordinary && ordinary.Color == Colors.White &&
                    unrelatedText.Foreground is SolidColorBrush white && white.Color == Colors.White, "Peer and unrelated text keep their ordinary color");
                SaveRender(content, "automatic-history.png", 1200, 760);
                Arrange(content, 900, 580); await Dispatcher.Yield(DispatcherPriority.ContextIdle);
                SaveRender(content, "automatic-history-minimum.png", 900, 580);
                Expect(table.ActualHeight >= 120, "Minimum window retains useful history viewport");
                window.HighContrast = true; Arrange(content, 900, 580); await Dispatcher.Yield(DispatcherPriority.ContextIdle);
                var peerText = VisualDescendants<TextBlock>(table).First(text => text.Text == rendered.Commits[0].Description);
                Expect(peerText.Foreground is SolidColorBrush peerBrush && peerBrush.Color != Color.FromRgb(255, 242, 168), "High contrast disables yellow override");
                SaveRender(content, "automatic-history-high-contrast.png", 900, 580);
                rendered.SelectedCommit = null;
                Expect(rendered.Commits.All(item => !item.IsSameAuthor), "Clearing selection clears relation");
            }
            finally { window.Close(); }
        });
        Expect(!File.Exists(Path.Combine(first, "verifier-ran")), "Automatic reads did not execute the repository-configured signature verifier");
        await File.WriteAllTextAsync(Path.Combine(_root, "automatic-history-timings.json"), JsonSerializer.Serialize(timings, new JsonSerializerOptions { WriteIndented = true }));
    }

    private sealed class AutomaticRunner : IProcessRunner
    {
        private readonly WindowsProcessRunner _inner = new();
        public int Calls;
        public bool FailLog, HoldLog;
        public TaskCompletionSource LogStarted { get; } = new(TaskCreationOptions.RunContinuationsAsynchronously);
        public async Task<ProcessResult> RunAsync(ProcessRequest request, CancellationToken token, Action<ProcessOutput>? output = null)
        {
            Interlocked.Increment(ref Calls);
            if (request.Arguments.Contains("log"))
            {
                if (FailLog) return new(ProcessOutcome.Exited, 1, [], Encoding.UTF8.GetBytes("fixture error"), TimeSpan.Zero);
                if (HoldLog) { LogStarted.TrySetResult(); await Task.Delay(Timeout.Infinite, token); }
            }
            return await _inner.RunAsync(request, token, output);
        }
    }
}
