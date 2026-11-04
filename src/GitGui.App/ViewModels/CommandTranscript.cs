using System.Text;
using GitGui.Core.Git;
using GitGui.Core.Processes;

namespace GitGui.App.ViewModels;

// Native output remains separate from command metadata until presentation.
internal sealed class CommandTranscript
{
    private readonly StringBuilder _commands = new();
    public void Collect(ProcessOutput output)
    {
        if (!output.IsCommand) return;
        lock (_commands)
        {
            var text = Encoding.UTF8.GetString(output.Bytes);
            if (_commands.Length + text.Length <= ActivityJournal.OutputLimit) _commands.Append(text);
        }
    }
    public string Complete(string output)
    {
        lock (_commands)
        {
            var text = _commands + GitActions.Redact(output);
            _commands.Clear();
            return text;
        }
    }
    public void Clear() { lock (_commands) _commands.Clear(); }
}
