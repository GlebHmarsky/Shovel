using System.Text;
using GitGui.Core.Processes;

namespace GitGui.Core.Git;

public sealed record RemoteSetupOptions(string Name, string Location)
{
    public bool IsValid => Name.Length is > 0 and <= 255 && !Name.StartsWith('-') &&
        !Name.Any(char.IsWhiteSpace) && !Name.Any(char.IsControl) &&
        Location.Length is > 0 and <= 32767 && !Location.Any(char.IsControl) &&
        !(Uri.TryCreate(Location, UriKind.Absolute, out var uri) &&
          uri.Scheme is "http" or "https" && (uri.UserInfo.Contains(':') || uri.Query.Length != 0));
}

public sealed partial class GitActions
{
    public Task<ActionContext> PrepareRemoteSetupAsync(ActionContext cached, CancellationToken token) => Task.Run(async () =>
    {
        if ((await identities.ReadAsync(cached.Identity.Worktree, token)).Fingerprint != cached.Identity.Fingerprint)
            throw new GitActionException("ActionIdentityChanged");
        return cached with { ConfigStamp = await ConfigStamp(cached.Git, cached.Identity.Worktree, token) };
    }, token);

    public Task<ActionResult> AddRemoteAsync(ActionContext context, RemoteSetupOptions options,
        CancellationToken token, Action<ProcessOutput>? progress = null) => Task.Run(() =>
    {
        using var gate = EnterMutationGate(context.Identity);
        try { return Execute().GetAwaiter().GetResult(); }
        finally { gate.ReleaseMutex(); }

        async Task<ActionResult> Execute()
        {
            if (!options.IsValid) throw new GitActionException("RemoteAddInvalid");
            var path = context.Identity.Worktree; var git = context.Git;
            if ((await identities.ReadAsync(path, token)).Fingerprint != context.Identity.Fingerprint)
                throw new GitActionException("ActionIdentityChanged");
            await Read(git, path, ["check-ref-format", "refs/remotes/" + options.Name + "/probe"], token);
            if (await ConfigStamp(git, path, token) != context.ConfigStamp)
                throw new GitActionException("BranchManageChanged");
            token.ThrowIfCancellationRequested();
            // No -f: save local configuration only. Git rejects duplicate names and owns
            // config.lock; no global edits, shell expansion or automatic network request.
            var environment = EnvironmentForGit(); environment["GIT_OPTIONAL_LOCKS"] = null;
            var result = await runner.RunAsync(new(git.Executable, path,
                ["--no-pager", "--no-lazy-fetch", "remote", "add", "--", options.Name, options.Location])
                { Environment = environment, Timeout = TimeSpan.FromSeconds(30), MaximumOutputBytes = 1024 * 1024 }, token, progress);
            return new(result.Outcome, result.ExitCode, Redact(Decode(result) + Encoding.UTF8.GetString(result.StandardError)));
        }
    }, token);
}
