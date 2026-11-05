using System.Text;

namespace GitGui.Core.Git;

public sealed partial class GitActions
{
    // All index/worktree mutations share this common-directory gate, including linked
    // worktrees and other app profiles. Callers acquire and release on one worker thread;
    // awaiting on that thread would make mutex ownership depend on the continuation.
    private static Mutex EnterMutationGate(RepositoryIdentity identity, string category = "GitAction")
    {
        var key = Convert.ToHexString(System.Security.Cryptography.SHA256.HashData(Encoding.UTF8.GetBytes(identity.CommonDirectory.ToUpperInvariant())));
        var gate = new Mutex(false, @"Local\Shovel." + category + "." + key);
        bool owned;
        try { owned = gate.WaitOne(0); } catch (AbandonedMutexException) { owned = true; }
        if (owned) return gate;
        gate.Dispose(); throw new GitActionException("ActionOtherInstance");
    }
}
