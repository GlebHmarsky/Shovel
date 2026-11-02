using System.Windows;
using System.Windows.Controls;

namespace GitGui.App.Views;

// Convert restored legacy pixel geometry after its first arrangement. GridSplitter then
// maintains star weights natively, including through resize, maximize and UI scaling.
public static class ProportionalPanes
{
    public static void Columns(ColumnDefinition first, ColumnDefinition second)
    {
        if (first.ActualWidth <= 0 || second.ActualWidth <= 0) return;
        var total = first.ActualWidth + second.ActualWidth;
        first.Width = new(first.ActualWidth / total, GridUnitType.Star);
        second.Width = new(second.ActualWidth / total, GridUnitType.Star);
    }
    public static void Rows(RowDefinition first, RowDefinition second)
    {
        if (first.ActualHeight <= 0 || second.ActualHeight <= 0) return;
        var total = first.ActualHeight + second.ActualHeight;
        first.Height = new(first.ActualHeight / total, GridUnitType.Star);
        second.Height = new(second.ActualHeight / total, GridUnitType.Star);
    }
}
