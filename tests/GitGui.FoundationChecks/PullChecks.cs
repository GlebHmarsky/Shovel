using System.IO;
using System.Text;
using System.Windows;
using System.Windows.Threading;
using GitGui.App.Localization;
using GitGui.App.ViewModels;
using GitGui.App.Views;
using GitGui.Core.Git;
using GitGui.ProcessHost;

namespace GitGui.FoundationChecks;

internal static partial class Program
{
    private static async Task RunPullChecksAsync()
    {
        var git = _actionFixture!.Git; var runner = new WindowsProcessRunner();
        var reader = new RepositoryReader(runner);
        var actions = new GitActions(runner, reader, new WindowsRepositoryIdentity());
        async Task<string> Git(string repo, params string[] args)
        {
            var result = await runner.RunAsync(new(git.Executable, repo, args), default);
            Expect(result.ExitCode == 0, "Disposable pull fixture command: " + args[0]);
            return Encoding.UTF8.GetString(result.StandardOutput).Trim();
        }
        async Task<(string Repo, string Peer)> Create(string name)
        {
            var repo = Fixture(name); var remote = Fixture(name + " remote.git"); var peer = Fixture(name + " peer");
            await Git(remote, "init", "--bare", "-b", "main"); await Git(repo, "init", "-b", "main");
            await Git(repo, "config", "user.name", "Pull Fixture"); await Git(repo, "config", "user.email", "pull@example.invalid");
            await File.WriteAllTextAsync(Path.Combine(repo, "base.txt"), "base\n");
            await File.WriteAllTextAsync(Path.Combine(repo, "draft.txt"), "draft base\n");
            await Git(repo, "add", "."); await Git(repo, "commit", "-m", "Base");
            await Git(repo, "remote", "add", "origin", remote); await Git(repo, "push", "-u", "origin", "main");
            await Git(peer, "clone", remote, ".");
            await Git(peer, "config", "user.name", "Peer"); await Git(peer, "config", "user.email", "peer@example.invalid");
            return (repo, peer);
        }
        async Task CommitFile(string repo, string file, string content)
        {
            await File.WriteAllTextAsync(Path.Combine(repo, file), content);
            await Git(repo, "add", file); await Git(repo, "commit", "-m", file);
        }
        async Task<GitActionViewModel> Pull(string repo, PullStrategy strategy, bool stash = false)
        {
            var context = await actions.PrepareAsync(git, repo, default);
            var model = new GitActionViewModel(context, GitAction.Pull, actions,
                async () => { await reader.InspectAsync(git, repo, default); }) { Strategy = strategy, AutoStash = stash };
            model.RunCommand.Execute(null); await model.RunningTask.WaitAsync(TimeSpan.FromSeconds(30)); return model;
        }
        foreach (var strategy in Enum.GetValues<PullStrategy>())
        {
            await Check($"Pull {strategy} updates superproject pointers without touching submodule worktrees", async () =>
            {
                var (repo, peer) = await Create("submodule " + strategy);
                var module = Fixture("module source " + strategy);
                await Git(module, "init", "-b", "main");
                await Git(module, "config", "user.name", "Module Fixture"); await Git(module, "config", "user.email", "module@example.invalid");
                await CommitFile(module, "module.txt", "module base");
                await Git(peer, "-c", "protocol.file.allow=always", "submodule", "add", module, "module");
                await Git(peer, "commit", "-m", "Add module"); await Git(peer, "push");
                Expect((await Pull(repo, strategy)).Succeeded, "Pull can introduce a submodule pointer");
                await Git(repo, "-c", "protocol.file.allow=always", "submodule", "update", "--init");
                var checkout = Path.Combine(repo, "module"); var before = await Git(checkout, "rev-parse", "HEAD");
                await File.WriteAllTextAsync(Path.Combine(checkout, "module.txt"), "preserve submodule edit");
                await CommitFile(module, "module.txt", "module next");
                var next = await Git(module, "rev-parse", "HEAD");
                await Git(Path.Combine(peer, "module"), "fetch", "origin");
                await Git(Path.Combine(peer, "module"), "checkout", next);
                await Git(peer, "add", "module"); await Git(peer, "commit", "-m", "Advance pointer"); await Git(peer, "push");
                await Git(repo, "config", "submodule.recurse", "true");
                var result = await Pull(repo, strategy);
                Expect(result.Succeeded, "Superproject-only Pull succeeds: " + result.Status + " " + result.RawOutput);
                Expect((await Git(repo, "ls-tree", "HEAD", "module")).Contains(next) && await Git(checkout, "rev-parse", "HEAD") == before,
                    "Superproject pointer updated while module checkout stays unchanged");
                Expect(await File.ReadAllTextAsync(Path.Combine(checkout, "module.txt")) == "preserve submodule edit", "Submodule local content untouched");
            });
            await Check($"Pull {strategy} respects an existing cone sparse checkout", async () =>
            {
                var (repo, peer) = await Create("sparse " + strategy);
                Directory.CreateDirectory(Path.Combine(peer, "included")); Directory.CreateDirectory(Path.Combine(peer, "excluded"));
                await CommitFile(peer, "included/file.txt", "included base"); await CommitFile(peer, "excluded/file.txt", "excluded base"); await Git(peer, "push");
                Expect((await Pull(repo, strategy)).Succeeded, "Initial files imported");
                await Git(repo, "sparse-checkout", "set", "included");
                var patterns = await File.ReadAllBytesAsync(Path.Combine(repo, ".git", "info", "sparse-checkout"));
                await CommitFile(peer, "included/file.txt", "included next"); await CommitFile(peer, "excluded/file.txt", "excluded next"); await Git(peer, "push");
                Expect((await Pull(repo, strategy)).Succeeded, "Pull works in sparse worktree");
                Expect(await File.ReadAllTextAsync(Path.Combine(repo, "included", "file.txt")) == "included next" && !Directory.Exists(Path.Combine(repo, "excluded")),
                    "Only included worktree files materialized");
                var afterPatterns = await File.ReadAllBytesAsync(Path.Combine(repo, ".git", "info", "sparse-checkout"));
                Expect(patterns.SequenceEqual(afterPatterns), "Sparse patterns unchanged");
            });
            await Check($"Pull {strategy} integrates real divergent files and optional tracked auto-stash", async () =>
            {
                var (repo, peer) = await Create("diverged " + strategy);
                await CommitFile(repo, "local.txt", "local commit\n");
                var oldTip = await Git(repo, "rev-parse", "HEAD"); await Git(repo, "branch", "keep", oldTip);
                await CommitFile(peer, "incoming.txt", "remote commit\n"); await Git(peer, "push");
                var remoteTip = await Git(peer, "rev-parse", "HEAD");
                await Git(repo, "config", "rebase.updateRefs", "true");
                await Git(repo, "config", "branch.main.mergeOptions", "--squash --no-commit");
                await File.WriteAllTextAsync(Path.Combine(repo, "draft.txt"), "staged draft\n"); await Git(repo, "add", "draft.txt");
                await File.AppendAllTextAsync(Path.Combine(repo, "draft.txt"), "unstaged draft\n");
                await File.WriteAllTextAsync(Path.Combine(repo, "untracked.txt"), "untracked stays\n");
                var model = await Pull(repo, strategy, stash: true);
                Expect(model.Succeeded, "Selected pull strategy really succeeds: " + model.Status + " " + model.RawOutput);
                Expect(File.Exists(Path.Combine(repo, "local.txt")) && File.Exists(Path.Combine(repo, "incoming.txt")), "Both sides' committed files survive");
                Expect(await File.ReadAllTextAsync(Path.Combine(repo, "draft.txt")) == "staged draft\nunstaged draft\n", "Staged plus unstaged content restored");
                Expect(await File.ReadAllTextAsync(Path.Combine(repo, "untracked.txt")) == "untracked stays\n" && await Git(repo, "stash", "list") == "", "Untracked file untouched and successful autostash removed by Git");
                Expect(await Git(repo, "rev-parse", "keep") == oldTip, "Other branches are not rewritten by rebase.updateRefs");
                var parents = (await Git(repo, "rev-list", "--parents", "-n", "1", "HEAD")).Split(' ');
                Expect(strategy == PullStrategy.Rebase ? parents.Length == 2 && parents[1] == remoteTip && parents[0] != oldTip :
                    parents.Length == 3 && parents[1] == oldTip && parents[2] == remoteTip, "Rebase linear parent versus merge's preserved parents");
            });
            await Check($"Pull {strategy} conflicts survive reinspection and reopen, then terminal abort restores state", async () =>
            {
                var (repo, peer) = await Create("conflict " + strategy);
                await CommitFile(repo, "base.txt", "local line\n"); var before = await Git(repo, "rev-parse", "HEAD");
                await CommitFile(peer, "base.txt", "remote line\n"); await Git(peer, "push");
                var model = await Pull(repo, strategy);
                var operation = strategy == PullStrategy.Rebase ? RepositoryOperation.Rebase : RepositoryOperation.Merge;
                Expect(!model.Succeeded && (await reader.InspectAsync(git, repo, default)).Operation.HasFlag(operation), "Conflicted sequence remains recoverable");
                Expect(model.Status == UiText.Get(strategy == PullStrategy.Rebase ? "ActionRebaseConflict" : "ActionMergeConflict"), "Localized recovery instructions plus original output");
                Expect((await actions.PrepareAsync(git, repo, default)).Repository.Operation.HasFlag(operation), "Freshly opened context observes sequence");
                await Git(repo, strategy == PullStrategy.Rebase ? "rebase" : "merge", "--abort");
                Expect(await Git(repo, "rev-parse", "HEAD") == before && await File.ReadAllTextAsync(Path.Combine(repo, "base.txt")) == "local line\n", "Git abort restores committed local branch");
            });
            await Check($"Pull {strategy} auto-stash apply conflict is not reported as success", async () =>
            {
                var (repo, peer) = await Create("stash conflict " + strategy);
                await File.WriteAllTextAsync(Path.Combine(repo, "base.txt"), "uncommitted line\n");
                await CommitFile(peer, "base.txt", "incoming line\n"); await Git(peer, "push");
                var model = await Pull(repo, strategy, stash: true);
                Expect(!model.Succeeded && model.Status == UiText.Get("ActionStashConflict"), "Exit zero with unmerged index is a conflict, not success");
                Expect((await Git(repo, "ls-files", "--unmerged")).Length > 0 && (await Git(repo, "stash", "list")).Length > 0, "Unmerged content and recovery stash retained");
                Expect((await Git(repo, "stash", "show", "-p")).Contains("uncommitted line"), "Original local content remains in Git recovery stash");
            });
        }
        await Check("Default Pull permits unrelated untracked files and rejects dirty rebase without silently enabling autostash", async () =>
        {
            var (repo, peer) = await Create("pull protections");
            await CommitFile(peer, "incoming.txt", "incoming\n"); await Git(peer, "push");
            await File.WriteAllTextAsync(Path.Combine(repo, "untracked.txt"), "keep");
            Expect((await Pull(repo, PullStrategy.Rebase)).Succeeded, "Unrelated untracked file is not a blanket rejection");
            await Git(repo, "config", "rebase.autoStash", "true");
            await File.WriteAllTextAsync(Path.Combine(repo, "base.txt"), "dirty content");
            var before = await Git(repo, "rev-parse", "HEAD");
            var failed = await Pull(repo, PullStrategy.Rebase);
            Expect(!failed.Succeeded && failed.RawOutput.Length > 0 && await Git(repo, "rev-parse", "HEAD") == before, "Unchecked checkbox overrides config and Git rejects dirty rebase");
            Expect(await File.ReadAllTextAsync(Path.Combine(repo, "base.txt")) == "dirty content", "No dirty content discarded");
        });
        await Check("Rebase protects ignored local files from incoming tracked paths", async () =>
        {
            var (repo, peer) = await Create("ignored pull");
            await CommitFile(peer, "ignored.txt", "incoming tracked"); await Git(peer, "push");
            await File.AppendAllTextAsync(Path.Combine(repo, ".git", "info", "exclude"), "\nignored.txt\n");
            await File.WriteAllTextAsync(Path.Combine(repo, "ignored.txt"), "local ignored content");
            var model = await Pull(repo, PullStrategy.Rebase, stash: true);
            Expect(!model.Succeeded && model.Status == UiText.Get("ActionIgnoredCollision") &&
                await File.ReadAllTextAsync(Path.Combine(repo, "ignored.txt")) == "local ignored content", "Ignored content is preserved before rebase checkout");
            File.Delete(Path.Combine(repo, "ignored.txt"));
            Directory.CreateDirectory(Path.Combine(peer, "generated"));
            await CommitFile(peer, "generated/file.txt", "incoming generated file"); await Git(peer, "push");
            await File.AppendAllTextAsync(Path.Combine(repo, ".git", "info", "exclude"), "\ngenerated/\n");
            Directory.CreateDirectory(Path.Combine(repo, "generated"));
            await File.WriteAllTextAsync(Path.Combine(repo, "generated", "file.txt"), "preserve generated content");
            var directoryCollision = await Pull(repo, PullStrategy.Rebase);
            Expect(!directoryCollision.Succeeded && directoryCollision.Status == UiText.Get("ActionIgnoredCollision") &&
                await File.ReadAllTextAsync(Path.Combine(repo, "generated", "file.txt")) == "preserve generated content", "Collapsed ignored directory still protects its files");
        });
        await Check("Post-submission validation can be cancelled before Git mutation", async () =>
        {
            var started = new TaskCompletionSource(); var reconciled = 0;
            var model = new GitActionViewModel(_actionFixture!, GitAction.Pull, actions,
                () => { reconciled++; return Task.CompletedTask; }, prepareExecution: async token =>
                {
                    started.SetResult(); await Task.Delay(Timeout.Infinite, token); return _actionFixture!;
                });
            model.RunCommand.Execute(null); await started.Task;
            model.Cancel(); await model.RunningTask;
            Expect(!model.IsRunning && !model.Succeeded && reconciled == 1 && model.Status == UiText.Get("ActionResultCancelled"),
                "Cancellation drains validation and reconciles without starting mutation");
        });
        await Check("Cached options are editable without preparation; only submission validates, with retained failures", async () =>
        {
            var context = _actionFixture!; var calls = 0;
            Task<ActionContext> Prepare(CancellationToken token)
            {
                calls++;
                return Task.FromException<ActionContext>(new GitReadException(GitFailure.CommandFailed, "fixture read failed"));
            }
            var model = new GitActionViewModel(context, GitAction.Pull, actions, () => Task.CompletedTask, prepareExecution: Prepare);
            var dialog = new GitActionWindow(model); var content = (FrameworkElement)dialog.Content;
            Arrange(content, 650, 520); SaveRender(content, "pull-cached.png", 650, 520);
            Expect(dialog.ResizeMode == ResizeMode.NoResize && model.RunCommand.CanExecute(null) && model.Rebase && !model.AutoStash,
                "Fixed-size close-only options immediately usable with default rebase");
            await Dispatcher.Yield(DispatcherPriority.ContextIdle);
            Expect(calls == 0, "Opening and dispatcher pumping do not prepare Git targets");
            dialog.Close();
            Expect(calls == 0 && !model.HasAttempt && !model.CanEdit, "Dismissing performs no work");
            var failure = new GitActionViewModel(context, GitAction.Pull, actions, () => Task.CompletedTask, prepareExecution: Prepare);
            var errorDialog = new GitActionWindow(failure);
            failure.RunCommand.Execute(null);
            await failure.RunningTask;
            Expect(calls == 1 && failure.RawOutput == "fixture read failed" && !failure.Succeeded && !failure.IsRunning,
                "Validation occurs after submit; failure retained in operation details");
            errorDialog.Close();
        });
    }
}
