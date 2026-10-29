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
    private static readonly Brush[] Colors = [Frozen(244, 179, 28), Frozen(225, 80, 75), Frozen(75, 170, 133), Frozen(98, 159, 227), Frozen(179, 119, 206)];
    private static readonly Brush[] LightColors = [Frozen(148, 100, 0), Frozen(181, 42, 39), Frozen(30, 116, 78), Frozen(38, 101, 173), Frozen(128, 65, 163)];
    public const double LaneSpacing = 16;
    public static double WidthFor(int lanes) => Math.Max(42, lanes * LaneSpacing + 16);

    public CommitGraphCell()
    {
        ClipToBounds = true; IsHitTestVisible = false;
        // A semantic resource invalidates the drawing on live palette/high-contrast changes.
        SetResourceReference(TextElement.ForegroundProperty, "Theme.HistoryText");
    }
    protected override void OnRender(DrawingContext drawing)
    {
        base.OnRender(drawing);
        if (Row is not { } row) return;
        var colors = TryFindResource("Theme.IsDark") is false ? LightColors : Colors;
        Brush Color(int index) => SystemParameters.HighContrast ? (IsSelected ? SystemColors.HighlightTextBrush : TextElement.GetForeground(this)) : colors[index % colors.Length];
        Point PointAt(int lane, double y) => new(12 + lane * LaneSpacing, y * ActualHeight);
        foreach (var line in row.Lines)
        {
            var start = PointAt(line.FromLane, line.FromY);
            var end = PointAt(line.ToLane, line.ToY);
            var geometry = new StreamGeometry();
            using (var context = geometry.Open())
            {
                context.BeginFigure(start, false, false);
                if (start.X == end.X) context.LineTo(end, true, false);
                else
                {
                    // One broad quarter-turn at the fork/join, with the branch straight
                    // through the row boundary. Never bend a live lane around another merge.
                    var radius = Math.Min(10, Math.Min(Math.Abs(end.X - start.X), end.Y - start.Y));
                    var direction = Math.Sign(end.X - start.X);
                    if (line.FromY == .5)
                    {
                        context.LineTo(new(end.X - direction * radius, start.Y), true, false);
                        context.QuadraticBezierTo(new(end.X, start.Y), new(end.X, start.Y + radius), true, false);
                        context.LineTo(end, true, false);
                    }
                    else
                    {
                        context.LineTo(new(start.X, end.Y - radius), true, false);
                        context.QuadraticBezierTo(new(start.X, end.Y), new(start.X + direction * radius, end.Y), true, false);
                        context.LineTo(end, true, false);
                    }
                }
            }
            geometry.Freeze();
            drawing.DrawGeometry(null, new Pen(Color(line.Color), 1.7), geometry);
        }
        var center = PointAt(row.NodeLane, .5);
        var brush = Color(row.NodeColor);
        drawing.DrawEllipse(row.IsMerge ? null : brush, row.IsMerge ? new Pen(brush, 1.7) : null, center, row.IsMerge ? 5 : 3, row.IsMerge ? 5 : 3);
        // Merge nodes have a second ring; meaning does not rely on lane color.
        if (row.IsMerge) drawing.DrawEllipse(brush, null, center, 1.5, 1.5);
    }

    private static Brush Frozen(byte red, byte green, byte blue)
    {
        var brush = new SolidColorBrush(Color.FromRgb(red, green, blue)); brush.Freeze(); return brush;
    }
}
