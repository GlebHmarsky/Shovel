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
    private static async Task RunStashChecksAsync()
    {
        var runner = new WindowsProcessRunner(); var reader = new RepositoryReader(runner); var identities = new WindowsRepositoryIdentity();
        var git = await reader.DiscoverAsync(null, default); var actions = new GitActions(runner, reader, identities);
        async Task<string> Git(string repo, params string[] args)
        {
            var result = await runner.RunAsync(new(git.Executable, repo, args), default);
            Expect(result.ExitCode == 0, "Fixture Git " + string.Join(' ', args) + ": " + Encoding.UTF8.GetString(result.StandardError));
            return Encoding.UTF8.GetString(result.StandardOutput).Trim();
        }
        async Task<string> Repo(string name, bool unborn = false)
        {
            var repo = Fixture("stash " + name); await Git(repo, "init", "-b", "main");
            await Git(repo, "config", "user.name", "Stash Fixture"); await Git(repo, "config", "user.email", "stash@example.invalid");
            if (!unborn) { await File.WriteAllTextAsync(Path.Combine(repo, "file.txt"), "base\n"); await Git(repo, "add", "."); await Git(repo, "commit", "-m", "base"); }
            return repo;
        }
        async Task<ActionContext> Context(string repo)
        {
            var snapshot = await reader.InspectAsync(git, repo, default, includeHistory: false);
            return new(git, await identities.ReadAsync(repo, default), snapshot, new(snapshot, HistoryScope.AllCommits, "", [], false), [], "");
        }
        async Task<StashList> List(string repo) => await actions.ReadStashesAsync(git, await identities.ReadAsync(repo, default), default);
        async Task<ActionResult> Execute(string repo, StashAction action, bool untracked = false, bool index = false)
        {
            var list = await List(repo);
            return await actions.ExecuteStashAsync(await Context(repo), new(action, list, list.Entries.FirstOrDefault(), "fixture 日本語 --message", untracked, index), default);
        }
        async Task Reject(string code, Func<Task> run)
        {
            try { await run(); throw new InvalidOperationException("Expected " + code); }
            catch (GitActionException error) { Expect(error.Code == code, "Expected " + code + " got " + error.Code); }
        }
        await Check("Stash create preserves staged/unstaged, optional untracked and ignored files; inspection compares saved objects", async () =>
        {
            var repo = await Repo("inspection 日本語");
            await File.WriteAllTextAsync(Path.Combine(repo, "file.txt"), "staged\n"); await Git(repo, "add", ".");
            await File.AppendAllTextAsync(Path.Combine(repo, "file.txt"), "unstaged\n");
            await File.WriteAllBytesAsync(Path.Combine(repo, "asset.bin"), [0, 1, 2, 3]);
            await File.WriteAllTextAsync(Path.Combine(repo, ".git", "info", "exclude"), "ignored.tmp\n");
            await File.WriteAllTextAsync(Path.Combine(repo, "ignored.tmp"), "keep ignored");
            Expect((await Execute(repo, StashAction.Create, untracked: true)).Success, "Create stash through app");
            var list = await List(repo); Expect(list.Entries.Count == 1 && list.Entries[0].Message.Contains("日本語"), "Named stash listed");
            Expect(await File.ReadAllTextAsync(Path.Combine(repo, "file.txt")) == "base\n" && !File.Exists(Path.Combine(repo, "asset.bin")) &&
                await File.ReadAllTextAsync(Path.Combine(repo, "ignored.tmp")) == "keep ignored", "Saved content cleaned; ignored untouched");
            var data = await actions.InspectStashAsync(git, await identities.ReadAsync(repo, default), list.Entries[0].Hash, default);
            var staged = await reader.ReadCommitFilesAsync(git, repo, data.Index.Hash, data.Index.Parents[0], default);
            var stagedDiff = await reader.ReadCommitFileDiffAsync(git, repo, staged, staged.Files.Single(), default);
            Expect(stagedDiff.RawPatch.Contains("+staged") && !stagedDiff.RawPatch.Contains("+unstaged"), "Saved index separate from saved worktree");
            var files = await reader.ReadCommitFilesAsync(git, repo, data.Worktree.Hash, data.Worktree.Parents[1], default);
            Expect((await reader.ReadCommitFileDiffAsync(git, repo, files, files.Files.Single(), default)).RawPatch.Contains("+unstaged"), "Unstaged delta inspected");
            var loose = await reader.ReadCommitFilesAsync(git, repo, data.Untracked!.Hash, null, default);
            Expect((await reader.ReadCommitFileDiffAsync(git, repo, loose, loose.Files.Single(), default)).NewBytes == 4, "Binary saved-untracked content inspected");
            Expect((await Execute(repo, StashAction.Apply, index: true)).Success && (await List(repo)).Entries.Count == 1, "Apply retains stash");
            Expect(await Git(repo, "show", ":file.txt") == "staged" && await File.ReadAllTextAsync(Path.Combine(repo, "file.txt")) == "staged\nunstaged\n", "Restore index and worktree separately");
        });
        await Check("Default stash excludes untracked; Pop removes only successfully applied entry and Drop preserves worktree", async () =>
        {
            var repo = await Repo("pop drop"); await File.WriteAllTextAsync(Path.Combine(repo, "file.txt"), "saved\n");
            await File.WriteAllTextAsync(Path.Combine(repo, "loose.txt"), "keep");
            Expect((await Execute(repo, StashAction.Create)).Success && File.Exists(Path.Combine(repo, "loose.txt")), "Default leaves untracked");
            Expect((await Execute(repo, StashAction.Pop)).MessageCode == "StashPopped" && (await List(repo)).Entries.Count == 0, "Pop really removes entry");
            Expect(await File.ReadAllTextAsync(Path.Combine(repo, "file.txt")) == "saved\n", "Applied bytes present");
            await Execute(repo, StashAction.Create); var before = await Git(repo, "status", "--porcelain");
            Expect((await Execute(repo, StashAction.Drop)).Success && (await List(repo)).Entries.Count == 0 && before == await Git(repo, "status", "--porcelain"), "Drop affects only stash reference/log");
        });
        await Check("Conflicted Pop retains stash, conflict files and unmerged index without a fake recovery sequence", async () =>
        {
            var repo = await Repo("conflict"); await File.WriteAllTextAsync(Path.Combine(repo, "file.txt"), "stash line\n"); await Execute(repo, StashAction.Create);
            var saved = (await List(repo)).Entries[0].Hash;
            await File.WriteAllTextAsync(Path.Combine(repo, "file.txt"), "committed line\n"); await Git(repo, "add", "."); await Git(repo, "commit", "-m", "conflicting tip");
            var result = await Execute(repo, StashAction.Pop);
            Expect(!result.Success && result.HasConflicts && result.MessageCode == "StashApplyFailed", "Conflict reported");
            Expect((await List(repo)).Entries.Single().Hash == saved && (await Git(repo, "ls-files", "-u")).Length > 0 &&
                (await Context(repo)).Repository.Operation == RepositoryOperation.None, "Preserve stash and conflict, no Continue/Abort fabricated");
        });
        await Check("Untracked collision fails Pop without removing stash or overwriting existing file", async () =>
        {
            var repo = await Repo("collision"); await File.WriteAllTextAsync(Path.Combine(repo, "loose.txt"), "saved"); await Execute(repo, StashAction.Create, untracked: true);
            await File.WriteAllTextAsync(Path.Combine(repo, "loose.txt"), "existing"); var result = await Execute(repo, StashAction.Pop);
            Expect(!result.Success && (await List(repo)).Entries.Count == 1 && await File.ReadAllTextAsync(Path.Combine(repo, "loose.txt")) == "existing", "Collision stays recoverable");
        });
        await Check("Stash stale ordinal, replaced identity, unborn and cancelled mutations refuse safely", async () =>
        {
            var repo = await Repo("stale"); await File.WriteAllTextAsync(Path.Combine(repo, "file.txt"), "first"); await Execute(repo, StashAction.Create);
            var context = await Context(repo); var old = await List(repo);
            await File.WriteAllTextAsync(Path.Combine(repo, "file.txt"), "second"); await Execute(repo, StashAction.Create);
            await Reject("StashChanged", () => actions.ExecuteStashAsync(context, new(StashAction.Drop, old, old.Entries[0]), default));
            await Reject("ActionIdentityChanged", () => actions.ExecuteStashAsync(context with { Identity = context.Identity with { Fingerprint = "changed" } }, new(StashAction.Create, old), default));
            var unborn = await Repo("unborn", true); await Reject("StashNeedsCommit", () => Execute(unborn, StashAction.Create));
            using var cancel = new CancellationTokenSource(); cancel.Cancel();
            await ExpectException<OperationCanceledException>(() => actions.ExecuteStashAsync(context, new(StashAction.Drop, old, old.Entries[0]), cancel.Token));
            Expect((await List(repo)).Entries.Count == 2, "No entry removed");
        });
        await Check("Pop never reapplies or drops shifted entry after concurrent stash creation", async () =>
        {
            var repo = await Repo("pop race"); await File.WriteAllTextAsync(Path.Combine(repo, "file.txt"), "other stash"); await Execute(repo, StashAction.Create);
            var other = (await List(repo)).Entries[0].Hash; await Execute(repo, StashAction.Drop);
            await File.WriteAllTextAsync(Path.Combine(repo, "file.txt"), "saved"); await Execute(repo, StashAction.Create);
            var list = await List(repo); var hook = new StashRunner(runner);
            hook.AfterApply = () => Git(repo, "stash", "store", "-m", "other client", other);
            var observed = new GitActions(hook, new RepositoryReader(hook), identities);
            var result = await observed.ExecuteStashAsync(await Context(repo), new(StashAction.Pop, list, list.Entries[0]), default);
            Expect(result.MessageCode == "StashAppliedRetained" && (await List(repo)).Entries.Count == 2 && hook.ApplyCalls == 1, "Partial success explicit; no repeat apply or wrong drop");
        });
        await Check("Stash empty/locked outcomes preserve files and linked worktrees share saved entries", async () =>
        {
            var repo = await Repo("linked and locked"); Expect((await Execute(repo, StashAction.Create)).Success && (await List(repo)).Entries.Count == 0, "Clean worktree has no fabricated stash");
            await File.WriteAllTextAsync(Path.Combine(repo, "file.txt"), "saved\n"); await Execute(repo, StashAction.Create);
            var linked = Path.Combine(_root, "stash linked worktree"); await Git(repo, "worktree", "add", "-b", "other", linked);
            Expect((await List(linked)).Entries.Single().Hash == (await List(repo)).Entries.Single().Hash, "Shared stash log exposed from linked worktree");
            Expect((await Execute(linked, StashAction.Apply)).Success && await File.ReadAllTextAsync(Path.Combine(linked, "file.txt")) == "saved\n" &&
                await File.ReadAllTextAsync(Path.Combine(repo, "file.txt")) == "base\n", "Apply writes only selected worktree");
            await File.WriteAllTextAsync(Path.Combine(repo, "file.txt"), "locked data\n");
            var lockFile = Path.Combine(repo, ".git", "index.lock"); await File.WriteAllTextAsync(lockFile, "fixture lock");
            var result = await Execute(repo, StashAction.Create);
            Expect(!result.Success && File.Exists(lockFile) && await File.ReadAllTextAsync(Path.Combine(repo, "file.txt")) == "locked data\n" &&
                (await List(repo)).Entries.Count == 1, "Failed create preserves lock, file and existing stash");
        });
        await Check("Stash window opens cached with zero Git calls; bound create/apply/drop, retained failures and layouts", async () =>
        {
            var repo = await Repo("view"); await File.WriteAllTextAsync(Path.Combine(repo, "file.txt"), "saved\n"); await Execute(repo, StashAction.Create);
            var hook = new StashRunner(runner); var observedReader = new RepositoryReader(hook); var observed = new GitActions(hook, observedReader, identities);
            var cache = new StashCache { List = await List(repo), Message = "retained draft" }; var interaction = new StashInteraction();
            var model = new StashViewModel(await Context(repo), observed, observedReader, interaction, cache, _ => { });
            var watch = Stopwatch.StartNew(); var window = new StashWindow(model); var content = (FrameworkElement)window.Content;
            Arrange(content, 1200, 760);
            Expect(hook.Calls == 0 && model.Rows.Count == 1 && model.Message == "retained draft", "No construction-time Git, cached list/draft usable");
            Console.WriteLine($"Stash cached construction/layout: {watch.Elapsed.TotalMilliseconds:F0} ms, zero Git calls");
            try
            {
                await model.StartAsync(); await model.WaitForIdleAsync();
                Expect(model.Files?.Files.Count == 1 && model.Files.Lines.Count > 0, "Real selected file/diff loaded");
                Arrange(content, 1200, 760); SaveRender(content, "stashes.png", 1200, 760);
                Arrange(content, 900, 580); SaveRender(content, "stashes-minimum.png", 900, 580);
                var calls = hook.Calls; model.DropCommand.Execute(null); Expect(hook.Calls == calls && interaction.Message.Contains(repo) && interaction.Message.Contains(cache.List!.Entries[0].Hash), "Declined actual-target Drop makes no calls");
                model.ApplyCommand.Execute(null); await model.Running; Expect(model.Action?.Succeeded == true && model.Rows.Count == 1, "Bound apply works");
                model.CreateCommand.Execute(null); await model.Running; Expect(model.Action?.Succeeded == true && model.Rows.Count == 2, "Bound create works");
                interaction.Confirm = true; model.DropCommand.Execute(null); await model.Running; Expect(model.Rows.Count == 1, "Bound confirmed drop works");
                hook.FailList = true; await model.RefreshAsync(); Expect(model.Rows.Count == 1 && !model.ApplyCommand.CanExecute(null), "Failed refresh retains rows and disallows stale selection writes");
            }
            finally { await model.CloseAsync(); window.Close(); }
            var samples = new List<double>();
            for (var i = 0; i < 5; i++)
            {
                var warm = new StashViewModel(await Context(repo), observed, observedReader, interaction, cache, _ => { });
                var calls = hook.Calls; watch.Restart(); var warmedWindow = new StashWindow(warm); Arrange((FrameworkElement)warmedWindow.Content, 1200, 760);
                samples.Add(watch.Elapsed.TotalMilliseconds); Expect(hook.Calls == calls, "Warm opening must not query Git");
                await warm.CloseAsync(); warmedWindow.Close();
            }
            Console.WriteLine("Stash warm cached construction/layout ms (five samples): " + string.Join(", ", samples.Select(ms => ms.ToString("F0"))));
        });
        await Check("Pop cancellation after apply retains stash; unchecked index restoration overrides Git config", async () =>
        {
            var repo = await Repo("cancel after apply"); await File.WriteAllTextAsync(Path.Combine(repo, "file.txt"), "saved\n"); await Git(repo, "add", ".");
            await Execute(repo, StashAction.Create); await Git(repo, "config", "stash.index", "true");
            var list = await List(repo); using var cancel = new CancellationTokenSource();
            var hook = new StashRunner(runner) { AfterApply = () => { cancel.Cancel(); return Task.CompletedTask; } };
            var observed = new GitActions(hook, new RepositoryReader(hook), identities);
            var result = await observed.ExecuteStashAsync(await Context(repo), new(StashAction.Pop, list, list.Entries[0]), cancel.Token);
            Expect(result.MessageCode == "StashAppliedRetained" && (await List(repo)).Entries.Count == 1 && hook.ApplyCalls == 1, "Cancelled final step retains applied entry");
            Expect(await File.ReadAllTextAsync(Path.Combine(repo, "file.txt")) == "saved\n" && await Git(repo, "diff", "--cached", "--name-only") == "", "Unchecked restoration leaves worktree changes unstaged despite config");
        });
        await Check("Stash cancelled reads never publish over newer selection or after closure", async () =>
        {
            var repo = await Repo("late reads"); await File.WriteAllTextAsync(Path.Combine(repo, "file.txt"), "first\n"); await Execute(repo, StashAction.Create);
            var hook = new StashRunner(runner); var observedReader = new RepositoryReader(hook); var observed = new GitActions(hook, observedReader, identities);
            var cache = new StashCache { List = await List(repo) };
            var model = new StashViewModel(await Context(repo), observed, observedReader, new StashInteraction(), cache, _ => { });
            var arrived = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously); var release = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
            hook.AfterList = async () => { hook.AfterList = null; arrived.TrySetResult(); await release.Task; };
            var oldRead = model.StartAsync(); await arrived.Task.WaitAsync(TimeSpan.FromSeconds(10));
            await File.WriteAllTextAsync(Path.Combine(repo, "file.txt"), "second\n"); await Execute(repo, StashAction.Create);
            var newRead = model.RefreshAsync(); release.SetResult(); await Task.WhenAll(oldRead, newRead); await model.WaitForIdleAsync();
            Expect(model.Rows.Count == 2 && model.Selected?.Entry.Hash == cache.List!.Entries.Last().Hash, "Retain selected hash after new entry, reject late cancelled list");
            model.Selected = model.Rows[0]; model.Selected = null; await model.WaitForIdleAsync();
            Expect(model.Files is null && model.Comparisons.Count == 0 && model.Details == "", "Late detail cannot replace empty selection");
            arrived = new(TaskCreationOptions.RunContinuationsAsynchronously); release = new(TaskCreationOptions.RunContinuationsAsynchronously);
            hook.AfterList = async () => { hook.AfterList = null; arrived.TrySetResult(); await release.Task; };
            var closingRead = model.RefreshAsync(); await arrived.Task.WaitAsync(TimeSpan.FromSeconds(10)); var close = model.CloseAsync();
            release.SetResult(); await close; await closingRead;
            model.CancelCommand.Execute(null); await model.CloseAsync();
            Expect(!model.CreateCommand.CanExecute(null) && model.Files is null, "Close drains late read and prevents further writes");
        });
        await Check("Shell opens Stashes without preparation and refreshes history after a stash mutation", async () =>
        {
            var repo = await Repo("shell"); await File.WriteAllTextAsync(Path.Combine(repo, "file.txt"), "saved\n");
            var hook = new StashRunner(runner); var observedReader = new RepositoryReader(hook); var interaction = new StashInteraction();
            var shell = new ShellViewModel(observedReader, interaction, identities, actions: new GitActions(hook, observedReader, identities));
            shell.CompleteStartup(new(new() { RepositoryPaths = [repo], SelectedRepositoryPath = repo }, SettingsLoadStatus.Loaded)); await shell.WaitForIdleAsync();
            var calls = hook.Calls;
            interaction.Show = async model =>
            {
                Expect(hook.Calls == calls, "No opening-time queries before interaction");
                model.CreateCommand.Execute(null); await model.Running; Expect(model.Action?.Succeeded == true && shell.HasAction, "Shell owns top-center stash action");
            };
            shell.StashesCommand.Execute(null); await shell.StashDialogTask; await shell.WaitForIdleAsync();
            Expect(!shell.ActionOpen && shell.CanNavigate && (await List(repo)).Entries.Count == 1 && await Git(repo, "status", "--porcelain") == "", "Mutation and shell reinspection completed");
            shell.BeginShutdown(); await shell.WaitForIdleAsync();
        });
    }
    private sealed class StashInteraction : IUserInteraction
    {
        public bool Confirm; public string Message = ""; public Func<StashViewModel, Task>? Show;
        public string? ChooseRepository() => null; public string? ChooseGit() => null;
        public bool ConfirmRecovery(string message, string title) { Message = message; return Confirm; }
        public Task ShowStashesAsync(StashViewModel model) => Show?.Invoke(model) ?? Task.CompletedTask;
    }
    private sealed class StashRunner(IProcessRunner inner) : IProcessRunner
    {
        public int Calls, ApplyCalls; public bool FailList; public Func<Task>? AfterApply, AfterList;
        public async Task<ProcessResult> RunAsync(ProcessRequest request, CancellationToken token, Action<ProcessOutput>? progress = null)
        {
            Interlocked.Increment(ref Calls);
            if (FailList && request.Arguments.Contains("stash") && request.Arguments.Contains("list")) throw new IOException();
            var result = await inner.RunAsync(request, token, progress);
            if (request.Arguments.Contains("stash") && request.Arguments.Contains("list") && AfterList is not null) await AfterList();
            if (request.Arguments.Contains("stash") && request.Arguments.Contains("apply")) { ApplyCalls++; if (AfterApply is not null) await AfterApply(); }
            return result;
        }
    }
}
