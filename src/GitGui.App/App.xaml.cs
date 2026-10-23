using System.ComponentModel;
using System.IO;
using System.Security.Cryptography;
using System.Text;
using System.Windows;
using System.Windows.Threading;
using GitGui.App.Localization;
using GitGui.App.ViewModels;
using GitGui.App.Views;
using GitGui.Core.Diagnostics;
using GitGui.Core.Settings;

namespace GitGui.App;

/// <summary>Composition root and lifetime owner. Views contain only presentation behavior.</summary>
public partial class App : Application
{
    private readonly CancellationTokenSource _startupCancellation = new();
    private Mutex? _profileMutex;
    private bool _ownsProfile;
    private DiagnosticLog? _log;
    private SettingsStore? _settings;
    private ShellViewModel? _shell;
    private Task _startupTask = Task.CompletedTask;
    private bool _closing;
    private bool _allowClose;
    private bool _fatal;

    protected override void OnStartup(StartupEventArgs e)
    {
        base.OnStartup(e);
        DispatcherUnhandledException += OnDispatcherException;
        AppDomain.CurrentDomain.UnhandledException += OnDomainException;
        try
        {
            var directory = ResolveDataDirectory(e.Args);
            // One writer per local user/profile; the hash keeps the path out of the mutex name.
            var identity = Convert.ToHexString(SHA256.HashData(Encoding.UTF8.GetBytes(directory.ToUpperInvariant())));
            _profileMutex = new Mutex(false, @"Local\GitGui.Foundation." + identity);
            try { _ownsProfile = _profileMutex.WaitOne(0); }
            catch (AbandonedMutexException) { _ownsProfile = true; }
            if (!_ownsProfile)
            {
                MessageBox.Show(UiText.Get("AlreadyRunning"), UiText.Get("AppTitle"),
                    MessageBoxButton.OK, MessageBoxImage.Information);
                Shutdown();
                return;
            }

            _log = new DiagnosticLog(directory);
            _log.Record(DiagnosticEvent.Started);
            _settings = new SettingsStore(directory);
            _shell = new ShellViewModel();
            var window = new MainWindow(_shell);
            MainWindow = window;
            window.Closing += OnWindowClosing;
            window.Show();
            _startupTask = InitializeAsync(window, _startupCancellation.Token);
        }
        catch (Exception exception) { FailAndExit(exception); }
    }

    private async Task InitializeAsync(MainWindow window, CancellationToken cancellationToken)
    {
        try
        {
            // File opening, JSON parsing and directory work must also stay off the dispatcher.
            var result = await Task.Run(() => _settings!.LoadAsync(cancellationToken), cancellationToken);
            if (_closing || _fatal) return;
            window.RestoreLayout(result.Settings);
            _shell!.CompleteStartup(result);
            _log!.Record(result.CanSave ? DiagnosticEvent.SettingsLoaded : DiagnosticEvent.SettingsReadOnly);
        }
        catch (OperationCanceledException) when (cancellationToken.IsCancellationRequested) { }
        catch (Exception exception) { FailAndExit(exception); }
    }

    private async void OnWindowClosing(object? sender, CancelEventArgs e)
    {
        if (_allowClose) return;
        e.Cancel = true;
        if (_closing || sender is not MainWindow window) return;
        _closing = true;
        _shell!.BeginShutdown();
        _startupCancellation.Cancel();
        try
        {
            await _startupTask;
            if (_fatal) return;
            if (_shell.LoadedSettings?.CanSave == true)
            {
                var snapshot = window.CaptureLayout();
                try
                {
                    using var timeout = new CancellationTokenSource(TimeSpan.FromSeconds(5));
                    await Task.Run(() => _settings!.SaveAsync(snapshot, timeout.Token), timeout.Token);
                    _log!.Record(DiagnosticEvent.SettingsSaved);
                }
                catch (Exception exception) when (exception is IOException or UnauthorizedAccessException or OperationCanceledException)
                {
                    _log!.Record(DiagnosticEvent.SettingsSaveFailed, exception);
                    var answer = MessageBox.Show(window, UiText.Get("SettingsSaveFailedQuestion"), UiText.Get("AppTitle"),
                        MessageBoxButton.YesNo, MessageBoxImage.Warning, MessageBoxResult.No);
                    if (answer != MessageBoxResult.Yes)
                    {
                        _closing = false;
                        _shell.ResumeAfterSaveFailure();
                        return;
                    }
                }
            }
            _log!.Record(DiagnosticEvent.Stopped);
            // Diagnostics are best effort and must never indefinitely hold shutdown open.
            try { await _log.DisposeAsync().AsTask().WaitAsync(TimeSpan.FromSeconds(2)); }
            catch (TimeoutException) { }
            _allowClose = true;
            window.Close();
            Shutdown();
        }
        catch (Exception exception) { FailAndExit(exception); }
    }

    private static string ResolveDataDirectory(string[] arguments)
    {
        if (arguments.Length == 0)
            return Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "GitGui", "Foundation");
        if (arguments.Length == 2 && arguments[0] == "--data-directory" && Path.IsPathFullyQualified(arguments[1]))
            return Path.TrimEndingDirectorySeparator(Path.GetFullPath(arguments[1]));
        throw new ArgumentException("InvalidStartupArguments");
    }

    private void OnDispatcherException(object sender, DispatcherUnhandledExceptionEventArgs e)
    {
        e.Handled = true;
        FailAndExit(e.Exception);
    }

    private void OnDomainException(object sender, UnhandledExceptionEventArgs e)
    {
        // The runtime will terminate. Do not attempt UI recovery or serialize possibly invalid state.
        _log?.Record(DiagnosticEvent.FatalError, e.ExceptionObject as Exception);
    }

    private void FailAndExit(Exception exception)
    {
        if (_fatal) return;
        _fatal = true;
        _log?.Record(DiagnosticEvent.FatalError, exception);
        MessageBox.Show(UiText.Get("FatalErrorMessage"), UiText.Get("AppTitle"),
            MessageBoxButton.OK, MessageBoxImage.Error);
        _allowClose = true;
        Shutdown(1);
    }

    protected override void OnExit(ExitEventArgs e)
    {
        _startupCancellation.Cancel();
        _startupCancellation.Dispose();
        if (_ownsProfile) _profileMutex!.ReleaseMutex();
        _profileMutex?.Dispose();
        base.OnExit(e);
    }
}
