using GitGui.Core.Processes;

namespace GitGui.Core.Git;

public enum CheckoutChanges { Keep, StashAndReapply, DiscardTracked }

public sealed partial class GitActions
{
    public static bool IsCheckout(ActionOptions options) => options.Action is GitAction.CheckoutCommit or GitAction.SwitchBranch ||
        options.Action == GitAction.NewBranch && options.Checkout;

    private async Task<ActionResult> ExecuteCheckoutAsync(ActionContext context, ActionOptions options, string[] command,
        Func<string[], Task<ActionResult>> write, CancellationToken token)
    {
        if (!Enum.IsDefined(options.CheckoutChanges)) throw new GitActionException("ActionReadFailed");
        var git = context.Git; var path = context.Identity.Worktree;
        if (options.CheckoutReview is { } reviewed)
        {
            var current = await ReadWorkingCopyAsync(git, path, token);
            if (current.Identity.Fingerprint != reviewed.Identity.Fingerprint || current.Repository.Commit != reviewed.Repository.Commit ||
                current.Repository.Branch != reviewed.Repository.Branch || current.IndexStamp != reviewed.IndexStamp ||
                current.ConfigStamp != reviewed.ConfigStamp || !current.Changes.Entries.SequenceEqual(reviewed.Changes.Entries))
                throw new GitActionException("CheckoutChanged");
        }
        else if (options.CheckoutChanges != CheckoutChanges.Keep) throw new GitActionException("CheckoutChanged");

        string? stash = null;
        if (options.CheckoutChanges == CheckoutChanges.StashAndReapply && options.CheckoutReview!.Changes.Entries.Count > 0)
        {
            var prior = await StashTip();
            var saved = await write(["stash", "push", "--include-untracked", .. options.CheckoutStashMessage.Length > 0 ? new[] { "--message", options.CheckoutStashMessage } : []]);
            if (!saved.Success) return saved;
            stash = await StashTip();
            if (stash is null || stash == prior) throw new GitActionException("CheckoutStashUnavailable");
        }
        else if (options.CheckoutChanges == CheckoutChanges.DiscardTracked)
        {
            // Preserve untracked/ignored files and submodule worktrees. Native switch still
            // refuses collisions; configured checkout hooks and LFS filters run normally.
            var discarded = await write(["-c", "submodule.recurse=false", "restore", "--source=HEAD", "--staged", "--worktree", "--", "."]);
            if (!discarded.Success) return discarded;
        }
        var switched = await write(command);
        // Never roll back a failed switch or remove recovery data. The saved stash remains
        // discoverable even if a hook fails after HEAD has already changed.
        if (!switched.Success) return stash is null ? switched : switched with { MessageCode = "CheckoutStashRetained" };
        if (stash is null) return switched;
        var applied = await write(["stash", "apply", "--index", stash]);
        var unmerged = await Read(git, path, ["ls-files", "--unmerged", "-z"], token);
        if (!applied.Success || unmerged.StandardOutput.Length > 0)
            return applied with { HasConflicts = unmerged.StandardOutput.Length > 0, MessageCode = "CheckoutStashRetained" };
        // Drop only the entry we created, and only while it is still at the reviewed slot.
        // An external stash operation must never cause us to delete somebody else's entry.
        if (await StashTip() != stash) return applied with { MessageCode = "CheckoutStashRetained" };
        var dropped = await write(["stash", "drop", "stash@{0}"]);
        return dropped.Success ? applied : dropped with { MessageCode = "CheckoutStashRetained" };

        async Task<string?> StashTip()
        {
            var result = await Raw(git, path, ["rev-parse", "--verify", "--quiet", "refs/stash"], token);
            if (result.Outcome != ProcessOutcome.Exited || result.ExitCode is not (0 or 1)) throw new GitActionException("CheckoutStashUnavailable");
            return result.ExitCode == 0 ? Decode(result).Trim() : null;
        }
    }
}
