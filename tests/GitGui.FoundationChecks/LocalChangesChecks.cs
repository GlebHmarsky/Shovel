using System.Diagnostics;
using System.IO;
using System.Text;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Threading;
using GitGui.App.Localization;
using GitGui.App.ViewModels;
using GitGui.App.Views;
using GitGui.Core.Git;
using GitGui.Core.Processes;
using GitGui.Core.Settings;
using GitGui.ProcessHost;

namespace GitGui.FoundationChecks;

internal static partial class Program
{
    private static async Task RunLocalChangesChecksAsync()
    {
        var runner = new WindowsProcessRunner(); var reader = new RepositoryReader(runner);
        var git = await reader.DiscoverAsync(null, default);
        var repo = Fixture("status 日本語 spaces"); var second = Fixture("status second");
        async Task Git(string directory, params string[] args)
        {
            var result = await runner.RunAsync(new(git.Executable, directory, args), default);
            Expect(result.ExitCode == 0, "Disposable status fixture Git command succeeded");
        }
        await Git(repo, "init", "-b", "main"); await Git(second, "init", "-b", "main");
        await Git(repo, "config", "user.name", "Status Fixture"); await Git(repo, "config", "user.email", "status@example.invalid");
        await Check("Status reads unborn and clean repositories without requiring commits", async () =>
        {
            Expect((await reader.ReadLocalChangesAsync(git, repo, false, default)).Entries.Count == 0, "Unborn empty status");
            await File.WriteAllTextAsync(Path.Combine(repo, "both.txt"), "base");
            await File.WriteAllTextAsync(Path.Combine(repo, "rename old.txt"), "rename unique content");
            await File.WriteAllTextAsync(Path.Combine(repo, "deleted.txt"), "delete");
            await Git(repo, "add", ".");
            Expect((await reader.ReadLocalChangesAsync(git, repo, false, default)).Entries.All(row => row.Group == ChangeGroup.Staged && row.Code == 'A'), "Unborn index additions");
            await Git(repo, "commit", "-m", "Status baseline");
            Expect((await reader.ReadLocalChangesAsync(git, repo, false, default)).Entries.Count == 0, "Clean committed status");
        });
        await Check("Real status separates index/worktree, renames, deletion, Unicode untracked and ignored paths without index writes", async () =>
        {
            await File.WriteAllTextAsync(Path.Combine(repo, "both.txt"), "staged"); await Git(repo, "add", "both.txt");
            await File.WriteAllTextAsync(Path.Combine(repo, "both.txt"), "unstaged");
            await Git(repo, "mv", "rename old.txt", "renamed 日本語.txt"); File.Delete(Path.Combine(repo, "deleted.txt"));
            Directory.CreateDirectory(Path.Combine(repo, "new dir")); await File.WriteAllBytesAsync(Path.Combine(repo, "new dir", "binary 日本語.bin"), [0, 255, 0]);
            await File.WriteAllTextAsync(Path.Combine(repo, ".git", "info", "exclude"), "ignored/\n");
            Directory.CreateDirectory(Path.Combine(repo, "ignored")); await File.WriteAllTextAsync(Path.Combine(repo, "ignored", "generated.txt"), "ignored");
            var before = await File.ReadAllBytesAsync(Path.Combine(repo, ".git", "index"));
            var entries = (await reader.ReadLocalChangesAsync(git, repo, false, default)).Entries;
            Expect(entries.Count(row => row.Path == "both.txt") == 2 && entries.Any(row => row.Group == ChangeGroup.Staged && row.Path == "both.txt") &&
                entries.Any(row => row.Group == ChangeGroup.Unstaged && row.Path == "both.txt"), "Same path has independent staged/unstaged rows");
            Expect(entries.Any(row => row.Code == 'R' && row.Path == "renamed 日本語.txt" && row.OriginalPath == "rename old.txt"), "NUL rename destination and origin preserved");
            Expect(entries.Any(row => row.Code == 'D' && row.Group == ChangeGroup.Unstaged) && entries.Any(row => row.Path == "new dir/binary 日本語.bin" && row.Group == ChangeGroup.Untracked), "Deleted and binary untracked paths");
            Expect(!entries.Any(row => row.Group == ChangeGroup.Ignored) && (await reader.ReadLocalChangesAsync(git, repo, true, default)).Entries.Any(row => row.Group == ChangeGroup.Ignored && row.Path == "ignored/"), "Ignored paths opt in; directory collapse explicit");
            var afterIndex = await File.ReadAllBytesAsync(Path.Combine(repo, ".git", "index"));
            Expect(before.SequenceEqual(afterIndex), "Read-only status preserves index bytes");
        });
        await Check("Status suppresses configured clean filters and fsmonitor helpers", async () =>
        {
            var marker = Path.Combine(repo, "filter-ran");
            await File.WriteAllTextAsync(Path.Combine(repo, ".gitattributes"), "both.txt filter=fixture\n");
            await Git(repo, "config", "filter.fixture.clean", "echo ran > filter-ran; cat");
            await Git(repo, "config", "filter.fixture.required", "true");
            await Git(repo, "config", "core.fsmonitor", "echo ran > filter-ran");
            await reader.ReadLocalChangesAsync(git, repo, false, default);
            Expect(!File.Exists(marker), "No configured filter/fsmonitor executes");
            await Git(repo, "config", "--unset", "core.fsmonitor"); await Git(repo, "config", "--remove-section", "filter.fixture");
        });
        await Check("Status parser preserves control characters and rejects malformed, invalid UTF-8 and oversized data", async () =>
        {
            var rows = RepositoryReader.ParseLocalChanges(Encoding.UTF8.GetBytes("? line\nwith\ttab\0"));
            Expect(rows.Single().Path == "line\nwith\ttab", "NUL boundaries, not line splitting");
            foreach (var bytes in new[] { Encoding.UTF8.GetBytes("? truncated"), Encoding.UTF8.GetBytes("2 R. N... 100644 100644 100644 a b R100 name\0"), new byte[] { 255, 0 }, new byte[RepositoryReader.MaximumStatusBytes + 1] })
                await ExpectException<GitReadException>(() => Task.Run(() => RepositoryReader.ParseLocalChanges(bytes)));
            await ExpectException<GitReadException>(() => Task.Run(() => RepositoryReader.ParseLocalChanges(Encoding.UTF8.GetBytes(string.Concat(Enumerable.Repeat("? x\0", RepositoryReader.MaximumStatusEntries + 1))))));
            using var cancellation = new CancellationTokenSource(); cancellation.Cancel();
            await ExpectException<OperationCanceledException>(() => Task.Run(() => RepositoryReader.ParseLocalChanges(Encoding.UTF8.GetBytes("? name\0"), cancellation.Token)));
        });
        await Check("Conflicted status is distinct from normal staged files", async () =>
        {
            await File.WriteAllTextAsync(Path.Combine(second, "conflict.txt"), "base");
            await Git(second, "config", "user.name", "Conflict"); await Git(second, "config", "user.email", "conflict@example.invalid");
            await Git(second, "add", "."); await Git(second, "commit", "-m", "base"); await Git(second, "checkout", "-b", "side");
            await File.WriteAllTextAsync(Path.Combine(second, "conflict.txt"), "side"); await Git(second, "commit", "-am", "side");
            await Git(second, "checkout", "main"); await File.WriteAllTextAsync(Path.Combine(second, "conflict.txt"), "main"); await Git(second, "commit", "-am", "main");
            var result = await runner.RunAsync(new(git.Executable, second, ["merge", "side"]), default);
            Expect(result.ExitCode != 0, "Disposable merge conflicts");
            Expect((await reader.ReadLocalChangesAsync(git, second, false, default)).Entries.Single().Group == ChangeGroup.Conflicted, "Unmerged record appears only in conflict group");
        });
    }
    private sealed class StatusGateRunner(IProcessRunner inner) : IProcessRunner
    {
        public bool Hold, Fail;
        public int StatusCalls;
        public TaskCompletionSource Started { get; private set; } = new(TaskCreationOptions.RunContinuationsAsynchronously);
        public TaskCompletionSource Release { get; private set; } = new(TaskCreationOptions.RunContinuationsAsynchronously);
        public void ResetGate() { Hold = true; Started = new(TaskCreationOptions.RunContinuationsAsynchronously); Release = new(TaskCreationOptions.RunContinuationsAsynchronously); }
        public async Task<ProcessResult> RunAsync(ProcessRequest request, CancellationToken token, Action<ProcessOutput>? output = null)
        {
            if (request.Arguments.Contains("status"))
            {
                StatusCalls++; Started.TrySetResult();
                if (Hold) await Release.Task.WaitAsync(token);
                if (Fail) return new(ProcessOutcome.Exited, 128, [], Encoding.UTF8.GetBytes("fixture failure"), TimeSpan.Zero);

            }
            return await inner.RunAsync(request, token, output);
        }
    }
}
