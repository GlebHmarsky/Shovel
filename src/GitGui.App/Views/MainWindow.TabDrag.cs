using System.Windows;
using System.Windows.Input;
using System.Windows.Controls;
using System.Windows.Documents;
using System.Windows.Media;
using System.Globalization;
using GitGui.App.ViewModels;

namespace GitGui.App.Views;

public partial class MainWindow
{
    private RepositoryTab? _dragTab;
    private Point _dragStart;
    private TabDragAdorner? _dragFeedback;
    private int _dropIndex;
    private void OnTabMouseUp(object sender, MouseButtonEventArgs e) => _dragTab = null;
    private void OnTabMouseMove(object sender, MouseEventArgs e)
    {
        if (_dragTab is not { } tab || e.LeftButton != MouseButtonState.Pressed || !ViewModel.CanNavigate) return;
        var point = e.GetPosition(RepositoryTabs);
        if (Math.Abs(point.X - _dragStart.X) < SystemParameters.MinimumHorizontalDragDistance &&
            Math.Abs(point.Y - _dragStart.Y) < SystemParameters.MinimumVerticalDragDistance) return;
        // Native drag loop supplies capture, Escape cancellation and feedback. Only this strip accepts it.
        var layer = AdornerLayer.GetAdornerLayer(RepositoryTabs);
        _dragFeedback = new(RepositoryTabs, tab.Label) { IsHitTestVisible = false }; layer?.Add(_dragFeedback);
        try { DragDrop.DoDragDrop(RepositoryTabs, new DataObject(typeof(RepositoryTab), tab), DragDropEffects.Move); }
        finally
        {
            layer?.Remove(_dragFeedback); _dragFeedback = null; _dragTab = null;
            for (var i = 0; i < ViewModel.Repositories.Count; i++)
                if (RepositoryTabs.ItemContainerGenerator.ContainerFromIndex(i) is ListBoxItem item) item.RenderTransform = Transform.Identity;
        }
    }
    private void OnTabDragOver(object sender, DragEventArgs e)
    {
        e.Effects = ViewModel.CanNavigate && _dragTab is not null && ReferenceEquals(e.Data.GetData(typeof(RepositoryTab)), _dragTab)
            ? DragDropEffects.Move : DragDropEffects.None;
        e.Handled = true;
        if (e.Effects != DragDropEffects.Move) return;
        var point = e.GetPosition(RepositoryTabs); var boundary = ViewModel.Repositories.Count; var x = RepositoryTabs.ActualWidth;
        for (var i = 0; i < ViewModel.Repositories.Count; i++)
        {
            if (RepositoryTabs.ItemContainerGenerator.ContainerFromIndex(i) is not ListBoxItem item) continue;
            item.RenderTransform = Transform.Identity;
            var left = item.TranslatePoint(new Point(), RepositoryTabs).X;
            if (boundary == ViewModel.Repositories.Count && point.X < left + item.ActualWidth / 2) { boundary = i; x = left; }
        }
        var source = ViewModel.Repositories.IndexOf(_dragTab!);
        _dropIndex = Math.Clamp(boundary > source ? boundary - 1 : boundary, 0, ViewModel.Repositories.Count - 1);
        if (_dragFeedback is { } feedback) { feedback.Position = point; feedback.Insertion = x; feedback.InvalidateVisual(); }
        if (boundary < ViewModel.Repositories.Count && RepositoryTabs.ItemContainerGenerator.ContainerFromIndex(boundary) is ListBoxItem next)
            next.RenderTransform = new TranslateTransform(6, 0);
    }
    private void OnTabDrop(object sender, DragEventArgs e)
    {
        if (_dragTab is not { } tab || !ReferenceEquals(e.Data.GetData(typeof(RepositoryTab)), tab) || RepositoryTabs.ActualWidth <= 0) return;
        ViewModel.ReorderTab(tab, _dropIndex); e.Handled = true;
    }

    private sealed class TabDragAdorner(UIElement target, string label) : Adorner(target)
    {
        public Point Position { get; set; }
        public double Insertion { get; set; }
        protected override void OnRender(DrawingContext dc)
        {
            IsHitTestVisible = false;
            dc.DrawLine(new Pen(Brushes.Gray, 3), new(Insertion, 2), new(Insertion, AdornedElement.RenderSize.Height - 2));
            var text = new FormattedText(label, CultureInfo.CurrentUICulture, FlowDirection.LeftToRight,
                new Typeface("Segoe UI"), 12, Brushes.White, VisualTreeHelper.GetDpi(this).PixelsPerDip) { MaxTextWidth = 220, Trimming = TextTrimming.CharacterEllipsis };
            dc.PushOpacity(.8);
            dc.DrawRoundedRectangle(Brushes.DimGray, null, new(Position.X + 12, Position.Y + 12, text.Width + 24, text.Height + 14), 4, 4);
            dc.DrawText(text, new(Position.X + 24, Position.Y + 19)); dc.Pop();
        }
    }
}
