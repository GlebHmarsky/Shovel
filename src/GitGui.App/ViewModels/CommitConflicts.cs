using GitGui.Core.Git;

namespace GitGui.App.ViewModels;
public sealed partial class CommitViewModel
{
    public RelayCommand ResolveConflictCommand { get; private set; } = null!;
    public bool IsConflictSelected => _state.Selected?.Change.Group == ChangeGroup.Conflicted;
    private void InitializeConflicts() => ResolveConflictCommand = new(() => FileTask = OpenConflictAsync(), () => Ready && IsConflictSelected);
    private async Task OpenConflictAsync()
    {
        var model = new ConflictViewModel(_state.Context!, _state.Selected!.Change, _actions);
        _readCancel?.Cancel(); _diffCancel?.Cancel(); _readCancel = _diffCancel = null; _readGeneration++; _diffGeneration++;
        _loading = _backgroundLoading = _diffLoading = false; _fileBusy = true; UpdateBusy();
        try { await _interaction.ShowConflictAsync(model); }
        finally
        {
            await model.CloseAsync(); HasMutation |= model.HasMutation; _fileBusy = false;
            if (model.HasMutation)
            {
                _state.ReadAt = default; Status = model.Status; RawOutput = model.Output;
                _state.LastResult = model.Status; _state.LastOutput = model.Output;
                if (!_closed) { LoadingTask = LoadAsync(reconcile: true); await LoadingTask; if (!_closed) QueueDiff(_state.Selected); }
            }
            UpdateBusy();
        }
    }
}
