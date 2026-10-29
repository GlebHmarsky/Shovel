using System.Collections.Concurrent;
using System.IO;
using System.Text.Json;
using GitGui.App.ViewModels;
using GitGui.Core.Diagnostics;
using GitGui.Core.Drafts;
using GitGui.Core.Git;

namespace GitGui.FoundationChecks;
internal static partial class Program
{
    private static bool _draftIoChecks;
    private static async Task RunDraftIoChecksAsync()
    {
        const string identity = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
        const string worktree = "fixture-worktree-secret";
        var identities = new DraftIdentityProvider();
        await Check("Repeated clean-cache profile replacement preserves the newer draft with typed failure evidence", async () =>
        {
            var faults = new ConcurrentQueue<CommitDraftDiagnostic>();
            var store = new CommitDraftStore(Fixture("draft reopen io"), identities, faults.Enqueue);
            for (var i = 0; i < 12; i++)
            {
                var state = new CommitWorkspaceState();
                var first = new CommitDraftCoordinator(state, store, worktree, identity, TimeSpan.Zero);
                await first.StartOrRetryAsync(); state.Subject = "old " + i; first.TextChanged(); await first.CloseAsync();
                var previous = await store.LoadAsync(worktree, identity, default);
                await store.SaveAsync(worktree, identity, previous.Revision, new("new external " + i, ""), default);
                var reopened = new CommitDraftCoordinator(state, store, worktree, identity, TimeSpan.Zero);
                await reopened.StartOrRetryAsync(); await reopened.CloseAsync();
                Expect(state.Subject == "new external " + i && reopened.Problem is null, "Clean cache adopts newer draft");
            }
            Console.WriteLine($"Reopen iterations: 12; typed I/O events: {faults.Count}");
        });
        await Check("A temporary publication lock recovers; cancellation keeps the last durable draft", async () =>
        {
            var folder = Fixture("draft transient io"); var baseline = new CommitDraftStore(folder, identities);
            var saved = await baseline.SaveAsync(worktree, identity, null, new("before", ""), default);
            var file = Directory.GetFiles(folder, "*.json").Single(); var pause = new DraftIoIdentity();
            FileStream? held = null;
            var faults = new ConcurrentQueue<CommitDraftDiagnostic>();
            var writer = new CommitDraftStore(folder, pause, detail =>
            {
                faults.Enqueue(detail);
                Console.WriteLine($"Publication diagnostic: {detail.Operation}/{detail.Stage}/Win32={detail.NativeError}/retry={detail.Retrying}");
                Interlocked.Exchange(ref held, null)?.Dispose();
            });
            pause.Arm();
            var write = writer.SaveAsync(worktree, identity, saved.Revision, new("after", ""), default);
            try
            {
                await pause.Entered.Task.WaitAsync(TimeSpan.FromSeconds(5));
                held = new FileStream(file, FileMode.Open, FileAccess.Read, FileShare.Read); // A short external reader disallows replacement.
                pause.Release.TrySetResult();
                await write;
            }
            finally { held?.Dispose(); pause.Release.TrySetResult(); }
            Expect(faults.Any(d => d.Stage == CommitDraftStage.PublishRecord && d.NativeError == 32 && d.Retrying), "Real sharing violation captured and retried");
            var after = await baseline.LoadAsync(worktree, identity, default);
            Expect(after.Text.Subject == "after", "Replacement completed after external reader closed");
            using var cancel = new CancellationTokenSource();
            using (var locked = new FileStream(file, FileMode.Open, FileAccess.ReadWrite, FileShare.None))
            {
                var cancelledWriter = new CommitDraftStore(folder, identities, _ => cancel.Cancel());
                await ExpectException<OperationCanceledException>(() => cancelledWriter.SaveAsync(worktree, identity, after.Revision, new("cancelled", ""), cancel.Token));
            }
            Expect((await baseline.LoadAsync(worktree, identity, default)).Text.Subject == "after" && !Directory.EnumerateFiles(folder, "*.tmp").Any(), "Cancellation leaves durable text and no owned temporary files");
        });
        await Check("A retry rechecks the revision and preserves both profiles instead of overwriting the winner", async () =>
        {
            var folder = Fixture("draft retry conflict"); var other = new CommitDraftStore(folder, identities);
            var saved = await other.SaveAsync(worktree, identity, null, new("initial", ""), default);
            var file = Directory.GetFiles(folder, "*.json").Single(); var pause = new DraftIoIdentity();
            FileStream? held = null; var replaced = 0;
            var writer = new CommitDraftStore(folder, pause, detail =>
            {
                if (detail.NativeError != 32 || Interlocked.Exchange(ref replaced, 1) != 0) return;
                Interlocked.Exchange(ref held, null)?.Dispose();
                // Callback runs on the worker, after the failed attempt released its writer gate.
                other.SaveAsync(worktree, identity, saved.Revision, new("other profile", ""), default).GetAwaiter().GetResult();
            });
            var state = new CommitWorkspaceState(); var coordinator = new CommitDraftCoordinator(state, writer, worktree, identity, TimeSpan.Zero);
            await coordinator.StartOrRetryAsync(); pause.Arm(); state.Subject = "my unsaved message"; coordinator.TextChanged();
            var flush = coordinator.FlushAsync();
            try
            {
                await pause.Entered.Task.WaitAsync(TimeSpan.FromSeconds(5));
                held = new FileStream(file, FileMode.Open, FileAccess.Read, FileShare.Read); pause.Release.TrySetResult();
                await flush; await coordinator.CloseAsync();
            }
            finally { held?.Dispose(); pause.Release.TrySetResult(); }
            Expect(coordinator.Problem is { Failure: CommitDraftFailure.Changed, RecoverySaved: true }, "Changed revision becomes recoverable conflict");
            Expect((await other.LoadAsync(worktree, identity, default)).Text.Subject == "other profile", "Other profile's canonical text survives");
            var recovery = Directory.GetFiles(folder, "*.recovery.json").Single();
            using var json = JsonDocument.Parse(await File.ReadAllBytesAsync(recovery));
            Expect(json.RootElement.GetProperty("Subject").GetString() == state.Subject, "My latest text survives in its own recovery copy");
        });
        await Check("Persistent locks stop bounded retries and diagnostics contain only stage and numeric error metadata", async () =>
        {
            var folder = Fixture("draft permanent io"); var logFolder = Fixture("draft diagnostics");
            var faults = new ConcurrentQueue<CommitDraftDiagnostic>();
            await using (var log = new DiagnosticLog(logFolder))
            {
                var store = new CommitDraftStore(folder, identities, d => { faults.Enqueue(d); log.RecordDraftStorage(d); });
                var saved = await store.SaveAsync(worktree, identity, null, new("sensitive-subject", "sensitive-body"), default);
                var file = Directory.GetFiles(folder, "*.json").Single();
                using (var locked = new FileStream(file, FileMode.Open, FileAccess.ReadWrite, FileShare.None))
                {
                    try { await store.SaveAsync(worktree, identity, saved.Revision, new("unsaved-secret", ""), default); throw new InvalidOperationException("Persistent lock ignored"); }
                    catch (CommitDraftException error) { Expect(error.Diagnostic is { Stage: CommitDraftStage.ReadRecord, NativeError: 32, Retrying: false }, "Terminal error retains typed context"); }
                }
                Expect(faults.Count == 4 && faults.Count(d => d.Retrying) == 3, "Only three short retries, then report failure");
                Expect((await store.LoadAsync(worktree, identity, default)).Text.Subject == "sensitive-subject", "Old durable text retained");
                faults.Clear();
                var blocker = Path.Combine(folder, "directory-blocker"); await File.WriteAllTextAsync(blocker, "fixture");
                var badDirectory = new CommitDraftStore(Path.Combine(blocker, "child"), identities, faults.Enqueue);
                await ExpectDraftFailure(CommitDraftFailure.Unavailable, () => badDirectory.SaveAsync(worktree, identity, null, new("secret", ""), default));
                Expect(faults.Count == 1 && faults.Single() is { Stage: CommitDraftStage.OpenDirectory, Retrying: false }, "Non-sharing failures are not retried");
            }
            var lines = await File.ReadAllLinesAsync(Path.Combine(logFolder, "diagnostics.jsonl"));
            Expect(lines.Length == 4, "All bounded retry/terminal events recorded");
            foreach (var line in lines)
            {
                using var json = JsonDocument.Parse(line);
                Expect(json.RootElement.EnumerateObject().Select(p => p.Name).Order().SequenceEqual(new[] { "Draft", "Event", "Failure", "TimeUtc" }), "Allowlisted top-level fields only");
                Expect(json.RootElement.GetProperty("Draft").EnumerateObject().Select(p => p.Name).Order().SequenceEqual(new[] { "NativeError", "Operation", "Stage" }), "No content, paths, keys or exception messages");
            }
            Expect(!string.Join("", lines).Contains("secret") && !string.Join("", lines).Contains(folder), "Sensitive fixture text/paths absent");
        });
    }
    private sealed class DraftIoIdentity : IRepositoryIdentityProvider
    {
        private int _reads; private bool _armed;
        public TaskCompletionSource Entered { get; } = new(TaskCreationOptions.RunContinuationsAsynchronously);
        public TaskCompletionSource Release { get; } = new(TaskCreationOptions.RunContinuationsAsynchronously);
        public void Arm() { _reads = 0; _armed = true; }
        public async Task<RepositoryIdentity> ReadAsync(string path, CancellationToken token)
        {
            if (_armed && Interlocked.Increment(ref _reads) == 2)
            { Entered.TrySetResult(); await Release.Task.WaitAsync(token); }
            token.ThrowIfCancellationRequested(); return new(path, path, path, new string('A', 64));
        }
    }
}
