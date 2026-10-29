using System.Windows;
using System.Windows.Controls;
using System.Windows.Input;
using System.Windows.Media;
using GitGui.App.ViewModels;

namespace GitGui.App.Views;
public partial class RepositorySidebarView : UserControl
{
    private bool _context;
    public RepositorySidebarView() => InitializeComponent();
    private void OnSelected(object sender, RoutedPropertyChangedEventArgs<object> e)
    {
        if (!_context && e.NewValue is SidebarNode { Select: { } command } && command.CanExecute(null)) command.Execute(null);
    }
    private void OnRightClick(object sender, MouseButtonEventArgs e)
    {
        if (sender is not TreeViewItem item) return;
        // Preview events tunnel through group containers before reaching the clicked child.
        // Only the nearest row may select itself, otherwise the group steals its menu.
        var source = e.OriginalSource as DependencyObject;
        while (source is not null && source is not TreeViewItem)
            source = source is Visual ? VisualTreeHelper.GetParent(source) : LogicalTreeHelper.GetParent(source);
        if (!ReferenceEquals(source, item)) return;
        _context = true;
        try { item.IsSelected = true; item.Focus(); }
        finally { _context = false; }
        e.Handled = true;
    }
    private void OnActivate(object sender, MouseButtonEventArgs e)
    {
        if (e.ChangedButton == MouseButton.Left && sender is TreeViewItem { DataContext: SidebarNode { Activate: { } command } } && command.CanExecute(null))
        { e.Handled = true; command.Execute(null); }
    }
    private void OnKey(object sender, KeyEventArgs e)
    {
        if (e.Key == Key.Enter && sender is TreeViewItem { DataContext: SidebarNode node })
        {
            var command = node.Activate ?? node.Select;
            if (command?.CanExecute(null) == true) { e.Handled = true; command.Execute(null); }
        }
    }
}
