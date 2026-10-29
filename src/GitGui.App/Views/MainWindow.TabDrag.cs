using System.Windows;
using System.Windows.Input;
using GitGui.App.ViewModels;

namespace GitGui.App.Views;

public partial class MainWindow
{
    private RepositoryTab? _dragTab;
    private Point _dragStart;
    private void OnTabMouseUp(object sender, MouseButtonEventArgs e) => _dragTab = null;
    private void OnTabMouseMove(object sender, MouseEventArgs e)
    {
        if (_dragTab is not { } tab || e.LeftButton != MouseButtonState.Pressed || !ViewModel.CanNavigate) return;
        var point = e.GetPosition(RepositoryTabs);
        if (Math.Abs(point.X - _dragStart.X) < SystemParameters.MinimumHorizontalDragDistance &&
            Math.Abs(point.Y - _dragStart.Y) < SystemParameters.MinimumVerticalDragDistance) return;
        // Native drag loop supplies capture, Escape cancellation and feedback. Only this strip accepts it.
        try { DragDrop.DoDragDrop(RepositoryTabs, new DataObject(typeof(RepositoryTab), tab), DragDropEffects.Move); }
        finally { _dragTab = null; }
    }
    private void OnTabDragOver(object sender, DragEventArgs e)
    {
        e.Effects = ViewModel.CanNavigate && _dragTab is not null && ReferenceEquals(e.Data.GetData(typeof(RepositoryTab)), _dragTab)
            ? DragDropEffects.Move : DragDropEffects.None;
        e.Handled = true;
    }
    private void OnTabDrop(object sender, DragEventArgs e)
    {
        if (_dragTab is not { } tab || !ReferenceEquals(e.Data.GetData(typeof(RepositoryTab)), tab) || RepositoryTabs.ActualWidth <= 0) return;
        var index = Math.Clamp((int)(e.GetPosition(RepositoryTabs).X / RepositoryTabs.ActualWidth * ViewModel.Repositories.Count), 0, ViewModel.Repositories.Count - 1);
        ViewModel.ReorderTab(tab, index); e.Handled = true;
    }
}
