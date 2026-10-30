using System.ComponentModel;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Input;
using System.Windows.Media;
using System.Windows.Threading;
using GitGui.App.ViewModels;

namespace GitGui.App.Views;

public partial class CommitWindow : Window
{
    private readonly CommitViewModel _model;
    private bool _closing, _allowClose;
    private ContextMenu? _openFileMenu;
    public CommitWindow(CommitViewModel model)
    {
        _model = model; DataContext = model; InitializeComponent(); GitGui.App.Services.TitleBarTheme.Attach(this);
        RestoreLayout();
        SizeChanged += (_, _) => FilesColumn.MaxWidth = Math.Max(250, ActualWidth - 445);
        Loaded += async (_, _) => await _model.StartLoading();
        PreviewKeyDown += OnKeyState; PreviewKeyUp += OnKeyState;
        PreviewMouseRightButtonUp += (_, _) => Dispatcher.BeginInvoke(() => { if (_openFileMenu?.IsOpen != true) _model.EndFileMenu(); }, DispatcherPriority.Background);
        PreviewMouseLeftButtonDown += (_, _) => { if (_openFileMenu?.IsOpen != true) _model.EndFileMenu(); };
        Deactivated += (_, _) => _model.ShiftHeld = false;
        Activated += (_, _) => _model.ShiftHeld = Keyboard.Modifiers.HasFlag(ModifierKeys.Shift);
        Closing += OnClosing;
    }
    public void RestoreLayout()
    {
        var layout = _model.Layout.Normalize(); var area = SystemParameters.WorkArea;
        Width = Math.Min(layout.Width, area.Width); Height = Math.Min(layout.Height, area.Height);
        FilesColumn.Width = new(Math.Min(layout.FilesWidth, Width - 445));
        UnstagedRow.Height = new(layout.UnstagedFraction, GridUnitType.Star);
        StagedRow.Height = new(1 - layout.UnstagedFraction, GridUnitType.Star);
        MessageRow.Height = new(Math.Min(layout.MessageHeight, Math.Max(180, Height - 330)));
        if (layout.Maximized) WindowState = WindowState.Maximized;
    }
    public void CaptureLayout()
    {
        var bounds = WindowState == WindowState.Normal ? new Rect(0, 0, Width, Height) : RestoreBounds;
        _model.Layout = (_model.Layout with { Width = bounds.Width, Height = bounds.Height, Maximized = WindowState == WindowState.Maximized,
            FilesWidth = FilesColumn.ActualWidth, UnstagedFraction = UnstagedRow.ActualHeight / Math.Max(1, UnstagedRow.ActualHeight + StagedRow.ActualHeight),
            MessageHeight = MessageRow.ActualHeight }).Normalize();
    }
    private void OnKeyState(object sender, KeyEventArgs e)
    {
        _model.ShiftHeld = Keyboard.Modifiers.HasFlag(ModifierKeys.Shift);
        if (e.RoutedEvent != Keyboard.PreviewKeyDownEvent) { if (_openFileMenu?.IsOpen != true) _model.EndFileMenu(); return; }
        if (e.Key == Key.Escape)
        {
            e.Handled = true;
            if (_openFileMenu?.IsOpen == true) _openFileMenu.IsOpen = false; else Close();
            return;
        }
        var table = FindFileTable(e.OriginalSource as DependencyObject);
        if (table is null) return;
        if (e.Key == Key.Apps || e.Key == Key.F10 && Keyboard.Modifiers == ModifierKeys.Shift) { _model.BeginFileMenu(); return; }
        if (_openFileMenu?.IsOpen != true) _model.EndFileMenu();
        using var menu = _model.CreateFileMenu(table == StagedTable);
        e.Handled = RunShortcut(menu, e.Key == Key.System ? e.SystemKey : e.Key, Keyboard.Modifiers);
    }
    private DataGrid? FindFileTable(DependencyObject? source)
    {
        while (source is not null)
        {
            if (source == UnstagedTable || source == StagedTable) return (DataGrid)source;
            source = source is Visual ? VisualTreeHelper.GetParent(source) : LogicalTreeHelper.GetParent(source);
        }
        return null;
    }
    private void OnFileRightButtonDown(object sender, MouseButtonEventArgs e)
    {
        if (sender is not DataGridRow { Item: ChangeRow change } row) return;
        _model.BeginFileMenu(); SelectContextRow(row, change); e.Handled = true;
    }
    private void SelectContextRow(DataGridRow row, ChangeRow change)
    {
        var table = change.Change.Group == GitGui.Core.Git.ChangeGroup.Staged ? StagedTable : UnstagedTable;
        if (!table.SelectedItems.Contains(change)) { table.SelectedItems.Clear(); table.SelectedItem = change; }
        row.Focus();
    }
    private void OnFileMenuOpening(object sender, ContextMenuEventArgs e)
    {
        if (sender is not DataGridRow { Item: ChangeRow, ContextMenu: not null } row) { e.Handled = true; return; }
        PrepareFileMenu(row);
    }
    public ContextMenu PrepareFileMenu(DataGridRow row)
    {
        var change = (ChangeRow)row.Item; var menu = row.ContextMenu;
        _model.BeginFileMenu(); SelectContextRow(row, change);
        (menu.DataContext as IDisposable)?.Dispose();
        menu.DataContext = _model.CreateFileMenu(change.Change.Group == GitGui.Core.Git.ChangeGroup.Staged);
        _openFileMenu = menu;
        return menu;
    }
    private void OnFileMenuClosed(object sender, RoutedEventArgs e)
    {
        var menu = (ContextMenu)sender; (menu.DataContext as IDisposable)?.Dispose(); menu.DataContext = null;
        _openFileMenu = null; _model.ShiftHeld = Keyboard.Modifiers.HasFlag(ModifierKeys.Shift); _model.EndFileMenu();
    }
    private void OnFileMenuKeyDown(object sender, KeyEventArgs e)
    {
        // Enter in a popup belongs to its highlighted item, not the grid's Stage shortcut.
        if (e.Key == Key.Enter) return;
        var menu = (ContextMenu)sender;
        if (menu.DataContext is ChangeFileMenu model && RunShortcut(model, e.Key == Key.System ? e.SystemKey : e.Key, Keyboard.Modifiers))
        { e.Handled = true; menu.IsOpen = false; }
    }
    private static bool RunShortcut(ChangeFileMenu menu, Key key, ModifierKeys modifiers)
    {
        var command = (key, modifiers) switch
        {
            (Key.Enter, ModifierKeys.None or ModifierKeys.Shift) => menu.StageCommand,
            (Key.Delete, ModifierKeys.None) => menu.DiscardCommand,
            (Key.C, ModifierKeys.Control) => menu.CopyCommand,
            (Key.C, ModifierKeys.Control | ModifierKeys.Shift) => menu.CopyFullCommand,
            (Key.S, ModifierKeys.Control | ModifierKeys.Shift | ModifierKeys.Alt) => menu.AllCommand,
            (Key.O, ModifierKeys.Control | ModifierKeys.Shift | ModifierKeys.Alt) => menu.OpenCommand,
            _ => null
        };
        if (command is null) return false;
        command.Execute(null); return true;
    }
    private void OnUnstagedSelection(object sender, SelectionChangedEventArgs e) => _model.Select(false, ((DataGrid)sender).SelectedItems.Cast<ChangeRow>().ToArray());
    private void OnStagedSelection(object sender, SelectionChangedEventArgs e) => _model.Select(true, ((DataGrid)sender).SelectedItems.Cast<ChangeRow>().ToArray());
    private void OnDiffSelection(object sender, SelectionChangedEventArgs e)
    {
        _model.SelectDiffRows(((DataGrid)sender).SelectedItems.Cast<DiffRow>().ToArray());
        if (((DataGrid)sender).SelectedItems.Count == 1 && ((DataGrid)sender).SelectedItem is { } row) ((DataGrid)sender).ScrollIntoView(row);
    }
    private async void OnClosing(object? sender, CancelEventArgs e)
    {
        if (_allowClose) return;
        e.Cancel = true; if (_closing) return; _closing = true;
        // Even an already idle model may finish synchronously. Leave WPF's Closing callback
        // before closing again; otherwise the ordinary idle close throws VerifyNotClosing.
        await System.Windows.Threading.Dispatcher.Yield();
        if (!_model.ConfirmCloseConflict()) { _closing = false; return; }
        CaptureLayout(); await _model.CloseAsync(); _allowClose = true; Close();
    }
}
