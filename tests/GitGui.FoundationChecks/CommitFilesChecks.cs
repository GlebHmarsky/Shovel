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
    private static async Task RunCommitFilesChecksAsync()
    {
        var runner = new WindowsProcessRunner(); var reader = new RepositoryReader(runner);
        var git = await reader.DiscoverAsync(null, default); var repo = Fixture("committed files 日本語");
        async Task<string> Git(params string[] args)
        {
            var result = await runner.RunAsync(new(git.Executable, repo, args), default);
            Expect(result.ExitCode == 0, "Fixture Git " + string.Join(' ', args) + Encoding.UTF8.GetString(result.StandardError));
            return Encoding.UTF8.GetString(result.StandardOutput).Trim();
        }
        async Task Write(string path, string text) => await File.WriteAllTextAsync(Path.Combine(repo, path), text);
        await Git("init", "-b", "main"); await Git("config", "user.name", "Files"); await Git("config", "user.email", "files@example.invalid"); await Git("config", "core.autocrlf", "false");
        await Write("-日本語 [file].txt", "before\n"); await Write("deleted.txt", "deleted\n"); await Write("renamed.txt", "rename me\n");
        await File.WriteAllBytesAsync(Path.Combine(repo, "asset.bin"), [0, 255, 1]); await Git("add", "."); await Git("commit", "-m", "Initial");
        var root = await Git("rev-parse", "HEAD");
        await Check("Initial commit files compare to empty tree with real text and binary sizes", async () =>
        {
            var comparison = await reader.ReadCommitFilesAsync(git, repo, root, null, default);
            Expect(comparison.Files.Count == 4 && comparison.Files.All(f => f.Code == 'A'), "All initial files added");
            var text = await reader.ReadCommitFileDiffAsync(git, repo, comparison, comparison.Files.Single(f => f.Path.StartsWith('-')), default);
            Expect(text.OldBytes == 0 && text.Lines.Any(l => l.Kind == DiffLineKind.Added && l.Text == "before"), "Initial text from object");
            var binary = await reader.ReadCommitFileDiffAsync(git, repo, comparison, comparison.Files.Single(f => f.Path == "asset.bin"), default);
            Expect(binary.Kind == FileDiffKind.Binary && binary.OldBytes == 0 && binary.NewBytes == 3, "Initial binary size");
        });
        await Write("-日本語 [file].txt", "after\nsecond"); File.Delete(Path.Combine(repo, "deleted.txt"));
        await Git("mv", "renamed.txt", "new name.txt"); await Write("added.txt", "added\n"); await File.WriteAllBytesAsync(Path.Combine(repo, "asset.bin"), [0, 255, 1, 2, 3]);
        await Git("add", "--all"); await Git("commit", "-m", "Second"); var second = await Git("rev-parse", "HEAD");
        await Check("Committed modifications/additions/deletions/renames ignore dirty index and working files", async () =>
        {
            await Write("-日本語 [file].txt", "uncommitted secret\n"); await Git("add", "--all");
            var comparison = await reader.ReadCommitFilesAsync(git, repo, second, root, default);
            Expect(comparison.Files.Single(f => f.Path == "new name.txt") is { Code: 'R', OriginalPath: "renamed.txt" }, "Exact rename paths");
            Expect(comparison.Files.Single(f => f.Path == "deleted.txt").Code == 'D' && comparison.Files.Single(f => f.Path == "added.txt").Code == 'A', "Added and deleted types");
            var diff = await reader.ReadCommitFileDiffAsync(git, repo, comparison, comparison.Files.Single(f => f.Path.StartsWith('-')), default);
            Expect(diff.RawPatch.Contains("+second") && !diff.RawPatch.Contains("secret") && diff.Lines.Any(l => l.Kind == DiffLineKind.Removed), "Immutable two-tree diff, EOF marker");
            var removed = await reader.ReadCommitFileDiffAsync(git, repo, comparison, comparison.Files.Single(f => f.Code == 'D'), default);
            Expect(removed.NewBytes == 0 && removed.Removed == 1, "Deleted file size and removed lines");
            var binary = await reader.ReadCommitFileDiffAsync(git, repo, comparison, comparison.Files.Single(f => f.Path == "asset.bin"), default);
            Expect(binary.OldBytes == 3 && binary.NewBytes == 5, "Stored binary delta");
            await Git("reset", "--hard", "HEAD"); // Disposable fixture only.
        });
        await Git("switch", "-c", "side", root); await Write("side.txt", "side\n"); await Git("add", "."); await Git("commit", "-m", "Side"); var side = await Git("rev-parse", "HEAD");
        await Git("switch", "main"); await Git("merge", "--no-ff", "side", "-m", "Merge"); var merge = await Git("rev-parse", "HEAD");
        await Check("Merge comparisons expose each parent rather than a misleading combined file list", async () =>
        {
            var first = await reader.ReadCommitFilesAsync(git, repo, merge, second, default);
            var other = await reader.ReadCommitFilesAsync(git, repo, merge, side, default);
            Expect(first.Files.Single().Path == "side.txt" && other.Files.Count == 5 && other.Files.All(f => f.Path != "side.txt"), "Parent-specific changes");
        });
        await Check("Committed diff reads suppress external helpers and expose missing/oversized objects", async () =>
        {
            await Git("config", "diff.external", "echo unexpected > helper-ran");
            var comparison = await reader.ReadCommitFilesAsync(git, repo, second, root, default);
            await reader.ReadCommitFileDiffAsync(git, repo, comparison, comparison.Files.First(), default);
            Expect(!File.Exists(Path.Combine(repo, "helper-ran")), "No external diff"); await Git("config", "--unset", "diff.external");
            await ExpectException<GitReadException>(() => reader.ReadCommitFilesAsync(git, repo, "--bad", null, default));
            await ExpectException<GitReadException>(() => reader.ReadCommitFilesAsync(git, repo, new string('a', 40), null, default));
            await Write("large.txt", new string('x', RepositoryReader.MaximumDiffLineLength + 1)); await Git("add", "."); await Git("commit", "-m", "Large");
            var large = await reader.ReadCommitFilesAsync(git, repo, await Git("rev-parse", "HEAD"), merge, default);
            Expect((await reader.ReadCommitFileDiffAsync(git, repo, large, large.Files.Single(), default)).Kind == FileDiffKind.TooLarge, "Explicit diff limit");
        });
        await Check("Committed submodule pointers stay visible despite configured ignore/expanded diff modes", async () =>
        {
            await Git("update-index", "--add", "--cacheinfo", "160000," + root + ",module"); await Git("commit", "-m", "Gitlink"); var before = await Git("rev-parse", "HEAD");
            await Git("update-index", "--cacheinfo", "160000," + second + ",module"); await Git("commit", "-m", "Update gitlink"); var after = await Git("rev-parse", "HEAD");
            await Git("config", "diff.submodule", "diff"); await Git("config", "diff.ignoreSubmodules", "all");
            var comparison = await reader.ReadCommitFilesAsync(git, repo, after, before, default);
            var file = comparison.Files.Single(); var diff = await reader.ReadCommitFileDiffAsync(git, repo, comparison, file, default);
            Expect(file.Path == "module" && diff.Kind == FileDiffKind.Submodule && diff.OldBytes is null && diff.NewBytes is null && diff.RawPatch.Contains("Subproject commit " + second), "Pointer-only change survives configuration");
            await Git("config", "--unset", "diff.submodule"); await Git("config", "--unset", "diff.ignoreSubmodules");
        });
        var record = new CommitRecord(second, [root], "Files", "files@example.invalid", DateTimeOffset.Now, "Files", "files@example.invalid", DateTimeOffset.Now, "Second");
        async Task Drain(CommitFilesViewModel model) { Task task; do { task = model.Pending; await task; } while (task != model.Pending); }
        await Check("Empty commits remain empty and shallow boundaries cannot masquerade as initial commits", async () =>
        {
            await Git("commit", "--allow-empty", "-m", "Empty"); var hash = await Git("rev-parse", "HEAD"); var parent = await Git("rev-parse", "HEAD^");
            Expect((await reader.ReadCommitFilesAsync(git, repo, hash, parent, default)).Files.Count == 0, "Real empty commit");
            try { await reader.ReadCommitFilesAsync(git, repo, second, null, default); throw new InvalidOperationException("Expected boundary rejection"); }
            catch (GitReadException error) { Expect(error.Failure == GitFailure.IncompleteHistory, "Use actual object parents, not a shallow history root"); }
        });
        await Check("Shell Changes tab follows commit selection without delaying history and stops on navigation", async () =>
        {
            var actions = new GitActions(runner, reader, new WindowsRepositoryIdentity());
            var shell = new ShellViewModel(reader, new CommitInteraction(), new WindowsRepositoryIdentity(), actions: actions);
            shell.CompleteStartup(new(new() { RepositoryPaths = [repo], SelectedRepositoryPath = repo }, GitGui.Core.Settings.SettingsLoadStatus.Loaded));
            await shell.WaitForIdleAsync(); Expect(shell.CommitFiles is null, "No unsolicited file read before Changes opens");
            shell.DetailsIndex = 4; var model = shell.CommitFiles!; await Drain(model);
            shell.SelectedCommit = shell.Commits.Single(c => c.Commit.Hash == second); model = shell.CommitFiles!;
            await Dispatcher.Yield(DispatcherPriority.ContextIdle); await Drain(model);
            Expect(model.Files.Count == 5 && model.CommitHash == second, "Selected history commit drives real files");
            var window = new MainWindow(shell); var content = (FrameworkElement)window.Content;
            Arrange(content, 1200, 760); await Dispatcher.Yield(DispatcherPriority.ContextIdle); SaveRender(content, "history-changes.png", 1200, 760);
            Arrange(content, 900, 580); await Dispatcher.Yield(DispatcherPriority.ContextIdle); Arrange(content, 900, 580);
            SaveRender(content, "history-changes-minimum.png", 900, 580);
            var details = VisualDescendants<TabControl>(content).Single(); var filesView = VisualDescendants<CommitFilesView>(content).Single();
            var output = VisualDescendants<Expander>(filesView).Single();
            Expect(output.TransformToAncestor(content).Transform(new Point(0, output.ActualHeight)).Y <= content.ActualHeight, "Minimum-size Git output remains inside the workspace");
            shell.DetailsIndex = 0; Expect(shell.CommitFiles is null, "Leave Changes releases active model");
            shell.BeginShutdown(); await shell.WaitForIdleAsync(); window.Close();
        });
        await Check("Changes panel binds real files/diffs, semantic markers and normal/minimum offscreen layout", async () =>
        {
            var model = new CommitFilesViewModel(reader, git, repo, record); model.Start(); await Drain(model);
            Expect(model.Files.Count == 5 && model.Lines.Count > 0 && !model.Busy, "Initial list followed by selected diff");
            var view = new CommitFilesView { DataContext = model }; Arrange(view, 950, 360); await Dispatcher.Yield(DispatcherPriority.ContextIdle);
            SaveRender(view, "commit-files.png", 950, 360); Arrange(view, 650, 260); SaveRender(view, "commit-files-minimum.png", 650, 260);
            Expect(new ChangeRow(new(ChangeGroup.Staged, "a", 'M')).Marker == "M" && new ChangeRow(new(ChangeGroup.Staged, "b", 'D')).Marker == "−" && new ChangeRow(new(ChangeGroup.Staged, "c", 'A')).Marker == "+", "Marker shapes preserve non-color meaning");
            Expect(model.Files.All(file => !file.CommittedDetails.Contains(file.Group, StringComparison.OrdinalIgnoreCase)), "Historical accessibility descriptions do not call committed files staged");
            var table = VisualDescendants<DataGrid>(view).Single(g => AutomationProperties.GetAutomationId(g) == "CommitFiles");
            Expect(table.Items.Count == 5 && table.EnableRowVirtualization, "Real table remains virtualized"); model.Stop(); await Drain(model);
        });
        await Check("Parent changes/cancellation discard late results; failed refresh retains useful files", async () =>
        {
            var gate = new CommitFileGate(runner); var model = new CommitFilesViewModel(new(gate), git, repo, record with { Hash = merge, Parents = [second, side] });
            model.Start(); await gate.Started.Task.WaitAsync(TimeSpan.FromSeconds(10)); model.Parent = model.Parents[1]; gate.Hold = false; gate.Release.TrySetResult(); await Drain(model);
            Expect(model.Files.Count == 5 && model.Files.All(f => f.Change.Path != "side.txt"), "No old-parent result published");
            var files = model.Files; gate.Fail = true; model.RefreshCommand.Execute(null); await Drain(model);
            Expect(ReferenceEquals(files, model.Files) && model.Output.Contains("fixture failed") && !model.Busy, "Retain same-comparison files and actual error"); model.Stop();
        });
        await Check("Working-copy validation no longer enumerates history refs and preserves detached/unborn/upstream state", async () =>
        {
            var observed = new WorkingTimingRunner(); var localReader = new RepositoryReader(observed); var actions = new GitActions(observed, localReader, new WindowsRepositoryIdentity());
            var context = await actions.ReadWorkingCopyAsync(git, repo, default);
            Expect(!observed.Samples.Any(s => s.Name == "for-each-ref") && context.Repository.Branch == "main", "No history enumeration for local context");
            await Git("switch", "--detach"); context = await actions.ReadWorkingCopyAsync(git, repo, default);
            Expect(context.Repository.HeadKind == HeadKind.Detached && context.Repository.Commit is not null, "Detached retained");
            await Git("switch", "--orphan", "unborn"); context = await actions.ReadWorkingCopyAsync(git, repo, default);
            Expect(context.Repository.HeadKind == HeadKind.Unborn && context.Repository.Branch == "unborn", "Unborn retained");
        });
    }
    private sealed class CommitFileGate(IProcessRunner inner) : IProcessRunner
    {
        public bool Hold = true, Fail;
        public TaskCompletionSource Started { get; } = new(TaskCreationOptions.RunContinuationsAsynchronously);
        public TaskCompletionSource Release { get; } = new(TaskCreationOptions.RunContinuationsAsynchronously);
        public async Task<ProcessResult> RunAsync(ProcessRequest request, CancellationToken token, Action<ProcessOutput>? output = null)
        {
            if (request.Arguments.Contains("--raw"))
            {
                if (Fail) return new(ProcessOutcome.Exited, 1, [], Encoding.UTF8.GetBytes("fixture failed"), TimeSpan.Zero);
                if (Hold) { Started.TrySetResult(); await Release.Task; token = default; }
            }
            return await inner.RunAsync(request, token, output);
        }
    }
}
