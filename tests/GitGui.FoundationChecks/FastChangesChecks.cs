using System.Diagnostics;
using System.IO;
using System.Text;
using System.Windows;
using System.Windows.Automation;
using System.Windows.Controls;
using System.Windows.Threading;
using GitGui.App.ViewModels;
using GitGui.App.Views;
using GitGui.Core.Git;
using GitGui.Core.Processes;
using GitGui.ProcessHost;

namespace GitGui.FoundationChecks;
internal static partial class Program
{
    private static async Task RunFastChangesChecksAsync()
    {
        var runner = new WindowsProcessRunner(); var reader = new RepositoryReader(runner); var identity = new WindowsRepositoryIdentity();
        var git = await reader.DiscoverAsync(null, default); var repo = Fixture("fast changes 日本語");
        async Task<string> Git(params string[] args)
        {
            var result = await runner.RunAsync(new(git.Executable, repo, args), default);
            Expect(result.ExitCode == 0, "Fixture Git: " + string.Join(' ', args) + Encoding.UTF8.GetString(result.StandardError));
            return Encoding.UTF8.GetString(result.StandardOutput).Trim();
        }
        await Git("init", "-b", "main"); await Git("config", "user.name", "Fast"); await Git("config", "user.email", "fast@example.invalid"); await Git("config", "core.autocrlf", "false");
        var literal = "-日本語 [one].txt"; await File.WriteAllTextAsync(Path.Combine(repo, literal), "base\n");
        await File.WriteAllTextAsync(Path.Combine(repo, "other.txt"), "base\n"); await Git("add", "."); await Git("commit", "-m", "Base");
        var head = await Git("rev-parse", "HEAD");
        await File.WriteAllTextAsync(Path.Combine(repo, literal), "edited\n"); await File.WriteAllTextAsync(Path.Combine(repo, "other.txt"), "unrelated\n");
        var gate = new FastReadGate(runner); var observed = new RepositoryReader(gate); var actions = new GitActions(gate, observed, identity);
        await Check("Single-file Stage/Unstage publishes scoped Git status and preserves unrelated files and HEAD", async () =>
        {
            var context = await actions.ReadWorkingCopyAsync(git, repo, default); gate.Requests.Clear();
            var staged = await actions.ExecuteWorkingCopyAsync(context, new(WorkingCopyAction.Stage, [context.Changes.Entries.Single(e => e.Path == literal)]), default);
            Expect(staged.Result.Success && staged.UpdatedContext is not null, "Targeted result supplied");
            context = staged.UpdatedContext!;
            Expect(context.Changes.Entries.Any(e => e.Path == literal && e.Group == ChangeGroup.Staged) && context.Changes.Entries.Any(e => e.Path == "other.txt"), "Real target plus retained unrelated rows");
            var unstaged = await actions.ExecuteWorkingCopyAsync(context, new(WorkingCopyAction.Unstage, [context.Changes.Entries.Single(e => e.Path == literal)]), default);
            Expect(unstaged.UpdatedContext!.Changes.Entries.Any(e => e.Path == literal && e.Group == ChangeGroup.Unstaged) && await Git("rev-parse", "HEAD") == head &&
                await File.ReadAllTextAsync(Path.Combine(repo, literal)) == "edited\n" && await File.ReadAllTextAsync(Path.Combine(repo, "other.txt")) == "unrelated\n", "Index-only unstage");
            Expect(gate.Requests.Where(r => r.Arguments.Contains("status")).All(IsTargeted) && !gate.Requests.Any(r => r.Arguments.Contains("remote")), "No full status or remote enumeration in action");
            Expect(gate.Requests.Any(r => r.Arguments.Contains("reset") && r.Arguments.Contains("--no-refresh") && r.Arguments.Any(a => a.StartsWith("--pathspec-from-file="))), "Explicit path-limited no-refresh reset");
        });
        await Check("Old diff and full-scan delays do not block another file action or publish obsolete results", async () =>
        {
            var model = new CommitViewModel(repo, git, observed, actions, new CommitInteraction());
            gate.HoldPatch = true; await model.StartLoading(); await gate.PatchStarted.Task.WaitAsync(TimeSpan.FromSeconds(10));
            gate.HoldGlobal = true; var watch = Stopwatch.StartNew(); model.StageCommand.Execute(null); await model.OperationTask.WaitAsync(TimeSpan.FromSeconds(4));
            Expect(model.Staged.Any(row => row.Change.Path == literal) && !model.IsMutating && model.Ready && !gate.PatchRelease.Task.IsCompleted, "Index result binds without waiting for old preview");
            Console.WriteLine($"Headless Stage with a held old diff: {watch.Elapsed.TotalMilliseconds:F0} ms");
            await gate.GlobalStarted.Task.WaitAsync(TimeSpan.FromSeconds(10));
            model.Select(true, [model.Staged.Single(row => row.Change.Path == literal)]); watch.Restart(); model.UnstageCommand.Execute(null);
            await model.OperationTask.WaitAsync(TimeSpan.FromSeconds(4));
            Expect(model.Unstaged.Any(row => row.Change.Path == literal) && model.Staged.Count == 0 && !model.IsMutating && model.Ready, "Unstage runs while old full refresh is held");
            Console.WriteLine($"Headless Unstage with a held full scan: {watch.Elapsed.TotalMilliseconds:F0} ms");
            gate.HoldPatch = gate.HoldGlobal = false; gate.PatchRelease.TrySetResult(); gate.GlobalRelease.TrySetResult();
            await model.LoadingTask; await model.DiffTask;
            Expect(model.Staged.Count == 0 && model.DiffTitle == literal && model.DiffRows.Any(row => row.Text == "edited"), "Late reads cannot restore old staged state or change selected path");
            await model.CloseAsync();
        });
        await Check("Background refresh failure retains successful file action and reports stale rows", async () =>
        {
            var model = new CommitViewModel(repo, git, observed, actions, new CommitInteraction()); await model.StartLoading(); await model.DiffTask;
            gate.FailGlobal = true; model.StageCommand.Execute(null); await model.OperationTask; await model.LoadingTask;
            Expect(model.Staged.Any(row => row.Change.Path == literal) && !model.Ready && model.RawOutput.Contains("fixture-full-refresh-failed") &&
                model.Status.Contains("staged", StringComparison.OrdinalIgnoreCase), "Keep successful mutation distinct from read failure");
            gate.FailGlobal = false; await model.CloseAsync(); await Git("reset", "--quiet", "--no-refresh", "HEAD", "--", literal);
        });
        string Pointer(long bytes) => "version https://git-lfs.github.com/spec/v1\noid sha256:" + new string('a', 64) + "\nsize " + bytes + "\n";
        await File.WriteAllTextAsync(Path.Combine(repo, ".gitattributes"), "*.uasset filter=lfs -diff -text\n");
        await File.WriteAllTextAsync(Path.Combine(repo, "asset.uasset"), Pointer(438032)); await Git("add", ".gitattributes", "asset.uasset"); await Git("commit", "-m", "Asset"); var old = await Git("rev-parse", "HEAD");
        await File.WriteAllTextAsync(Path.Combine(repo, "asset.uasset"), Pointer(437600)); await Git("add", "asset.uasset"); await Git("commit", "-m", "Asset updated"); var current = await Git("rev-parse", "HEAD");
        await File.WriteAllTextAsync(Path.Combine(repo, "asset.uasset"), Pointer(500000)); await Git("add", "asset.uasset");
        await File.WriteAllBytesAsync(Path.Combine(repo, "asset.uasset"), new byte[510000]);
        await Git("config", "filter.lfs.clean", "echo forbidden > helper-ran"); await Git("config", "filter.lfs.required", "true");
        var state = await actions.ReadWorkingCopyAsync(git, repo, default);
        await Check("Binary LFS cards use logical sizes without fetching content or invoking filters", async () =>
        {
            var staged = state.Changes.Entries.Single(e => e.Path == "asset.uasset" && e.Group == ChangeGroup.Staged);
            var diff = await observed.ReadFileDiffAsync(state, staged, new(), default);
            Expect(diff.Kind == FileDiffKind.Binary && diff.OldBytes == 437600 && diff.NewBytes == 500000 && diff.OldLfs && diff.NewLfs, "HEAD and index pointer sizes");
            var working = await observed.ReadFileDiffAsync(state, state.Changes.Entries.Single(e => e.Path == "asset.uasset" && e.Group == ChangeGroup.Unstaged), new(), default);
            Expect(working.OldBytes == 500000 && working.NewBytes == 510000 && working.NewLfs, "Real worktree bytes against logical pointer size");
            var files = await observed.ReadCommitFilesAsync(git, repo, current, old, default); var historical = await observed.ReadCommitFileDiffAsync(git, repo, files, files.Files.Single(), default);
            Expect(historical.OldBytes == 438032 && historical.NewBytes == 437600 && historical.OldLfs && historical.NewLfs && !File.Exists(Path.Combine(repo, "helper-ran")), "Historical pointer metadata, no helpers");
            var missing = BinaryComparison.Create(historical, files.Files[0] with { Code = 'A' })!;
            Expect(missing.Sides[0].Missing && missing.Sides[0].Size.Contains("No file") && missing.Sides[1].Extension == ".uasset", "Added side is absent, not confused with a zero-byte file");
        });
        await Check("Binary comparisons render old/new extension and sizes with no bottom activity bar", async () =>
        {
            var model = new CommitViewModel(repo, git, observed, actions, new CommitInteraction()); await model.StartLoading();
            model.Select(true, [model.Staged.Single(row => row.Change.Path == "asset.uasset")]); await model.DiffTask;
            Expect(model.HasBinary && model.Binary!.Sides.All(side => side.Extension == ".uasset" && side.IsLfs), "Real selected binary binding");
            var window = new CommitWindow(model); var content = (FrameworkElement)window.Content;
            Arrange(content, 1200, 760); await Dispatcher.Yield(DispatcherPriority.ContextIdle); SaveRender(content, "binary-commit.png", 1200, 760);
            Arrange(content, 900, 580); await Dispatcher.Yield(DispatcherPriority.ContextIdle); SaveRender(content, "binary-commit-minimum.png", 900, 580);
            Expect(!VisualDescendants<ProgressBar>(content).Any(), "No bottom throbber/progress bar");
            var cards = VisualDescendants<BinaryComparisonView>(content).Single();
            Expect(((FrameworkElement)System.Windows.Media.VisualTreeHelper.GetParent(cards)).Visibility == Visibility.Visible && VisualDescendants<DataGrid>(content).Single(grid => AutomationProperties.GetAutomationId(grid) == "FileDiff").Visibility == Visibility.Collapsed, "Binary cards replace text table");
            var compare = new BinaryComparisonView { DataContext = model.Binary }; Arrange(compare, 380, 245); SaveRender(compare, "binary-cards.png", 380, 245);
            Expect(model.Binary!.Sides.All(side => side.Accessible.Contains(side.Size)), "Accessible labels include old/new size independently of color");
            model.Select(false, [model.Unstaged.Single(row => row.Change.Path == "other.txt")]); await model.DiffTask;
            Arrange(content, 900, 580); await Dispatcher.Yield(DispatcherPriority.ContextIdle);
            Expect(!model.HasBinary && model.DiffRows.Any(row => row.Text == "unrelated") &&
                ((FrameworkElement)System.Windows.Media.VisualTreeHelper.GetParent(cards)).Visibility == Visibility.Collapsed,
                "Selecting text clears binary cards and restores real diff rows");
            model.Select(true, [model.Staged.Single(row => row.Change.Path == "asset.uasset")]); await model.DiffTask;
            var index = Path.Combine(repo, ".git", "index"); byte[]? savedIndex = null;
            // Change only this disposable index after background context publication. The next
            // diff must reject it and expose an error even while keeping the previous cards.
            model.SnapshotChanged += (_, _) =>
            {
                if (!model.IsMutating && model.HasBinary && savedIndex is null)
                { savedIndex = File.ReadAllBytes(index); File.AppendAllText(index, "fixture-external-change"); }
            };
            try
            {
                model.UnstageCommand.Execute(null); await model.OperationTask; await model.DiffTask;
                await model.LoadingTask; await model.DiffTask;
                Arrange(content, 900, 580); await Dispatcher.Yield(DispatcherPriority.ContextIdle);
                Expect(savedIndex is not null && model.HasBinary && model.HasDiffError &&
                    VisualDescendants<TextBlock>(content).Any(block => block.Text == model.DiffNotice && block.Visibility == Visibility.Visible),
                    "Retained binary cards cannot hide a failed refreshed diff");
            }
            finally { if (savedIndex is not null) File.WriteAllBytes(index, savedIndex); }
            await model.CloseAsync(); window.Close();
        });
        await Check("Malformed LFS metadata stays ordinary bytes; deleted binary sides are absent", async () =>
        {
            await File.AppendAllTextAsync(Path.Combine(repo, ".gitattributes"), "*.bad -diff\n");
            var malformed = Pointer(100).Replace("size 100", "size -1");
            await File.WriteAllTextAsync(Path.Combine(repo, "invalid.bad"), malformed);
            await File.WriteAllBytesAsync(Path.Combine(repo, "removed.bin"), [0, 1, 2, 3]);
            await Git("add", ".gitattributes", "invalid.bad", "removed.bin"); await Git("commit", "-m", "Binary boundary fixture");
            await File.WriteAllTextAsync(Path.Combine(repo, "invalid.bad"), "");
            File.Delete(Path.Combine(repo, "removed.bin"));
            var context = await actions.ReadWorkingCopyAsync(git, repo, default);
            var invalid = context.Changes.Entries.Single(entry => entry.Path == "invalid.bad");
            var diff = await observed.ReadFileDiffAsync(context, invalid, new(), default);
            var cards = BinaryComparison.Create(diff, invalid)!;
            Expect(diff.Kind == FileDiffKind.Binary && !diff.OldLfs && diff.OldBytes == Encoding.UTF8.GetByteCount(malformed) &&
                cards.Sides[1].Bytes == 0 && !cards.Sides[1].Missing, "Malformed pointer is raw bytes; zero-byte file exists");
            var removed = context.Changes.Entries.Single(entry => entry.Path == "removed.bin");
            diff = await observed.ReadFileDiffAsync(context, removed, new(), default); cards = BinaryComparison.Create(diff, removed)!;
            Expect(cards.Sides[0].Bytes == 4 && !cards.Sides[0].Missing && cards.Sides[1].Missing, "Real deletion has absent New side");
        });
    }
    private static bool IsTargeted(ProcessRequest request) => request.Arguments.Contains("status") && request.Arguments.Last() != "--";
    private sealed class FastReadGate(IProcessRunner inner) : IProcessRunner
    {
        public bool HoldGlobal, HoldPatch, FailGlobal;
        public List<ProcessRequest> Requests { get; } = [];
        public TaskCompletionSource GlobalStarted { get; } = new(TaskCreationOptions.RunContinuationsAsynchronously);
        public TaskCompletionSource GlobalRelease { get; } = new(TaskCreationOptions.RunContinuationsAsynchronously);
        public TaskCompletionSource PatchStarted { get; } = new(TaskCreationOptions.RunContinuationsAsynchronously);
        public TaskCompletionSource PatchRelease { get; } = new(TaskCreationOptions.RunContinuationsAsynchronously);
        public async Task<ProcessResult> RunAsync(ProcessRequest request, CancellationToken token, Action<ProcessOutput>? output = null)
        {
            lock (Requests) Requests.Add(request);
            if (request.Arguments.Contains("status") && !IsTargeted(request))
            {
                if (FailGlobal) return new(ProcessOutcome.Exited, 1, [], Encoding.UTF8.GetBytes("fixture-full-refresh-failed"), TimeSpan.Zero);
                if (HoldGlobal) { GlobalStarted.TrySetResult(); await GlobalRelease.Task; token = default; }
            }
            if (HoldPatch && request.Arguments.Contains("--patch")) { PatchStarted.TrySetResult(); await PatchRelease.Task; token = default; }
            return await inner.RunAsync(request, token, output);
        }
    }
}
