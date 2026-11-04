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
    private static bool _mergeToolSettingsChecks;
    private static async Task RunMergeToolSettingsChecksAsync()
    {
        var custom = new ExternalMergeToolSettings { Mode = ExternalMergeToolMode.CustomCommand, Command = "cat \"$LOCAL\" > \"$MERGED\"\r\ncat \"$REMOTE\" >> \"$MERGED\"" };
        await Check("Merge-tool preferences preserve old profiles and round-trip without changing other preferences", async () =>
        {
            var store = new SettingsStore(Fixture("merge-tool-profile"));
            await File.WriteAllTextAsync(store.FilePath, "{\"SchemaVersion\":6,\"UiScale\":1.3,\"UpdateChannel\":\"win-beta\"}");
            var loaded = await store.LoadAsync(default);
            Expect(loaded.Settings.ExternalMergeTool.Mode == ExternalMergeToolMode.GitDefault, "Old profiles preserve Git defaults");
            await store.SaveAsync(loaded.Settings with { ExternalMergeTool = custom }, default);
            var restarted = await new SettingsStore(Path.GetDirectoryName(store.FilePath)!).LoadAsync(default);
            Expect(restarted.Settings.ExternalMergeTool == custom && restarted.Settings.UiScale == 1.3 && restarted.Settings.UpdateChannel == "win-beta", "Exact command and unrelated preferences survive restart");
            foreach (var invalid in new[] { new ExternalMergeToolSettings { Mode = (ExternalMergeToolMode)99 }, custom with { Command = "bad\0command" }, custom with { Command = new string('x', 8193) }, new ExternalMergeToolSettings { Mode = ExternalMergeToolMode.NamedTool, ToolName = "--bad" } })
                Expect(invalid.ValidationCode is not null && invalid.Normalize().Mode == ExternalMergeToolMode.GitDefault, "Invalid persisted execution settings revert to Git default without truncating commands");
        });
        await Check("Settings validates merge-tool input and retains failed edits without runtime changes", async () =>
        {
            var writes = 0;
            var shell = new ShellViewModel(persist: _ => { writes++; throw new IOException("fixture"); });
            shell.CompleteStartup(new(new(), SettingsLoadStatus.Loaded));
            var model = new SettingsViewModel(true, 10, true, new(), shell.SaveSettingsAsync);
            model.MergeToolMode = ExternalMergeToolMode.NamedTool; model.MergeToolName = "--bad";
            Expect(!model.SaveCommand.CanExecute(null), "Option-shaped tool cannot save");
            model.MergeToolMode = ExternalMergeToolMode.CustomCommand; model.MergeToolCommand = custom.Command;
            Expect(writes == 0 && model.SaveCommand.CanExecute(null), "Opening/editing has no save or tool preparation");
            model.SaveCommand.Execute(null); await model.SaveTask;
            Expect(writes == 1 && model.Error.Length > 0 && model.MergeToolCommand == custom.Command && shell.ExternalMergeTool.Mode == ExternalMergeToolMode.GitDefault,
                "Failed persistence retains edits while runtime stays unchanged");
            shell.BeginShutdown();
        });
        await Check("Atomic settings save restores merge tool and auto-fetch together", async () =>
        {
            var store = new SettingsStore(Fixture("merge-tool-shell-profile"));
            var shell = new ShellViewModel(persist: value => store.SaveAsync(value, default)); shell.CompleteStartup(await store.LoadAsync(default));
            Expect(await shell.SaveSettingsAsync(false, 90, custom), "Combined settings save succeeds");
            var restored = new ShellViewModel(); restored.CompleteStartup(await store.LoadAsync(default));
            Expect(restored.ExternalMergeTool == custom && !restored.AutoFetchEnabled && restored.AutoFetchIntervalSeconds == 90, "Saved preference restored with auto-fetch");
            await shell.SaveAutoFetchSettingsAsync(true, 30);
            Expect((await store.LoadAsync(default)).Settings.ExternalMergeTool == custom, "Auto-fetch-only compatibility save preserves merge tool");
            shell.BeginShutdown(); restored.BeginShutdown();
        });
        await Check("Merge-tool settings bind and render in Dark and Light without a visible window", async () =>
        {
            foreach (var theme in new[] { AppAppearance.Dark, AppAppearance.Light })
            {
                AppTheme.Apply(Application.Current.Resources, false, theme);
                var model = new SettingsViewModel(true, 10, true, custom, (_, _, _) => throw new InvalidOperationException("Rendering must not save"));
                model.SelectedCategory = 5; var window = new SettingsWindow(model); var content = (FrameworkElement)window.Content;
                try
                {
                    foreach (var size in new[] { (840, 650), (560, 380) })
                    {
                        model.MergeToolMode = ExternalMergeToolMode.CustomCommand;
                        Arrange(content, size.Item1, size.Item2); await Dispatcher.Yield(DispatcherPriority.ContextIdle);
                        Expect(model.IsCustomCommand && model.MergeToolCommand == custom.Command, "Custom command stays bound at both sizes");
                        Expect(((TextBox)window.FindName("MergeToolCommandInput")).Text == custom.Command, "Actual command input shows the saved command");
                        SaveRender(content, $"merge-tool-settings-{theme}-{size.Item1}.png", size.Item1, size.Item2);
                        model.MergeToolMode = ExternalMergeToolMode.NamedTool; model.MergeToolName = "kdiff3";
                        model.MergeToolExecutablePath = @"C:\Tools with spaces\kdiff3.exe";
                        Arrange(content, size.Item1, size.Item2); await Dispatcher.Yield(DispatcherPriority.ContextIdle);
                        Expect(((TextBox)window.FindName("MergeToolNameInput")).Text == "kdiff3" &&
                            ((TextBox)window.FindName("MergeToolExecutableInput")).Text == model.MergeToolExecutablePath,
                            "Named executable controls bind without probing the path");
                        SaveRender(content, $"merge-tool-settings-named-{theme}-{size.Item1}.png", size.Item1, size.Item2);
                    }
                }
                finally { window.Close(); }
            }
            AppTheme.Apply(Application.Current.Resources, false);
        });

        var runner = new HistoryActionRunner(); var reader = new RepositoryReader(runner); var identities = new WindowsRepositoryIdentity();
        var actions = new GitActions(runner, reader, identities); var git = await reader.DiscoverAsync(null, default);
        const string relative = "file 日本語 with spaces.txt";
        async Task<string> Git(string repo, params string[] args)
        {
            var result = await runner.RunAsync(new(git.Executable, repo, args), default);
            Expect(result.ExitCode == 0, Encoding.UTF8.GetString(result.StandardError)); return Encoding.UTF8.GetString(result.StandardOutput).Trim();
        }
        async Task<string> Conflict(string name)
        {
            var repo = Fixture("merge-tool " + name); await Git(repo, "init", "-b", "main");
            await Git(repo, "config", "user.name", "Fixture"); await Git(repo, "config", "user.email", "fixture@example.invalid"); await Git(repo, "config", "core.autocrlf", "false");
            await File.WriteAllTextAsync(Path.Combine(repo, relative), "base\n", new UTF8Encoding(false)); await Git(repo, "add", "."); await Git(repo, "commit", "-m", "base");
            await Git(repo, "switch", "-c", "incoming"); await File.WriteAllTextAsync(Path.Combine(repo, relative), "incoming\n", new UTF8Encoding(false)); await Git(repo, "commit", "-am", "incoming");
            await Git(repo, "switch", "main"); await File.WriteAllTextAsync(Path.Combine(repo, relative), "current\n", new UTF8Encoding(false)); await Git(repo, "commit", "-am", "current");
            var merge = await runner.RunAsync(new(git.Executable, repo, ["merge", "--no-edit", "incoming"]), default); Expect(merge.ExitCode != 0, "Fixture conflict"); return repo;
        }
        async Task<ConflictPreview> Preview(string repo)
        {
            var context = await actions.ReadWorkingCopyAsync(git, repo, default);
            return await actions.ReadConflictAsync(context, context.Changes.Entries.Single(e => e.Group == ChangeGroup.Conflicted), default);
        }
        Task<WorkingCopyResult> Resolve(ConflictPreview p, ExternalMergeToolSettings tool, CancellationToken token = default) => actions.ExecuteWorkingCopyAsync(p.Context,
            new(WorkingCopyAction.ResolveConflict, [p.Change], Resolution: new(p, ConflictChoice.External, ToolSettings: tool)), token);

        await Check("Saved custom tool resolves the selected Unicode file through Git without changing configuration", async () =>
        {
            var repo = await Conflict("custom"); var p = await Preview(repo); var config = await File.ReadAllBytesAsync(Path.Combine(repo, ".git/config"));
            var model = new ConflictViewModel(p.Context, p.Change, actions, custom);
            var view = new ConflictView { DataContext = model }; Arrange(view, 1000, 680);
            await Dispatcher.Yield(DispatcherPriority.ContextIdle); await model.StartAsync();
            await Dispatcher.Yield(DispatcherPriority.ContextIdle);
            try
            {
                Expect(model.ExternalTool == ConflictViewModel.SavedToolOption, "Saved app tool selected initially");
                Expect(VisualDescendants<ComboBox>(view).Single().SelectedValue as string == ConflictViewModel.SavedToolOption, "Bound selector retains saved choice after asynchronous preview publication");
                var calls = runner.Calls; model.ExternalTool = ""; model.ExternalTool = ConflictViewModel.SavedToolOption;
                Expect(runner.Calls == calls && !model.HasMutation, "Choosing tools is inert");
                model.ExternalCommand.Execute(null); await model.Work;
                Expect(model.Resolved && await Git(repo, "show", ":" + relative) == "current\nincoming", "Quoted source variables resolve selected file and stage exact combined content: " + model.Status + " / " + model.Output);
                Expect((await File.ReadAllBytesAsync(Path.Combine(repo, relative))).SequenceEqual(Encoding.UTF8.GetBytes("current\nincoming\n")), "Exact result bytes preserved");
                var updatedConfig = await File.ReadAllBytesAsync(Path.Combine(repo, ".git/config"));
                Expect(config.SequenceEqual(updatedConfig), "Repository config byte-for-byte unchanged");
            }
            finally { await model.CloseAsync(); }
        });
        await Check("Explicit named tool overrides the saved app command", async () =>
        {
            var repo = await Conflict("override"); await Git(repo, "config", "mergetool.explicit.cmd", "cp \"$REMOTE\" \"$MERGED\""); await Git(repo, "config", "mergetool.explicit.trustExitCode", "true");
            var p = await Preview(repo); var model = new ConflictViewModel(p.Context, p.Change, actions, custom with { Command = "exit 1" }); await model.StartAsync();
            try { model.ExternalTool = "explicit"; model.ExternalCommand.Execute(null); await model.Work; Expect(model.Resolved && await Git(repo, "show", ":" + relative) == "incoming", "Per-conflict explicit choice bypasses saved failing command"); }
            finally { await model.CloseAsync(); }
        });
        await Check("Named executable override invokes Git adapter and preserves configured paths", async () =>
        {
            var repo = await Conflict("executable path"); var helper = Fixture("tool helper 日本語 with spaces");
            foreach (var source in Directory.EnumerateFiles(AppContext.BaseDirectory).Where(path => Path.GetExtension(path) is ".dll" or ".json" or ".exe"))
                File.Copy(source, Path.Combine(helper, Path.GetFileName(source)), true);
            var executable = Path.Combine(helper, "GitGui.FoundationChecks.exe");
            await Git(repo, "config", "mergetool.kdiff3.path", "missing-old-tool"); var p = await Preview(repo);
            var prior = Environment.GetEnvironmentVariable("SHOVEL_MERGETOOL_FIXTURE"); Environment.SetEnvironmentVariable("SHOVEL_MERGETOOL_FIXTURE", "1");
            try
            {
                var result = await Resolve(p, new() { Mode = ExternalMergeToolMode.NamedTool, ToolName = "kdiff3", ExecutablePath = executable });
                Expect(result.Result.Success && !result.Result.HasConflicts && await Git(repo, "show", ":" + relative) == "incoming", "Actual helper executable receives adapter arguments and writes staged result");
                Expect(await Git(repo, "config", "mergetool.kdiff3.path") == "missing-old-tool", "Override does not persist into repository");
            }
            finally { Environment.SetEnvironmentVariable("SHOVEL_MERGETOOL_FIXTURE", prior); }
        });
        await Check("Missing executable reports a localized error without changing conflict files or configuration", async () =>
        {
            var repo = await Conflict("missing executable"); var p = await Preview(repo);
            var settings = new ExternalMergeToolSettings { Mode = ExternalMergeToolMode.NamedTool, ToolName = "kdiff3", ExecutablePath = Path.Combine(repo, "missing tool.exe") };
            Expect((settings with { ExecutablePath = "relative/tool.exe" }).ValidationCode == "ConflictToolPathInvalid" &&
                (settings with { ToolName = "--bad" }).ValidationCode == "ConflictToolNameInvalid", "Relative executable paths and option-shaped names fail in-memory validation");
            var beforeIndex = await File.ReadAllBytesAsync(Path.Combine(repo, ".git/index"));
            var beforeFile = await File.ReadAllBytesAsync(Path.Combine(repo, relative));
            var beforeConfig = await File.ReadAllBytesAsync(Path.Combine(repo, ".git/config"));
            try { await Resolve(p, settings); throw new InvalidOperationException("Missing executable accepted"); }
            catch (GitActionException error)
            {
                Expect(error.Code == "ConflictToolExecutableMissing" && GitGui.App.Localization.UiText.Get(error.Code) != error.Code,
                    "Missing executable returns a resource-backed actionable error");
            }
            var afterIndex = await File.ReadAllBytesAsync(Path.Combine(repo, ".git/index"));
            var afterFile = await File.ReadAllBytesAsync(Path.Combine(repo, relative));
            var afterConfig = await File.ReadAllBytesAsync(Path.Combine(repo, ".git/config"));
            Expect(beforeIndex.SequenceEqual(afterIndex) && beforeFile.SequenceEqual(afterFile) && beforeConfig.SequenceEqual(afterConfig),
                "Failed executable validation preserves index, conflicted worktree bytes and Git configuration exactly");
        });
        await Check("Failed custom tool and stale file retain unresolved state", async () =>
        {
            var repo = await Conflict("failure"); var p = await Preview(repo);
            var result = await Resolve(p, custom with { Command = "printf 'tool failed\\n' >&2; exit 7" });
            Expect(!result.Result.Success && result.Result.Output.Contains("tool failed") && (await Git(repo, "ls-files", "-u")).Length > 0, "Nonzero exit keeps conflict and output");
            p = await Preview(repo); await File.WriteAllTextAsync(Path.Combine(repo, relative), "outside edit\n");
            try { await Resolve(p, custom); throw new InvalidOperationException("Stale file accepted"); }
            catch (GitActionException error) { Expect(error.Code == "ConflictEditedOutside", error.Code); }
            Expect(await File.ReadAllTextAsync(Path.Combine(repo, relative)) == "outside edit\n" && (await Git(repo, "ls-files", "-u")).Length > 0, "Outside edit and unresolved index preserved");
        });
        await Check("Cancelling a running custom merge command drains owned process and retains conflict", async () =>
        {
            var repo = await Conflict("cancel"); var p = await Preview(repo); using var cancellation = new CancellationTokenSource();
            var task = Resolve(p, custom with { Command = "printf ready > tool-ready; sleep 20; cp \"$REMOTE\" \"$MERGED\"" }, cancellation.Token);
            try
            {
                using var deadline = new CancellationTokenSource(TimeSpan.FromSeconds(8));
                while (!File.Exists(Path.Combine(repo, "tool-ready"))) { deadline.Token.ThrowIfCancellationRequested(); await Task.Delay(25, deadline.Token); }
                cancellation.Cancel(); var result = await task.WaitAsync(TimeSpan.FromSeconds(5));
                Expect(result.Result.Outcome == ProcessOutcome.Cancelled && (await Git(repo, "ls-files", "-u")).Length > 0, "Cancelled process reports cancellation without staging");
            }
            finally { cancellation.Cancel(); try { await task; } catch (OperationCanceledException) { } }
        });
    }
}
