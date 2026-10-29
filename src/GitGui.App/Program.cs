using System.IO;
using System.Security.Cryptography;
using System.Text;
using System.Windows;
using GitGui.App.Localization;
using Velopack;
using Velopack.Logging;

namespace GitGui.App;

public static class Program
{
    private static Mutex? _installedInstance;
    private static bool _ownsInstance;
    internal static string HandoffFile => Path.Combine(Directory.GetParent(AppContext.BaseDirectory.TrimEnd(Path.DirectorySeparatorChar))!.FullName, "shovel-update-pending");
    [STAThread]
    public static void Main(string[] args)
    {
        var locator = new Services.ShovelUpdateLocator();
        var fastHook = args.Length >= 2 && args[0] is "--veloapp-install" or "--veloapp-updated" or "--veloapp-obsolete" or "--veloapp-uninstall";
        if (locator.CurrentlyInstalledVersion is not null && !locator.IsPortable && !fastHook)
        {
            var identity = Convert.ToHexString(SHA256.HashData(Encoding.UTF8.GetBytes(AppContext.BaseDirectory.ToUpperInvariant())));
            _installedInstance = new Mutex(false, @"Global\Shovel.Installed." + identity);
            try { _ownsInstance = _installedInstance.WaitOne(0); } catch (AbandonedMutexException) { _ownsInstance = true; }
            var frameworkRestart = !string.IsNullOrEmpty(Environment.GetEnvironmentVariable("VELOPACK_RESTART")) ||
                !string.IsNullOrEmpty(Environment.GetEnvironmentVariable("VELOPACK_FIRSTRUN"));
            if (!_ownsInstance || File.Exists(HandoffFile) && !frameworkRestart)
            {
                MessageBox.Show(UiText.Get("UpdateOtherInstance"), UiText.Get("AppTitle"));
                if (_ownsInstance) _installedInstance.ReleaseMutex();
                _installedInstance.Dispose(); return;
            }
        }
        // Framework hooks run before WPF startup. Normal admission is locked before package cleanup.
        // Do not apply on launch/exit: only the explicit idle-gated action may do so.
        VelopackApp.Build().SetAutoApplyOnStartup(false).SetLocator(locator)
            .OnRestarted(_ => CancelHandoff()).OnFirstRun(_ => CancelHandoff()).Run();
        try { var app = new App(); app.InitializeComponent(); app.Run(); }
        finally { if (_ownsInstance) _installedInstance!.ReleaseMutex(); _installedInstance?.Dispose(); }
    }
    internal static void MarkHandoff() => File.WriteAllText(HandoffFile, "Pending explicit update. Repair with the latest installer if interrupted.");
    internal static void CancelHandoff() => File.Delete(HandoffFile);
}
