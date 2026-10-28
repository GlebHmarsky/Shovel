using System.Diagnostics;
using System.IO;
using System.Text;
using System.Windows;
using System.Windows.Controls;
using GitGui.App.Localization;
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
    private static async Task RunInitializationChecksAsync()
    {
        // Init rejects nested repositories. Keep these disposable destinations outside the source checkout.
        var root = Directory.CreateTempSubdirectory("shovel-init-checks-").FullName;
        await File.WriteAllTextAsync(Path.Combine(_root, "initialization-fixtures.txt"), root);
        var runner = new InitializationRunner(new WindowsProcessRunner()); var reader = new RepositoryReader(runner);
        var identities = new WindowsRepositoryIdentity(); var actions = new GitActions(runner, reader, identities);
        var git = await reader.DiscoverAsync(null, default);
        string Folder(string name) => Path.Combine(root, name);
        async Task<string> Git(string path, params string[] arguments)
        {
            var result = await runner.RunAsync(new(git.Executable, path, arguments), default);
            Expect(result.Outcome == ProcessOutcome.Exited && result.ExitCode == 0, "Fixture Git: " + Encoding.UTF8.GetString(result.StandardError));
            return Encoding.UTF8.GetString(result.StandardOutput).Trim();
        }
        async Task Reject(string code, string path, string branch = "")
        {
            var before = runner.InitCalls;
            try { await actions.InitializeRepositoryAsync(git, new(path, branch), default); throw new InvalidOperationException("Expected " + code); }
            catch (GitActionException error) { Expect(error.Code == code, "Expected " + code + "; got " + error.Code); }
            Expect(runner.InitCalls == before, "Rejected before init");
        }
        ShellViewModel Shell(InitializationInteraction ui)
        {
            var shell = new ShellViewModel(reader, ui, identities, actions: actions);
            shell.CompleteStartup(new(new(), SettingsLoadStatus.Defaults)); return shell;
        }

        await Check("New repository honors default branch and templates without touching source files or staging", async () =>
        {
            var config = Environment.GetEnvironmentVariable("GIT_CONFIG_GLOBAL")!;
            var previous = await File.ReadAllTextAsync(config);
            try
            {
                var templates = Directory.CreateDirectory(Folder("templates")).FullName;
                await File.WriteAllTextAsync(Path.Combine(templates, "fixture-marker"), "copied template");
                await Git(root, "config", "--file", config, "init.defaultBranch", "team-default");
                await Git(root, "config", "--file", config, "init.templateDir", templates);
                var target = Directory.CreateDirectory(Folder("populated 日本語 - folder")).FullName;
                var bytes = new byte[] { 0, 255, 1, 2 }; await File.WriteAllBytesAsync(Path.Combine(target, "keep.bin"), bytes);
                var result = await actions.InitializeRepositoryAsync(git, new(target), default);
                Expect(result.Action.Success && result.Repository is { HeadKind: HeadKind.Unborn, Branch: "team-default" }, "Actual configured unborn branch: " + result.Action + "; " + result.Repository);
                Expect((await File.ReadAllBytesAsync(Path.Combine(target, "keep.bin"))).SequenceEqual(bytes), "Existing binary preserved");
                Expect(await Git(target, "ls-files") == "" && await Git(target, "remote") == "", "No implicit staging, commit or remote");
                Expect(File.Exists(Path.Combine(target, ".git", "fixture-marker")), "Configured template respected");
                var context = await actions.ReadWorkingCopyAsync(git, target, default);
                Expect(context.Changes.Entries.Count == 1 && context.Changes.Entries[0].Group == ChangeGroup.Untracked, "Real untracked status available");
            }
            finally { await File.WriteAllTextAsync(config, previous); }
        });
        await Check("Explicit branch creates missing folders and existing repository opens without reinitializing", async () =>
        {
            var target = Folder("-new 日本語/child");
            var result = await actions.InitializeRepositoryAsync(git, new(target, "work/start"), default);
            Expect(result.Action.Success && result.Repository?.Branch == "work/start", "Missing destination created with explicit branch: " + result.Action + "; " + result.Repository);
            await Git(target, "config", "user.name", "Init Fixture"); await Git(target, "config", "user.email", "init@example.invalid");
            await File.WriteAllTextAsync(Path.Combine(target, "a.txt"), "first\n");
            var context = await actions.ReadWorkingCopyAsync(git, target, default);
            var stage = await actions.ExecuteWorkingCopyAsync(context, new(WorkingCopyAction.Stage, context.Changes.Entries), default);
            Expect(stage.Result.Success, "New repository accepts real Stage");
            await Git(target, "commit", "-m", "first");
            var head = await Git(target, "rev-parse", "HEAD"); var config = await File.ReadAllTextAsync(Path.Combine(target, ".git", "config"));
            var calls = runner.InitCalls;
            result = await actions.InitializeRepositoryAsync(git, new(target, "ignored"), default);
            Expect(result.Action.MessageCode == "InitAlreadyExists" && runner.InitCalls == calls, "Open instead of reinit");
            Expect(await Git(target, "rev-parse", "HEAD") == head && await File.ReadAllTextAsync(Path.Combine(target, ".git", "config")) == config && result.Repository?.Branch == "work/start", "HEAD, branch and config unchanged");
            var linked = Folder("linked-worktree"); await Git(target, "worktree", "add", "-b", "linked", linked);
            result = await actions.InitializeRepositoryAsync(git, new(linked), default);
            Expect(result.Action.MessageCode == "InitAlreadyExists" && result.Repository?.Branch == "linked", "Existing linked worktree opened");
        });
        await Check("Initialization rejects invalid names, nested roots, bare repositories and metadata without writes", async () =>
        {
            await Reject("InitInvalidDirectory", "relative");
            var file = Folder("file"); await File.WriteAllTextAsync(file, "keep"); await Reject("InitInvalidDirectory", file);
            var bad = Folder("invalid branch"); await Reject("InitInvalidBranch", bad, "bad..branch");
            Expect(!Directory.Exists(bad), "Invalid branch creates no folder");
            await Reject("InitInvalidBranch", bad, "--help");
            await Reject("InitNestedRepository", Folder("-new 日本語/child/nested"));
            await Reject("InitMetadataDirectory", Folder("-new 日本語/child/.git/child"));
            var bare = Folder("bare.git"); await Git(root, "init", "--bare", bare); await Reject("InitBareRepository", bare);
            var broken = Directory.CreateDirectory(Folder("broken")).FullName; await File.WriteAllTextAsync(Path.Combine(broken, ".git"), "not valid git metadata");
            var count = runner.InitCalls;
            await ExpectException<GitReadException>(() => actions.InitializeRepositoryAsync(git, new(broken), default));
            Expect(count == runner.InitCalls && await File.ReadAllTextAsync(Path.Combine(broken, ".git")) == "not valid git metadata", "Broken metadata preserved");
        });
        await Check("Missing executable and inherited Git redirection cannot initialize an unintended target", async () =>
        {
            var target = Folder("missing executable");
            await ExpectException<GitReadException>(() => actions.InitializeRepositoryAsync(new(Folder("missing-git.exe"), "2.51.1"), new(target), default));
            Expect(!Directory.Exists(target), "Start failure creates no destination");
            var redirect = Folder("-new 日本語/child/.git"); var original = await File.ReadAllBytesAsync(Path.Combine(redirect, "HEAD"));
            var prior = Environment.GetEnvironmentVariable("GIT_DIR");
            Environment.SetEnvironmentVariable("GIT_DIR", redirect);
            try
            {
                var result = await actions.InitializeRepositoryAsync(git, new(Folder("redirect excluded"), "independent"), default);
                Expect(result.Action.Success && result.Repository?.Branch == "independent" && (await File.ReadAllBytesAsync(Path.Combine(redirect, "HEAD"))).SequenceEqual(original), "Redirect scrubbed; existing repository not altered");
            }
            finally { Environment.SetEnvironmentVariable("GIT_DIR", prior); }
        });
        await Check("Initialization failure preserves partial output and cancellation reinspects a usable result", async () =>
        {
            var target = Folder("partial failure");
            runner.InitOverride = (request, _, _) =>
            {
                Directory.CreateDirectory(target); File.WriteAllText(Path.Combine(target, "partial"), "keep");
                return Task.FromResult(new ProcessResult(ProcessOutcome.Exited, 1, [], Encoding.UTF8.GetBytes("fixture failure"), TimeSpan.Zero));
            };
            try
            {
                var result = await actions.InitializeRepositoryAsync(git, new(target), default);
                Expect(!result.Action.Success && result.Repository is null && result.Action.Output.Contains("fixture failure", StringComparison.Ordinal) && File.Exists(Path.Combine(target, "partial")), "Partial files and failure output retained");
            }
            finally { runner.InitOverride = null; }
            runner.AfterInit = result => result with { Outcome = ProcessOutcome.Cancelled };
            try
            {
                var result = await actions.InitializeRepositoryAsync(git, new(Folder("cancelled usable"), "main"), default);
                Expect(result.Action.Outcome == ProcessOutcome.Cancelled && result.Repository?.HeadKind == HeadKind.Unborn, "Cancelled command is not rollback; usable result observed");
            }
            finally { runner.AfterInit = null; }
        });
        await Check("Same-destination initialization is bounded and cancellation releases the operation gate", async () =>
        {
            var entered = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
            runner.InitOverride = async (_, token, _) =>
            {
                entered.SetResult(); try { await Task.Delay(Timeout.Infinite, token); } catch (OperationCanceledException) { }
                return new(ProcessOutcome.Cancelled, null, [], [], TimeSpan.Zero);
            };
            var target = Folder("gated"); using var cancel = new CancellationTokenSource();
            var pending = actions.InitializeRepositoryAsync(git, new(target), cancel.Token);
            try { await entered.Task.WaitAsync(TimeSpan.FromSeconds(10)); await Reject("ActionOtherInstance", target); }
            finally { cancel.Cancel(); await pending; runner.InitOverride = null; }
            Expect((await actions.InitializeRepositoryAsync(git, new(target, "main"), default)).Action.Success, "Gate released after cancellation");
        });
        await Check("New repository options and cancelled modal perform zero Git calls with localized fixed layout", async () =>
        {
            var ui = new InitializationInteraction(); var shell = Shell(ui);
            var count = runner.Calls; ui.DuringOptions = () => Expect(runner.Calls == count && shell.ActionOpen, "No preparation while options open");
            shell.NewRepositoryCommand.Execute(null); await shell.ActionTask;
            Expect(runner.Calls == count && !shell.ActionOpen && shell.Repositories.Count == 0, "Cancel changes no repository or process state");
            var model = new RepositoryInitViewModel(ui); var window = new RepositoryInitWindow(model);
            var watch = Stopwatch.StartNew(); model.Directory = Folder("typed destination"); model.InitialBranch = "main";
            Arrange((FrameworkElement)window.Content, 604, 371); watch.Stop();
            SaveRender((FrameworkElement)window.Content, "init-options.png", 604, 371);
            Expect(window.ResizeMode == ResizeMode.NoResize && runner.Calls == count && model.CreateCommand.CanExecute(null), "Close-only dialog with no Git/filesystem preparation");
            Console.WriteLine($"Init options offscreen layout/edit: {watch.Elapsed.TotalMilliseconds:F0} ms; Git queries: {runner.Calls - count}");
            window.Close(); shell.BeginShutdown(); await shell.WaitForIdleAsync();
        });
        await Check("Bound New repository creates, selects, loads empty history and restores the saved tab", async () =>
        {
            var target = Directory.CreateDirectory(Folder("shell new")).FullName; await File.WriteAllTextAsync(Path.Combine(target, "new.txt"), "new\n");
            var ui = new InitializationInteraction { Options = new(target, "main") }; var shell = Shell(ui);
            ui.DuringOptions = () => Expect(runner.InitCalls > 0 && shell.Repositories.Count == 0 && !shell.IsBusy, "No tab or inspection before submit");
            var watch = Stopwatch.StartNew(); shell.NewRepositoryCommand.Execute(null); await shell.ActionTask; await shell.WaitForIdleAsync();
            Console.WriteLine($"Bound init through automatic empty history/status: {watch.Elapsed.TotalMilliseconds:F0} ms");
            Expect(shell.SelectedRepository?.Snapshot is { HeadKind: HeadKind.Unborn, Branch: "main" } && shell.Commits.Count == 0 && shell.CurrentAction?.Succeeded == true && !shell.ActionOpen, "Created and automatically opened");
            Expect(shell.CommitButtonLabel == UiText.Format("CommitOpen", 1), "Untracked count loaded after empty history");
            var window = new MainWindow(shell); Arrange((FrameworkElement)window.Content, 1200, 760); SaveRender((FrameworkElement)window.Content, "init-result.png", 1200, 760);
            Arrange((FrameworkElement)window.Content, 900, 580); SaveRender((FrameworkElement)window.Content, "init-result-minimum.png", 900, 580); window.Close();
            var saved = shell.CaptureSession(new()); shell.BeginShutdown(); await shell.WaitForIdleAsync();
            var restored = Shell(new()); restored.CompleteStartup(new(saved, SettingsLoadStatus.Loaded)); await restored.WaitForIdleAsync();
            Expect(restored.SelectedRepository?.Snapshot?.Branch == "main", "Saved repository restored automatically");
            restored.BeginShutdown(); await restored.WaitForIdleAsync();
        });
        await Check("Bound init failure retains previous tab and output; active init prevents navigation", async () =>
        {
            var ui = new InitializationInteraction { Options = new(Folder("shell owner"), "main") }; var shell = Shell(ui);
            shell.NewRepositoryCommand.Execute(null); await shell.ActionTask; await shell.WaitForIdleAsync(); var owner = shell.SelectedRepository;
            ui.Options = new(Folder("shell owner/nested")); shell.NewRepositoryCommand.Execute(null); await shell.ActionTask; await shell.WaitForIdleAsync();
            Expect(shell.SelectedRepository == owner && shell.Repositories.Count == 1 && shell.CurrentAction?.Status == UiText.Get("InitNestedRepository"), "Failed init keeps prior tab and understandable result");
            var entered = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
            runner.InitOverride = async (_, token, _) => { entered.SetResult(); try { await Task.Delay(Timeout.Infinite, token); } catch (OperationCanceledException) { } return new(ProcessOutcome.Cancelled, null, [], [], TimeSpan.Zero); };
            try
            {
                ui.Options = new(Folder("shell cancelled")); shell.NewRepositoryCommand.Execute(null); await entered.Task.WaitAsync(TimeSpan.FromSeconds(10));
                Expect(shell.ActionOpen && shell.CurrentAction?.CanCancel == true && !shell.NewRepositoryCommand.CanExecute(null) && !shell.CloseRepositoryCommand.CanExecute(null), "Single action owns cancel and navigation");
                shell.SelectedRepository = null; Expect(shell.SelectedRepository == owner, "Selection cannot change during creation");
                shell.CancelCommand.Execute(null); await shell.ActionTask; await shell.WaitForIdleAsync();
                Expect(!shell.ActionOpen && shell.SelectedRepository == owner && shell.CurrentAction?.Succeeded == false, "Cancellation drains before navigation resumes");
            }
            finally { runner.InitOverride = null; shell.BeginShutdown(); await shell.WaitForIdleAsync(); }
        });
    }
    private sealed class InitializationInteraction : IUserInteraction
    {
        public RepositoryInitOptions? Options; public Action? DuringOptions;
        public RepositoryInitOptions? ChooseRepositoryInitialization() { DuringOptions?.Invoke(); return Options; }
        public string? ChooseRepository() => null;
        public string? ChooseGit() => null;
    }
    private sealed class InitializationRunner(IProcessRunner inner) : IProcessRunner
    {
        public int Calls, InitCalls;
        public Func<ProcessRequest, CancellationToken, Action<ProcessOutput>?, Task<ProcessResult>>? InitOverride;
        public Func<ProcessResult, ProcessResult>? AfterInit;
        public async Task<ProcessResult> RunAsync(ProcessRequest request, CancellationToken token, Action<ProcessOutput>? output = null)
        {
            Interlocked.Increment(ref Calls);
            var init = request.Arguments.Contains("init"); if (init) Interlocked.Increment(ref InitCalls);
            if (init && InitOverride is not null) return await InitOverride(request, token, output);
            var result = await inner.RunAsync(request, token, output);
            return init && AfterInit is not null ? AfterInit(result) : result;
        }
    }
}
