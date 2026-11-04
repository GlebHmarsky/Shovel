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
using GitGui.ProcessHost;

namespace GitGui.FoundationChecks;
internal static partial class Program
{
    private static bool _historyFileActionChecks;
    private static async Task RunHistoricalFileActionChecksAsync()
    {
        var runner = new WindowsProcessRunner(); var reader = new RepositoryReader(runner); var identity = new WindowsRepositoryIdentity();
        var actions = new GitActions(runner, reader, identity); var git = await reader.DiscoverAsync(null, default); var repo = Fixture("historical files 日本語");
        async Task<string> Git(params string[] args)
        {
            var result = await runner.RunAsync(new(git.Executable, repo, args), default);
            Expect(result.ExitCode == 0, Encoding.UTF8.GetString(result.StandardError)); return Encoding.UTF8.GetString(result.StandardOutput).Trim();
        }
        const string relative = "file 日本語 [one].txt";
        await Git("init", "-b", "main"); await Git("config", "user.name", "History"); await Git("config", "user.email", "history@example.invalid"); await Git("config", "core.autocrlf", "false");
        var binary = new byte[20 * 1024 * 1024]; for (var i = 0; i < binary.Length; i++) binary[i] = (byte)(i % 251);
        await File.WriteAllBytesAsync(Path.Combine(repo, "large.bin"), binary);
        await File.WriteAllTextAsync(Path.Combine(repo, relative), "old\n"); await Git("add", "."); await Git("commit", "-m", "first"); var first = await Git("rev-parse", "HEAD");
        Directory.CreateDirectory(Path.Combine(repo, "folder")); await File.WriteAllTextAsync(Path.Combine(repo, "folder/added.txt"), "added\n");
        await File.WriteAllTextAsync(Path.Combine(repo, relative), "new\n"); await Git("add", "."); await Git("commit", "-m", "second"); var second = await Git("rev-parse", "HEAD");
        var record = new CommitRecord(second, [first], "History", "history@example.invalid", DateTimeOffset.Now, "History", "history@example.invalid", DateTimeOffset.Now, "second");
        async Task Drain(CommitFilesViewModel model) { Task pending; do { pending = model.Pending; await pending; } while (pending != model.Pending); }
        await Check("Historical binary export above 16 MiB preserves exact bytes within the explicit 64 MiB cap", async () =>
        {
            var exported = await actions.ReadHistoricalContentAsync(git, repo, first, "large.bin", default);
            Expect(exported.Length == binary.Length && exported.SequenceEqual(binary), "20 MiB native blob export is exact");
        });
        await Check("Historical file export reads immutable selected content and native filters", async () =>
        {
            await File.WriteAllTextAsync(Path.Combine(repo, relative), "dirty\n");
            Expect(Encoding.UTF8.GetString(await actions.ReadHistoricalContentAsync(git, repo, first, relative, default)) == "old\n", "Historical version ignores dirty working copy");
            await File.WriteAllTextAsync(Path.Combine(repo, ".gitattributes"), "*.txt filter=fixture\n");
            await Git("config", "filter.fixture.smudge", "sed s/old/filtered/"); await Git("config", "filter.fixture.clean", "cat");
            Expect(Encoding.UTF8.GetString(await actions.ReadHistoricalContentAsync(git, repo, first, relative, default)) == "filtered\n", "Native smudge is preserved");
            File.Delete(Path.Combine(repo, ".gitattributes"));
        });
        await Check("Restore selected version changes worktree but preserves staged content", async () =>
        {
            await File.WriteAllTextAsync(Path.Combine(repo, relative), "staged\n"); await Git("add", "--", relative);
            await File.WriteAllTextAsync(Path.Combine(repo, relative), "unstaged\n"); var before = await Git("show", ":" + relative);
            var review = await actions.ReviewHistoricalFileAsync(git, repo, first, relative, default);
            Expect((await actions.ExecuteHistoricalFileAsync(review, HistoricalFileMutation.Restore, default)).Success, "Native restore succeeds");
            Expect(await File.ReadAllTextAsync(Path.Combine(repo, relative)) == "old\n" && await Git("show", ":" + relative) == before, "Only worktree is replaced");
        });
        await Check("Historical history and blame use the selected source commit rather than working HEAD", async () =>
        {
            var context = await actions.ReadHistoricalInspectionAsync(git, repo, first, relative, default);
            var entry = context.Changes.Entries.Single();
            var history = await actions.ReadFileHistoryAsync(context, entry, 0, default);
            var blame = await actions.ReadFileBlameAsync(context, entry, default);
            Expect(context.Repository.Commit == first && history.Commits.Count == 1 && history.Commits[0].Hash == first, "History starts at captured selected commit");
            Expect(blame.Count == 1 && blame[0].Hash == first && blame[0].Text == "old", "Blame ignores later HEAD/index/worktree text");
        });
        await Check("Historical LFS actions change only selected path attributes and preserve unrelated rules", async () =>
        {
            await Git("lfs", "install", "--local");
            const string unrelated = "unrelated/*.dat filter=other\n";
            await File.WriteAllTextAsync(Path.Combine(repo, ".gitattributes"), unrelated);
            var review = await actions.ReviewHistoricalFileAsync(git, repo, second, relative, default);
            Expect((await actions.ExecuteHistoricalFileAsync(review, HistoricalFileMutation.LfsTrack, default)).Success, "Track succeeds");
            Expect((await Git("check-attr", "filter", "--", relative)).EndsWith(": lfs", StringComparison.Ordinal), "Native Git sees selected path LFS rule");
            review = await actions.ReviewHistoricalFileAsync(git, repo, second, relative, default);
            Expect((await actions.ExecuteHistoricalFileAsync(review, HistoricalFileMutation.LfsUntrack, default)).Success, "Untrack succeeds");
            Expect((await Git("check-attr", "filter", "--", relative)).EndsWith(": unspecified", StringComparison.Ordinal) &&
                (await File.ReadAllTextAsync(Path.Combine(repo, ".gitattributes"))).StartsWith(unrelated, StringComparison.Ordinal), "Selected LFS rule removed in effect and unrelated rule preserved");
            File.Delete(Path.Combine(repo, ".gitattributes"));
        });
        await Check("Reset to absent version removes only reviewed path and keeps staged entry", async () =>
        {
            var review = await actions.ReviewHistoricalFileAsync(git, repo, first, "folder/added.txt", default);
            Expect((await actions.ExecuteHistoricalFileAsync(review, HistoricalFileMutation.Restore, default)).Success, "Absent source removal succeeds");
            Expect(!File.Exists(Path.Combine(repo, "folder/added.txt")) && await Git("show", ":folder/added.txt") == "added", "Index and other files retained");
        });
        await Check("Historical reset refuses a file edited after review", async () =>
        {
            var review = await actions.ReviewHistoricalFileAsync(git, repo, second, relative, default);
            await File.WriteAllTextAsync(Path.Combine(repo, relative), "newer outside edit\n");
            try { await actions.ExecuteHistoricalFileAsync(review, HistoricalFileMutation.Restore, default); throw new InvalidOperationException("Stale file accepted"); }
            catch (GitActionException error) { Expect(error.Code == "CommitStale", "Stale error"); }
            Expect(await File.ReadAllTextAsync(Path.Combine(repo, relative)) == "newer outside edit\n", "Concurrent edit retained");
        });
        await Check("History file menu captures reset targets and file tree browses immutable directories", async () =>
        {
            var interaction = new HistoricalFileInteraction(); var model = new CommitFilesViewModel(reader, git, repo, record) { Actions = actions, Interaction = interaction };
            model.Start(); await Drain(model); var row = model.Files.Single(item => item.Change.Path == relative);
            var menu = model.CreateHistoryFileMenu(row)!; var before = await Git("status", "--porcelain");
            Expect(menu.Resets.Count == 3 && menu.Resets[1].Revision == second && menu.Resets[2].Revision == first, "HEAD, selected and compared parent targets are explicit");
            menu.CopyCommand.Execute(null); Expect(interaction.Copied == relative && await Git("status", "--porcelain") == before, "Menu and copy are inert");
            var retainedPath = model.DisplayedPath; var retainedLines = model.Lines;
            model.BrowseTree(second, ""); await Drain(model);
            Expect(model.TreeMode && model.Files.Any(item => item.Change.Path == "folder" && item.Change.IndexMode == "040000"), "Root tree lists folder");
            model.OpenTreeDirectory(model.Files.Single(item => item.Change.Path == "folder")); await Drain(model);
            Expect(model.TreeDirectory == "folder" && model.Files.Single().Change.Path == "folder/added.txt", "Directory expands from selected immutable tree despite missing worktree file");
            Expect(model.DisplayedPath == retainedPath && ReferenceEquals(model.Lines, retainedLines), "Tree navigation retains the labeled comparison until another file is selected");
            var view = new CommitFilesView { DataContext = model }; Arrange(view, 950, 400); await Dispatcher.Yield(DispatcherPriority.ContextIdle); SaveRender(view, "history-file-tree.png", 950, 400);
            model.Stop();
        });
        await Git("config", "diff.tool", "fixture"); await Git("config", "difftool.fixture.cmd", "cat \"$LOCAL\"; cat \"$REMOTE\"");
        await Check("External historical diff compares selected trees and retains executed command", async () =>
        {
            var comparison = await reader.ReadCommitFilesAsync(git, repo, second, first, default); var events = new List<ProcessOutput>();
            var result = await actions.DiffHistoricalFileAsync(git, repo, second, first, comparison.Files.Single(item => item.Path == relative), default, events.Add);
            Expect(result.Success && result.Output.Contains("old") && result.Output.Contains("new") && events.Any(output => output.IsCommand), "Native tool sees historical sides and command metadata");
            var initial = await reader.ReadCommitFilesAsync(git, repo, first, null, default);
            var rootDiff = await actions.DiffHistoricalFileAsync(git, repo, first, null, initial.Files.Single(item => item.Path == relative), default);
            Expect(rootDiff.Success && rootDiff.Output.Contains("old"), "Initial commit compares with a materialized empty tree");
        });
        await Check("History action survives view stop and retains ownership through reconciliation", async () =>
        {
            var gate = new HistoricalToolGate(runner); var gatedReader = new RepositoryReader(gate); var gated = new GitActions(gate, gatedReader, identity);
            var owned = false; var reconcile = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously); var reconciling = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
            var journal = new ActivityJournal();
            var model = new CommitFilesViewModel(gatedReader, git, repo, record) { Actions = gated, Interaction = new HistoricalFileInteraction(), Journal = journal, FileActionStateChanged = (_, value) => owned = value };
            model.Start(); await Drain(model); model.RunHistoryAction("diff", second, first, model.Files.Single(item => item.Change.Path == relative).Change);
            await gate.Started.Task.WaitAsync(TimeSpan.FromSeconds(10)); await Dispatcher.Yield(DispatcherPriority.ContextIdle);
            Expect(journal.Entries.Any(entry => !entry.Background && entry.Running && entry.Output.Contains("difftool")), "Running user action already exposes its command in Activity");
            model.Stop();
            Expect(owned && model.FileActionBusy && !model.FileActionTask.IsCompleted && !gate.Cancelled, "View stop does not cancel owned action");
            gate.Release.TrySetResult(); await model.FileActionTask; Expect(!owned && !model.FileActionBusy, "Owner releases after action drain");
            var restored = new CommitFilesViewModel(reader, git, repo, record) { Actions = actions, Interaction = new HistoricalFileInteraction(), FileActionStateChanged = (_, value) => owned = value,
                AfterFileMutation = async () => { reconciling.TrySetResult(); await reconcile.Task; } };
            restored.RunHistoryAction("restore", second, first, new(ChangeGroup.Staged, relative, 'M'));
            await reconciling.Task.WaitAsync(TimeSpan.FromSeconds(15)); Expect(owned && !restored.FileActionTask.IsCompleted, "Ownership includes reconciliation");
            reconcile.TrySetResult(); await restored.FileActionTask; Expect(!owned, "Reconciliation releases owner"); restored.Stop();
        });
        await Check("Cancelling comparison reads leaves an accepted history tool running", async () =>
        {
            var gate = new HistoricalToolGate(runner); var model = new CommitFilesViewModel(reader, git, repo, record) { Actions = new(gate, new(gate), identity), Interaction = new HistoricalFileInteraction() };
            model.Start(); await Drain(model); model.RunHistoryAction("diff", second, first, model.Files.Single(item => item.Change.Path == relative).Change);
            await gate.Started.Task.WaitAsync(TimeSpan.FromSeconds(10)); model.CancelCommand.Execute(null);
            Expect(!gate.Cancelled && model.FileActionBusy, "View cancellation cannot kill the accepted tool");
            gate.Release.TrySetResult(); await model.FileActionTask.WaitAsync(TimeSpan.FromSeconds(5));
            Expect(!model.FileActionBusy && !gate.Cancelled, "Tool finishes normally"); model.Stop();
        });
    }
    private sealed class HistoricalFileInteraction : IUserInteraction
    {
        public string Copied = "";
        public string? ChooseRepository() => null;
        public string? ChooseGit() => null;
        public void CopyFilePaths(string text) => Copied = text;
        public bool ConfirmRecovery(string text, string title) => true;
    }
    private sealed class HistoricalToolGate(IProcessRunner inner) : IProcessRunner
    {
        public TaskCompletionSource Started { get; } = new(TaskCreationOptions.RunContinuationsAsynchronously);
        public TaskCompletionSource Release { get; } = new(TaskCreationOptions.RunContinuationsAsynchronously);
        public bool Cancelled;
        public async Task<ProcessResult> RunAsync(ProcessRequest request, CancellationToken token, Action<ProcessOutput>? output = null)
        {
            if (request.Arguments.Contains("difftool"))
            {
                Started.TrySetResult();
                try { await Release.Task.WaitAsync(token); }
                catch (OperationCanceledException) { Cancelled = true; return new(ProcessOutcome.Cancelled, null, [], [], TimeSpan.Zero); }
            }
            return await inner.RunAsync(request, token, output);
        }
    }
}
