using System.IO;
using System.Diagnostics;
using System.Windows.Threading;
using GitGui.Core.Drafts;

namespace GitGui.App.ViewModels;

// Subject/Description in CommitWorkspaceState remain the only mutable editor text.
public sealed partial class CommitWorkspaceState
{
    internal string? DraftIdentity { get; set; }
    internal string? DraftRevision { get; set; }
    internal bool DraftObserved { get; set; }
    internal bool DraftDirty { get; set; }
    internal long DraftGeneration { get; set; }
    internal string DraftRecoveryId { get; } = Guid.NewGuid().ToString("N");
    internal bool DraftRecoverySaved { get; set; }
}
public sealed record CommitDraftSubmission(long Generation, CommitDraftText Text);
public sealed record CommitDraftProblem(CommitDraftFailure? Failure, bool RecoverySaved, string Location);

/// <summary>Dispatcher-owned lifetime, one load and one coalescing writer. No Git or logging.</summary>
public sealed class CommitDraftCoordinator
{
    private readonly CommitWorkspaceState _state;
    private readonly ICommitDraftStore _store;
    private readonly string _worktree, _identity;
    private readonly TimeSpan _debounce, _maximumDelay;
    private Task _load = Task.CompletedTask, _writer = Task.CompletedTask;
    private Task? _close;
    private CancellationTokenSource? _delay;
    private bool _started, _ready, _pending, _flush, _closed, _recoveryOnly, _otherCopies;
    private long _firstEdit, _lastEdit;
    private CommitDraftFailure? _failure;
    private CommitDraftText? _submittedBeforeObservation;
    public CommitDraftProblem? Problem { get; private set; }
    public Task LoadingTask => _load;
    public Task WritingTask => _writer;
    public event EventHandler? TextRestored;
    public event EventHandler? ProblemChanged;

    public CommitDraftCoordinator(CommitWorkspaceState state, ICommitDraftStore store, string worktree, string identity,
        TimeSpan? debounce = null, TimeSpan? maximumDelay = null)
    {
        _state = state; _store = store; _worktree = worktree; _identity = identity;
        _debounce = debounce ?? TimeSpan.FromMilliseconds(600);
        _maximumDelay = maximumDelay ?? TimeSpan.FromSeconds(2);
        // Pure in-memory binding, before display. Cached text cannot cross a replacement identity.
        if (state.DraftIdentity is not null && state.DraftIdentity != identity)
        {
            state.Subject = state.Description = ""; state.DraftRevision = null;
            state.DraftObserved = state.DraftDirty = state.DraftRecoverySaved = false; state.DraftGeneration++;
        }
        state.DraftIdentity = identity;
    }
    private CommitDraftText Text => new(_state.Subject, _state.Description);
    public CommitDraftSubmission CaptureSubmission() => new(_state.DraftGeneration, Text);
    public bool CommitSucceeded(CommitDraftSubmission submitted)
    {
        if (_closed || submitted.Generation != _state.DraftGeneration || submitted.Text != Text) return false;
        if (!_state.DraftObserved) _submittedBeforeObservation = submitted.Text;
        _state.Subject = _state.Description = ""; TextChanged(); TextRestored?.Invoke(this, EventArgs.Empty);
        return true;
    }
    public void TextChanged()
    {
        if (_closed) return;
        _state.DraftGeneration++; _state.DraftDirty = true;
        var now = Stopwatch.GetTimestamp();
        if (!_pending) _firstEdit = now;
        _lastEdit = now; _pending = true;
        Schedule();
    }
    // Called by the modal's existing post-display load/Refresh route, never its constructor/history.
    public Task StartOrRetryAsync()
    {
        if (_closed || !_load.IsCompleted || !_writer.IsCompleted) return _load;
        if (_started && Problem is null) return _load;
        _started = true; _ready = false; _load = LoadAsync();
        return _load;
    }
    private async Task LoadAsync()
    {
        await Dispatcher.Yield(DispatcherPriority.Background);
        var generation = _state.DraftGeneration;
        try
        {
            using var timeout = new CancellationTokenSource(TimeSpan.FromSeconds(5));
            var loaded = await _store.LoadAsync(_worktree, _identity, timeout.Token);
            if (_closed) return;
            _otherCopies = loaded.HasRecoveryCopies;
            // Reopening/refreshing must not silently rebase unsaved text onto another profile's revision.
            if (_state.DraftObserved && loaded.Revision != _state.DraftRevision && _state.DraftDirty)
                throw new CommitDraftException(CommitDraftFailure.Changed);
            // If typing/commit finished before the first restore, an unseen different draft must
            // survive too. Only an exact submitted message is eligible for that early clear.
            if (!_state.DraftObserved && (_state.DraftDirty || !Text.IsEmpty) && !loaded.Text.IsEmpty &&
                loaded.Text != Text && loaded.Text != _submittedBeforeObservation)
                throw new CommitDraftException(CommitDraftFailure.Changed);
            var previouslyObserved = _state.DraftObserved;
            _state.DraftRevision = loaded.Revision; _state.DraftObserved = true;
            if (!_state.DraftDirty && generation == _state.DraftGeneration)
            {
                if (previouslyObserved || Text.IsEmpty || Text == loaded.Text)
                { _state.Subject = loaded.Text.Subject; _state.Description = loaded.Text.Description; TextRestored?.Invoke(this, EventArgs.Empty); }
                else
                { _state.DraftDirty = true; _pending = true; }
            }
            _recoveryOnly = false; _failure = null;
            SetProblem(_otherCopies ? new(null, _state.DraftRecoverySaved, _store.Location) : null);
        }
        catch (Exception error) when (IsStorageFailure(error))
        {
            if (_closed) return;
            _failure = Failure(error); _recoveryOnly = true;
            SetProblem(new(_failure, _state.DraftRecoverySaved, _store.Location));
        }
        finally
        {
            if (!_closed)
            {
                _ready = true;
                if (_state.DraftDirty) { _pending = true; _firstEdit = _lastEdit = Stopwatch.GetTimestamp(); }
                Schedule();
            }
        }
    }
    private void Schedule()
    {
        if (!_closed && _ready && _pending && _writer.IsCompleted) _writer = WriteAsync();
    }
    private async Task WriteAsync()
    {
        await Dispatcher.Yield(DispatcherPriority.Background);
        while (_pending && !_closed)
        {
            // Trailing debounce, with a maximum wait under continuous typing. Only one delay/writer.
            while (!_flush)
            {
                var now = Stopwatch.GetTimestamp();
                var remaining = _debounce - Stopwatch.GetElapsedTime(_lastEdit, now);
                var maximum = _maximumDelay - Stopwatch.GetElapsedTime(_firstEdit, now);
                if (remaining <= TimeSpan.Zero || maximum <= TimeSpan.Zero) break;
                using var delay = new CancellationTokenSource(); _delay = delay;
                try { await Task.Delay(remaining < maximum ? remaining : maximum, delay.Token); }
                catch (OperationCanceledException) { }
                finally { if (_delay == delay) _delay = null; }
            }
            var generation = _state.DraftGeneration; var text = Text; _pending = false;
            try
            {
                using var timeout = new CancellationTokenSource(TimeSpan.FromSeconds(5));
                if (!_recoveryOnly)
                {
                    try
                    {
                        var saved = await _store.SaveAsync(_worktree, _identity, _state.DraftRevision, text, timeout.Token);
                        // Update only the observed revision. An older completion never assigns editor text.
                        _state.DraftRevision = saved.Revision; _state.DraftObserved = true;
                        if (generation == _state.DraftGeneration) _state.DraftDirty = false;
                        _failure = null;
                    }
                    catch (CommitDraftException error) when (error.Failure is CommitDraftFailure.Changed or CommitDraftFailure.Invalid or CommitDraftFailure.NewerSchema)
                    { _recoveryOnly = true; _failure = error.Failure; }
                }
                if (_recoveryOnly)
                {
                    // Preserve both profiles instead of overwriting, or replacing unreadable/newer bytes.
                    await _store.SaveRecoveryAsync(_worktree, _identity, _state.DraftRecoveryId, text, timeout.Token);
                    _state.DraftRecoverySaved = !text.IsEmpty;
                    SetProblem(new(_failure, !text.IsEmpty, _store.Location));
                }
                else
                {
                    if (_state.DraftRecoverySaved)
                    {
                        _otherCopies = await _store.SaveRecoveryAsync(_worktree, _identity, _state.DraftRecoveryId, CommitDraftText.Empty, timeout.Token);
                        _state.DraftRecoverySaved = false;
                    }
                    SetProblem(_otherCopies ? new(null, false, _store.Location) : null);
                }
            }
            catch (Exception error) when (IsStorageFailure(error))
            {
                SetProblem(new(Failure(error), false, _store.Location));
                // No tight retry loop after permission/lock/disk errors. Edit, Refresh or close retries.
            }
        }
    }
    public async Task FlushAsync()
    {
        if (_closed) return;
        _flush = true; _delay?.Cancel();
        try
        {
            if (!_started) await StartOrRetryAsync(); else await _load;
            if (_state.DraftDirty) { _pending = true; Schedule(); }
            Task writer;
            do { writer = _writer; await writer; } while (writer != _writer);
        }
        finally { _flush = false; }
    }
    public Task CloseAsync() => _close ??= CloseCoreAsync();
    private async Task CloseCoreAsync()
    {
        await FlushAsync();
        _closed = true; _delay?.Cancel();
    }
    private void SetProblem(CommitDraftProblem? problem)
    { Problem = problem; ProblemChanged?.Invoke(this, EventArgs.Empty); }
    private static bool IsStorageFailure(Exception error) => error is IOException or UnauthorizedAccessException or OperationCanceledException;
    private static CommitDraftFailure Failure(Exception error) => error is CommitDraftException draft ? draft.Failure : CommitDraftFailure.Unavailable;
}
