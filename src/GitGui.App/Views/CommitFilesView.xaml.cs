using System.Windows.Controls;
using System.Windows;
using System.Windows.Input;
using System.Windows.Media;
using GitGui.App.ViewModels;
using GitGui.App.Localization;
namespace GitGui.App.Views;
public partial class CommitFilesView : UserControl
{
    public CommitFilesView() { InitializeComponent(); Loaded += (_, _) => ProportionalPanes.Columns(FilesColumn, DiffColumn); }
    private static DataGridRow? Row(object source)
    {
        var item = source as DependencyObject;
        while (item is not null && item is not DataGridRow) item = VisualTreeHelper.GetParent(item);
        return item as DataGridRow;
    }
    private void OnFileRightClick(object sender, MouseButtonEventArgs e)
    {
        if (DataContext is not CommitFilesViewModel model || Row(e.OriginalSource)?.Item is not ChangeRow row) return;
        model.BeginHistoryMenu(); ((DataGrid)sender).SelectedItem = row;
    }
    private void OnFileDoubleClick(object sender, MouseButtonEventArgs e)
    { if (DataContext is CommitFilesViewModel model && Row(e.OriginalSource)?.Item is ChangeRow row) model.OpenTreeDirectory(row); }
    private void OnFileMenu(object sender, ContextMenuEventArgs e)
    {
        if (e.CursorLeft >= 0 && Row(e.OriginalSource) is null)
        { e.Handled = true; (DataContext as CommitFilesViewModel)?.EndHistoryMenu(); return; }
        if (DataContext is not CommitFilesViewModel model || ((DataGrid)sender).SelectedItem is not ChangeRow row || model.CreateHistoryFileMenu(row) is not { } actions)
        { e.Handled = true; (DataContext as CommitFilesViewModel)?.EndHistoryMenu(); return; }
        model.BeginHistoryMenu(); var menu = new ContextMenu();
        MenuItem Item(string key, ICommand command) => new() { Header = UiText.Get(key), Command = command };
        menu.Items.Add(Item("FileMenuOpen", actions.OpenCommand));
        menu.Items.Add(Item("FileExternalDiff", actions.DiffCommand));
        menu.Items.Add(Item("FileMenuExplorer", actions.ExplorerCommand));
        menu.Items.Add(new Separator());
        var reset = new MenuItem { Header = UiText.Get("HistoryFileReset") };
        foreach (var choice in actions.Resets) reset.Items.Add(new MenuItem { Header = choice.Label, Command = choice.Command });
        menu.Items.Add(reset); menu.Items.Add(Item("HistoryFileBlame", actions.BlameCommand)); menu.Items.Add(Item("HistoryFileHistory", actions.HistoryCommand));
        menu.Items.Add(Item("HistoryFileTree", actions.TreeCommand));
        var lfs = new MenuItem { Header = UiText.Get("HistoryFileLfs") };
        lfs.Items.Add(Item("FileLfsTrack", actions.LfsTrackCommand)); lfs.Items.Add(Item("FileLfsUntrack", actions.LfsUntrackCommand)); menu.Items.Add(lfs);
        menu.Items.Add(new Separator()); menu.Items.Add(Item("HistoryFileSave", actions.SaveCommand));
        menu.Items.Add(new Separator()); menu.Items.Add(Item("FileMenuCopy", actions.CopyCommand)); menu.Items.Add(Item("FileMenuCopyFull", actions.CopyFullCommand));
        menu.Closed += (_, _) => model.EndHistoryMenu(); ((DataGrid)sender).ContextMenu = menu;
    }
}
