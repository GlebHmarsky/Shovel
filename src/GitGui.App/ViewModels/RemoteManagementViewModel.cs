using GitGui.App.Localization;
using GitGui.Core.Git;

namespace GitGui.App.ViewModels;
public sealed class RemoteManagementViewModel : ObservableObject
{
    public IReadOnlyList<RemoteConfiguration> Remotes { get; }
    private RemoteConfiguration? _selected;
    private string _location = "", _pushLocation = "";
    public RemoteConfiguration? Selected
    {
        get => _selected;
        set { if (!Set(ref _selected, value)) return; _location = value?.Location ?? ""; _pushLocation = value?.PushLocation ?? "";
            Changed(nameof(Location)); Changed(nameof(PushLocation)); Refresh(); }
    }
    public string Location { get => _location; set { if (Set(ref _location, value)) Refresh(); } }
    public string PushLocation { get => _pushLocation; set { if (Set(ref _pushLocation, value)) Refresh(); } }
    public bool CanEdit => Selected?.CanEdit == true;
    private bool Valid => Selected is { CanEdit: true } remote && new RemoteSetupOptions(remote.Name, Location.Trim()).IsValid &&
        (PushLocation.Trim().Length == 0 || new RemoteSetupOptions(remote.Name, PushLocation.Trim()).IsValid);
    public string Readiness => UiText.Get(Selected is null ? "RemoteManageEmpty" : !CanEdit ? "RemoteManageUnsupported" : !Valid ? "RemoteAddInvalid" : "RemoteManageReady");
    public RemoteManagementOptions? Result { get; private set; }
    public RelayCommand SaveCommand { get; }
    public RelayCommand RemoveCommand { get; }
    public event EventHandler? Accepted;
    public RemoteManagementViewModel(IReadOnlyList<RemoteConfiguration> remotes, string? selected = null)
    {
        Remotes = remotes; _selected = remotes.FirstOrDefault(r => r.Name == selected) ?? remotes.FirstOrDefault();
        _location = _selected?.Location ?? ""; _pushLocation = _selected?.PushLocation ?? "";
        SaveCommand = new(() => Submit(false), () => Valid);
        RemoveCommand = new(() => Submit(true), () => Selected is not null);
    }
    private void Submit(bool remove) { Result = new(Selected!.Name, Location.Trim(), PushLocation.Trim(), remove); Accepted?.Invoke(this, EventArgs.Empty); }
    private void Refresh() { SaveCommand?.Refresh(); RemoveCommand?.Refresh(); Changed(nameof(CanEdit)); Changed(nameof(Readiness)); }
}
