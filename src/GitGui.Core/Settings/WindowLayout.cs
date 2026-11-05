namespace GitGui.Core.Settings;

public sealed record PaneLength(double Value, bool Star);
public sealed record PaneLayout(string Key, PaneLength[] Columns, PaneLength[] Rows);
public sealed record WindowLayout(string Key, double Width, double Height, bool Maximized, PaneLayout[] Panes)
{
    public static WindowLayout[] Normalize(IEnumerable<WindowLayout>? layouts) => (layouts ?? [])
        .Where(item => item is not null && item.Key is { Length: > 0 and < 160 } &&
            double.IsFinite(item.Width) && double.IsFinite(item.Height) && item.Width > 0 && item.Height > 0)
        .DistinctBy(item => item.Key).Take(32).Select(item => item with
        {
            Width = Math.Clamp(item.Width, 300, 7680), Height = Math.Clamp(item.Height, 150, 4320),
            Panes = (item.Panes ?? []).Where(pane => pane is not null && pane.Key is { Length: > 0 and < 160 } &&
                Valid(pane.Columns) && Valid(pane.Rows)).DistinctBy(pane => pane.Key).Take(32).ToArray()
        }).ToArray();
    private static bool Valid(PaneLength[]? lengths) => lengths is { Length: <= 16 } &&
        lengths.All(length => length is not null && double.IsFinite(length.Value) && length.Value is >= 0 and <= 10000);
}
