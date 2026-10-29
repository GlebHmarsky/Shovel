using GitGui.App.Localization;
using GitGui.Core.Git;

namespace GitGui.App.ViewModels;

public sealed class RemoteSetupViewModel : ObservableObject
{
    private readonly IReadOnlyList<string> _existing;
    private string _name, _location = "";
    public string Name { get => _name; set { if (Set(ref _name, value)) Refresh(); } }
    public string Location { get => _location; set { if (Set(ref _location, value)) Refresh(); } }
    public string Readiness => UiText.Get(_existing.Contains(Options.Name) ? "RemoteAddExists" :
        !Options.IsValid ? "RemoteAddInvalid" : "RemoteAddReady");
    public RemoteSetupOptions Options => new(Name.Trim(), Location.Trim());
    public RelayCommand SubmitCommand { get; }
    public event EventHandler? Accepted;
    public RemoteSetupViewModel(IReadOnlyList<string> existing)
    {
        _existing = existing; _name = existing.Contains("origin") ? "upstream" : "origin";
        SubmitCommand = new(() => Accepted?.Invoke(this, EventArgs.Empty), () => Options.IsValid && !_existing.Contains(Options.Name));
    }
    private void Refresh() { SubmitCommand.Refresh(); Changed(nameof(Readiness)); }
}
