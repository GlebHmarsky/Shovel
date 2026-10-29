using System.Diagnostics;
using System.IO;
using Microsoft.Win32;

namespace GitGui.App.Services;

public enum RepositoryTool { Explorer, Bash, Rider, Code }
public sealed record RepositoryLaunch(string Executable, string Directory, IReadOnlyList<string> Arguments);
public interface IRepositoryTools
{
    Task<RepositoryLaunch?> PrepareAsync(RepositoryTool tool, string directory, string? git, string? configured, CancellationToken token);
    Task LaunchAsync(RepositoryLaunch request, CancellationToken token);
}

/// <summary>Explicit desktop handoff, separate from owned/cancellable Git command processes.</summary>
public sealed class RepositoryTools : IRepositoryTools
{
    public Task<RepositoryLaunch?> PrepareAsync(RepositoryTool tool, string directory, string? git, string? configured, CancellationToken token) => Task.Run(() =>
    {
        token.ThrowIfCancellationRequested();
        if (!Path.IsPathFullyQualified(directory) || !Directory.Exists(directory)) throw new DirectoryNotFoundException();
        var executable = configured ?? Candidates(tool, git).FirstOrDefault(File.Exists);
        if (executable is null || !File.Exists(executable)) return null;
        // Only inspect root-level project names after the user clicks Rider. No recursive scan.
        var target = directory;
        if (tool == RepositoryTool.Rider)
            foreach (var extension in new[] { "*.uproject", "*.slnx", "*.sln", "*.csproj" })
            {
                token.ThrowIfCancellationRequested();
                var candidates = Directory.EnumerateFiles(directory, extension, SearchOption.TopDirectoryOnly).Take(2).ToArray();
                if (candidates.Length == 1) { target = candidates[0]; break; }
                if (candidates.Length > 1) break; // Let Rider choose within the folder; do not guess a project.
            }
        return CreateLaunch(tool, executable, directory, target);
    }, token);

    public static RepositoryLaunch CreateLaunch(RepositoryTool tool, string executable, string directory, string target) =>
        new(executable, directory, tool switch
        {
            RepositoryTool.Bash => ["--cd=" + directory],
            RepositoryTool.Code => ["--new-window", directory],
            RepositoryTool.Rider => [target],
            _ => [directory]
        });

    public Task LaunchAsync(RepositoryLaunch request, CancellationToken token) => Task.Run(() =>
    {
        token.ThrowIfCancellationRequested();
        var start = new ProcessStartInfo(request.Executable) { WorkingDirectory = request.Directory, UseShellExecute = false };
        foreach (var argument in request.Arguments) start.ArgumentList.Add(argument);
        // Editors may hand off to an existing instance. Do not wait or kill their process on app exit.
        using var process = Process.Start(start) ?? throw new IOException();
    }, token);

    private static IEnumerable<string> Candidates(RepositoryTool tool, string? git)
    {
        var programs = Environment.GetFolderPath(Environment.SpecialFolder.ProgramFiles);
        var local = Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData);
        if (tool == RepositoryTool.Explorer) { yield return Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.Windows), "explorer.exe"); yield break; }
        if (tool == RepositoryTool.Bash)
        {
            var parent = git is null ? null : Directory.GetParent(git);
            for (var i = 0; i < 4 && parent is not null; i++, parent = parent.Parent) yield return Path.Combine(parent.FullName, "git-bash.exe");
            yield return Path.Combine(programs, "Git", "git-bash.exe");
            yield return Path.Combine(local, "Programs", "Git", "git-bash.exe");
            yield break;
        }
        var name = tool == RepositoryTool.Rider ? "rider64.exe" : "Code.exe";
        foreach (var hive in new[] { Registry.CurrentUser, Registry.LocalMachine })
        {
            string? registered = null;
            try { using var key = hive.OpenSubKey(@"Software\Microsoft\Windows\CurrentVersion\App Paths\" + name); registered = key?.GetValue(null) as string; }
            catch (System.Security.SecurityException) { }
            if (registered is not null && Path.IsPathFullyQualified(registered.Trim('"'))) yield return registered.Trim('"');
        }
        if (tool == RepositoryTool.Code)
        {
            yield return Path.Combine(local, "Programs", "Microsoft VS Code", name);
            yield return Path.Combine(programs, "Microsoft VS Code", name);
            yield break;
        }
        yield return Path.Combine(local, "Programs", "Rider", "bin", name);
        // Known standalone/current Toolbox install roots only, with bounded depth and enumeration.
        foreach (var root in new[] { Path.Combine(programs, "JetBrains"), Path.Combine(local, "Programs"), Path.Combine(local, "JetBrains", "Toolbox", "apps") })
            foreach (var install in Directories(root).Where(p => Path.GetFileName(p).Contains("Rider", StringComparison.OrdinalIgnoreCase)))
            {
                yield return Path.Combine(install, "bin", name);
                foreach (var channel in Directories(install))
                {
                    yield return Path.Combine(channel, "bin", name);
                    foreach (var version in Directories(channel)) yield return Path.Combine(version, "bin", name);
                }
            }
    }
    private static string[] Directories(string root)
    {
        try { return Directory.Exists(root) ? Directory.EnumerateDirectories(root).Take(32).OrderDescending(StringComparer.OrdinalIgnoreCase).ToArray() : []; }
        catch (IOException) { return []; }
        catch (UnauthorizedAccessException) { return []; }
    }
}
