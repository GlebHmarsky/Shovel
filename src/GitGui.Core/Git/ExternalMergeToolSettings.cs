using System.Text.Json.Serialization;

namespace GitGui.Core.Git;

public enum ExternalMergeToolMode { GitDefault, NamedTool, CustomCommand }

// Saved by Shovel, applied only to the explicit mergetool invocation. This never
// rewrites repository or global Git configuration, or probes the file system.
public sealed record ExternalMergeToolSettings
{
    public ExternalMergeToolMode Mode { get; init; }
    public string ToolName { get; init; } = "";
    public string ExecutablePath { get; init; } = "";
    public string Command { get; init; } = "";

    [JsonIgnore]
    public string? ValidationCode => Mode switch
    {
        ExternalMergeToolMode.GitDefault => null,
        ExternalMergeToolMode.NamedTool when !ValidName(ToolName) => "ConflictToolNameInvalid",
        ExternalMergeToolMode.NamedTool when !ValidPath(ExecutablePath) => "ConflictToolPathInvalid",
        ExternalMergeToolMode.NamedTool => null,
        ExternalMergeToolMode.CustomCommand when string.IsNullOrWhiteSpace(Command) || Command.Length > 8192 || Command.Contains('\0') => "ConflictToolCommandInvalid",
        ExternalMergeToolMode.CustomCommand => null,
        _ => "ConflictToolSettingsInvalid"
    };

    public ExternalMergeToolSettings Normalize() => ValidationCode is not null ? new() : Mode switch
    {
        ExternalMergeToolMode.NamedTool => this with { Command = "" },
        ExternalMergeToolMode.CustomCommand => this with { ToolName = "", ExecutablePath = "" },
        _ => new()
    };

    internal static bool ValidName(string? name) => name is { Length: > 0 and <= 128 } && !name.StartsWith('-') &&
        name.All(c => char.IsAsciiLetterOrDigit(c) || c is '-' or '_' or '.');

    private static bool ValidPath(string? path) => path is not null && (path.Length == 0 ||
        path.Length <= 32767 && !path.Any(char.IsControl) && path.IndexOfAny(Path.GetInvalidPathChars()) < 0 &&
        Path.IsPathFullyQualified(path) && Path.GetExtension(path).Equals(".exe", StringComparison.OrdinalIgnoreCase));
}
