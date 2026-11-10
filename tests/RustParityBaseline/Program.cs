// Reference-only native outcome probe. Never included in the Rust payload.
using System.Text;
using System.Text.Json;
using GitGui.Core.Git;
using GitGui.Core.Processes;
using GitGui.ProcessHost;

if (args.Length is not (2 or 3) || args.Length == 3 && args[2] is not ("--init" or "--clone")) throw new ArgumentException("Expected new fixture directory, output directory and optional --init or --clone.");
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
    public Task<ProcessResult> RunAsync(ProcessRequest request, CancellationToken token, Action<ProcessOutput>? output = null)
    {
        var environment = new Dictionary<string, string?>(request.Environment, StringComparer.OrdinalIgnoreCase)
        {
            ["GIT_AUTHOR_DATE"] = "2026-01-01T00:00:00+00:00",
            ["GIT_COMMITTER_DATE"] = "2026-01-01T00:00:00+00:00"
        };
        return _inner.RunAsync(request with { Environment = environment }, token, output);
    }
}
