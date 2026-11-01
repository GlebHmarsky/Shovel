using GitGui.Core.Git;
namespace GitGui.App.ViewModels;

public sealed record SideBySideRow(DiffRow? Old, DiffRow? New)
{
    public static IReadOnlyList<SideBySideRow> Create(IReadOnlyList<DiffRow> rows)
    {
        var result = new List<SideBySideRow>();
        for (var i = 0; i < rows.Count;)
        {
            if (rows[i].Kind is not (DiffLineKind.Added or DiffLineKind.Removed))
            { result.Add(new(rows[i], rows[i])); i++; continue; }
            var old = new List<DiffRow>(); var next = new List<DiffRow>();
            while (i < rows.Count && rows[i].Kind is DiffLineKind.Added or DiffLineKind.Removed)
            {
                var row = rows[i++];
                (row.Kind == DiffLineKind.Removed ? old : next).Add(row);
            }
            for (var n = 0; n < Math.Max(old.Count, next.Count); n++)
                result.Add(new(n < old.Count ? old[n] : null, n < next.Count ? next[n] : null));
        }
        return result;
    }
}
