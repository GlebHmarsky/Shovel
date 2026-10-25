using System.IO;
using System.Text;
using GitGui.Core.Git;
using GitGui.Core.Processes;
using GitGui.ProcessHost;

namespace GitGui.FoundationChecks;

internal static partial class Program
{
    private static ActionContext? _actionFixture;
    private static async Task RunActionChecksAsync(RepositoryReader reader, GitInstallation git, IProcessRunner runner,
        Func<string, string[], Task<ProcessResult>> command)
    {
        var repo = Fixture("actions — 日本語"); var remote = Fixture("action remote.git");
        async Task Git(string path, params string[] args) => Expect((await command(path, args)).ExitCode == 0, "Action fixture Git command succeeded");
        await Git(repo, "init", "-b", "main"); await Git(remote, "init", "--bare", "-b", "main");
        await Git(repo, "config", "user.name", "Action Fixture"); await Git(repo, "config", "user.email", "action@example.invalid");
        await File.WriteAllTextAsync(Path.Combine(repo, "base.txt"), "base"); await Git(repo, "add", "."); await Git(repo, "commit", "-m", "Base");
        await Git(repo, "remote", "add", "origin", remote);
        var actions = new GitActions(runner, reader, new WindowsRepositoryIdentity());
        Task<ActionContext> Prepare(string? path = null) => actions.PrepareAsync(git, path ?? repo, default);
        Task<ActionResult> Run(ActionContext context, ActionOptions options, CancellationToken token = default) => actions.ExecuteAsync(context, options with { AllowConfiguredPrograms = true }, token);
        async Task Reject(string code, Func<Task> act)
        {
            try { await act(); throw new InvalidOperationException("Expected action rejection: " + code); }
            catch (GitActionException exception) { Expect(exception.Code == code, "Expected safe action rejection"); }
        }
        await Check("Create/switch branches, reject invalid/duplicate names, require explicit execution approval", async () =>
        {
            var context = await Prepare();
            await Reject("ActionApprovalRequired", () => actions.ExecuteAsync(context, new(GitAction.NewBranch, "not-approved"), default));
            Expect((await Run(context, new(GitAction.NewBranch, "feature/日本語", Checkout: false))).Success, "Create real branch without checkout");
            Expect((await reader.InspectAsync(git, repo, default)).Branch == "main", "Unchecked checkout retains HEAD");
            Expect((await Run(await Prepare(), new(GitAction.SwitchBranch, "feature/日本語"))).Success, "Switch real branch");
            Expect((await reader.InspectAsync(git, repo, default)).Branch == "feature/日本語", "Switch changes HEAD");
            Expect(!(await Run(await Prepare(), new(GitAction.NewBranch, "main"))).Success, "Duplicate cannot reset existing branch");
            await Reject("ActionBranchName", async () => await Run(await Prepare(), new(GitAction.NewBranch, "--bad")));
            Expect((await Run(await Prepare(), new(GitAction.SwitchBranch, "main"))).Success, "Return fixture to main");
        });
        await Check("Explicit push, tags, upstream and fetch use disposable local bare remote", async () =>
        {
            await Git(repo, "tag", "v-fixture");
            Expect((await Run(await Prepare(), new(GitAction.Push, "main", Remote: "origin", RemoteBranch: "main", AllTags: true, SetUpstream: true))).Success, "Push and set upstream");
            Expect((await command(remote, ["show-ref", "--verify", "refs/tags/v-fixture"])).ExitCode == 0, "Tag really published");
            Expect((await reader.InspectAsync(git, repo, default)).UpstreamRef == "refs/remotes/origin/main", "Upstream persists in repository config");
            Expect((await Run(await Prepare(), new(GitAction.Fetch, Remote: "origin", AllRemotes: true))).Success, "Fetch actually runs");
            _actionFixture = await Prepare();
        });
        var peer = Fixture("action peer"); await Git(peer, "clone", remote, ".");
        await Git(peer, "config", "user.name", "Peer"); await Git(peer, "config", "user.email", "peer@example.invalid");
        await Check("Pull fast-forwards, then rejects divergence without rewriting local commits", async () =>
        {
            await Git(peer, "commit", "--allow-empty", "-m", "Peer one"); await Git(peer, "push", "origin", "main");
            await Git(repo, "config", "branch.main.mergeOptions", "--squash");
            await Reject("ActionCustomMergeOptions", async () => await Run(await Prepare(), new(GitAction.Pull, Remote: "origin", RemoteBranch: "main")));
            await Git(repo, "config", "--unset", "branch.main.mergeOptions");
            var previous = (await Prepare()).Repository.Commit;
            var result = await Run(await Prepare(), new(GitAction.Pull, Remote: "origin", RemoteBranch: "main"));
            Expect(result.Success && (await Prepare()).Repository.Commit != previous, "Real fast-forward changes local HEAD");
            await Git(repo, "commit", "--allow-empty", "-m", "Local divergence");
            await Git(peer, "commit", "--allow-empty", "-m", "Peer divergence"); await Git(peer, "push", "origin", "main");
            previous = (await Prepare()).Repository.Commit;
            result = await Run(await Prepare(), new(GitAction.Pull, Remote: "origin", RemoteBranch: "main"));
            Expect(!result.Success && (await Prepare()).Repository.Commit == previous, "Divergence retains local tip");
            Expect(!(await Run(await Prepare(), new(GitAction.Push, "main", Remote: "origin", RemoteBranch: "main"))).Success, "Non-fast-forward push rejected");
        });
        await Check("Dirty/sequence/stale identity/config gates preserve repository state", async () =>
        {
            await File.WriteAllTextAsync(Path.Combine(repo, "untracked.txt"), "preserve");
            await Reject("ActionDirty", async () => await Run(await Prepare(), new(GitAction.SwitchBranch, "feature/日本語")));
            File.Delete(Path.Combine(repo, "untracked.txt"));
            var context = await Prepare();
            await Git(repo, "branch", "external-change");
            await Reject("ActionChanged", () => Run(context, new(GitAction.Fetch, Remote: "origin")));
            context = await Prepare(); await Git(repo, "config", "shovel.changed", "true");
            await Reject("ActionChanged", () => Run(context, new(GitAction.Fetch, Remote: "origin")));
            context = await Prepare();
            await Reject("ActionIdentityChanged", () => Run(context with { Identity = context.Identity with { Fingerprint = "replaced" } }, new(GitAction.Fetch, Remote: "origin")));
            var marker = Path.Combine(repo, ".git", "MERGE_HEAD"); await File.WriteAllTextAsync(marker, context.Repository.Commit);
            try { await Reject("ActionSequence", async () => await Run(await Prepare(), new(GitAction.Fetch, Remote: "origin"))); }
            finally { File.Delete(marker); }
        });
        await Check("Failed hooks are honored; unavailable remote leaves usable local history", async () =>
        {
            var hooks = Fixture("action hooks"); await Git(repo, "config", "core.hooksPath", hooks.Replace('\\', '/'));
            var hook = Path.Combine(hooks, "pre-push"); await File.WriteAllTextAsync(hook, "#!/bin/sh\necho hook-fixture-rejected >&2\nexit 1\n", new UTF8Encoding(false));
            var failed = await Run(await Prepare(), new(GitAction.Push, "feature/日本語", Remote: "origin", RemoteBranch: "feature-test"));
            Expect(!failed.Success && failed.Output.Contains("hook-fixture-rejected"), "Do not disable pre-push hook");
            File.Delete(hook);
            await Git(repo, "remote", "add", "missing", Path.Combine(_root, "missing-remote.git"));
            failed = await Run(await Prepare(), new(GitAction.Fetch, Remote: "missing"));
            Expect(!failed.Success && (await reader.InspectAsync(git, repo, default)).Commit is not null, "Remote failure does not break local reads");
            await Git(repo, "remote", "remove", "missing");
        });
        await Check("Cancellation kills waiting hook; shared-worktree action gate rejects concurrent writes", async () =>
        {
            var hooks = Fixture("waiting hooks"); var marker = Path.Combine(hooks, "started");
            await Git(repo, "config", "core.hooksPath", hooks.Replace('\\', '/'));
            await File.WriteAllTextAsync(Path.Combine(hooks, "pre-push"), "#!/bin/sh\necho started > '" + marker.Replace('\\', '/') + "'\nsleep 60\n", new UTF8Encoding(false));
            var linked = Path.Combine(_root, "action-linked"); await Git(repo, "worktree", "add", "-b", "linked-action", linked);
            var context = await Prepare(); var linkedContext = await Prepare(linked);
            using var cancellation = new CancellationTokenSource(TimeSpan.FromSeconds(12));
            var pending = Run(context, new(GitAction.Push, "feature/日本語", Remote: "origin", RemoteBranch: "feature-test"), cancellation.Token);
            var deadline = DateTime.UtcNow.AddSeconds(8);
            while (!File.Exists(marker) && DateTime.UtcNow < deadline && !pending.IsCompleted) await Task.Delay(25);
            Expect(File.Exists(marker), "Hook started before cancellation");
            await Reject("ActionOtherInstance", () => Run(linkedContext, new(GitAction.NewBranch, "concurrent", Checkout: false)));
            cancellation.Cancel(); var stopped = await pending.WaitAsync(TimeSpan.FromSeconds(5));
            Expect(stopped.Outcome == ProcessOutcome.Cancelled, "Owned waiting hook cancelled");
            await Git(repo, "config", "--unset", "core.hooksPath");
            Expect((await Run(await Prepare(), new(GitAction.NewBranch, "after-cancel", Checkout: false))).Success, "Gate released after cancellation and reinspection");
        });
        await Check("Credential failure stays explicit and URL secrets are redacted", async () =>
        {
            var failureRunner = new AuthenticationFailureRunner(runner);
            var failedActions = new GitActions(failureRunner, reader, new WindowsRepositoryIdentity());
            var result = await failedActions.ExecuteAsync(await Prepare(), new(GitAction.Push, "main", Remote: "origin", RemoteBranch: "main", AllowConfiguredPrograms: true), default);
            Expect(!result.Success && result.Output.Contains("Authentication failed") && !result.Output.Contains("secret") && !result.Output.Contains("token-value"), "Controlled auth failure never logs/exposes URL credentials");
        });
        await Check("Branch checkout preserves ignored files that collide with tracked target files", async () =>
        {
            await Git(repo, "switch", "-c", "ignored-target");
            await File.WriteAllTextAsync(Path.Combine(repo, "ignored.txt"), "tracked target");
            await Git(repo, "add", "ignored.txt"); await Git(repo, "commit", "-m", "Tracked target fixture");
            await Git(repo, "switch", "main");
            await File.AppendAllTextAsync(Path.Combine(repo, ".git", "info", "exclude"), "\nignored.txt\n");
            await File.WriteAllTextAsync(Path.Combine(repo, "ignored.txt"), "must survive");
            var result = await Run(await Prepare(), new(GitAction.SwitchBranch, "ignored-target"));
            Expect(!result.Success && await File.ReadAllTextAsync(Path.Combine(repo, "ignored.txt")) == "must survive" && (await Prepare()).Repository.Branch == "main", "Ignored content cannot be silently overwritten");
        });
    }
    private sealed class AuthenticationFailureRunner(IProcessRunner inner) : IProcessRunner
    {
        public Task<ProcessResult> RunAsync(ProcessRequest request, CancellationToken token, Action<ProcessOutput>? output = null) =>
            request.Arguments.Contains("push") ? Task.FromResult(new ProcessResult(ProcessOutcome.Exited, 128, [],
                Encoding.UTF8.GetBytes("Authentication failed: https://user:secret@example.invalid/repo?token=token-value"), TimeSpan.Zero)) : inner.RunAsync(request, token, output);
    }
}
