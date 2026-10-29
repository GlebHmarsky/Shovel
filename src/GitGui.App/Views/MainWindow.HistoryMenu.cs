using System.Windows;
using System.Windows.Automation;
using System.Windows.Controls;
using System.Windows.Input;
using System.Windows.Threading;
using GitGui.App.ViewModels;

namespace GitGui.App.Views;

public partial class MainWindow
{
    private ContextMenu? _historyMenu;
    private void OnHistoryRightDown(object sender, MouseButtonEventArgs e)
    {
        if (sender is not DataGridRow row || row.Item is not CommitRow) return;
        ViewModel.BeginHistoryMenu(); row.IsSelected = true; row.Focus(); e.Handled = true;
    }
    private void OnHistoryMenuOpening(object sender, ContextMenuEventArgs e)
    {
        if (sender is not DataGridRow row || row.Item is not CommitRow) { e.Handled = true; return; }
        PrepareHistoryMenu(row);
    }
    public ContextMenu PrepareHistoryMenu(DataGridRow row)
    {
        ViewModel.BeginHistoryMenu(); row.IsSelected = true;
        var menu = row.ContextMenu; menu.Items.Clear();
        foreach (var entry in ViewModel.CreateHistoryMenu((CommitRow)row.Item)) menu.Items.Add(CreateItem(entry));
        _historyMenu = menu; return menu;
    }
    private static MenuItem CreateItem(HistoryMenuEntry entry)
    {
        var item = new MenuItem { Header = entry.Label, Command = entry.Command, ToolTip = entry.Help };
        AutomationProperties.SetName(item, entry.Label); AutomationProperties.SetHelpText(item, entry.Help);
        ToolTipService.SetShowOnDisabled(item, true);
        if (entry.Children is not null) foreach (var child in entry.Children) item.Items.Add(CreateItem(child));
        return item;
    }
    private void OnHistoryMenuClosed(object sender, RoutedEventArgs e)
    {
        _historyMenu = null;
        // Let an item click acquire the action lifetime before deferred details reads.
        Dispatcher.BeginInvoke(() => ViewModel.EndHistoryMenu(), DispatcherPriority.Background);
    }
}
