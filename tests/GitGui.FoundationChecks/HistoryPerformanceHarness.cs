using System.Diagnostics;
using System.Globalization;
using System.IO;
using System.Reflection;
using System.Runtime.InteropServices;
using System.Security.Cryptography;
using System.Text.Json;
using System.Windows;
using System.Windows.Threading;
using GitGui.App.Services;
using GitGui.App.Localization;
using GitGui.App.ViewModels;
using GitGui.Core.Diagnostics;
using GitGui.Core.Git;
using GitGui.Core.Settings;
using GitGui.ProcessHost;

namespace GitGui.FoundationChecks;

internal sealed record HistoryPerformanceOptions(string Profile, int Samples, int CancelAfterMs, bool IdleFiveMinutes)
{
    public static HistoryPerformanceOptions Parse(string[] args)
    {
        if (args.Length < 2 || args[0] != "--measure-history-performance")
            throw new ArgumentException("Usage: --measure-history-performance <merge-heavy|large|million> [--samples=3] [--cancel-after-ms=50] [--idle-five-minutes]");
        _ = HistoryPerformanceSpec.For(args[1]);
        var samples = 3; var cancel = 50; var idle = false;
        var seen = new HashSet<string>(StringComparer.Ordinal);
        foreach (var argument in args.Skip(2))
        {
            var key = argument.Split('=', 2)[0];
            if (!seen.Add(key)) throw new ArgumentException("Duplicate benchmark option: " + key);
            if (argument == "--idle-five-minutes") idle = true;
            else if (argument.StartsWith("--samples=", StringComparison.Ordinal) && int.TryParse(argument[10..], NumberStyles.None, CultureInfo.InvariantCulture, out samples) && samples is >= 1 and <= 20) { }
            else if (argument.StartsWith("--cancel-after-ms=", StringComparison.Ordinal) && int.TryParse(argument[18..], NumberStyles.None, CultureInfo.InvariantCulture, out cancel) && cancel is >= 1 and <= 5000) { }
            else throw new ArgumentException("Unknown or out-of-range benchmark option: " + argument);
        }
        return new(args[1], samples, cancel, idle);
    }
}

internal sealed class HistoryPerformanceReport
{
    public int Schema { get; } = 2;
    public DateTimeOffset StartedUtc { get; } = DateTimeOffset.UtcNow;
    public string Status { get; set; } = "running";
    public string? Failure { get; set; }
    public string? FixtureRoot { get; set; }
    public string Cleanup { get; set; } = "pending";
    public object? Machine { get; set; }
    public HistoryPerformanceOptions? Options { get; set; }
    public HistoryPerformanceSpec? Fixture { get; set; }
    public List<object> Stages { get; } = [];
    public List<string> Validations { get; } = [];
    public string[] Boundaries { get; } =
    [
        "Shell stages keep the default Commit overview; warm switches report synchronous history publication separately from comparison and secondary-read drain. Empty-tree fixtures measure an empty file list, not asset/diff latency.",
        "Headless foundation host + production reader/view-model. No visible windows, input, paint, desktop startup or installed-app measurement.",
        "All Commits and 200-row pages are unchanged. Fixture generation/maintenance, CLI validation, discovery and companion-tab priming are separate from measured reads.",
        "Synthetic empty-tree commits, packed objects/refs, reachable commit graph; timestamps/parents/ref placement deterministic. No network or user repositories.",
        "OS caches/antivirus are not changed or flushed. Setup/validation warm caches. Iteration zero is first in this host, not reboot-cold startup. No p95 claim.",
        "Resource values are polling lower bounds, not native Job peaks: short-lived descendants/CPU tails can be missed. Unavailable samples are explicit. Working sets can share pages.",
        "Host memory/CPU includes WPF, allocations from setup, report data, JIT/GC and sampler overhead. It is not Shovel.exe's process footprint.",
        "50ms memory sampling runs outside the dispatcher and contributes overhead. Idle uses 1000ms sampling; CPU is normalized over all logical processors.",
        "Deep-offset probe restarts graph lanes at its offset; only sequential Next cursors validate cross-page graph continuity. It is not a full UI walk to the last page.",
        "ElapsedMs stops at action completion; resource elapsed includes sampler startup/final sampling. Nested stage trace timings overlap and must not be summed.",
        "Cancellation timer starts at production log request submission, before native process startup is observable. A completed-before-cancel race is inconclusive, never a successful cancellation sample."
    ];
}

/// <summary>Opt-in entry point. Register before Program.Main's existing argument/root dispatch.</summary>
internal static class HistoryPerformanceHarness
{
    public static int Run(string[] args)
    {
        HistoryPerformanceOptions options;
        try { options = HistoryPerformanceOptions.Parse(args); }
        catch (ArgumentException error) { Console.Error.WriteLine(error.Message); return 2; }
        if (!OperatingSystem.IsWindows() || Thread.CurrentThread.GetApartmentState() != ApartmentState.STA)
        { Console.Error.WriteLine("History performance requires the Windows STA foundation entry point."); return 2; }
        var output = Path.Combine(Path.GetFullPath("artifacts/history-performance"), DateTime.UtcNow.ToString("yyyyMMdd-HHmmss", CultureInfo.InvariantCulture) + "-" + Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(output);
        var report = new HistoryPerformanceReport { Options = options, Fixture = HistoryPerformanceSpec.For(options.Profile) };
        using var stop = new CancellationTokenSource(TimeSpan.FromHours(2));
        ConsoleCancelEventHandler onCancel = (_, e) => { e.Cancel = true; stop.Cancel(); };
        Console.CancelKeyPress += onCancel;
        var previousContext = SynchronizationContext.Current;
        var dispatcher = Dispatcher.CurrentDispatcher;
        SynchronizationContext.SetSynchronizationContext(new DispatcherSynchronizationContext(dispatcher));
        // Plain Application only. Never construct the production App or call Run/Show.
        try
        {
            var application = Application.Current ?? new Application { ShutdownMode = ShutdownMode.OnExplicitShutdown };
            application.Resources.MergedDictionaries.Add((ResourceDictionary)Application.LoadComponent(new Uri("/Shovel;component/Resources/AppStyles.xaml", UriKind.Relative)));
            var frame = new DispatcherFrame();
            async Task PumpAsync()
            {
                try { await RunAsync(report, output, stop.Token); }
                finally { frame.Continue = false; }
            }
            var task = PumpAsync();
            if (!task.IsCompleted) Dispatcher.PushFrame(frame);
            task.GetAwaiter().GetResult();
        }
        catch (Exception error)
        {
            report.Status = stop.IsCancellationRequested ? "cancelled" : "failed";
            report.Failure = Failure(error);
        }
        finally
        {
            SynchronizationContext.SetSynchronizationContext(previousContext);
            Console.CancelKeyPress -= onCancel;
            Save(report, output);
        }
        Console.WriteLine($"History performance {report.Status}. Report: {Path.Combine(output, "report.json")}");
        if (report.Failure is not null) Console.Error.WriteLine(report.Failure);
        return report.Status == "completed" && report.Cleanup == "deleted-owned-fixtures" ? 0 : stop.IsCancellationRequested ? 130 : 1;
    }

    private static async Task RunAsync(HistoryPerformanceReport report, string output, CancellationToken token)
    {
        var options = report.Options!; var spec = report.Fixture!;
        var area = new HistoryPerformanceArea(); report.FixtureRoot = area.Root;
        var cleanupAllowed = true;
        try
        {
            var runner = new HistoryPerformanceRunner(area); var trace = new PerformanceTrace();
            var reader = new RepositoryReader(runner, trace);
            var git = await MeasureAsync("git-discovery", -1, () => reader.DiscoverAsync(null, token));
            using var assembly = File.OpenRead(typeof(ShellViewModel).Assembly.Location);
            report.Machine = new
            {
                os = RuntimeInformation.OSDescription, architecture = RuntimeInformation.ProcessArchitecture.ToString(),
                runtime = RuntimeInformation.FrameworkDescription, logicalProcessors = Environment.ProcessorCount,
                processor = Environment.GetEnvironmentVariable("PROCESSOR_IDENTIFIER"),
                gcAvailableMemoryBytes = GC.GetGCMemoryInfo().TotalAvailableMemoryBytes,
                appVersion = typeof(ShellViewModel).Assembly.GetName().Version?.ToString(),
                appAssemblySha256 = Convert.ToHexString(SHA256.HashData(assembly)),
                harnessVersion = Assembly.GetExecutingAssembly().GetName().Version?.ToString(), gitVersion = git.Version,
                fixtureDriveType = new DriveInfo(Path.GetPathRoot(area.Root)!).DriveType.ToString(),
                fixtureFileSystem = new DriveInfo(Path.GetPathRoot(area.Root)!).DriveFormat,
                cachePolicy = "No flush/reboot; setup and validation warm caches; antivirus/storage model unobserved"
            };
            Console.WriteLine($"Generating {spec.Name}: {spec.Commits:N0} visible commits; Ctrl+C cancels owned work.");
            var repo = await MeasureAsync("fixture-generation-and-maintenance-excluded", -1,
                () => HistoryPerformanceFixtures.CreateAsync(area, runner, git, spec, "primary", token));
            var companion = await HistoryPerformanceFixtures.CreateAsync(area, runner, git, new("companion", 400, false, 0), "companion", token);
            CommitHistory? initial = null;
            for (var sample = 0; sample < options.Samples; sample++)
            {
                initial = await MeasureAsync("core-first-history-page", sample, () => DeadlineAsync(async ct =>
                {
                    var snapshot = await reader.InspectAsync(git, repo, ct);
                    var page = await reader.ReadInitialHistoryAsync(git, snapshot, HistoryScope.AllCommits, ct);
                    _ = page.Entries.Select(entry => new CommitRow(entry, page.Session.Repository)).ToArray();
                    return page;
                }, token));
                await ValidatePageAsync(initial);
                var first = initial;
                var refreshed = await MeasureAsync("core-refresh-unchanged", sample, () => DeadlineAsync(async ct =>
                    await reader.ReadRefreshedHistoryAsync(git, await reader.InspectAsync(git, repo, ct), first, ct), token));
                Require(refreshed.Entries.Select(e => e.Commit.Hash).SequenceEqual(first.Entries.Select(e => e.Commit.Hash)), "Unchanged refresh changed rows.");
                var second = await MeasureAsync("core-next-page", sample, () => DeadlineAsync(ct => reader.ReadHistoryPageAsync(git, first.Session, first.Next!, ct), token));
                await ValidatePageAsync(second);
                Require(!first.Entries.Select(e => e.Commit.Hash).Intersect(second.Entries.Select(e => e.Commit.Hash)).Any(), "Duplicate rows across pages.");
                Require(second.Cursor.Graph == first.Next!.Graph, "Lost sequential graph cursor.");
                var deep = await MeasureAsync("core-last-offset-probe", sample, () => DeadlineAsync(ct => reader.ReadHistoryPageAsync(git, first.Session,
                    new HistoryCursor(spec.Commits - RepositoryReader.HistoryPageSize, GraphCursor.Start), ct), token));
                await ValidatePageAsync(deep);
                Require(deep.Entries.Count == 200 && !deep.HasMore, "Last page count/end mismatch.");
                await MeasureAsync("core-cancellation-under-load", sample, () => CancelAsync(reader, runner, git, first, spec, options.CancelAfterMs, token));
            }
            // Traversal/count validation is outside timing and happens after the core read samples.
            await ValidateFixtureAsync(runner, git, repo, spec, token);
            report.Validations.Add($"CLI All Commits={spec.Commits}; merges={spec.Merges}; --all plus detached HEAD={spec.Commits + 1}; sampled pages match CLI hashes/parents/topology, sequential edge and true end.");

            var identities = new WindowsRepositoryIdentity();
            var interaction = new HistoryPerformanceInteraction(git.Executable);
            var model = new ShellViewModel(reader, interaction, identities, actions: new GitActions(runner, reader, identities));
            var shellDrained = false;
            using var shutdown = token.Register(() => dispatcherShutdown());
            void dispatcherShutdown() => Application.Current.Dispatcher.BeginInvoke(() => model.BeginShutdown());
            try
            {
                // Empty startup enables executable selection without beginning a repository read.
                model.CompleteStartup(new(new(), SettingsLoadStatus.Defaults));
                model.ChooseGitCommand.Execute(null);
                await MeasureAsync("headless-shell-first-rows-and-drain", 0, async () =>
                {
                    model.CompleteStartup(new(new() { RepositoryPaths = [repo, companion], SelectedRepositoryPath = repo }, SettingsLoadStatus.Loaded));
                    await WaitShellAsync(model, token);
                    Require(model.Commits.Count == 200, "Shell did not publish a full history page: " + model.Status);
                    return new { rows = model.Commits.Count, includes = "identity/inspection/read/format/publication plus divergence/count/sidebar/comparison drain; no window layout or paint" };
                });
                var primaryTab = model.Repositories[0]; var companionTab = model.Repositories[1];
                model.SelectedRepository = companionTab; await WaitShellAsync(model, token);
                Require(model.Commits.Count == 200, "Companion tab failed to prime.");
                for (var sample = 0; sample < options.Samples; sample++)
                {
                    // Revalidate primary just before leaving it; the cache freshness window is unchanged.
                    model.SelectedRepository = primaryTab; await WaitShellAsync(model, token);
                    model.RefreshRepositoryCommand.Execute(null); await WaitShellAsync(model, token);
                    var cachedRows = model.Commits;
                    model.SelectedRepository = companionTab; await WaitShellAsync(model, token);
                    await MeasureAsync("headless-warm-tab-switch", sample, async () =>
                    {
                        var calls = runner.Calls; var watch = Stopwatch.StartNew();
                        model.SelectedRepository = primaryTab;
                        var synchronousPublicationMs = watch.Elapsed.TotalMilliseconds;
                        Require(ReferenceEquals(model.Commits, cachedRows), "Warm switch did not publish cached rows synchronously.");
                        await WaitShellAsync(model, token);
                        Require(runner.Since(calls).All(call => call.Operation != "log"), "Fresh cached switch unexpectedly re-read history.");
                        Require(ReferenceEquals(model.Commits, cachedRows), "Comparison loading replaced cached history rows.");
                        return new { synchronousPublicationMs, calls = runner.Calls - calls, rows = model.Commits.Count, validatedAndDrainedMs = watch.Elapsed.TotalMilliseconds };
                    });
                    await MeasureAsync("headless-shell-refresh", sample, async () =>
                    {
                        var calls = runner.Calls;
                        var previousRows = model.Commits;
                        model.RefreshRepositoryCommand.Execute(null);
                        Require(ReferenceEquals(model.Commits, previousRows), "Refresh erased useful cached rows before completion.");
                        await WaitShellAsync(model, token);
                        Require(model.Commits.Count == 200 && ReferenceEquals(model.Commits, previousRows), "Unchanged refresh failed to retain row identity: " + model.Status);
                        Require(runner.Since(calls).Any(call => call.Operation == "log"), "Refresh did not re-read history.");
                        return new { rows = model.Commits.Count, includes = "history, divergence, local-count, sidebar and comparison drain" };
                    });
                }
                if (options.IdleFiveMinutes)
                    await MeasureAsync("headless-five-minute-idle", 0, async () =>
                    {
                        await WaitShellAsync(model, token);
                        var calls = runner.Calls;
                        await Task.Delay(TimeSpan.FromMinutes(5), token); // Dispatcher keeps running, no simulated activation.
                        Require(runner.Calls == calls, "Idle view launched periodic Git work.");
                        return new { seconds = 300, gitCalls = runner.Calls - calls, repositories = 2, visibleWindows = 0 };
                    }, intervalMs: 1000);
                report.Validations.Add("Cached switch published identical history rows synchronously without a history log; secondary comparison reads were drained and counted. Unchanged refresh re-read Git and retained row identity.");
            }
            finally
            {
                model.BeginShutdown();
                try { await model.WaitForIdleAsync().WaitAsync(TimeSpan.FromSeconds(60)); shellDrained = true; }
                finally { if (!shellDrained) cleanupAllowed = false; }
            }
            token.ThrowIfCancellationRequested();
            report.Status = "completed";

            async Task ValidatePageAsync(CommitHistory page)
            {
                Require(page.Session.Scope == HistoryScope.AllCommits && RepositoryReader.HistoryPageSize == 200 && page.Entries.Count <= 200, "Changed history scope/page size.");
                var cli = await runner.GitAsync(git, repo, token, "log", "--topo-order", "--no-show-signature", "--format=%H|%P", "--skip=" + page.Cursor.Offset.ToString(CultureInfo.InvariantCulture),
                    "--max-count=200", "--branches", "--remotes", "--tags", "HEAD", "--");
                var expected = cli.Split('\n', StringSplitOptions.RemoveEmptyEntries).Select(line => line.TrimEnd('\r')).ToArray();
                var actual = page.Entries.Select(entry => entry.Commit.Hash + "|" + string.Join(' ', entry.Commit.Parents)).ToArray();
                Require(actual.SequenceEqual(expected), "Page differs from CLI topology/hashes/parents.");
                Require(page.Entries.All(entry => entry.Graph.IsMerge == (entry.Commit.Parents.Count > 1) && entry.Graph.LaneCount <= CommitGraph.MaximumLanes), "Invalid graph merge/width.");
            }

            async Task<T> MeasureAsync<T>(string name, int sample, Func<Task<T>> action, int intervalMs = 50)
            {
                token.ThrowIfCancellationRequested();
                var calls = runner.Calls;
                var previousTrace = new HashSet<TimingSample>(trace.Snapshot(), ReferenceEqualityComparer.Instance);
                await using var sampler = new HistoryPerformanceSampler(intervalMs);
                var watch = Stopwatch.StartNew();
                try
                {
                    var value = await action(); watch.Stop();
                    var resources = await sampler.FinishAsync();
                    // Do not serialize full CommitHistory/RepositorySnapshot into the timing report.
                    report.Stages.Add(new { name, sample, outcome = "completed", elapsedMs = watch.Elapsed.TotalMilliseconds, resources,
                        result = value is CommitHistory history ? (object)new { rows = history.Entries.Count, offset = history.Cursor.Offset, history.HasMore } : value is GitInstallation ? "discovered" : value is string ? "generated" : (object?)value,
                        calls = runner.Since(calls), nestedTrace = trace.Snapshot().Where(timing => !previousTrace.Contains(timing)).ToArray() });
                    Console.WriteLine($"{name} [{sample}]: completed ({watch.Elapsed.TotalMilliseconds:F1} ms, headless)");
                    Save(report, output); return value;
                }
                catch (Exception error)
                {
                    watch.Stop();
                    report.Stages.Add(new { name, sample, outcome = "failed", elapsedMs = watch.Elapsed.TotalMilliseconds, failure = Failure(error), resources = await sampler.FinishAsync(), calls = runner.Since(calls) });
                    Save(report, output); throw;
                }
            }
        }
        finally
        {
            try
            {
                if (!cleanupAllowed) report.Cleanup = "retained-because-shell-did-not-drain";
                else { area.Dispose(); report.Cleanup = "deleted-owned-fixtures"; }
            }
            catch (Exception error) when (error is IOException or UnauthorizedAccessException)
            { report.Cleanup = "retained: " + error.GetType().Name; Console.Error.WriteLine("Owned fixture cleanup incomplete: " + area.Root); }
        }
    }

    private static async Task<object> CancelAsync(RepositoryReader reader, HistoryPerformanceRunner runner, GitInstallation git,
        CommitHistory first, HistoryPerformanceSpec spec, int delayMs, CancellationToken token)
    {
        using var cancel = CancellationTokenSource.CreateLinkedTokenSource(token);
        cancel.CancelAfter(TimeSpan.FromSeconds(45));
        var submitted = runner.ArmLog();
        var read = reader.ReadHistoryPageAsync(git, first.Session, new HistoryCursor(spec.Commits - 200, GraphCursor.Start), cancel.Token);
        var watch = Stopwatch.StartNew();
        try
        {
            // Propagate a pre-log failure instead of waiting forever for the submission signal.
            if (await Task.WhenAny(read, submitted) == read) { await read; return new { outcome = "completed-before-log-signal", cancelRequested = false }; }
            await Task.WhenAny(read, Task.Delay(delayMs, token));
            if (read.IsCompleted) { await read; return new { outcome = "completed-before-cancel", cancelRequested = false, configuredDelayMs = delayMs }; }
            token.ThrowIfCancellationRequested();
            watch.Restart(); cancel.Cancel();
            try { await read; return new { outcome = "completed-after-cancel-race", cancelRequested = true, cancelToDrainMs = watch.Elapsed.TotalMilliseconds }; }
            catch (GitReadException error) when (error.Failure == GitFailure.Cancelled)
            { return new { outcome = "cancelled-and-drained", cancelRequested = true, cancelToDrainMs = watch.Elapsed.TotalMilliseconds }; }
            catch (OperationCanceledException) when (cancel.IsCancellationRequested && !token.IsCancellationRequested)
            { return new { outcome = "cancelled-and-drained", cancelRequested = true, cancelToDrainMs = watch.Elapsed.TotalMilliseconds }; }
        }
        finally
        {
            runner.DisarmLog(); cancel.Cancel();
            // Never delete the fixture while a read still owns it, including outer Ctrl+C.
            try { await read; }
            catch (GitReadException error) when (error.Failure == GitFailure.Cancelled) { }
            catch (OperationCanceledException) { }
        }
    }

    internal static async Task ValidateFixtureAsync(HistoryPerformanceRunner runner, GitInstallation git, string repo, HistoryPerformanceSpec spec, CancellationToken token)
    {
        async Task<long> Count(params string[] roots) => long.Parse(await runner.GitAsync(git, repo, token, ["rev-list", "--count", .. roots, "--"]), CultureInfo.InvariantCulture);
        Require(await Count("--branches", "--remotes", "--tags", "HEAD") == spec.Commits, "All Commits fixture count mismatch.");
        Require(await Count("--min-parents=2", "--branches", "--remotes", "--tags", "HEAD") == spec.Merges, "Merge fixture count mismatch.");
        Require(await Count("--all", "HEAD") == spec.Commits + 1, "Stash-only scope sentinel missing.");
        Require(await Count("--branches", "--remotes", "--tags") == spec.Commits - 1, "Detached HEAD scope sentinel missing.");
        Require(await Count("--branches", "--tags", "HEAD") == spec.Commits - 1, "Remote-only scope sentinel missing.");
        Require(await Count("--branches", "--remotes", "HEAD") == spec.Commits - 1, "Tag-only scope sentinel missing.");
    }
    private static async Task<T> DeadlineAsync<T>(Func<CancellationToken, Task<T>> action, CancellationToken token)
    {
        using var deadline = CancellationTokenSource.CreateLinkedTokenSource(token);
        deadline.CancelAfter(TimeSpan.FromSeconds(45));
        return await action(deadline.Token);
    }
    private static async Task WaitShellAsync(ShellViewModel model, CancellationToken token)
    {
        await model.WaitForIdleAsync().WaitAsync(TimeSpan.FromSeconds(65), token);
        await Dispatcher.Yield(DispatcherPriority.Background);
        if (model.CommitFiles is not { } files) return;
        Task pending;
        do { pending = files.Pending; await pending.WaitAsync(TimeSpan.FromSeconds(65), token); } while (pending != files.Pending);
        Require(!files.Busy && files.Files.Count == 0 && files.Status == UiText.Format("CommitFilesCount", 0), "Empty-tree comparison failed to publish successfully.");
    }
    internal static void Require(bool condition, string message) { if (!condition) throw new InvalidOperationException(message); }
    private static string Failure(Exception error) => error is GitReadException git ? "GitReadException: " + git.Failure :
        error is OperationCanceledException ? "Cancelled/deadline exceeded" : error.GetType().Name + ": " + error.Message.TruncateHistoryPerformance(2048);
    private static void Save(HistoryPerformanceReport report, string output) => File.WriteAllText(Path.Combine(output, "report.json"),
        JsonSerializer.Serialize(report, new JsonSerializerOptions { WriteIndented = true }));
    private sealed class HistoryPerformanceInteraction(string executable) : IUserInteraction
    {
        public string? ChooseRepository() => null;
        public string? ChooseGit() => executable;
    }
}
