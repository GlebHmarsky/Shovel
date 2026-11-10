// Reference-only native outcome probe. Never included in the Rust payload.
using System.Text;
using System.Text.Json;
using GitGui.Core.Git;
using GitGui.Core.Processes;
using GitGui.ProcessHost;

if (args.Length is not (2 or 3) || args.Length == 3 && args[2] is not ("--init" or "--clone" or "--divergence" or "--history" or "--remotes")) throw new ArgumentException("Expected new fixture directory, output directory and optional --init, --clone, --divergence or --history, --remotes.");
var root = Path.GetFullPath(args[0]);
var output = Path.GetFullPath(args[1]);
if (Directory.Exists(root) || File.Exists(root)) throw new IOException("Fixture path must not exist.");
Directory.CreateDirectory(root); Directory.CreateDirectory(output);
var config = Path.Combine(root, "isolated.config");
await File.WriteAllTextAsync(config, "[user]\nname=Fixture\nemail=fixture@example.invalid\n[commit]\ngpgsign=false\n[core]\nautocrlf=false\n");
Environment.SetEnvironmentVariable("GIT_CONFIG_GLOBAL", config);
Environment.SetEnvironmentVariable("GIT_CONFIG_NOSYSTEM", "1");
Environment.SetEnvironmentVariable("GIT_AUTHOR_DATE", "2026-01-01T00:00:00+00:00");
Environment.SetEnvironmentVariable("GIT_COMMITTER_DATE", "2026-01-01T00:00:00+00:00");
var runner = new FixedDateRunner();
var reader = new RepositoryReader(runner);
var actions = new GitActions(runner, reader, new WindowsRepositoryIdentity());
var git = await reader.DiscoverAsync(null, default);
async Task<string> Git(string path, params string[] arguments)
{
    var result = await runner.RunAsync(new(git.Executable, path, arguments), default);
    if (result.ExitCode != 0) throw new IOException(Encoding.UTF8.GetString(result.StandardError));
    return Encoding.UTF8.GetString(result.StandardOutput).TrimEnd();
}

if (args.Length == 3 && args[2] == "--remotes")
{
    var path = Path.Combine(root, "repository");
    Directory.CreateDirectory(path);
    await Git(path, "init", "--initial-branch=main");
    await File.WriteAllTextAsync(Path.Combine(path, "file"), "base\n");
    await Git(path, "add", "file"); await Git(path, "commit", "-m", "base");
    await File.WriteAllTextAsync(Path.Combine(path, "file"), "staged\n"); await Git(path, "add", "file");
    await File.WriteAllTextAsync(Path.Combine(path, "file"), "working\n");
    var context = await actions.PrepareAsync(git, path, default);
    var records = new List<object>();
    async Task Record(string name, bool success, int count) {
        var cfg = await runner.RunAsync(new(git.Executable, path, ["config", "--local", "--get-regexp", @"^(remote\.|branch\.main\.)"]), default);
        records.Add(new {
            @case = name, success, commands = count,
            head = await Git(path, "rev-parse", "HEAD"),
            index = await Git(path, "ls-files", "--stage"),
            worktree = await File.ReadAllTextAsync(Path.Combine(path, "file")),
            status = await Git(path, "status", "--porcelain=v1"),
            config = Encoding.UTF8.GetString(cfg.StandardOutput).TrimEnd().Replace("\r", ""),
            refs = await Git(path, "for-each-ref", "--format=%(refname) %(objectname)")
        });
    }
    var before = runner.RemoteWrites;
    var prepared = await actions.PrepareRemoteSetupAsync(context, default);
    var result = await actions.AddRemoteAsync(prepared, new("origin", "no-contact::initial"), default);
    await Record("add", result.Success, runner.RemoteWrites - before);
    async Task Change(string name, string fetch, string push, bool remove=false) {
        var reviewed = await actions.ReadRemoteConfigurationAsync(git, path, default);
        var count = runner.RemoteWrites;
        var changed = await actions.ChangeRemoteAsync(context, reviewed, new("origin", fetch, push, remove), default);
        await Record(name, changed.Success, runner.RemoteWrites - count);
    }
    await Change("edit", "../fetch two", "../push two");
    await Change("fallback", "../fetch two", "");
    await Change("unchanged", "../fetch two", "");
    var stale = await actions.ReadRemoteConfigurationAsync(git, path, default);
    await Git(path, "config", "remote.origin.url", "../external");
    before = runner.RemoteWrites;
    try {
        await actions.ChangeRemoteAsync(context, stale, new("origin", "../should-not-write", ""), default);
        throw new IOException("Expected stale reviewed-config rejection.");
    } catch (GitActionException e) when (e.Code == "RemoteManageChanged") { }
    await Record("stale", false, runner.RemoteWrites - before);
    var head = await Git(path, "rev-parse", "HEAD");
    await Git(path, "update-ref", "refs/remotes/origin/main", head);
    await Git(path, "config", "branch.main.remote", "origin");
    await Git(path, "config", "branch.main.merge", "refs/heads/main");
    await Change("remove", "", "", true);
    await File.WriteAllTextAsync(Path.Combine(output, "remotes.json"), JsonSerializer.Serialize(records, new JsonSerializerOptions { WriteIndented=true }));
    Console.WriteLine("PASS baseline local remote workflow: 6 native outcomes.");
    return;
}

if (args.Length == 3 && args[2] == "--history")
{
    var path = Path.Combine(root, "history");
    Directory.CreateDirectory(path);
    await Git(path, "init", "--initial-branch=main");
    var stream = new StringBuilder();
    for (var i = 1; i <= 610; i++) {
        var message = $"Commit {i}\n\nFull retained metadata";
        stream.Append($"commit refs/heads/main\nmark :{i}\ncommitter Fixture <fixture@example.invalid> {1600000000+i} +0000\ndata {Encoding.UTF8.GetByteCount(message)}\n{message}\n\n");
    }
    stream.Append("done\n");
    var input = Path.Combine(root, "history.fi");
    await File.WriteAllTextAsync(input, stream.ToString(), new UTF8Encoding(false));
    var imported = await runner.RunAsync(new(git.Executable, path, ["fast-import", "--quiet"]) { StandardInputFile = input }, default);
    if (imported.ExitCode != 0) throw new IOException("Import failed.");
    var configBefore = Convert.ToHexString(await File.ReadAllBytesAsync(Path.Combine(path, ".git", "config")));
    var headBefore = await Git(path, "rev-parse", "HEAD");
    var records = new List<object>();
    async Task Record(string name, CommitHistory page) {
        var config = Convert.ToHexString(await File.ReadAllBytesAsync(Path.Combine(path, ".git", "config")));
        if (config != configBefore || File.Exists(Path.Combine(path, ".git", "index"))) throw new InvalidOperationException("Read changed config or index.");
        records.Add(new { @case = name, offset = page.Cursor.Offset, next = page.Next?.Offset,
            hashes = string.Join("\n", page.Entries.Select(e => e.Commit.Hash)), head = await Git(path, "rev-parse", "HEAD"),
            config, indexExists = false, status = await Git(path, "status", "--porcelain", "-z") });
    }
    var first = await reader.ReadInitialHistoryAsync(git, await reader.InspectAsync(git, path, default), HistoryScope.AllCommits, default);
    await Record("first", first);
    var second = await reader.ReadHistoryPageAsync(git, first.Session, first.Next!, default);
    await Record("older", second);
    var refreshed = await reader.ReadRefreshedHistoryAsync(git, await reader.InspectAsync(git, path, default), second, default);
    await Record("unchanged-refresh", refreshed);
    var last = refreshed;
    while (last.Next is not null) last = await reader.ReadHistoryPageAsync(git, last.Session, last.Next, default);
    await Record("end", last);
    var newer = await reader.ReadHistoryPageAsync(git, second.Session, first.Cursor, default);
    await Record("newer", newer);
    if (headBefore != await Git(path, "rev-parse", "HEAD")) throw new InvalidOperationException("Read changed HEAD.");
    // Move the ref to an existing immutable tip so no clock/worktree/index affects this read comparison.
    await Git(path, "update-ref", "refs/heads/main", first.Entries[1].Commit.Hash);
    try {
        await reader.ReadHistoryPageAsync(git, second.Session, second.Next!, default);
        throw new InvalidOperationException("Changed boundary accepted.");
    } catch (GitReadException e) when (e.Failure == GitFailure.ChangedDuringRead) { }
    var changed = await reader.ReadRefreshedHistoryAsync(git, await reader.InspectAsync(git, path, default), second, default);
    await Record("changed-refresh", changed);
    await File.WriteAllTextAsync(Path.Combine(output, "history.json"), JsonSerializer.Serialize(records, new JsonSerializerOptions { WriteIndented = true }));
    Console.WriteLine("PASS baseline history refresh, paging and exact read preservation");
    return;
}

if (args.Length == 3 && args[2] == "--divergence")
{
    var path = Path.Combine(root, "repository");
    Directory.CreateDirectory(path);
    await Git(path, "init", "--initial-branch=main");
    var tree = await Git(path, "mktree");
    var basis = await Git(path, "commit-tree", tree, "-m", "Root");
    var left = await Git(path, "commit-tree", tree, "-p", basis, "-m", "Local one");
    var tip = await Git(path, "commit-tree", tree, "-p", left, "-m", "Local two");
    var right = await Git(path, "commit-tree", tree, "-p", basis, "-m", "Upstream one");
    foreach (var (key, value) in new[] {
        ("remote.team.url", "https://example.invalid/never-contact"),
        ("remote.team.fetch", "+refs/heads/*:refs/remotes/team/*"),
        ("branch.main.remote", "team"), ("branch.main.merge", "refs/heads/topic"),
        ("branch.main.pushRemote", "different-destination") })
        await Git(path, "config", key, value);
    async Task<HistorySession> Session(string directory) => await reader.OpenHistoryAsync(git,
        await reader.InspectAsync(git, directory, default), HistoryScope.AllCommits, default);
    var records = new List<object>();
    foreach (var (name, head, upstream, ahead, behind) in new[] {
        ("sync", basis, basis, 0L, 0L), ("ahead", tip, basis, 2L, 0L),
        ("behind", basis, right, 0L, 1L), ("diverged", tip, right, 2L, 1L) })
    {
        await Git(path, "update-ref", "refs/heads/main", head);
        await Git(path, "update-ref", "refs/remotes/team/topic", upstream);
        var session = await Session(path);
        var headFile = Convert.ToHexString(await File.ReadAllBytesAsync(Path.Combine(path, ".git", "HEAD")));
        var configBytes = Convert.ToHexString(await File.ReadAllBytesAsync(Path.Combine(path, ".git", "config")));
        var status = await Git(path, "status", "--porcelain", "-z");
        var counts = await reader.ReadDivergenceAsync(git, session, default);
        if (counts != new BranchDivergence(DivergenceState.Available, ahead, behind) ||
            headFile != Convert.ToHexString(await File.ReadAllBytesAsync(Path.Combine(path, ".git", "HEAD"))) ||
            configBytes != Convert.ToHexString(await File.ReadAllBytesAsync(Path.Combine(path, ".git", "config"))) ||
            File.Exists(Path.Combine(path, ".git", "index")) || status != await Git(path, "status", "--porcelain", "-z"))
            throw new InvalidOperationException("Divergence/state preservation failed.");
        records.Add(new { @case = name, state = "Available", ahead, behind, head, upstream, config = configBytes, headFile, status, indexExists = false });
    }
    var captured = await Session(path);
    await Git(path, "update-ref", "refs/remotes/team/topic", tip);
    if (await reader.ReadDivergenceAsync(git, captured, default) != new BranchDivergence(DivergenceState.Available, 2, 1)) throw new InvalidOperationException("Mixed snapshot tips.");
    records.Add(new { @case = "immutable", state = "Available", ahead = 2, behind = 1 });
    async Task Require(string directory, DivergenceState state) {
        if ((await reader.ReadDivergenceAsync(git, await Session(directory), default)).State != state) throw new InvalidOperationException("Wrong divergence state: " + state);
        records.Add(new { @case = state.ToString(), state = state.ToString() });
    }
    await File.WriteAllTextAsync(Path.Combine(path, ".git", "shallow"), tip + "\n");
    await Require(path, DivergenceState.Shallow);
    File.Delete(Path.Combine(path, ".git", "shallow"));
    await Git(path, "update-ref", "-d", "refs/remotes/team/topic");
    await Require(path, DivergenceState.MissingUpstream);
    await Git(path, "config", "--unset", "branch.main.remote");
    await Require(path, DivergenceState.NoUpstream);
    await Git(path, "checkout", "--detach", tip);
    var empty = Path.Combine(root, "empty"); Directory.CreateDirectory(empty);
    await Git(empty, "init", "--initial-branch=main");
    await Require(path, DivergenceState.Detached);
    await Require(empty, DivergenceState.Unborn);
    await File.WriteAllTextAsync(Path.Combine(output, "divergence.json"), JsonSerializer.Serialize(records, new JsonSerializerOptions { WriteIndented = true }));
    Console.WriteLine("PASS baseline divergence: 10 states and exact read-only preservation");
    return;
}

if (args.Length == 3 && args[2] == "--clone")
{
    var source = Path.Combine(root, "-source 日本語");
    Directory.CreateDirectory(source);
    await Git(source, "init", "--initial-branch=main");
    await File.WriteAllBytesAsync(Path.Combine(source, "file.txt"), Encoding.UTF8.GetBytes("base\n"));
    await File.WriteAllBytesAsync(Path.Combine(source, "asset.dat"), [0, 255, 1, 2]);
    await File.WriteAllBytesAsync(Path.Combine(source, ".gitattributes"), Encoding.UTF8.GetBytes("*.dat filter=fixture\n"));
    await Git(source, "add", "."); await Git(source, "commit", "-m", "base"); await Git(source, "tag", "v1");
    await Git(source, "checkout", "-b", "feature/topic");
    await File.WriteAllBytesAsync(Path.Combine(source, "file.txt"), Encoding.UTF8.GetBytes("feature\n"));
    await Git(source, "commit", "-am", "feature");
    await Git(source, "checkout", "main");
    await File.WriteAllBytesAsync(Path.Combine(source, "file.txt"), Encoding.UTF8.GetBytes("main\n"));
    await Git(source, "commit", "-am", "main");
    await File.WriteAllTextAsync(Path.Combine(output, "source-path.txt"), source);
    foreach (var scenario in new[] { "default", "branch", "tag", "hook-failure" })
    {
        var branch = scenario == "branch" ? "feature/topic" : scenario == "tag" ? "v1" : "";
        if (scenario == "hook-failure")
        {
            var hooks = Path.Combine(root, "hooks"); Directory.CreateDirectory(hooks);
            await File.WriteAllBytesAsync(Path.Combine(hooks, "post-checkout"), Encoding.UTF8.GetBytes("#!/bin/sh\nprintf 'ran\\n' > hook-ran\nexit 1\n"));
            await Git(root, "config", "--file", config, "core.hooksPath", hooks);
        }
        var path = Path.Combine(root, scenario);
        using var lifetime = ProcessLifetime.CompleteStartedCommands();
        var result = await actions.CloneRepositoryAsync(git, new(source, path, branch), default);
        var message = scenario == "hook-failure" ? "CloneIncomplete" : "CloneSucceeded";
        if (result.Action.MessageCode != message || result.Repository is null)
            throw new InvalidOperationException("Unexpected clone result: " + result.Action.MessageCode);
        var files = new SortedDictionary<string, string>();
        foreach (var name in new[] { ".gitattributes", "asset.dat", "file.txt", "hook-ran" })
            if (File.Exists(Path.Combine(path, name))) files[name] = Convert.ToHexString(await File.ReadAllBytesAsync(Path.Combine(path, name)));
        var state = new {
            message = result.Action.MessageCode,
            headFile = Convert.ToHexString(await File.ReadAllBytesAsync(Path.Combine(path, ".git", "HEAD"))),
            head = await Git(path, "rev-parse", "HEAD"),
            index = await Git(path, "ls-files", "--stage", "-z"),
            status = await Git(path, "status", "--porcelain", "-z"),
            config = Convert.ToHexString(await File.ReadAllBytesAsync(Path.Combine(path, ".git", "config"))),
            worktree = files
        };
        await File.WriteAllTextAsync(Path.Combine(output, scenario + ".json"), JsonSerializer.Serialize(state, new JsonSerializerOptions { WriteIndented = true }));
        Console.WriteLine("PASS baseline clone: " + scenario);
    }
    return;
}

if (args.Length == 3)
{
    foreach (var scenario in new[] { "configured", "existing", "partial" })
    {
        await File.WriteAllTextAsync(config, "[user]\nname=Fixture\nemail=fixture@example.invalid\n[commit]\ngpgsign=false\n[core]\nautocrlf=false\n[init]\ndefaultBranch=team-default\n");
        var templates = Path.Combine(root, scenario + "-templates");
        Directory.CreateDirectory(templates);
        if (scenario != "existing")
        {
            await File.WriteAllBytesAsync(Path.Combine(templates, "fixture-marker"), Encoding.UTF8.GetBytes("copied template"));
            if (scenario == "partial") await File.WriteAllBytesAsync(Path.Combine(templates, "objects"), Encoding.UTF8.GetBytes("blocked native object directory"));
            await Git(root, "config", "--file", config, "init.templateDir", templates);
        }
        var path = Path.Combine(root, scenario == "existing" ? "-new 日本語/child" : scenario == "configured" ? "-populated 日本語 folder" : "partial");
        if (scenario != "existing")
        {
            Directory.CreateDirectory(path);
            await File.WriteAllBytesAsync(Path.Combine(path, "keep.bin"), [0, 255, 1, 2]);
        }
        using var lifetime = ProcessLifetime.CompleteStartedCommands();
        var result = await actions.InitializeRepositoryAsync(git,
            new(path, scenario == "configured" ? "" : scenario == "existing" ? "work/start" : "main"), default);
        if (scenario == "existing")
        {
            await Git(path, "commit", "--allow-empty", "-m", "base");
            await File.WriteAllBytesAsync(Path.Combine(path, "kept"), Encoding.UTF8.GetBytes("untouched"));
            result = await actions.InitializeRepositoryAsync(git, new(path, "--ignored-invalid"), default);
        }
        var expectedMessage = scenario == "existing" ? "InitAlreadyExists" : scenario == "partial" ? "InitIncomplete" : "InitSucceeded";
        if (result.Action.MessageCode != expectedMessage || result.Repository is null)
            throw new InvalidOperationException("Unexpected initialization result: " + result.Action.MessageCode);
        async Task<string> Bytes(string name) => File.Exists(Path.Combine(path, name))
            ? Convert.ToHexString(await File.ReadAllBytesAsync(Path.Combine(path, name))) : "";
        var files = new SortedDictionary<string, string>();
        var file = scenario == "existing" ? "kept" : "keep.bin";
        files[file] = await Bytes(file);
        var state = new {
            message = result.Action.MessageCode,
            repositoryObserved = result.Repository is not null,
            headFile = await Bytes(".git/HEAD"),
            indexExists = File.Exists(Path.Combine(path, ".git", "index")),
            index = scenario == "partial" ? "" : await Git(path, "ls-files", "--stage", "-z"),
            status = scenario == "partial" ? "" : await Git(path, "status", "--porcelain", "-z"),
            tip = scenario == "existing" ? await Git(path, "rev-parse", "HEAD") : "",
            config = await Bytes(".git/config"),
            templateMarker = await Bytes(".git/fixture-marker"),
            worktree = files
        };
        await File.WriteAllTextAsync(Path.Combine(output, scenario + ".json"), JsonSerializer.Serialize(state, new JsonSerializerOptions { WriteIndented = true }));
        Console.WriteLine("PASS baseline initialization: " + scenario);
    }
    return;
}

foreach (var scenario in new[] { "stage-commit", "removed-commit", "external-index", "unstage" })
{
    var path = Path.Combine(root, scenario); Directory.CreateDirectory(path);
    await Git(path, "init", "--initial-branch=main");
    foreach (var name in new[] { "one.txt", "two.txt", "other.txt" })
        await File.WriteAllTextAsync(Path.Combine(path, name), "base\n");
    await Git(path, "add", "."); await Git(path, "commit", "-m", "base");
    await File.WriteAllTextAsync(Path.Combine(path, "one.txt"), "one\n");
    await File.WriteAllTextAsync(Path.Combine(path, "two.txt"), "two\n");
    var context = await actions.ReadWorkingCopyAsync(git, path, default);
    async Task Execute(WorkingCopyAction kind, string? name = null, string message = "")
    {
        var entries = name is null ? Array.Empty<LocalChange>() : context.Changes.Entries.Where(e => e.Path == name).ToArray();
        using var lifetime = ProcessLifetime.CompleteStartedCommands();
        var result = await actions.ExecuteWorkingCopyAsync(context, new(kind, entries, message), default);
        if (!result.Result.Success) throw new InvalidOperationException(result.Result.MessageCode);
        context = result.UpdatedContext ?? await actions.ReadWorkingCopyAsync(git, path, default);
    }
    switch (scenario)
    {
        case "stage-commit":
            await Execute(WorkingCopyAction.Stage, "one.txt");
            await Execute(WorkingCopyAction.Stage, "two.txt");
            await Execute(WorkingCopyAction.Commit, message: "queued commit");
            break;
        case "removed-commit":
            await Execute(WorkingCopyAction.Stage, "one.txt");
            // Removed requests execute nothing; the separate pinned WPF lifetime
            // suite checks its actual bound Activity command and draft ownership.
            break;
        case "external-index":
            await Git(path, "add", "one.txt");
            context = await actions.ReadWorkingCopyAsync(git, path, default);
            await Git(path, "add", "two.txt");
            try
            {
                await actions.ExecuteWorkingCopyAsync(context, new(WorkingCopyAction.Commit, [], "must remain"), default);
                throw new InvalidOperationException("External index change was accepted");
            }
            catch (GitActionException error) when (error.Code == "CommitStale") { }
            break;
        case "unstage":
            await Git(path, "add", "one.txt", "two.txt");
            context = await actions.ReadWorkingCopyAsync(git, path, default);
            await Execute(WorkingCopyAction.Unstage, "one.txt");
            await Execute(WorkingCopyAction.Unstage, "two.txt");
            break;
    }
    var files = new SortedDictionary<string, string>();
    foreach (var name in new[] { "one.txt", "two.txt", "other.txt" })
        files[name] = Convert.ToHexString(await File.ReadAllBytesAsync(Path.Combine(path, name)));
    var state = new {
        head = await Git(path, "rev-parse", "HEAD"),
        index = await Git(path, "ls-files", "--stage", "-z"),
        status = await Git(path, "status", "--porcelain", "-z"),
        worktree = files,
        config = Convert.ToHexString(await File.ReadAllBytesAsync(Path.Combine(path, ".git", "config")))
    };
    await File.WriteAllTextAsync(Path.Combine(output, scenario + ".json"), JsonSerializer.Serialize(state, new JsonSerializerOptions { WriteIndented = true }));
    Console.WriteLine("PASS baseline native outcome: " + scenario);
}

// The app deliberately clears inherited Git overrides. The fixture runner fixes
// only commit timestamps after that boundary, matching Rust's test Git environment.
sealed class FixedDateRunner : IProcessRunner
{
    private readonly WindowsProcessRunner _inner = new();
    public int RemoteWrites { get; private set; }
    public Task<ProcessResult> RunAsync(ProcessRequest request, CancellationToken token, Action<ProcessOutput>? output = null)
    {
        if (request.Arguments.Contains("remote") && request.Arguments.Any(a => a is "add" or "set-url" or "remove") ||
            request.Arguments.Contains("config") && request.Arguments.Contains("--unset-all")) RemoteWrites++;
        var environment = new Dictionary<string, string?>(request.Environment, StringComparer.OrdinalIgnoreCase)
        {
            ["GIT_AUTHOR_DATE"] = "2026-01-01T00:00:00+00:00",
            ["GIT_COMMITTER_DATE"] = "2026-01-01T00:00:00+00:00"
        };
        return _inner.RunAsync(request with { Environment = environment }, token, output);
    }
}
