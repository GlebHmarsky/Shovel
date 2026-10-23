using System.Diagnostics;
using System.Text.Json;
using GitGui.Core.Processes;

namespace GitGui.ProcessHost;

internal static class Program
{
    // Parent attaches this waiting host to a job BEFORE sending a request. Never launch from arguments.
    private static async Task<int> Main()
    {
        try
        {
            using var input = Console.OpenStandardInput();
            using var payload = new MemoryStream();
            var buffer = new byte[4096];
            int read;
            while ((read = await input.ReadAsync(buffer)) != 0)
            {
                if (payload.Length + read > 128 * 1024) return 125;
                payload.Write(buffer, 0, read);
            }
            var request = JsonSerializer.Deserialize<ProcessRequest>(payload.ToArray());
            if (request is null || !Path.IsPathFullyQualified(request.Executable) ||
                !request.Executable.EndsWith(".exe", StringComparison.OrdinalIgnoreCase) ||
                !Path.IsPathFullyQualified(request.WorkingDirectory)) return 125;
            using var child = new Process
            {
                StartInfo = new(request.Executable)
                {
                    WorkingDirectory = request.WorkingDirectory, UseShellExecute = false,
                    CreateNoWindow = true, RedirectStandardInput = true,
                    RedirectStandardOutput = true, RedirectStandardError = true
                }
            };
            foreach (var argument in request.Arguments) child.StartInfo.ArgumentList.Add(argument);
            foreach (var (key, value) in request.Environment)
            {
                if (value is null) child.StartInfo.Environment.Remove(key);
                else child.StartInfo.Environment[key] = value;
            }
            child.Start();
            child.StandardInput.Close(); // Read-only commands cannot wait for invisible input.
            var stdout = child.StandardOutput.BaseStream.CopyToAsync(Console.OpenStandardOutput());
            var stderr = child.StandardError.BaseStream.CopyToAsync(Console.OpenStandardError());
            await child.WaitForExitAsync();
            await Task.WhenAll(stdout, stderr);
            return child.ExitCode;
        }
        catch (Exception) { return 125; } // Protocol/start failure; never expose request or exception text.
    }
}
