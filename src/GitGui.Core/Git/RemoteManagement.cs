using System.Text;
using System.Text.RegularExpressions;
using GitGui.Core.Processes;

namespace GitGui.Core.Git;

public sealed record RemoteConfiguration(string Name, string Location, string PushLocation, bool CanEdit);
public sealed record RemoteConfigurationSnapshot(IReadOnlyList<RemoteConfiguration> Remotes, string ConfigStamp);
public sealed record RemoteManagementOptions(string Name, string Location, string PushLocation, bool Remove = false);

public sealed partial class GitActions
{
    public Task<RemoteConfigurationSnapshot> ReadRemoteConfigurationAsync(GitInstallation git, string path, CancellationToken token) => Task.Run(async () =>
    {
        var stamp = await ConfigStamp(git, path, token);
        var result = await Raw(git, path, ["config", "--local", "--null", "--get-regexp", @"^remote\..*\.(url|pushurl)$"], token);
        if (result.Outcome != ProcessOutcome.Exited || result.ExitCode is not (0 or 1)) throw ReadFailure(result);
        var effective = await Raw(git, path, ["config", "--null", "--get-regexp", @"^remote\..*\.(url|pushurl)$"], token);
        if (effective.Outcome != ProcessOutcome.Exited || effective.ExitCode is not (0 or 1)) throw ReadFailure(effective);
        var allRecords = Decode(effective).Split('\0', StringSplitOptions.RemoveEmptyEntries);
        var values = new Dictionary<string, (List<string> Fetch, List<string> Push)>(StringComparer.Ordinal);
        foreach (var record in Decode(result).Split('\0', StringSplitOptions.RemoveEmptyEntries))
        {
            var split = record.IndexOf('\n'); if (split < 0) throw new GitActionException("ActionUnsupportedRemote");
            var match = Regex.Match(record[..split], @"^remote\.(.+)\.(url|pushurl)$", RegexOptions.CultureInvariant);
            if (!match.Success) throw new GitActionException("ActionUnsupportedRemote");
            var name = match.Groups[1].Value;
            if (!values.TryGetValue(name, out var lists)) { lists = ([], []); values.Add(name, lists); }
            (match.Groups[2].Value == "url" ? lists.Fetch : lists.Push).Add(record[(split + 1)..]);
        }
        if (values.Count > 32) throw new GitActionException("ActionUnsupportedRemote");
        var remotes = values.Select(item =>
        {
            var fetch = string.Join("\n", item.Value.Fetch); var push = string.Join("\n", item.Value.Push);
            var editable = item.Value.Fetch.Count == 1 && item.Value.Push.Count <= 1 && Redact(fetch) == fetch && Redact(push) == push &&
                allRecords.Count(record => record.StartsWith("remote." + item.Key + ".url\n", StringComparison.Ordinal)) == item.Value.Fetch.Count &&
                allRecords.Count(record => record.StartsWith("remote." + item.Key + ".pushurl\n", StringComparison.Ordinal)) == item.Value.Push.Count &&
                new RemoteSetupOptions(item.Key, fetch).IsValid && (push.Length == 0 || new RemoteSetupOptions(item.Key, push).IsValid);
            return new RemoteConfiguration(item.Key, Redact(fetch), Redact(push), editable);
        }).OrderBy(r => r.Name, StringComparer.Ordinal).ToArray();
        if (stamp != await ConfigStamp(git, path, token)) throw new GitActionException("RemoteManageChanged");
        return new RemoteConfigurationSnapshot(remotes, stamp);
    }, token);

    public Task<ActionResult> ChangeRemoteAsync(ActionContext context, RemoteConfigurationSnapshot reviewed,
        RemoteManagementOptions options, CancellationToken token, Action<ProcessOutput>? progress = null) => Task.Run(() =>
    {
        using var gate = EnterMutationGate(context.Identity);
        try { return Change().GetAwaiter().GetResult(); }
        finally { gate.ReleaseMutex(); }
        async Task<ActionResult> Change()
        {
            var remote = reviewed.Remotes.SingleOrDefault(r => r.Name == options.Name) ?? throw new GitActionException("RemoteManageChanged");
            if (!new RemoteSetupOptions(options.Name, "placeholder").IsValid) throw new GitActionException("RemoteAddInvalid");
            if (!options.Remove && (!remote.CanEdit || !new RemoteSetupOptions(options.Name, options.Location).IsValid ||
                options.PushLocation.Length != 0 && !new RemoteSetupOptions(options.Name, options.PushLocation).IsValid)) throw new GitActionException("RemoteManageUnsupported");
            var path = context.Identity.Worktree; var git = context.Git;
            if ((await identities.ReadAsync(path, token)).Fingerprint != context.Identity.Fingerprint) throw new GitActionException("ActionIdentityChanged");
            if (await ConfigStamp(git, path, token) != reviewed.ConfigStamp) throw new GitActionException("RemoteManageChanged");
            var commands = new List<string[]>();
            if (options.Remove) commands.Add(["remote", "remove", "--", remote.Name]);
            else
            {
                if (options.Location != remote.Location) commands.Add(["remote", "set-url", "--", remote.Name, options.Location]);
                if (options.PushLocation != remote.PushLocation)
                    commands.Add(options.PushLocation.Length == 0 ? ["config", "--local", "--unset-all", "--", "remote." + remote.Name + ".pushurl"] :
                        ["remote", "set-url", "--push", "--", remote.Name, options.PushLocation]);
            }
            var output = new StringBuilder(); ActionResult last = new(ProcessOutcome.Exited, 0, "");
            foreach (var command in commands)
            {
                token.ThrowIfCancellationRequested();
                var environment = EnvironmentForGit(); environment["GIT_OPTIONAL_LOCKS"] = null;
                var result = await runner.RunAsync(new(git.Executable, path, ["--no-pager", "--no-lazy-fetch", .. command])
                    { Environment = environment, Timeout = TimeSpan.FromSeconds(30), MaximumOutputBytes = 1024 * 1024 }, token, progress);
                output.Append(Redact(Decode(result) + Encoding.UTF8.GetString(result.StandardError)));
                last = new(result.Outcome, result.ExitCode, output.ToString());
                if (!last.Success) break;
            }
            return last;
        }
    }, token);
}
