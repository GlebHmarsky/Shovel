using System.Diagnostics;
using System.IO;
using System.Text;
using System.Text.Json;
using GitGui.App.ViewModels;
using GitGui.Core.Drafts;
using GitGui.Core.Git;
using GitGui.Core.Processes;
using GitGui.Core.Settings;
using GitGui.ProcessHost;

namespace GitGui.FoundationChecks;

internal static partial class Program
{
    private static bool _routine;

    // Independent of the WPF dispatcher: a blocked dispatcher cannot defeat this deadline.
    // Process exit closes WindowsProcessRunner's kill-on-close jobs and their owned children.
    private static System.Threading.Timer RoutineDeadline(TimeSpan time, string message) => new(_ =>
    {
        Console.Error.WriteLine(message);
        Environment.Exit(124);
    }, null, time, Timeout.InfiniteTimeSpan);

    private static int RunRoutineChecks()
    {
        _routine = true;
        var watch = Stopwatch.StartNew();
        _root = Path.Combine(Path.GetFullPath("artifacts/smoke-checks"), DateTime.UtcNow.ToString("yyyyMMdd-HHmmss") + "-" + Guid.NewGuid().ToString("N"));
        using var deadline = RoutineDeadline(TimeSpan.FromMinutes(3), "Routine suite exceeded 180 seconds; no preview may be published.");
        try
        {
            Directory.CreateDirectory(_root);
            RunWpfChecks(); // One existing resource/layout smoke check; never production App startup.
            RunViewModelChecks(routineOnly: true);
            Expect(System.Windows.Application.Current is HeadlessApplication { StartupSuppressed: true }, "No production startup");
            Expect(!System.Windows.Application.Current.Windows.OfType<System.Windows.Window>().Any(w => w.IsVisible), "No visible windows");
            Console.WriteLine($"{_passed} passed; {_failed} failed; routine elapsed {watch.Elapsed.TotalSeconds:F1} s. Fixtures: {_root}");
            return 0;
        }
        catch (Exception error)
        {
            Console.Error.WriteLine($"Routine verification stopped after {watch.Elapsed.TotalSeconds:F1} s: {error.Message}");
            return 1;
        }
    }

    // Small ordered smoke workflow; later steps reuse its tiny disposable repo and fail fast.
    // No loops over old suites, large histories, sleep-based races or hidden scenario matrices.
    private static async Task RunRoutineScenariosAsync()
    {
        await Check("Settings save and reload", async () =>
        {
            var store = new SettingsStore(Fixture("settings"));
            await store.LoadAsync(default);
            await store.SaveAsync(new() { WindowWidth = 1234 }, default);
            Expect((await store.LoadAsync(default)).Settings.WindowWidth == 1234, "Saved layout restored");
        });
        await Check("Invalid settings are preserved", async () =>
        {
            var store = new SettingsStore(Fixture("invalid-settings"));
            await File.WriteAllTextAsync(store.FilePath, "broken");
            Expect(!(await store.LoadAsync(default)).CanSave && await File.ReadAllTextAsync(store.FilePath) == "broken", "Do not overwrite invalid data");
        });
        var runner = new WindowsProcessRunner(); var reader = new RepositoryReader(runner);
        var identities = new WindowsRepositoryIdentity(); var actions = new GitActions(runner, reader, identities);
        await Check("Process arguments preserve literal Unicode and shell characters", async () =>
        {
            string[] values = ["space 日本語", "& $(not-a-command)", "\"quoted\""];
            var result = await runner.RunAsync(new(ProbeExecutable, _root, ["--probe", "arguments", .. values]), default);
            Expect(result.ExitCode == 0 && JsonSerializer.Deserialize<string[]>(result.StandardOutput)!.SequenceEqual(values), "Literal arguments");
        });
        await Check("Cancellation drains an owned process", async () =>
        {
            using var cancel = new CancellationTokenSource(TimeSpan.FromMilliseconds(150));
            var result = await runner.RunAsync(new(ProbeExecutable, _root, ["--probe", "wait"]), cancel.Token);
            Expect(result.Outcome == ProcessOutcome.Cancelled, "Cancelled process completes");
        });
        var git = await reader.DiscoverAsync(null, default);
        // Init intentionally rejects nested repositories. Keep all real-Git fixtures outside this checkout.
        var root = Path.Combine(Path.GetTempPath(), "Shovel-smoke-" + Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(root);
        await File.WriteAllTextAsync(Path.Combine(_root, "git-fixture.txt"), root);
        var repo = Path.Combine(root, "repo 日本語");
        async Task<string> Git(string path, params string[] args)
        {
            var result = await runner.RunAsync(new(git.Executable, path, args), default);
            Expect(result.ExitCode == 0, "Fixture command failed: " + Encoding.UTF8.GetString(result.StandardError));
            return Encoding.UTF8.GetString(result.StandardOutput).Trim();
        }
        Task<WorkingCopyContext> Read() => actions.ReadWorkingCopyAsync(git, repo, default);
        async Task Act(WorkingCopyAction action, string? message = null)
        {
            var context = await Read();
            var entries = context.Changes.Entries.Where(e => action == WorkingCopyAction.Unstage ? e.Group == ChangeGroup.Staged : e.Group != ChangeGroup.Staged).ToArray();
            Expect((await actions.ExecuteWorkingCopyAsync(context, new(action, entries, message ?? ""), default)).Result.Success, "Working-copy action succeeds");
        }
        await Check("Initialize a real repository with no implicit commit", async () =>
        {
            var result = await actions.InitializeRepositoryAsync(git, new(repo, "main"), default);
            Expect(result.Action.Success && result.Repository?.Commit is null, "Unborn repository");
            await Git(repo, "config", "user.name", "Smoke Fixture");
            await Git(repo, "config", "user.email", "smoke@example.invalid");
            await Git(repo, "config", "core.autocrlf", "false");
            await Git(repo, "config", "core.hooksPath", Fixture("empty-hooks"));
        });
        const string file = "-file 日本語.txt";
        var path = Path.Combine(repo, file);
        await Check("Stage a literal untracked file", async () =>
        {
            await File.WriteAllTextAsync(path, "base\n"); await Act(WorkingCopyAction.Stage);
            Expect((await Read()).Changes.Entries.Single().Group == ChangeGroup.Staged, "File enters index");
        });
        await Check("Unstage preserves newer working content", async () =>
        {
            await File.WriteAllTextAsync(path, "newer\n"); await Act(WorkingCopyAction.Unstage);
            Expect((await Read()).Changes.Entries.Single().Group == ChangeGroup.Untracked && await File.ReadAllTextAsync(path) == "newer\n", "Only index changed");
        });
        await Check("Commit records the staged message and content", async () =>
        {
            await Act(WorkingCopyAction.Stage); await Act(WorkingCopyAction.Commit, "Smoke 日本語\n\nBody");
            Expect((await Git(repo, "log", "-1", "--format=%B")).Contains("Body") && (await Read()).Changes.Entries.Count == 0, "Commit exists and status is clean");
        });
        await Check("All Commits reads real history and ref labels", async () =>
        {
            await Git(repo, "tag", "smoke-tag");
            var history = await reader.ReadInitialHistoryAsync(git, await reader.InspectAsync(git, repo, default), HistoryScope.AllCommits, default);
            Expect(history.Entries.Single().Commit.Message.Contains("Body") && history.Session.References.Any(r => r.Name == "refs/tags/smoke-tag"), "Message and refs visible");
        });
        await Check("Text diff reads an unstaged addition", async () =>
        {
            await File.AppendAllTextAsync(path, "addition\n"); var context = await Read();
            var diff = await reader.ReadFileDiffAsync(context, context.Changes.Entries.Single(), new(), default);
            Expect(diff.Lines.Any(l => l.Kind == DiffLineKind.Added), "Real diff line");
        });
        await Check("Binary diff reports bytes without decoding content", async () =>
        {
            var binary = Path.Combine(repo, "asset.bin"); await File.WriteAllBytesAsync(binary, [0, 255, 0, 1]);
            var context = await Read(); var diff = await reader.ReadFileDiffAsync(context, context.Changes.Entries.Single(e => e.Path == "asset.bin"), new(), default);
            Expect(diff.Kind == FileDiffKind.Binary && diff.NewBytes == 4, "Binary size");
            File.Delete(binary);
        });
        await Check("Existing index lock is retained on failed staging", async () =>
        {
            var lockPath = Path.Combine(repo, ".git", "index.lock"); await File.WriteAllTextAsync(lockPath, "owned by fixture");
            try
            {
                var context = await Read();
                try { var result = await actions.ExecuteWorkingCopyAsync(context, new(WorkingCopyAction.Stage, context.Changes.Entries), default); Expect(!result.Result.Success, "Locked write must fail"); }
                catch (GitActionException) { }
                Expect(await File.ReadAllTextAsync(lockPath) == "owned by fixture", "Foreign lock retained");
            }
            finally { File.Delete(lockPath); }
        });
        static BranchManagementOptions BranchOptions(ActionContext context, string name) =>
            new(BranchManagementAction.Delete, context.History.References.Single(r => r.Name == "refs/heads/" + name));
        await Check("Delete a merged local branch", async () =>
        {
            await Git(repo, "branch", "remove-me"); var context = await actions.PrepareAsync(git, repo, default);
            Expect((await actions.ExecuteBranchManagementAsync(context, BranchOptions(context, "remove-me"), default)).Success, "Merged branch removed");
            Expect(!(await Git(repo, "branch", "--list", "remove-me")).Any(), "Ref gone");
        });
        await Check("Refuse deleting the current branch", async () =>
        {
            var context = await actions.PrepareAsync(git, repo, default);
            try { await actions.ExecuteBranchManagementAsync(context, BranchOptions(context, "main"), default); throw new InvalidOperationException("Deletion unexpectedly allowed"); }
            catch (GitActionException e) { Expect(e.Code == "BranchManageCurrent", "Current branch protection"); }
        });
        var identity = await identities.ReadAsync(repo, default);
        var store = new CommitDraftStore(Fixture("drafts"), identities);
        await Check("Commit draft survives reopening its store", async () =>
        {
            await store.SaveAsync(repo, identity.Fingerprint, null, new("Draft 日本語", "Body"), default);
            Expect((await new CommitDraftStore(store.Location, identities).LoadAsync(repo, identity.Fingerprint, default)).Text.Subject == "Draft 日本語", "Durable draft");
        });
        await Check("Stale draft writer cannot overwrite another revision", async () =>
        {
            try { await store.SaveAsync(repo, identity.Fingerprint, null, new("stale", ""), default); throw new InvalidOperationException("Stale write allowed"); }
            catch (CommitDraftException e) { Expect(e.Failure == CommitDraftFailure.Changed, "Revision check"); }
        });
        async Task Stash(StashAction action)
        {
            var list = await actions.ReadStashesAsync(git, identity, default);
            Expect((await actions.ExecuteStashAsync(await actions.PrepareAsync(git, repo, default), new(action, list, list.Entries.FirstOrDefault(), "smoke"), default)).Success, "Stash command succeeds");
        }
        await Check("Create stash saves the pending edit", async () =>
        {
            await Stash(StashAction.Create);
            Expect((await Read()).Changes.Entries.Count == 0 && (await actions.ReadStashesAsync(git, identity, default)).Entries.Count == 1, "Saved and clean");
        });
        await Check("Pop stash restores its edit and removes the entry", async () =>
        {
            await Stash(StashAction.Pop);
            Expect((await File.ReadAllTextAsync(path)).Contains("addition") && (await actions.ReadStashesAsync(git, identity, default)).Entries.Count == 0, "Restored once");
            await Act(WorkingCopyAction.Stage); await Act(WorkingCopyAction.Commit, "Update");
        });
        var remote = Path.Combine(root, "remote.git");
        await Check("Push publishes to a disposable local bare remote", async () =>
        {
            await Git(root, "init", "--bare", remote); await Git(repo, "remote", "add", "origin", remote);
            Expect((await actions.ExecuteAsync(await actions.PrepareAsync(git, repo, default), new(GitAction.Push, Branch: "main", Remote: "origin", RemoteBranch: "main", SetUpstream: true), default)).Success, "Push succeeds");
            Expect(await Git(remote, "rev-parse", "refs/heads/main") == await Git(repo, "rev-parse", "HEAD"), "Remote tip matches");
        });
        var peer = Path.Combine(root, "peer");
        await Check("Fetch imports a peer tip without moving local HEAD", async () =>
        {
            await Git(root, "clone", "-b", "main", remote, peer);
            await Git(peer, "config", "user.name", "Peer"); await Git(peer, "config", "user.email", "peer@example.invalid");
            await File.WriteAllTextAsync(Path.Combine(peer, "peer.txt"), "from peer\n");
            await Git(peer, "add", "."); await Git(peer, "commit", "-m", "Peer"); await Git(peer, "push");
            var head = await Git(repo, "rev-parse", "HEAD");
            Expect((await actions.ExecuteAsync(await actions.PrepareAsync(git, repo, default), new(GitAction.Fetch, Remote: "origin"), default)).Success, "Fetch succeeds");
            Expect(await Git(repo, "rev-parse", "HEAD") == head && await Git(repo, "rev-parse", "origin/main") != head, "Fetch leaves local branch");
        });
        await Check("Pull imports the peer file", async () =>
        {
            Expect((await actions.ExecuteAsync(await actions.PrepareAsync(git, repo, default), new(GitAction.Pull, Remote: "origin", RemoteBranch: "main"), default)).Success, "Pull succeeds");
            Expect(await File.ReadAllTextAsync(Path.Combine(repo, "peer.txt")) == "from peer\n", "Pulled real content");
        });
        await Check("Abort recovers a real merge conflict", async () =>
        {
            await Git(repo, "checkout", "-b", "conflict"); await File.WriteAllTextAsync(path, "theirs\n"); await Git(repo, "commit", "-am", "Theirs");
            await Git(repo, "checkout", "main"); await File.WriteAllTextAsync(path, "ours\n"); await Git(repo, "commit", "-am", "Ours");
            var conflict = await runner.RunAsync(new(git.Executable, repo, ["merge", "conflict"]), default);
            Expect(conflict.ExitCode != 0, "Fixture must conflict");
            var context = await actions.PrepareAsync(git, repo, default);
            Expect(context.Repository.Operation == RepositoryOperation.Merge, "Conflict detected");
            Expect((await actions.ExecuteAsync(context, new(GitAction.Abort), default)).Success && await File.ReadAllTextAsync(path) == "ours\n", "Abort restores original content");
        });
        var first = Fixture("model-first"); var second = Fixture("model-second");
        var modelRunner = new ModelRunner(first) { UseRequestDirectory = true }; modelRunner.Release.TrySetResult();
        var model = new ShellViewModel(new(modelRunner), new ModelInteraction(first), new ModelIdentities());
        await Check("Startup restores selected tab and automatically requests history", async () =>
        {
            model.CompleteStartup(new(new() { RepositoryPaths = [first, second], SelectedRepositoryPath = second }, SettingsLoadStatus.Loaded));
            await model.WaitForIdleAsync();
            Expect(model.Commits.Count == 1 && model.SelectedRepository?.Directory == second && model.Repositories[0].Snapshot is null, "Only selected tab loads");
        });
        await Check("Warm tabs retain history without another Git request", async () =>
        {
            model.SelectedRepository = model.Repositories[0]; await model.WaitForIdleAsync(); var calls = modelRunner.Calls;
            model.SelectedRepository = model.Repositories[1]; await model.WaitForIdleAsync();
            Expect(model.Commits.Count == 1 && modelRunner.Calls == calls, "Cached rows reused");
            model.BeginShutdown(); await model.WaitForIdleAsync();
        });
        await Check("Obsolete history cannot appear in another tab", async () =>
        {
            var delayed = new ModelRunner(first) { HoldHistory = true, IgnoreHistoryCancellation = true }; delayed.Release.TrySetResult();
            var pending = CreateModel(delayed, first); pending.OpenRepositoryCommand.Execute(null);
            await delayed.HistoryStarted.Task.WaitAsync(TimeSpan.FromSeconds(5));
            pending.SelectedRepository = new RepositoryTab(second); delayed.ReleaseHistory.TrySetResult();
            await pending.WaitForIdleAsync();
            Expect(pending.Commits.Count == 0 && pending.SelectedCommit is null, "Captured tab generation respected");
            pending.BeginShutdown(); await pending.WaitForIdleAsync();
        });
    }
}
