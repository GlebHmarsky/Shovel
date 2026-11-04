using System.Security.Cryptography;
using System.Text;
using GitGui.Core.Processes;

namespace GitGui.Core.Git;

public sealed record GlobalGitIdentity(string Name, string Email, string Stamp);

public sealed partial class GitActions
{
    public Task<GlobalGitIdentity> ReadGlobalIdentityAsync(GitInstallation git, string directory, CancellationToken token) => Task.Run(async () =>
    {
        var result = await runner.RunAsync(new(git.Executable, directory, ["config", "--global", "--null", "--get-regexp", @"^user\.(name|email)$"])
            { Environment = EnvironmentForGit() }, token);
        // Missing identity values (including a missing config file) are normal exit 1.
        if (result.Outcome != ProcessOutcome.Exited || result.ExitCode is not (0 or 1)) throw ReadFailure(result);
        var pairs = Decode(result).Split('\0', StringSplitOptions.RemoveEmptyEntries);
        string Value(string key) => pairs.LastOrDefault(value => value.StartsWith(key + "\n", StringComparison.OrdinalIgnoreCase))?[(key.Length + 1)..] ?? "";
        return new GlobalGitIdentity(Value("user.name"), Value("user.email"), Convert.ToHexString(SHA256.HashData(result.StandardOutput)));
    }, token);

    public Task WriteGlobalIdentityAsync(GitInstallation git, string directory, GlobalGitIdentity reviewed, string name, string email, CancellationToken token) => Task.Run(async () =>
    {
        if (!ValidIdentityValue(name) || !ValidIdentityValue(email)) throw new GitActionException("SettingsIdentityInvalid");
        if (await ReadGlobalIdentityAsync(git, directory, token) != reviewed) throw new GitActionException("SettingsIdentityChanged");
        // Each native config write is locked by Git. If the second write fails the first is retained;
        // the UI reports possible partial completion and requires reloading before retry, never rolls it back.
        foreach (var (key, value, old) in new[] { ("user.name", name, reviewed.Name), ("user.email", email, reviewed.Email) })
        {
            if (value == old) continue;
            var result = await runner.RunAsync(new(git.Executable, directory, ["config", "--global", "--replace-all", "--", key, value])
                { Environment = EnvironmentForGit() }, token);
            if (result.Outcome != ProcessOutcome.Exited || result.ExitCode != 0) throw ReadFailure(result);
        }
    }, token);
    public static bool ValidIdentityValue(string text) => !string.IsNullOrWhiteSpace(text) && text.Length <= 512 && !text.Any(char.IsControl);
}
