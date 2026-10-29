using System.IO;
using System.Text.Json;
using System.Windows;
using System.Windows.Automation;
using System.Windows.Controls;
using GitGui.App.Localization;
using GitGui.App.ViewModels;
using GitGui.App.Views;
using GitGui.Core.Diagnostics;
using GitGui.Core.Settings;

namespace GitGui.FoundationChecks;

/// <summary>Dependency-free checks; all disk writes use disposable fixture directories.</summary>
internal static partial class Program
{
    private static int _passed;
    private static int _failed;
    private static string _root = "";

    [STAThread]
    private static int Main(string[] args)
    {
        if (args.FirstOrDefault() == "--measure-history-performance") return HistoryPerformanceHarness.Run(args);
        if (args.FirstOrDefault() == "--version") { Console.WriteLine("git version 2.46.0"); return 0; }
        if (args.FirstOrDefault() == "--probe") return RunProbeAsync(args.Skip(1).ToArray()).GetAwaiter().GetResult();
        if (args.Length == 0 || args is ["--smoke"]) return RunRoutineChecks();
        if (args is ["--check-sidebar"] or ["--render-sidebar"] or ["--check-workspace"] or ["--check-operation-style"] or ["--check-history-layout"])
        {
            _historyLayoutChecks = args[0] == "--check-history-layout";
            _operationStyleChecks = args[0] == "--check-operation-style";
            _workspaceChecks = args[0] == "--check-workspace";
            _sidebarRenderOnly = args[0] == "--render-sidebar";
            _sidebarChecks = !_workspaceChecks && !_operationStyleChecks && !_historyLayoutChecks; _routine = true;
            _root = Path.Combine(Path.GetFullPath(_historyLayoutChecks ? "artifacts/history-layout-checks" : _operationStyleChecks ? "artifacts/operation-style-checks" : _workspaceChecks ? "artifacts/workspace-checks" : "artifacts/sidebar-checks"), DateTime.UtcNow.ToString("yyyyMMdd-HHmmss") + "-" + Guid.NewGuid().ToString("N"));
            Directory.CreateDirectory(_root);
            using var deadline = RoutineDeadline(TimeSpan.FromMinutes(3), "Sidebar checks exceeded deadline");
            var app = new HeadlessApplication(); app.InitializeComponent(); RunViewModelChecks();
            Console.WriteLine($"{_passed} passed; {_failed} failed. Fixtures: {_root}"); return _failed == 0 ? 0 : 1;
        }
        string[] optionalModes = ["--full", "--check-branches", "--check-drafts", "--check-history-performance", "--check-init", "--check-file-actions", "--check-file-menus", "--check-stashes", "--check-recovery", "--check-fast-changes", "--check-commit-files", "--check-hunks", "--check-theme", "--check-local", "--check-commit", "--measure-working-copy", "--measure-working-copy-assets", "--measure-actions", "--measure-history", "--measure-large-history"];
        if (args.Length != 1 || !optionalModes.Contains(args[0])) { Console.Error.WriteLine("Use --smoke (default), --full, or a documented focused/measurement command."); return 2; }
        if (args.FirstOrDefault() == "--full") args = [];
        _root = Path.Combine(Path.GetFullPath(args.FirstOrDefault() == "--measure-actions" ? "artifacts/action-benchmarks" : args.FirstOrDefault() is "--measure-history" or "--measure-large-history" ? "artifacts/history-benchmarks" : args.FirstOrDefault() == "--check-hunks" ? "artifacts/hunk-checks" : args.FirstOrDefault() == "--check-commit" ? "artifacts/commit-checks" : args.FirstOrDefault() == "--check-theme" ? "artifacts/theme-checks" : args.FirstOrDefault() == "--check-local" ? "artifacts/local-changes-checks" : args.FirstOrDefault() ?? "artifacts/foundation-checks"),
            DateTime.UtcNow.ToString("yyyyMMdd-HHmmss") + "-" + Guid.NewGuid().ToString("N"));
        if (args.FirstOrDefault() is "--measure-working-copy" or "--measure-working-copy-assets") _root = Path.Combine(Path.GetFullPath("artifacts/working-copy-benchmarks"), Path.GetFileName(_root));
        if (args.FirstOrDefault() == "--check-commit-files") _root = Path.Combine(Path.GetFullPath("artifacts/commit-files-checks"), Path.GetFileName(_root));
        if (args.FirstOrDefault() == "--check-fast-changes") _root = Path.Combine(Path.GetFullPath("artifacts/fast-changes-checks"), Path.GetFileName(_root));
        if (args.FirstOrDefault() == "--check-recovery") _root = Path.Combine(Path.GetFullPath("artifacts/recovery-checks"), Path.GetFileName(_root));
        if (args.FirstOrDefault() == "--check-stashes") _root = Path.Combine(Path.GetFullPath("artifacts/stash-checks"), Path.GetFileName(_root));
        if (args.FirstOrDefault() == "--check-file-menus") _root = Path.Combine(Path.GetFullPath("artifacts/file-menu-checks"), Path.GetFileName(_root));
        if (args.FirstOrDefault() == "--check-file-actions") _root = Path.Combine(Path.GetFullPath("artifacts/file-action-checks"), Path.GetFileName(_root));
        if (args.FirstOrDefault() == "--check-init") _root = Path.Combine(Path.GetFullPath("artifacts/init-checks"), Path.GetFileName(_root));
        if (args.FirstOrDefault() is "--check-branches" or "--check-drafts" or "--check-history-performance")
            _root = Path.Combine(Path.GetFullPath("artifacts/integration-checks"), Path.GetFileName(_root));
        Directory.CreateDirectory(_root);
        if (args.FirstOrDefault() is "--check-branches" or "--check-drafts")
        {
            var app = new HeadlessApplication(); app.InitializeComponent();
            RunViewModelChecks(branchManagementOnly: args[0] == "--check-branches", draftsOnly: args[0] == "--check-drafts");
            Console.WriteLine($"{_passed} passed; {_failed} failed. Fixtures: {_root}"); return _failed == 0 ? 0 : 1;
        }
        if (args.FirstOrDefault() == "--check-history-performance")
        {
            RunHistoryPerformanceChecksAsync().GetAwaiter().GetResult();
            Console.WriteLine($"{_passed} passed; {_failed} failed. Fixtures: {_root}"); return _failed == 0 ? 0 : 1;
        }
        if (args.FirstOrDefault() == "--check-init") { var app = new HeadlessApplication(); app.InitializeComponent(); RunViewModelChecks(initOnly: true); Console.WriteLine($"{_passed} passed; {_failed} failed. Fixtures: {_root}"); return _failed == 0 ? 0 : 1; }
        if (args.FirstOrDefault() == "--check-file-actions") { var app = new HeadlessApplication(); app.InitializeComponent(); RunViewModelChecks(fileActionsOnly: true); Console.WriteLine($"{_passed} passed; {_failed} failed. Fixtures: {_root}"); return _failed == 0 ? 0 : 1; }
        if (args.FirstOrDefault() == "--check-file-menus") { var app = new HeadlessApplication(); app.InitializeComponent(); RunViewModelChecks(fileMenusOnly: true); Console.WriteLine($"{_passed} passed; {_failed} failed. Fixtures: {_root}"); return _failed == 0 ? 0 : 1; }
        if (args.FirstOrDefault() == "--check-stashes") { var app = new HeadlessApplication(); app.InitializeComponent(); RunViewModelChecks(stashesOnly: true); Console.WriteLine($"{_passed} passed; {_failed} failed. Fixtures: {_root}"); return _failed == 0 ? 0 : 1; }
        if (args.FirstOrDefault() == "--check-recovery") { var app = new HeadlessApplication(); app.InitializeComponent(); RunViewModelChecks(recoveryOnly: true); Console.WriteLine($"{_passed} passed; {_failed} failed. Fixtures: {_root}"); return _failed == 0 ? 0 : 1; }
        if (args.FirstOrDefault() == "--check-fast-changes") { var app = new HeadlessApplication(); app.InitializeComponent(); RunViewModelChecks(fastChangesOnly: true); Console.WriteLine($"{_passed} passed; {_failed} failed. Fixtures: {_root}"); return _failed == 0 ? 0 : 1; }
        if (args.FirstOrDefault() == "--check-commit-files") { var app = new HeadlessApplication(); app.InitializeComponent(); RunViewModelChecks(commitFilesOnly: true); Console.WriteLine($"{_passed} passed; {_failed} failed. Fixtures: {_root}"); return _failed == 0 ? 0 : 1; }
        if (args.FirstOrDefault() == "--measure-working-copy") { MeasureWorkingCopyAsync().GetAwaiter().GetResult(); return 0; }
        if (args.FirstOrDefault() == "--measure-working-copy-assets") { MeasureWorkingCopyAsync(assets: true).GetAwaiter().GetResult(); return 0; }
        if (args.FirstOrDefault() == "--check-hunks")
        {
            var app = new HeadlessApplication(); app.InitializeComponent(); RunViewModelChecks(hunksOnly: true);
            Console.WriteLine($"{_passed} passed; {_failed} failed. Fixtures: {_root}"); return _failed == 0 ? 0 : 1;
        }
        if (args.FirstOrDefault() == "--measure-actions") { MeasureActions(); return 0; }
        if (args.FirstOrDefault() == "--measure-history") { MeasureShellConstruction(); RunHistoryBenchmarkAsync().GetAwaiter().GetResult(); return 0; }
        if (args.FirstOrDefault() == "--measure-large-history") { RunLargeHistoryBenchmarkAsync().GetAwaiter().GetResult(); return 0; }
        if (args.FirstOrDefault() == "--check-theme")
        {
            RunWpfChecks(); RunThemeChecks();
            Console.WriteLine($"{_passed} passed; {_failed} failed. Fixtures: {_root}"); return _failed == 0 ? 0 : 1;
        }
        if (args.FirstOrDefault() == "--check-local")
        {
            var app = new HeadlessApplication(); app.InitializeComponent(); RunViewModelChecks(localOnly: true);
            Console.WriteLine($"{_passed} passed; {_failed} failed. Fixtures: {_root}"); return _failed == 0 ? 0 : 1;
        }
        if (args.FirstOrDefault() == "--check-commit")
        {
            var app = new HeadlessApplication(); app.InitializeComponent(); RunViewModelChecks(commitOnly: true);
            Console.WriteLine($"{_passed} passed; {_failed} failed. Fixtures: {_root}"); return _failed == 0 ? 0 : 1;
        }
        RunCoreChecksAsync().GetAwaiter().GetResult();
        RunProcessChecksAsync().GetAwaiter().GetResult();
        RunAuthenticationChecksAsync().GetAwaiter().GetResult();
        RunWpfChecks();
        RunThemeChecks();
        RunViewModelChecks();
        Expect(Application.Current is HeadlessApplication { StartupSuppressed: true }, "Headless dispatcher must suppress production startup");
        Expect(!Application.Current.Windows.OfType<Window>().Any(window => window.IsVisible), "Headless checks must not display windows");
        Console.WriteLine($"{_passed} passed; {_failed} failed. Fixtures: {_root}");
        return _failed == 0 ? 0 : 1;
    }

    private static async Task RunCoreChecksAsync()
    {
        await Check("Settings round-trip, Unicode path and last-good backup", async () =>
        {
            var folder = Fixture("layout — שלום 日本語");
            var store = new SettingsStore(folder);
            Expect((await store.LoadAsync(default)).Status == SettingsLoadStatus.Defaults, "First run must use defaults");
            await store.SaveAsync(new() { WindowWidth = 1300, SelectedSection = NavigationSection.Branches }, default);
            var first = await File.ReadAllBytesAsync(store.FilePath);
            await store.SaveAsync(new() { WindowWidth = 1400, DetailsHeight = 300 }, default);
            var backup = await File.ReadAllBytesAsync(store.FilePath + ".bak");
            Expect(first.SequenceEqual(backup), "Previous valid file must be backed up");
            var reloaded = await new SettingsStore(folder).LoadAsync(default);
            Expect(reloaded.Status == SettingsLoadStatus.Loaded && reloaded.Settings.WindowWidth == 1400 &&
                reloaded.Settings.DetailsHeight == 300, "Saved layout must reload");
            Expect(!Directory.EnumerateFiles(folder, "*.tmp").Any(), "Successful writes must not leave temporary files");
        });

        foreach (var (name, json, status) in new[]
        {
            ("broken", "{", SettingsLoadStatus.Invalid),
            ("array", "[]", SettingsLoadStatus.Invalid),
            ("null", "null", SettingsLoadStatus.Invalid),
            ("string-schema", "{\"SchemaVersion\":\"one\"}", SettingsLoadStatus.Invalid),
            ("newer", "{\"SchemaVersion\":99}", SettingsLoadStatus.NewerSchema),
            ("older", "{\"SchemaVersion\":0}", SettingsLoadStatus.UnsupportedSchema),
            ("missing-schema", "{}", SettingsLoadStatus.Invalid)
        })
        {
            await Check($"Preserve {name} settings", async () =>
            {
                var store = new SettingsStore(Fixture(name));
                await File.WriteAllTextAsync(store.FilePath, json);
                var result = await store.LoadAsync(default);
                Expect(result.Status == status && !result.CanSave, "Unsafe schema must be read-only");
                await ExpectException<InvalidOperationException>(() => store.SaveAsync(new(), default));
                Expect(await File.ReadAllTextAsync(store.FilePath) == json, "Original bytes must survive");
            });
        }

        await Check("Bounded settings input", async () =>
        {
            var store = new SettingsStore(Fixture("oversized"));
            await File.WriteAllTextAsync(store.FilePath, new string(' ', 65537));
            Expect((await store.LoadAsync(default)).Status == SettingsLoadStatus.Invalid, "Oversized input must be rejected");
        });

        await Check("Invalid geometry and enum fall back safely", async () =>
        {
            var store = new SettingsStore(Fixture("geometry"));
            await File.WriteAllTextAsync(store.FilePath,
                "{\"SchemaVersion\":1,\"WindowWidth\":-10,\"DetailsHeight\":10000,\"SelectedSection\":900}");
            var result = await store.LoadAsync(default);
            Expect(result.Settings.WindowWidth == 900 && result.Settings.DetailsHeight == 360 &&
                result.Settings.SelectedSection == NavigationSection.History, "Geometry must stay within usable bounds");
        });

        await Check("Locked settings are not overwritten", async () =>
        {
            var store = new SettingsStore(Fixture("locked"));
            await using var locked = new FileStream(store.FilePath, FileMode.CreateNew, FileAccess.Write, FileShare.None);
            Expect((await store.LoadAsync(default)).Status == SettingsLoadStatus.Unavailable, "Locked input must be unavailable");
            await ExpectException<InvalidOperationException>(() => store.SaveAsync(new(), default));
        });

        await Check("Cancellation preserves existing settings", async () =>
        {
            var store = new SettingsStore(Fixture("cancel"));
            await store.LoadAsync(default);
            await store.SaveAsync(new(), default);
            var before = await File.ReadAllBytesAsync(store.FilePath);
            using var cancelled = new CancellationTokenSource();
            cancelled.Cancel();
            await ExpectException<OperationCanceledException>(() => store.LoadAsync(cancelled.Token));
            await ExpectException<OperationCanceledException>(() => store.SaveAsync(new() { WindowWidth = 1600 }, cancelled.Token));
            var after = await File.ReadAllBytesAsync(store.FilePath);
            Expect(before.SequenceEqual(after), "Cancelled save must preserve bytes");
        });

        await Check("External settings edits are preserved", async () =>
        {
            var store = new SettingsStore(Fixture("external"));
            await store.LoadAsync(default);
            await store.SaveAsync(new(), default);
            const string external = "{\"SchemaVersion\":1,\"WindowWidth\":1550}";
            await File.WriteAllTextAsync(store.FilePath, external);
            await ExpectException<IOException>(() => store.SaveAsync(new(), default));
            Expect(await File.ReadAllTextAsync(store.FilePath) == external, "Do not clobber an external edit");
        });

        await Check("Diagnostics contain allowlisted codes only", async () =>
        {
            var folder = Fixture("log-privacy");
            var log = new DiagnosticLog(folder);
            log.Record(DiagnosticEvent.Started);
            log.Record(DiagnosticEvent.SettingsSaveFailed, new IOException("secret-token https://user:password@host/private/repo"));
            await log.DisposeAsync();
            var lines = await File.ReadAllLinesAsync(Path.Combine(folder, "diagnostics.jsonl"));
            Expect(lines.Length == 2 && !log.HasFailed, "Queued events must flush on normal disposal");
            foreach (var line in lines)
            {
                using var entry = JsonDocument.Parse(line);
                Expect(entry.RootElement.EnumerateObject().Select(p => p.Name).Order()
                    .SequenceEqual(new[] { "Event", "Failure", "TimeUtc" }), "Only approved fields belong in diagnostics");
                Expect(!line.Contains("secret") && !line.Contains("password") && !line.Contains("repo"), "Exception content must not be logged");
            }
        });

        await Check("Diagnostics rotate and replace only their own previous log", async () =>
        {
            var folder = Fixture("log-rotation");
            var file = Path.Combine(folder, "diagnostics.jsonl");
            await File.WriteAllTextAsync(file, new string('x', 256 * 1024));
            await File.WriteAllTextAsync(file + ".1", "older");
            var log = new DiagnosticLog(folder);
            log.Record(DiagnosticEvent.Started);
            await log.DisposeAsync();
            Expect(new FileInfo(file + ".1").Length == 256 * 1024 && new FileInfo(file).Length < 1024,
                "Rotation must bound retained logs");
        });

        await Check("Unavailable diagnostics do not crash the application", async () =>
        {
            var invalidDirectory = Path.Combine(Fixture("log-unavailable"), "file");
            await File.WriteAllTextAsync(invalidDirectory, "fixture");
            var log = new DiagnosticLog(invalidDirectory);
            log.Record(DiagnosticEvent.Started);
            await log.DisposeAsync();
            Expect(log.HasFailed, "Logging failure must be recorded without an unhandled exception");
        });
    }

    private static void RunWpfChecks()
    {
        Check("WPF resources load and the empty shell has no enabled Git action", () =>
        {
            var app = new HeadlessApplication();
            app.InitializeComponent();
            var model = new ShellViewModel();
            var window = new MainWindow(model);
            // Load/measure XAML without showing a window. This is not a visual/runtime acceptance test.
            var content = (FrameworkElement)window.Content;
            content.Measure(new Size(1200, 800));
            content.Arrange(new Rect(0, 0, 1200, 800));
            content.UpdateLayout();
            Expect(!model.IsReady, "Loading state must be explicit");
            Expect(window.Title == UiText.Get("AppTitle"), "Window title must resolve from English resources");
            var buttons = Descendants<Button>(window).ToArray();
            var unfinishedLabels = new[] { "Stash", "Explorer", "Terminal", "Editor", "Tools" }.Select(UiText.Get).ToArray();
            Expect(!buttons.Any(button => unfinishedLabels.Contains(AutomationProperties.GetName(button))), "Unfinished toolbar actions are omitted");
            var workspace = Descendants<RadioButton>(window).Single(item => AutomationProperties.GetAutomationId(item) == "CurrentWorkspace");
            Expect(AutomationProperties.GetName(workspace) == UiText.Get("AllCommits"), "Implemented workspace has a localized accessible heading");
            Expect(!Descendants<ListBox>(window).Any(list => AutomationProperties.GetAutomationId(list) == "Navigation"), "No no-op sidebar selector remains");
            Expect(!model.CanOpenBranchMenu && !model.CanSwitchBranch && model.FetchActionHelp == UiText.Get("ActionWorkspaceStarting"), "Startup disables actions for a real state and explains it");
            model.CompleteStartup(new(new() { SelectedSection = NavigationSection.Branches }, SettingsLoadStatus.Loaded));
            Expect(model.IsReady && model.AreaTitle == UiText.Get("AllCommits"), "Retired prototype sections fall back to implemented history");
            Expect(model.BranchActionHelp == UiText.Get("ActionOpenRepository") && !model.CanOpenBranchMenu, "Empty workspace explains the missing repository");
            model.CompleteStartup(new(new(), SettingsLoadStatus.NewerSchema));
            Expect(model.Status == UiText.Get("SettingsNewer"), "Unsafe settings must be explained in UI");
            window.Close();
            return Task.CompletedTask;
        }).GetAwaiter().GetResult();
    }

    private static IEnumerable<T> Descendants<T>(DependencyObject parent) where T : DependencyObject
    {
        foreach (var child in LogicalTreeHelper.GetChildren(parent).OfType<DependencyObject>())
        {
            if (child is T match) yield return match;
            foreach (var descendant in Descendants<T>(child)) yield return descendant;
        }
    }

    // WPF queues Startup when Application is constructed, even without Application.Run.
    // Dispatcher-based checks must load resources without composing the user's real app/profile.
    private sealed class HeadlessApplication : Application
    {
        public bool StartupSuppressed { get; private set; }
        public HeadlessApplication() => ShutdownMode = ShutdownMode.OnExplicitShutdown;
        public void InitializeComponent() => Resources.MergedDictionaries.Add((ResourceDictionary)LoadComponent(
            new Uri("/Shovel;component/Resources/AppStyles.xaml", UriKind.Relative)));
        protected override void OnStartup(StartupEventArgs e) => StartupSuppressed = true;
    }

    private static string Fixture(string name)
    {
        var directory = Path.Combine(_root, name);
        Directory.CreateDirectory(directory);
        return directory;
    }

    private static async Task Check(string name, Func<Task> action)
    {
        if (_routine && _passed + _failed >= 30) throw new InvalidOperationException("Routine check cap is 30; replace a check or use an optional suite.");
        var watch = System.Diagnostics.Stopwatch.StartNew();
        using var deadline = _routine ? RoutineDeadline(TimeSpan.FromSeconds(30), "Check timeout: " + name) : null;
        Console.WriteLine($"RUN {name}");
        try { await action(); _passed++; Console.WriteLine($"PASS {name} ({watch.Elapsed.TotalMilliseconds:F0} ms)"); }
        catch (Exception exception)
        {
            _failed++; Console.Error.WriteLine($"FAIL {name} ({watch.Elapsed.TotalMilliseconds:F0} ms): {exception}");
            if (_routine) throw;
        }
    }

    private static void Expect(bool condition, string message)
    {
        if (!condition) throw new InvalidOperationException(message);
    }

    private static async Task ExpectException<T>(Func<Task> action) where T : Exception
    {
        try { await action(); }
        catch (T) { return; }
        throw new InvalidOperationException($"Expected {typeof(T).Name}");
    }
}
