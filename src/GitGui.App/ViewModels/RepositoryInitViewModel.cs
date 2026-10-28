using System.IO;
using GitGui.App.Localization;
using GitGui.App.Services;
using GitGui.Core.Git;

namespace GitGui.App.ViewModels;

public sealed class RepositoryInitViewModel : ObservableObject
{
    private string _directory = "", _initialBranch = "";
    public string Directory { get => _directory; set { if (Set(ref _directory, value)) { CreateCommand.Refresh(); Changed(nameof(Readiness)); } } }
    public string InitialBranch { get => _initialBranch; set => Set(ref _initialBranch, value); }
    public RepositoryInitOptions Options => new(Directory, InitialBranch);
    public RelayCommand BrowseCommand { get; }
    public RelayCommand CreateCommand { get; }
    public event EventHandler? Accepted;
    private bool CanCreate => Path.IsPathFullyQualified(Directory) && Directory.IndexOfAny(Path.GetInvalidPathChars()) < 0;
    public string Readiness => UiText.Get(CanCreate ? "InitReady" : "InitEnterDirectory");
    public RepositoryInitViewModel(IUserInteraction interaction)
    {
        BrowseCommand = new(() => { if (interaction.ChooseInitializationFolder() is { } folder) Directory = folder; });
        CreateCommand = new(() => Accepted?.Invoke(this, EventArgs.Empty), () => CanCreate);
    }
}
