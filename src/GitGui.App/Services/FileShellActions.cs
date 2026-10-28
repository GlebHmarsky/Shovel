using System.Diagnostics;
using System.IO;
using System.Runtime.InteropServices;
using GitGui.Core.Git;

namespace GitGui.App.Services;

public sealed record FileShellTarget(string Path, bool SelectInExplorer, bool MissingFileFolder);

/// <summary>Explicit user handoff to Windows, never a Git or menu-opening prerequisite.</summary>
public static class FileShellActions
{
    public static Task SavePatchAsync(string destination, byte[] bytes, string root, IReadOnlyList<string> sourcePaths, CancellationToken token) => Task.Run(async () =>
    {
        destination = Path.GetFullPath(destination);
        if (destination.Split(Path.DirectorySeparatorChar, Path.AltDirectorySeparatorChar).Any(part => part.Equals(".git", StringComparison.OrdinalIgnoreCase)) ||
            sourcePaths.Any(path => string.Equals(destination, RepositoryReader.WorkingCopyPath(root, path), StringComparison.OrdinalIgnoreCase)))
            throw new GitActionException("FilePatchDestination");
        var temporary = Path.Combine(Path.GetDirectoryName(destination)!, ".shovel-patch-" + Guid.NewGuid().ToString("N"));
        try
        {
            await File.WriteAllBytesAsync(temporary, bytes, token); token.ThrowIfCancellationRequested();
            File.Move(temporary, destination, overwrite: true);
        }
        finally { try { File.Delete(temporary); } catch (IOException) { } catch (UnauthorizedAccessException) { } }
    }, token);
    private static int _active;
    public static Task<bool> OpenAsync(string root, string relative, bool explorer, CancellationToken token)
    {
        if (Interlocked.CompareExchange(ref _active, 1, 0) != 0) throw new GitActionException("FileMenuShellBusy");
        var completion = new TaskCompletionSource<bool>(TaskCreationOptions.RunContinuationsAsynchronously);
        _ = completion.Task.ContinueWith(task => { _ = task.Exception; }, CancellationToken.None, TaskContinuationOptions.OnlyOnFaulted | TaskContinuationOptions.ExecuteSynchronously, TaskScheduler.Default);
        var worker = new Thread(() =>
        {
            try
            {
                token.ThrowIfCancellationRequested(); var target = PrepareTarget(root, relative, explorer);
                token.ThrowIfCancellationRequested();
                Marshal.ThrowExceptionForHR(CoInitializeEx(IntPtr.Zero, 2));
                try
                {
                    if (target.SelectInExplorer)
                    {
                        Marshal.ThrowExceptionForHR(SHParseDisplayName(target.Path, IntPtr.Zero, out var pidl, 0, out _));
                        try { Marshal.ThrowExceptionForHR(SHOpenFolderAndSelectItems(pidl, 0, IntPtr.Zero, 0)); }
                        finally { Marshal.FreeCoTaskMem(pidl); }
                    }
                    else using (Process.Start(new ProcessStartInfo(target.Path) { UseShellExecute = true, Verb = "open" })) { }
                    completion.TrySetResult(target.MissingFileFolder);
                }
                finally { CoUninitialize(); }
            }
            catch (Exception error) { completion.TrySetException(error); }
            finally { Volatile.Write(ref _active, 0); }
        }) { IsBackground = true, Name = "Shovel file handoff" };
        try { worker.SetApartmentState(ApartmentState.STA); worker.Start(); }
        catch { Volatile.Write(ref _active, 0); throw; }
        // A slow shell extension must not hold the UI/close operation. The single native
        // worker remains bounded until it returns; cancellation cannot retract a sent handoff.
        return completion.Task.WaitAsync(TimeSpan.FromSeconds(15), token);
    }
    public static FileShellTarget PrepareTarget(string root, string relative, bool explorer)
    {
        root = Path.TrimEndingDirectorySeparator(Path.GetFullPath(root));
        var file = RepositoryReader.WorkingCopyPath(root, relative);
        var target = file;
        for (var part = file; ; part = Path.GetDirectoryName(part)!)
        {
            if (Path.Exists(part) && File.GetAttributes(part).HasFlag(FileAttributes.ReparsePoint)) throw new GitActionException("FileMenuLinkedPath");
            if (string.Equals(part, Path.TrimEndingDirectorySeparator(root), StringComparison.OrdinalIgnoreCase)) break;
        }
        if (File.Exists(file)) return new(file, explorer, false);
        if (!explorer) throw new GitActionException("FileMenuMissingFile");
        while (!Directory.Exists(target))
        {
            if (string.Equals(target, Path.TrimEndingDirectorySeparator(root), StringComparison.OrdinalIgnoreCase)) throw new GitActionException("FileMenuMissingFile");
            target = Path.GetDirectoryName(target)!;
        }
        return new(target, false, true);
    }
    [DllImport("ole32.dll", ExactSpelling = true)] private static extern int CoInitializeEx(IntPtr reserved, uint model);
    [DllImport("ole32.dll", ExactSpelling = true)] private static extern void CoUninitialize();
    [DllImport("shell32.dll", CharSet = CharSet.Unicode, ExactSpelling = true)]
    private static extern int SHParseDisplayName(string name, IntPtr binding, out IntPtr pidl, uint flags, out uint attributes);
    [DllImport("shell32.dll", ExactSpelling = true)]
    private static extern int SHOpenFolderAndSelectItems(IntPtr pidl, uint count, IntPtr children, uint flags);
}
