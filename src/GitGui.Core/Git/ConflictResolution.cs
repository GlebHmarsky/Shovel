using System.Diagnostics;
using System.Security.Cryptography;
using System.Text;
using GitGui.Core.Processes;

namespace GitGui.Core.Git;

public sealed record ConflictSide(int Stage, string Hash, string Mode, string? Text);
public sealed record ConflictPreview(WorkingCopyContext Context, LocalChange Change, ConflictSide? Base, ConflictSide? Current,
    ConflictSide? Incoming, string CurrentSource, string IncomingSource, string WorktreeStamp, string? WorkingText, bool Utf8Bom, bool CanEdit, string Attributes)
{ public IReadOnlyList<string> Tools { get; init; } = []; }
public enum ConflictChoice { Current, Incoming, Edited, External }
public sealed record ConflictResolution(ConflictPreview Preview, ConflictChoice Choice, string Text = "", string ExternalTool = "");

public sealed partial class GitActions
{
    public const int MaximumConflictTextBytes = 512 * 1024;

    public Task<ConflictPreview> ReadConflictAsync(WorkingCopyContext context, LocalChange change, CancellationToken token) => Task.Run(async () =>
    {
        if (change.Group != ChangeGroup.Conflicted || !context.Changes.Entries.Contains(change)) throw new GitActionException("CommitStale");
        var path = context.Identity.Worktree; var git = context.Git;
        ValidateRegularTarget(path, change.Path);
        if ((await identities.ReadAsync(path, token)).Fingerprint != context.Identity.Fingerprint ||
            await IndexStampAsync(context.Repository, token) != context.IndexStamp) throw new GitActionException("CommitStale");
        var stamp = await RepositoryReader.WorktreeContentStampAsync(path, change.Path, token);
        var raw = await Read(git, path, ["--literal-pathspecs", "ls-files", "--unmerged", "-z", "--", change.Path], token);
        var sides = new Dictionary<int, ConflictSide>();
        foreach (var record in Decode(raw).Split('\0', StringSplitOptions.RemoveEmptyEntries))
        {
            var split = record.IndexOf('\t');
            var fields = split < 0 ? [] : record[..split].Split(' ');
            if (fields.Length != 3 || record[(split + 1)..] != change.Path || !ObjectId(fields[1]) ||
                !int.TryParse(fields[2], out var stage) || stage is < 1 or > 3 || sides.ContainsKey(stage)) throw new GitActionException("CommitStale");
            if (fields[0] is not ("100644" or "100755")) throw new GitActionException("ConflictUnsupported");
            var size = long.Parse(Decode(await Read(git, path, ["cat-file", "-s", fields[1]], token)), System.Globalization.CultureInfo.InvariantCulture);
            string? text = null;
            if (size <= MaximumConflictTextBytes)
                text = ConflictText((await Read(git, path, ["cat-file", "blob", fields[1]], token)).StandardOutput);
            sides[stage] = new(stage, fields[1], fields[0], text);
        }
        if (sides.Count == 0) throw new GitActionException("CommitStale");
        var file = RepositoryReader.WorkingCopyPath(path, change.Path);
        var bytes = File.Exists(file) && new FileInfo(file).Length <= MaximumConflictTextBytes ? await File.ReadAllBytesAsync(file, token) : null;
        var working = bytes is null ? null : ConflictText(bytes);
        // Filtered/encoded text is resolved through Git's side checkout or configured tool. Never write a pointer as an asset.
        var attributeStamp = Decode(await Read(git, path, ["--literal-pathspecs", "check-attr", "-z", "filter", "working-tree-encoding", "--", change.Path], token));
        var attributes = attributeStamp.Split('\0');
        var editable = working is not null && sides.Values.All(s => s.Text is not null) &&
            attributes.Where((_, i) => i % 3 == 2).All(v => v is "unspecified" or "unset");
        var snapshot = context.Repository;
        var current = snapshot.Branch ?? snapshot.Commit ?? "";
        var incoming = "";
        if (snapshot.Recovery?.Kind == RecoveryKind.Rebase)
        { current = snapshot.Commit ?? ""; incoming = snapshot.Recovery.Branch + " · " + snapshot.Recovery.CurrentCommit; }
        else
        {
            var marker = snapshot.Recovery?.Kind switch { RecoveryKind.Merge => "MERGE_HEAD", RecoveryKind.CherryPick => "CHERRY_PICK_HEAD", RecoveryKind.Revert => "REVERT_HEAD", _ => null };
            if (marker is not null)
            {
                var markerPath = Path.Combine(snapshot.GitDirectory, marker);
                if (File.Exists(markerPath) && new FileInfo(markerPath).Length < 4096)
                    incoming = string.Join(", ", (await File.ReadAllTextAsync(markerPath, token)).Split('\n', StringSplitOptions.RemoveEmptyEntries).Select(s => s.Trim()).Where(ObjectId));
            }
        }
        if (snapshot.Recovery?.Kind == RecoveryKind.Merge && ObjectId(incoming))
        {
            var names = Decode(await Read(git, path, ["for-each-ref", "--points-at=" + incoming, "--format=%(refname:short)", "refs/heads", "refs/remotes"], token))
                .Split('\n', StringSplitOptions.RemoveEmptyEntries).Select(s => s.TrimEnd('\r')).Take(3).ToArray();
            if (names.Length > 0) incoming = string.Join(", ", names) + " · " + incoming;
        }
        if (await IndexStampAsync(snapshot, token) != context.IndexStamp ||
            stamp != await RepositoryReader.WorktreeContentStampAsync(path, change.Path, token)) throw new GitActionException("CommitStale");
        var tools = new HashSet<string>(StringComparer.Ordinal) { "bc", "kdiff3", "meld", "p4merge", "tortoisemerge", "vscode", "winmerge" };
        var configured = await Raw(git, path, ["config", "--name-only", "--get-regexp", @"^mergetool\..*\.(cmd|path)$"], token);
        if (configured.Outcome != ProcessOutcome.Exited || configured.ExitCode is not (0 or 1)) throw ReadFailure(configured);
        foreach (var key in Decode(configured).Split('\n', StringSplitOptions.RemoveEmptyEntries))
        {
            var name = key.TrimEnd('\r'); var end = name.LastIndexOf('.');
            if (name.StartsWith("mergetool.", StringComparison.Ordinal) && end > 10 && ValidMergeTool(name[10..end])) tools.Add(name[10..end]);
        }
        return new ConflictPreview(context, change, sides.GetValueOrDefault(1), sides.GetValueOrDefault(2), sides.GetValueOrDefault(3), current, incoming,
            stamp, working, bytes is not null && bytes.AsSpan().StartsWith(new byte[] { 0xef, 0xbb, 0xbf }), editable, attributeStamp)
            { Tools = tools.Order(StringComparer.Ordinal).Take(64).ToArray() };
    }, token);

    private static bool ValidMergeTool(string name) => name.Length is > 0 and <= 128 && !name.StartsWith('-') &&
        name.All(c => char.IsAsciiLetterOrDigit(c) || c is '-' or '_' or '.');

    private static string? ConflictText(byte[] bytes)
    {
        if (bytes.Contains((byte)0)) return null;
        try
        {
            var text = new UTF8Encoding(false, true).GetString(bytes).TrimStart('\ufeff');
            if (text.StartsWith("version https://git-lfs.github.com/spec/v1", StringComparison.Ordinal) ||
                text.Count(c => c == '\n') > 20000 || text.Split('\n').Any(line => line.Length > 16384)) return null;
            return text;
        }
        catch (DecoderFallbackException) { return null; }
    }

    private async Task<WorkingCopyResult> ResolveConflictLockedAsync(WorkingCopyContext context, RepositorySnapshot fresh,
        ConflictResolution resolution, Stopwatch timing, CancellationToken token, Action<ProcessOutput>? progress)
    {
        var preview = resolution.Preview; var path = context.Identity.Worktree; var relative = preview.Change.Path;
        if (!ReferenceEquals(preview.Context, context) || !context.Changes.Entries.Contains(preview.Change) || !Enum.IsDefined(resolution.Choice) ||
            fresh.Recovery != context.Repository.Recovery) throw new GitActionException("CommitStale");
        ValidateRegularTarget(path, relative);
        if (preview.Attributes != Decode(await Read(context.Git, path, ["--literal-pathspecs", "check-attr", "-z", "filter", "working-tree-encoding", "--", relative], token))) throw new GitActionException("CommitStale");
        if (preview.WorktreeStamp != await RepositoryReader.WorktreeContentStampAsync(path, relative, token)) throw new GitActionException("ConflictEditedOutside");
        var validation = timing.Elapsed.TotalMilliseconds; var command = Stopwatch.StartNew();
        var file = RepositoryReader.WorkingCopyPath(path, relative);
        ActionResult result;
        if (resolution.Choice == ConflictChoice.External)
        {
            string? tool = resolution.ExternalTool;
            if (tool.Length > 0 && (!ValidMergeTool(tool) || !preview.Tools.Contains(tool))) throw new GitActionException("ConflictToolInvalid");
            foreach (var key in tool.Length == 0 ? new[] { "merge.guitool", "merge.tool" } : [])
            {
                var value = await Raw(context.Git, path, ["config", "--get", key], token);
                if (value.ExitCode == 0) { tool = Decode(value).Trim(); break; }
            }
            if (string.IsNullOrWhiteSpace(tool) || tool.Any(char.IsControl)) throw new GitActionException("ConflictToolMissing");
            result = await Write(["mergetool", "--no-prompt", "--tool=" + tool, "--", relative]);
        }
        else
        {
            if (resolution.Choice == ConflictChoice.Edited)
            {
                if (!preview.CanEdit || Encoding.UTF8.GetByteCount(resolution.Text) > MaximumConflictTextBytes || ConflictText(Encoding.UTF8.GetBytes(resolution.Text)) is null)
                    throw new GitActionException("ConflictTextLimit");
                if (resolution.Text.Split('\n').Any(line => line.StartsWith("<<<<<<<", StringComparison.Ordinal) || line.StartsWith("=======", StringComparison.Ordinal) || line.StartsWith(">>>>>>>", StringComparison.Ordinal)))
                    throw new GitActionException("ConflictMarkersRemain");
                // Stage follows the durable worktree save. If Git fails, retain the user's edited result.
                var temp = file + ".shovel-" + Guid.NewGuid().ToString("N") + ".tmp";
                try
                {
                    await File.WriteAllTextAsync(temp, resolution.Text, new UTF8Encoding(preview.Utf8Bom), token);
                    token.ThrowIfCancellationRequested();
                    if (preview.WorktreeStamp != await RepositoryReader.WorktreeContentStampAsync(path, relative, token)) throw new GitActionException("ConflictEditedOutside");
                    File.Move(temp, file, overwrite: true);
                }
                finally { if (File.Exists(temp)) File.Delete(temp); }
            }
            else
            {
                var side = resolution.Choice == ConflictChoice.Current ? preview.Current : preview.Incoming;
                if (side is null) File.Delete(file);
                else
                {
                    result = await Write(["restore", "--worktree", "--no-recurse-submodules", resolution.Choice == ConflictChoice.Current ? "--ours" : "--theirs", "--", relative]);
                    if (!result.Success) return new(result) { ValidationMilliseconds = validation, CommandMilliseconds = command.Elapsed.TotalMilliseconds };
                }
            }
            result = await Write(["add", "--all", "--", relative]);
        }
        if (result.Success)
        {
            var unresolved = await Read(context.Git, path, ["--literal-pathspecs", "ls-files", "--unmerged", "-z", "--", relative], token);
            if (unresolved.StandardOutput.Length > 0) result = result with { HasConflicts = true, MessageCode = "ConflictStillUnmerged" };
        }
        return new(result) { ValidationMilliseconds = validation, CommandMilliseconds = command.Elapsed.TotalMilliseconds };

        async Task<ActionResult> Write(string[] args)
        {
            var environment = EnvironmentForGit(); environment["GIT_OPTIONAL_LOCKS"] = null; environment["GIT_EDITOR"] = "false";
            var run = await runner.RunAsync(new(context.Git.Executable, path, ["--no-pager", "--no-lazy-fetch", "--literal-pathspecs", "-c", "core.fsmonitor=false",
                "-c", "gc.auto=0", "-c", "maintenance.auto=false", "-c", "submodule.recurse=false", .. args])
                { Environment = environment, Timeout = TimeSpan.FromMinutes(10), MaximumOutputBytes = 1024 * 1024 }, token, progress);
            return new(run.Outcome, run.ExitCode, Redact(Decode(run) + Encoding.UTF8.GetString(run.StandardError)));
        }
    }
}
