using System.IO;
using System.Text;
using System.Windows;
using System.Windows.Automation;
using System.Windows.Controls;
using System.Windows.Threading;
using GitGui.App.Localization;
using GitGui.App.ViewModels;
using GitGui.App.Views;
using GitGui.Core.Git;
using GitGui.Core.Processes;
using GitGui.ProcessHost;

namespace GitGui.FoundationChecks;

internal static partial class Program
{
    private static async Task RunHunkChecksAsync()
    {
        var runner = new WindowsProcessRunner(); var reader = new RepositoryReader(runner);
        var identities = new WindowsRepositoryIdentity(); var actions = new GitActions(runner, reader, identities);
        var git = await reader.DiscoverAsync(null, default); var repo = Fixture("hunks 日本語 spaces");
        const string name = "-literal [日本語].txt";
        var file = Path.Combine(repo, name);
        var original = string.Concat(Enumerable.Range(1, 90).Select(i => $"original line {i:D3}\n"));
        string Changed(params int[] changes)
        {
            var text = original;
            foreach (var index in changes) text = text.Replace($"original line {index:D3}\n", $"changed line {index:D3}\n" + (index == 5 ? "inserted line\n" : ""));
            return text;
        }
        async Task<string> Git(params string[] args)
        {
            var result = await runner.RunAsync(new(git.Executable, repo, args), default);
            Expect(result.ExitCode == 0, "Fixture Git: " + string.Join(' ', args) + " " + Encoding.UTF8.GetString(result.StandardError));
            return Encoding.UTF8.GetString(result.StandardOutput);
        }
        Task<WorkingCopyContext> Read() => actions.ReadWorkingCopyAsync(git, repo, default);
        async Task<(WorkingCopyContext Context, FileDiff Diff)> Preview(ChangeGroup group = ChangeGroup.Unstaged, string path = name, DiffOptions? options = null)
        {
            var context = await Read();
            return (context, await reader.ReadFileDiffAsync(context, context.Changes.Entries.Single(row => row.Path == path && row.Group == group), options ?? new(), default));
        }
        WorkingCopyOptions Options(FileDiff diff, params int[] ids) => new(diff.HunkPreview!.Change.Group == ChangeGroup.Staged ? WorkingCopyAction.UnstageHunks : WorkingCopyAction.StageHunks,
            [diff.HunkPreview.Change], Hunks: new(diff.HunkPreview, ids));
        async Task Apply((WorkingCopyContext Context, FileDiff Diff) preview, params int[] ids)
        {
            var result = await actions.ExecuteWorkingCopyAsync(preview.Context, Options(preview.Diff, ids), default);
            Expect(result.Result.Success, "Hunk apply: " + result.Result.Output);
        }
        async Task Reset()
        {
            // Only this disposable fixture; never reset a working repository.
            await Git("reset", "--hard", "HEAD");
            await File.WriteAllTextAsync(file, Changed(5, 40, 75));
        }
        await Git("init", "-b", "main"); await Git("config", "user.name", "Hunk Fixture"); await Git("config", "user.email", "hunks@example.invalid");
        await Git("config", "core.autocrlf", "false");
        await File.WriteAllTextAsync(file, original); await File.WriteAllTextAsync(Path.Combine(repo, "other.txt"), "unrelated\n");
        await Git("add", "."); await Git("commit", "-m", "base"); await Reset();

        await Check("Selected hunks stage disjoint edits with offsets while preserving worktree and unrelated index content", async () =>
        {
            await File.WriteAllTextAsync(Path.Combine(repo, "other.txt"), "independently staged\n"); await Git("add", "other.txt");
            var preview = await Preview(); Expect(preview.Diff.HunkPreview?.Hunks.Count == 3, "Three separately selectable hunks");
            await Apply(preview, 2, 0, 2);
            Expect(await Git("show", ":" + name) == Changed(5, 75), "Only the first and third blocks reached the index, duplicates deduplicated");
            Expect(await File.ReadAllTextAsync(file) == Changed(5, 40, 75) && await Git("show", ":other.txt") == "independently staged\n", "Worktree and unrelated stage are unchanged");
            await Apply(await Preview(ChangeGroup.Staged), 0);
            Expect(await Git("show", ":" + name) == Changed(75) && await File.ReadAllTextAsync(file) == Changed(5, 40, 75), "Reverse hunk removes just one block from the index");
            await Apply(await Preview(), 1);
            Expect(await Git("show", ":" + name) == Changed(40, 75), "Partially staged file stages against the current index");
        });
        await Check("Invalid hunk selections never fall back to staging a whole file", async () =>
        {
            await Reset(); var preview = await Preview(); var before = await Git("write-tree");
            await ExpectException<GitActionException>(() => actions.ExecuteWorkingCopyAsync(preview.Context, Options(preview.Diff, 99), default));
            await ExpectException<GitActionException>(() => actions.ExecuteWorkingCopyAsync(preview.Context, Options(preview.Diff), default));
            Expect(before == await Git("write-tree"), "Invalid and empty selections leave index untouched");
        });
        await Check("Stale index, same-size/mtime edits, HEAD and configuration reject old hunk previews", async () =>
        {
            await Reset(); var preview = await Preview();
            await File.WriteAllTextAsync(Path.Combine(repo, "other.txt"), "outside client\n"); await Git("add", "other.txt");
            var index = await Git("write-tree");
            await ExpectException<GitActionException>(() => actions.ExecuteWorkingCopyAsync(preview.Context, Options(preview.Diff, 0), default));
            Expect(index == await Git("write-tree"), "External stage retained");
            await Reset(); preview = await Preview(); var time = File.GetLastWriteTimeUtc(file);
            await File.WriteAllTextAsync(file, Changed(5, 40, 75).Replace("original line 060", "external line 060")); File.SetLastWriteTimeUtc(file, time);
            index = await Git("write-tree");
            await ExpectException<GitActionException>(() => actions.ExecuteWorkingCopyAsync(preview.Context, Options(preview.Diff, 0), default));
            Expect(index == await Git("write-tree"), "Hash detects content outside displayed hunks despite preserved size and mtime");
            await Reset(); preview = await Preview(); await Git("config", "diff.algorithm", "patience");
            await ExpectException<GitActionException>(() => actions.ExecuteWorkingCopyAsync(preview.Context, Options(preview.Diff, 0), default));
            await Git("config", "--unset", "diff.algorithm"); preview = await Preview(); await Git("commit", "--allow-empty", "-m", "external tip");
            await ExpectException<GitActionException>(() => actions.ExecuteWorkingCopyAsync(preview.Context, Options(preview.Diff, 0), default));
        });
        await Check("Attribute changes and clean filters cannot turn a suppressed-filter preview into index content", async () =>
        {
            await Reset(); var preview = await Preview(); var index = await Git("write-tree");
            var attributes = Path.Combine(repo, ".gitattributes"); await File.WriteAllTextAsync(attributes, "*.txt filter=fixture\n");
            await ExpectException<GitActionException>(() => actions.ExecuteWorkingCopyAsync(preview.Context, Options(preview.Diff, 0), default));
            Expect(index == await Git("write-tree"), "Attribute-only external change rejected");
            await Git("config", "filter.fixture.clean", "echo ran > filter-ran; cat");
            var filtered = await Preview();
            Expect(filtered.Diff.HunkAvailability == HunkAvailability.Filtered && filtered.Diff.HunkPreview is null && !File.Exists(Path.Combine(repo, "filter-ran")), "Read never runs configured clean filter; whole-file action remains the path");
            await File.WriteAllTextAsync(attributes, "*.txt ident\n");
            Expect((await Preview()).Diff.HunkAvailability == HunkAvailability.Filtered, "Ident conversion explicitly excluded");
            File.Delete(attributes); await Git("config", "--remove-section", "filter.fixture");
        });
        await Check("Canonical hunk output overrides custom prefixes and preserves whitespace without configured fixes", async () =>
        {
            await Reset(); await Git("config", "diff.noprefix", "true"); await Git("config", "diff.interHunkContext", "100");
            await Git("config", "apply.whitespace", "fix"); await Git("config", "apply.ignoreWhitespace", "change");
            await File.WriteAllTextAsync(file, Changed(5, 40, 75).Replace("changed line 040", "changed line 040   "));
            var preview = await Preview(); Expect(preview.Diff.HunkPreview?.Hunks.Count == 3, "Canonical independent blocks despite config");
            await Apply(preview, 1);
            Expect(await Git("show", ":" + name) == Changed(40).Replace("changed line 040", "changed line 040   "), "Selected whitespace retained exactly");
            foreach (var key in new[] { "diff.noprefix", "diff.interHunkContext", "apply.whitespace", "apply.ignoreWhitespace" }) await Git("config", "--unset", key);
        });
        await Check("CRLF normalization and no-final-newline markers survive hunk staging and unstaging", async () =>
        {
            await Reset(); await Git("config", "core.autocrlf", "true");
            var contents = Changed(5, 75).TrimEnd('\n').Replace("\n", "\r\n"); await File.WriteAllTextAsync(file, contents);
            var preview = await Preview(); Expect(preview.Diff.HunkPreview is not null, "Text EOL normalization is supported");
            await Apply(preview, Enumerable.Range(0, preview.Diff.HunkPreview!.Hunks.Count).ToArray());
            Expect(await Git("show", ":" + name) == Changed(5, 75).TrimEnd('\n') && await File.ReadAllTextAsync(file) == contents, "Index LF, working CRLF and EOF retained");
            preview = await Preview(ChangeGroup.Staged); await Apply(preview, Enumerable.Range(0, preview.Diff.HunkPreview!.Hunks.Count).ToArray());
            Expect(await Git("show", ":" + name) == original && await File.ReadAllTextAsync(file) == contents, "Reverse all blocks restores index only");
            await Git("config", "core.autocrlf", "false");
        });
        await Check("Binary, additions, renames and display-only comparisons explain unavailable hunk actions", async () =>
        {
            await Reset();
            Expect((await Preview(options: new(true))).Diff.HunkAvailability == HunkAvailability.PreviewOptions &&
                (await Preview(options: new(false, true))).Diff.HunkAvailability == HunkAvailability.PreviewOptions, "Whitespace/whole-file modes are read-only previews");
            await File.WriteAllBytesAsync(file, [0, 255, 0, 2]); var binary = await Preview();
            Expect(binary.Diff.Kind == FileDiffKind.Binary && binary.Diff.HunkPreview is null, "Binary remains whole-file");
            await Reset(); await File.WriteAllTextAsync(Path.Combine(repo, "new.txt"), "new\n");
            Expect((await Preview(ChangeGroup.Untracked, "new.txt")).Diff.HunkAvailability == HunkAvailability.ChangeType, "Untracked remains whole-file");
            await Git("mv", "other.txt", "renamed.txt");
            Expect((await Preview(ChangeGroup.Staged, "renamed.txt")).Diff.HunkPreview is null, "Rename metadata cannot be partially applied");
            await Reset(); File.Delete(Path.Combine(repo, "new.txt"));
        });
        await Check("Index lock and check failures leave all hunks unapplied and retain the lock/output", async () =>
        {
            await Reset(); var preview = await Preview(); var index = await Git("write-tree");
            var lockPath = Path.Combine(repo, ".git", "index.lock"); await File.WriteAllTextAsync(lockPath, "fixture-owned");
            try
            {
                var result = await actions.ExecuteWorkingCopyAsync(preview.Context, Options(preview.Diff, 0, 2), default);
                Expect(!result.Result.Success && File.ReadAllText(lockPath) == "fixture-owned", "Existing lock blocks write and stays intact");
            }
            finally { File.Delete(lockPath); }
            Expect(index == await Git("write-tree"), "Atomic failure leaves index unchanged");
            var gate = new HunkGateRunner(runner) { FailCheck = true };
            var guarded = new GitActions(gate, new RepositoryReader(gate), identities);
            var failed = await guarded.ExecuteWorkingCopyAsync(preview.Context, Options(preview.Diff, 0), default);
            Expect(!failed.Result.Success && failed.Result.Output.Contains("fixture patch rejected") && gate.Writes == 0 && index == await Git("write-tree"), "Failed precheck never attempts a write or fallback");
        });
        await Check("Cancellation before patch application drains validation without an index write", async () =>
        {
            await Reset(); var preview = await Preview(); var index = await Git("write-tree");
            var gate = new HunkGateRunner(runner) { HoldCheck = true }; var guarded = new GitActions(gate, new RepositoryReader(gate), identities);
            using var cancellation = new CancellationTokenSource();
            var task = guarded.ExecuteWorkingCopyAsync(preview.Context, Options(preview.Diff, 1), cancellation.Token);
            await gate.Started.Task.WaitAsync(TimeSpan.FromSeconds(10)); cancellation.Cancel();
            await ExpectException<OperationCanceledException>(async () => await task.WaitAsync(TimeSpan.FromSeconds(10)));
            Expect(gate.Writes == 0 && index == await Git("write-tree") && await File.ReadAllTextAsync(file) == Changed(5, 40, 75), "Cancelled selection changes neither index nor worktree");
        });
        await Check("External edits during patch validation are rechecked before the cached write", async () =>
        {
            await Reset(); var preview = await Preview(); var index = await Git("write-tree");
            var gate = new HunkGateRunner(runner) { AfterCheck = () => File.WriteAllTextAsync(file, Changed(5, 40, 75).Replace("original line 060", "external line 060")) };
            var guarded = new GitActions(gate, new RepositoryReader(gate), identities);
            await ExpectException<GitActionException>(() => guarded.ExecuteWorkingCopyAsync(preview.Context, Options(preview.Diff, 0), default));
            Expect(gate.Writes == 0 && index == await Git("write-tree"), "Final revalidation prevents the observed concurrent edit from reaching apply");
        });
        await Check("Bound hunk commands select complete blocks, retain drafts and render normal/minimum layouts", async () =>
        {
            await Reset(); var state = new CommitWorkspaceState { Subject = "Draft remains", Description = "Body remains" };
            var model = new CommitViewModel(repo, git, reader, actions, new CommitInteraction(), state);
            await model.StartLoading(); await model.DiffTask;
            var window = new CommitWindow(model); var content = (FrameworkElement)window.Content; Arrange(content, 1200, 760);
            await Dispatcher.Yield(DispatcherPriority.ContextIdle);
            var table = VisualDescendants<DataGrid>(content).Single(grid => AutomationProperties.GetAutomationId(grid) == "FileDiff");
            var blocks = model.DiffRows.Where(row => row.Kind == DiffLineKind.Hunk).ToArray(); Expect(blocks.Length == 3, "Diff block rows");
            table.SelectedItems.Add(blocks[0]); table.SelectedItems.Add(blocks[2]);
            Expect(model.CanApplyHunks && model.HunkActionLabel == UiText.Format("HunkStageMany", 2) && blocks[0].Accessibility.Contains("1"), "Real DataGrid multi-selection and accessible block identity");
            await Dispatcher.Yield(DispatcherPriority.ContextIdle); Arrange(content, 1200, 760);
            var button = VisualDescendants<Button>(content).Single(item => AutomationProperties.GetAutomationId(item) == "ApplyHunks");
            Expect(button.IsEnabled && Equals(button.Content, UiText.Format("HunkStageMany", 2)), "Bound button updates after the dispatcher processes selection");
            SaveRender(content, "hunks-selected.png", 1200, 760);
            Arrange(content, 900, 580); await Dispatcher.Yield(DispatcherPriority.ContextIdle);
            Expect(button.TranslatePoint(new Point(button.ActualWidth, button.ActualHeight), content).X <= 900, "Hunk action remains inside the minimum layout");
            SaveRender(content, "hunks-minimum.png", 900, 580);
            model.ApplyHunksCommand.Execute(null); await model.OperationTask; await model.DiffTask;
            Expect(await Git("show", ":" + name) == Changed(5, 75) && model.Subject == "Draft remains" && model.Description == "Body remains" && model.DiffTitle == name && !model.CanApplyHunks, "Actual bound action preserves draft/path, clears stale selection");
            model.Select(true, [model.Staged.Single(row => row.Change.Path == name)]); await model.DiffTask;
            var chosen = model.DiffRows.Where(row => row.HunkId == 0).ToArray(); model.SelectDiffRows(chosen);
            Expect(model.HunkActionLabel == UiText.Get("HunkUnstage") && model.CanApplyHunks, "Several lines in one block deduplicate to one unstage action");
            model.ApplyHunksCommand.Execute(null); await model.OperationTask; await model.DiffTask;
            Expect(await Git("show", ":" + name) == Changed(75), "Bound reverse action worked");
            model.IgnoreWhitespace = true; await model.DiffTask; Expect(!model.CanApplyHunks && model.HunkHelp == UiText.Get("HunkUnavailablePreviewOptions"), "Availability is explained");
            model.IgnoreWhitespace = false; await model.DiffTask;
            model.SelectDiffRows(model.DiffRows.Where(row => row.HunkId == 0).ToArray());
            await File.WriteAllTextAsync(file, Changed(40)); model.ApplyHunksCommand.Execute(null); await model.OperationTask; await model.DiffTask;
            Expect(model.Status == UiText.Get("HunkStale") && model.Subject == "Draft remains" && !model.CanApplyHunks && await Git("show", ":" + name) == Changed(75), "Stale UI action reports failure, reinspects, retains draft, never writes old patch");
            await model.CloseAsync(); window.Close();
        });
    }

    private sealed class HunkGateRunner(IProcessRunner inner) : IProcessRunner
    {
        public bool FailCheck, HoldCheck;
        public Func<Task>? AfterCheck;
        public int Writes;
        public TaskCompletionSource Started { get; } = new(TaskCreationOptions.RunContinuationsAsynchronously);
        public async Task<ProcessResult> RunAsync(ProcessRequest request, CancellationToken token, Action<ProcessOutput>? output = null)
        {
            if (request.Arguments.Contains("apply"))
            {
                if (!request.Arguments.Contains("--check")) Writes++;
                else
                {
                    if (FailCheck) return new(ProcessOutcome.Exited, 1, [], Encoding.UTF8.GetBytes("fixture patch rejected"), TimeSpan.Zero);
                    if (HoldCheck) { Started.TrySetResult(); await Task.Delay(Timeout.Infinite, token); }
                }
            }
            var result = await inner.RunAsync(request, token, output);
            if (AfterCheck is not null && request.Arguments.Contains("apply") && request.Arguments.Contains("--check")) await AfterCheck();
            return result;
        }
    }
}
