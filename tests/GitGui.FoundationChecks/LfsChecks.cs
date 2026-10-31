using System.IO;
using System.Text;
using System.Diagnostics;
using System.Windows.Threading;
using GitGui.App.ViewModels;
using GitGui.Core.Git;
using GitGui.Core.Processes;
using GitGui.ProcessHost;

namespace GitGui.FoundationChecks;
internal static partial class Program
{
    private static bool _lfsChecks;
    private static async Task RunLfsChecksAsync()
    {
        await RunLfsDiscardChecksAsync();
        var runner = new WindowsProcessRunner(); var reader = new RepositoryReader(runner);
        var actions = new GitActions(runner, reader, new WindowsRepositoryIdentity()); var git = await reader.DiscoverAsync(null, default);
        var repo = Fixture("LFS asset resolution");
        async Task<string> Git(params string[] args)
        {
            var result = await runner.RunAsync(new(git.Executable, repo, args), default);
            Expect(result.ExitCode == 0, Encoding.UTF8.GetString(result.StandardError)); return Encoding.UTF8.GetString(result.StandardOutput).Trim();
        }
        await Git("init", "-b", "main"); await Git("config", "user.name", "LFS Fixture"); await Git("config", "user.email", "lfs@example.invalid");
        await Git("config", "core.autocrlf", "false"); await Git("config", "filter.lfs.process", "git-lfs filter-process");
        await Git("config", "filter.lfs.clean", "git-lfs clean -- %f"); await Git("config", "filter.lfs.smudge", "git-lfs smudge -- %f"); await Git("config", "filter.lfs.required", "true");
        await File.WriteAllTextAsync(Path.Combine(repo, ".gitattributes"), "*.uasset filter=lfs diff=lfs merge=lfs -text\n");
        var bytes = new byte[65536]; new Random(13).NextBytes(bytes); bytes[0] = 0;
        var asset = Path.Combine(repo, "test.uasset"); await File.WriteAllBytesAsync(asset, bytes);
        await Git("add", "."); await Git("commit", "-m", "base");
        await Check("LFS read-only status verifies hydrated bytes and preserves index/custom filters", async () =>
        {
            File.SetLastWriteTimeUtc(asset, DateTime.UtcNow.AddSeconds(10));
            var snapshot = await reader.InspectAsync(git, repo, default, includeHistory: false); var stamp = await GitActions.IndexStampAsync(snapshot, default);
            var changes = await reader.ReadLocalChangesAsync(git, repo, false, default);
            Expect(changes.Entries.Count == 0 && stamp == await GitActions.IndexStampAsync(snapshot, default), "Touched clean LFS asset stays clean; index untouched");
            bytes[20] ^= 1; await File.WriteAllBytesAsync(asset, bytes);
            changes = await reader.ReadLocalChangesAsync(git, repo, false, default);
            Expect(changes.Entries.Single().Path == "test.uasset", "Real changed asset remains visible");
            await Git("config", "filter.lfs.clean", "exit 42"); await Git("config", "filter.lfs.process", "exit 42");
            changes = await reader.ReadLocalChangesAsync(git, repo, false, default);
            Expect(changes.Entries.Single().Path == "test.uasset", "Custom filters remain suppressed");
            await Git("config", "filter.lfs.process", "git-lfs filter-process"); await Git("config", "filter.lfs.clean", "git-lfs clean -- %f");
        });
        await Check("LFS discard restores actual bytes and removes affected UI rows", async () =>
        {
            var context = await actions.ReadWorkingCopyAsync(git, repo, default);
            var result = await actions.ExecuteWorkingCopyAsync(context, new(WorkingCopyAction.Discard, context.Changes.Entries), default);
            bytes[20] ^= 1;
            Expect(result.Result.Success && result.UpdatedContext?.Changes.Entries.Count == 0 && (await File.ReadAllBytesAsync(asset)).SequenceEqual(bytes), "Hydrated asset restored and clean");
        });
        await Check("LFS conflict hydrates and stages the chosen side", async () =>
        {
            await Git("switch", "-c", "incoming"); bytes[30] = 42; await File.WriteAllBytesAsync(asset, bytes); await Git("commit", "-am", "incoming");
            var incoming = bytes.ToArray(); await Git("switch", "main"); bytes[30] = 43; await File.WriteAllBytesAsync(asset, bytes); await Git("commit", "-am", "current");
            var merge = await runner.RunAsync(new(git.Executable, repo, ["merge", "--no-edit", "incoming"]), default); Expect(merge.ExitCode != 0, "LFS pointer conflict");
            var context = await actions.ReadWorkingCopyAsync(git, repo, default); var entry = context.Changes.Entries.Single(e => e.Group == ChangeGroup.Conflicted);
            var preview = await actions.ReadConflictAsync(context, entry, default); Expect(!preview.CanEdit, "Pointer text cannot be saved as an asset");
            var result = await actions.ExecuteWorkingCopyAsync(context, new(WorkingCopyAction.ResolveConflict, [entry], Resolution: new(preview, ConflictChoice.Incoming)), default);
            Expect(result.Result.Success && (await File.ReadAllBytesAsync(asset)).SequenceEqual(incoming) && await Git("ls-files", "-u") == "", "Actual incoming LFS bytes and resolved index");
        });
        await Check("Discard stays responsive and cancellable while a native asset restore is delayed", async () =>
        {
            await Git("merge", "--abort"); bytes[50] ^= 1; await File.WriteAllBytesAsync(asset, bytes);
            var gate = new DiscardDelayRunner(runner); var observed = new RepositoryReader(gate);
            var model = new CommitViewModel(repo, git, observed, new(gate, observed, new WindowsRepositoryIdentity()), new FileActionInteraction { Response = "" });
            await model.StartLoading(); model.Select(false, [model.Unstaged.Single(e => e.Change.Path == "test.uasset")]); await model.DiffTask;
            using var menu = model.CreateFileMenu(false); menu.DiscardCommand.Execute(null); await gate.Started.Task.WaitAsync(TimeSpan.FromSeconds(10));
            var watch = Stopwatch.StartNew(); await Dispatcher.Yield(DispatcherPriority.Input);
            Console.WriteLine($"Input-priority dispatcher response while discard is held: {watch.Elapsed.TotalMilliseconds:F1} ms");
            Expect(watch.Elapsed < TimeSpan.FromSeconds(1) && model.IsMutating && !model.Ready, "Dispatcher remains available; no competing writes");
            model.RefreshCommand.Execute(null); await model.OperationTask.WaitAsync(TimeSpan.FromSeconds(10));
            Expect((await File.ReadAllBytesAsync(asset)).SequenceEqual(bytes) && !model.IsMutating, "Cancel before native restore preserves modified asset");
            await model.CloseAsync();
        });
    }
    private sealed class DiscardDelayRunner(IProcessRunner inner) : IProcessRunner
    {
        public TaskCompletionSource Started { get; } = new(TaskCreationOptions.RunContinuationsAsynchronously);
        public async Task<ProcessResult> RunAsync(ProcessRequest request, CancellationToken token, Action<ProcessOutput>? output = null)
        {
            if (request.Arguments.Contains("checkout-index")) { Started.TrySetResult(); await Task.Delay(Timeout.InfiniteTimeSpan, token); }
            return await inner.RunAsync(request, token, output);
        }
    }
}
