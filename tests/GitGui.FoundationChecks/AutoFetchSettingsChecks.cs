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
    private static bool _autoFetchSettingsChecks;
    private static async Task RunAutoFetchSettingsChecksAsync()
    {
        await Check("Auto-fetch preferences default old profiles and survive a settings-store restart", async () =>
        {
            var directory = Fixture("auto-fetch-settings-profile"); var store = new SettingsStore(directory);
            await File.WriteAllTextAsync(store.FilePath, "{\"SchemaVersion\":6,\"UiScale\":1.3,\"UpdateChannel\":\"win-beta\"}");
            var loaded = await store.LoadAsync(default);
            Expect(loaded.CanSave && loaded.Settings.AutoFetchEnabled && loaded.Settings.AutoFetchIntervalSeconds == 10, "Existing schema-6 profiles retain the original default");
            await store.SaveAsync(loaded.Settings with { AutoFetchEnabled = false, AutoFetchIntervalSeconds = 75 }, default);
            var restarted = await new SettingsStore(directory).LoadAsync(default);
            Expect(!restarted.Settings.AutoFetchEnabled && restarted.Settings.AutoFetchIntervalSeconds == 75 && restarted.Settings.UiScale == 1.3 && restarted.Settings.UpdateChannel == "win-beta", "Both preferences round-trip without disturbing other settings");
            Expect(new AppSettings { AutoFetchIntervalSeconds = -1 }.Normalize().AutoFetchIntervalSeconds == 10 &&
                new AppSettings { AutoFetchIntervalSeconds = int.MaxValue }.Normalize().AutoFetchIntervalSeconds == 3600, "Out-of-range persisted intervals stay within supported bounds");
        });
        await Check("Settings validates whole-second intervals and closes only after successful saving", async () =>
        {
            var calls = 0; var succeed = false; var saved = 0;
            var model = new SettingsViewModel(true, 10, true, (_, _) => { calls++; return Task.FromResult(succeed); });
            model.Saved += (_, _) => saved++;
            Expect(calls == 0, "Opening the model performs no save or preparation");
            foreach (var text in new[] { "", "abc", "9", "3601", "10.5" })
            {
                model.IntervalSeconds = text;
                Expect(!model.SaveCommand.CanExecute(null), "Invalid interval cannot submit: " + text);
            }
            model.IntervalSeconds = "60";
            Expect(model.SaveCommand.CanExecute(null), "Valid edit can submit");
            model.SaveCommand.Execute(null); await model.SaveTask;
            Expect(calls == 1 && saved == 0 && model.Error.Length > 0 && model.IntervalSeconds == "60", "Save failure keeps typed value and error visible");
            succeed = true; model.SaveCommand.Execute(null); await model.SaveTask;
            Expect(calls == 2 && saved == 1, "Successful retry signals close exactly once");
            var protectedModel = new SettingsViewModel(true, 10, false, (_, _) => { calls++; return Task.FromResult(true); });
            Expect(!protectedModel.SaveCommand.CanExecute(null) && calls == 2, "Protected profile cannot be overwritten");
        });
        await Check("Shell applies saved preferences only after durable save and restores them on startup", async () =>
        {
            var directory = Fixture("auto-fetch-shell-profile"); var store = new SettingsStore(directory);
            var loaded = await store.LoadAsync(default);
            var shell = new ShellViewModel(persist: settings => store.SaveAsync(settings, default));
            shell.CompleteStartup(loaded);
            Expect(await shell.SaveAutoFetchSettingsAsync(false, 90), "Settings submission succeeds");
            var persisted = await new SettingsStore(directory).LoadAsync(default);
            var restored = new ShellViewModel(); restored.CompleteStartup(persisted);
            Expect(!shell.AutoFetchEnabled && !restored.AutoFetchEnabled && restored.AutoFetchIntervalSeconds == 90, "Live scheduler and restarted shell use the saved preferences");
            var failed = new ShellViewModel(persist: _ => throw new IOException("fixture save failure"));
            failed.CompleteStartup(new(new(), SettingsLoadStatus.Loaded));
            Expect(!await failed.SaveAutoFetchSettingsAsync(false, 60) && failed.AutoFetchEnabled && failed.AutoFetchIntervalSeconds == 10, "Failed save leaves existing scheduler behavior intact");
            shell.BeginShutdown(); restored.BeginShutdown(); failed.BeginShutdown();
        });
        await Check("Overlapping preference saves preserve the newly chosen auto-fetch settings in either order", async () =>
        {
            foreach (var settingsFirst in new[] { false, true })
            {
                var entered = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
                var release = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
                var writes = new List<AppSettings>(); var calls = 0; var active = 0; var maximumActive = 0;
                var shell = new ShellViewModel(persist: async settings =>
                {
                    maximumActive = Math.Max(maximumActive, ++active);
                    try
                    {
                        if (++calls == 1) { entered.TrySetResult(); await release.Task; }
                        writes.Add(settings);
                    }
                    finally { active--; }
                });
                shell.CompleteStartup(new(new(), SettingsLoadStatus.Loaded));
                try
                {
                    Task<bool> save; Task other;
                    if (settingsFirst)
                    {
                        save = shell.SaveAutoFetchSettingsAsync(false, 60);
                        await entered.Task.WaitAsync(TimeSpan.FromSeconds(3));
                        shell.SaveScalePreference(); other = shell.PreferenceTask;
                    }
                    else
                    {
                        shell.SaveScalePreference(); other = shell.PreferenceTask;
                        await entered.Task.WaitAsync(TimeSpan.FromSeconds(3));
                        save = shell.SaveAutoFetchSettingsAsync(false, 60);
                    }
                    Expect(shell.AutoFetchEnabled, "Pending persistence has not yet changed the scheduler");
                    release.TrySetResult(); await Task.WhenAll(save, other);
                    Expect(await save && writes.Count == 2 && maximumActive == 1, "Preference writes run serially and both complete");
                    Expect(!writes[^1].AutoFetchEnabled && writes[^1].AutoFetchIntervalSeconds == 60 && !shell.AutoFetchEnabled,
                        "The last persisted snapshot retains the newer settings regardless of admission order");
                }
                finally { release.TrySetResult(); shell.BeginShutdown(); }
            }
        });
        await Check("Settings controls bind saved values and render without opening a desktop window", async () =>
        {
            var saves = 0;
            foreach (var theme in new[] { AppAppearance.Dark, AppAppearance.Light })
            {
                AppTheme.Apply(Application.Current.Resources, false, theme);
                var model = new SettingsViewModel(false, 75, true, (_, _) => { saves++; return Task.FromResult(true); });
                var window = new SettingsWindow(model); var content = (FrameworkElement)window.Content;
                try
                {
                    foreach (var size in new[] { (600, 360), (400, 260) })
                    {
                        Arrange(content, size.Item1, size.Item2); await Dispatcher.Yield(DispatcherPriority.ContextIdle);
                        Expect(((CheckBox)window.FindName("AutoFetchEnabledInput")).IsChecked == false &&
                            ((TextBox)window.FindName("AutoFetchIntervalInput")).Text == "75", "Saved values reach actual input controls");
                        SaveRender(content, $"auto-fetch-settings-{theme}-{size.Item1}.png", size.Item1, size.Item2);
                    }
                }
                finally { window.Close(); }
            }
            AppTheme.Apply(Application.Current.Resources, false);
            Expect(saves == 0, "Open/render/dismiss has no persistence side effects");
        });

        var runner = new AutoFetchPreferenceRunner(); var reader = new RepositoryReader(runner); var identities = new WindowsRepositoryIdentity();
        var git = await reader.DiscoverAsync(null, default); var actions = new GitActions(runner, reader, identities);
        async Task Git(string path, params string[] args)
        {
            var result = await runner.Inner.RunAsync(new(git.Executable, path, args), default);
            Expect(result.ExitCode == 0, Encoding.UTF8.GetString(result.StandardError));
        }
        var source = Fixture("auto-fetch-source"); var first = Fixture("auto-fetch-first"); var second = Fixture("auto-fetch-second");
        await Git(source, "init", "-b", "main"); await File.WriteAllTextAsync(Path.Combine(source, "file.txt"), "base\n"); await Git(source, "add", ".");
        await Git(source, "-c", "user.name=Fixture", "-c", "user.email=fixture@example.invalid", "commit", "-m", "base");
        await Git(first, "clone", "--no-hardlinks", source, "."); await Git(second, "clone", "--no-hardlinks", source, ".");
        var interaction = new AutoFetchPreferenceInteraction();
        var live = new ShellViewModel(reader, interaction, identities, actions: actions);
        live.CompleteStartup(new(new() { RepositoryPaths = [first, second], AutoFetchEnabled = false }, SettingsLoadStatus.Loaded));
        await live.WaitForIdleAsync(); var firstTab = live.Repositories[0]; var secondTab = live.Repositories[1];
        live.SelectedRepository = secondTab; await live.WaitForIdleAsync(); live.SelectedRepository = firstTab; await live.WaitForIdleAsync();
        try
        {
            await Check("Disabled auto-fetch performs no scheduled network work while manual Fetch still works", async () =>
            {
                var before = runner.Fetches;
                await live.AutoFetchDueAsync(DateTimeOffset.UtcNow + TimeSpan.FromDays(1));
                Expect(runner.Fetches == before && !live.AutoFetchEnabled, "Restored disabled state stops admission before any Git fetch");
                live.DefaultFetchCommand.Execute(null); await live.ActionTask; await live.WaitForIdleAsync();
                Expect(runner.Fetches == before + 1 && live.CurrentAction?.Succeeded == true && live.CurrentAction.IsBackground == false, "Manual fetch executes normally when automatic fetch is disabled");
            });
            await Check("Changing auto-fetch intervals resets the schedule without immediate fetch or local status scans", async () =>
            {
                await live.ConfigureAutoFetchAsync(true, 60); var before = runner.Fetches; var now = DateTimeOffset.UtcNow;
                var statusCalls = runner.StatusCalls;
                await live.AutoFetchDueAsync(now + TimeSpan.FromSeconds(40));
                Expect(runner.Fetches == before, "Enable waits for the selected interval");
                await live.AutoFetchDueAsync(now + TimeSpan.FromSeconds(60));
                Expect(runner.Fetches == before + 2, "Both inspected repositories fetch at the new deadline");
                await live.ConfigureAutoFetchAsync(true, 30); before = runner.Fetches; now = DateTimeOffset.UtcNow;
                await live.AutoFetchDueAsync(now + TimeSpan.FromSeconds(20));
                Expect(runner.Fetches == before, "Interval change starts a fresh delay");
                await live.AutoFetchDueAsync(now + TimeSpan.FromSeconds(30));
                Expect(runner.Fetches == before + 2, "Shortened interval is used by scheduler");
                await live.AutoFetchDueAsync(now + TimeSpan.FromSeconds(59));
                Expect(runner.Fetches == before + 2, "No duplicate admission before the next interval");
                Expect(runner.StatusCalls == statusCalls && interaction.AuthPrompts == 0, "Settings preserve background behavior: no local scan or credential prompt");
                Expect(runner.LastFetch?.Environment.GetValueOrDefault("GCM_INTERACTIVE") == "never", "Automatic fetch uses noninteractive credentials");
            });
            await Check("Disabling cancels and drains active background fetch without cancelling another repository's manual work", async () =>
            {
                var manualGate = runner.Hold(first); var backgroundGate = runner.Hold(second);
                live.SelectedRepository = firstTab; await live.WaitForIdleAsync();
                var problems = interaction.Problems;
                live.DefaultFetchCommand.Execute(null); var manual = live.ActionTask;
                await manualGate.Started.Task.WaitAsync(TimeSpan.FromSeconds(8));
                var automatic = live.AutoFetchDueAsync(DateTimeOffset.UtcNow + TimeSpan.FromHours(2));
                await backgroundGate.Started.Task.WaitAsync(TimeSpan.FromSeconds(8));
                runner.HoldCancelledRefreshFor = second;
                Expect(await live.SaveAutoFetchSettingsAsync(false, 30).WaitAsync(TimeSpan.FromSeconds(5)) && !live.AutoFetchEnabled,
                    "Settings save completes and disables admission without waiting for cancelled Git reconciliation");
                await runner.CancelledRefreshStarted.Task.WaitAsync(TimeSpan.FromSeconds(8));
                Expect(!automatic.IsCompleted && !live.PreferenceTask.IsCompleted && !manual.IsCompleted,
                    "Background drain stays tracked after the dialog may close; independent manual work is still pending");
                runner.CancelledRefreshRelease.TrySetResult(); await live.PreferenceTask; await automatic;
                Expect(backgroundGate.Cancelled && !manualGate.Cancelled && !manual.IsCompleted && live.HasUserOperation, "Only background work was cancelled and drained");
                var calls = runner.Fetches;
                await live.AutoFetchDueAsync(DateTimeOffset.UtcNow + TimeSpan.FromDays(2));
                Expect(runner.Fetches == calls, "No new background requests after disable");
                manualGate.Release.TrySetResult(); await manual;
                Expect(live.CurrentAction?.Succeeded == true && interaction.Problems == problems, "Manual work finishes normally; explicit background cancellation stays quiet");
            });
        }
        finally
        {
            runner.ReleaseAll(); live.BeginShutdown(); await live.StopAutoFetchAsync(); await live.WaitForIdleAsync();
        }
    }

    private sealed class AutoFetchPreferenceInteraction : IUserInteraction
    {
        public int AuthPrompts, Problems;
        public string? ChooseRepository() => null;
        public string? ChooseGit() => null;
        public void ShowOperationProblem(string repository, string title, string status, string output) => Problems++;
        public Task<AuthenticationAnswer?> ShowAuthenticationAsync(string context, AuthenticationPrompt prompt, CancellationToken token)
        { AuthPrompts++; return Task.FromResult<AuthenticationAnswer?>(null); }
    }
    private sealed class AutoFetchPreferenceRunner : IProcessRunner
    {
        public WindowsProcessRunner Inner { get; } = new();
        public int Fetches, StatusCalls;
        public ProcessRequest? LastFetch;
        public string? HoldCancelledRefreshFor;
        public TaskCompletionSource CancelledRefreshStarted { get; } = new(TaskCreationOptions.RunContinuationsAsynchronously);
        public TaskCompletionSource CancelledRefreshRelease { get; } = new(TaskCreationOptions.RunContinuationsAsynchronously);
        private readonly Dictionary<string, FetchGate> _gates = new(StringComparer.OrdinalIgnoreCase);
        public FetchGate Hold(string path) => _gates[path] = new();
        public void ReleaseAll() { CancelledRefreshRelease.TrySetResult(); foreach (var gate in _gates.Values) gate.Release.TrySetResult(); }
        public async Task<ProcessResult> RunAsync(ProcessRequest request, CancellationToken token, Action<ProcessOutput>? output = null)
        {
            if (string.Equals(request.WorkingDirectory, HoldCancelledRefreshFor, StringComparison.OrdinalIgnoreCase) &&
                _gates.TryGetValue(request.WorkingDirectory, out var cancelledGate) && cancelledGate.Cancelled)
            {
                CancelledRefreshStarted.TrySetResult(); await CancelledRefreshRelease.Task.WaitAsync(token);
            }
            if (request.Arguments.Contains("status")) Interlocked.Increment(ref StatusCalls);
            if (request.Arguments.Contains("fetch"))
            {
                Interlocked.Increment(ref Fetches); LastFetch = request;
                if (_gates.TryGetValue(request.WorkingDirectory, out var gate))
                {
                    gate.Started.TrySetResult();
                    try { await gate.Release.Task.WaitAsync(token); }
                    catch (OperationCanceledException) { gate.Cancelled = true; throw; }
                }
            }
            return await Inner.RunAsync(request, token, output);
        }
        public sealed class FetchGate
        {
            public TaskCompletionSource Started { get; } = new(TaskCreationOptions.RunContinuationsAsynchronously);
            public TaskCompletionSource Release { get; } = new(TaskCreationOptions.RunContinuationsAsynchronously);
            public bool Cancelled;
        }
    }
}
