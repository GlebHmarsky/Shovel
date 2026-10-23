namespace GitGui.Core.Git;

// Y coordinates are fractions of a row: top=0, node=.5, bottom=1.
public sealed record GraphLine(int FromLane, double FromY, int ToLane, double ToY, int Color);
public sealed record GraphRow(int NodeLane, int NodeColor, int LaneCount, bool IsMerge, IReadOnlyList<GraphLine> Lines);

/// <summary>Deterministic lane layout for child-before-parent history, independent of WPF.</summary>
public static class CommitGraph
{
    public const int MaximumLanes = 64;
    private sealed record Lane(string Hash, int Color);

    public static IReadOnlyList<GraphRow> Build(IReadOnlyList<CommitRecord> commits, CancellationToken token = default)
    {
        var lanes = new List<Lane>();
        var seen = new HashSet<string>(StringComparer.Ordinal);
        var rows = new List<GraphRow>(commits.Count);
        var nextColor = 0;
        foreach (var commit in commits)
        {
            token.ThrowIfCancellationRequested();
            // Reject pathological octopus commits before building/searching their lane list.
            if (commit.Parents.Count > MaximumLanes) throw new GitReadException(GitFailure.HistoryTooWide);
            if (!seen.Add(commit.Hash) || commit.Parents.Any(seen.Contains)) throw new GitReadException(GitFailure.InvalidOutput);
            var node = lanes.FindIndex(lane => lane.Hash == commit.Hash);
            var incoming = node >= 0;
            if (!incoming) { node = lanes.Count; lanes.Add(new(commit.Hash, nextColor++)); }
            var before = lanes.ToArray();
            var color = before[node].Color;
            lanes.RemoveAt(node);
            var parentLanes = new List<Lane>();
            foreach (var parent in commit.Parents.Distinct(StringComparer.Ordinal))
            {
                var lane = lanes.Find(item => item.Hash == parent);
                if (lane is null)
                {
                    lane = new(parent, parentLanes.Count == 0 ? color : nextColor++);
                    lanes.Insert(Math.Min(node + parentLanes.Count, lanes.Count), lane);
                }
                parentLanes.Add(lane);
            }
            var width = Math.Max(before.Length, lanes.Count);
            if (width > MaximumLanes) throw new GitReadException(GitFailure.HistoryTooWide);
            var lines = new List<GraphLine>();
            for (var i = 0; i < before.Length; i++)
            {
                if (i == node) { if (incoming) lines.Add(new(node, 0, node, .5, color)); }
                else lines.Add(new(i, 0, lanes.FindIndex(lane => lane.Hash == before[i].Hash), 1, before[i].Color));
            }
            foreach (var parent in parentLanes) lines.Add(new(node, .5, lanes.IndexOf(parent), 1, parent.Color));
            rows.Add(new(node, color, width, commit.Parents.Count > 1, lines.ToArray()));
        }
        return rows;
    }
}
