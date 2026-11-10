// Reference-only native outcome probe. Never included in the Rust payload.
using System.Text;
using System.Text.Json;
using GitGui.Core.Git;
using GitGui.Core.Processes;
using GitGui.ProcessHost;

if (args.Length != 2) throw new ArgumentException("Expected new fixture directory and output directory.");
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
