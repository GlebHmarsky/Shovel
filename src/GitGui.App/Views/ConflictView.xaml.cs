using System.Windows;
using System.Windows.Controls;
using System.Windows.Controls.Primitives;
using GitGui.App.ViewModels;

namespace GitGui.App.Views;
public partial class ConflictView : UserControl
{
    public ConflictView()
    {
        InitializeComponent(); Loaded += (_, _) =>
        {
            if (Window.GetWindow(this)?.DataContext is not CommitViewModel parent) return;
            var fraction = parent.Layout.ConflictSourcesFraction;
            SourcesRow.Height = new(fraction, GridUnitType.Star); ResultRow.Height = new(1 - fraction, GridUnitType.Star);
        };
    }
    private void OnSplitCompleted(object sender, DragCompletedEventArgs e)
    {
        if (Window.GetWindow(this)?.DataContext is CommitViewModel parent)
            parent.Layout = (parent.Layout with { ConflictSourcesFraction = SourcesRow.ActualHeight / Math.Max(1, SourcesRow.ActualHeight + ResultRow.ActualHeight) }).Normalize();
    }
}
