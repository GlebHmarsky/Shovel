using System.ComponentModel;
using GitGui.App.Localization;
using GitGui.Core.Git;

namespace GitGui.App.ViewModels;
public sealed partial class CommitViewModel
{
    private ConflictViewModel? _conflict;
    private Task _conflictDrain = Task.CompletedTask;
    public ConflictViewModel? Conflict { get => _conflict; private set { if (Set(ref _conflict, value)) { Changed(nameof(HasConflict)); Changed(nameof(ShowRegularDiff)); } } }
    public bool HasConflict => Conflict?.HasPreview == true && _state.Selected?.Change.Path == Conflict.Path && IsConflictSelected;
    public bool ShowRegularDiff => !HasConflict;
    public bool CanSelectFiles => Conflict?.Busy != true || !Conflict.HasMutation;
    public RelayCommand ResolveConflictCommand { get; private set; } = null!;
    public bool IsConflictSelected => _state.Selected?.Change.Group == ChangeGroup.Conflicted;
    private void InitializeConflicts() => ResolveConflictCommand = new(() => FileTask = OpenConflictAsync(), () => Ready && IsConflictSelected);
    private async Task OpenConflictAsync()
    {
        if (_closed || _state.Context is null || !IsConflictSelected) return;
        var change = _state.Selected!.Change;
        if (Conflict?.Path == change.Path && !Conflict.Resolved) { Changed(nameof(HasConflict)); Changed(nameof(ShowRegularDiff)); return; }
        if (Conflict is { } prior)
        {
            prior.PropertyChanged -= OnConflictChanged; prior.Completed -= OnConflictCompleted;
            _conflictDrain = Task.WhenAll(_conflictDrain, prior.CloseAsync());
        }
        var model = new ConflictViewModel(_state.Context, change, _actions) { Journal = Journal };
        model.PropertyChanged += OnConflictChanged; model.Completed += OnConflictCompleted; Conflict = model;
        _diffCancel?.Cancel(); _diffCancel = null; _diffGeneration++; _diffLoading = false;
        await _conflictDrain;
        if (!_closed && ReferenceEquals(Conflict, model)) await model.StartAsync();
    }
    public bool ConfirmCloseConflict() => Conflict is not { HasEditorChanges: true, Resolved: false } editor ||
        _interaction.ConfirmRecovery(UiText.Format("ConflictCloseEdits", editor.Path), editor.Title);
    private bool CanLeaveConflict(ChangeRow row)
    {
        if (Conflict is not { } prior || prior.Path == row.Change.Path) return true;
        if (!ConfirmCloseConflict()) return false;
        prior.PropertyChanged -= OnConflictChanged; prior.Completed -= OnConflictCompleted;
        _conflictDrain = Task.WhenAll(_conflictDrain, prior.CloseAsync()); Conflict = null;
        return true;
    }
    private void OnConflictChanged(object? sender, PropertyChangedEventArgs args)
    {
        if (!ReferenceEquals(sender, Conflict)) return;
        _fileBusy = Conflict!.Busy && Conflict.HasMutation; HasMutation |= Conflict.HasMutation;
        if (args.PropertyName == nameof(ConflictViewModel.Output)) RawOutput = Conflict.Output;
        Changed(nameof(CanSelectFiles)); Changed(nameof(HasConflict)); Changed(nameof(ShowRegularDiff)); UpdateBusy();
    }
    private async void OnConflictCompleted(object? sender, EventArgs args)
    {
        if (_closed || !ReferenceEquals(sender, Conflict)) return;
        var model = Conflict!; Status = model.Status; RawOutput = model.Output;
        _state.LastResult = model.Status; _state.LastOutput = model.Output; _state.ReadAt = default;
        LoadingTask = LoadAsync(reconcile: true); await LoadingTask;
        if (!_closed) QueueDiff(_state.Selected);
    }
}
