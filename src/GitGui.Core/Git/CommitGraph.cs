namespace GitGui.Core.Git;

// Y coordinates are fractions of a row: top=0, node=.5, bottom=1.
public sealed record GraphLine(int FromLane, double FromY, int ToLane, double ToY, int Color);
public sealed record GraphRow(int NodeLane, int NodeColor, int LaneCount, bool IsMerge, IReadOnlyList<GraphLine> Lines)
{
    public bool IsStash { get; init; }
}

// Empty hashes reserve vacant columns. Duplicate targets retain separate branch tracks
// until that target's row, rather than joining unrelated branches early.
public sealed record GraphLane(string Hash, int Color);
public sealed record GraphCursor(IReadOnlyList<GraphLane> Lanes, int NextColor)
{
    public static GraphCursor Start { get; } = new([], 0);
}
public sealed record GraphPage(IReadOnlyList<GraphRow> Rows, GraphCursor Next);

public static class CommitGraph
{
    public const int MaximumLanes = 64;
    public static IReadOnlyList<GraphRow> Build(IReadOnlyList<CommitRecord> commits, CancellationToken token = default)
        => BuildPage(commits, GraphCursor.Start, token).Rows;

    public static GraphPage BuildPage(IReadOnlyList<CommitRecord> commits, GraphCursor cursor, CancellationToken token = default)
    {
        var lanes = cursor.Lanes.ToList();
        var seen = new HashSet<string>(StringComparer.Ordinal);
        var rows = new List<GraphRow>(commits.Count);
        var nextColor = cursor.NextColor;
        foreach (var commit in commits)
        {
            token.ThrowIfCancellationRequested();
            if (commit.Parents.Count > MaximumLanes) throw new GitReadException(GitFailure.HistoryTooWide);
            if (!seen.Add(commit.Hash) || commit.Parents.Any(seen.Contains)) throw new GitReadException(GitFailure.InvalidOutput);
            var node = lanes.FindIndex(lane => lane.Hash == commit.Hash);
            var incoming = node >= 0;
            if (!incoming) node = Allocate(new(commit.Hash, nextColor++));
            var before = lanes.ToArray(); var color = before[node].Color;
            for (var i = 0; i < lanes.Count; i++)
                if (lanes[i].Hash == commit.Hash) lanes[i] = new("", lanes[i].Color);
            var parents = commit.Parents.Distinct(StringComparer.Ordinal).ToArray();
            var outgoing = new List<int>();
            for (var p = 0; p < parents.Length; p++)
            {
                if (p == 0) { lanes[node] = new(parents[p], color); outgoing.Add(node); }
                else
                {
                    var target = lanes.FindIndex(lane => lane.Hash == parents[p]);
                    outgoing.Add(target >= 0 ? target : Allocate(new(parents[p], nextColor++)));
                }
            }
            var width = Math.Max(before.Length, lanes.Count);
            var lines = new List<GraphLine>();
            for (var i = 0; i < before.Length; i++)
            {
                if (before[i].Hash.Length == 0) continue;
                if (before[i].Hash == commit.Hash)
                { if (incoming || i != node) lines.Add(new(i, 0, node, .5, before[i].Color)); }
                else lines.Add(new(i, 0, i, 1, before[i].Color));
            }
            foreach (var lane in outgoing) lines.Add(new(node, .5, lane, 1, lanes[lane].Color));
            rows.Add(new(node, color, width, parents.Length > 1, lines.ToArray()));
            while (lanes.Count > 0 && lanes[^1].Hash.Length == 0) lanes.RemoveAt(lanes.Count - 1);
        }
        return new(rows, new(lanes.ToArray(), nextColor));

        int Allocate(GraphLane lane)
        {
            var slot = lanes.FindIndex(item => item.Hash.Length == 0);
            if (slot >= 0) { lanes[slot] = lane; return slot; }
            if (lanes.Count >= MaximumLanes) throw new GitReadException(GitFailure.HistoryTooWide);
            lanes.Add(lane); return lanes.Count - 1;
        }
    }
}
