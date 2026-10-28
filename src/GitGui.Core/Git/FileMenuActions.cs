using System.Diagnostics;
using System.Security.Cryptography;
using System.Text;
using GitGui.Core.Processes;

namespace GitGui.Core.Git;

public sealed partial class GitActions
{
    // Runs inside ExecuteWorkingCopyAsync's common-directory gate, after identity/HEAD/index/config validation.
    private async Task<WorkingCopyResult> ExecuteFileMenuLockedAsync(WorkingCopyContext context, RepositorySnapshot fresh,
        WorkingCopyOptions options, double initialValidation, CancellationToken token, Action<ProcessOutput>? progress)
    {
        var watch = Stopwatch.StartNew(); var path = context.Identity.Worktree; var git = context.Git;
        var entries = options.Entries;
        if (entries.Count == 0 || entries.Any(item => !context.Changes.Entries.Contains(item))) throw new GitActionException("CommitStale");
        if (entries.Any(item => item.Group is ChangeGroup.Conflicted or ChangeGroup.Ignored || item.HeadMode == "160000" || item.IndexMode == "160000"))
            throw new GitActionException("FileActionUnsupported");
        var paths = FileActionPaths(entries);
        foreach (var relative in paths) ValidateRegularTarget(path, relative);
        var scoped = paths.Length <= 128 && paths.Sum(item => item.Length + 3) < 16000;
        var current = await reader.ReadLocalChangesAsync(git, path, false, token, scoped ? paths : null, context.FilterOverrides);
        if (entries.Any(entry => !current.Entries.Contains(entry)) || await IndexStampAsync(fresh, token) != context.IndexStamp)
            throw new GitActionException("CommitStale");
        if (options.Action is WorkingCopyAction.StashFiles && (fresh.Commit is null || fresh.Operation != RepositoryOperation.None))
            throw new GitActionException(fresh.Commit is null ? "StashNeedsCommit" : "ActionSequence");
        if (options.Action == WorkingCopyAction.Discard && entries.Select(entry => entry.Group == ChangeGroup.Staged).Distinct().Count() > 1)
            throw new GitActionException("CommitStale");
        var temporary = Directory.CreateTempSubdirectory("shovel-files-");
        var pathFile = Path.Combine(temporary.FullName, "paths.nul");
        // Beside the real index so split-index shared files resolve correctly; never overwrite it.
        var privateIndex = Path.Combine(fresh.GitDirectory, "shovel-index-" + Guid.NewGuid().ToString("N"));
        var privateIndexOwned = false;
        var validation = initialValidation + watch.Elapsed.TotalMilliseconds; double commandTime = 0;
        ActionResult result;
        try
        {
            if (options.Action == WorkingCopyAction.Discard)
            {
                var staged = entries[0].Group == ChangeGroup.Staged;
                var tracked = FileActionPaths(entries.Where(entry => entry.Group != ChangeGroup.Untracked).ToArray());
                if (tracked.Length > 0)
                {
                    await WritePaths(tracked);
                    result = await Write(staged && fresh.Commit is null
                        ? ["rm", "--cached", "--ignore-unmatch", "-f", "--pathspec-from-file=" + pathFile, "--pathspec-file-nul"]
                        : ["restore", "--worktree", "--no-recurse-submodules", .. staged ? new[] { "--staged", "--source=" + fresh.Commit } : [],
                            "--pathspec-from-file=" + pathFile, "--pathspec-file-nul"]);
                    if (!result.Success) return await Finish(result);
                }
                else result = new(ProcessOutcome.Exited, 0, "");
                // Delete only the exact reviewed regular files. Never use clean or recursive deletion.
                foreach (var entry in entries.Where(entry => entry.Group == ChangeGroup.Untracked || staged && fresh.Commit is null))
                {
                    token.ThrowIfCancellationRequested(); ValidateRegularTarget(path, entry.Path);
                    File.Delete(RepositoryReader.WorkingCopyPath(path, entry.Path));
                }
            }
            else if (options.Action == WorkingCopyAction.StashFiles)
            {
                if (options.Message.Contains('\0') || Encoding.UTF8.GetByteCount(options.Message) > 4096) throw new GitActionException("StashMessageLimit");
                await WritePaths(paths);
                // Native path-limited stash still captures the entire index in its second parent.
                // Build a private HEAD index plus only selected index changes so --index restoration
                // cannot replay unrelated staged files. The real index is updated only after saving.
                await using (var copy = new FileStream(privateIndex, FileMode.CreateNew, FileAccess.Write, FileShare.None, 65536, FileOptions.Asynchronous))
                {
                    privateIndexOwned = true;
                    if (context.IndexStamp != "missing")
                    {
                        await using var source = new FileStream(Path.Combine(fresh.GitDirectory, "index"), FileMode.Open, FileAccess.Read, FileShare.ReadWrite | FileShare.Delete, 65536, FileOptions.Asynchronous);
                        await source.CopyToAsync(copy, token);
                    }
                }
                if (context.IndexStamp == "missing")
                {
                    File.Delete(privateIndex); result = await Write(["read-tree", "--empty"], privateIndex);
                    if (!result.Success) return await Finish(result);
                }
                else
                {
                    await using var verify = File.OpenRead(privateIndex);
                    if (Convert.ToHexString(await SHA256.HashDataAsync(verify, token)) != context.IndexStamp) throw new GitActionException("CommitStale");
                }
                var treeResult = await Write(["write-tree"], privateIndex);
                if (!treeResult.Success) return await Finish(treeResult);
                var tree = treeResult.Output.Trim();
                if (!ObjectId(tree) || await IndexStampAsync(fresh, token) != context.IndexStamp) throw new GitActionException("CommitStale");
                result = await Write(["read-tree", fresh.Commit!], privateIndex);
                if (!result.Success) return await Finish(result);
                var tracked = FileActionPaths(entries.Where(entry => entry.Group != ChangeGroup.Untracked).ToArray());
                if (tracked.Length > 0)
                {
                    await WritePaths(tracked);
                    result = await Write(["restore", "--staged", "--source=" + tree, "--pathspec-from-file=" + pathFile, "--pathspec-file-nul"], privateIndex);
                    if (!result.Success) return await Finish(result);
                }
                await WritePaths(paths);
                var beforeStash = await reader.ReadWorkingCopyHeadAsync(git, fresh, token);
                if (beforeStash.Commit != fresh.Commit || beforeStash.Branch != fresh.Branch || beforeStash.Operation != fresh.Operation ||
                    await IndexStampAsync(fresh, token) != context.IndexStamp || await ConfigStamp(git, path, token) != context.ConfigStamp) throw new GitActionException("CommitStale");
                result = await Write(["stash", "push", "--no-keep-index", "--include-untracked", "--message=" + options.Message,
                    "--pathspec-from-file=" + pathFile, "--pathspec-file-nul"], privateIndex);
                if (result.Success && tracked.Length > 0)
                {
                    try
                    {
                        if (await IndexStampAsync(fresh, token) != context.IndexStamp)
                            return await Finish(result with { MessageCode = "FileStashIndexRetained" });
                        await WritePaths(tracked);
                        var reset = await Write(["restore", "--staged", "--source=" + fresh.Commit, "--pathspec-from-file=" + pathFile, "--pathspec-file-nul"]);
                        result = reset with { Output = result.Output + reset.Output, MessageCode = reset.Success ? null : "FileStashIndexRetained" };
                    }
                    catch (Exception error) when (error is OperationCanceledException or IOException or UnauthorizedAccessException)
                    { result = result with { Outcome = error is OperationCanceledException ? ProcessOutcome.Cancelled : ProcessOutcome.Exited, MessageCode = "FileStashIndexRetained" }; }
                }
            }
            else if (options.Action is WorkingCopyAction.IgnoreFiles or WorkingCopyAction.IgnoreExtension)
            {
                if (!scoped) throw new GitActionException("FileActionSelectionLimit");
                if (entries.Any(entry => entry.Group != ChangeGroup.Untracked)) throw new GitActionException("FileIgnoreTracked");
                var patterns = options.Action == WorkingCopyAction.IgnoreExtension
                    ? paths.Select(item => Path.GetExtension(item)).Select(extension => extension.Length == 0 ? throw new GitActionException("FileIgnoreNoExtension") : "*" + EscapeIgnore(extension))
                    : paths.Select(item => "/" + EscapeIgnore(item));
                await AppendRulesAsync(path, ".gitignore", patterns, token, alwaysAppend: true);
                result = new(ProcessOutcome.Exited, 0, "");
                var ignored = await Raw(git, path, ["check-ignore", "-z", "--", .. paths], token);
                var matches = Decode(ignored).Split('\0', StringSplitOptions.RemoveEmptyEntries).ToHashSet(StringComparer.Ordinal);
                if (ignored.Outcome != ProcessOutcome.Exited || ignored.ExitCode is not (0 or 1) || paths.Any(item => !matches.Contains(item)))
                    result = result with { MessageCode = "FileIgnoreInherited" };
                paths = [.. paths, ".gitignore"];
            }
            else if (options.Action is WorkingCopyAction.LfsTrack or WorkingCopyAction.LfsUntrack)
            {
                if (paths.Length > 128 || paths.Sum(item => item.Length + 8) > 16000) throw new GitActionException("FileActionSelectionLimit");
                ValidateRegularTarget(path, ".gitattributes");
                if (options.Action == WorkingCopyAction.LfsTrack)
                {
                    var version = await Raw(git, path, ["lfs", "version"], token);
                    if (version.Outcome != ProcessOutcome.Exited || version.ExitCode != 0) throw new GitActionException("FileLfsMissing");
                    var filter = await Raw(git, path, ["config", "--get-regexp", @"^filter\.lfs\.(clean|process)$"], token);
                    if (filter.Outcome != ProcessOutcome.Exited || filter.ExitCode != 0 || filter.StandardOutput.Length == 0) throw new GitActionException("FileLfsSetup");
                }
                // Specific literal rules override broad root rules without deleting settings for other files.
                // This edits attributes only: no automatic install, migration, re-add, network or hook changes.
                var suffix = options.Action == WorkingCopyAction.LfsTrack ? " filter=lfs diff=lfs merge=lfs -text" : " !filter !diff !merge !text";
                await AppendRulesAsync(path, ".gitattributes", paths.Select(item => AttributePattern(item) + suffix), token, alwaysAppend: true);
                result = new(ProcessOutcome.Exited, 0, "");
                paths = [.. paths, ".gitattributes"];
                if (result.Success)
                {
                    var attributes = await Read(git, path, ["--literal-pathspecs", "check-attr", "-z", "filter", "--", .. options.Entries.Select(item => item.Path)], token);
                    if (Decode(attributes).Split('\0').Where((_, i) => i % 3 == 2).Any(value => (value == "lfs") != (options.Action == WorkingCopyAction.LfsTrack)))
                        result = result with { MessageCode = "FileLfsInherited" };
                }
            }
            else if (options.Action == WorkingCopyAction.ExternalDiff)
            {
                if (entries.Count != 1) throw new GitActionException("FileMenuSingle");
                string? tool = null;
                foreach (var key in new[] { "diff.guitool", "merge.guitool", "diff.tool", "merge.tool" })
                {
                    var configured = await Raw(git, path, ["config", "--get", key], token);
                    if (configured.Outcome != ProcessOutcome.Exited || configured.ExitCode is not (0 or 1)) throw new GitActionException("ActionReadFailed");
                    if (configured.ExitCode == 0) { tool = Decode(configured).TrimEnd('\r', '\n'); break; }
                }
                if (string.IsNullOrWhiteSpace(tool) || tool.Any(char.IsControl)) throw new GitActionException("FileDiffToolMissing");
                var entry = entries[0];
                string[] scope = entry.Group == ChangeGroup.Staged ? fresh.Commit is { } head ? ["--cached", head] : ["--cached"] : [];
                if (entry.Group == ChangeGroup.Untracked)
                {
                    // --no-index returns 1 for differences even with a successful tool, which
                    // cannot be distinguished from a tool failure of 1. Use an isolated intent-
                    // to-add index so ordinary difftool exit semantics remain trustworthy.
                    using (new FileStream(privateIndex, FileMode.CreateNew, FileAccess.Write)) { privateIndexOwned = true; }
                    File.Delete(privateIndex);
                    result = await Write(["read-tree", "--empty"], privateIndex);
                    if (!result.Success) return await Finish(result);
                    result = await Write(["add", "--intent-to-add", "--", entry.Path], privateIndex);
                    if (!result.Success) return await Finish(result);
                }
                result = await Write(["difftool", "--no-prompt", "--trust-exit-code", "--tool=" + tool, "--no-textconv",
                    .. scope, "--no-exit-code", "--", .. paths], entry.Group == ChangeGroup.Untracked ? privateIndex : null);
            }
            else throw new GitActionException("ActionReadFailed");
            return await Finish(result);
        }
        finally
        {
            foreach (var name in new[] { "paths.nul" })
                try { File.Delete(Path.Combine(temporary.FullName, name)); } catch (IOException) { } catch (UnauthorizedAccessException) { }
            if (privateIndexOwned)
                foreach (var file in new[] { privateIndex, privateIndex + ".lock" })
                    try { File.Delete(file); } catch (IOException) { } catch (UnauthorizedAccessException) { }
            try { temporary.Delete(); } catch (IOException) { } catch (UnauthorizedAccessException) { }
        }

        Task WritePaths(string[] values) => File.WriteAllTextAsync(pathFile, string.Join('\0', values) + "\0", new UTF8Encoding(false), token);
        async Task<ActionResult> Write(string[] arguments, string? indexFile = null)
        {
            var environment = EnvironmentForGit(); environment["GIT_OPTIONAL_LOCKS"] = null; environment["GIT_EDITOR"] = "false";
            if (indexFile is not null) environment["GIT_INDEX_FILE"] = indexFile;
            var run = await runner.RunAsync(new(git.Executable, path, ["--no-pager", "--no-lazy-fetch", "--literal-pathspecs", "-c", "core.fsmonitor=false",
                "-c", "gc.auto=0", "-c", "maintenance.auto=false", "-c", "submodule.recurse=false", .. arguments])
                { Environment = environment, Timeout = TimeSpan.FromMinutes(options.Action == WorkingCopyAction.ExternalDiff ? 10 : 5), MaximumOutputBytes = 1024 * 1024 }, token, progress);
            commandTime += run.Duration.TotalMilliseconds;
            return new(run.Outcome, run.ExitCode, Redact(Decode(run) + Encoding.UTF8.GetString(run.StandardError)));
        }
        async Task<WorkingCopyResult> Finish(ActionResult action)
        {
            WorkingCopyContext? updated = null; var refresh = Stopwatch.StartNew();
            if (action.Success && scoped && options.Action != WorkingCopyAction.ExternalDiff)
            {
                using var observe = new CancellationTokenSource(TimeSpan.FromSeconds(20));
                try
                {
                    var stamp = await IndexStampAsync(fresh, observe.Token);
                    var changed = await reader.ReadLocalChangesAsync(git, path, false, observe.Token, paths, context.FilterOverrides);
                    if (stamp == await IndexStampAsync(fresh, observe.Token))
                    {
                        var targets = paths.ToHashSet(StringComparer.Ordinal);
                        var rows = context.Changes.Entries.Where(entry => !targets.Contains(entry.Path) && (entry.OriginalPath is null || !targets.Contains(entry.OriginalPath)))
                            .Concat(changed.Entries).OrderBy(entry => entry.Group).ThenBy(entry => entry.Path, StringComparer.Ordinal).ToArray();
                        updated = context with { Repository = fresh, IndexStamp = stamp, Changes = new(rows, changed.ReadAt, false) };
                    }
                }
                catch (Exception error) when (error is GitReadException or GitActionException or IOException or UnauthorizedAccessException or OperationCanceledException) { }
            }
            return new(action) { UpdatedContext = updated, ValidationMilliseconds = validation, CommandMilliseconds = commandTime, RefreshMilliseconds = refresh.Elapsed.TotalMilliseconds };
        }
    }

    public static string[] FileActionPaths(IReadOnlyList<LocalChange> entries) => entries.SelectMany(entry => entry.OriginalPath is { } old && entry.Code == 'R'
        ? new[] { entry.Path, old } : [entry.Path]).Distinct(StringComparer.Ordinal).ToArray();

    internal static void ValidateRegularTarget(string root, string relative)
    {
        var file = RepositoryReader.WorkingCopyPath(root, relative);
        for (var part = file; ; part = Path.GetDirectoryName(part)!)
        {
            if (Path.Exists(part) && File.GetAttributes(part).HasFlag(FileAttributes.ReparsePoint)) throw new GitActionException("FileMenuLinkedPath");
            if (string.Equals(part, Path.TrimEndingDirectorySeparator(root), StringComparison.OrdinalIgnoreCase)) break;
        }
        if (Directory.Exists(file)) throw new GitActionException("FileActionUnsupported");
    }
    public static string EscapeIgnore(string value)
    {
        if (value.Any(char.IsControl)) throw new GitActionException("FileRulePathUnsupported");
        return string.Concat(value.Select(c => "\\*?[]!# ".Contains(c) ? "\\" + c : c.ToString()));
    }
    public static string AttributePattern(string value)
    {
        if (value.Any(char.IsControl)) throw new GitActionException("FileRulePathUnsupported");
        var pattern = "/" + string.Concat(value.Select(c => "*?[]".Contains(c) ? "\\" + c : c.ToString()));
        return "\"" + pattern.Replace("\\", "\\\\").Replace("\"", "\\\"") + "\"";
    }
    private static async Task AppendRulesAsync(string root, string relative, IEnumerable<string> rules, CancellationToken token, bool alwaysAppend = false)
    {
        ValidateRegularTarget(root, relative); var file = RepositoryReader.WorkingCopyPath(root, relative);
        // Exclusive access avoids overwriting a concurrent editor; no global ignore/config changes.
        await using var stream = new FileStream(file, FileMode.OpenOrCreate, FileAccess.ReadWrite, FileShare.None, 4096, FileOptions.Asynchronous);
        if (stream.Length > 1024 * 1024) throw new GitActionException("FileRulesTooLarge");
        var data = new byte[(int)stream.Length]; await stream.ReadExactlyAsync(data, token);
        string text;
        try { text = new UTF8Encoding(false, true).GetString(data); } catch (DecoderFallbackException) { throw new GitActionException("FileRulesEncoding"); }
        var newline = text.Contains("\r\n", StringComparison.Ordinal) ? "\r\n" : "\n";
        var existing = text.Split('\n').Select(item => item.TrimEnd('\r')).ToHashSet(StringComparer.Ordinal);
        var additions = rules.Distinct(StringComparer.Ordinal).Where(item => alwaysAppend || !existing.Contains(item)).ToArray();
        if (additions.Length == 0) return;
        var bytes = Encoding.UTF8.GetBytes((data.Length > 0 && data[^1] != '\n' ? newline : "") + string.Join(newline, additions) + newline);
        if (stream.Length + bytes.Length > 1024 * 1024) throw new GitActionException("FileRulesTooLarge");
        await stream.WriteAsync(bytes, token); await stream.FlushAsync(token);
    }
}
