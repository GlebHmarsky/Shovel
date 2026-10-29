using System.Globalization;
using System.Text.RegularExpressions;

namespace GitGui.Core.Git;

public sealed partial class GitActions
{
    public static bool IsHistoryAction(GitAction action) => action is GitAction.CheckoutCommit or GitAction.ResetSoft or GitAction.ResetMixed or GitAction.ResetHard or GitAction.RevertCommit;
    public static bool IsReset(GitAction action) => action is GitAction.ResetSoft or GitAction.ResetMixed or GitAction.ResetHard;

    private async Task<string[]> HistoryCommandAsync(ActionContext context, ActionOptions options, CancellationToken token)
    {
        var git = context.Git; var path = context.Identity.Worktree; var target = options.StartRef;
        if (!Regex.IsMatch(target, "^[0-9a-f]{40}([0-9a-f]{24})?$")) throw new GitActionException("HistoryTargetUnavailable");
        // Read immutable commit metadata, not graph-projected stash parents or a ref name.
        var metadata = Decode(await Read(git, path, ["cat-file", "commit", target], token)).Split("\n\n", 2)[0];
        var parents = metadata.Split('\n').Count(line => line.StartsWith("parent ", StringComparison.Ordinal));
        if (options.Action == GitAction.CheckoutCommit)
            return ["-c", "submodule.recurse=false", "switch", "--detach", "--no-guess", "--no-overwrite-ignore", "--no-recurse-submodules", target];
        if (IsReset(options.Action))
        {
            if (context.Repository.HeadKind != HeadKind.Branch) throw new GitActionException("HistoryResetNeedsBranch");
            var worktrees = Decode(await Read(git, path, ["worktree", "list", "--porcelain", "-z"], token));
            if (worktrees.Split('\0').Count(field => field == "branch refs/heads/" + context.Repository.Branch) > 1)
                throw new GitActionException("BranchManageOccupied");
            var mode = options.Action == GitAction.ResetSoft ? "--soft" : options.Action == GitAction.ResetMixed ? "--mixed" : "--hard";
            return ["-c", "submodule.recurse=false", "reset", mode, "--no-recurse-submodules", target, "--"];
        }
        if (parents > 1 ? options.Mainline < 1 || options.Mainline > parents : options.Mainline != 0)
            throw new GitActionException("HistoryRevertMainlineRequired");
        return ["-c", "submodule.recurse=false", "revert", "--no-edit", .. options.Mainline == 0 ? Array.Empty<string>() :
            new[] { "--mainline", options.Mainline.ToString(CultureInfo.InvariantCulture) }, target];
    }
}
