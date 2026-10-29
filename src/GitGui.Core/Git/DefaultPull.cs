namespace GitGui.Core.Git;

public sealed partial class GitActions
{
    // Called after explicit submission only. Do not mistake a tracking-ref suffix for
    // its server branch when custom refspecs or changed configuration are present.
    public Task ValidateDefaultPullTargetAsync(ActionContext context, string remote, string branch, CancellationToken token) => Task.Run(async () =>
    {
        if (context.Repository.HeadKind != HeadKind.Branch) throw new GitActionException("ActionPullNeedsBranch");
        var localRef = "refs/heads/" + context.Repository.Branch;
        var data = await Read(context.Git, context.Identity.Worktree,
            ["for-each-ref", "--count=1", "--format=%(refname)%00%(upstream:remotename)%00%(upstream:remoteref)", "--", localRef], token);
        if (Decode(data).TrimEnd('\r', '\n') != localRef + "\0" + remote + "\0refs/heads/" + branch)
            throw new GitActionException("PullDefaultTargetChanged");
    }, token);
}
