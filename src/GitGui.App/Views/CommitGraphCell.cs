using System.Windows;
using System.Windows.Media;
using System.Windows.Documents;
using GitGui.Core.Git;

namespace GitGui.App.Views;

/// <summary>One lightweight drawing per visible DataGrid row; no graph computation here.</summary>
public sealed class CommitGraphCell : FrameworkElement
{
    public static readonly DependencyProperty RowProperty = DependencyProperty.Register(nameof(Row), typeof(GraphRow),
        typeof(CommitGraphCell), new FrameworkPropertyMetadata(null, FrameworkPropertyMetadataOptions.AffectsRender));
    public GraphRow? Row { get => (GraphRow?)GetValue(RowProperty); set => SetValue(RowProperty, value); }
    public static readonly DependencyProperty IsSelectedProperty = DependencyProperty.Register(nameof(IsSelected), typeof(bool),
        typeof(CommitGraphCell), new FrameworkPropertyMetadata(false, FrameworkPropertyMetadataOptions.AffectsRender));
    public bool IsSelected { get => (bool)GetValue(IsSelectedProperty); set => SetValue(IsSelectedProperty, value); }
    private static readonly Brush[] Colors = [Frozen(42, 112, 179), Frozen(176, 77, 137), Frozen(37, 137, 111), Frozen(178, 113, 35), Frozen(112, 91, 178)];
    public const double LaneSpacing = 20;
    public static double WidthFor(int lanes) => Math.Max(84, lanes * LaneSpacing + 16);

    public CommitGraphCell() { ClipToBounds = true; IsHitTestVisible = false; }
    protected override void OnRender(DrawingContext drawing)
    {
        base.OnRender(drawing);
        if (Row is not { } row) return;
        Brush Color(int index) => IsSelected || SystemParameters.HighContrast ? TextElement.GetForeground(this) : Colors[index % Colors.Length];
        Point PointAt(int lane, double y) => new(16 + lane * LaneSpacing, y * ActualHeight);
        foreach (var line in row.Lines)
        {
            var start = PointAt(line.FromLane, line.FromY);
            var end = PointAt(line.ToLane, line.ToY);
            var geometry = new StreamGeometry();
            using (var context = geometry.Open())
            {
                context.BeginFigure(start, false, false);
                var middle = (start.Y + end.Y) / 2;
                context.BezierTo(new(start.X, middle), new(end.X, middle), end, true, false);
            }
            geometry.Freeze();
            drawing.DrawGeometry(null, new Pen(Color(line.Color), 1.7), geometry);
        }
        var center = PointAt(row.NodeLane, .5);
        var brush = Color(row.NodeColor);
        drawing.DrawEllipse(brush, null, center, 4.5, 4.5);
        // Merge nodes have a second ring; meaning does not rely on lane color.
        if (row.IsMerge) drawing.DrawEllipse(null, new Pen(brush, 1.2), center, 7, 7);
    }

    private static Brush Frozen(byte red, byte green, byte blue)
    {
        var brush = new SolidColorBrush(Color.FromRgb(red, green, blue)); brush.Freeze(); return brush;
    }
}
