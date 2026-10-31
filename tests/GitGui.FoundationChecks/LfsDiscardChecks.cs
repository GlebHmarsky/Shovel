using System.IO;
using System.Text;
using GitGui.Core.Git;
using GitGui.Core.Processes;
using GitGui.ProcessHost;

namespace GitGui.FoundationChecks;

internal static partial class Program
{
    private static async Task RunLfsDiscardChecksAsync()
    {
        var native = new WindowsProcessRunner(); var observed = new LfsDiscardRunner(native);
        var reader = new RepositoryReader(observed); var actions = new GitActions(observed, reader, new WindowsRepositoryIdentity());
        var git = await reader.DiscoverAsync(null, default);
        const string first = "asset [α].uasset", second = "second.uasset";
        var baseline = new byte[65536]; new Random(31).NextBytes(baseline); baseline[0] = 0;
        async Task<string> Git(string repo, params string[] args)
        {
            var run = await native.RunAsync(new(git.Executable, repo, args), default);
            Expect(run.ExitCode == 0, "Fixture Git: " + Encoding.UTF8.GetString(run.StandardError));
            return Encoding.UTF8.GetString(run.StandardOutput).Trim();
        }
        async Task Edit(string repo, string file, byte value)
        {
            var bytes = baseline.ToArray(); bytes[123] = value;
            await File.WriteAllBytesAsync(Path.Combine(repo, file), bytes);
        }
        async Task<string> Repo(string name)
        {
            observed.Reset(); var repo = Fixture("LFS discard " + name);
            await Git(repo, "init", "-b", "main"); await Git(repo, "config", "user.name", "LFS fixture"); await Git(repo, "config", "user.email", "lfs@example.invalid");
            await Git(repo, "config", "core.autocrlf", "false");
            await Git(repo, "config", "filter.lfs.process", "git-lfs filter-process"); await Git(repo, "config", "filter.lfs.clean", "git-lfs clean -- %f");
            await Git(repo, "config", "filter.lfs.smudge", "git-lfs smudge -- %f"); await Git(repo, "config", "filter.lfs.required", "true");
            await File.WriteAllTextAsync(Path.Combine(repo, ".gitattributes"), "*.uasset filter=lfs diff=lfs merge=lfs -text\n");
            foreach (var file in new[] { first, second }) await File.WriteAllBytesAsync(Path.Combine(repo, file), baseline);
            await File.WriteAllTextAsync(Path.Combine(repo, "ordinary.txt"), "baseline\n");
            await Git(repo, "add", "."); await Git(repo, "commit", "-m", "base");
            return repo;
        }
        Task<WorkingCopyContext> Read(string repo) => actions.ReadWorkingCopyAsync(git, repo, default);
        Task<WorkingCopyResult> Discard(WorkingCopyContext context) => actions.ExecuteWorkingCopyAsync(context,
            new(WorkingCopyAction.Discard, context.Changes.Entries.Where(e => e.Group == ChangeGroup.Unstaged).ToArray()), default);

        await Check("LFS batch discard verifies selected bytes, retains partial staging and runs native index hooks", async () =>
        {
            var repo = await Repo("partial batch");
            await Edit(repo, first, 41); await Git(repo, "add", "--", first); var staged = await File.ReadAllBytesAsync(Path.Combine(repo, first));
            await Edit(repo, first, 42); await Edit(repo, second, 43);
            await File.WriteAllTextAsync(Path.Combine(repo, "keep.txt"), "unrelated");
            await File.WriteAllTextAsync(Path.Combine(repo, ".git", "hooks", "post-index-change"), "#!/bin/sh\nprintf index > .git/index-marker\n");
            var context = await Read(repo); observed.Reset(); var result = await Discard(context);
            Expect(result.Result.Success && result.UpdatedContext is not null && observed.StatusCalls == 0, "Selected LFS verification avoids raw status");
            Expect((await File.ReadAllBytesAsync(Path.Combine(repo, first))).SequenceEqual(staged) &&
                (await File.ReadAllBytesAsync(Path.Combine(repo, second))).SequenceEqual(baseline), "Actual selected bytes restored from index");
            Expect(result.UpdatedContext!.Changes.Entries.Any(e => e.Path == first && e.Group == ChangeGroup.Staged) &&
                !result.UpdatedContext.Changes.Entries.Any(e => e.Group == ChangeGroup.Unstaged) &&
                result.UpdatedContext.Changes.Entries.Any(e => e.Path == "keep.txt"), "Staged/unrelated rows retained");
            Expect(File.Exists(Path.Combine(repo, ".git", "index-marker")), "Native post-index-change hook runs");
        });
        await Check("LFS discard rejects restored bytes and index changes after the reviewed snapshot", async () =>
        {
            var repo = await Repo("stale"); await Edit(repo, first, 42); var context = await Read(repo);
            await File.WriteAllBytesAsync(Path.Combine(repo, first), baseline); observed.Reset();
            await ExpectException<GitActionException>(() => Discard(context));
            Expect(observed.CheckoutCalls == 0, "Externally restored content cannot authorize a stale discard");
            await Edit(repo, first, 43); context = await Read(repo); await Git(repo, "add", "--", first);
            await ExpectException<GitActionException>(() => Discard(context));
            Expect(observed.CheckoutCalls == 0, "Changed index rejects before checkout");
        });
        await Check("LFS discard reconciliation detects same-size same-timestamp edits after checkout", async () =>
        {
            var repo = await Repo("racing bytes"); await Edit(repo, first, 42); var context = await Read(repo); observed.Reset();
            observed.AfterCheckout = async () =>
            {
                var file = Path.Combine(repo, first); var stamp = File.GetLastWriteTimeUtc(file);
                await Edit(repo, first, 44); File.SetLastWriteTimeUtc(file, stamp);
            };
            var result = await Discard(context);
            Expect(result.Result.Success && result.UpdatedContext?.Changes.Entries.Any(e => e.Path == first && e.Group == ChangeGroup.Unstaged) == true,
                "Native success does not hide a concurrent modification");
            Expect(observed.StatusCalls == 0 && (await File.ReadAllBytesAsync(Path.Combine(repo, first)))[123] == 44, "Fresh hash catches change without cached metadata shortcut");
        });
        await Check("LFS discard falls back to native status when the index changes during checkout", async () =>
        {
            var repo = await Repo("racing index"); await Edit(repo, first, 42); var context = await Read(repo); observed.Reset();
            observed.AfterCheckout = async () => { await Edit(repo, first, 45); await Git(repo, "add", "--", first); };
            var result = await Discard(context);
            Expect(result.Result.Success && observed.StatusCalls > 0 &&
                result.UpdatedContext?.Changes.Entries.Any(e => e.Path == first && e.Group == ChangeGroup.Staged) == true, "Changed pointer uses actual native staged status");
        });
        await Check("LFS discard preserves fallback behavior for changed lengths and missing files", async () =>
        {
            var repo = await Repo("fallback"); await File.WriteAllBytesAsync(Path.Combine(repo, first), [0, 1, 2]);
            File.Delete(Path.Combine(repo, second)); var context = await Read(repo); observed.Reset(); var result = await Discard(context);
            Expect(result.Result.Success && observed.StatusCalls > 0 && result.UpdatedContext?.Changes.Entries.Count == 0, "Fallback reconciles deletion and changed size");
            foreach (var file in new[] { first, second }) Expect((await File.ReadAllBytesAsync(Path.Combine(repo, file))).SequenceEqual(baseline), "Fallback hydrates exact asset");
        });
        await Check("LFS discard reports missing local objects without pretending hydration succeeded", async () =>
        {
            var repo = await Repo("missing object"); await Edit(repo, first, 42);
            // Redirect only this disposable fixture's storage; there is no remote to contact.
            await Git(repo, "config", "lfs.storage", Path.Combine(repo, ".git", "empty-lfs"));
            var context = await Read(repo); observed.Reset(); var result = await Discard(context);
            Expect(!result.Result.Success && result.UpdatedContext is null && observed.CheckoutCalls == 1, "Required native LFS filter failure remains a failure");
        });
        await Check("Ordinary files in LFS repositories bypass specialized discard probes", async () =>
        {
            var repo = await Repo("ordinary text"); await File.WriteAllTextAsync(Path.Combine(repo, "ordinary.txt"), "changed\n");
            var context = await Read(repo); observed.Reset(); var result = await Discard(context);
            Expect(result.Result.Success && observed.StatusCalls == 2 && observed.IndexMetadataCalls == 0 &&
                await File.ReadAllTextAsync(Path.Combine(repo, "ordinary.txt")) == "baseline\n", "Normal status path has no additional index probes");
        });
    }

    private sealed class LfsDiscardRunner(IProcessRunner inner) : IProcessRunner
    {
        public int StatusCalls, CheckoutCalls, IndexMetadataCalls;
        public Func<Task>? AfterCheckout;
        public void Reset() { StatusCalls = 0; CheckoutCalls = 0; IndexMetadataCalls = 0; AfterCheckout = null; }
        public async Task<ProcessResult> RunAsync(ProcessRequest request, CancellationToken token, Action<ProcessOutput>? output = null)
        {
            if (request.Arguments.Contains("status")) Interlocked.Increment(ref StatusCalls);
            if (request.Arguments.Contains("ls-files")) Interlocked.Increment(ref IndexMetadataCalls);
            var checkout = request.Arguments.Contains("checkout-index");
            if (checkout) Interlocked.Increment(ref CheckoutCalls);
            var result = await inner.RunAsync(request, token, output);
            if (checkout && AfterCheckout is { } action) { AfterCheckout = null; await action(); }
            return result;
        }
    }
}
