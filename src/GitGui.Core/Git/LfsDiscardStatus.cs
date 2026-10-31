namespace GitGui.Core.Git;

public sealed partial class RepositoryReader
{
    // Selected ordinary LFS modifications need index metadata + one SHA-256 pass, not
    // status's raw blob hashing followed by a second hydrated-content verification pass.
    // The caller brackets this read with the existing index/HEAD/config/identity guards.
    internal async Task<LocalChanges?> TryReadLfsDiscardChangesAsync(WorkingCopyContext context,
        IReadOnlyList<LocalChange> selected, CancellationToken token)
    {
        if (!OperatingSystem.IsWindows() || context.FilterOverrides?.StandardLfs != true || selected.Count is 0 or > 128 ||
            selected.Any(e => e.Group != ChangeGroup.Unstaged || e.Code != 'M' || e.OriginalPath is not null ||
                e.Submodule != "N..." || e.IndexMode != "100644" || e.WorktreeMode != e.IndexMode || !IsObjectId(e.IndexObject))) return null;
        var paths = selected.Select(e => e.Path).Distinct(StringComparer.Ordinal).ToArray();
        if (paths.Length != selected.Count || paths.Sum(p => p.Length + 3) >= 16000) return null;
        // Opening Local Changes already discovers these immutable pointers. A cold/evicted
        // cache uses native status immediately, avoiding extra probes for ordinary text files.
        var pointers = new StatusPointer[selected.Count];
        for (var i = 0; i < selected.Count; i++)
        {
            if (!_statusPointers.TryGetValue(selected[i].IndexObject, out var pointer) || pointer.Oid.Length == 0 || pointer.Size < 1024) return null;
            pointers[i] = pointer;
        }
        var git = context.Git; var root = context.Identity.Worktree;
        foreach (var path in paths) WorkingCopyPath(root, path);
        var indexTask = RunAsync(git.Executable, root, ReadArguments(["--literal-pathspecs", "ls-files", "--stage", "-v", "-z", "--", .. paths]), token);
        var attributeTask = RunAsync(git.Executable, root, ReadArguments(["--literal-pathspecs", "check-attr", "-z", "filter", "text", "working-tree-encoding", "--", .. paths]), token);
        await Task.WhenAll(indexTask, attributeTask);
        var index = await indexTask; var attributes = await attributeTask;
        RequireSuccess(index); RequireSuccess(attributes);
        // Reject sparse/assume-unchanged, changed objects/modes, conflicts, expanded paths
        // and custom transformations. Native status remains the authoritative fallback.
        var expected = selected.Select(e => $"H {e.IndexMode} {e.IndexObject} 0\t{e.Path}").ToHashSet(StringComparer.Ordinal);
        var records = Text(index.StandardOutput).Split('\0', StringSplitOptions.RemoveEmptyEntries);
        if (records.Length != expected.Count || !expected.SetEquals(records)) return null;
        var fields = Text(attributes.StandardOutput).Split('\0');
        if (fields.Length != paths.Length * 9 + 1 || fields[^1].Length != 0) return null;
        for (var i = 0; i < paths.Length; i++)
        {
            var offset = i * 9;
            if (fields[offset] != paths[i] || fields[offset + 1] != "filter" || fields[offset + 2] != "lfs" ||
                fields[offset + 3] != paths[i] || fields[offset + 4] != "text" || fields[offset + 5] != "unset" ||
                fields[offset + 6] != paths[i] || fields[offset + 7] != "working-tree-encoding" || fields[offset + 8] != "unspecified") return null;
        }
        var modified = new List<LocalChange>();
        for (var i = 0; i < selected.Count; i++)
        {
            token.ThrowIfCancellationRequested();
            var entry = selected[i];
            var match = await MatchesLfsContentAsync(root, entry.Path, pointers[i], token);
            if (match is null) return null;
            if (!match.Value) modified.Add(entry);
        }
        // The index objects/modes still match the reviewed snapshot, including partial staging.
        var targets = paths.ToHashSet(StringComparer.Ordinal);
        var staged = context.Changes.Entries.Where(e => e.Group == ChangeGroup.Staged && targets.Contains(e.Path));
        return new(staged.Concat(modified).ToArray(), DateTimeOffset.Now, false);
    }
}
