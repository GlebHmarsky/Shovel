namespace GitGui.Core.Git;

public sealed partial class GitActions
{
    public Task<ActionOptions> ResolveDefaultNetworkAsync(ActionContext context, ActionOptions options, CancellationToken token) => Task.Run(async () =>
    {
        if (options.Action == GitAction.Fetch)
        {
            // Git's branch remote preference, then origin, or the only available remote.
            var configured = context.Repository.Branch is { } branch
                ? await Raw(context.Git, context.Identity.Worktree, ["config", "--get", "branch." + branch + ".remote"], token) : null;
            if (configured is not null && (configured.Outcome != Processes.ProcessOutcome.Exited || configured.ExitCode is not (0 or 1)))
                throw new GitReadException(GitFailure.CommandFailed);
            var name = configured?.ExitCode == 0 ? Decode(configured).TrimEnd('\r', '\n') :
                context.Remotes.Any(remote => remote.Name == "origin") ? "origin" : context.Remotes.Count == 1 ? context.Remotes[0].Name : "";
            if (!context.Remotes.Any(remote => remote.Name == name)) throw new GitActionException("DefaultFetchNeedsTarget");
            return options with { Remote = name, AllRemotes = false };
        }
        if (options.Action != GitAction.Push || context.Repository.HeadKind != HeadKind.Branch)
            throw new GitActionException("DefaultPushNeedsTarget");
        // Ask Git for the push remote (including branch.pushRemote / remote.pushDefault).
        // push:remoteref is empty on supported Git versions for default refspecs, so
        // resolve the documented single-branch modes from local config below.
        var localRef = "refs/heads/" + context.Repository.Branch;
        var result = await Raw(context.Git, context.Identity.Worktree,
            ["for-each-ref", "--count=1", "--format=%(refname)%00%(push:remotename)%00%(upstream:remotename)%00%(upstream:remoteref)", "--", localRef], token);
        token.ThrowIfCancellationRequested();
        var fields = Decode(result).TrimEnd('\r', '\n').Split('\0');
        if (result.Outcome != Processes.ProcessOutcome.Exited || result.ExitCode != 0 || fields.Length != 4 || fields[0] != localRef ||
            !context.Remotes.Any(remote => remote.Name == fields[1])) throw new GitActionException("DefaultPushNeedsTarget");
        var settings = await Task.WhenAll(
            Raw(context.Git, context.Identity.Worktree, ["config", "--get-all", "remote." + fields[1] + ".push"], token),
            Raw(context.Git, context.Identity.Worktree, ["config", "--get", "push.default"], token));
        if (settings.Any(item => item.Outcome != Processes.ProcessOutcome.Exited || item.ExitCode is not (0 or 1)))
            throw new GitReadException(GitFailure.CommandFailed);
        // A configured refspec can publish multiple refs or force updates. The options
        // dialog is the explicit target route; the default button never guesses its scope.
        if (settings[0].ExitCode == 0) throw new GitActionException("DefaultPushNeedsTarget");
        var mode = settings[1].ExitCode == 0 ? Decode(settings[1]).TrimEnd('\r', '\n') : "simple";
        var destination = mode switch
        {
            "current" => localRef,
            "upstream" or "tracking" when fields[1] == fields[2] => fields[3],
            "simple" when fields[2].Length > 0 && fields[1] != fields[2] => localRef,
            "simple" when fields[1] == fields[2] && fields[3] == localRef => localRef,
            _ => ""
        };
        if (!destination.StartsWith("refs/heads/", StringComparison.Ordinal)) throw new GitActionException("DefaultPushNeedsTarget");
        return options with { Branch = context.Repository.Branch!, Remote = fields[1], RemoteBranch = destination[11..], AllTags = false, SetUpstream = false };
    }, token);
}
