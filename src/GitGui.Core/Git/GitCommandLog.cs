using System.Text;
using GitGui.Core.Processes;

namespace GitGui.Core.Git;

public sealed partial class GitActions
{
    // Command headers are session output, never diagnostic events. Keep native
    // stdout untouched: write-tree and other plumbing callers parse its bytes.
    private Task<ProcessResult> RunLoggedAsync(ProcessRequest request, CancellationToken token, Action<ProcessOutput>? progress = null)
    {
        token.ThrowIfCancellationRequested();
        progress?.Invoke(new(false, Encoding.UTF8.GetBytes(FormatCommand(request.Arguments))) { IsCommand = true });
        return runner.RunAsync(request, token, progress);
    }

    public static string FormatCommand(IReadOnlyList<string> arguments)
    {
        var visible = new List<string>();
        var started = false;
        for (var i = 0; i < arguments.Count; i++)
        {
            var value = arguments[i];
            if (value == "-c") { i++; continue; } // Includes private helper commands/configuration.
            if (!started && value.StartsWith('-')) continue;
            started = true;
            visible.Add(Quote(Redact(value)));
            if (value is "-m" or "--message" or "-F" or "--file")
            { if (++i < arguments.Count) visible.Add("***"); }
            if (visible.Count >= 64) { visible.Add("…"); break; }
        }
        var line = "$ git " + string.Join(" ", visible);
        return (line.Length <= 4096 ? line : line[..4096] + "…") + "\n";
    }

    private static string Quote(string value)
    {
        var singleLine = value.Replace("\r", "\\r").Replace("\n", "\\n").Replace("\t", "\\t");
        return singleLine.Any(char.IsWhiteSpace) || singleLine.Contains('"')
            ? "\"" + singleLine.Replace("\"", "\\\"") + "\"" : singleLine;
    }
}
