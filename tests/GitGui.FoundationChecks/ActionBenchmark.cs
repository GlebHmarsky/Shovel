using System.Diagnostics;
using System.IO;
using System.Text.Json;
using System.Windows;
using System.Windows.Controls;
using GitGui.App.Localization;
using GitGui.App.ViewModels;
using GitGui.App.Views;
using GitGui.Core.Git;
using GitGui.ProcessHost;

namespace GitGui.FoundationChecks;

internal static partial class Program
{
    // Headless, isolated local fixture. No user paths, credentials or command arguments are recorded.
    private static void MeasureActions()
    {
        foreach (System.Collections.DictionaryEntry variable in Environment.GetEnvironmentVariables())
            if (((string)variable.Key).StartsWith("GIT_", StringComparison.OrdinalIgnoreCase)) Environment.SetEnvironmentVariable((string)variable.Key, null);
        var config = Path.Combine(_root, "empty.config"); File.WriteAllText(config, "");
        Environment.SetEnvironmentVariable("GIT_CONFIG_GLOBAL", config);
        Environment.SetEnvironmentVariable("GIT_CONFIG_NOSYSTEM", "1");
        var app = new HeadlessApplication(); app.InitializeComponent();
        var runner = new TimingRunner(); var reader = new RepositoryReader(runner);
        var git = reader.DiscoverAsync(null, default).GetAwaiter().GetResult();
        var repo = Fixture("dialog fixture");
        void Git(params string[] args) => Expect(runner.RunAsync(new(git.Executable, repo, args), default).GetAwaiter().GetResult().ExitCode == 0, "Fixture command");
        Git("init", "-b", "main");
        var tree = WriteFixtureObject(repo, "tree", []); string? tip = null;
        for (var i = 0; i < 2500; i++) tip = Commit(repo, tree, tip is null ? [] : [tip], $"Dialog timing {i}\n");
        File.WriteAllText(Path.Combine(repo, ".git", "refs", "heads", "main"), tip + "\n");
        Git("remote", "add", "origin", repo); Git("config", "branch.main.remote", "origin"); Git("config", "branch.main.merge", "refs/heads/main");
        var actions = new GitActions(runner, reader, new WindowsRepositoryIdentity());
        var samples = new List<object>();
        for (var i = 0; i < 7; i++)
        {
            var open = Stopwatch.StartNew();
            var loading = new GitActionWindow(new GitActionViewModel(repo, GitAction.Pull, actions, () => Task.CompletedTask));
            Arrange((FrameworkElement)loading.Content, 650, 520);
            var loadingDialogMs = open.Elapsed.TotalMilliseconds; loading.Close();
            runner.Calls.Clear(); var watch = Stopwatch.StartNew();
            var context = actions.PrepareAsync(git, repo, default).GetAwaiter().GetResult();
            var prepareMs = watch.Elapsed.TotalMilliseconds;
            watch.Restart();
            var model = new GitActionViewModel(context, GitAction.Pull, actions, () => Task.CompletedTask);
            var window = new GitActionWindow(model);
            Arrange((FrameworkElement)window.Content, 650, 520);
            var dialogMs = watch.Elapsed.TotalMilliseconds;
            Expect(model.Rebase && VisualDescendants<RadioButton>((FrameworkElement)window.Content)
                .Single(radio => Equals(radio.Content, UiText.Get("ActionPullRebase"))).IsChecked == true, "Rendered default is rebase");
            if (i == 0)
            {
                SaveRender((FrameworkElement)window.Content, "pull-benchmark.png", 650, 520);
                Arrange((FrameworkElement)window.Content, 520, 440);
                SaveRender((FrameworkElement)window.Content, "pull-benchmark-minimum.png", 520, 440);
            }
            window.Close();
            samples.Add(new { iteration = i, loadingDialogMs, prepareMs, dialogMs, calls = runner.Calls.ToArray() });
        }
        File.WriteAllText(Path.Combine(_root, "action-timings.json"), JsonSerializer.Serialize(samples, new JsonSerializerOptions { WriteIndented = true }));
        Console.WriteLine($"Timings: {_root}");
    }
}
