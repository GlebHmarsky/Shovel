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
internal static class Program
{
    private static int _passed;
    private static int _failed;
    private static string _root = "";

    [STAThread]
    private static int Main(string[] args)
    {
        _root = Path.Combine(Path.GetFullPath(args.FirstOrDefault() ?? "artifacts/foundation-checks"),
            DateTime.UtcNow.ToString("yyyyMMdd-HHmmss") + "-" + Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(_root);
        RunCoreChecksAsync().GetAwaiter().GetResult();
        RunWpfChecks();
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
            var app = new GitGui.App.App();
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
            Expect(buttons.Length == 8 && buttons.All(button => !button.IsEnabled), "Future Git/tool buttons must stay disabled");
            var navigation = Descendants<ListBox>(window).Single();
            var firstItem = navigation.ItemContainerGenerator.ContainerFromIndex(0) as ListBoxItem;
            Expect(firstItem is not null && AutomationProperties.GetName(firstItem) == UiText.Get("LocalChanges"),
                "Accessible navigation names must use localized labels, not model ToString values");
            model.CompleteStartup(new(new() { SelectedSection = NavigationSection.Branches }, SettingsLoadStatus.Loaded));
            Expect(model.IsReady && model.AreaTitle == UiText.Get("Branches"), "Startup must restore selection");
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

    private static string Fixture(string name)
    {
        var directory = Path.Combine(_root, name);
        Directory.CreateDirectory(directory);
        return directory;
    }

    private static async Task Check(string name, Func<Task> action)
    {
        try { await action(); _passed++; Console.WriteLine($"PASS {name}"); }
        catch (Exception exception) { _failed++; Console.Error.WriteLine($"FAIL {name}: {exception}"); }
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
