using GitGui.App.Localization;
using GitGui.App.Services;
using GitGui.Core.Git;

namespace GitGui.App.ViewModels;
public sealed class RepositoryCloneViewModel : ObservableObject
{
    private string _source = "", _directory = "", _branch = "";
    public string Source { get => _source; set { if (Set(ref _source, value)) Refresh(); } }
    public string Directory { get => _directory; set { if (Set(ref _directory, value)) Refresh(); } }
    public string Branch { get => _branch; set { if (Set(ref _branch, value)) Refresh(); } }
    public RepositoryCloneOptions Options => new(Source.Trim(), Directory.Trim(), Branch.Trim());
    public string Readiness => UiText.Get(Options.IsValid ? "CloneReady" : "CloneInvalid");
    public RelayCommand BrowseCommand { get; }
    public RelayCommand SubmitCommand { get; }
    public event EventHandler? Accepted;
    public RepositoryCloneViewModel(IUserInteraction interaction)
    {
        BrowseCommand = new(() => { if (interaction.ChooseInitializationFolder() is { } folder) Directory = folder; });
        SubmitCommand = new(() => Accepted?.Invoke(this, EventArgs.Empty), () => Options.IsValid);
    }
    private void Refresh() { SubmitCommand.Refresh(); Changed(nameof(Readiness)); }
}
