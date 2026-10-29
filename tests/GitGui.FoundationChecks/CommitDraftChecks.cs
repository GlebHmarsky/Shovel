using System.IO;
using System.Text;
using System.Text.Json;
using System.Windows.Threading;
using GitGui.App.Localization;
using GitGui.App.ViewModels;
using GitGui.Core.Drafts;
using GitGui.Core.Git;
using GitGui.Core.Processes;
using GitGui.ProcessHost;

namespace GitGui.FoundationChecks;

internal static partial class Program
{
    // Integration registers this in the existing dispatcher/isolated-config harness; not registered on this worker branch.
    private static async Task RunCommitDraftChecksAsync()
    {
        const string identity = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
        var identities = new DraftIdentityProvider();
        CommitDraftStore Store(string name) => new(Fixture(name), identities);
        CommitDraftCoordinator Coordinate(CommitWorkspaceState state, ICommitDraftStore store) =>
            new(state, store, "fixture-worktree", identity, TimeSpan.Zero);

        await Check("Draft storage reopens Unicode text only and atomically clears submitted content", async () =>
        {
            var store = Store("draft roundtrip");
            var missing = await store.LoadAsync("fixture-worktree", identity, default);
            Expect(missing.Revision is null && missing.Text.IsEmpty, "Missing draft does not invent text");
            var text = new CommitDraftText("Subject 日本語", "Body שלום\r\nsecond line\n");
            var saved = await store.SaveAsync("fixture-worktree", identity, missing.Revision, text, default);
            var reopened = await new CommitDraftStore(store.Location, identities).LoadAsync("fixture-worktree", identity, default);
            Expect(reopened.Text == text && reopened.Revision == saved.Revision, "New store restores exact subject/body");
            var file = Directory.GetFiles(store.Location, "*.json").Single();
            using (var json = JsonDocument.Parse(await File.ReadAllBytesAsync(file)))
                Expect(json.RootElement.EnumerateObject().Select(p => p.Name).Order().SequenceEqual(
                    new[] { "SchemaVersion", "Key", "Revision", "Subject", "Description" }.Order()), "Only identity key, version and editor text are persisted");
            var empty = await store.SaveAsync("fixture-worktree", identity, reopened.Revision, CommitDraftText.Empty, default);
            Expect(empty.Revision != saved.Revision && (await store.LoadAsync("fixture-worktree", identity, default)).Text.IsEmpty, "Clear keeps a fresh empty revision");
            using (var cleared = JsonDocument.Parse(await File.ReadAllBytesAsync(file)))
                Expect(cleared.RootElement.GetProperty("Subject").GetString() == "" && cleared.RootElement.GetProperty("Description").GetString() == "", "Canonical record no longer contains submitted message");
            await ExpectDraftFailure(CommitDraftFailure.Changed, () => store.SaveAsync("fixture-worktree", identity, null, text, default));
            Expect(!Directory.EnumerateFiles(store.Location, "*.tmp").Any(), "Normal replacement leaves no temporary text files");
        });
        await Check("Two profiles cannot replace the same observed draft revision", async () =>
        {
            var store = Store("draft profiles");
            var first = new CommitDraftStore(store.Location, identities);
            var second = new CommitDraftStore(store.Location, identities);
            async Task<bool> Save(ICommitDraftStore writer, string subject)
            {
                try { await writer.SaveAsync("fixture-worktree", identity, null, new(subject, ""), default); return true; }
                catch (CommitDraftException error) when (error.Failure == CommitDraftFailure.Changed) { return false; }
            }
            var winners = await Task.WhenAll(Save(first, "profile one"), Save(second, "profile two"));
            Expect(winners.Count(value => value) == 1, "Exactly one writer owns a revision");
            var current = await store.LoadAsync("fixture-worktree", identity, default);
            await ExpectDraftFailure(CommitDraftFailure.Changed, () => second.SaveAsync("fixture-worktree", identity, null, new("stale", ""), default));
            Expect((await store.LoadAsync("fixture-worktree", identity, default)).Text == current.Text, "Stale update preserves winner");
        });
        foreach (var (name, bytes, failure) in new[]
        {
            ("broken", "{", CommitDraftFailure.Invalid),
            ("array", "[]", CommitDraftFailure.Invalid),
            ("typed schema", "{\"SchemaVersion\":\"one\"}", CommitDraftFailure.Invalid),
            ("newer", "{\"SchemaVersion\":99}", CommitDraftFailure.NewerSchema),
            ("missing fields", "{\"SchemaVersion\":1}", CommitDraftFailure.Invalid),
            ("oversized", new string(' ', CommitDraftStore.MaximumFileBytes + 1), CommitDraftFailure.Invalid)
        })
        {
            await Check("Unknown draft bytes survive load/save: " + name, async () =>
            {
                var store = Store("draft invalid " + name);
                var saved = await store.SaveAsync("fixture-worktree", identity, null, new("original", ""), default);
                var file = Directory.GetFiles(store.Location, "*.json").Single();
                await File.WriteAllTextAsync(file, bytes);
                var before = await File.ReadAllBytesAsync(file);
                await ExpectDraftFailure(failure, () => store.LoadAsync("fixture-worktree", identity, default));
                await ExpectDraftFailure(failure, () => store.SaveAsync("fixture-worktree", identity, saved.Revision, new("replacement", ""), default));
                Expect(before.SequenceEqual(await File.ReadAllBytesAsync(file)), "Unreadable/newer bytes are never overwritten");
            });
        }
        await Check("Cancellation after temporary-file flush preserves canonical bytes and removes only the owned temporary file", async () =>
        {
            var store = Store("draft atomic cancellation");
            var saved = await store.SaveAsync("fixture-worktree", identity, null, new("durable original", ""), default);
            var file = Directory.GetFiles(store.Location, "*.json").Single();
            var before = await File.ReadAllBytesAsync(file);
            var gate = new DraftPublicationIdentity();
            var writer = new CommitDraftStore(store.Location, gate);
            using var cancel = new CancellationTokenSource();
            var write = writer.SaveAsync("fixture-worktree", identity, saved.Revision, new("must not publish", ""), cancel.Token);
            await gate.BeforePublish.Task.WaitAsync(TimeSpan.FromSeconds(10));
            Expect(Directory.EnumerateFiles(store.Location, "*.tmp").Count() == 1, "Fixture reached the flushed-before-replace boundary");
            cancel.Cancel(); await ExpectException<OperationCanceledException>(() => write);
            Expect(before.SequenceEqual(await File.ReadAllBytesAsync(file)) && !Directory.EnumerateFiles(store.Location, "*.tmp").Any(),
                "Interrupted publication keeps old complete record and cleans its temporary text");
        });
        await Check("Locked draft and writer gate fail/cancel without replacing saved text", async () =>
        {
            var store = Store("draft locks");
            var saved = await store.SaveAsync("fixture-worktree", identity, null, new("kept", ""), default);
            var file = Directory.GetFiles(store.Location, "*.json").Single();
            var before = await File.ReadAllBytesAsync(file);
            using (var locked = new FileStream(file, FileMode.Open, FileAccess.ReadWrite, FileShare.None))
            {
                await ExpectDraftFailure(CommitDraftFailure.Unavailable, () => store.LoadAsync("fixture-worktree", identity, default));
                await ExpectDraftFailure(CommitDraftFailure.Unavailable, () => store.SaveAsync("fixture-worktree", identity, saved.Revision, new("blocked", ""), default));
            }
            var lockFile = Directory.GetFiles(store.Location, "*.lock").Single();
            using (var locked = new FileStream(lockFile, FileMode.Open, FileAccess.ReadWrite, FileShare.None))
            using (var cancel = new CancellationTokenSource(TimeSpan.FromMilliseconds(100)))
                await ExpectException<OperationCanceledException>(() => store.SaveAsync("fixture-worktree", identity, saved.Revision, new("cancelled", ""), cancel.Token));
            Expect(before.SequenceEqual(await File.ReadAllBytesAsync(file)), "Lock/cancellation preserves last durable bytes");
            await store.SaveAsync("fixture-worktree", identity, saved.Revision, new("retry succeeds", ""), default);
        });
        await Check("Unavailable directory and size bounds retain the previous draft", async () =>
        {
            var folder = Fixture("draft permissions surrogate"); var blocked = Path.Combine(folder, "ordinary-file");
            await File.WriteAllTextAsync(blocked, "fixture");
            var invalid = new CommitDraftStore(Path.Combine(blocked, "child"), identities);
            await ExpectDraftFailure(CommitDraftFailure.Unavailable, () => invalid.SaveAsync("fixture-worktree", identity, null, new("unsaved", ""), default));
            var store = Store("draft limits");
            var saved = await store.SaveAsync("fixture-worktree", identity, null, new("kept", ""), default);
            await ExpectDraftFailure(CommitDraftFailure.TooLarge, () => store.SaveAsync("fixture-worktree", identity, saved.Revision,
                new(new string('x', CommitDraftStore.MaximumSubjectCharacters + 1), ""), default));
            await ExpectDraftFailure(CommitDraftFailure.Invalid, () => store.SaveAsync("fixture-worktree", identity, saved.Revision, new("\ud800", ""), default));
            Expect((await store.LoadAsync("fixture-worktree", identity, default)).Text.Subject == "kept", "Bad edits do not truncate valid storage");
        });
        await Check("Modal draft construction performs no I/O; close and a fresh state restore text", async () =>
        {
            var gate = new DraftGateStore(Store("draft lifetime"));
            var state = new CommitWorkspaceState { Subject = "before visible", Description = "body", LastOutput = "not-a-draft-secret" };
            var first = Coordinate(state, gate);
            Expect(gate.LoadCalls == 0 && gate.SaveCalls == 0, "Construction is preparation-free");
            first.TextChanged(); await first.StartOrRetryAsync(); await first.CloseAsync(); await first.CloseAsync();
            var reopened = new CommitWorkspaceState(); var next = Coordinate(reopened, gate);
            await next.StartOrRetryAsync();
            Expect(reopened.Subject == "before visible" && reopened.Description == "body", "New tab/process state restores persisted editor text");
            Expect(!string.Join("", Directory.GetFiles(gate.Location, "*.json").Select(File.ReadAllText)).Contains("not-a-draft-secret"), "Output is not draft data");
            await next.CloseAsync();
        });
        await Check("A delayed load cannot replace newly typed text or resurrect a committed draft", async () =>
        {
            var store = Store("draft delayed load");
            await store.SaveAsync("fixture-worktree", identity, null, new("saved older", "body"), default);
            var gate = new DraftGateStore(store) { LoadGate = new() };
            var held = gate.LoadGate!;
            var state = new CommitWorkspaceState(); var draft = Coordinate(state, gate);
            var load = draft.StartOrRetryAsync(); await held.Started.Task.WaitAsync(TimeSpan.FromSeconds(10));
            state.Subject = "saved older"; state.Description = "body"; draft.TextChanged();
            var submitted = draft.CaptureSubmission();
            Expect(draft.CommitSucceeded(submitted), "Current submitted draft clears");
            held.Release.TrySetResult(); await load; await draft.CloseAsync();
            Expect(state.Subject.Length == 0 && (await store.LoadAsync("fixture-worktree", identity, default)).Text.IsEmpty, "Old loaded message never returns after commit");
        });
        await Check("Typing before restore preserves both unseen saved text and the newer editor copy", async () =>
        {
            var store = Store("draft early typing");
            await store.SaveAsync("fixture-worktree", identity, null, new("unseen saved draft", ""), default);
            var gate = new DraftGateStore(store) { LoadGate = new() }; var held = gate.LoadGate!;
            var state = new CommitWorkspaceState(); var draft = Coordinate(state, gate);
            var load = draft.StartOrRetryAsync(); await held.Started.Task.WaitAsync(TimeSpan.FromSeconds(10));
            state.Subject = "new typed draft"; draft.TextChanged();
            held.Release.TrySetResult(); await load; await draft.CloseAsync();
            Expect(state.Subject == "new typed draft" && (await store.LoadAsync("fixture-worktree", identity, default)).Text.Subject == "unseen saved draft",
                "Neither the newly typed message nor unseen durable message is overwritten");
            var copy = Directory.GetFiles(store.Location, "*.recovery.json").Single();
            using var json = JsonDocument.Parse(await File.ReadAllBytesAsync(copy));
            Expect(json.RootElement.GetProperty("Subject").GetString() == "new typed draft", "Early edit is durable as an independent recovery copy");
        });
        await Check("Serialized writes preserve edits made while an older save completes", async () =>
        {
            var store = Store("draft delayed save");
            var gate = new DraftGateStore(store) { SaveGate = new() }; var held = gate.SaveGate!;
            var state = new CommitWorkspaceState { Subject = "old" }; var draft = Coordinate(state, gate);
            draft.TextChanged(); await draft.StartOrRetryAsync(); await held.Started.Task.WaitAsync(TimeSpan.FromSeconds(10));
            state.Subject = "new"; state.Description = "latest body"; draft.TextChanged();
            held.Release.TrySetResult(); await draft.CloseAsync();
            Expect(state.Subject == "new" && (await store.LoadAsync("fixture-worktree", identity, default)).Text == new CommitDraftText("new", "latest body") &&
                gate.MaximumActiveSaves == 1, "New text wins with one writer and no obsolete text publication");
        });
        await Check("A delayed successful-clear write preserves a newer editor draft", async () =>
        {
            var store = Store("draft delayed clear"); var gate = new DraftGateStore(store);
            var state = new CommitWorkspaceState { Subject = "submitted" }; var draft = Coordinate(state, gate);
            draft.TextChanged(); await draft.StartOrRetryAsync(); await draft.FlushAsync();
            var submitted = draft.CaptureSubmission(); gate.SaveGate = new(); var held = gate.SaveGate!;
            Expect(draft.CommitSucceeded(submitted), "Submitted text clears immediately");
            await held.Started.Task.WaitAsync(TimeSpan.FromSeconds(10));
            state.Subject = "next commit"; draft.TextChanged();
            held.Release.TrySetResult(); await draft.CloseAsync();
            Expect(state.Subject == "next commit" && (await store.LoadAsync("fixture-worktree", identity, default)).Text.Subject == "next commit", "Late tombstone cannot erase later text");
        });
        await Check("Cross-profile conflict saves a separate recovery copy and warns on restart", async () =>
        {
            var store = Store("draft conflict recovery");
            var state = new CommitWorkspaceState(); var draft = Coordinate(state, store);
            await draft.StartOrRetryAsync();
            await store.SaveAsync("fixture-worktree", identity, null, new("other profile", ""), default);
            state.Subject = "this profile"; draft.TextChanged(); await draft.CloseAsync();
            Expect(draft.Problem is { Failure: CommitDraftFailure.Changed, RecoverySaved: true }, "Conflict is visible and locally recoverable");
            Expect((await store.LoadAsync("fixture-worktree", identity, default)).Text.Subject == "other profile", "Other profile is not overwritten");
            var recovery = Directory.GetFiles(store.Location, "*.recovery.json").Single();
            using var json = JsonDocument.Parse(await File.ReadAllBytesAsync(recovery));
            Expect(json.RootElement.GetProperty("Subject").GetString() == "this profile", "Conflicting text survives close");
            var reopened = new CommitWorkspaceState(); var next = Coordinate(reopened, store); await next.StartOrRetryAsync();
            Expect(reopened.Subject == "other profile" && next.Problem is { Failure: null }, "Restart restores primary and reports alternate copies");
            await next.CloseAsync();
        });
        await Check("Reopen of clean cached state adopts a newer profile draft without overwriting it", async () =>
        {
            var store = Store("draft clean reopen");
            var state = new CommitWorkspaceState { Subject = "old" }; var first = Coordinate(state, store);
            first.TextChanged(); await first.StartOrRetryAsync(); await first.CloseAsync();
            var previous = await store.LoadAsync("fixture-worktree", identity, default);
            await store.SaveAsync("fixture-worktree", identity, previous.Revision, new("new external", ""), default);
            var reopened = Coordinate(state, store); await reopened.StartOrRetryAsync(); await reopened.CloseAsync();
            Expect(state.Subject == "new external" && (await store.LoadAsync("fixture-worktree", identity, default)).Text.Subject == "new external", "Clean session cache does not overwrite a newer persisted revision");
        });
        await Check("Transient save failure retains text and explicit Refresh retries without a Git dependency", async () =>
        {
            var store = Store("draft retry");
            var gate = new DraftGateStore(store) { FailSaves = true };
            var state = new CommitWorkspaceState { Subject = "retry me" }; var draft = Coordinate(state, gate);
            draft.TextChanged(); await draft.StartOrRetryAsync(); await draft.FlushAsync();
            Expect(draft.Problem is { RecoverySaved: false } && state.Subject == "retry me", "Storage failure is visible and text remains");
            gate.FailSaves = false; await draft.StartOrRetryAsync(); await draft.CloseAsync();
            Expect((await store.LoadAsync("fixture-worktree", identity, default)).Text.Subject == "retry me" && draft.Problem is null, "Retry recovers ordinary persistence");
        });
        await Check("Commit success acknowledges only the submitted version; failure leaves the draft", async () =>
        {
            var store = Store("draft submission generation");
            var state = new CommitWorkspaceState { Subject = "first" }; var draft = Coordinate(state, store);
            draft.TextChanged(); await draft.StartOrRetryAsync(); await draft.FlushAsync();
            var submitted = draft.CaptureSubmission(); // Failed/cancelled commit intentionally has no acknowledgement.
            await draft.FlushAsync();
            Expect((await store.LoadAsync("fixture-worktree", identity, default)).Text.Subject == "first", "Failure/cancellation retains submitted text");
            state.Subject = "newer"; draft.TextChanged();
            Expect(!draft.CommitSucceeded(submitted), "Stale success never clears newer text");
            await draft.CloseAsync();
            Expect((await store.LoadAsync("fixture-worktree", identity, default)).Text.Subject == "newer", "Newer text is durable");
        });
        await Check("Burst edits coalesce into one close-time save and no work survives closure", async () =>
        {
            var gate = new DraftGateStore(Store("draft debounce"));
            var state = new CommitWorkspaceState();
            var draft = new CommitDraftCoordinator(state, gate, "fixture-worktree", identity, TimeSpan.FromSeconds(10));
            await draft.StartOrRetryAsync();
            for (var i = 0; i < 100; i++) { state.Subject = "edit " + i; draft.TextChanged(); }
            await draft.CloseAsync();
            Expect(gate.SaveCalls == 1 && (await gate.LoadAsync("fixture-worktree", identity, default)).Text.Subject == "edit 99", "Close bypasses debounce and writes only the latest snapshot");
            var calls = gate.SaveCalls; draft.TextChanged(); await draft.StartOrRetryAsync(); await draft.FlushAsync();
            Expect(gate.SaveCalls == calls, "Closed coordinator admits no late operations");
        });
        await RunCommitDraftRepositoryChecksAsync();
    }

    private static async Task ExpectDraftFailure(CommitDraftFailure expected, Func<Task> action)
    {
        try { await action(); }
        catch (CommitDraftException error) when (error.Failure == expected) { return; }
        throw new InvalidOperationException("Expected typed draft failure: " + expected);
    }
    private sealed class DraftIdentityProvider : IRepositoryIdentityProvider
    {
        public Task<RepositoryIdentity> ReadAsync(string path, CancellationToken token)
        {
            token.ThrowIfCancellationRequested();
            return Task.FromResult(new RepositoryIdentity(path, path, path, new string('A', 64)));
        }
    }
    private sealed class DraftPublicationIdentity : IRepositoryIdentityProvider
    {
        private int _reads;
        public TaskCompletionSource BeforePublish { get; } = new(TaskCreationOptions.RunContinuationsAsynchronously);
        public async Task<RepositoryIdentity> ReadAsync(string path, CancellationToken token)
        {
            if (Interlocked.Increment(ref _reads) == 2)
            {
                BeforePublish.TrySetResult();
                await Task.Delay(Timeout.InfiniteTimeSpan, token);
            }
            token.ThrowIfCancellationRequested();
            return new(path, path, path, new string('A', 64));
        }
    }
    private sealed class DraftIoGate
    {
        public TaskCompletionSource Started { get; } = new(TaskCreationOptions.RunContinuationsAsynchronously);
        public TaskCompletionSource Release { get; } = new(TaskCreationOptions.RunContinuationsAsynchronously);
        public async Task WaitAsync() { Started.TrySetResult(); await Release.Task.WaitAsync(TimeSpan.FromSeconds(10)); }
    }
    private sealed class DraftGateStore(ICommitDraftStore inner) : ICommitDraftStore
    {
        public string Location => inner.Location;
        public DraftIoGate? LoadGate, SaveGate;
        public bool FailSaves;
        public int LoadCalls, SaveCalls, MaximumActiveSaves;
        private int _active;
        public async Task<CommitDraftSnapshot> LoadAsync(string root, string identity, CancellationToken token)
        {
            LoadCalls++;
            var result = await inner.LoadAsync(root, identity, token);
            if (LoadGate is { } gate) { LoadGate = null; await gate.WaitAsync(); }
            return result;
        }
        public async Task<CommitDraftSnapshot> SaveAsync(string root, string identity, string? revision, CommitDraftText text, CancellationToken token)
        {
            SaveCalls++; MaximumActiveSaves = Math.Max(MaximumActiveSaves, ++_active);
            try
            {
                if (SaveGate is { } gate) { SaveGate = null; await gate.WaitAsync(); }
                if (FailSaves) throw new CommitDraftException(CommitDraftFailure.Unavailable);
                return await inner.SaveAsync(root, identity, revision, text, token);
            }
            finally { _active--; }
        }
        public Task<bool> SaveRecoveryAsync(string root, string identity, string copyId, CommitDraftText text, CancellationToken token) =>
            inner.SaveRecoveryAsync(root, identity, copyId, text, token);
    }
}


internal static partial class Program
{
    private static async Task RunCommitDraftRepositoryChecksAsync()
    {
        var observed = new DraftObservedRunner(new WindowsProcessRunner());
        var reader = new RepositoryReader(observed); var identities = new WindowsRepositoryIdentity();
        var actions = new GitActions(observed, reader, identities); var git = await reader.DiscoverAsync(null, default);
        var repo = Fixture("draft real commit"); var store = new CommitDraftStore(Fixture("draft real storage"), identities);
        async Task<string> Git(string directory, params string[] args)
        {
            var result = await observed.RunAsync(new(git.Executable, directory, args), default);
            Expect(result.ExitCode == 0, "Draft fixture Git command failed");
            return Encoding.UTF8.GetString(result.StandardOutput).Trim();
        }
        async Task Initialize(string target)
        {
            await Git(target, "init", "-b", "main");
            await Git(target, "config", "user.name", "Draft Fixture");
            await Git(target, "config", "user.email", "draft@example.invalid");
            await Git(target, "config", "core.autocrlf", "false");
        }
        await Initialize(repo);
        await File.WriteAllTextAsync(Path.Combine(repo, "file.txt"), "base\n"); await Git(repo, "add", ".");
        var identity = await identities.ReadAsync(repo, default);
        CommitViewModel Editor(ICommitDraftStore? source = null, CommitWorkspaceState? state = null) =>
            new(repo, git, reader, actions, new CommitInteraction(), state, identity.Fingerprint, source ?? store);
        var hook = Path.Combine(repo, ".git", "hooks", "pre-commit");

        await Check("Bound failed commit retains durable draft and bound success clears it before closing", async () =>
        {
            var gate = new DraftGateStore(store);
            var before = observed.Calls; var editor = Editor(gate);
            var window = new GitGui.App.Views.CommitWindow(editor);
            Expect(observed.Calls == before && gate.LoadCalls == 0 && editor.CommitTarget == UiText.Get("CommitDraftPersistenceHelp"), "Modal construction explains persistence without initiating Git/draft work");
            editor.Subject = "retained failed subject"; editor.Description = "exact body שלום\nsecond line";
            await editor.StartLoading(); await editor.DraftLoadingTask; await editor.DiffTask;
            await File.WriteAllTextAsync(hook, "#!/bin/sh\nexit 1\n", new UTF8Encoding(false));
            Expect(editor.CommitCommand.CanExecute(null), "Real staged commit is ready");
            editor.CommitCommand.Execute(null); await editor.OperationTask;
            Expect(editor.Subject == "retained failed subject" && (await reader.InspectAsync(git, repo, default)).Commit is null, "Failed hook leaves message and unborn state");
            await editor.CloseAsync();
            Expect((await store.LoadAsync(repo, identity.Fingerprint, default)).Text.Description == "exact body שלום\nsecond line", "Failure persists complete body");

            window.Close(); await Dispatcher.Yield(DispatcherPriority.Background);
            File.Delete(hook); editor = Editor(); await editor.StartLoading(); await editor.DraftLoadingTask;
            Expect(editor.Subject == "retained failed subject", "Fresh editor restores failure draft");
            editor.Subject = "successful submitted draft";
            editor.CommitCommand.Execute(null); await editor.OperationTask;
            Expect(editor.Subject.Length == 0 && (await store.LoadAsync(repo, identity.Fingerprint, default)).Text.IsEmpty, "Success clears the durable submitted draft before modal close");
            Expect((await Git(repo, "log", "-1", "--format=%s")) == "successful submitted draft", "Real Git commit, not a mocked result");
            await editor.CloseAsync();
        });
        await Check("Bound cancelled commit retains persisted text and staged changes", async () =>
        {
            await File.WriteAllTextAsync(Path.Combine(repo, "file.txt"), "after base\n"); await Git(repo, "add", ".");
            var tip = await Git(repo, "rev-parse", "HEAD");
            await File.WriteAllTextAsync(hook, "#!/bin/sh\necho draft-cancel-ready >&2\nsleep 30\n", new UTF8Encoding(false));
            var editor = Editor(); await editor.StartLoading(); await editor.DraftLoadingTask;
            editor.Subject = "cancelled draft"; editor.CommitCommand.Execute(null);
            await observed.HookStarted.Task.WaitAsync(TimeSpan.FromSeconds(10)); editor.Cancel();
            await editor.OperationTask.WaitAsync(TimeSpan.FromSeconds(15)); await editor.CloseAsync();
            Expect(await Git(repo, "rev-parse", "HEAD") == tip && (await store.LoadAsync(repo, identity.Fingerprint, default)).Text.Subject == "cancelled draft",
                "Cancellation leaves tip unchanged and draft durable");
            Expect((await reader.ReadLocalChangesAsync(git, repo, false, default)).Entries.Any(row => row.Group == ChangeGroup.Staged), "Staged work is preserved");
            File.Delete(hook);
        });
        await Check("Closing a repository tab and creating a new shell restores its draft after modal display", async () =>
        {
            var interaction = new CommitInteraction();
            var shell = new ShellViewModel(reader, interaction, identities, actions: actions) { DraftStore = store };
            shell.CompleteStartup(new(new() { RepositoryPaths = [repo], SelectedRepositoryPath = repo }, GitGui.Core.Settings.SettingsLoadStatus.Loaded));
            await shell.WaitForIdleAsync();
            interaction.OnOpen = async editor =>
            {
                await editor.StartLoading(); await editor.DraftLoadingTask;
                editor.Subject = "tab closed draft"; editor.Description = "tab body";
            };
            shell.ShowChangesCommand.Execute(null); await shell.CommitDialogTask;
            shell.CloseRepositoryCommand.Execute(null); await shell.WaitForIdleAsync();
            Expect(shell.Repositories.Count == 0, "Tab really closed");
            shell.BeginShutdown(); await shell.WaitForIdleAsync();
            var restarted = new ShellViewModel(reader, interaction, identities, actions: actions) { DraftStore = store };
            restarted.CompleteStartup(new(new() { RepositoryPaths = [repo], SelectedRepositoryPath = repo }, GitGui.Core.Settings.SettingsLoadStatus.Loaded));
            await restarted.WaitForIdleAsync();
            interaction.OnOpen = async editor =>
            {
                Expect(editor.Subject.Length == 0, "New shell does not preload disk draft into history/modal construction");
                await editor.StartLoading(); await editor.DraftLoadingTask;
                Expect(editor.Subject == "tab closed draft" && editor.Description == "tab body", "Post-display restore survives tab/app state replacement");
            };
            restarted.ShowChangesCommand.Execute(null); await restarted.CommitDialogTask;
            restarted.BeginShutdown(); await restarted.WaitForIdleAsync();
        });
        await Check("Linked worktrees have independent draft identity keys", async () =>
        {
            var rootDraft = await store.LoadAsync(repo, identity.Fingerprint, default);
            await store.SaveAsync(repo, identity.Fingerprint, rootDraft.Revision, new("root worktree", ""), default);
            var linked = Path.Combine(Fixture("draft linked parent"), "linked");
            await Git(repo, "worktree", "add", "-b", "draft-linked", linked);
            var linkedIdentity = await identities.ReadAsync(linked, default);
            Expect(linkedIdentity.CommonDirectory == identity.CommonDirectory && linkedIdentity.Fingerprint != identity.Fingerprint, "Real linked identities share common Git but not worktree identity");
            Expect((await store.LoadAsync(linked, linkedIdentity.Fingerprint, default)).Text.IsEmpty, "Linked worktree never inherits root draft");
            await store.SaveAsync(linked, linkedIdentity.Fingerprint, null, new("linked worktree", ""), default);
            Expect((await store.LoadAsync(repo, identity.Fingerprint, default)).Text.Subject == "root worktree" &&
                (await store.LoadAsync(linked, linkedIdentity.Fingerprint, default)).Text.Subject == "linked worktree", "Independent durable records");
        });
        await Check("Replaced metadata at the same path cannot inherit saved or cached draft text", async () =>
        {
            var target = Fixture("draft replaced repo"); await Initialize(target);
            var before = await identities.ReadAsync(target, default);
            var savedStore = new CommitDraftStore(Fixture("draft replaced storage"), identities);
            await savedStore.SaveAsync(target, before.Fingerprint, null, new("original repository", ""), default);
            var cached = new CommitWorkspaceState { Subject = "cached original" };
            _ = new CommitDraftCoordinator(cached, savedStore, target, before.Fingerprint);
            // Both paths are known children of this disposable fixture; preserve old metadata.
            Directory.Move(Path.Combine(target, ".git"), Path.Combine(target, "old-metadata"));
            await Git(target, "init", "-b", "replacement");
            var after = await identities.ReadAsync(target, default);
            Expect(before.Fingerprint != after.Fingerprint, "Physical metadata replacement changes existing identity");
            await ExpectDraftFailure(CommitDraftFailure.IdentityChanged, () => savedStore.LoadAsync(target, before.Fingerprint, default));
            await ExpectDraftFailure(CommitDraftFailure.IdentityChanged, () => savedStore.SaveAsync(target, before.Fingerprint, null, new("stale", ""), default));
            var next = new CommitDraftCoordinator(cached, savedStore, target, after.Fingerprint);
            Expect(cached.Subject.Length == 0, "Cached text is cleared by replacement binding before it is displayed");
            await next.StartOrRetryAsync(); await next.CloseAsync();
            Expect((await savedStore.LoadAsync(target, after.Fingerprint, default)).Text.IsEmpty, "Replacement has no draft");
        });
        await Check("A delayed saved draft wins over the default message of a pending merge", async () =>
        {
            var target = Fixture("draft merge restore"); await Initialize(target);
            var file = Path.Combine(target, "conflict.txt");
            await File.WriteAllTextAsync(file, "base\n"); await Git(target, "add", "."); await Git(target, "commit", "-m", "base");
            await Git(target, "switch", "-c", "side"); await File.WriteAllTextAsync(file, "side\n"); await Git(target, "commit", "-am", "side");
            await Git(target, "switch", "main"); await File.WriteAllTextAsync(file, "main\n"); await Git(target, "commit", "-am", "main");
            var conflict = await observed.RunAsync(new(git.Executable, target, ["merge", "side"]), default);
            Expect(conflict.ExitCode != 0, "Fixture has a pending conflicted merge");
            await File.WriteAllTextAsync(file, "resolved\n"); await Git(target, "add", ".");
            var mergeIdentity = await identities.ReadAsync(target, default);
            var mergeStore = new CommitDraftStore(Fixture("draft merge storage"), identities);
            await mergeStore.SaveAsync(target, mergeIdentity.Fingerprint, null, new("my merge subject", "my merge body"), default);
            var gate = new DraftGateStore(mergeStore) { LoadGate = new() }; var held = gate.LoadGate!;
            var editor = new CommitViewModel(target, git, reader, actions, new CommitInteraction(), identity: mergeIdentity.Fingerprint, draftStore: gate);
            var loading = editor.StartLoading(); await held.Started.Task.WaitAsync(TimeSpan.FromSeconds(10)); await loading;
            Expect(editor.Subject.Length == 0, "Default merge text does not race an unfinished restore");
            held.Release.TrySetResult(); await editor.DraftLoadingTask; await Dispatcher.Yield(DispatcherPriority.Background);
            Expect(editor.Subject == "my merge subject" && editor.Description == "my merge body", "Saved draft wins regardless of scan completion order");
            await editor.CloseAsync();
        });
    }
    private sealed class DraftObservedRunner(IProcessRunner inner) : IProcessRunner
    {
        public int Calls;
        public TaskCompletionSource HookStarted { get; } = new(TaskCreationOptions.RunContinuationsAsynchronously);
        public Task<ProcessResult> RunAsync(ProcessRequest request, CancellationToken token, Action<ProcessOutput>? output = null)
        {
            Interlocked.Increment(ref Calls);
            return inner.RunAsync(request, token, chunk =>
            {
                if (Encoding.UTF8.GetString(chunk.Bytes).Contains("draft-cancel-ready", StringComparison.Ordinal)) HookStarted.TrySetResult();
                output?.Invoke(chunk);
            });
        }
    }
}
