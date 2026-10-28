using System.Diagnostics;
using System.IO;
using System.Text;
using System.Windows;
using System.Windows.Automation;
using System.Windows.Controls;
using System.Windows.Threading;
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
    private static async Task RunCommitWorkspaceChecksAsync()
    {
        var runner = new WindowsProcessRunner(); var reader = new RepositoryReader(runner);
        var identities = new WindowsRepositoryIdentity(); var actions = new GitActions(runner, reader, identities);
        var git = await reader.DiscoverAsync(null, default); var repo = Fixture("commit 日本語 spaces");
        async Task<string> Git(string directory, params string[] args)
        {
            var result = await runner.RunAsync(new(git.Executable, directory, args), default);
            Expect(result.ExitCode == 0, "Fixture Git: " + string.Join(' ', args) + " " + Encoding.UTF8.GetString(result.StandardError));
            return Encoding.UTF8.GetString(result.StandardOutput).Trim();
        }
        Task<WorkingCopyContext> Read() => actions.ReadWorkingCopyAsync(git, repo, default);
        async Task Act(WorkingCopyContext context, WorkingCopyOptions options)
        {
            var result = await actions.ExecuteWorkingCopyAsync(context, options, default);
            Expect(result.Result.Success, "Working-copy action: " + result.Result.Output);
        }
        await Git(repo, "init", "-b", "main"); await Git(repo, "config", "user.name", "Commit Fixture");
        await Git(repo, "config", "user.email", "commit@example.invalid"); await Git(repo, "config", "core.autocrlf", "false");
        const string literal = "-literal [日本語].txt";
        await Check("Unborn stage/unstage uses literal Unicode paths and preserves partially staged worktree bytes", async () =>
        {
            await File.WriteAllTextAsync(Path.Combine(repo, literal), "original\n");
            await File.WriteAllTextAsync(Path.Combine(repo, "other.txt"), "other\n");
            var context = await Read();
            await Act(context, new(WorkingCopyAction.Stage, context.Changes.Entries.Where(row => row.Path == literal).ToArray()));
            await File.WriteAllTextAsync(Path.Combine(repo, literal), "latest\n"); context = await Read();
            Expect(context.Changes.Entries.Count(row => row.Path == literal) == 2, "Index and worktree remain distinct");
            await Act(context, new(WorkingCopyAction.Unstage, context.Changes.Entries.Where(row => row.Group == ChangeGroup.Staged).ToArray()));
            Expect(await File.ReadAllTextAsync(Path.Combine(repo, literal)) == "latest\n" && (await Read()).Changes.Entries.All(row => row.Group == ChangeGroup.Untracked), "Unborn unstage only removes index entries");
            context = await Read(); await Act(context, new(WorkingCopyAction.Stage, context.Changes.Entries));
            context = await Read(); var result = await actions.ExecuteWorkingCopyAsync(context, new(WorkingCopyAction.Commit, [], "Initial 日本語\n\nFull body"), default);
            Expect(result.Result.Success && result.CommitHash?.Length == 40, "Real initial commit with hash");
            Expect((await Git(repo, "log", "-1", "--format=%B")).Contains("Full body"), "Full UTF-8 message committed");
        });
        await Check("Selected diffs distinguish HEAD/index/worktree, line numbers, untracked text and binary byte sizes", async () =>
        {
            await File.WriteAllTextAsync(Path.Combine(repo, literal), "staged\n"); await Git(repo, "add", "--", literal);
            await File.WriteAllTextAsync(Path.Combine(repo, literal), "worktree\n");
            await File.WriteAllTextAsync(Path.Combine(repo, "new.txt"), "new line\n");
            await File.WriteAllBytesAsync(Path.Combine(repo, "asset.bin"), [0, 255, 1, 0]);
            var context = await Read();
            async Task<FileDiff> Diff(string name, ChangeGroup group) => await reader.ReadFileDiffAsync(context,
                context.Changes.Entries.Single(row => row.Path == name && row.Group == group), new(), default);
            var index = await Diff(literal, ChangeGroup.Staged); var worktree = await Diff(literal, ChangeGroup.Unstaged);
            Expect(index.Lines.Any(row => row.Kind == DiffLineKind.Added && row.Text == "staged" && row.NewLine == 1) &&
                index.Lines.Any(row => row.Kind == DiffLineKind.Removed && row.Text == "latest" && row.OldLine == 1), "HEAD to index numbers/text");
            Expect(worktree.Lines.Any(row => row.Kind == DiffLineKind.Added && row.Text == "worktree") && worktree.Lines.Any(row => row.Kind == DiffLineKind.Removed && row.Text == "staged"), "Index to worktree, not HEAD");
            var added = await Diff("new.txt", ChangeGroup.Untracked); var binary = await Diff("asset.bin", ChangeGroup.Untracked);
            Expect(added.Added == 1 && added.OldBytes == 0 && added.NewBytes == 9 && added.Lines.Any(row => row.Kind == DiffLineKind.Added), "Untracked no-index diff");
            Expect(binary.Kind == FileDiffKind.Binary && binary.OldBytes == 0 && binary.NewBytes == 4 && binary.Lines.Count == 0, "Binary summary without decoded garbage");
            await Act(context, new(WorkingCopyAction.Stage, context.Changes.Entries.Where(row => row.Group != ChangeGroup.Staged).ToArray()));
            await Act(await Read(), new(WorkingCopyAction.Commit, [], "Text and binary"));
            await File.WriteAllBytesAsync(Path.Combine(repo, "asset.bin"), [0, 255, 1, 0, 2, 3]); context = await Read();
            binary = await Diff("asset.bin", ChangeGroup.Unstaged);
            Expect(binary.OldBytes == 4 && binary.NewBytes == 6, "Tracked binary size delta");
        });
        await Check("Rename staging, deletion, whitespace filtering and large diff limits are explicit", async () =>
        {
            await Git(repo, "mv", "other.txt", "renamed.txt"); await File.WriteAllTextAsync(Path.Combine(repo, "renamed.txt"), "other revised\n");
            var context = await Read();
            await Act(context, new(WorkingCopyAction.Stage, context.Changes.Entries.Where(row => row.Path == "renamed.txt" && row.Group == ChangeGroup.Unstaged).ToArray()));
            context = await Read(); await Act(context, new(WorkingCopyAction.Unstage, context.Changes.Entries.Where(row => row.Path == "renamed.txt" && row.Group == ChangeGroup.Staged).ToArray()));
            Expect(File.Exists(Path.Combine(repo, "renamed.txt")), "Unstage renamed path preserves working file");
            File.Delete(Path.Combine(repo, "new.txt")); context = await Read();
            var deleted = await reader.ReadFileDiffAsync(context, context.Changes.Entries.Single(row => row.Path == "new.txt"), new(), default);
            Expect(deleted.Removed == 1 && deleted.NewBytes == 0, "Deleted diff and size");
            await File.WriteAllTextAsync(Path.Combine(repo, literal), "worktree   \r\n"); context = await Read();
            var row = context.Changes.Entries.Single(entry => entry.Path == literal);
            var whitespace = await reader.ReadFileDiffAsync(context, row, new(true), default);
            Expect(whitespace.Added == 0 && whitespace.Removed == 0, "Optional whitespace comparison");
            await File.WriteAllTextAsync(Path.Combine(repo, "long.txt"), new string('a', RepositoryReader.MaximumDiffLineLength + 1)); context = await Read();
            Expect((await reader.ReadFileDiffAsync(context, context.Changes.Entries.Single(entry => entry.Path == "long.txt"), new(), default)).Kind == FileDiffKind.TooLarge, "Long line does not freeze UI");
            File.Delete(Path.Combine(repo, "long.txt"));
        });
        await Check("Read-only diffs suppress configured filters/external diff while explicit stage honors clean filters", async () =>
        {
            await File.WriteAllTextAsync(Path.Combine(repo, ".gitattributes"), "*.txt filter=fixture\n");
            await Git(repo, "config", "filter.fixture.clean", "echo ran > filter-ran; cat");
            await Git(repo, "config", "diff.external", "echo ran > diff-ran");
            var context = await Read();
            await reader.ReadFileDiffAsync(context, context.Changes.Entries.First(row => row.Path == literal), new(), default);
            Expect(!File.Exists(Path.Combine(repo, "filter-ran")) && !File.Exists(Path.Combine(repo, "diff-ran")), "Automatic read executes neither helper");
            await Act(context, new(WorkingCopyAction.Stage, context.Changes.Entries.Where(row => row.Path == literal).ToArray()));
            Expect(File.Exists(Path.Combine(repo, "filter-ran")), "Explicit stage preserves configured clean behavior");
            await Git(repo, "config", "--remove-section", "filter.fixture"); await Git(repo, "config", "--unset", "diff.external");
            File.Delete(Path.Combine(repo, "filter-ran")); File.Delete(Path.Combine(repo, ".gitattributes"));
        });
        await Check("Stale index and existing locks reject actions without deleting user files or locks", async () =>
        {
            var context = await Read(); await Git(repo, "add", "--", "asset.bin");
            await ExpectException<GitActionException>(() => actions.ExecuteWorkingCopyAsync(context, new(WorkingCopyAction.Unstage, context.Changes.Entries.Where(row => row.Group == ChangeGroup.Staged).ToArray()), default));
            context = await Read(); var lockPath = Path.Combine(repo, ".git", "index.lock"); await File.WriteAllTextAsync(lockPath, "fixture");
            try
            {
                var result = await actions.ExecuteWorkingCopyAsync(context, new(WorkingCopyAction.Stage, context.Changes.Entries.Where(row => row.Group != ChangeGroup.Staged).ToArray()), default);
                Expect(!result.Result.Success && File.Exists(lockPath), "Git lock failure retained, never auto-deleted");
            }
            finally { File.Delete(lockPath); }
        });
        var interaction = new CommitInteraction(); var state = new CommitWorkspaceState();
        var model = new CommitViewModel(repo, git, reader, actions, interaction, state);
        await model.StartLoading(); await model.DiffTask;
        await Check("Shift reuses three action buttons and file/all actions execute real index changes", async () =>
        {
            model.ShiftHeld = true;
            Expect(model.StageLabel == UiText.Get("CommitStageAll") && model.UnstageLabel == UiText.Get("CommitUnstageAll") && model.CommitLabel == UiText.Get("CommitAndPush"), "Compact modifier labels");
            model.StageCommand.Execute(null); await model.OperationTask; await model.DiffTask;
            Expect(model.Unstaged.Count == 0 && model.Staged.Count >= 2 && model.TimingSummary.Length > 0, "Stage all completed with phase timings");
            model.UnstageCommand.Execute(null); await model.OperationTask; await model.DiffTask;
            Expect(model.Staged.Count == 0 && model.Unstaged.Count >= 2, "Unstage all completed");
            model.StageCommand.Execute(null); await model.OperationTask; await model.DiffTask; model.Subject = "UI commit";
            Expect(!model.CommitCommand.CanExecute(null), "Shift commit needs known upstream");
            model.ShiftHeld = false; Expect(model.CommitCommand.CanExecute(null), "Ordinary commit remains available");
        });
        await Check("Failed hooks retain staged files/draft; amend confirms actual hash and changes the existing commit", async () =>
        {
            var hook = Path.Combine(repo, ".git", "hooks", "pre-commit"); await File.WriteAllTextAsync(hook, "#!/bin/sh\necho fixture-hook-rejected >&2\nexit 1\n", new UTF8Encoding(false));
            var prior = await Git(repo, "rev-parse", "HEAD"); model.CommitCommand.Execute(null); await model.OperationTask; await model.DiffTask;
            Expect(model.Subject == "UI commit" && model.Staged.Count > 0 && model.RawOutput.Contains("fixture-hook-rejected") && await Git(repo, "rev-parse", "HEAD") == prior, "Failed hook preserved intent/index");
            File.Delete(hook); model.CommitCommand.Execute(null); await model.OperationTask; await model.DiffTask;
            Expect(model.Subject.Length == 0 && model.Staged.Count == 0, "Successful commit clears draft only after success");
            prior = await Git(repo, "rev-parse", "HEAD"); var count = await Git(repo, "rev-list", "--count", "HEAD");
            model.Amend = true; Expect(model.Subject == "UI commit", "Amend preloads last message when draft empty"); model.Subject = "Amended 日本語";
            interaction.AllowAmend = false; model.CommitCommand.Execute(null); Expect(interaction.AmendHash == prior && !model.IsMutating, "Actual-target confirmation may cancel");
            interaction.AllowAmend = true; model.CommitCommand.Execute(null); await model.OperationTask; await model.DiffTask;
            Expect(await Git(repo, "rev-list", "--count", "HEAD") == count && await Git(repo, "rev-parse", "HEAD") != prior, "Amend replaces tip without another parent");
        });
        await Check("Commit and Push uses configured upstream; rejected push keeps the new commit without retry", async () =>
        {
            var remote = Fixture("commit local bare"); await Git(remote, "init", "--bare"); await Git(repo, "remote", "add", "origin", remote);
            await Git(repo, "push", "-u", "origin", "main");
            await File.WriteAllTextAsync(Path.Combine(repo, literal), "push one\n"); await model.StartLoading(); await model.DiffTask;
            model.ShiftHeld = true; model.StageCommand.Execute(null); await model.OperationTask; await model.DiffTask;
            model.Subject = "Commit and push"; model.CommitCommand.Execute(null); await model.OperationTask; await model.DiffTask;
            var pushed = await Git(repo, "rev-parse", "HEAD");
            Expect(await Git(remote, "rev-parse", "refs/heads/main") == pushed && model.Status == UiText.Format("CommitCreatedPushed", pushed), "Real local transport push");
            var hook = Path.Combine(remote, "hooks", "pre-receive"); await File.WriteAllTextAsync(hook, "#!/bin/sh\necho fixture-push-rejected >&2\nexit 1\n", new UTF8Encoding(false));
            await File.WriteAllTextAsync(Path.Combine(repo, literal), "push two\n"); await model.StartLoading(); await model.DiffTask;
            model.StageCommand.Execute(null); await model.OperationTask; await model.DiffTask; model.Subject = "Commit retained";
            model.CommitCommand.Execute(null); await model.OperationTask; await model.DiffTask;
            var retained = await Git(repo, "rev-parse", "HEAD");
            Expect(retained != pushed && await Git(remote, "rev-parse", "refs/heads/main") == pushed && model.Subject.Length == 0 && model.Status.Contains(retained) && model.RawOutput.Contains("fixture-push-rejected"), "Commit succeeds independently of push; no forced retry");
        });
        await model.CloseAsync();
        await Check("Modal construction uses no Git; delayed/failed/cancelled refresh retains cached rows and drafts", async () =>
        {
            await File.WriteAllTextAsync(Path.Combine(repo, literal), "preview change\n");
            var transport = new StatusGateRunner(runner); var observed = new RepositoryReader(transport); var observedActions = new GitActions(transport, observed, identities);
            var cached = new CommitWorkspaceState { Subject = "Draft preserved" };
            var first = new CommitViewModel(repo, git, observed, observedActions, interaction, cached); await first.StartLoading(); await first.DiffTask; await first.CloseAsync();
            var calls = transport.StatusCalls; var watch = Stopwatch.StartNew();
            var viewModel = new CommitViewModel(repo, git, observed, observedActions, interaction, cached); var window = new CommitWindow(viewModel);
            var content = (FrameworkElement)window.Content; Arrange(content, 1200, 760); var open = watch.Elapsed.TotalMilliseconds;
            Expect(transport.StatusCalls == calls && viewModel.Unstaged.Count > 0 && viewModel.Subject == "Draft preserved", "Synchronous construction binds cache without subprocesses");
            transport.ResetGate(); _ = viewModel.StartLoading(); await transport.Started.Task.WaitAsync(TimeSpan.FromSeconds(10));
            SaveRender(content, "commit-loading.png", 1200, 760); var rows = viewModel.Unstaged;
            transport.Release.TrySetResult(); await viewModel.LoadingTask; await viewModel.DiffTask;
            Arrange(content, 1200, 760); SaveRender(content, "commit-loaded.png", 1200, 760);
            Arrange(content, 900, 580); SaveRender(content, "commit-minimum.png", 900, 580);
            transport.Hold = false; transport.Fail = true; await viewModel.StartLoading();
            Expect(viewModel.Unstaged.Count == rows.Count && !viewModel.Ready && viewModel.RawOutput.Contains("fixture failure"), "Failed refresh retains rows but disables stale actions");
            transport.Fail = false; transport.ResetGate(); _ = viewModel.StartLoading(); await transport.Started.Task; viewModel.Cancel(); await viewModel.LoadingTask;
            Expect(viewModel.Unstaged.Count == rows.Count && !viewModel.Ready && viewModel.Subject == "Draft preserved", "Cancelled read keeps session draft and stale rows");
            transport.Release.TrySetResult(); transport.Hold = false;
            File.WriteAllText(Path.Combine(_root, "commit-timings.txt"), $"Cached modal construct + offscreen layout: {open:F2} ms. Headless, scan excluded.\n");
            await viewModel.CloseAsync(); window.Close();
        });
        await Check("Late diff results are discarded after new selection; large file/diff lists are virtualized", async () =>
        {
            await Task.Run(() => { for (var i = 0; i < 1500; i++) File.WriteAllText(Path.Combine(repo, $"bulk-{i:D4}.txt"), "new\n"); });
            await File.WriteAllTextAsync(Path.Combine(repo, "large-text.txt"), string.Concat(Enumerable.Range(1, 3000).Select(i => $"line {i}\n")));
            var gate = new DiffGateRunner(runner); var observed = new RepositoryReader(gate); var observedActions = new GitActions(gate, observed, identities);
            var viewModel = new CommitViewModel(repo, git, observed, observedActions, interaction); await viewModel.StartLoading(); await viewModel.DiffTask;
            var old = viewModel.Unstaged.First(row => row.Change.Path == literal); var next = viewModel.Unstaged.First(row => row.Change.Path == "large-text.txt");
            viewModel.Select(false, [next]); await viewModel.DiffTask;
            gate.Hold = true; viewModel.Select(false, [old]); await gate.Started.Task.WaitAsync(TimeSpan.FromSeconds(10));
            viewModel.Select(false, [next]); gate.Hold = false; gate.Release.TrySetResult(); await viewModel.DiffTask;
            Expect(viewModel.DiffTitle == next.Path && viewModel.DiffRows.Any(row => row.Text == "line 3000") && !viewModel.DiffRows.Any(row => row.Text == "preview change"), "Old completion never binds in newer selection");
            var window = new CommitWindow(viewModel); var content = (FrameworkElement)window.Content; Arrange(content, 1200, 760); await Dispatcher.Yield(DispatcherPriority.ContextIdle);
            var tables = VisualDescendants<DataGrid>(content).ToDictionary(AutomationProperties.GetAutomationId);
            Expect(tables["UnstagedFiles"].Items.Count >= 1500 && VisualDescendants<DataGridRow>(tables["UnstagedFiles"]).Count() < 80, "File virtualization");
            Expect(tables["FileDiff"].Items.Count >= 3000 && VisualDescendants<DataGridRow>(tables["FileDiff"]).Count() < 80, "Diff virtualization");
            await viewModel.CloseAsync(); window.Close();
        });
        await Check("Commit entry retains shell history, counts unique paths, and owns modal lifetime", async () =>
        {
            var shellInteraction = new CommitInteraction(); var shell = new ShellViewModel(reader, shellInteraction, identities, actions: actions);
            shell.CompleteStartup(new(new() { RepositoryPaths = [repo], SelectedRepositoryPath = repo }, SettingsLoadStatus.Loaded)); await shell.WaitForIdleAsync();
            var history = shell.Commits; Expect(history.Count > 0 && shell.CommitButtonLabel != UiText.Get("CommitOpenUnknown"), "Count read follows history");
            shellInteraction.OnOpen = async editor =>
            {
                Expect(shell.IsHistory && ReferenceEquals(history, shell.Commits) && !shell.CanNavigate, "History retained behind owned editor");
                await editor.StartLoading(); await editor.DiffTask;
            };
            shell.ShowChangesCommand.Execute(null); await shell.CommitDialogTask;
            Expect(shell.CanNavigate && ReferenceEquals(history, shell.Commits), "Dismiss without mutation retains history page");
            shell.BeginShutdown(); await shell.WaitForIdleAsync();
        });
        await Check("Cancelling an active commit hook drains its process and preserves the staged work", async () =>
        {
            var target = Fixture("cancel commit"); await Git(target, "init", "-b", "main");
            await Git(target, "config", "user.name", "Cancel"); await Git(target, "config", "user.email", "cancel@example.invalid");
            await File.WriteAllTextAsync(Path.Combine(target, "kept.txt"), "kept"); await Git(target, "add", ".");
            await File.WriteAllTextAsync(Path.Combine(target, ".git", "hooks", "pre-commit"), "#!/bin/sh\necho fixture-cancel-ready >&2\nsleep 30\n", new UTF8Encoding(false));
            var context = await actions.ReadWorkingCopyAsync(git, target, default); using var cancellation = new CancellationTokenSource();
            var started = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
            var task = actions.ExecuteWorkingCopyAsync(context, new(WorkingCopyAction.Commit, [], "Cancel me"), cancellation.Token,
                chunk => { if (Encoding.UTF8.GetString(chunk.Bytes).Contains("fixture-cancel-ready")) started.TrySetResult(); });
            await started.Task.WaitAsync(TimeSpan.FromSeconds(10)); cancellation.Cancel();
            var result = await task.WaitAsync(TimeSpan.FromSeconds(10));
            Expect(result.Result.Outcome == ProcessOutcome.Cancelled && (await reader.InspectAsync(git, target, default)).Commit is null &&
                (await reader.ReadLocalChangesAsync(git, target, false, default)).Entries.Single().Group == ChangeGroup.Staged && File.Exists(Path.Combine(target, "kept.txt")), "Cancellation is observed without rollback or file deletion");
        });
        await Check("Unresolved merge previews stay explicit and committing requires resolved index entries", async () =>
        {
            var target = Fixture("commit conflict"); await Git(target, "init", "-b", "main");
            await Git(target, "config", "user.name", "Conflict"); await Git(target, "config", "user.email", "conflict@example.invalid");
            await File.WriteAllTextAsync(Path.Combine(target, "conflict.txt"), "base\n"); await Git(target, "add", "."); await Git(target, "commit", "-m", "base");
            await Git(target, "switch", "-c", "side"); await File.WriteAllTextAsync(Path.Combine(target, "conflict.txt"), "side\n"); await Git(target, "commit", "-am", "side");
            await Git(target, "switch", "main"); await File.WriteAllTextAsync(Path.Combine(target, "conflict.txt"), "main\n"); await Git(target, "commit", "-am", "main");
            var merge = await runner.RunAsync(new(git.Executable, target, ["merge", "side"]), default); Expect(merge.ExitCode != 0, "Fixture conflict");
            var editor = new CommitViewModel(target, git, reader, actions, interaction); await editor.StartLoading(); await editor.DiffTask;
            Expect(editor.Unstaged.Single().Change.Group == ChangeGroup.Conflicted && !editor.CommitCommand.CanExecute(null) &&
                editor.DiffNotice == UiText.Get("DiffConflict") && editor.DiffRows.Count > 0, "Combined preview and unresolved state are visible");
            await File.WriteAllTextAsync(Path.Combine(target, "conflict.txt"), "resolved\n"); editor.StageCommand.Execute(null); await editor.OperationTask; await editor.DiffTask;
            Expect(editor.CommitCommand.CanExecute(null) && !editor.CanAmend, "Resolved merge accepts commit, not amend");
            editor.CommitCommand.Execute(null); await editor.OperationTask; await editor.DiffTask;
            Expect((await reader.InspectAsync(git, target, default)).Operation == RepositoryOperation.None && (await Git(target, "rev-list", "--parents", "-n", "1", "HEAD")).Split(' ').Length == 3, "Real merge commit and recovery state cleared by Git");
            await editor.CloseAsync();
        });
    }
    private sealed class CommitInteraction : IUserInteraction
    {
        public bool AllowAmend = true; public string? AmendHash;
        public Func<CommitViewModel, Task>? OnOpen;
        public string? ChooseRepository() => null;
        public string? ChooseGit() => null;
        public bool ConfirmAmend(string branch, string hash) { AmendHash = hash; return AllowAmend; }
        public Task ShowCommitAsync(CommitViewModel model) => OnOpen?.Invoke(model) ?? Task.CompletedTask;
    }
    private sealed class DiffGateRunner(IProcessRunner inner) : IProcessRunner
    {
        public bool Hold;
        public TaskCompletionSource Started { get; } = new(TaskCreationOptions.RunContinuationsAsynchronously);
        public TaskCompletionSource Release { get; } = new(TaskCreationOptions.RunContinuationsAsynchronously);
        public async Task<ProcessResult> RunAsync(ProcessRequest request, CancellationToken token, Action<ProcessOutput>? output = null)
        {
            if (Hold && request.Arguments.Contains("--patch")) { Started.TrySetResult(); await Release.Task; token = default; }
            return await inner.RunAsync(request, token, output);
        }
    }
}
