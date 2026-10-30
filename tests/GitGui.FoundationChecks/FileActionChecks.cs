using System.Diagnostics;
using System.IO;
using System.Text;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Threading;
using GitGui.App.Localization;
using GitGui.App.Services;
using GitGui.App.ViewModels;
using GitGui.App.Views;
using GitGui.Core.Git;
using GitGui.Core.Processes;
using GitGui.ProcessHost;

namespace GitGui.FoundationChecks;

internal static partial class Program
{
    private static async Task RunFileActionChecksAsync(bool discardOnly = false)
    {
        var runner = new FileMenuRunner(new WindowsProcessRunner()); var reader = new RepositoryReader(runner);
        var identity = new WindowsRepositoryIdentity(); var actions = new GitActions(runner, reader, identity); var git = await reader.DiscoverAsync(null, default);
        async Task<ProcessResult> Raw(string repo, params string[] args) => await runner.RunAsync(new(git.Executable, repo, args), default);
        async Task<string> Git(string repo, params string[] args)
        {
            var run = await Raw(repo, args); Expect(run.Outcome == ProcessOutcome.Exited && run.ExitCode == 0, "Fixture Git: " + Encoding.UTF8.GetString(run.StandardError));
            return Encoding.UTF8.GetString(run.StandardOutput).Trim();
        }
        Task Write(string repo, string name, string text) => File.WriteAllTextAsync(Path.Combine(repo, name), text, new UTF8Encoding(false));
        async Task<string> Repo(string name, bool commit = true)
        {
            var repo = Fixture("file actions " + name); await Git(repo, "init", "-b", "main");
            await Git(repo, "config", "user.name", "File Fixture"); await Git(repo, "config", "user.email", "file@example.invalid"); await Git(repo, "config", "core.autocrlf", "false");
            if (commit) { await Write(repo, "a.txt", "base\n"); await Write(repo, "b.txt", "other\n"); await Git(repo, "add", "."); await Git(repo, "commit", "-m", "base"); }
            return repo;
        }
        Task<WorkingCopyContext> Context(string repo) => actions.ReadWorkingCopyAsync(git, repo, default);
        async Task<WorkingCopyResult> Act(string repo, WorkingCopyAction action, Func<LocalChange, bool> select, string message = "")
        {
            var context = await Context(repo); return await actions.ExecuteWorkingCopyAsync(context, new(action, context.Changes.Entries.Where(select).ToArray(), message), default);
        }
        async Task Reject(string code, Func<Task> action)
        {
            try { await action(); throw new InvalidOperationException("Expected " + code); }
            catch (GitActionException error) { Expect(error.Code == code, "Expected " + code + "; got " + error.Code); }
        }

        await Check("Discard unstaged restores index, preserves partial staging and unrelated files with scoped results", async () =>
        {
            var repo = await Repo("partial"); await Write(repo, "a.txt", "staged\n"); await Git(repo, "add", "a.txt");
            await Write(repo, "a.txt", "unstaged\n"); await Write(repo, "b.txt", "keep\n");
            var ready = await Context(repo); var watch = Stopwatch.StartNew();
            var result = await actions.ExecuteWorkingCopyAsync(ready, new(WorkingCopyAction.Discard, ready.Changes.Entries.Where(entry => entry.Path == "a.txt" && entry.Group == ChangeGroup.Unstaged).ToArray()), default);
            Console.WriteLine($"Discard cached selection through scoped refresh: {watch.Elapsed.TotalMilliseconds:F0} ms; validation {result.ValidationMilliseconds:F0}, command {result.CommandMilliseconds:F0}, refresh {result.RefreshMilliseconds:F0} ms");
            Expect(result.Result.Success && result.UpdatedContext is not null, "Successful targeted reconciliation");
            Expect(await File.ReadAllTextAsync(Path.Combine(repo, "a.txt")) == "staged\n" && await Git(repo, "show", ":a.txt") == "staged", "Index and worktree retain staged version");
            Expect(await File.ReadAllTextAsync(Path.Combine(repo, "b.txt")) == "keep\n" && await Git(repo, "show", "HEAD:a.txt") == "base", "Unrelated content and HEAD preserved");
        });
        await Check("Unstaged discard honors filters and index hooks without invoking post-checkout", async () =>
        {
            var repo = await Repo("discard hooks");
            await Write(repo, ".gitattributes", "*.txt filter=fixture\n");
            await Git(repo, "config", "filter.fixture.clean", "cat");
            await Git(repo, "config", "filter.fixture.smudge", "sed s/base/converted/");
            await Git(repo, "config", "filter.fixture.required", "true");
            var hooks = Path.Combine(repo, ".git", "discard-hooks"); Directory.CreateDirectory(hooks);
            await Git(repo, "config", "core.hooksPath", hooks.Replace('\\', '/'));
            const string checkoutHook = "#!/bin/sh\nprintf '%s' \"$3\" > .git/checkout-marker\nexit 42\n";
            await Write(repo, ".git/discard-hooks/post-checkout", checkoutHook);
            await Write(repo, ".git/discard-hooks/post-index-change", "#!/bin/sh\nprintf index > .git/index-marker\n");
            await Write(repo, "a.txt", "edited\n");
            var result = await Act(repo, WorkingCopyAction.Discard, e => e.Path == "a.txt");
            Expect(result.Result.Success && await File.ReadAllTextAsync(Path.Combine(repo, "a.txt")) == "converted\n", "Native smudge conversion retained");
            Expect(await Git(repo, "show", ":a.txt") == "base" && File.Exists(Path.Combine(repo, ".git/index-marker")), "Index blob preserved and native index hook invoked");
            Expect(!File.Exists(Path.Combine(repo, ".git/checkout-marker")) && await File.ReadAllTextAsync(Path.Combine(hooks, "post-checkout")) == checkoutHook, "File discard leaves post-checkout untouched and does not invoke it");
            var branch = await Raw(repo, "switch", "-c", "hook-check");
            Expect(branch.ExitCode != 0 && await File.ReadAllTextAsync(Path.Combine(repo, ".git/checkout-marker")) == "1", "Branch checkout retains the configured hook and its failure");
        });
        await Check("Unstaged discard reports required-filter failure and respects existing index locks", async () =>
        {
            var repo = await Repo("discard failure"); await Write(repo, ".gitattributes", "a.txt filter=fixture\n");
            await Git(repo, "config", "filter.fixture.smudge", "exit 42"); await Git(repo, "config", "filter.fixture.required", "true");
            await Write(repo, "a.txt", "edit\n");
            var result = await Act(repo, WorkingCopyAction.Discard, e => e.Path == "a.txt");
            Expect(!result.Result.Success && result.UpdatedContext is null && await Git(repo, "show", ":a.txt") == "base", "No false success or index content change on filter failure");
            await Write(repo, "a.txt", "retry\n"); await Write(repo, ".git/index.lock", "owned fixture lock");
            result = await Act(repo, WorkingCopyAction.Discard, e => e.Path == "a.txt");
            Expect(!result.Result.Success && await File.ReadAllTextAsync(Path.Combine(repo, "a.txt")) == "retry\n" && File.Exists(Path.Combine(repo, ".git/index.lock")), "Native lock failure precedes worktree writes, without lock deletion");
        });
        await Check("Discard staged removes both sides only for selected paths, including added and unborn files", async () =>
        {
            var repo = await Repo("staged"); await Write(repo, "a.txt", "index\n"); await Write(repo, "-日本語 [new].txt", "new\n"); await Git(repo, "add", ".");
            await Write(repo, "a.txt", "work\n"); await Write(repo, "b.txt", "keep\n");
            var result = await Act(repo, WorkingCopyAction.Discard, entry => entry.Group == ChangeGroup.Staged);
            Expect(result.Result.Success && await File.ReadAllTextAsync(Path.Combine(repo, "a.txt")) == "base\n" && !File.Exists(Path.Combine(repo, "-日本語 [new].txt")), "HEAD restored and selected addition removed");
            Expect(await Git(repo, "diff", "--cached", "--name-only") == "" && await File.ReadAllTextAsync(Path.Combine(repo, "b.txt")) == "keep\n", "Unrelated unstaged bytes preserved");
            var unborn = await Repo("unborn", false); await Write(unborn, "a.txt", "new\n"); await Git(unborn, "add", "."); await Write(unborn, "keep.txt", "keep\n");
            Expect((await Act(unborn, WorkingCopyAction.Discard, entry => entry.Group == ChangeGroup.Staged)).Result.Success && !File.Exists(Path.Combine(unborn, "a.txt")) && File.Exists(Path.Combine(unborn, "keep.txt")), "Unborn selected index/file removal only");
        });
        await Check("Discard restores deleted/renamed/binary paths and deletes only selected untracked files", async () =>
        {
            const string binary = "-日本語 [binary].dat";
            var repo = await Repo("rename binary"); await File.WriteAllBytesAsync(Path.Combine(repo, binary), [0, 1, 2]); await Git(repo, "add", "."); await Git(repo, "commit", "-m", "binary");
            await Git(repo, "mv", "a.txt", "renamed [a].txt");
            Expect((await Act(repo, WorkingCopyAction.Discard, entry => entry.Group == ChangeGroup.Staged)).Result.Success && File.Exists(Path.Combine(repo, "a.txt")) && !File.Exists(Path.Combine(repo, "renamed [a].txt")), "Rename both endpoints restored");
            File.Delete(Path.Combine(repo, "a.txt")); await File.WriteAllBytesAsync(Path.Combine(repo, binary), [0, 3, 4]); await Write(repo, "new.txt", "delete\n"); await Write(repo, "keep.txt", "keep\n");
            var result = await Act(repo, WorkingCopyAction.Discard, entry => entry.Path != "keep.txt");
            Expect(result.Result.Success && File.Exists(Path.Combine(repo, "a.txt")) && !File.Exists(Path.Combine(repo, "new.txt")) && File.Exists(Path.Combine(repo, "keep.txt")), "Deletion restore and literal selected delete");
            Expect((await File.ReadAllBytesAsync(Path.Combine(repo, binary))).SequenceEqual(new byte[] { 0, 1, 2 }), "Binary restoration with a literal Unicode/space/dash/glob path");
        });
        await Check("Discard rejects stale index/status and existing locks, with no automatic cleanup", async () =>
        {
            var repo = await Repo("guards"); await Write(repo, "a.txt", "edit\n"); var context = await Context(repo); var options = new WorkingCopyOptions(WorkingCopyAction.Discard, context.Changes.Entries);
            await Git(repo, "add", "a.txt"); await Reject("CommitStale", () => actions.ExecuteWorkingCopyAsync(context, options, default));
            context = await Context(repo); options = new(WorkingCopyAction.Discard, context.Changes.Entries);
            var lockPath = Path.Combine(repo, ".git", "index.lock"); await Write(repo, ".git/index.lock", "fixture lock");
            var locked = await actions.ExecuteWorkingCopyAsync(context, options, default);
            Expect(!locked.Result.Success && File.Exists(lockPath) && await File.ReadAllTextAsync(Path.Combine(repo, "a.txt")) == "edit\n", "Existing Git lock respected");
            // Only remove our known fixture-created lock, never cleanup a user's lock.
            File.Delete(lockPath); await Git(repo, "reset", "--", "a.txt"); context = await Context(repo); options = new(WorkingCopyAction.Discard, context.Changes.Entries);
            await Write(repo, "a.txt", "base\n"); await Reject("CommitStale", () => actions.ExecuteWorkingCopyAsync(context, options, default));
            using var cancelled = new CancellationTokenSource(); cancelled.Cancel(); await ExpectException<OperationCanceledException>(() => actions.ExecuteWorkingCopyAsync(context, options, cancelled.Token));
        });
        if (discardOnly) return;
        await Check("Selected stash preserves unrelated staged/worktree changes and saves selected binary/untracked content", async () =>
        {
            var repo = await Repo("stash"); await Write(repo, "a.txt", "selected index\n"); await Write(repo, "b.txt", "other index\n"); await Git(repo, "add", ".");
            await Write(repo, "a.txt", "selected work\n"); await Write(repo, "b.txt", "other work\n"); await File.WriteAllBytesAsync(Path.Combine(repo, "new [binary].dat"), [0, 12, 13]);
            var result = await Act(repo, WorkingCopyAction.StashFiles, entry => entry.Path != "b.txt", "selected fixture");
            Expect(result.Result.Success && await File.ReadAllTextAsync(Path.Combine(repo, "a.txt")) == "base\n" && !File.Exists(Path.Combine(repo, "new [binary].dat")), "Selected paths saved and restored to HEAD");
            Expect(await Git(repo, "show", ":b.txt") == "other index" && await File.ReadAllTextAsync(Path.Combine(repo, "b.txt")) == "other work\n", "Unrelated index and worktree retained");
            Expect(await Git(repo, "diff-tree", "--no-commit-id", "--name-only", "-r", "stash@{0}^1", "stash@{0}^2") == "a.txt" &&
                await Git(repo, "diff-tree", "--no-commit-id", "--name-only", "-r", "stash@{0}^1", "stash@{0}") == "a.txt", "Both stash trees exclude unrelated changes");
            // Native stash apply --index can reject an unrelated dirty index. Verify preservation
            // above, then clean only this disposable fixture's unrelated file before restoration.
            await Git(repo, "restore", "--staged", "--worktree", "--source=HEAD", "--", "b.txt");
            await Git(repo, "stash", "apply", "--index");
            Expect(await Git(repo, "show", ":a.txt") == "selected index" && await File.ReadAllTextAsync(Path.Combine(repo, "a.txt")) == "selected work\n", "Split selected state restored");
            Expect((await File.ReadAllBytesAsync(Path.Combine(repo, "new [binary].dat"))).SequenceEqual(new byte[] { 0, 12, 13 }), "Saved binary untracked file restored");
            Expect(await Git(repo, "diff-tree", "--no-commit-id", "--name-only", "-r", "stash@{0}^1", "stash@{0}^2") == "a.txt", "Stash index parent contains no unrelated staged changes");
            Expect(!Directory.EnumerateFiles(Path.Combine(repo, ".git"), "shovel-index-*").Any(), "Owned private index removed after completion");
        });
        await Check("Selected patches apply text, binary and untracked additions without mutating source or failed destination", async () =>
        {
            var repo = await Repo("patch"); await File.WriteAllBytesAsync(Path.Combine(repo, "binary.dat"), [0, 1, 2]); await Git(repo, "add", "."); await Git(repo, "commit", "-m", "binary");
            await Write(repo, "a.txt", "changed\n"); await File.WriteAllBytesAsync(Path.Combine(repo, "binary.dat"), [0, 7, 8, 9]); await Write(repo, "new [日本語].txt", "new\n");
            var context = await Context(repo); var patch = await actions.ReadSelectedPatchAsync(context, context.Changes.Entries, default);
            var destination = Path.Combine(_root, "selection.patch"); await FileShellActions.SavePatchAsync(destination, patch, repo, context.Changes.Entries.Select(item => item.Path).ToArray(), default);
            Expect(await Git(repo, "diff", "--cached", "--name-only") == "" && await File.ReadAllTextAsync(Path.Combine(repo, "a.txt")) == "changed\n", "Export did not stage or discard");
            var target = Fixture("patch recipient"); await Git(target, "clone", "--no-hardlinks", repo, "."); await Git(target, "apply", "--binary", destination);
            Expect(await File.ReadAllTextAsync(Path.Combine(target, "new [日本語].txt")) == "new\n" && (await File.ReadAllBytesAsync(Path.Combine(target, "binary.dat"))).SequenceEqual(new byte[] { 0, 7, 8, 9 }), "Export applies real binary/text/new content");
            await Reject("FilePatchDestination", () => FileShellActions.SavePatchAsync(Path.Combine(repo, "a.txt"), patch, repo, ["a.txt"], default));
            using var cancellation = new CancellationTokenSource(); cancellation.Cancel();
            await ExpectException<OperationCanceledException>(() => FileShellActions.SavePatchAsync(destination, [9], repo, [], cancellation.Token));
            Expect((await File.ReadAllBytesAsync(destination)).SequenceEqual(patch), "Cancelled export preserves existing destination");
        });
        await Check("Ignore writes literal paths and extension rules without deleting or untracking files", async () =>
        {
            var repo = await Repo("ignore"); await Write(repo, "!x [one] #.tmp", "keep"); await Write(repo, "other.log", "keep");
            var result = await Act(repo, WorkingCopyAction.IgnoreFiles, entry => entry.Path.EndsWith(".tmp", StringComparison.Ordinal));
            Expect(result.Result.Success && (await Raw(repo, "check-ignore", "--", "!x [one] #.tmp")).ExitCode == 0, "Literal wildcard punctuation ignored");
            await Write(repo, "also.log", "keep"); result = await Act(repo, WorkingCopyAction.IgnoreExtension, entry => entry.Path == "other.log");
            Expect(result.Result.Success && (await Raw(repo, "check-ignore", "--", "also.log")).ExitCode == 0 && File.Exists(Path.Combine(repo, "other.log")), "Extension scope works without deletion");
            await Write(repo, "a.txt", "tracked\n"); await Reject("FileIgnoreTracked", () => Act(repo, WorkingCopyAction.IgnoreFiles, entry => entry.Path == "a.txt"));
            await Write(repo, ".gitignore", "/again.tmp\n!/again.tmp\n"); await Write(repo, "again.tmp", "keep");
            result = await Act(repo, WorkingCopyAction.IgnoreFiles, entry => entry.Path == "again.tmp");
            Expect(result.Result.Success && (await Raw(repo, "check-ignore", "--", "again.tmp")).ExitCode == 0, "Appended rule overrides earlier identical rule and later negation");
        });
        await Check("LFS exact-path tracking/reset preserves unrelated rules, reports overrides and changes no index/history", async () =>
        {
            var repo = await Repo("lfs"); await Write(repo, "space [literal].dat", "asset"); await Write(repo, ".gitattributes", "*.txt text\n");
            var version = await Raw(repo, "lfs", "version");
            if (version.ExitCode != 0) { await Reject("FileLfsMissing", () => Act(repo, WorkingCopyAction.LfsTrack, item => item.Path.EndsWith(".dat"))); return; }
            await Git(repo, "config", "filter.lfs.process", "git-lfs filter-process"); await Git(repo, "config", "filter.lfs.required", "true");
            var result = await Act(repo, WorkingCopyAction.LfsTrack, entry => entry.Path == "space [literal].dat");
            Expect(result.Result.Success && (await Git(repo, "check-attr", "filter", "--", "space [literal].dat")).EndsWith(": lfs", StringComparison.Ordinal), "Exact bracket/space rule applies");
            result = await Act(repo, WorkingCopyAction.LfsUntrack, entry => entry.Path == "space [literal].dat");
            Expect(result.Result.Success && (await Git(repo, "check-attr", "filter", "--", "space [literal].dat")).EndsWith(": unspecified", StringComparison.Ordinal), "Specific reset clears LFS");
            Expect((await File.ReadAllTextAsync(Path.Combine(repo, ".gitattributes"))).StartsWith("*.txt text\n", StringComparison.Ordinal) && await Git(repo, "diff", "--cached", "--name-only") == "", "Other attributes and index preserved");
            Directory.CreateDirectory(Path.Combine(repo, "sub")); await Write(repo, "sub/item.dat", "asset"); await Write(repo, "sub/.gitattributes", "*.dat filter=other\n");
            result = await Act(repo, WorkingCopyAction.LfsTrack, entry => entry.Path == "sub/item.dat"); Expect(result.Result.MessageCode == "FileLfsInherited", "Nested override explained instead of claiming conversion");
        });
        await Check("External Diff executes the configured fixture tool on exact old/new versions and reports nonzero exit", async () =>
        {
            var repo = await Repo("external diff"); await Write(repo, "a.txt", "working\n");
            await Write(repo, ".git/diff-test.sh", "#!/bin/sh\ncp \"$1\" .git/diff-left\ncp \"$2\" .git/diff-right\n");
            await Git(repo, "config", "diff.tool", "shovel-fixture"); await Git(repo, "config", "difftool.shovel-fixture.cmd", "sh .git/diff-test.sh \"$LOCAL\" \"$REMOTE\"");
            var result = await Act(repo, WorkingCopyAction.ExternalDiff, entry => entry.Path == "a.txt");
            Expect(result.Result.Success && await File.ReadAllTextAsync(Path.Combine(repo, ".git/diff-left")) == "base\n" && await File.ReadAllTextAsync(Path.Combine(repo, ".git/diff-right")) == "working\n", "Real configured tool sees selected comparison; no GUI launched");
            await Git(repo, "add", "a.txt"); await Write(repo, "a.txt", "later\n"); result = await Act(repo, WorkingCopyAction.ExternalDiff, entry => entry.Path == "a.txt" && entry.Group == ChangeGroup.Staged);
            Expect(result.Result.Success && await File.ReadAllTextAsync(Path.Combine(repo, ".git/diff-right")) == "working\n", "Staged external diff excludes later working content");
            await Write(repo, "new.txt", "new\n"); result = await Act(repo, WorkingCopyAction.ExternalDiff, entry => entry.Path == "new.txt");
            Expect(result.Result.Success && await File.ReadAllTextAsync(Path.Combine(repo, ".git/diff-right")) == "new\n", "Untracked external diff treats differences as success");
            await Write(repo, ".git/diff-test.sh", "#!/bin/sh\nexit 7\n"); result = await Act(repo, WorkingCopyAction.ExternalDiff, entry => entry.Path == "a.txt" && entry.Group == ChangeGroup.Staged); Expect(!result.Result.Success, "External error not false success");
            await Git(repo, "config", "--unset", "diff.tool"); await Reject("FileDiffToolMissing", () => Act(repo, WorkingCopyAction.ExternalDiff, entry => entry.Path == "a.txt" && entry.Group == ChangeGroup.Staged));
        });

        await Check("File history follows renames, pages beyond 200 and blame excludes working edits", async () =>
        {
            var repo = await Repo("history", false);
            using (var importer = new Process { StartInfo = new(git.Executable) { WorkingDirectory = repo, UseShellExecute = false, CreateNoWindow = true,
                RedirectStandardInput = true, RedirectStandardOutput = true, RedirectStandardError = true, StandardInputEncoding = new UTF8Encoding(false), ArgumentList = { "fast-import", "--quiet" } } })
            {
                importer.Start(); var output = importer.StandardOutput.ReadToEndAsync(); var error = importer.StandardError.ReadToEndAsync();
                for (var i = 1; i <= 205; i++)
                {
                    var message = $"File revision {i}\n"; var content = $"line {i}\n";
                    await importer.StandardInput.WriteAsync($"commit refs/heads/main\nmark :{i}\ncommitter Author <a@example.invalid> {1700000000 + i} +0000\ndata {Encoding.UTF8.GetByteCount(message)}\n{message}");
                    if (i > 1) await importer.StandardInput.WriteAsync($"from :{i - 1}\n");
                    await importer.StandardInput.WriteAsync($"M 100644 inline old.txt\ndata {Encoding.UTF8.GetByteCount(content)}\n{content}\n");
                }
                importer.StandardInput.Close(); await importer.WaitForExitAsync().WaitAsync(TimeSpan.FromSeconds(15)); await Task.WhenAll(output, error); Expect(importer.ExitCode == 0, "File fixture import");
            }
            await Git(repo, "reset", "--hard", "HEAD"); await Git(repo, "mv", "old.txt", "new.txt"); await Git(repo, "commit", "-m", "rename"); await Write(repo, "new.txt", "uncommitted\n");
            var context = await Context(repo); var entry = context.Changes.Entries.Single(); var first = await actions.ReadFileHistoryAsync(context, entry, 0, default); var next = await actions.ReadFileHistoryAsync(context, entry, 200, default, first.Next);
            Expect(first.Commits.Count == 200 && first.HasMore && next.Commits.Count == 6 && !next.HasMore && first.Commits.Select(row => row.Hash).Intersect(next.Commits.Select(row => row.Hash)).Count() == 0, "Complete rename-following pages without duplicate/truncation");
            var blame = await actions.ReadFileBlameAsync(context, entry, default); Expect(blame.Single().Text == "line 205" && blame[0].Email == "a@example.invalid", "Actual committed line author and content");
            var vm = new FileHistoryViewModel(context, entry, actions, reader, false); var calls = runner.Calls; var watch = Stopwatch.StartNew(); var window = new FileHistoryWindow(vm);
            Arrange((FrameworkElement)window.Content, 1200, 760); await Dispatcher.Yield(DispatcherPriority.ContextIdle);
            Expect(runner.Calls == calls, "History window construction/layout has no Git prerequisite"); Console.WriteLine($"File history window cached construction/layout: {watch.Elapsed.TotalMilliseconds:F0} ms; zero Git calls");
            await vm.StartAsync(); await DrainFiles(vm);
            Expect(vm.History.Count == 200 && (vm.Details.Contains("File revision", StringComparison.Ordinal) || vm.Details.Contains("rename", StringComparison.Ordinal)), "Bound history/full message");
            Arrange((FrameworkElement)window.Content, 1200, 760); SaveRender((FrameworkElement)window.Content, "file-history-normal.png", 1200, 760);
            Arrange((FrameworkElement)window.Content, 900, 580); SaveRender((FrameworkElement)window.Content, "file-history-minimum.png", 900, 580);
            vm.Mode = 1; await vm.Pending; Expect(vm.Blame.Count == 1, "Bound blame tab"); Arrange((FrameworkElement)window.Content, 1200, 760); SaveRender((FrameworkElement)window.Content, "file-blame.png", 1200, 760);
            await vm.CloseAsync(); window.Close();
        });
        await Check("Discard confirmation is target-specific, cancel starts no Git, and bound submit performs the change", async () =>
        {
            var repo = await Repo("confirmation"); await Write(repo, "a.txt", "work\n"); await Write(repo, "b.txt", "keep\n");
            var ui = new FileActionInteraction(); var model = new CommitViewModel(repo, git, reader, actions, ui); await model.StartLoading(); await model.DiffTask;
            model.Select(false, [model.Unstaged.Single(row => row.Change.Path == "a.txt")]); await model.DiffTask;
            using var menu = model.CreateFileMenu(false); var calls = runner.Calls; menu.DiscardCommand.Execute(null);
            Expect(runner.Calls == calls && ui.Prompt?.Targets.Count == 1 && ui.Prompt.Targets[0].Contains("a.txt", StringComparison.Ordinal), "Declined prompt targets only selection with zero Git");
            var prompt = new FileActionWindow(ui.Prompt!); Arrange((FrameworkElement)prompt.Content, 648, 430); SaveRender((FrameworkElement)prompt.Content, "file-discard-confirmation.png", 648, 430); prompt.Close();
            ui.Response = ""; menu.DiscardCommand.Execute(null); await model.OperationTask; await model.LoadingTask; await model.DiffTask;
            Expect(await File.ReadAllTextAsync(Path.Combine(repo, "a.txt")) == "base\n" && await File.ReadAllTextAsync(Path.Combine(repo, "b.txt")) == "keep\n", "Bound confirmed discard actual file result");
            await model.CloseAsync();
        });
        await Check("A refresh during confirmation rejects the old targets without executing Discard", async () =>
        {
            var repo = await Repo("confirmation race"); await Write(repo, "a.txt", "keep\n"); var ui = new FileActionInteraction { Response = "" };
            var model = new CommitViewModel(repo, git, reader, actions, ui); await model.StartLoading(); model.Select(false, [model.Unstaged.Single()]); await model.DiffTask;
            ui.DuringPrompt = () =>
            {
                var pending = model.StartLoading(); var frame = new DispatcherFrame();
                async Task Finish() { await pending; frame.Continue = false; } var completion = Finish(); Dispatcher.PushFrame(frame); completion.GetAwaiter().GetResult();
            };
            using var menu = model.CreateFileMenu(false); menu.DiscardCommand.Execute(null);
            Expect(model.Status == UiText.Get("FileMenuStale") && await File.ReadAllTextAsync(Path.Combine(repo, "a.txt")) == "keep\n", "No retarget after pumped confirmation"); await model.CloseAsync();
        });
        await Check("File history failures retain rows; cancelled old modes and closed reads cannot publish", async () =>
        {
            var repo = await Repo("history cancellation"); await Write(repo, "a.txt", "edit\n"); var context = await Context(repo);
            var hook = new FileActionRunner(runner); var observedReader = new RepositoryReader(hook);
            var model = new FileHistoryViewModel(context, context.Changes.Entries.Single(), new(hook, observedReader, identity), observedReader, false);
            await model.StartAsync(); var retained = model.History; hook.FailLog = true; model.RefreshCommand.Execute(null); await model.Pending;
            Expect(ReferenceEquals(retained, model.History) && model.Status == UiText.Get("ActionReadFailed"), "Failed refresh keeps real rows"); hook.FailLog = false;
            var arrived = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously); var release = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
            hook.AfterLog = async () => { hook.AfterLog = null; arrived.SetResult(); await release.Task; };
            model.RefreshCommand.Execute(null); var old = model.Pending; await arrived.Task.WaitAsync(TimeSpan.FromSeconds(10)); model.Mode = 1;
            release.SetResult(); await old; await model.Pending; Expect(model.Blame.Count == 1 && model.Status == UiText.Format("FileBlameCount", 1), "Cancelled history cannot overwrite newer blame mode");
            arrived = new(TaskCreationOptions.RunContinuationsAsynchronously); release = new(TaskCreationOptions.RunContinuationsAsynchronously);
            hook.AfterLog = async () => { hook.AfterLog = null; arrived.SetResult(); await release.Task; };
            model.Mode = 0; await arrived.Task.WaitAsync(TimeSpan.FromSeconds(10)); var close = model.CloseAsync(); release.SetResult(); await close;
            model.CancelCommand.Execute(null); await model.CloseAsync();
            Expect(ReferenceEquals(retained, model.History) && !model.RefreshCommand.CanExecute(null) && !model.CancelCommand.CanExecute(null), "Close drains read without late replacement or disposed-token access");
        });
        await Check("Selected stash cancellation after saving reports retained index and never retries or drops", async () =>
        {
            var repo = await Repo("stash cancelled"); await Write(repo, "a.txt", "saved\n"); await Git(repo, "add", "a.txt"); await Git(repo, "update-index", "--split-index");
            var context = await Context(repo); using var cancel = new CancellationTokenSource();
            var hook = new FileActionRunner(runner) { AfterStash = () => { cancel.Cancel(); return Task.CompletedTask; } };
            var observed = new GitActions(hook, new RepositoryReader(hook), identity);
            var result = await observed.ExecuteWorkingCopyAsync(context, new(WorkingCopyAction.StashFiles, context.Changes.Entries, "cancel fixture"), cancel.Token);
            Expect(result.Result.MessageCode == "FileStashIndexRetained" && hook.StashCalls == 1 && (await Git(repo, "stash", "list", "--format=%H")).Length == 40, "Saved stash retained and partial result explained");
            Expect(await Git(repo, "show", ":a.txt") == "saved" && await Git(repo, "show", "stash@{0}:a.txt") == "saved", "No index reset after cancellation; saved content retained");
            Expect(!Directory.EnumerateFiles(Path.Combine(repo, ".git"), "shovel-index-*").Any(), "Private split-index files cleaned only after process completion");
        });
        await Check("File history explicitly follows first-parent merge lineage; bound menu inspection, patch and stash work", async () =>
        {
            var repo = await Repo("bound actions"); await Git(repo, "switch", "-c", "side"); await Write(repo, "a.txt", "side\n"); await Git(repo, "commit", "-am", "side change");
            await Git(repo, "switch", "main"); await Write(repo, "b.txt", "main\n"); await Git(repo, "commit", "-am", "main change"); await Git(repo, "merge", "--no-ff", "side", "-m", "merge side"); await Write(repo, "a.txt", "working\n");
            var ui = new FileActionInteraction { PatchFile = Path.Combine(_root, "bound.patch") }; var hook = new FileActionRunner(runner); var observedReader = new RepositoryReader(hook);
            var model = new CommitViewModel(repo, git, observedReader, new(hook, observedReader, identity), ui);
            await model.StartLoading(); model.Select(false, [model.Unstaged.Single()]); await model.DiffTask;
            using var menu = model.CreateFileMenu(false); var calls = runner.Calls; menu.HistoryCommand.Execute(null);
            Expect(ui.History is not null && runner.Calls == calls, "Bound history opens without a Git prerequisite"); await ui.History!.StartAsync();
            Expect(ui.History.History.Count == 2 && ui.History.History[0].Commit.Parents.Count == 2 && ui.History.History.All(row => row.Subject != "side change"), "Merge integration shown in explicit first-parent scope");
            await ui.History.CloseAsync();
            var arrived = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously); var release = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
            hook.AfterDiff = async () => { hook.AfterDiff = null; arrived.SetResult(); await release.Task; };
            menu.PatchCommand.Execute(null); await arrived.Task.WaitAsync(TimeSpan.FromSeconds(10));
            Expect(model.RefreshLabel == UiText.Get("ActionCancel") && !model.Ready, "Patch export exposes cancellation and prevents competing writes");
            model.RefreshCommand.Execute(null); release.SetResult(); await model.FileTask; Expect(!File.Exists(ui.PatchFile), "Bound Cancel prevents patch publication");
            menu.PatchCommand.Execute(null); await model.FileTask;
            Expect(File.Exists(ui.PatchFile) && (await File.ReadAllTextAsync(ui.PatchFile)).Contains("+working", StringComparison.Ordinal), "Bound patch exports selected real changes");
            ui.Response = "from menu"; menu.StashCommand.Execute(null); await model.OperationTask; await model.LoadingTask; await model.DiffTask;
            Expect((await Git(repo, "stash", "list")).Contains("from menu", StringComparison.Ordinal) && await File.ReadAllTextAsync(Path.Combine(repo, "a.txt")) == "side\n", "Bound selected stash executes and reconciles"); await model.CloseAsync();
        });
        async Task DrainFiles(FileHistoryViewModel vm)
        { if (vm.Files is not { } files) return; await files.Pending; await files.Pending; }
    }
    private sealed class FileActionInteraction : IUserInteraction
    {
        public FileActionPrompt? Prompt; public string? Response, PatchFile; public Action? DuringPrompt; public FileHistoryViewModel? History;
        public string? ChooseRepository() => null;
        public string? ChooseGit() => null;
        public string? ConfirmFileAction(FileActionPrompt prompt) { Prompt = prompt; DuringPrompt?.Invoke(); return Response; }
        public string? ChoosePatchFile() => PatchFile;
        public void ShowFileHistory(FileHistoryViewModel model) => History = model;
    }
    private sealed class FileActionRunner(IProcessRunner inner) : IProcessRunner
    {
        public bool FailLog; public Func<Task>? AfterLog, AfterStash, AfterDiff; public int StashCalls;
        public async Task<ProcessResult> RunAsync(ProcessRequest request, CancellationToken token, Action<ProcessOutput>? progress = null)
        {
            if (FailLog && request.Arguments.Contains("log")) throw new IOException();
            var result = await inner.RunAsync(request, token, progress);
            if (request.Arguments.Contains("log") && AfterLog is not null) await AfterLog();
            if (request.Arguments.Contains("diff") && AfterDiff is not null) await AfterDiff();
            if (request.Arguments.Contains("stash") && request.Arguments.Contains("push")) { StashCalls++; if (AfterStash is not null) await AfterStash(); }
            return result;
        }
    }
}
